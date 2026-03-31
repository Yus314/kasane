use std::any::Any;
use std::sync::Arc;

use crate::display::{DisplayMap, DisplayMapRef};
use crate::element::{Element, FlexChild, InteractiveId, PluginTag};
use crate::input::{ChordState, DropEvent, KeyEvent, KeyResponse, MouseEvent};
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
    EffectsBatch, GutterSide, IoEvent, KeyHandleResult, OverlayContext, OverlayContribution,
    PaintHook, PaneContext, PluginAuthorities, PluginBackend, PluginCapabilities, PluginDiagnostic,
    PluginId, RenderOrnamentContext, SlotId, SourcedContribution, SourcedOrnamentBatch,
    TransformContext, TransformSubject, TransformTarget,
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
    pub(crate) plugin_tag: PluginTag,
    pub(crate) capabilities: PluginCapabilities,
    pub(crate) authorities: PluginAuthorities,
    pub(crate) last_state_hash: u64,
    pub(crate) needs_recollect: bool,
    /// Framework-managed chord state for plugins using `CompiledKeyMap`.
    pub(crate) chord_state: ChordState,
    /// State hash at last `refresh_key_groups` call for caching.
    pub(crate) last_group_refresh_hash: u64,
    /// Structured capability descriptor for interference detection.
    pub(crate) descriptor: Option<super::CapabilityDescriptor>,
}

