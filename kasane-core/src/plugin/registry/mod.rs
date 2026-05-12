mod collection;
mod input_dispatch;

#[cfg(debug_assertions)]
#[allow(unused_imports)]
pub(crate) use collection::check_transform_conflicts;

use std::any::Any;
use std::cell::RefCell;

use crate::display::{
    CategorizedDirectives, DirectiveSet, DirectiveStabilityMonitor, partition_by_category,
};
use crate::element::{Element, InteractiveId, PluginTag};
use crate::input::{ChordState, DropEvent, KeyEvent, MouseEvent};
use crate::scroll::{DefaultScrollCandidate, ScrollPolicyResult};
use crate::state::DirtyFlags;
use crate::workspace::Placement;
use crate::workspace::WorkspaceQuery;

use super::AppView;
use super::bridge::PluginBridge;
use super::effects::{MouseHandleResult, PluginEffects, TextInputHandleResult};
use super::inline_box::{InlineBoxStack, MAX_INLINE_BOX_DEPTH};
use super::state::Plugin;
use super::traits::MousePreDispatchResult;
use super::{
    Command, EffectsBatch, IoEvent, PluginAuthorities, PluginCapabilities, PluginDiagnostic,
    PluginId, SlotId, SourcedContribution,
};

/// Pre-decomposed result of a single `collect_ornaments` pass.
///
/// Avoids redundant per-frame plugin calls by collecting ornaments once
/// and splitting the result into emphasis, cursor, and surfaces.
pub struct CollectedOrnaments {
    /// Emphasis decorations from all plugins, sorted by priority.
    pub emphasis: Vec<super::CellDecoration>,
    /// Winning cursor style hint (modality.rank(), priority winner-takes-all).
    pub cursor_style: Option<crate::render::CursorStyleHint>,
    /// Winning cursor position override (modality.rank(), priority winner-takes-all).
    pub cursor_position: Option<(u16, u16, crate::render::CursorStyle, crate::protocol::Color)>,
    /// Accumulated cursor effects from all plugins.
    pub cursor_effects: Vec<super::CursorEffectOrn>,
    /// Surface ornaments from all plugins.
    pub surfaces: Vec<super::SurfaceOrn>,
}

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
    pub(crate) backend: Box<PluginBridge>,
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
    directive_stability: RefCell<DirectiveStabilityMonitor>,
    variable_store: super::variable_store::PluginVariableStore,
    suppressed_builtins: std::collections::HashSet<super::BuiltinTarget>,
    /// Plugin IDs that were unloaded since the last drain. Used by the salsa
    /// sync path to clean up contribution caches.
    unloaded_ids: Vec<PluginId>,
    /// Pub/sub topic bus owned by the runtime so the
    /// `PluginEffects::evaluate_pubsub` trait impl can drive it without
    /// the caller having to thread one in. Persists frame-to-frame so
    /// `TopicBus::detect_oscillation` history stays valid.
    topic_bus: super::pubsub::TopicBus,
}

