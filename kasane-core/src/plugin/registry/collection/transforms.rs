//! Transform chain dispatch (TRANSFORMER plugins) and menu-item transforms.

use crate::plugin::algebra::element_patch::ElementPatch;
use crate::plugin::{
    AppView, PaneContext, PluginCapabilities, PluginId, TransformContext, TransformSubject,
    TransformTarget,
};

use super::super::PluginView;
#[cfg(debug_assertions)]
use super::detect_transform_conflicts_from_patches;

impl<'a> PluginView<'a> {
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
            let slot = &self.slots[*i];
            let patch_opt = slot.backend.transform_patch(&target, state, &ctx);
            match patch_opt {
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
                let slot = &self.slots[*i];
                let patch = slot.backend.transform_patch(&target, state, &ctx);
                (*i, slot.backend.id(), patch)
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
                    let slot = &self.slots[slot_idx];
                    result = slot.backend.transform(&target, result, state, &ctx);
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
            let transformed = slot
                .backend
                .transform_menu_item(input, index, selected, state);
            if let Some(transformed) = transformed {
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
    pub fn has_transform_for(&self, _target: TransformTarget) -> bool {
        self.slots
            .iter()
            .any(|s| s.capabilities.contains(PluginCapabilities::TRANSFORMER))
    }
}
