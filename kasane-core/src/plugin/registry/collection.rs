//! View collection methods for [`PluginView`].
//!
//! Contains all read-only collection logic: contributions, transforms,
//! annotations, display maps, overlays, ornaments, and menu/info resolution.

use std::sync::Arc;

use crate::display::{DirectiveSet, DisplayMap, DisplayMapRef};
use crate::element::{Element, FlexChild};
use crate::plugin::compose::{Composable, ContentAnnotationSet, ContributionSet, OverlaySet};
use crate::plugin::element_patch::ElementPatch;
use crate::plugin::{
    AnnotateContext, AnnotationResult, AppView, BackgroundLayer, ContributeContext, Contribution,
    GutterSide, OverlayContext, OverlayContribution, PaneContext, PluginCapabilities, PluginId,
    RenderOrnamentContext, SlotId, SourcedContribution, TransformContext, TransformSubject,
    TransformTarget,
};

use super::{CollectedOrnaments, ContributionCache, PluginSlot, PluginView};

/// Convert `display::GutterSide` to `plugin::GutterSide`.
fn display_gutter_to_plugin(side: crate::display::GutterSide) -> GutterSide {
    match side {
        crate::display::GutterSide::Left => GutterSide::Left,
        crate::display::GutterSide::Right => GutterSide::Right,
    }
}

/// Extract a `Rect` from an `OverlayAnchor` when deterministic without layout.
fn overlay_anchor_rect(anchor: &crate::element::OverlayAnchor) -> Option<crate::layout::Rect> {
    match anchor {
        crate::element::OverlayAnchor::Absolute { x, y, w, h } => Some(crate::layout::Rect {
            x: *x,
            y: *y,
            w: *w,
            h: *h,
        }),
        // Fill and AnchorPoint need layout to determine the final rect.
        _ => None,
    }
}

/// Debug-only: detect potential transform conflicts from collected patches.
///
/// For native (patch-aware) plugins, scope is derived from `ElementPatch::scope()`.
/// For legacy plugins, scope is derived from `transform_descriptor()`.
///
/// Warns when:
/// - Multiple plugins declare `Replacement` scope for the same target
/// - Non-Identity transforms appear before a Replacement (they'll be absorbed)
#[cfg(debug_assertions)]
fn detect_transform_conflicts_from_patches(
    entries: &[(usize, PluginId, Option<ElementPatch>)],
    slots: &[PluginSlot],
    target: &TransformTarget,
) {
    use crate::plugin::context::TransformScope;

    let mut replacement_count = 0;
    let mut replacement_plugin: Option<&PluginId> = None;
    let mut has_non_identity_before_replacement = false;
    let mut seen_non_identity = false;

    for (slot_idx, plugin_id, patch) in entries {
        let scope = if let Some(p) = patch {
            // Native plugin: derive scope from patch
            p.scope()
        } else {
            // Legacy plugin: use declared descriptor
            if let Some(desc) = slots[*slot_idx].backend.transform_descriptor() {
                if !desc.targets.contains(target) {
                    continue;
                }
                desc.scope
            } else {
                continue;
            }
        };

        match scope {
            TransformScope::Replacement => {
                replacement_count += 1;
                if seen_non_identity {
                    has_non_identity_before_replacement = true;
                }
                replacement_plugin = Some(plugin_id);
            }
            TransformScope::Identity => {}
            _ => {
                seen_non_identity = true;
            }
        }
    }

    if replacement_count > 1 {
        tracing::warn!(
            target: "kasane::plugin::transform",
            "Multiple plugins declare Replacement scope for {:?} — \
             only the last in the chain will take effect",
            target,
        );
    }
    if has_non_identity_before_replacement && let Some(pid) = replacement_plugin {
        tracing::warn!(
            target: "kasane::plugin::transform",
            "Non-identity transforms appear before Replacement by {:?} for {:?} — \
             those transforms will be absorbed",
            pid,
            target,
        );
    }
}

/// Check for transform conflicts given a list of (plugin_id, descriptor) pairs.
///
/// Extracted as a free function for unit-testability.
#[cfg(debug_assertions)]
#[allow(dead_code)] // used by tests in tests/compose.rs
pub(crate) fn check_transform_conflicts(
    descriptors: &[(PluginId, Option<crate::plugin::TransformDescriptor>)],
    target: &TransformTarget,
) {
    use crate::plugin::context::TransformScope;

    let mut replacement_count = 0;
    let mut replacement_plugin: Option<&PluginId> = None;
    let mut has_non_identity_before_replacement = false;
    let mut seen_non_identity = false;

    for (plugin_id, desc) in descriptors {
        let Some(desc) = desc else {
            continue;
        };
        // Only consider descriptors that mention this target
        if !desc.targets.contains(target) {
            continue;
        }
        match desc.scope {
            TransformScope::Replacement => {
                replacement_count += 1;
                if seen_non_identity {
                    has_non_identity_before_replacement = true;
                }
                replacement_plugin = Some(plugin_id);
            }
            TransformScope::Identity => {}
            _ => {
                seen_non_identity = true;
            }
        }
    }

    if replacement_count > 1 {
        tracing::warn!(
            target: "kasane::plugin::transform",
            "Multiple plugins declare Replacement scope for {:?} — \
             only the last in the chain will take effect",
            target,
        );
    }
    if has_non_identity_before_replacement && let Some(pid) = replacement_plugin {
        tracing::warn!(
            target: "kasane::plugin::transform",
            "Non-identity transforms appear before Replacement by {:?} for {:?} — \
             those transforms will be absorbed",
            pid,
            target,
        );
    }
}