/// Immutable view over plugins for the render phase.
///
/// Borrows the plugin list and capabilities from [`PluginRuntime`] without
/// requiring `&mut` access. All read-only view queries (contribute, transform,
/// annotate, overlay, display map, etc.) live here.
pub struct PluginView<'a> {
    slots: &'a [PluginSlot],
    directive_stability: &'a RefCell<DirectiveStabilityMonitor>,
    suppressed_builtins: &'a std::collections::HashSet<super::BuiltinTarget>,
    /// Lazy per-plugin cache for unified display results.
    ///
    /// For plugins with `has_unified_display() == true`, the first collection
    /// method to access a plugin populates this cache with the partitioned
    /// result of `unified_display()`. Subsequent collection methods (spatial,
    /// annotation, content annotation) read from the cache instead of calling
    /// the plugin again.
    pub(crate) unified_cache: RefCell<Vec<Option<CategorizedDirectives>>>,
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
            directive_stability: RefCell::new(DirectiveStabilityMonitor::new()),
            variable_store: super::variable_store::PluginVariableStore::default(),
            suppressed_builtins: std::collections::HashSet::new(),
            unloaded_ids: Vec::new(),
            topic_bus: super::pubsub::TopicBus::new(),
        }
    }

    pub fn plugin_count(&self) -> usize {
        self.slots.len()
    }

    /// Test-only: capabilities of each registered plugin slot.
    ///
    /// Introduced for the widget test shim (Phase β-3.3a) which aggregates
    /// per-slot caps the way the legacy `WidgetBackend` did internally.
    #[doc(hidden)]
    pub fn all_slot_capabilities_for_test(&self) -> Vec<PluginCapabilities> {
        self.slots.iter().map(|s| s.capabilities).collect()
    }

    /// Test-only: capability descriptors per slot.
    #[doc(hidden)]
    pub fn all_slot_descriptors_for_test(&self) -> Vec<Option<super::CapabilityDescriptor>> {
        self.slots.iter().map(|s| s.descriptor.clone()).collect()
    }

    /// Test-only: view-deps per slot.
    #[doc(hidden)]
    pub fn all_slot_view_deps_for_test(&self) -> Vec<DirtyFlags> {
        self.slots
            .iter()
            .map(|slot| slot.backend.view_deps())
            .collect()
    }

    /// Test-only: first plugin that produces a `BackgroundLayer` for `line`.
    ///
    /// Mirrors the legacy `WidgetBackend::decorate_background` shape (single
    /// `Option`, first-wins) for the widget test shim.
    #[doc(hidden)]
    pub fn first_decorate_background_for_test(
        &self,
        line: usize,
        state: &AppView<'_>,
        ctx: &super::AnnotateContext,
    ) -> Option<super::BackgroundLayer> {
        for slot in &self.slots {
            let result = slot.backend.decorate_background(line, state, ctx);
            if result.is_some() {
                return result;
            }
        }
        None
    }

    /// Test-only: first plugin that produces a gutter `(priority, Element)` for
    /// `(side, line)`.
    #[doc(hidden)]
    pub fn first_decorate_gutter_for_test(
        &self,
        side: crate::plugin::GutterSide,
        line: usize,
        state: &AppView<'_>,
        ctx: &super::AnnotateContext,
    ) -> Option<(i16, crate::element::Element)> {
        for slot in &self.slots {
            let result = slot.backend.decorate_gutter(side, line, state, ctx);
            if result.is_some() {
                return result;
            }
        }
        None
    }

    /// Test-only: first plugin that produces a non-`Identity` `ElementPatch`
    /// for `target`.
    #[doc(hidden)]
    pub fn first_transform_patch_for_test(
        &self,
        target: &super::TransformTarget,
        state: &AppView<'_>,
        ctx: &super::TransformContext,
    ) -> Option<super::ElementPatch> {
        for slot in &self.slots {
            let patch = slot.backend.transform_patch(target, state, ctx);
            if let Some(p) = patch
                && !matches!(p, super::ElementPatch::Identity)
            {
                return Some(p);
            }
        }
        None
    }

    /// Sync lens registrations from this runtime to `lens_registry`
    /// (Composable Lenses auto-wired lifecycle).
    ///
    /// Two-phase:
    /// 1. **Drop** lens entries owned by plugin ids no longer in
    ///    this runtime. A plugin that unloaded between calls has
    ///    its lenses purged so they don't outlive the backing
    ///    runtime.
    /// 2. **Register** by calling `register_lenses` on every
    ///    current backend. Native plugins return 0 (no-op
    ///    default); WASM plugins query `declare-lenses` and
    ///    register one `WasmLensAdapter` per declaration.
    ///
    /// Idempotent: re-registering an existing lens replaces the
    /// previous adapter (same `LensId`). Cache entries for the
    /// previous adapter are dropped on re-registration per
    /// `LensRegistry::register`.
    ///
    /// Returns the number of lenses registered in step 2 (the
    /// count from step 1 is implicit; call
    /// `LensRegistry::registered_ids().count()` before / after to
    /// observe the diff).
    pub fn sync_lenses(&self, lens_registry: &mut crate::lens::LensRegistry) -> usize {
        let live: std::collections::HashSet<super::PluginId> =
            self.slots.iter().map(|s| s.backend.id()).collect();
        let stale_plugins: Vec<super::PluginId> = lens_registry
            .registered_ids()
            .map(|id| id.plugin.clone())
            .filter(|p| !live.contains(p))
            .collect();
        for plugin in stale_plugins {
            lens_registry.unregister_by_plugin(&plugin);
        }
        let mut registered = 0usize;
        for slot in &self.slots {
            registered += slot.backend.register_lenses(lens_registry);
        }
        registered
    }

    /// Access the plugin variable store.
    pub fn variable_store(&self) -> &super::variable_store::PluginVariableStore {
        &self.variable_store
    }

    /// Mutably access the plugin variable store.
    pub fn variable_store_mut(&mut self) -> &mut super::variable_store::PluginVariableStore {
        &mut self.variable_store
    }

    /// Drain pending runtime diagnostics from all plugins.
    pub fn drain_all_diagnostics(&mut self) -> Vec<PluginDiagnostic> {
        self.slots
            .iter_mut()
            .flat_map(|slot| slot.backend.drain_diagnostics())
            .collect()
    }

    /// Check if a built-in target has been suppressed by any registered plugin.
    pub fn is_builtin_suppressed(&self, target: super::BuiltinTarget) -> bool {
        self.suppressed_builtins.contains(&target)
    }

    /// Return the full set of suppressed built-in targets.
    pub fn suppressed_builtins(&self) -> &std::collections::HashSet<super::BuiltinTarget> {
        &self.suppressed_builtins
    }

    /// Borrow an immutable view for the render phase.
    pub fn view(&self) -> PluginView<'_> {
        let slot_count = self.slots.len();
        PluginView {
            slots: &self.slots,
            directive_stability: &self.directive_stability,
            suppressed_builtins: &self.suppressed_builtins,
            unified_cache: RefCell::new(vec![None; slot_count]),
        }
    }

    /// Returns true if any plugin's state_hash changed during the last
    /// `prepare_plugin_cache()` call.
    pub fn any_plugin_state_changed(&self) -> bool {
        self.any_plugin_state_changed
    }

    pub fn register_backend(&mut self, mut plugin: Box<PluginBridge>) {
        let id = plugin.id();
        let caps = plugin.capabilities();
        let authorities = plugin.authorities();
        let new_suppressions = plugin.suppressed_builtins().clone();
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
        self.suppressed_builtins.extend(new_suppressions);
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
            self.variable_store.clear_for_plugin(id);
            self.unloaded_ids.push(id.clone());
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
            // Reap any variables this plugin had exposed via
            // Command::ExposeVariable. Without this, the entries would
            // outlive the plugin instance and re-loading the same plugin
            // would briefly see its old values.
            self.variable_store.clear_for_plugin(id);
            self.unloaded_ids.push(id.clone());
            true
        } else {
            false
        }
    }

    /// Drain the list of recently-unloaded plugin IDs.
    ///
    /// Called by the salsa sync path to clean up contribution caches
    /// for plugins that have been removed.
    pub fn drain_unloaded_ids(&mut self) -> Vec<PluginId> {
        std::mem::take(&mut self.unloaded_ids)
    }

    /// Check whether any plugin's internal state changed since the last call,
    /// and compute per-plugin `needs_recollect` based on state hash changes
    /// and the intersection of `dirty` flags with each plugin's `view_deps()`.
    ///
    /// Native (PluginBridge-backed) plugins read `state_hash` and `view_deps`
    /// via direct field access; External implementers fall through the
    /// vtable. Called every frame in the salsa sync path.
    pub fn prepare_plugin_cache(&mut self, dirty: DirtyFlags) {
        self.any_plugin_state_changed = false;
        for slot in &mut self.slots {
            let (current_hash, view_deps) = (slot.backend.state_hash(), slot.backend.view_deps());
            let hash_changed = current_hash != slot.last_state_hash;
            if hash_changed {
                slot.last_state_hash = current_hash;
                self.any_plugin_state_changed = true;
            }
            slot.needs_recollect = hash_changed || dirty.intersects(view_deps);
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
            let id = slot.backend.id();
            let effects = slot.backend.on_init_effects(app);
            batch.push(id, effects);
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
            let id = slot.backend.id();
            let effects = slot.backend.on_active_session_ready_effects(app);
            batch.push(id, effects);
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
                let effects = slot.backend.on_active_session_ready_effects(app);
                return EffectsBatch::single(target.clone(), effects);
            }
        }
        EffectsBatch::default()
    }

    /// Notify all plugins about a state change and collect typed runtime effects.
    ///
    /// Phase β-1.5: native (PluginBridge-backed) plugins dispatch via a
    /// concrete method call on `&mut PluginBridge`, bypassing the vtable.
    /// External implementers (WasmPlugin etc.) keep the existing
    /// `Box<dyn PluginBackend>` vtable path.
    pub fn notify_state_changed_batch(
        &mut self,
        app: &AppView<'_>,
        dirty: DirtyFlags,
    ) -> EffectsBatch {
        let mut batch = EffectsBatch::default();
        for slot in &mut self.slots {
            let id = slot.backend.id();
            let effects = slot.backend.on_state_changed_effects(app, dirty);
            batch.push(id, effects);
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
    pub fn evaluate_pubsub(
        &mut self,
        bus: &mut super::pubsub::TopicBus,
        app: &AppView<'_>,
    ) -> EffectsBatch {
        bus.clear();

        // Phase 1: Collect publications from all plugins.
        for slot in &self.slots {
            slot.backend.collect_publications(bus, app);
        }

        // Phase 2: Deliver to subscribers and collect on_subscription effects.
        // Per-topic batch handlers can return Effects that flow back through
        // the same EffectsBatch shape as notify_state_changed, so commands
        // and scroll plans land in the correct frame's UpdateResult.
        bus.begin_delivery();
        let mut batch = EffectsBatch::default();
        for slot in &mut self.slots {
            let plugin_id = slot.backend.id();
            let effects = slot.backend.deliver_subscriptions(bus, app);
            batch.push(plugin_id, effects);
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

    /// Collect workspace save data from all plugins.
    ///
    /// Returns a map of plugin ID → JSON data for plugins that have
    /// workspace-persistent state. The caller should embed this in
    /// `SavedLayout::plugin_data` before writing to disk.
    pub fn collect_workspace_data(&self) -> std::collections::HashMap<String, serde_json::Value> {
        let mut data = std::collections::HashMap::new();
        for slot in &self.slots {
            if let Some(value) = slot.backend.workspace_save() {
                data.insert(slot.backend.id().0.clone(), value);
            }
        }
        data
    }

    /// Distribute workspace restore data to plugins.
    ///
    /// For each entry in `plugin_data`, the corresponding plugin's
    /// `workspace_restore()` method is called with the saved data.
    pub fn distribute_workspace_data(
        &mut self,
        plugin_data: &std::collections::HashMap<String, serde_json::Value>,
    ) {
        for slot in &mut self.slots {
            let id = slot.backend.id().0.clone();
            if let Some(data) = plugin_data.get(&id) {
                slot.backend.workspace_restore(data);
            }
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
        plugin: Box<PluginBridge>,
        app: &AppView<'_>,
    ) -> EffectsBatch {
        let id = plugin.id();
        // Persist state from old plugin before shutdown
        let persisted = if let Some(pos) = self.slots.iter().position(|s| s.backend.id() == id) {
            let state_data = self.slots[pos].backend.persist_state();
            self.slots[pos].backend.on_shutdown();
            state_data
        } else {
            None
        };
        // register_backend handles replacement or insertion
        self.register_backend(plugin);
        // Init the new plugin, then restore persisted state
        if let Some(pos) = self.slots.iter().position(|s| s.backend.id() == id) {
            if let Some(data) = persisted
                && !self.slots[pos].backend.restore_state(&data)
            {
                tracing::debug!(
                    plugin_id = %id.0,
                    "state restore failed (schema change?), starting fresh"
                );
            }
            let effects = self.slots[pos].backend.on_init_effects(app);
            return EffectsBatch::single(id, effects);
        }
        EffectsBatch::default()
    }

    pub fn plugins_mut(&mut self) -> impl Iterator<Item = &mut PluginBridge> {
        self.slots.iter_mut().map(|s| &mut *s.backend)
    }

    /// Get a mutable reference to a plugin backend by its ID.
    pub fn backend_mut_by_id(&mut self, id: &PluginId) -> Option<&mut PluginBridge> {
        self.slots
            .iter_mut()
            .find(|s| s.backend.id() == *id)
            .map(|s| &mut *s.backend)
    }

    /// Refresh a slot's cached capabilities and descriptor after in-place mutation.
    pub fn refresh_slot_metadata(&mut self, id: &PluginId) {
        if let Some(slot) = self.slots.iter_mut().find(|s| s.backend.id() == *id) {
            slot.capabilities = slot.backend.capabilities();
            slot.descriptor = slot.backend.capability_descriptor();
            slot.needs_recollect = true;
        }
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
                let effects = slot.backend.on_io_event_effects(event, app);
                return EffectsBatch::single(target.clone(), effects);
            }
        }
        EffectsBatch::default()
    }

    /// Deliver a Kakoune-command-error event (ADR-042) to the plugin
    /// identified by `target` (the plugin-id parsed from the `info_show`
    /// marker payload).
    pub fn deliver_command_error_batch(
        &mut self,
        target: &PluginId,
        error: &super::error_attribution::PluginErrorEvent,
        app: &AppView<'_>,
    ) -> EffectsBatch {
        crate::perf::perf_span!("deliver_command_error");
        for slot in &mut self.slots {
            if &slot.backend.id() == target {
                let effects = slot.backend.on_command_error_effects(error, app);
                return EffectsBatch::single(target.clone(), effects);
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
                let effects = slot.backend.update_effects(payload.as_mut(), app);
                return EffectsBatch::single(target.clone(), effects);
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
}

impl PluginEffects for PluginRuntime {
    fn notify_state_changed(&mut self, app: &AppView<'_>, flags: DirtyFlags) -> EffectsBatch {
        self.notify_state_changed_batch(app, flags)
    }

    fn dispatch_key_pre_dispatch(
        &mut self,
        key: &KeyEvent,
        app: &AppView<'_>,
    ) -> super::KeyPreDispatchResult {
        PluginRuntime::dispatch_key_pre_dispatch(self, key, app)
    }

    fn dispatch_text_input_pre_dispatch(
        &mut self,
        text: &str,
        app: &AppView<'_>,
    ) -> super::TextInputPreDispatchResult {
        PluginRuntime::dispatch_text_input_pre_dispatch(self, text, app)
    }

    fn observe_key_all(&mut self, key: &KeyEvent, app: &AppView<'_>) {
        PluginRuntime::observe_key_all(self, key, app)
    }

    fn dispatch_key_middleware(&mut self, key: &KeyEvent, app: &AppView<'_>) -> KeyDispatchResult {
        PluginRuntime::dispatch_key_middleware(self, key, app)
    }

    fn observe_text_input_all(&mut self, text: &str, app: &AppView<'_>) {
        PluginRuntime::observe_text_input_all(self, text, app)
    }

    fn dispatch_text_input_handler(
        &mut self,
        text: &str,
        app: &AppView<'_>,
    ) -> TextInputHandleResult {
        PluginRuntime::dispatch_text_input_handler(self, text, app)
    }

    fn dispatch_mouse_pre_dispatch(
        &mut self,
        event: &MouseEvent,
        app: &AppView<'_>,
    ) -> MousePreDispatchResult {
        PluginRuntime::dispatch_mouse_pre_dispatch(self, event, app)
    }

    fn dispatch_mouse_fallback(
        &mut self,
        event: &MouseEvent,
        scroll_amount: i32,
        app: &AppView<'_>,
    ) -> Option<Vec<Command>> {
        PluginRuntime::dispatch_mouse_fallback(self, event, scroll_amount, app)
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

    fn dispatch_command_error(
        &mut self,
        target: &PluginId,
        error: &super::error_attribution::PluginErrorEvent,
        app: &AppView<'_>,
    ) -> EffectsBatch {
        self.deliver_command_error_batch(target, error, app)
    }

    fn evaluate_pubsub(&mut self, app: &AppView<'_>) -> EffectsBatch {
        // Temporarily move the bus out so the inner method can borrow
        // `self.slots` mutably without aliasing `self.topic_bus`. The
        // bus restores on the next line; oscillation history persists
        // because it lives inside the moved value.
        let mut bus = std::mem::take(&mut self.topic_bus);
        let batch = PluginRuntime::evaluate_pubsub(self, &mut bus, app);
        self.topic_bus = bus;
        batch
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
    /// Dispatch `paint_inline_box(box_id)` to the plugin owning the box.
    ///
    /// Returns the `Element` the plugin wants rendered inside the inline
    /// box, or `None` if the plugin has no handler registered or returned
    /// `None`. Used by the rendering pipeline to fill `BufferParagraph`'s
    /// `inline_box_paint_commands` after walk-paint, per ADR-031 Phase 10
    /// Step 2 (Step A.2b). Returns `None` quickly for plugins that lack
    /// the `INLINE_BOX_PAINTER` capability bit.
    pub fn paint_inline_box(
        &self,
        owner: &super::PluginId,
        box_id: u64,
        app: &AppView<'_>,
    ) -> Option<Element> {
        let slot = self.slots.iter().find(|s| s.backend.id() == *owner)?;
        if !slot
            .capabilities
            .contains(super::PluginCapabilities::INLINE_BOX_PAINTER)
        {
            return None;
        }
        // Reentrancy / recursion / cycle guard. A plugin's
        // `paint_inline_box(box_id_outer)` may legitimately produce an
        // `Element` tree that contains *another* inline-box, which the
        // host then resolves via a fresh `paint_inline_box(box_id_inner)`
        // call. Without bounds, malicious or buggy plugins can blow the
        // stack via self-cycles (`box_id_inner == box_id_outer`) or
        // mutual cycles (plugin A → box_B → plugin B → box_A → …). The
        // host enforces both bounds; plugins are not trusted.
        InlineBoxStack::with(|stack| {
            if stack.depth() >= MAX_INLINE_BOX_DEPTH {
                stack.log_overflow_once(owner, box_id);
                return None;
            }
            if stack.contains(box_id) {
                stack.log_cycle_once(owner, box_id);
                return None;
            }
            stack.push(box_id);
            let result = slot.backend.paint_inline_box(box_id, app);
            stack.pop();
            result
        })
    }

    /// Ensure the unified display cache is populated for the given plugin slot.
    ///
    /// Returns `true` if the plugin uses unified display (cache is valid).
    /// Returns `false` for legacy plugins.
    fn ensure_unified_cached(&self, idx: usize, state: &AppView<'_>) -> bool {
        let slot = &self.slots[idx];
        if !slot.backend.has_unified_display() {
            return false;
        }

        {
            let cache = self.unified_cache.borrow();
            if cache[idx].is_some() {
                return true;
            }
        }

        let directives = slot.backend.unified_display(state);
        let plugin_id = slot.backend.id();
        let priority = slot.backend.display_directive_priority();

        let mut set = DirectiveSet::default();
        for d in directives {
            set.push(d, priority, plugin_id.clone());
        }

        let categorized = partition_by_category(&set);
        self.unified_cache.borrow_mut()[idx] = Some(categorized);
        true
    }

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

    /// Check if a built-in target has been suppressed by any registered plugin.
    pub fn is_builtin_suppressed(&self, target: super::BuiltinTarget) -> bool {
        self.suppressed_builtins.contains(&target)
    }
}

impl Default for PluginRuntime {
    fn default() -> Self {
        Self::new()
    }
}
