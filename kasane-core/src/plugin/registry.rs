use std::any::Any;
use std::sync::Arc;

use crate::display::{DisplayMap, DisplayMapRef};
use crate::element::{Element, FlexChild, InteractiveId};
use crate::input::{KeyEvent, MouseEvent};
use crate::scroll::{DefaultScrollCandidate, ScrollPolicyResult};
use crate::state::DirtyFlags;
use crate::workspace::Placement;
use crate::workspace::WorkspaceQuery;

use super::AppView;
use super::bridge::PluginBridge;
use super::effects::{MouseHandleResult, PluginEffects};
use super::state::Plugin;
use super::{
    AnnotateContext, AnnotationResult, BackgroundLayer, Command, ContributeContext, Contribution,
    InitBatch, IoEvent, KeyHandleResult, OverlayContext, OverlayContribution, PaintHook,
    PaneContext, PluginAuthorities, PluginBackend, PluginCapabilities, PluginId, ReadyBatch,
    RuntimeBatch, SlotId, SourcedContribution, TransformContext, TransformTarget,
};

pub struct PluginSurfaceSet {
    pub owner: PluginId,
    pub surfaces: Vec<Box<dyn crate::surface::Surface>>,
    pub legacy_workspace_request: Option<Placement>,
}

pub struct PluginRuntime {
    plugins: Vec<Box<dyn PluginBackend>>,
    capabilities: Vec<PluginCapabilities>,
    authorities: Vec<PluginAuthorities>,
    any_plugin_state_changed: bool,
    last_state_hashes: Vec<u64>,
}

/// Immutable view over plugins for the render phase.
///
/// Borrows the plugin list and capabilities from [`PluginRuntime`] without
/// requiring `&mut` access. All read-only view queries (contribute, transform,
/// annotate, overlay, display map, paint hooks, etc.) live here.
pub struct PluginView<'a> {
    plugins: &'a [Box<dyn PluginBackend>],
    capabilities: &'a [PluginCapabilities],
}

pub enum KeyDispatchResult {
    Consumed {
        source_plugin: PluginId,
        commands: Vec<Command>,
    },
    Passthrough(KeyEvent),
}

