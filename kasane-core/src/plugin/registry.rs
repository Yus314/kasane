use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;

use std::sync::Arc;

use crate::display::{DisplayMap, DisplayMapRef};
use crate::element::{Element, FlexChild, InteractiveId};
use crate::layout::HitMap;
use crate::scroll::{DefaultScrollCandidate, ScrollPolicyResult};
use crate::state::{AppState, DirtyFlags};
use crate::workspace::Placement;

use super::bridge::PluginBridge;
use super::state::Plugin;
use super::{
    AnnotateContext, AnnotationResult, BackgroundLayer, ContributeContext, Contribution, InitBatch,
    IoEvent, OverlayContext, OverlayContribution, PaintHook, PluginBackend, PluginCapabilities,
    PluginId, ReadyBatch, RuntimeBatch, SlotId, SourcedContribution, TransformContext,
    TransformTarget,
};

/// Cached result for a single plugin's contributions.
#[derive(Default)]
struct PluginCacheEntry {
    last_state_hash: u64,
    /// Cached contribute_to() results, keyed by SlotId.
    contributions: HashMap<SlotId, Option<Contribution>>,
}

struct PluginSlotCache {
    entries: Vec<PluginCacheEntry>,
}

impl PluginSlotCache {
    fn new() -> Self {
        PluginSlotCache {
            entries: Vec::new(),
        }
    }
}

pub struct PluginSurfaceSet {
    pub owner: PluginId,
    pub surfaces: Vec<Box<dyn crate::surface::Surface>>,
    pub legacy_workspace_request: Option<Placement>,
}

pub struct PluginRegistry {
    plugins: Vec<Box<dyn PluginBackend>>,
    capabilities: Vec<PluginCapabilities>,
    hit_map: HitMap,
    slot_cache: RefCell<PluginSlotCache>,
    any_plugin_state_changed: bool,
}

