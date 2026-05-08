//! Per-line annotation collection (ANNOTATOR plugins) and content annotations.

use crate::element::{Element, FlexChild};
use crate::plugin::compose::{Composable, ContentAnnotationSet};
use crate::plugin::{
    AnnotateContext, AnnotationResult, AppView, BackgroundLayer, GutterSide, PluginCapabilities,
    PluginId,
};

use super::super::{PluginSlot, PluginView};
use super::display_gutter_to_plugin;

impl<'a> PluginView<'a> {
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
        let mut backgrounds: Vec<Option<crate::protocol::WireFace>> = vec![None; line_count];
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
                                style: crate::protocol::Style::from_face(face),
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
                        height_lines,
                        box_id,
                        alignment,
                    } if *line < line_count => {
                        // Phase 10 Step 2-renderer B (commit forthcoming):
                        // emit InlineOp::InlineBox so GUI consumers can
                        // extract slot metadata via
                        // `InlineDecoration::inline_box_slots()` and route
                        // it through Parley's `push_inline_box` plus the
                        // host's `paint_inline_box(box_id)` callback. The
                        // atom-level pipeline (`apply_inline_ops`) still
                        // emits `width_cells` placeholder spaces so the
                        // cell-grid (TUI) backend and any GUI path that
                        // does not yet consume slot metadata keep correct
                        // display-column accounting.
                        uni_inline.entry(*line).or_default().push(
                            crate::render::InlineOp::InlineBox {
                                at: *byte_offset,
                                width_cells: *width_cells,
                                height_lines: *height_lines,
                                box_id: *box_id,
                                alignment: *alignment,
                                owner: pid.clone(),
                            },
                        );
                        has_inline = true;
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
                            .decorate_gutter(GutterSide::Left, line, state, ctx)
                    {
                        left_parts.push((prio, pid.clone(), el));
                        has_left = true;
                    }
                    if let Some((prio, el)) =
                        slot.backend
                            .decorate_gutter(GutterSide::Right, line, state, ctx)
                    {
                        right_parts.push((prio, pid.clone(), el));
                        has_right = true;
                    }
                    if let Some(bg) = slot.backend.decorate_background(line, state, ctx) {
                        bg_layers.push((bg, pid.clone()));
                    }
                    if let Some(inline) = slot.backend.decorate_inline(line, state, ctx) {
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
                0 => Element::text(" ", crate::protocol::Style::default()),
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
                0 => Element::text(" ", crate::protocol::Style::default()),
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
                backgrounds[line] = Some(bg_layers.last().unwrap().0.style.to_face());
                has_bg = true;
            }

            if !vt_parts.is_empty() {
                has_virtual_text = true;
                vt_parts.sort_by_key(|(prio, id, _)| (*prio, id.clone()));
                let separator = crate::protocol::Atom::with_style(
                    "  ",
                    crate::protocol::Style::from_face(&crate::protocol::WireFace {
                        attributes: crate::protocol::Attributes::DIM,
                        ..crate::protocol::WireFace::default()
                    }),
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
}