impl PluginRuntime {
    pub fn new() -> Self {
        PluginRuntime {
            plugins: Vec::new(),
            capabilities: Vec::new(),
            authorities: Vec::new(),
            any_plugin_state_changed: false,
            last_state_hashes: Vec::new(),
        }
    }

    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }

    /// Borrow an immutable view for the render phase.
    pub fn view(&self) -> PluginView<'_> {
        PluginView {
            plugins: &self.plugins,
            capabilities: &self.capabilities,
        }
    }

    /// Returns true if any plugin's state_hash changed during the last
    /// `prepare_plugin_cache()` call.
    pub fn any_plugin_state_changed(&self) -> bool {
        self.any_plugin_state_changed
    }

    pub fn register_backend(&mut self, plugin: Box<dyn PluginBackend>) {
        let id = plugin.id();
        let caps = plugin.capabilities();
        let authorities = plugin.authorities();
        if let Some(pos) = self.plugins.iter().position(|p| p.id() == id) {
            // Replace existing plugin with same ID (e.g. FS plugin overrides bundled)
            self.plugins[pos] = plugin;
            self.capabilities[pos] = caps;
            self.authorities[pos] = authorities;
            self.last_state_hashes[pos] = 0;
        } else {
            self.plugins.push(plugin);
            self.capabilities.push(caps);
            self.authorities.push(authorities);
            self.last_state_hashes.push(0);
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
            self.authorities.remove(pos);
            self.last_state_hashes.remove(pos);
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
            self.authorities.remove(pos);
            self.last_state_hashes.remove(pos);
            true
        } else {
            false
        }
    }

    /// Check whether any plugin's internal state changed since the last call.
    /// Call once per frame before rendering (during the mutable phase).
    pub fn prepare_plugin_cache(&mut self, _dirty: DirtyFlags) {
        // Grow hash tracking if plugins were registered after last prepare
        while self.last_state_hashes.len() < self.plugins.len() {
            self.last_state_hashes.push(0);
        }

        self.any_plugin_state_changed = false;
        for (i, plugin) in self.plugins.iter().enumerate() {
            let current_hash = plugin.state_hash();
            if current_hash != self.last_state_hashes[i] {
                self.last_state_hashes[i] = current_hash;
                self.any_plugin_state_changed = true;
            }
        }
    }

    /// Initialize all plugins and collect typed bootstrap effects.
    pub fn init_all_batch(&mut self, app: &AppView<'_>) -> InitBatch {
        let mut batch = InitBatch::default();
        for plugin in &mut self.plugins {
            batch.effects.merge(plugin.on_init_effects(app));
        }
        batch
    }

    /// Initialize all plugins.
    pub fn init_all(&mut self, app: &AppView<'_>) -> InitBatch {
        self.init_all_batch(app)
    }

    /// Notify all plugins that the active session is ready for transport-bound startup work.
    pub fn notify_active_session_ready_batch(&mut self, app: &AppView<'_>) -> ReadyBatch {
        let mut batch = ReadyBatch::default();
        for plugin in &mut self.plugins {
            batch
                .effects
                .merge(plugin.on_active_session_ready_effects(app));
        }
        batch
    }

    /// Notify all plugins that the active session is ready for transport-bound startup work.
    pub fn notify_active_session_ready(&mut self, app: &AppView<'_>) -> ReadyBatch {
        self.notify_active_session_ready_batch(app)
    }

    /// Notify a single plugin that the active session is ready.
    pub fn notify_plugin_active_session_ready_batch(
        &mut self,
        target: &PluginId,
        app: &AppView<'_>,
    ) -> ReadyBatch {
        for plugin in &mut self.plugins {
            if &plugin.id() == target {
                let mut batch = ReadyBatch::default();
                batch
                    .effects
                    .merge(plugin.on_active_session_ready_effects(app));
                return batch;
            }
        }
        ReadyBatch::default()
    }

    /// Notify all plugins about a state change and collect typed runtime effects.
    pub fn notify_state_changed_batch(
        &mut self,
        app: &AppView<'_>,
        dirty: DirtyFlags,
    ) -> RuntimeBatch {
        let mut batch = RuntimeBatch::default();
        for plugin in &mut self.plugins {
            batch
                .effects
                .merge(plugin.on_state_changed_effects(app, dirty));
        }
        batch
    }

    /// Notify interested plugins that the workspace layout changed.
    pub fn notify_workspace_changed(&mut self, query: &WorkspaceQuery<'_>) {
        for (i, plugin) in self.plugins.iter_mut().enumerate() {
            if !self.capabilities[i].contains(PluginCapabilities::WORKSPACE_OBSERVER) {
                continue;
            }
            plugin.on_workspace_changed(query);
        }
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
        app: &AppView<'_>,
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
            batch.effects.merge(self.plugins[pos].on_init_effects(app));
            return batch;
        }
        InitBatch::default()
    }

    /// Reload a single plugin by replacing it in-place.
    pub fn reload_plugin(
        &mut self,
        plugin: Box<dyn PluginBackend>,
        app: &AppView<'_>,
    ) -> InitBatch {
        self.reload_plugin_batch(plugin, app)
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

    /// Collect plugin-owned surfaces for a single owner during reload reconciliation.
    pub fn collect_plugin_surfaces_for_owner(
        &mut self,
        target: &PluginId,
    ) -> Option<PluginSurfaceSet> {
        for plugin in &mut self.plugins {
            if plugin.id() != *target {
                continue;
            }
            let owner = plugin.id();
            let plugin_surfaces = plugin.surfaces();
            if plugin_surfaces.is_empty() {
                return None;
            }
            return Some(PluginSurfaceSet {
                owner,
                surfaces: plugin_surfaces,
                legacy_workspace_request: plugin.workspace_request(),
            });
        }
        None
    }

    /// Collect paint hooks from all plugins. Call after `init_all_batch()`.
    pub fn collect_paint_hooks(&self) -> Vec<Box<dyn PaintHook>> {
        self.view().collect_paint_hooks()
    }

    /// Collect paint hooks for a single owner in plugin registration order.
    pub fn collect_paint_hooks_for_owner(&self, target: &PluginId) -> Vec<Box<dyn PaintHook>> {
        self.view().collect_paint_hooks_for_owner(target)
    }

    /// Plugin IDs that currently contribute paint hooks, in registry order.
    pub fn paint_hook_owners_in_order(&self) -> Vec<PluginId> {
        self.view().paint_hook_owners_in_order()
    }

    // --- Menu item transformation ---

    /// Transform a menu item through all plugins. Returns None if no plugin transforms it.
    pub fn transform_menu_item(
        &self,
        item: &[crate::protocol::Atom],
        index: usize,
        selected: bool,
        app: &AppView<'_>,
    ) -> Option<Vec<crate::protocol::Atom>> {
        self.view().transform_menu_item(item, index, selected, app)
    }

    // --- Cursor style override ---

    /// Query plugins for a cursor style override. Returns the first non-None.
    pub fn cursor_style_override(&self, app: &AppView<'_>) -> Option<crate::render::CursorStyle> {
        self.view().cursor_style_override(app)
    }

    /// Query plugins for a default buffer scroll policy. Returns the first non-None.
    pub fn handle_default_scroll(
        &mut self,
        candidate: DefaultScrollCandidate,
        app: &AppView<'_>,
    ) -> Option<(PluginId, ScrollPolicyResult)> {
        for (i, plugin) in self.plugins.iter_mut().enumerate() {
            if !self.capabilities[i].contains(PluginCapabilities::SCROLL_POLICY) {
                continue;
            }
            if let Some(result) = plugin.handle_default_scroll(candidate, app) {
                return Some((plugin.id(), result));
            }
        }
        None
    }

    pub fn dispatch_key_middleware(
        &mut self,
        key: &KeyEvent,
        app: &AppView<'_>,
    ) -> KeyDispatchResult {
        let mut current_key = key.clone();
        for plugin in &mut self.plugins {
            match plugin.handle_key_middleware(&current_key, app) {
                KeyHandleResult::Consumed(commands) => {
                    return KeyDispatchResult::Consumed {
                        source_plugin: plugin.id(),
                        commands,
                    };
                }
                KeyHandleResult::Transformed(next_key) => current_key = next_key,
                KeyHandleResult::Passthrough => {}
            }
        }

        KeyDispatchResult::Passthrough(current_key)
    }

    // ===========================================================================
    // New dispatch API: Contribute / Transform / Annotate
    // ===========================================================================

    /// Collect contributions from all plugins for a given region, sorted by priority.
    pub fn collect_contributions(
        &self,
        region: &SlotId,
        app: &AppView<'_>,
        ctx: &ContributeContext,
    ) -> Vec<Contribution> {
        self.view().collect_contributions(region, app, ctx)
    }

    pub fn collect_contributions_with_sources(
        &self,
        region: &SlotId,
        app: &AppView<'_>,
        ctx: &ContributeContext,
    ) -> Vec<SourcedContribution> {
        self.view()
            .collect_contributions_with_sources(region, app, ctx)
    }

    /// Apply the transform chain for a given target.
    pub fn apply_transform_chain(
        &self,
        target: TransformTarget,
        default_element_fn: impl FnOnce() -> Element,
        app: &AppView<'_>,
    ) -> Element {
        self.apply_transform_chain_in_pane(target, default_element_fn, app, PaneContext::default())
    }

    pub fn apply_transform_chain_in_pane(
        &self,
        target: TransformTarget,
        default_element_fn: impl FnOnce() -> Element,
        app: &AppView<'_>,
        pane_context: PaneContext,
    ) -> Element {
        self.view()
            .apply_transform_chain_in_pane(target, default_element_fn, app, pane_context)
    }

    /// Collect annotations from all annotating plugins for visible lines.
    pub fn collect_annotations(
        &self,
        app: &AppView<'_>,
        ctx: &AnnotateContext,
    ) -> AnnotationResult {
        self.view().collect_annotations(app, ctx)
    }

    /// Collect display transformation directives from all plugins and build
    /// a `DisplayMapRef`.
    pub fn collect_display_map(&self, app: &AppView<'_>) -> DisplayMapRef {
        self.view().collect_display_map(app)
    }

    /// Collect raw display directives from all plugins (without building a DisplayMap).
    pub fn collect_display_directives(
        &self,
        app: &AppView<'_>,
    ) -> Vec<crate::display::DisplayDirective> {
        self.view().collect_display_directives(app)
    }

    /// Collect overlay contributions with collision-avoidance context.
    pub fn collect_overlays_with_ctx(
        &self,
        app: &AppView<'_>,
        ctx: &OverlayContext,
    ) -> Vec<OverlayContribution> {
        self.view().collect_overlays_with_ctx(app, ctx)
    }

    /// Check if any plugin has TRANSFORMER capability for a given target.
    pub fn has_transform_for(&self, _target: TransformTarget) -> bool {
        self.view().has_transform_for(_target)
    }

    // --- Plugin message delivery ---

    /// Check whether a plugin is allowed to spawn external processes.
    pub fn plugin_allows_process_spawn(&self, plugin_id: &PluginId) -> bool {
        self.plugins
            .iter()
            .find(|p| &p.id() == plugin_id)
            .is_some_and(|p| p.allows_process_spawn())
    }

    /// Check whether a plugin has a specific host-resolved authority.
    pub fn plugin_has_authority(&self, plugin_id: &PluginId, authority: PluginAuthorities) -> bool {
        self.plugins
            .iter()
            .zip(self.authorities.iter())
            .find(|(plugin, _)| &plugin.id() == plugin_id)
            .is_some_and(|(_, authorities)| authorities.contains(authority))
    }

    /// Deliver an I/O event to a specific plugin by ID.
    pub fn deliver_io_event_batch(
        &mut self,
        target: &PluginId,
        event: &IoEvent,
        app: &AppView<'_>,
    ) -> RuntimeBatch {
        crate::perf::perf_span!("deliver_io_event");
        for (i, plugin) in self.plugins.iter_mut().enumerate() {
            if &plugin.id() == target {
                if !self.capabilities[i].contains(PluginCapabilities::IO_HANDLER) {
                    return RuntimeBatch::default();
                }
                let mut batch = RuntimeBatch::default();
                batch.effects.merge(plugin.on_io_event_effects(event, app));
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
        app: &AppView<'_>,
    ) -> RuntimeBatch {
        let mut payload = payload;
        for plugin in &mut self.plugins {
            if &plugin.id() == target {
                let mut batch = RuntimeBatch::default();
                batch
                    .effects
                    .merge(plugin.update_effects(payload.as_mut(), app));
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

    /// Broadcast key observation to all plugins.
    pub fn observe_key_all(&mut self, key: &KeyEvent, app: &AppView<'_>) {
        for plugin in self.plugins_mut() {
            plugin.observe_key(key, app);
        }
    }

    /// Broadcast mouse observation to all plugins.
    pub fn observe_mouse_all(&mut self, event: &MouseEvent, app: &AppView<'_>) {
        for plugin in self.plugins_mut() {
            plugin.observe_mouse(event, app);
        }
    }

    /// First-wins mouse handler dispatch.
    pub fn dispatch_mouse_handler(
        &mut self,
        event: &MouseEvent,
        id: InteractiveId,
        app: &AppView<'_>,
    ) -> MouseHandleResult {
        for plugin in self.plugins_mut() {
            if let Some(commands) = plugin.handle_mouse(event, id, app) {
                let source = plugin.id();
                return MouseHandleResult::Handled {
                    source_plugin: source,
                    commands,
                };
            }
        }
        MouseHandleResult::NotHandled
    }
}

impl PluginEffects for PluginRuntime {
    fn notify_state_changed(&mut self, app: &AppView<'_>, flags: DirtyFlags) -> RuntimeBatch {
        self.notify_state_changed_batch(app, flags)
    }

    fn observe_key_all(&mut self, key: &KeyEvent, app: &AppView<'_>) {
        PluginRuntime::observe_key_all(self, key, app)
    }

    fn dispatch_key_middleware(&mut self, key: &KeyEvent, app: &AppView<'_>) -> KeyDispatchResult {
        PluginRuntime::dispatch_key_middleware(self, key, app)
    }

    fn observe_mouse_all(&mut self, event: &MouseEvent, app: &AppView<'_>) {
        PluginRuntime::observe_mouse_all(self, event, app)
    }

    fn dispatch_mouse_handler(
        &mut self,
        event: &MouseEvent,
        id: InteractiveId,
        app: &AppView<'_>,
    ) -> MouseHandleResult {
        PluginRuntime::dispatch_mouse_handler(self, event, id, app)
    }

    fn handle_default_scroll(
        &mut self,
        candidate: DefaultScrollCandidate,
        app: &AppView<'_>,
    ) -> Option<ScrollPolicyResult> {
        PluginRuntime::handle_default_scroll(self, candidate, app).map(|(_, result)| result)
    }
}

impl<'a> PluginView<'a> {
    /// Check if any registered plugin has the given capability.
    fn has_capability(&self, cap: PluginCapabilities) -> bool {
        self.capabilities.iter().any(|c| c.contains(cap))
    }

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
        use super::compose::{Composable, ContributionSet};

        self.plugins
            .iter()
            .enumerate()
            .filter_map(|(i, plugin)| {
                let caps = self.capabilities[i];
                if !caps.contains(PluginCapabilities::CONTRIBUTOR) {
                    return None;
                }
                let result = plugin.contribute_to(region, state, ctx);
                result.map(|contribution| SourcedContribution {
                    contributor: plugin.id(),
                    contribution,
                })
            })
            .fold(ContributionSet::empty(), |acc, sc| {
                acc.compose(ContributionSet::from_vec(vec![sc]))
            })
            .into_vec()
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
        state: &AppView<'_>,
    ) -> Element {
        self.apply_transform_chain_in_pane(
            target,
            default_element_fn,
            state,
            PaneContext::default(),
        )
    }

    pub fn apply_transform_chain_in_pane(
        &self,
        target: TransformTarget,
        default_element_fn: impl FnOnce() -> Element,
        state: &AppView<'_>,
        pane_context: PaneContext,
    ) -> Element {
        let mut element = default_element_fn();

        let mut chain: Vec<(usize, i16, PluginId)> = Vec::new();
        for (i, plugin) in self.plugins.iter().enumerate() {
            if self.capabilities[i].contains(PluginCapabilities::TRANSFORMER) {
                let prio = plugin.transform_priority();
                chain.push((i, prio, plugin.id()));
            }
        }
        chain.sort_by_key(|(_, prio, id)| (std::cmp::Reverse(*prio), id.clone()));

        for (pos, (i, _, _)) in chain.iter().enumerate() {
            let ctx = TransformContext {
                is_default: true,
                chain_position: pos,
                pane_surface_id: pane_context.surface_id,
                pane_focused: pane_context.focused,
            };
            element = self.plugins[*i].transform(&target, element, state, &ctx);
        }

        element
    }

    /// Collect annotations from all annotating plugins for visible lines.
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
        let directives = crate::display::resolve(&set, line_count);
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
    fn collect_tagged_display_directives(
        &self,
        state: &AppView<'_>,
    ) -> crate::display::DirectiveSet {
        let mut set = crate::display::DirectiveSet::default();
        for (i, plugin) in self.plugins.iter().enumerate() {
            if !self.capabilities[i].contains(PluginCapabilities::DISPLAY_TRANSFORM) {
                continue;
            }
            let directives = plugin.display_directives(state);
            if directives.is_empty() {
                continue;
            }
            let priority = plugin.display_directive_priority();
            let plugin_id = plugin.id();
            for d in directives {
                set.push(d, priority, plugin_id.clone());
            }
        }
        set
    }

    /// Collect overlay contributions with collision-avoidance context.
    pub fn collect_overlays_with_ctx(
        &self,
        state: &AppView<'_>,
        ctx: &OverlayContext,
    ) -> Vec<OverlayContribution> {
        use super::compose::{Composable, OverlaySet};

        self.plugins
            .iter()
            .enumerate()
            .filter_map(|(i, plugin)| {
                let caps = self.capabilities[i];
                if (caps.contains(PluginCapabilities::CONTRIBUTOR)
                    || caps.contains(PluginCapabilities::OVERLAY))
                    && let Some(mut oc) = plugin.contribute_overlay_with_ctx(state, ctx)
                {
                    oc.plugin_id = plugin.id();
                    Some(oc)
                } else {
                    None
                }
            })
            .fold(OverlaySet::empty(), |acc, oc| {
                acc.compose(OverlaySet::from_vec(vec![oc]))
            })
            .into_vec()
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

    /// Query plugins for a cursor style override. Returns the first non-None.
    pub fn cursor_style_override(&self, state: &AppView<'_>) -> Option<crate::render::CursorStyle> {
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

    /// Collect paint hooks from all plugins.
    pub fn collect_paint_hooks(&self) -> Vec<Box<dyn PaintHook>> {
        let mut hooks = Vec::new();
        for (i, plugin) in self.plugins.iter().enumerate() {
            if self.capabilities[i].contains(PluginCapabilities::PAINT_HOOK) {
                hooks.extend(plugin.paint_hooks());
            }
        }
        hooks
    }

    /// Collect paint hooks for a single owner in plugin registration order.
    pub fn collect_paint_hooks_for_owner(&self, target: &PluginId) -> Vec<Box<dyn PaintHook>> {
        for (i, plugin) in self.plugins.iter().enumerate() {
            if plugin.id() != *target {
                continue;
            }
            if !self.capabilities[i].contains(PluginCapabilities::PAINT_HOOK) {
                return vec![];
            }
            return plugin.paint_hooks();
        }
        vec![]
    }

    /// Plugin IDs that currently contribute paint hooks, in registry order.
    pub fn paint_hook_owners_in_order(&self) -> Vec<PluginId> {
        self.plugins
            .iter()
            .zip(self.capabilities.iter())
            .filter(|(_, caps)| caps.contains(PluginCapabilities::PAINT_HOOK))
            .map(|(plugin, _)| plugin.id())
            .collect()
    }

    /// Check if any plugin has TRANSFORMER capability for a given target.
    pub fn has_transform_for(&self, _target: TransformTarget) -> bool {
        self.capabilities
            .iter()
            .any(|c| c.contains(PluginCapabilities::TRANSFORMER))
    }
}

impl Default for PluginRuntime {
    fn default() -> Self {
        Self::new()
    }
}