impl PluginRegistry {
    pub fn new() -> Self {
        PluginRegistry {
            plugins: Vec::new(),
            capabilities: Vec::new(),
            hit_map: HitMap::new(),
            slot_cache: RefCell::new(PluginSlotCache::new()),
            any_plugin_state_changed: false,
        }
    }

    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }

    /// Check if any registered plugin has the given capability.
    fn has_capability(&self, cap: PluginCapabilities) -> bool {
        self.capabilities.iter().any(|c| c.contains(cap))
    }

    /// Returns true if any plugin's state_hash changed during the last
    /// `prepare_plugin_cache()` call.
    pub fn any_plugin_state_changed(&self) -> bool {
        self.any_plugin_state_changed
    }

    pub fn register_backend(&mut self, plugin: Box<dyn PluginBackend>) {
        let id = plugin.id();
        let caps = plugin.capabilities();
        if let Some(pos) = self.plugins.iter().position(|p| p.id() == id) {
            // Replace existing plugin with same ID (e.g. FS plugin overrides bundled)
            self.plugins[pos] = plugin;
            self.capabilities[pos] = caps;
            // Reset the cache entry for the replaced plugin
            self.slot_cache.get_mut().entries[pos] = PluginCacheEntry::default();
        } else {
            self.plugins.push(plugin);
            self.capabilities.push(caps);
            self.slot_cache
                .get_mut()
                .entries
                .push(PluginCacheEntry::default());
        }
    }

    pub fn contains_plugin(&self, id: &PluginId) -> bool {
        self.plugins.iter().any(|plugin| plugin.id() == *id)
    }

    /// Remove a plugin from the registry without running shutdown hooks.
    ///
    /// Prefer [`Self::unload_plugin`] for normal lifecycle transitions.
    pub fn remove_plugin(&mut self, id: &PluginId) -> bool {
        if let Some(pos) = self.plugins.iter().position(|plugin| plugin.id() == *id) {
            self.plugins.remove(pos);
            self.capabilities.remove(pos);
            self.slot_cache.get_mut().entries.remove(pos);
            true
        } else {
            false
        }
    }

    /// Shut down and remove a single plugin by ID.
    pub fn unload_plugin(&mut self, id: &PluginId) -> bool {
        if let Some(pos) = self.plugins.iter().position(|plugin| plugin.id() == *id) {
            self.plugins[pos].on_shutdown();
            self.plugins.remove(pos);
            self.capabilities.remove(pos);
            self.slot_cache.get_mut().entries.remove(pos);
            true
        } else {
            false
        }
    }

    /// Invalidate cache entries based on dirty flags and state hash changes.
    /// Call once per frame before rendering (during the mutable phase).
    pub fn prepare_plugin_cache(&mut self, _dirty: DirtyFlags) {
        let cache = self.slot_cache.get_mut();
        self.any_plugin_state_changed = false;

        // Grow entries if plugins were registered after last prepare
        while cache.entries.len() < self.plugins.len() {
            cache.entries.push(PluginCacheEntry::default());
        }

        for (i, plugin) in self.plugins.iter().enumerate() {
            let entry = &mut cache.entries[i];
            let current_hash = plugin.state_hash();

            // L1: state hash changed → invalidate all contributions for this plugin
            if current_hash != entry.last_state_hash {
                entry.last_state_hash = current_hash;
                entry.contributions.clear();
                self.any_plugin_state_changed = true;
            }
        }
    }

    /// Initialize all plugins and collect typed bootstrap effects.
    pub fn init_all_batch(&mut self, state: &AppState) -> InitBatch {
        let mut batch = InitBatch::default();
        for plugin in &mut self.plugins {
            batch.effects.merge(plugin.on_init_effects(state));
        }
        batch
    }

    /// Initialize all plugins.
    pub fn init_all(&mut self, state: &AppState) -> InitBatch {
        self.init_all_batch(state)
    }

    /// Notify all plugins that the active session is ready for transport-bound startup work.
    pub fn notify_active_session_ready_batch(&mut self, state: &AppState) -> ReadyBatch {
        let mut batch = ReadyBatch::default();
        for plugin in &mut self.plugins {
            batch
                .effects
                .merge(plugin.on_active_session_ready_effects(state));
        }
        batch
    }

    /// Notify all plugins that the active session is ready for transport-bound startup work.
    pub fn notify_active_session_ready(&mut self, state: &AppState) -> ReadyBatch {
        self.notify_active_session_ready_batch(state)
    }

    /// Notify a single plugin that the active session is ready.
    pub fn notify_plugin_active_session_ready_batch(
        &mut self,
        target: &PluginId,
        state: &AppState,
    ) -> ReadyBatch {
        for plugin in &mut self.plugins {
            if &plugin.id() == target {
                let mut batch = ReadyBatch::default();
                batch
                    .effects
                    .merge(plugin.on_active_session_ready_effects(state));
                return batch;
            }
        }
        ReadyBatch::default()
    }

    /// Notify all plugins about a state change and collect typed runtime effects.
    pub fn notify_state_changed_batch(
        &mut self,
        state: &AppState,
        dirty: DirtyFlags,
    ) -> RuntimeBatch {
        let mut batch = RuntimeBatch::default();
        for plugin in &mut self.plugins {
            batch
                .effects
                .merge(plugin.on_state_changed_effects(state, dirty));
        }
        batch
    }

    /// Shut down all plugins. Call before application exit.
    pub fn shutdown_all(&mut self) {
        for plugin in &mut self.plugins {
            plugin.on_shutdown();
        }
    }

    /// Reload a single plugin by replacing it in-place.
    ///
    /// Shuts down the old plugin (if it exists with the same ID), registers the
    /// new one, and initializes it, collecting typed bootstrap effects.
    pub fn reload_plugin_batch(
        &mut self,
        plugin: Box<dyn PluginBackend>,
        state: &AppState,
    ) -> InitBatch {
        let id = plugin.id();
        // Shut down old plugin if present
        if let Some(pos) = self.plugins.iter().position(|p| p.id() == id) {
            self.plugins[pos].on_shutdown();
        }
        // register_backend handles replacement or insertion
        self.register_backend(plugin);
        // Init the new plugin
        if let Some(pos) = self.plugins.iter().position(|p| p.id() == id) {
            let mut batch = InitBatch::default();
            batch
                .effects
                .merge(self.plugins[pos].on_init_effects(state));
            return batch;
        }
        InitBatch::default()
    }

    /// Reload a single plugin by replacing it in-place.
    pub fn reload_plugin(&mut self, plugin: Box<dyn PluginBackend>, state: &AppState) -> InitBatch {
        self.reload_plugin_batch(plugin, state)
    }

    pub fn plugins_mut(&mut self) -> impl Iterator<Item = &mut Box<dyn PluginBackend>> {
        self.plugins.iter_mut()
    }

    /// Collect plugin-owned surfaces during the bootstrap preflight stage.
    pub fn collect_plugin_surfaces(&mut self) -> Vec<PluginSurfaceSet> {
        let mut surfaces = Vec::new();
        for plugin in &mut self.plugins {
            let owner = plugin.id();
            let plugin_surfaces = plugin.surfaces();
            if !plugin_surfaces.is_empty() {
                surfaces.push(PluginSurfaceSet {
                    owner,
                    surfaces: plugin_surfaces,
                    legacy_workspace_request: plugin.workspace_request(),
                });
            }
        }
        surfaces
    }

    /// Collect paint hooks from all plugins. Call after `init_all_batch()`.
    pub fn collect_paint_hooks(&self) -> Vec<Box<dyn PaintHook>> {
        let mut hooks = Vec::new();
        for (i, plugin) in self.plugins.iter().enumerate() {
            if self.capabilities[i].contains(PluginCapabilities::PAINT_HOOK) {
                hooks.extend(plugin.paint_hooks());
            }
        }
        hooks
    }

    pub fn set_hit_map(&mut self, hit_map: HitMap) {
        self.hit_map = hit_map;
    }

    pub fn hit_test(&self, x: u16, y: u16) -> Option<InteractiveId> {
        self.hit_map.test(x, y)
    }

    /// Hit test returning both the InteractiveId and its bounding Rect.
    pub fn hit_test_with_rect(
        &self,
        x: u16,
        y: u16,
    ) -> Option<(InteractiveId, crate::layout::Rect)> {
        self.hit_map.test_with_rect(x, y)
    }

    // --- Menu item transformation ---

    /// Transform a menu item through all plugins. Returns None if no plugin transforms it.
    pub fn transform_menu_item(
        &self,
        item: &[crate::protocol::Atom],
        index: usize,
        selected: bool,
        state: &AppState,
    ) -> Option<Vec<crate::protocol::Atom>> {
        let mut current: Option<Vec<crate::protocol::Atom>> = None;
        for (i, plugin) in self.plugins.iter().enumerate() {
            if !self.capabilities[i].contains(PluginCapabilities::MENU_TRANSFORM) {
                continue;
            }
            let input = current.as_deref().unwrap_or(item);
            if let Some(transformed) = plugin.transform_menu_item(input, index, selected, state) {
                current = Some(transformed);
            }
        }
        current
    }

    // --- Cursor style override ---

    /// Query plugins for a cursor style override. Returns the first non-None.
    pub fn cursor_style_override(&self, state: &AppState) -> Option<crate::render::CursorStyle> {
        for (i, plugin) in self.plugins.iter().enumerate() {
            if !self.capabilities[i].contains(PluginCapabilities::CURSOR_STYLE) {
                continue;
            }
            if let Some(style) = plugin.cursor_style_override(state) {
                return Some(style);
            }
        }
        None
    }

    /// Query plugins for a default buffer scroll policy. Returns the first non-None.
    pub fn handle_default_scroll(
        &mut self,
        candidate: DefaultScrollCandidate,
        state: &AppState,
    ) -> Option<(PluginId, ScrollPolicyResult)> {
        for (i, plugin) in self.plugins.iter_mut().enumerate() {
            if !self.capabilities[i].contains(PluginCapabilities::SCROLL_POLICY) {
                continue;
            }
            if let Some(result) = plugin.handle_default_scroll(candidate, state) {
                return Some((plugin.id(), result));
            }
        }
        None
    }

    // ===========================================================================
    // New dispatch API: Contribute / Transform / Annotate
    // ===========================================================================

    /// Collect contributions from all plugins for a given region, sorted by priority.
    pub fn collect_contributions(
        &self,
        region: &SlotId,
        state: &AppState,
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
        state: &AppState,
        ctx: &ContributeContext,
    ) -> Vec<SourcedContribution> {
        let mut cache = self.slot_cache.borrow_mut();
        let mut contributions: Vec<SourcedContribution> = self
            .plugins
            .iter()
            .enumerate()
            .filter_map(|(i, plugin)| {
                let caps = self.capabilities[i];
                if !caps.contains(PluginCapabilities::CONTRIBUTOR) {
                    return None;
                }
                // Check contribution cache
                if let Some(entry) = cache.entries.get(i)
                    && let Some(cached) = entry.contributions.get(region)
                {
                    return cached.clone().map(|contribution| SourcedContribution {
                        contributor: plugin.id(),
                        contribution,
                    });
                }
                let result = plugin.contribute_to(region, state, ctx);
                while cache.entries.len() <= i {
                    cache.entries.push(PluginCacheEntry::default());
                }
                cache.entries[i]
                    .contributions
                    .insert(region.clone(), result.clone());
                result.map(|contribution| SourcedContribution {
                    contributor: plugin.id(),
                    contribution,
                })
            })
            .collect();
        contributions.sort_by_key(|c| (c.contribution.priority, c.contributor.clone()));
        contributions
    }

    /// Apply the transform chain for a given target.
    ///
    /// Plugins with the `TRANSFORMER` capability are collected into a chain,
    /// sorted by priority in **descending** order (high priority = inner =
    /// applied first). The `default_element_fn` is evaluated lazily as the
    /// seed element, then each transformer is applied in order.
    pub fn apply_transform_chain(
        &self,
        target: TransformTarget,
        default_element_fn: impl FnOnce() -> Element,
        state: &AppState,
    ) -> Element {
        let mut element = default_element_fn();

        // Collect (index, priority, plugin_id) for TRANSFORMER plugins
        let mut chain: Vec<(usize, i16, PluginId)> = Vec::new();
        for (i, plugin) in self.plugins.iter().enumerate() {
            if self.capabilities[i].contains(PluginCapabilities::TRANSFORMER) {
                let prio = plugin.transform_priority();
                chain.push((i, prio, plugin.id()));
            }
        }
        // Sort by priority descending (high = inner = applied first), then by plugin_id
        chain.sort_by_key(|(_, prio, id)| (std::cmp::Reverse(*prio), id.clone()));

        for (pos, (i, _, _)) in chain.iter().enumerate() {
            let ctx = TransformContext {
                is_default: true,
                chain_position: pos,
            };
            element = self.plugins[*i].transform(&target, element, state, &ctx);
        }

        element
    }

    /// Collect annotations from all annotating plugins for visible lines.
    pub fn collect_annotations(&self, state: &AppState, ctx: &AnnotateContext) -> AnnotationResult {
        if !self.has_capability(PluginCapabilities::ANNOTATOR) {
            return AnnotationResult {
                left_gutter: None,
                right_gutter: None,
                line_backgrounds: None,
            };
        }

        let line_count = state.visible_line_range().len();
        let mut has_left = false;
        let mut has_right = false;
        let mut has_bg = false;

        let mut left_rows: Vec<FlexChild> = Vec::with_capacity(line_count);
        let mut right_rows: Vec<FlexChild> = Vec::with_capacity(line_count);
        let mut backgrounds: Vec<Option<crate::protocol::Face>> = vec![None; line_count];

        for (line, bg_slot) in backgrounds.iter_mut().enumerate().take(line_count) {
            let mut left_parts: Vec<(i16, PluginId, Element)> = Vec::new();
            let mut right_parts: Vec<(i16, PluginId, Element)> = Vec::new();
            let mut bg_layers: Vec<(BackgroundLayer, PluginId)> = Vec::new();

            for (i, plugin) in self.plugins.iter().enumerate() {
                if !self.capabilities[i].contains(PluginCapabilities::ANNOTATOR) {
                    continue;
                }
                if let Some(ann) = plugin.annotate_line_with_ctx(line, state, ctx) {
                    let prio = ann.priority;
                    let pid = plugin.id();
                    if let Some(el) = ann.left_gutter {
                        left_parts.push((prio, pid.clone(), el));
                        has_left = true;
                    }
                    if let Some(el) = ann.right_gutter {
                        right_parts.push((prio, pid.clone(), el));
                        has_right = true;
                    }
                    if let Some(bg) = ann.background {
                        bg_layers.push((bg, pid));
                    }
                }
            }

            // Sort gutter elements by priority then plugin_id (deterministic tie-breaking)
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
                *bg_slot = Some(bg_layers.last().unwrap().0.face);
                has_bg = true;
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
        }
    }

    /// Collect display transformation directives from all plugins and build
    /// a `DisplayMapRef`.
    ///
    /// In the initial implementation, only a single plugin may contribute
    /// directives. If no plugin contributes directives, returns an identity map.
    pub fn collect_display_map(&self, state: &AppState) -> DisplayMapRef {
        if !self.has_capability(PluginCapabilities::DISPLAY_TRANSFORM) {
            let line_count = state.visible_line_range().len();
            return Arc::new(DisplayMap::identity(line_count));
        }

        let mut all_directives = Vec::new();
        let mut contributor_count = 0;
        for (i, plugin) in self.plugins.iter().enumerate() {
            if !self.capabilities[i].contains(PluginCapabilities::DISPLAY_TRANSFORM) {
                continue;
            }
            let directives = plugin.display_directives(state);
            if !directives.is_empty() {
                all_directives.extend(directives);
                contributor_count += 1;
            }
        }

        // Initial constraint: single plugin only
        debug_assert!(
            contributor_count <= 1,
            "DisplayMap: only one plugin may contribute display directives (got {contributor_count})"
        );

        let line_count = state.visible_line_range().len();
        let dm = DisplayMap::build(line_count, &all_directives);
        Arc::new(dm)
    }

    /// Collect raw display directives from all plugins (without building a DisplayMap).
    ///
    /// Used by `sync_display_directives()` to feed directives into Salsa inputs,
    /// where the `display_map_query` tracked function builds the actual `DisplayMap`.
    pub fn collect_display_directives(
        &self,
        state: &AppState,
    ) -> Vec<crate::display::DisplayDirective> {
        if !self.has_capability(PluginCapabilities::DISPLAY_TRANSFORM) {
            return Vec::new();
        }

        let mut all_directives = Vec::new();
        for (i, plugin) in self.plugins.iter().enumerate() {
            if !self.capabilities[i].contains(PluginCapabilities::DISPLAY_TRANSFORM) {
                continue;
            }
            let directives = plugin.display_directives(state);
            if !directives.is_empty() {
                all_directives.extend(directives);
            }
        }
        all_directives
    }

    /// Collect overlay contributions with collision-avoidance context.
    pub fn collect_overlays_with_ctx(
        &self,
        state: &AppState,
        ctx: &OverlayContext,
    ) -> Vec<OverlayContribution> {
        let mut contributions = Vec::new();
        for (i, plugin) in self.plugins.iter().enumerate() {
            let caps = self.capabilities[i];
            if (caps.contains(PluginCapabilities::CONTRIBUTOR)
                || caps.contains(PluginCapabilities::OVERLAY))
                && let Some(mut oc) = plugin.contribute_overlay_with_ctx(state, ctx)
            {
                oc.plugin_id = plugin.id();
                contributions.push(oc);
            }
        }
        contributions.sort_by_key(|c| (c.z_index, c.plugin_id.clone()));
        contributions
    }

    /// Check if any plugin has TRANSFORMER capability for a given target.
    pub fn has_transform_for(&self, _target: TransformTarget) -> bool {
        self.capabilities
            .iter()
            .any(|c| c.contains(PluginCapabilities::TRANSFORMER))
    }

    // --- Plugin message delivery ---

    /// Check whether a plugin is allowed to spawn external processes.
    pub fn plugin_allows_process_spawn(&self, plugin_id: &PluginId) -> bool {
        self.plugins
            .iter()
            .find(|p| &p.id() == plugin_id)
            .is_some_and(|p| p.allows_process_spawn())
    }

    /// Deliver an I/O event to a specific plugin by ID.
    pub fn deliver_io_event_batch(
        &mut self,
        target: &PluginId,
        event: &IoEvent,
        state: &AppState,
    ) -> RuntimeBatch {
        crate::perf::perf_span!("deliver_io_event");
        for (i, plugin) in self.plugins.iter_mut().enumerate() {
            if &plugin.id() == target {
                if !self.capabilities[i].contains(PluginCapabilities::IO_HANDLER) {
                    return RuntimeBatch::default();
                }
                let mut batch = RuntimeBatch::default();
                batch
                    .effects
                    .merge(plugin.on_io_event_effects(event, state));
                return batch;
            }
        }
        RuntimeBatch::default()
    }

    /// Deliver a message to a specific plugin by ID.
    pub fn deliver_message_batch(
        &mut self,
        target: &PluginId,
        payload: Box<dyn Any>,
        state: &AppState,
    ) -> RuntimeBatch {
        let mut payload = payload;
        for plugin in &mut self.plugins {
            if &plugin.id() == target {
                let mut batch = RuntimeBatch::default();
                batch
                    .effects
                    .merge(plugin.update_effects(payload.as_mut(), state));
                return batch;
            }
        }
        RuntimeBatch::default()
    }

    /// Register a `Plugin` by wrapping it in a `PluginBridge`.
    ///
    /// The bridge adapts the pure interface to `PluginBackend`, with framework-owned
    /// state and generation-based `state_hash()` for L1 cache invalidation.
    pub fn register<P: Plugin>(&mut self, plugin: P) {
        let bridge = PluginBridge::new(plugin);
        self.register_backend(Box::new(bridge));
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}