impl<'a> PluginView<'a> {
    /// Collect contributions from all plugins for a given region, sorted by priority.
    pub fn collect_contributions(
        &self,
        region: &SlotId,
        state: &AppView<'_>,
        ctx: &ContributeContext,
    ) -> Vec<Contribution> {
        self.collect_contributions_with_sources(region, state, ctx)
            .into_iter()
            .map(|sc| sc.contribution)
            .collect()
    }

    pub fn collect_contributions_with_sources(
        &self,
        region: &SlotId,
        state: &AppView<'_>,
        ctx: &ContributeContext,
    ) -> Vec<SourcedContribution> {
        self.slots
            .iter()
            .filter_map(|slot| {
                if !slot.capabilities.contains(PluginCapabilities::CONTRIBUTOR) {
                    return None;
                }
                let result = slot.backend.contribute_to(region, state, ctx);
                result.map(|contribution| SourcedContribution {
                    contributor: slot.backend.id(),
                    contribution,
                })
            })
            .fold(ContributionSet::empty(), |acc, sc| {
                acc.compose(ContributionSet::from_vec(vec![sc]))
            })
            .into_vec()
    }

    /// Collect contributions with per-plugin caching.
    ///
    /// Only calls `contribute_to()` for plugins whose `needs_recollect` is true.
    /// For non-stale plugins, the cached result from the previous frame is reused.
    pub fn collect_contributions_cached(
        &self,
        region: &SlotId,
        state: &AppView<'_>,
        ctx: &ContributeContext,
        cache: &mut ContributionCache,
    ) -> Vec<Contribution> {
        self.collect_contributions_with_sources_cached(region, state, ctx, cache)
            .into_iter()
            .map(|sc| sc.contribution)
            .collect()
    }

    /// Collect contributions with per-plugin caching (with source tracking).
    pub fn collect_contributions_with_sources_cached(
        &self,
        region: &SlotId,
        state: &AppView<'_>,
        ctx: &ContributeContext,
        cache: &mut ContributionCache,
    ) -> Vec<SourcedContribution> {
        self.slots
            .iter()
            .filter_map(|slot| {
                if !slot.capabilities.contains(PluginCapabilities::CONTRIBUTOR) {
                    return None;
                }

                let plugin_id = slot.backend.id();
                let cache_key = (plugin_id.clone(), region.clone());

                if slot.needs_recollect {
                    let result =
                        slot.backend
                            .contribute_to(region, state, ctx)
                            .map(|contribution| SourcedContribution {
                                contributor: plugin_id,
                                contribution,
                            });
                    cache.contributions.insert(cache_key, result.clone());
                    result
                } else {
                    cache.contributions.get(&cache_key).cloned().flatten()
                }
            })
            .fold(ContributionSet::empty(), |acc, sc| {
                acc.compose(ContributionSet::from_vec(vec![sc]))
            })
            .into_vec()
    }

    /// Collect transform patches from all TRANSFORMER plugins for a target,
    /// without applying them.
    ///
    /// Returns a composed `Some(patch)` when all plugins return pure patches,
    /// or `None` when any plugin returns a legacy (imperative) or impure patch.
    /// Used by `sync_transform_patches()` to store patches as Salsa inputs.
    pub fn collect_transform_patches(
        &self,
        target: TransformTarget,
        state: &AppView<'_>,
    ) -> Option<ElementPatch> {
        let mut chain: Vec<(usize, i16, PluginId)> = Vec::new();
        for (i, slot) in self.slots.iter().enumerate() {
            if slot.capabilities.contains(PluginCapabilities::TRANSFORMER) {
                let prio = slot.backend.transform_priority();
                chain.push((i, prio, slot.backend.id()));
            }
        }
        chain.sort_by_key(|(_, prio, id)| (std::cmp::Reverse(*prio), id.clone()));

        if chain.is_empty() {
            return Some(ElementPatch::Identity);
        }

        let pane_context = PaneContext::default();
        let mut patches = Vec::new();
        for (pos, (i, _, _)) in chain.iter().enumerate() {
            let ctx = TransformContext {
                is_default: true,
                chain_position: pos,
                pane_surface_id: pane_context.surface_id,
                pane_focused: pane_context.focused,
                target_line: target.as_buffer_line(),
            };
            match self.slots[*i].backend.transform_patch(&target, state, &ctx) {
                Some(p) if p.is_pure() => patches.push(p),
                Some(_) | None => return None, // impure or legacy → fall back to imperative
            }
        }

        Some(ElementPatch::Compose(patches).normalize())
    }

