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
use super::context::TransformScope;
use super::effects::{MouseHandleResult, PluginEffects};
use super::state::Plugin;
use super::{
    AnnotateContext, AnnotationResult, BackgroundLayer, Command, ContributeContext, Contribution,
    InitBatch, IoEvent, KeyHandleResult, OverlayContext, OverlayContribution, PaintHook,
    PaneContext, PluginAuthorities, PluginBackend, PluginCapabilities, PluginId, ReadyBatch,
    RuntimeBatch, SlotId, SourcedContribution, TransformContext, TransformSubject, TransformTarget,
};

pub struct PluginSurfaceSet {
    pub owner: PluginId,
    pub surfaces: Vec<Box<dyn crate::surface::Surface>>,
    pub legacy_workspace_request: Option<Placement>,
}

/// Sentinel value for `last_state_hash`: guarantees hash mismatch on first
/// `prepare_plugin_cache()` after registration, so newly registered plugins
/// are always collected on their first frame.
const HASH_SENTINEL: u64 = u64::MAX;

pub(crate) struct PluginSlot {
    pub(crate) backend: Box<dyn PluginBackend>,
    pub(crate) capabilities: PluginCapabilities,
    pub(crate) authorities: PluginAuthorities,
    pub(crate) last_state_hash: u64,
    pub(crate) needs_recollect: bool,
}

pub struct PluginRuntime {
    slots: Vec<PluginSlot>,
    any_plugin_state_changed: bool,
}

