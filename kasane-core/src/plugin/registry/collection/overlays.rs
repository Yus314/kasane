//! Overlay collection and menu/info overlay resolution (OVERLAY plugins).

use crate::plugin::algebra::compose::{Composable, OverlaySet};
use crate::plugin::{AppView, OverlayContext, OverlayContribution, PluginCapabilities};

use super::super::PluginView;
use super::overlay_anchor_rect;

impl<'a> PluginView<'a> {
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
            let result_opt = slot
                .backend
                .contribute_overlay_with_ctx(state, &running_ctx);
            if let Some(mut oc) = result_opt {
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
    pub fn resolve_menu_overlay(&self, state: &AppView<'_>) -> Option<crate::element::Overlay> {
        for slot in self.slots {
            if !slot
                .capabilities
                .contains(PluginCapabilities::MENU_RENDERER)
            {
                continue;
            }
            let overlay = slot.backend.render_menu_overlay(state, self);
            if let Some(overlay) = overlay {
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
            let overlays = slot.backend.render_info_overlays(state, avoid, self);
            if let Some(overlays) = overlays {
                return Some(overlays);
            }
        }
        None
    }
}
