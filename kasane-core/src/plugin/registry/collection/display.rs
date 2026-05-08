//! Display directive / display map / scroll offset collection (DISPLAY_TRANSFORM).

use std::sync::Arc;

use crate::display::{DirectiveSet, DisplayMap, DisplayMapRef};
use crate::plugin::app_view::FrameworkAccess;
use crate::plugin::{AppView, PluginCapabilities};

use super::super::PluginView;

impl<'a> PluginView<'a> {
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
        let mut directives = crate::display_algebra::bridge::resolve_via_algebra(&set, line_count);
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
    ///
    /// Includes contributions from enabled lenses on
    /// `state.lens_registry` — a buffer with no display-transform
    /// plugin but at least one enabled lens still produces
    /// directives.
    pub fn collect_display_directives(
        &self,
        state: &AppView<'_>,
    ) -> Vec<crate::display::DisplayDirective> {
        let has_display_plugin = self.has_capability(PluginCapabilities::DISPLAY_TRANSFORM);
        let has_enabled_lens = state.as_app_state().lens_registry.enabled_count() > 0;
        if !has_display_plugin && !has_enabled_lens {
            return Vec::new();
        }

        let set = self.collect_tagged_display_directives(state);
        if set.is_empty() {
            return Vec::new();
        }
        let line_count = state.visible_line_range().len();
        crate::display_algebra::bridge::resolve_via_algebra(&set, line_count)
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

        // Composable Lenses (Roadmap §Backlog): enabled lenses
        // contribute their directives onto the same set as plugin
        // display handlers, so the algebra resolves both streams
        // uniformly. The registry's `collect_directives` returns
        // `(directive, priority, owner_plugin_id)` triples in
        // `LensId` sort order, matching the
        // `DirectiveSet::push` shape.
        for (directive, priority, plugin_id) in
            state.as_app_state().lens_registry.collect_directives(state)
        {
            set.push(directive, priority, plugin_id);
        }

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
}