/// Immutable view over plugins for the render phase.
///
/// Borrows the plugin list and capabilities from [`PluginRuntime`] without
/// requiring `&mut` access. All read-only view queries (contribute, transform,
/// annotate, overlay, display map, paint hooks, etc.) live here.
pub struct PluginView<'a> {
    slots: &'a [PluginSlot],
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
            slots: Vec::new(),
            any_plugin_state_changed: false,
        }
    }

    pub fn plugin_count(&self) -> usize {
        self.slots.len()
    }

    /// Borrow an immutable view for the render phase.
    pub fn view(&self) -> PluginView<'_> {
        PluginView { slots: &self.slots }
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
        if let Some(pos) = self.slots.iter().position(|s| s.backend.id() == id) {
            // Replace existing plugin with same ID (e.g. FS plugin overrides bundled)
            let slot = &mut self.slots[pos];
            slot.backend = plugin;
            slot.capabilities = caps;
            slot.authorities = authorities;
            slot.last_state_hash = HASH_SENTINEL;
            slot.needs_recollect = true;
        } else {
            self.slots.push(PluginSlot {
                backend: plugin,
                capabilities: caps,
                authorities,
                last_state_hash: HASH_SENTINEL,
                needs_recollect: true,
            });
        }
    }

    pub fn contains_plugin(&self, id: &PluginId) -> bool {
        self.slots.iter().any(|s| s.backend.id() == *id)
    }

    /// Remove a plugin from the registry without running shutdown hooks.
    ///
    /// Prefer [`Self::unload_plugin`] for normal lifecycle transitions.
    pub fn remove_plugin(&mut self, id: &PluginId) -> bool {
        if let Some(pos) = self.slots.iter().position(|s| s.backend.id() == *id) {
            self.slots.remove(pos);
            true
        } else {
            false
        }
    }

    /// Shut down and remove a single plugin by ID.
    pub fn unload_plugin(&mut self, id: &PluginId) -> bool {
        if let Some(pos) = self.slots.iter().position(|s| s.backend.id() == *id) {
            self.slots[pos].backend.on_shutdown();
            self.slots.remove(pos);
            true
        } else {
            false
        }
    }

    /// Check whether any plugin's internal state changed since the last call,
    /// and compute per-plugin `needs_recollect` based on state hash changes
    /// and the intersection of `dirty` flags with each plugin's `view_deps()`.
    pub fn prepare_plugin_cache(&mut self, dirty: DirtyFlags) {
        self.any_plugin_state_changed = false;
        for slot in &mut self.slots {
            let current_hash = slot.backend.state_hash();
            let hash_changed = current_hash != slot.last_state_hash;
            if hash_changed {
                slot.last_state_hash = current_hash;
                self.any_plugin_state_changed = true;
            }
            slot.needs_recollect = hash_changed || dirty.intersects(slot.backend.view_deps());
        }
    }

    /// Returns true if any plugin needs its view contributions re-collected.
    pub fn any_needs_recollect(&self) -> bool {
        self.slots.iter().any(|s| s.needs_recollect)
    }

    /// Initialize all plugins and collect typed bootstrap effects.
    pub fn init_all_batch(&mut self, app: &AppView<'_>) -> InitBatch {
        let mut batch = InitBatch::default();
        for slot in &mut self.slots {
            batch.effects.merge(slot.backend.on_init_effects(app));
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
        for slot in &mut self.slots {
            batch
                .effects
                .merge(slot.backend.on_active_session_ready_effects(app));
        }
        batch
    }

    /// Notify a single plugin that the active session is ready.
    pub fn notify_plugin_active_session_ready_batch(
        &mut self,
        target: &PluginId,
        app: &AppView<'_>,
    ) -> ReadyBatch {
        for slot in &mut self.slots {
            if &slot.backend.id() == target {
                let mut batch = ReadyBatch::default();
                batch
                    .effects
                    .merge(slot.backend.on_active_session_ready_effects(app));
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
        for slot in &mut self.slots {
            batch
                .effects
                .merge(slot.backend.on_state_changed_effects(app, dirty));
        }
        batch
    }

    /// Notify interested plugins that the workspace layout changed.
    pub fn notify_workspace_changed(&mut self, query: &WorkspaceQuery<'_>) {
        for slot in &mut self.slots {
            if !slot
                .capabilities
                .contains(PluginCapabilities::WORKSPACE_OBSERVER)
            {
                continue;
            }
            slot.backend.on_workspace_changed(query);
        }
    }

    /// Shut down all plugins. Call before application exit.
    pub fn shutdown_all(&mut self) {
        for slot in &mut self.slots {
            slot.backend.on_shutdown();
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
        if let Some(pos) = self.slots.iter().position(|s| s.backend.id() == id) {
            self.slots[pos].backend.on_shutdown();
        }
        // register_backend handles replacement or insertion
        self.register_backend(plugin);
        // Init the new plugin
        if let Some(pos) = self.slots.iter().position(|s| s.backend.id() == id) {
            let mut batch = InitBatch::default();
            batch
                .effects
                .merge(self.slots[pos].backend.on_init_effects(app));
            return batch;
        }
        InitBatch::default()
    }

    pub fn plugins_mut(&mut self) -> impl Iterator<Item = &mut Box<dyn PluginBackend>> {
        self.slots.iter_mut().map(|s| &mut s.backend)
    }

    /// Collect plugin-owned surfaces during the bootstrap preflight stage.
    pub fn collect_plugin_surfaces(&mut self) -> Vec<PluginSurfaceSet> {
        let mut surfaces = Vec::new();
        for slot in &mut self.slots {
            let owner = slot.backend.id();
            let plugin_surfaces = slot.backend.surfaces();
            if !plugin_surfaces.is_empty() {
                surfaces.push(PluginSurfaceSet {
                    owner,
                    surfaces: plugin_surfaces,
                    legacy_workspace_request: slot.backend.workspace_request(),
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
        for slot in &mut self.slots {
            if slot.backend.id() != *target {
                continue;
            }
            let owner = slot.backend.id();
            let plugin_surfaces = slot.backend.surfaces();
            if plugin_surfaces.is_empty() {
                return None;
            }
            return Some(PluginSurfaceSet {
                owner,
                surfaces: plugin_surfaces,
                legacy_workspace_request: slot.backend.workspace_request(),
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
        for slot in &mut self.slots {
            if !slot
                .capabilities
                .contains(PluginCapabilities::SCROLL_POLICY)
            {
                continue;
            }
            if let Some(result) = slot.backend.handle_default_scroll(candidate, app) {
                return Some((slot.backend.id(), result));
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
        for slot in &mut self.slots {
            match slot.backend.handle_key_middleware(&current_key, app) {
                KeyHandleResult::Consumed(commands) => {
                    return KeyDispatchResult::Consumed {
                        source_plugin: slot.backend.id(),
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
        subject: TransformSubject,
        app: &AppView<'_>,
    ) -> TransformSubject {
        self.apply_transform_chain_in_pane(target, subject, app, PaneContext::default())
    }

    pub fn apply_transform_chain_in_pane(
        &self,
        target: TransformTarget,
        subject: TransformSubject,
        app: &AppView<'_>,
        pane_context: PaneContext,
    ) -> TransformSubject {
        self.view()
            .apply_transform_chain_in_pane(target, subject, app, pane_context)
    }

    /// Apply the hierarchical transform chain for a target with refinement.
    ///
    /// For style-specific targets (e.g. `MenuPrompt`), this applies the generic
    /// parent target (`Menu`) first, then the specific target (`MenuPrompt`).
    /// For non-refinement targets, this is equivalent to `apply_transform_chain`.
    pub fn apply_transform_chain_hierarchical(
        &self,
        target: TransformTarget,
        subject: TransformSubject,
        app: &AppView<'_>,
    ) -> TransformSubject {
        self.view()
            .apply_transform_chain_hierarchical(target, subject, app)
    }

    pub fn apply_transform_chain_hierarchical_in_pane(
        &self,
        target: TransformTarget,
        subject: TransformSubject,
        app: &AppView<'_>,
        pane_context: PaneContext,
    ) -> TransformSubject {
        self.view()
            .apply_transform_chain_hierarchical_in_pane(target, subject, app, pane_context)
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
        self.slots
            .iter()
            .find(|s| &s.backend.id() == plugin_id)
            .is_some_and(|s| s.backend.allows_process_spawn())
    }

    /// Check whether a plugin has a specific host-resolved authority.
    pub fn plugin_has_authority(&self, plugin_id: &PluginId, authority: PluginAuthorities) -> bool {
        self.slots
            .iter()
            .find(|s| &s.backend.id() == plugin_id)
            .is_some_and(|s| s.authorities.contains(authority))
    }

    /// Deliver an I/O event to a specific plugin by ID.
    pub fn deliver_io_event_batch(
        &mut self,
        target: &PluginId,
        event: &IoEvent,
        app: &AppView<'_>,
    ) -> RuntimeBatch {
        crate::perf::perf_span!("deliver_io_event");
        for slot in &mut self.slots {
            if &slot.backend.id() == target {
                if !slot.capabilities.contains(PluginCapabilities::IO_HANDLER) {
                    return RuntimeBatch::default();
                }
                let mut batch = RuntimeBatch::default();
                batch
                    .effects
                    .merge(slot.backend.on_io_event_effects(event, app));
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
        for slot in &mut self.slots {
            if &slot.backend.id() == target {
                let mut batch = RuntimeBatch::default();
                batch
                    .effects
                    .merge(slot.backend.update_effects(payload.as_mut(), app));
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
        for slot in &mut self.slots {
            slot.backend.observe_key(key, app);
        }
    }

    /// Broadcast mouse observation to all plugins.
    pub fn observe_mouse_all(&mut self, event: &MouseEvent, app: &AppView<'_>) {
        for slot in &mut self.slots {
            slot.backend.observe_mouse(event, app);
        }
    }

    /// First-wins mouse handler dispatch.
    pub fn dispatch_mouse_handler(
        &mut self,
        event: &MouseEvent,
        id: InteractiveId,
        app: &AppView<'_>,
    ) -> MouseHandleResult {
        for slot in &mut self.slots {
            if let Some(commands) = slot.backend.handle_mouse(event, id, app) {
                let source = slot.backend.id();
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
    /// Returns true if any plugin needs its view contributions re-collected.
    pub fn any_needs_recollect(&self) -> bool {
        self.slots.iter().any(|s| s.needs_recollect)
    }

    /// Check if any registered plugin has the given capability.
    fn has_capability(&self, cap: PluginCapabilities) -> bool {
        self.slots.iter().any(|s| s.capabilities.contains(cap))
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
        let mut result = subject;

        let mut chain: Vec<(usize, i16, PluginId)> = Vec::new();
        for (i, slot) in self.slots.iter().enumerate() {
            if slot.capabilities.contains(PluginCapabilities::TRANSFORMER) {
                let prio = slot.backend.transform_priority();
                chain.push((i, prio, slot.backend.id()));
            }
        }
        chain.sort_by_key(|(_, prio, id)| (std::cmp::Reverse(*prio), id.clone()));

        #[cfg(debug_assertions)]
        detect_transform_conflicts(&chain, self.slots, &target);

        for (pos, (i, _, _)) in chain.iter().enumerate() {
            let ctx = TransformContext {
                is_default: true,
                chain_position: pos,
                pane_surface_id: pane_context.surface_id,
                pane_focused: pane_context.focused,
            };
            result = self.slots[*i]
                .backend
                .transform(&target, result, state, &ctx);
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
            };
        }

        let line_count = state.visible_line_range().len();
        let mut has_left = false;
        let mut has_right = false;
        let mut has_bg = false;
        let mut has_inline = false;

        let mut left_rows: Vec<FlexChild> = Vec::with_capacity(line_count);
        let mut right_rows: Vec<FlexChild> = Vec::with_capacity(line_count);
        let mut backgrounds: Vec<Option<crate::protocol::Face>> = vec![None; line_count];
        let mut inline_decorations: Vec<Option<crate::render::InlineDecoration>> =
            vec![None; line_count];

        for line in 0..line_count {
            let mut left_parts: Vec<(i16, PluginId, Element)> = Vec::new();
            let mut right_parts: Vec<(i16, PluginId, Element)> = Vec::new();
            let mut bg_layers: Vec<(BackgroundLayer, PluginId)> = Vec::new();

            for slot in self.slots.iter() {
                if !slot.capabilities.contains(PluginCapabilities::ANNOTATOR) {
                    continue;
                }
                if let Some(ann) = slot.backend.annotate_line_with_ctx(line, state, ctx) {
                    let prio = ann.priority;
                    let pid = slot.backend.id();
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
        for slot in self.slots.iter() {
            if !slot
                .capabilities
                .contains(PluginCapabilities::DISPLAY_TRANSFORM)
            {
                continue;
            }
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
        set
    }

    /// Collect overlay contributions with collision-avoidance context.
    pub fn collect_overlays_with_ctx(
        &self,
        state: &AppView<'_>,
        ctx: &OverlayContext,
    ) -> Vec<OverlayContribution> {
        use super::compose::{Composable, OverlaySet};

        self.slots
            .iter()
            .filter_map(|slot| {
                if (slot.capabilities.contains(PluginCapabilities::CONTRIBUTOR)
                    || slot.capabilities.contains(PluginCapabilities::OVERLAY))
                    && let Some(mut oc) = slot.backend.contribute_overlay_with_ctx(state, ctx)
                {
                    oc.plugin_id = slot.backend.id();
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

    /// Query plugins for a cursor style override. Returns the first non-None.
    pub fn cursor_style_override(&self, state: &AppView<'_>) -> Option<crate::render::CursorStyle> {
        for slot in self.slots.iter() {
            if !slot.capabilities.contains(PluginCapabilities::CURSOR_STYLE) {
                continue;
            }
            if let Some(style) = slot.backend.cursor_style_override(state) {
                return Some(style);
            }
        }
        None
    }

    /// Collect paint hooks from all plugins.
    pub fn collect_paint_hooks(&self) -> Vec<Box<dyn PaintHook>> {
        let mut hooks = Vec::new();
        for slot in self.slots.iter() {
            if slot.capabilities.contains(PluginCapabilities::PAINT_HOOK) {
                hooks.extend(slot.backend.paint_hooks());
            }
        }
        hooks
    }

    /// Collect paint hooks for a single owner in plugin registration order.
    pub fn collect_paint_hooks_for_owner(&self, target: &PluginId) -> Vec<Box<dyn PaintHook>> {
        for slot in self.slots.iter() {
            if slot.backend.id() != *target {
                continue;
            }
            if !slot.capabilities.contains(PluginCapabilities::PAINT_HOOK) {
                return vec![];
            }
            return slot.backend.paint_hooks();
        }
        vec![]
    }

    /// Plugin IDs that currently contribute paint hooks, in registry order.
    pub fn paint_hook_owners_in_order(&self) -> Vec<PluginId> {
        self.slots
            .iter()
            .filter(|s| s.capabilities.contains(PluginCapabilities::PAINT_HOOK))
            .map(|s| s.backend.id())
            .collect()
    }

    /// Check if any plugin has TRANSFORMER capability for a given target.
    pub fn has_transform_for(&self, _target: TransformTarget) -> bool {
        self.slots
            .iter()
            .any(|s| s.capabilities.contains(PluginCapabilities::TRANSFORMER))
    }
}

/// Debug-only: detect potential transform conflicts in a chain.
///
/// Warns when:
/// - Multiple plugins declare `Replacement` scope for the same target
/// - Non-Identity transforms appear before a Replacement (they'll be absorbed)
#[cfg(debug_assertions)]
fn detect_transform_conflicts(
    chain: &[(usize, i16, PluginId)],
    slots: &[PluginSlot],
    target: &TransformTarget,
) {
    let descriptors: Vec<(PluginId, Option<super::TransformDescriptor>)> = chain
        .iter()
        .map(|(i, _, _)| {
            let slot = &slots[*i];
            (slot.backend.id(), slot.backend.transform_descriptor())
        })
        .collect();
    check_transform_conflicts(&descriptors, target);
}

/// Check for transform conflicts given a list of (plugin_id, descriptor) pairs.
///
/// Extracted as a free function for unit-testability.
#[cfg(debug_assertions)]
pub(crate) fn check_transform_conflicts(
    descriptors: &[(PluginId, Option<super::TransformDescriptor>)],
    target: &TransformTarget,
) {
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

impl Default for PluginRuntime {
    fn default() -> Self {
        Self::new()
    }
}