    /// Apply the transform chain for a given target.
    ///
    /// Plugins with the `TRANSFORMER` capability are collected into a chain,
    /// sorted by priority in **descending** order (high priority = inner =
    /// applied first). The `subject` is the seed, then each transformer is
    /// applied in order.
    pub fn apply_transform_chain(
        &self,
        target: TransformTarget,
        subject: TransformSubject,
        state: &AppView<'_>,
    ) -> TransformSubject {
        self.apply_transform_chain_in_pane(target, subject, state, PaneContext::default())
    }

    pub fn apply_transform_chain_in_pane(
        &self,
        target: TransformTarget,
        subject: TransformSubject,
        state: &AppView<'_>,
        pane_context: PaneContext,
    ) -> TransformSubject {
        let mut chain: Vec<(usize, i16, PluginId)> = Vec::new();
        for (i, slot) in self.slots.iter().enumerate() {
            if slot.capabilities.contains(PluginCapabilities::TRANSFORMER) {
                let prio = slot.backend.transform_priority();
                chain.push((i, prio, slot.backend.id()));
            }
        }
        chain.sort_by_key(|(_, prio, id)| (std::cmp::Reverse(*prio), id.clone()));

        if chain.is_empty() {
            return subject;
        }

        // Collect patches from patch-aware plugins; None = legacy (imperative)
        let entries: Vec<(usize, PluginId, Option<ElementPatch>)> = chain
            .iter()
            .enumerate()
            .map(|(pos, (i, _, _))| {
                let ctx = TransformContext {
                    is_default: true,
                    chain_position: pos,
                    pane_surface_id: pane_context.surface_id,
                    pane_focused: pane_context.focused,
                    target_line: target.as_buffer_line(),
                };
                let patch = self.slots[*i].backend.transform_patch(&target, state, &ctx);
                (*i, self.slots[*i].backend.id(), patch)
            })
            .collect();

        #[cfg(debug_assertions)]
        detect_transform_conflicts_from_patches(&entries, self.slots, &target);

        // Apply: accumulate patches algebraically, flush at legacy boundaries
        let mut result = subject;
        let mut pending: Vec<ElementPatch> = Vec::new();

        for (pos, (slot_idx, _, patch)) in entries.into_iter().enumerate() {
            match patch {
                Some(p) => pending.push(p),
                None => {
                    // Flush accumulated patches before legacy transform
                    if !pending.is_empty() {
                        let composed =
                            ElementPatch::Compose(std::mem::take(&mut pending)).normalize();
                        let ctx = TransformContext {
                            is_default: true,
                            chain_position: pos,
                            pane_surface_id: pane_context.surface_id,
                            pane_focused: pane_context.focused,
                            target_line: target.as_buffer_line(),
                        };
                        result = composed.apply_with_context(result, &ctx);
                    }
                    let ctx = TransformContext {
                        is_default: true,
                        chain_position: pos,
                        pane_surface_id: pane_context.surface_id,
                        pane_focused: pane_context.focused,
                        target_line: target.as_buffer_line(),
                    };
                    result = self.slots[slot_idx]
                        .backend
                        .transform(&target, result, state, &ctx);
                }
            }
        }

        // Final flush of remaining patches
        if !pending.is_empty() {
            let ctx = TransformContext {
                is_default: true,
                chain_position: 0,
                pane_surface_id: pane_context.surface_id,
                pane_focused: pane_context.focused,
                target_line: target.as_buffer_line(),
            };
            let composed = ElementPatch::Compose(pending).normalize();
            result = composed.apply_with_context(result, &ctx);
        }

        result
    }

    /// Apply the hierarchical transform chain for a target with refinement.
    ///
    /// For style-specific targets (e.g. `MenuPrompt`), applies the generic parent
    /// target first, then the specific target. For non-refinement targets, this is
    /// equivalent to `apply_transform_chain`.
    pub fn apply_transform_chain_hierarchical(
        &self,
        target: TransformTarget,
        subject: TransformSubject,
        state: &AppView<'_>,
    ) -> TransformSubject {
        self.apply_transform_chain_hierarchical_in_pane(
            target,
            subject,
            state,
            PaneContext::default(),
        )
    }

    pub fn apply_transform_chain_hierarchical_in_pane(
        &self,
        target: TransformTarget,
        subject: TransformSubject,
        state: &AppView<'_>,
        pane_context: PaneContext,
    ) -> TransformSubject {
        let chain = target.refinement_chain();
        let mut result = subject;
        for step_target in chain {
            result = self.apply_transform_chain_in_pane(step_target, result, state, pane_context);
        }
        result
    }