pub struct PluginRuntime {
    slots: Vec<PluginSlot>,
    any_plugin_state_changed: bool,
    next_tag: u16,
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
            next_tag: 1,
        }
    }

    pub fn plugin_count(&self) -> usize {
        self.slots.len()
    }

    /// Drain pending runtime diagnostics from all plugins.
    pub fn drain_all_diagnostics(&mut self) -> Vec<PluginDiagnostic> {
        self.slots
            .iter_mut()
            .flat_map(|slot| slot.backend.drain_diagnostics())
            .collect()
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

    pub fn register_backend(&mut self, mut plugin: Box<dyn PluginBackend>) {
        let id = plugin.id();
        let caps = plugin.capabilities();
        let authorities = plugin.authorities();
        if let Some(pos) = self.slots.iter().position(|s| s.backend.id() == id) {
            // Replace existing plugin with same ID (e.g. FS plugin overrides bundled)
            let tag = self.slots[pos].plugin_tag;
            plugin.set_plugin_tag(tag);
            let descriptor = plugin.capability_descriptor();
            let slot = &mut self.slots[pos];
            slot.backend = plugin;
            slot.capabilities = caps;
            slot.authorities = authorities;
            slot.last_state_hash = HASH_SENTINEL;
            slot.needs_recollect = true;
            slot.descriptor = descriptor;
        } else {
            let tag = PluginTag(self.next_tag);
            self.next_tag = self.next_tag.checked_add(1).expect("plugin tag overflow");
            plugin.set_plugin_tag(tag);
            let descriptor = plugin.capability_descriptor();
            // Check for potential interference with existing plugins
            if let Some(ref new_desc) = descriptor {
                for existing in &self.slots {
                    if let Some(ref existing_desc) = existing.descriptor
                        && new_desc.may_interfere(existing_desc)
                    {
                        tracing::warn!(
                            new_plugin = %id.0,
                            existing_plugin = %existing.backend.id().0,
                            "potential plugin interference detected"
                        );
                    }
                }
            }
            self.slots.push(PluginSlot {
                backend: plugin,
                plugin_tag: tag,
                capabilities: caps,
                authorities,
                last_state_hash: HASH_SENTINEL,
                needs_recollect: true,
                chord_state: ChordState::default(),
                last_group_refresh_hash: HASH_SENTINEL,
                descriptor,
            });
        }
    }

    pub fn contains_plugin(&self, id: &PluginId) -> bool {
        self.slots.iter().any(|s| s.backend.id() == *id)
    }

    /// Return the plugin tag assigned to a plugin, or `None` if not found.
    pub fn plugin_tag(&self, id: &PluginId) -> Option<PluginTag> {
        self.slots
            .iter()
            .find(|s| s.backend.id() == *id)
            .map(|s| s.plugin_tag)
    }

    /// Return all assigned plugin tags (in registration order).
    pub fn all_plugin_tags(&self) -> Vec<(PluginId, PluginTag)> {
        self.slots
            .iter()
            .map(|s| (s.backend.id(), s.plugin_tag))
            .collect()
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
    pub fn init_all_batch(&mut self, app: &AppView<'_>) -> EffectsBatch {
        let mut batch = EffectsBatch::default();
        for slot in &mut self.slots {
            batch.effects.merge(slot.backend.on_init_effects(app));
        }
        batch
    }

    /// Initialize all plugins.
    pub fn init_all(&mut self, app: &AppView<'_>) -> EffectsBatch {
        self.init_all_batch(app)
    }

    /// Notify all plugins that the active session is ready for transport-bound startup work.
    pub fn notify_active_session_ready_batch(&mut self, app: &AppView<'_>) -> EffectsBatch {
        let mut batch = EffectsBatch::default();
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
    ) -> EffectsBatch {
        for slot in &mut self.slots {
            if &slot.backend.id() == target {
                let mut batch = EffectsBatch::default();
                batch
                    .effects
                    .merge(slot.backend.on_active_session_ready_effects(app));
                return batch;
            }
        }
        EffectsBatch::default()
    }

    /// Notify all plugins about a state change and collect typed runtime effects.
    pub fn notify_state_changed_batch(
        &mut self,
        app: &AppView<'_>,
        dirty: DirtyFlags,
    ) -> EffectsBatch {
        let mut batch = EffectsBatch::default();
        for slot in &mut self.slots {
            batch
                .effects
                .merge(slot.backend.on_state_changed_effects(app, dirty));
        }
        batch
    }

    /// Run the two-phase pub/sub evaluation cycle.
    ///
    /// 1. **Collect**: Each plugin with publishers emits values onto the bus.
    /// 2. **Deliver**: Each plugin with subscribers receives published values.
    ///
    /// Call this after `notify_state_changed_batch()` and before
    /// `prepare_plugin_cache()` / view collection.
    pub fn evaluate_pubsub(&mut self, bus: &mut super::pubsub::TopicBus, app: &AppView<'_>) {
        bus.clear();

        // Phase 1: Collect publications from all plugins.
        for slot in &self.slots {
            slot.backend.collect_publications(bus, app);
        }

        // Phase 2: Deliver to subscribers.
        bus.begin_delivery();
        for slot in &mut self.slots {
            slot.backend.deliver_subscriptions(bus);
        }
        bus.end_delivery();

        // Phase 3: Record frame hashes and detect oscillation.
        bus.record_frame_hashes();
        let oscillations = bus.detect_oscillation();
        for (topic, kind) in &oscillations {
            tracing::warn!(
                topic = topic.as_str(),
                kind = ?kind,
                "pub/sub oscillation detected — topic values are cycling between frames"
            );
        }
    }

    /// Evaluate all plugin-defined extension points.
    ///
    /// Iterates over all registered extension point definitions, collects
    /// contributions from all plugins, and returns the collected results.
    pub fn evaluate_extensions(
        &self,
        input: &super::channel::ChannelValue,
        app: &AppView<'_>,
    ) -> super::extension_point::ExtensionResults {
        let mut results = super::extension_point::ExtensionResults::new();

        // Collect all defined extension point IDs.
        let definitions: Vec<_> = self
            .slots
            .iter()
            .flat_map(|slot| {
                slot.backend
                    .extension_definitions()
                    .iter()
                    .map(|def| def.id.clone())
            })
            .collect();

        // For each extension point, collect contributions from all plugins.
        for ext_id in &definitions {
            for slot in &self.slots {
                for output in slot.backend.evaluate_extension(ext_id, input, app) {
                    results.insert(ext_id.clone(), output);
                }
            }
        }

        results
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
    ) -> EffectsBatch {
        let id = plugin.id();
        // Shut down old plugin if present
        if let Some(pos) = self.slots.iter().position(|s| s.backend.id() == id) {
            self.slots[pos].backend.on_shutdown();
        }
        // register_backend handles replacement or insertion
        self.register_backend(plugin);
        // Init the new plugin
        if let Some(pos) = self.slots.iter().position(|s| s.backend.id() == id) {
            let mut batch = EffectsBatch::default();
            batch
                .effects
                .merge(self.slots[pos].backend.on_init_effects(app));
            return batch;
        }
        EffectsBatch::default()
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

    /// Resolve navigation policy for a display unit via FirstWins dispatch.
    ///
    /// Iterates plugins with `NAVIGATION_POLICY` capability. The first plugin
    /// returning `Some` wins. Falls back to `NavigationPolicy::default_for(role)`.
    pub fn resolve_navigation_policy(
        &self,
        unit: &crate::display::unit::DisplayUnit,
    ) -> crate::display::navigation::NavigationPolicy {
        for slot in self.slots.iter() {
            if !slot
                .capabilities
                .contains(PluginCapabilities::NAVIGATION_POLICY)
            {
                continue;
            }
            if let Some(policy) = slot.backend.navigation_policy(unit) {
                return policy;
            }
        }
        crate::display::navigation::NavigationPolicy::default_for(&unit.role)
    }

    /// Dispatch a navigation action via FirstWins dispatch.
    ///
    /// Iterates plugins with `NAVIGATION_ACTION` capability. The first non-Pass
    /// result wins. Falls back to `ActionResult::Pass`.
    pub fn dispatch_navigation_action(
        &mut self,
        unit: &crate::display::unit::DisplayUnit,
        action: crate::display::navigation::NavigationAction,
    ) -> crate::display::navigation::ActionResult {
        for slot in &mut self.slots {
            if !slot
                .capabilities
                .contains(PluginCapabilities::NAVIGATION_ACTION)
            {
                continue;
            }
            if let Some(result) = slot.backend.navigation_action(unit, action.clone()) {
                return result;
            }
        }
        crate::display::navigation::ActionResult::Pass
    }

    pub fn dispatch_key_middleware(
        &mut self,
        key: &KeyEvent,
        app: &AppView<'_>,
    ) -> KeyDispatchResult {
        let mut current_key = key.clone();
        for slot in &mut self.slots {
            if !slot
                .capabilities
                .contains(PluginCapabilities::INPUT_HANDLER)
            {
                continue;
            }
            // --- Key map dispatch path (Phase 2+) ---
            if slot.backend.compiled_key_map().is_some() {
                // Refresh group active flags if state changed.
                let current_hash = slot.backend.state_hash();
                if current_hash != slot.last_group_refresh_hash {
                    slot.backend.refresh_key_groups(app);
                    slot.last_group_refresh_hash = current_hash;
                }

                if let Some(result) = Self::dispatch_key_map(slot, &current_key, app) {
                    return result;
                }
                // No match in this plugin's key map — fall through to next plugin.
                continue;
            }

            // --- Legacy dispatch path ---
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

    /// Key map dispatch for a single plugin slot.
    ///
    /// Returns `Some(result)` if the key was consumed or a chord was started,
    /// `None` if this plugin doesn't handle the key.
    fn dispatch_key_map(
        slot: &mut PluginSlot,
        key: &KeyEvent,
        app: &AppView<'_>,
    ) -> Option<KeyDispatchResult> {
        use crate::input::key_map::DEFAULT_CHORD_TIMEOUT_MS;

        let plugin_id = slot.backend.id();

        // 1. If a chord is pending, try to resolve it.
        if slot.chord_state.is_pending() {
            let timeout = slot
                .backend
                .compiled_key_map()
                .map_or(DEFAULT_CHORD_TIMEOUT_MS, |m| m.chord_timeout_ms);

            if slot.chord_state.is_timed_out(timeout) {
                // Timeout: cancel chord, re-dispatch this key from scratch.
                slot.chord_state.cancel();
                return Self::dispatch_key_map(slot, key, app);
            }

            let leader = slot.chord_state.pending_leader.clone().unwrap();
            if let Some(action_id) = slot
                .backend
                .compiled_key_map()
                .and_then(|m| m.match_chord_follower(&leader, key))
            {
                // Chord matched — invoke action.
                slot.chord_state.cancel();
                let response = slot.backend.invoke_action(action_id, key, app);
                return Some(Self::key_response_to_dispatch(response, plugin_id));
            }

            // No chord match — cancel and pass through (don't consume).
            slot.chord_state.cancel();
            return None;
        }

        // 2. Not pending — check for chord leader.
        if slot
            .backend
            .compiled_key_map()
            .is_some_and(|m| m.match_chord_leader(key))
        {
            slot.chord_state.set_pending(key.clone());
            return Some(KeyDispatchResult::Consumed {
                source_plugin: plugin_id,
                commands: vec![],
            });
        }

        // 3. Try single-key binding.
        if let Some(action_id) = slot
            .backend
            .compiled_key_map()
            .and_then(|m| m.match_key(key))
        {
            let response = slot.backend.invoke_action(action_id, key, app);
            return Some(Self::key_response_to_dispatch(response, plugin_id));
        }

        // 4. No match at all — passthrough.
        None
    }

    fn key_response_to_dispatch(response: KeyResponse, plugin_id: PluginId) -> KeyDispatchResult {
        match response {
            KeyResponse::Pass => KeyDispatchResult::Passthrough(KeyEvent {
                key: crate::input::Key::Escape,
                modifiers: crate::input::Modifiers::empty(),
            }),
            KeyResponse::Consume => KeyDispatchResult::Consumed {
                source_plugin: plugin_id,
                commands: vec![],
            },
            KeyResponse::ConsumeRedraw => KeyDispatchResult::Consumed {
                source_plugin: plugin_id,
                commands: vec![Command::RequestRedraw(DirtyFlags::ALL)],
            },
            KeyResponse::ConsumeWith(commands) => KeyDispatchResult::Consumed {
                source_plugin: plugin_id,
                commands,
            },
        }
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
    ) -> EffectsBatch {
        crate::perf::perf_span!("deliver_io_event");
        for slot in &mut self.slots {
            if &slot.backend.id() == target {
                if !slot.capabilities.contains(PluginCapabilities::IO_HANDLER) {
                    return EffectsBatch::default();
                }
                let mut batch = EffectsBatch::default();
                batch
                    .effects
                    .merge(slot.backend.on_io_event_effects(event, app));
                return batch;
            }
        }
        EffectsBatch::default()
    }

    /// Deliver a message to a specific plugin by ID.
    pub fn deliver_message_batch(
        &mut self,
        target: &PluginId,
        payload: Box<dyn Any>,
        app: &AppView<'_>,
    ) -> EffectsBatch {
        let mut payload = payload;
        for slot in &mut self.slots {
            if &slot.backend.id() == target {
                let mut batch = EffectsBatch::default();
                batch
                    .effects
                    .merge(slot.backend.update_effects(payload.as_mut(), app));
                return batch;
            }
        }
        EffectsBatch::default()
    }

    /// Start a named process task on a specific plugin.
    ///
    /// Returns the spawn commands (typically a single `SpawnProcess`) for the
    /// event loop to dispatch. Returns an empty vec if the plugin or task is
    /// not found.
    pub fn start_process_task(&mut self, target: &PluginId, name: &str) -> Vec<Command> {
        for slot in &mut self.slots {
            if &slot.backend.id() == target {
                return slot.backend.start_process_task(name);
            }
        }
        vec![]
    }

    /// Register a [`Plugin`] by wrapping it in a [`PluginBridge`].
    ///
    /// The bridge builds a [`HandlerTable`] from `P::register()`, then dispatches
    /// all `PluginBackend` methods through the table's erased handlers.
    pub fn register<P: Plugin>(&mut self, plugin: P) {
        let bridge = PluginBridge::new(plugin);
        self.register_backend(Box::new(bridge));
    }

    /// Broadcast key observation to all plugins with INPUT_HANDLER capability.
    pub fn observe_key_all(&mut self, key: &KeyEvent, app: &AppView<'_>) {
        for slot in &mut self.slots {
            if !slot
                .capabilities
                .contains(PluginCapabilities::INPUT_HANDLER)
            {
                continue;
            }
            slot.backend.observe_key(key, app);
        }
    }

    /// Broadcast mouse observation to all plugins with INPUT_HANDLER capability.
    pub fn observe_mouse_all(&mut self, event: &MouseEvent, app: &AppView<'_>) {
        for slot in &mut self.slots {
            if !slot
                .capabilities
                .contains(PluginCapabilities::INPUT_HANDLER)
            {
                continue;
            }
            slot.backend.observe_mouse(event, app);
        }
    }

    /// Owner-based mouse handler dispatch.
    ///
    /// If the `InteractiveId` has a plugin owner tag, dispatches directly to the
    /// owning plugin (O(1) lookup). Falls back to first-wins iteration for
    /// framework-owned or unassigned IDs.
    pub fn dispatch_mouse_handler(
        &mut self,
        event: &MouseEvent,
        id: InteractiveId,
        app: &AppView<'_>,
    ) -> MouseHandleResult {
        if id.owner != PluginTag::FRAMEWORK && id.owner != PluginTag::UNASSIGNED {
            // Direct dispatch to owning plugin
            if let Some(slot) = self.slots.iter_mut().find(|s| s.plugin_tag == id.owner)
                && let Some(commands) = slot.backend.handle_mouse(event, id, app)
            {
                return MouseHandleResult::Handled {
                    source_plugin: slot.backend.id(),
                    commands,
                };
            }
            return MouseHandleResult::NotHandled;
        }
        // Legacy fallback for framework/unassigned IDs
        for slot in &mut self.slots {
            if !slot
                .capabilities
                .contains(PluginCapabilities::INPUT_HANDLER)
            {
                continue;
            }
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

    /// Broadcast drop observation to all plugins with DROP_HANDLER capability.
    pub fn observe_drop_all(&mut self, event: &DropEvent, app: &AppView<'_>) {
        for slot in &mut self.slots {
            if !slot.capabilities.contains(PluginCapabilities::DROP_HANDLER) {
                continue;
            }
            slot.backend.observe_drop(event, app);
        }
    }

    /// Owner-based drop handler dispatch.
    pub fn dispatch_drop_handler(
        &mut self,
        event: &DropEvent,
        id: InteractiveId,
        app: &AppView<'_>,
    ) -> MouseHandleResult {
        if id.owner != PluginTag::FRAMEWORK && id.owner != PluginTag::UNASSIGNED {
            if let Some(slot) = self.slots.iter_mut().find(|s| s.plugin_tag == id.owner)
                && let Some(commands) = slot.backend.handle_drop(event, id, app)
            {
                return MouseHandleResult::Handled {
                    source_plugin: slot.backend.id(),
                    commands,
                };
            }
            return MouseHandleResult::NotHandled;
        }
        for slot in &mut self.slots {
            if !slot.capabilities.contains(PluginCapabilities::DROP_HANDLER) {
                continue;
            }
            if let Some(commands) = slot.backend.handle_drop(event, id, app) {
                return MouseHandleResult::Handled {
                    source_plugin: slot.backend.id(),
                    commands,
                };
            }
        }
        MouseHandleResult::NotHandled
    }
}

impl PluginEffects for PluginRuntime {
    fn notify_state_changed(&mut self, app: &AppView<'_>, flags: DirtyFlags) -> EffectsBatch {
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

    fn observe_drop_all(&mut self, event: &DropEvent, app: &AppView<'_>) {
        PluginRuntime::observe_drop_all(self, event, app)
    }

    fn dispatch_drop_handler(
        &mut self,
        event: &DropEvent,
        id: InteractiveId,
        app: &AppView<'_>,
    ) -> MouseHandleResult {
        PluginRuntime::dispatch_drop_handler(self, event, id, app)
    }

    fn handle_default_scroll(
        &mut self,
        candidate: DefaultScrollCandidate,
        app: &AppView<'_>,
    ) -> Option<ScrollPolicyResult> {
        PluginRuntime::handle_default_scroll(self, candidate, app).map(|(_, result)| result)
    }

    fn resolve_navigation_policy(
        &self,
        unit: &crate::display::unit::DisplayUnit,
    ) -> crate::display::navigation::NavigationPolicy {
        PluginRuntime::resolve_navigation_policy(self, unit)
    }

    fn dispatch_navigation_action(
        &mut self,
        unit: &crate::display::unit::DisplayUnit,
        action: crate::display::navigation::NavigationAction,
    ) -> crate::display::navigation::ActionResult {
        PluginRuntime::dispatch_navigation_action(self, unit, action)
    }
}

/// Per-plugin contribution cache for incremental recollection (Phase 5).
///
/// Stores the last-known contribution from each plugin for each slot.
/// When a plugin's `needs_recollect` is false, its cached contribution is
/// reused instead of calling `contribute_to()` again.
#[derive(Default)]
pub struct ContributionCache {
    contributions: std::collections::HashMap<(PluginId, SlotId), Option<SourcedContribution>>,
}

impl ContributionCache {
    /// Remove all cached entries for a plugin (e.g., after unloading).
    pub fn remove_plugin(&mut self, plugin_id: &PluginId) {
        self.contributions.retain(|(id, _), _| id != plugin_id);
    }
}

impl<'a> PluginView<'a> {
    /// Returns true if any plugin needs its view contributions re-collected.
    pub fn any_needs_recollect(&self) -> bool {
        self.slots.iter().any(|s| s.needs_recollect)
    }

    /// Returns true if any plugin with the given capability needs recollection.
    fn any_capability_needs_recollect(&self, cap: PluginCapabilities) -> bool {
        self.slots
            .iter()
            .any(|s| s.needs_recollect && s.capabilities.contains(cap))
    }

    /// Check if any CONTRIBUTOR plugin needs recollection.
    pub fn any_contributor_needs_recollect(&self) -> bool {
        self.any_capability_needs_recollect(PluginCapabilities::CONTRIBUTOR)
    }

    /// Check if any ANNOTATOR plugin needs recollection.
    pub fn any_annotator_needs_recollect(&self) -> bool {
        self.any_capability_needs_recollect(PluginCapabilities::ANNOTATOR)
    }

    /// Check if any OVERLAY plugin needs recollection.
    pub fn any_overlay_needs_recollect(&self) -> bool {
        self.slots.iter().any(|s| {
            s.needs_recollect
                && (s.capabilities.contains(PluginCapabilities::OVERLAY)
                    || s.capabilities.contains(PluginCapabilities::CONTRIBUTOR))
        })
    }

    /// Check if any DISPLAY_TRANSFORM plugin needs recollection.
    pub fn any_display_transform_needs_recollect(&self) -> bool {
        self.any_capability_needs_recollect(PluginCapabilities::DISPLAY_TRANSFORM)
    }

    /// Check if any registered plugin has the given capability.
    pub(crate) fn has_capability(&self, cap: PluginCapabilities) -> bool {
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
        use super::compose::{Composable, ContributionSet};

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
    ) -> Option<super::element_patch::ElementPatch> {
        use super::element_patch::ElementPatch;

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
        use super::element_patch::ElementPatch;

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

        // Partition annotators by decomposition support
        let annotator_slots: Vec<&PluginSlot> = self
            .slots
            .iter()
            .filter(|s| s.capabilities.contains(PluginCapabilities::ANNOTATOR))
            .collect();

        for line in 0..line_count {
            let mut left_parts: Vec<(i16, PluginId, Element)> = Vec::new();
            let mut right_parts: Vec<(i16, PluginId, Element)> = Vec::new();
            let mut bg_layers: Vec<(BackgroundLayer, PluginId)> = Vec::new();
            let mut vt_parts: Vec<(i16, PluginId, Vec<crate::protocol::Atom>)> = Vec::new();

            for slot in &annotator_slots {
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
                let separator = crate::protocol::Atom {
                    face: crate::protocol::Face {
                        attributes: crate::protocol::Attributes::DIM,
                        ..crate::protocol::Face::default()
                    },
                    contents: "  ".into(),
                };
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
        state
            .as_app_state()
            .fold_toggle_state
            .filter_directives(&mut directives);
        if directives.is_empty() {
            return Arc::new(DisplayMap::identity(line_count));
        }
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
    pub fn cursor_style_override(
        &self,
        state: &AppView<'_>,
    ) -> Option<crate::render::CursorStyleHint> {
        for slot in self.slots.iter() {
            if !slot.capabilities.contains(PluginCapabilities::CURSOR_STYLE) {
                continue;
            }
            if let Some(hint) = slot.backend.cursor_style_override(state) {
                return Some(hint);
            }
        }
        None
    }

    /// Collect cell decorations from all participating plugins, sorted by priority.
    pub fn collect_cell_decorations(&self, state: &AppView<'_>) -> Vec<super::CellDecoration> {
        let mut all = Vec::new();
        for slot in self.slots.iter() {
            if !slot
                .capabilities
                .contains(PluginCapabilities::CELL_DECORATION)
            {
                continue;
            }
            all.extend(slot.backend.decorate_cells(state));
        }
        all.sort_by_key(|d| d.priority);
        all
    }

    /// Collect all cell-level emphasis decorations, combining legacy cell decorations
    /// with new render ornament emphasis proposals.
    pub fn collect_emphasis_decorations(
        &self,
        state: &AppView<'_>,
        ctx: &RenderOrnamentContext,
    ) -> Vec<super::CellDecoration> {
        let mut all = self.collect_cell_decorations(state);
        for sourced in self.collect_render_ornaments(state, ctx) {
            all.extend(
                sourced
                    .batch
                    .emphasis
                    .into_iter()
                    .map(|orn| super::CellDecoration {
                        target: orn.target,
                        face: orn.face,
                        merge: orn.merge,
                        priority: orn.priority,
                    }),
            );
        }
        all.sort_by_key(|d| d.priority);
        all
    }

    /// Collect backend-independent physical ornament proposals from all participating plugins.
    pub fn collect_render_ornaments(
        &self,
        state: &AppView<'_>,
        ctx: &RenderOrnamentContext,
    ) -> Vec<SourcedOrnamentBatch> {
        let mut all = Vec::new();
        for slot in self.slots.iter() {
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
            all.push(SourcedOrnamentBatch {
                plugin_id: slot.backend.id(),
                batch,
            });
        }
        all
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
    entries: &[(usize, PluginId, Option<super::ElementPatch>)],
    slots: &[PluginSlot],
    target: &TransformTarget,
) {
    use super::context::TransformScope;

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