    /// Collect annotations from all annotating plugins for visible lines.
    ///
    /// For unified display plugins, Decoration and Inline category directives
    /// are pre-indexed by line from the unified cache, avoiding per-line plugin
    /// calls. Legacy plugins are called per-line as before.
    pub fn collect_annotations(
        &self,
        state: &AppView<'_>,
        ctx: &AnnotateContext,
    ) -> AnnotationResult {
        if !self.has_capability(PluginCapabilities::ANNOTATOR) {
            return AnnotationResult {
                left_gutter: None,
                right_gutter: None,
                line_backgrounds: None,
                inline_decorations: None,
                virtual_text: None,
            };
        }

        let line_count = state.visible_line_range().len();
        let mut has_left = false;
        let mut has_right = false;
        let mut has_bg = false;
        let mut has_inline = false;
        let mut has_virtual_text = false;

        let mut left_rows: Vec<FlexChild> = Vec::with_capacity(line_count);
        let mut right_rows: Vec<FlexChild> = Vec::with_capacity(line_count);
        let mut backgrounds: Vec<Option<crate::protocol::Face>> = vec![None; line_count];
        let mut inline_decorations: Vec<Option<crate::render::InlineDecoration>> =
            vec![None; line_count];
        let mut virtual_texts: Vec<Option<Vec<crate::protocol::Atom>>> = vec![None; line_count];

        // Phase 1: Pre-index unified plugin decoration/inline by line.
        // This converts O(lines × plugins) plugin calls into O(total_directives).
        let mut uni_left: std::collections::HashMap<usize, Vec<(i16, PluginId, Element)>> =
            std::collections::HashMap::new();
        let mut uni_right: std::collections::HashMap<usize, Vec<(i16, PluginId, Element)>> =
            std::collections::HashMap::new();
        let mut uni_bg: std::collections::HashMap<usize, Vec<(BackgroundLayer, PluginId)>> =
            std::collections::HashMap::new();
        let mut uni_vt: std::collections::HashMap<
            usize,
            Vec<(i16, PluginId, Vec<crate::protocol::Atom>)>,
        > = std::collections::HashMap::new();
        let mut uni_inline: std::collections::HashMap<usize, Vec<crate::render::InlineOp>> =
            std::collections::HashMap::new();

        for (idx, slot) in self.slots.iter().enumerate() {
            if !slot.capabilities.contains(PluginCapabilities::ANNOTATOR) {
                continue;
            }
            if !self.ensure_unified_cached(idx, state) {
                continue;
            }

            let cache = self.unified_cache.borrow();
            let cat = cache[idx].as_ref().unwrap();
            let pid = slot.backend.id();

            for td in &cat.decoration {
                match &td.directive {
                    crate::display::DisplayDirective::StyleLine {
                        line,
                        face,
                        z_order,
                    } if *line < line_count => {
                        uni_bg.entry(*line).or_default().push((
                            BackgroundLayer {
                                face: *face,
                                z_order: *z_order,
                                blend: crate::plugin::context::BlendMode::Opaque,
                            },
                            pid.clone(),
                        ));
                        has_bg = true;
                    }
                    crate::display::DisplayDirective::Gutter {
                        line,
                        side,
                        content,
                        priority,
                    } if *line < line_count => {
                        let parts = match display_gutter_to_plugin(*side) {
                            GutterSide::Left => {
                                has_left = true;
                                uni_left.entry(*line).or_default()
                            }
                            GutterSide::Right => {
                                has_right = true;
                                uni_right.entry(*line).or_default()
                            }
                        };
                        parts.push((*priority, pid.clone(), content.clone()));
                    }
                    crate::display::DisplayDirective::VirtualText {
                        line,
                        content,
                        priority,
                        ..
                    } if *line < line_count => {
                        if !content.is_empty() {
                            uni_vt.entry(*line).or_default().push((
                                *priority,
                                pid.clone(),
                                content.clone(),
                            ));
                            has_virtual_text = true;
                        }
                    }
                    _ => {}
                }
            }

            for td in &cat.inline {
                match &td.directive {
                    crate::display::DisplayDirective::InsertInline {
                        line,
                        byte_offset,
                        content,
                        ..
                    } if *line < line_count => {
                        uni_inline.entry(*line).or_default().push(
                            crate::render::InlineOp::Insert {
                                at: *byte_offset,
                                content: content.clone(),
                            },
                        );
                        has_inline = true;
                    }
                    crate::display::DisplayDirective::HideInline { line, byte_range }
                        if *line < line_count =>
                    {
                        uni_inline
                            .entry(*line)
                            .or_default()
                            .push(crate::render::InlineOp::Hide {
                                range: byte_range.clone(),
                            });
                        has_inline = true;
                    }
                    crate::display::DisplayDirective::InlineBox {
                        line,
                        byte_offset,
                        width_cells,
                        ..
                    } if *line < line_count => {
                        // Phase 10 Step 1 — placeholder projection. The WIT
                        // contract reserves a non-text inline slot, but the
                        // host paint extension (`paint-inline-box(box-id)`)
                        // is not yet wired (Step 2). Project to a
                        // `width_cells`-space `InsertInline` so adjacent
                        // atoms keep correct display-column accounting and
                        // the slot is observable to plugin authors.
                        let n = width_cells.max(0.0).round() as usize;
                        if n > 0 {
                            uni_inline.entry(*line).or_default().push(
                                crate::render::InlineOp::Insert {
                                    at: *byte_offset,
                                    content: vec![crate::protocol::Atom::plain(" ".repeat(n))],
                                },
                            );
                            has_inline = true;
                        }
                    }
                    crate::display::DisplayDirective::StyleInline {
                        line,
                        byte_range,
                        face,
                    } if *line < line_count => {
                        uni_inline
                            .entry(*line)
                            .or_default()
                            .push(crate::render::InlineOp::Style {
                                range: byte_range.clone(),
                                face: *face,
                            });
                        has_inline = true;
                    }
                    _ => {}
                }
            }
        }

        // Phase 2: Partition legacy annotators (non-unified).
        let legacy_annotator_slots: Vec<(usize, &PluginSlot)> = self
            .slots
            .iter()
            .enumerate()
            .filter(|(idx, s)| {
                s.capabilities.contains(PluginCapabilities::ANNOTATOR)
                    && self.unified_cache.borrow()[*idx].is_none()
            })
            .collect();

        for line in 0..line_count {
            // Start with unified entries for this line
            let mut left_parts = uni_left.remove(&line).unwrap_or_default();
            let mut right_parts = uni_right.remove(&line).unwrap_or_default();
            let mut bg_layers = uni_bg.remove(&line).unwrap_or_default();
            let mut vt_parts = uni_vt.remove(&line).unwrap_or_default();

            // Build inline decoration from unified ops
            if let Some(mut ops) = uni_inline.remove(&line) {
                ops.sort_by_key(|op| op.sort_key());
                if inline_decorations[line].is_some() {
                    tracing::warn!(
                        line,
                        "multiple plugins provide inline decoration for same line; first wins"
                    );
                } else {
                    inline_decorations[line] = Some(crate::render::InlineDecoration::new(ops));
                    has_inline = true;
                }
            }

            for (_slot_idx, slot) in &legacy_annotator_slots {
                let pid = slot.backend.id();

                if slot.backend.has_decomposed_annotations() {
                    // Native (HandlerTable) path: call per-concern methods directly
                    if let Some((prio, el)) =
                        slot.backend
                            .annotate_gutter(GutterSide::Left, line, state, ctx)
                    {
                        left_parts.push((prio, pid.clone(), el));
                        has_left = true;
                    }
                    if let Some((prio, el)) =
                        slot.backend
                            .annotate_gutter(GutterSide::Right, line, state, ctx)
                    {
                        right_parts.push((prio, pid.clone(), el));
                        has_right = true;
                    }
                    if let Some(bg) = slot.backend.annotate_background(line, state, ctx) {
                        bg_layers.push((bg, pid.clone()));
                    }
                    if let Some(inline) = slot.backend.annotate_inline(line, state, ctx) {
                        if inline_decorations[line].is_some() {
                            tracing::warn!(
                                line,
                                "multiple plugins provide inline decoration for same line; first wins"
                            );
                        } else {
                            inline_decorations[line] = Some(inline);
                            has_inline = true;
                        }
                    }
                    for vt in slot.backend.annotate_virtual_text(line, state, ctx) {
                        if !vt.atoms.is_empty() {
                            vt_parts.push((vt.priority, pid.clone(), vt.atoms));
                        }
                    }
                } else {
                    // Legacy (WASM) path: call monolithic method and decompose
                    if let Some(ann) = slot.backend.annotate_line_with_ctx(line, state, ctx) {
                        let prio = ann.priority;
                        if let Some(el) = ann.left_gutter {
                            left_parts.push((prio, pid.clone(), el));
                            has_left = true;
                        }
                        if let Some(el) = ann.right_gutter {
                            right_parts.push((prio, pid.clone(), el));
                            has_right = true;
                        }
                        if let Some(bg) = ann.background {
                            bg_layers.push((bg, pid.clone()));
                        }
                        if let Some(inline) = ann.inline {
                            if inline_decorations[line].is_some() {
                                tracing::warn!(
                                    line,
                                    "multiple plugins provide inline decoration for same line; first wins"
                                );
                            } else {
                                inline_decorations[line] = Some(inline);
                                has_inline = true;
                            }
                        }
                        for vt in ann.virtual_text {
                            if !vt.atoms.is_empty() {
                                vt_parts.push((vt.priority, pid.clone(), vt.atoms));
                            }
                        }
                    }
                }
            }

            left_parts.sort_by_key(|(prio, id, _)| (*prio, id.clone()));
            right_parts.sort_by_key(|(prio, id, _)| (*prio, id.clone()));

            let left_cell = match left_parts.len() {
                0 => Element::text(" ", crate::protocol::Face::default()),
                1 => left_parts.pop().unwrap().2,
                _ => Element::row(
                    left_parts
                        .into_iter()
                        .map(|(_, _, el)| FlexChild::fixed(el))
                        .collect(),
                ),
            };
            left_rows.push(FlexChild::fixed(left_cell));

            let right_cell = match right_parts.len() {
                0 => Element::text(" ", crate::protocol::Face::default()),
                1 => right_parts.pop().unwrap().2,
                _ => Element::row(
                    right_parts
                        .into_iter()
                        .map(|(_, _, el)| FlexChild::fixed(el))
                        .collect(),
                ),
            };
            right_rows.push(FlexChild::fixed(right_cell));

            if !bg_layers.is_empty() {
                bg_layers.sort_by_key(|(l, id)| (l.z_order, id.clone()));
                backgrounds[line] = Some(bg_layers.last().unwrap().0.face);
                has_bg = true;
            }

            if !vt_parts.is_empty() {
                has_virtual_text = true;
                vt_parts.sort_by_key(|(prio, id, _)| (*prio, id.clone()));
                let separator = crate::protocol::Atom::from_face(
                    crate::protocol::Face {
                        attributes: crate::protocol::Attributes::DIM,
                        ..crate::protocol::Face::default()
                    },
                    "  ",
                );
                let mut merged = Vec::new();
                for (i, (_, _, atoms)) in vt_parts.into_iter().enumerate() {
                    if i > 0 {
                        merged.push(separator.clone());
                    }
                    merged.extend(atoms);
                }
                virtual_texts[line] = Some(merged);
            }
        }

        AnnotationResult {
            left_gutter: if has_left {
                Some(Element::column(left_rows))
            } else {
                None
            },
            right_gutter: if has_right {
                Some(Element::column(right_rows))
            } else {
                None
            },
            line_backgrounds: if has_bg { Some(backgrounds) } else { None },
            inline_decorations: if has_inline {
                Some(inline_decorations)
            } else {
                None
            },
            virtual_text: if has_virtual_text {
                Some(virtual_texts)
            } else {
                None
            },
        }
    }

    /// Collect all projection descriptors from registered plugins.
    pub fn collect_projection_descriptors(&self) -> Vec<crate::display::ProjectionDescriptor> {
        let mut result = Vec::new();
        for slot in self.slots.iter() {
            if !slot
                .capabilities
                .contains(PluginCapabilities::DISPLAY_TRANSFORM)
            {
                continue;
            }
            result.extend_from_slice(slot.backend.projection_descriptors());
        }
        result
    }

    /// Collect display transformation directives from all plugins and build
    /// a `DisplayMapRef`.
    pub fn collect_display_map(&self, state: &AppView<'_>) -> DisplayMapRef {
        if !self.has_capability(PluginCapabilities::DISPLAY_TRANSFORM) {
            let line_count = state.visible_line_range().len();
            return Arc::new(DisplayMap::identity(line_count));
        }

        let line_count = state.visible_line_range().len();
        let set = self.collect_tagged_display_directives(state);
        if set.is_empty() {
            return Arc::new(DisplayMap::identity(line_count));
        }
        let mut directives = crate::display::resolve(&set, line_count);
        // Filter out fold ranges that have been toggled open by the user.
        // Per-projection fold state scoping: use the active structural projection's
        // fold state if one is active, otherwise fall back to the global fold state.
        if let Some(active_id) = state.projection_policy().active_structural() {
            state
                .projection_policy()
                .fold_state_for(active_id)
                .filter_directives(&mut directives);
        } else {
            state.fold_toggle_state().filter_directives(&mut directives);
        }
        // Cursor safety net: never hide the line the cursor is on.
        let cursor_line = state.cursor_line().max(0) as usize;
        directives.retain(|d| match d {
            crate::display::DisplayDirective::Hide { range } => !range.contains(&cursor_line),
            _ => true,
        });
        if directives.is_empty() {
            return Arc::new(DisplayMap::identity(line_count));
        }
        // Record directives for oscillation detection (P-032 §temporal).
        self.directive_stability.borrow_mut().record(&directives);
        let dm = DisplayMap::build(line_count, &directives);
        Arc::new(dm)
    }

    /// Collect raw display directives from all plugins (without building a DisplayMap).
    pub fn collect_display_directives(
        &self,
        state: &AppView<'_>,
    ) -> Vec<crate::display::DisplayDirective> {
        if !self.has_capability(PluginCapabilities::DISPLAY_TRANSFORM) {
            return Vec::new();
        }

        let set = self.collect_tagged_display_directives(state);
        if set.is_empty() {
            return Vec::new();
        }
        let line_count = state.visible_line_range().len();
        crate::display::resolve(&set, line_count)
    }

    /// Collect tagged display directives from all display-transform plugins.
    ///
    /// The resulting `DirectiveSet` forms a commutative monoid (see `compose::Composable`):
    /// plugin evaluation order does not affect the resolved output.
    ///
    /// Unified-aware: plugins with `has_unified_display()` contribute their
    /// spatial directives from the unified cache. Legacy plugins use
    /// `display_directives()` / `projection_directives()` as before.
    fn collect_tagged_display_directives(&self, state: &AppView<'_>) -> DirectiveSet {
        let mut set = DirectiveSet::default();
        let projection_policy = state.projection_policy();

        for (idx, slot) in self.slots.iter().enumerate() {
            if !slot
                .capabilities
                .contains(PluginCapabilities::DISPLAY_TRANSFORM)
            {
                continue;
            }

            // Unified path: pull spatial from cache
            if self.ensure_unified_cached(idx, state) {
                let cache = self.unified_cache.borrow();
                if let Some(cat) = &cache[idx] {
                    for td in &cat.spatial {
                        set.push(td.directive.clone(), td.priority, td.plugin_id.clone());
                    }
                }
                continue;
            }

            // Legacy path
            let has_projections = !slot.backend.projection_descriptors().is_empty();

            // Legacy display handlers: only if plugin does NOT define projections
            if !has_projections {
                let directives = slot.backend.display_directives(state);
                if directives.is_empty() {
                    continue;
                }
                let priority = slot.backend.display_directive_priority();
                let plugin_id = slot.backend.id();
                for d in directives {
                    set.push(d, priority, plugin_id.clone());
                }
            }

            // Projection handlers: only call active projections
            for desc in slot.backend.projection_descriptors() {
                if !projection_policy.is_active(&desc.id) {
                    continue;
                }
                let directives = slot.backend.projection_directives(&desc.id, state);
                if directives.is_empty() {
                    continue;
                }
                let plugin_id = slot.backend.id();
                for d in directives {
                    set.push(d, desc.priority, plugin_id.clone());
                }
            }
        }
        set
    }

    /// Collect content annotations from all plugins with CONTENT_ANNOTATOR capability.
    ///
    /// For unified display plugins, InterLine category directives (InsertBefore,
    /// InsertAfter) are converted from the unified cache. Legacy plugins use
    /// `content_annotations()` as before. Results are merged via monoidal
    /// composition.
    pub fn collect_content_annotations(
        &self,
        state: &AppView<'_>,
        ctx: &crate::plugin::AnnotateContext,
    ) -> Vec<crate::display::ContentAnnotation> {
        use crate::display::content_annotation::{ContentAnchor, ContentAnnotation};

        if !self.has_capability(PluginCapabilities::CONTENT_ANNOTATOR) {
            return Vec::new();
        }

        let mut result = ContentAnnotationSet::empty();

        for (idx, slot) in self.slots.iter().enumerate() {
            if !slot
                .capabilities
                .contains(PluginCapabilities::CONTENT_ANNOTATOR)
            {
                continue;
            }

            // Unified path: convert InterLine directives to ContentAnnotation
            if self.ensure_unified_cached(idx, state) {
                let cache = self.unified_cache.borrow();
                if let Some(cat) = &cache[idx] {
                    let pid = slot.backend.id();
                    let mut converted = Vec::new();
                    for td in &cat.interline {
                        match &td.directive {
                            crate::display::DisplayDirective::InsertBefore {
                                line,
                                content,
                                priority,
                            } => {
                                converted.push(ContentAnnotation {
                                    anchor: ContentAnchor::InsertBefore(*line),
                                    element: content.clone(),
                                    plugin_id: pid.clone(),
                                    priority: *priority,
                                });
                            }
                            crate::display::DisplayDirective::InsertAfter {
                                line,
                                content,
                                priority,
                            } => {
                                converted.push(ContentAnnotation {
                                    anchor: ContentAnchor::InsertAfter(*line),
                                    element: content.clone(),
                                    plugin_id: pid.clone(),
                                    priority: *priority,
                                });
                            }
                            _ => {}
                        }
                    }
                    if !converted.is_empty() {
                        result = result.compose(ContentAnnotationSet::from_vec(converted));
                    }
                }
                continue;
            }

            // Legacy path
            let annotations = slot.backend.content_annotations(state, ctx);
            if !annotations.is_empty() {
                result = result.compose(ContentAnnotationSet::from_vec(annotations));
            }
        }

        result.into_vec()
    }

    /// Collect overlay contributions with collision-avoidance context.
    ///
    /// Iterates plugins one at a time, accumulating previously-contributed
    /// overlay rects in `OverlayContext::existing_overlays` so each plugin
    /// can position itself to avoid prior overlays.
    pub fn collect_overlays_with_ctx(
        &self,
        state: &AppView<'_>,
        ctx: &OverlayContext,
    ) -> Vec<OverlayContribution> {
        let mut running_ctx = ctx.clone();
        let mut result = OverlaySet::empty();
        for slot in self.slots {
            if !(slot.capabilities.contains(PluginCapabilities::CONTRIBUTOR)
                || slot.capabilities.contains(PluginCapabilities::OVERLAY))
            {
                continue;
            }
            if let Some(mut oc) = slot
                .backend
                .contribute_overlay_with_ctx(state, &running_ctx)
            {
                oc.plugin_id = slot.backend.id();
                // Record this overlay's rect for subsequent plugins' avoidance.
                if let Some(rect) = overlay_anchor_rect(&oc.anchor) {
                    running_ctx.existing_overlays.push(rect);
                }
                result = result.compose(OverlaySet::from_vec(vec![oc]));
            }
        }
        result.into_vec()
    }

    /// Resolve the display scroll offset via plugin override (first-wins).
    ///
    /// Iterates plugins with `SCROLL_OFFSET` capability. The first plugin
    /// returning `Some` wins. Falls back to `default_offset`.
    pub fn resolve_display_scroll_offset(
        &self,
        cursor_display_y: usize,
        viewport_height: usize,
        default_offset: usize,
        state: &AppView<'_>,
    ) -> usize {
        for slot in self.slots {
            if !slot
                .capabilities
                .contains(PluginCapabilities::SCROLL_OFFSET)
            {
                continue;
            }
            if let Some(offset) = slot.backend.compute_display_scroll_offset(
                cursor_display_y,
                viewport_height,
                default_offset,
                state,
            ) {
                return offset;
            }
        }
        default_offset
    }

    /// Resolve a custom menu overlay via plugin renderer (first-wins).
    ///
    /// Iterates plugins with `MENU_RENDERER` capability. The first plugin
    /// returning `Some` wins. Returns `None` if no plugin provides a custom menu.
    pub fn resolve_menu_overlay(&self, state: &AppView<'_>) -> Option<crate::element::Overlay> {
        for slot in self.slots {
            if !slot
                .capabilities
                .contains(PluginCapabilities::MENU_RENDERER)
            {
                continue;
            }
            if let Some(overlay) = slot.backend.render_menu_overlay(state, self) {
                return Some(overlay);
            }
        }
        None
    }

    /// Resolve custom info overlays via plugin renderer (first-wins).
    ///
    /// Iterates plugins with `INFO_RENDERER` capability. The first plugin
    /// returning `Some` wins. Returns `None` if no plugin provides custom info.
    pub fn resolve_info_overlays(
        &self,
        state: &AppView<'_>,
        avoid: &[crate::layout::Rect],
    ) -> Option<Vec<crate::element::Overlay>> {
        for slot in self.slots {
            if !slot
                .capabilities
                .contains(PluginCapabilities::INFO_RENDERER)
            {
                continue;
            }
            if let Some(overlays) = slot.backend.render_info_overlays(state, avoid, self) {
                return Some(overlays);
            }
        }
        None
    }

    /// Transform a menu item through all plugins.
    pub fn transform_menu_item(
        &self,
        item: &[crate::protocol::Atom],
        index: usize,
        selected: bool,
        state: &AppView<'_>,
    ) -> Option<Vec<crate::protocol::Atom>> {
        let mut current: Option<Vec<crate::protocol::Atom>> = None;
        for slot in self.slots.iter() {
            if !slot
                .capabilities
                .contains(PluginCapabilities::MENU_TRANSFORM)
            {
                continue;
            }
            let input = current.as_deref().unwrap_or(item);
            if let Some(transformed) = slot
                .backend
                .transform_menu_item(input, index, selected, state)
            {
                current = Some(transformed);
            }
        }
        current
    }

    /// Collect all render ornaments in a single pass and decompose into
    /// emphasis, cursor style, cursor effects, and surfaces.
    ///
    /// This avoids redundant per-frame `render_ornaments()` calls (which are
    /// expensive for WASM plugins).
    pub fn collect_ornaments(
        &self,
        state: &AppView<'_>,
        ctx: &RenderOrnamentContext,
    ) -> CollectedOrnaments {
        let mut emphasis = Vec::new();
        let mut cursor_style: Option<(crate::plugin::CursorStyleOrn, usize)> = None;
        let mut cursor_position: Option<(crate::plugin::CursorPositionOrn, usize)> = None;
        let mut cursor_effects = Vec::new();
        let mut surfaces = Vec::new();

        for (idx, slot) in self.slots.iter().enumerate() {
            if !slot
                .capabilities
                .contains(PluginCapabilities::RENDER_ORNAMENT)
            {
                continue;
            }
            let batch = slot.backend.render_ornaments(state, ctx);
            if batch.is_empty() {
                continue;
            }

            emphasis.extend(batch.emphasis);

            if let Some(candidate) = batch.cursor_style {
                let replace = match &cursor_style {
                    None => true,
                    Some((current, _)) => {
                        let lhs = (candidate.modality.rank(), candidate.priority);
                        let rhs = (current.modality.rank(), current.priority);
                        lhs > rhs
                    }
                };
                if replace {
                    cursor_style = Some((candidate, idx));
                }
            }

            if let Some(candidate) = batch.cursor_position {
                let replace = match &cursor_position {
                    None => true,
                    Some((current, _)) => {
                        let lhs = (candidate.modality.rank(), candidate.priority);
                        let rhs = (current.modality.rank(), current.priority);
                        lhs > rhs
                    }
                };
                if replace {
                    cursor_position = Some((candidate, idx));
                }
            }

            cursor_effects.extend(batch.cursor_effects);
            surfaces.extend(batch.surfaces);
        }

        emphasis.sort_by_key(|d| d.priority);

        CollectedOrnaments {
            emphasis,
            cursor_style: cursor_style.map(|(orn, _)| orn.hint),
            cursor_position: cursor_position.map(|(orn, _)| (orn.x, orn.y, orn.style, orn.color)),
            cursor_effects,
            surfaces,
        }
    }

    /// Check if any plugin has TRANSFORMER capability for a given target.
    pub fn has_transform_for(&self, _target: TransformTarget) -> bool {
        self.slots
            .iter()
            .any(|s| s.capabilities.contains(PluginCapabilities::TRANSFORMER))
    }
}
