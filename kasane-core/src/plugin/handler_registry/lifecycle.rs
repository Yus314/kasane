//! Lifecycle / state-change / I/O / workspace / process-task / update handlers.

use std::any::Any;

use crate::state::DirtyFlags;
use crate::workspace::WorkspaceQuery;

use super::super::error_attribution::PluginErrorEvent;
use super::super::process_task::{ProcessTaskEntry, ProcessTaskResult, ProcessTaskSpec};
use super::super::{
    AppView, ChannelValue, Effects, IoEvent, KakouneSideEffects, PluginState, ProcessCapableEffects,
};

use super::{HandlerRegistry, Transparency};

impl<S: PluginState + Clone + 'static> HandlerRegistry<S> {
    pub fn declare_interests(&mut self, flags: DirtyFlags) {
        self.table.interests = flags;
    }

    /// Suppress a built-in plugin feature.
    ///
    /// When called, the corresponding built-in plugin will skip its default
    /// behavior, allowing this plugin to provide a full replacement.
    pub fn suppress_builtin(&mut self, target: super::super::BuiltinTarget) {
        self.table.suppressed_builtins.insert(target);
    }

    // =========================================================================
    // Lifecycle handlers
    // =========================================================================

    /// Register a tier-1 initialization handler
    /// ([ADR-044](../../../../docs/decisions.md#adr-044-handler--effect-tier-hierarchy)).
    ///
    /// Bound `E: Into<KakouneSideEffects>` rejects raw [`Effects`] returns
    /// at compile time. See [`Self::on_state_changed_tier1`] for the
    /// asymmetric-`From` rationale.
    pub fn on_init_tier1<E: Into<KakouneSideEffects> + 'static>(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>) -> (S, E) + Send + Sync + 'static,
    ) {
        self.table.init_handler = Some(Box::new(move |state, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            let (new_state, side) = handler(s, app);
            let side: KakouneSideEffects = side.into();
            let effects: Effects = side.into();
            (Box::new(new_state) as Box<dyn PluginState>, effects)
        }));
    }

    /// Register a tier-1 session-ready handler
    /// ([ADR-044](../../../../docs/decisions.md#adr-044-handler--effect-tier-hierarchy)).
    ///
    /// Bound `E: Into<KakouneSideEffects>` rejects raw [`Effects`] returns
    /// at compile time. The WIT-level `session-ready-command` variant
    /// already narrows to a subset; the Tier-1 type lifts the same
    /// guarantee to the native side.
    pub fn on_session_ready_tier1<E: Into<KakouneSideEffects> + 'static>(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>) -> (S, E) + Send + Sync + 'static,
    ) {
        self.table.session_ready_handler = Some(Box::new(move |state, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            let (new_state, side) = handler(s, app);
            let side: KakouneSideEffects = side.into();
            let effects: Effects = side.into();
            (Box::new(new_state) as Box<dyn PluginState>, effects)
        }));
    }

    /// Register a tier-1 state-changed handler
    /// ([ADR-044](../../../../docs/decisions.md#adr-044-handler--effect-tier-hierarchy)).
    ///
    /// Accepts closures returning `(S, KakouneSideEffects)`. The bound
    /// `E: Into<KakouneSideEffects>` rejects [`Effects`] returns at compile
    /// time, because there is intentionally no `From<Effects> for
    /// KakouneSideEffects` impl — `Effects` may carry `ProcessCommand`
    /// variants that re-entrance-prone handlers must not issue.
    ///
    /// Use this for `on_state_changed` handlers that need to flag bugs like
    /// the silent `SpawnProcess` drop ([#100](https://github.com/Yus314/kasane/issues/100)
    /// / [#101](https://github.com/Yus314/kasane/issues/101)) at the compiler
    /// instead of at runtime.
    ///
    /// See `handler_registry::tests::tier1_setter_rejects_effects_at_compile_time`
    /// for the negative case (a closure returning `Effects` is structurally
    /// excluded from this setter's bound and does not compile).
    pub fn on_state_changed_tier1<E: Into<KakouneSideEffects> + 'static>(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>, DirtyFlags) -> (S, E) + Send + Sync + 'static,
    ) {
        // Re-lift through KakouneSideEffects → Effects at the table boundary.
        self.table.state_changed_handler = Some(Box::new(move |state, app, dirty| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            let (new_state, side) = handler(s, app, dirty);
            let side: KakouneSideEffects = side.into();
            let effects: Effects = side.into();
            (Box::new(new_state) as Box<dyn PluginState>, effects)
        }));
    }

    /// Register a tier-2 I/O event handler
    /// ([ADR-044](../../../../docs/decisions.md#adr-044-handler--effect-tier-hierarchy)).
    ///
    /// Accepts closures returning `(S, ProcessCapableEffects)` (or narrower
    /// tier types — `ObservationEffects` and `KakouneSideEffects` lift via
    /// `From`). The bound `E: Into<ProcessCapableEffects>` rejects raw
    /// [`Effects`] returns; Effects is the type-erased lowest common
    /// denominator, and Tier 2 is the structurally widest *typed* tier.
    /// Migrating Effects-returning handlers means picking the right tier
    /// per ADR-044 — for I/O event handlers, Tier 2 is appropriate.
    pub fn on_io_event_tier2<E: Into<ProcessCapableEffects> + 'static>(
        &mut self,
        handler: impl Fn(&S, &IoEvent, &AppView<'_>) -> (S, E) + Send + Sync + 'static,
    ) {
        self.table.io_event_handler = Some(Box::new(move |state, event, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            let (new_state, tier) = handler(s, event, app);
            let tier: ProcessCapableEffects = tier.into();
            let effects: Effects = tier.into();
            (Box::new(new_state) as Box<dyn PluginState>, effects)
        }));
    }

    /// Register a tier-2 process task
    /// ([ADR-044](../../../../docs/decisions.md#adr-044-handler--effect-tier-hierarchy)).
    ///
    /// Tier-2 because process-task completion handlers naturally chain into
    /// further spawns (e.g. picker → preview pipelines). The bound
    /// `E: Into<ProcessCapableEffects>` rejects raw [`Effects`] returns;
    /// migrate via `ProcessCapableEffects::none()` /
    /// `KakouneSideEffects::none()`.
    pub fn on_process_task_tier2<E: Into<ProcessCapableEffects> + 'static>(
        &mut self,
        name: &'static str,
        spec: ProcessTaskSpec,
        handler: impl Fn(&S, &ProcessTaskResult, &AppView<'_>) -> (S, E) + Send + Sync + 'static,
    ) {
        self.table.process_tasks.push(ProcessTaskEntry {
            name,
            spec,
            handler: Box::new(move |state, result, app| {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                let (new_state, tier) = handler(s, result, app);
                let tier: ProcessCapableEffects = tier.into();
                let effects: Effects = tier.into();
                (Box::new(new_state) as Box<dyn PluginState>, effects)
            }),
            streaming: false,
            transparent: false,
        });
    }

    /// Register a tier-2 streaming process task
    /// ([ADR-044](../../../../docs/decisions.md#adr-044-handler--effect-tier-hierarchy)).
    ///
    /// Streaming counterpart of [`Self::on_process_task_tier2`].
    pub fn on_process_task_streaming_tier2<E: Into<ProcessCapableEffects> + 'static>(
        &mut self,
        name: &'static str,
        spec: ProcessTaskSpec,
        handler: impl Fn(&S, &ProcessTaskResult, &AppView<'_>) -> (S, E) + Send + Sync + 'static,
    ) {
        self.table.process_tasks.push(ProcessTaskEntry {
            name,
            spec,
            handler: Box::new(move |state, result, app| {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                let (new_state, tier) = handler(s, result, app);
                let tier: ProcessCapableEffects = tier.into();
                let effects: Effects = tier.into();
                (Box::new(new_state) as Box<dyn PluginState>, effects)
            }),
            streaming: true,
            transparent: false,
        });
    }

    /// Register a workspace-changed handler.
    pub fn on_workspace_changed(
        &mut self,
        handler: impl Fn(&S, &WorkspaceQuery<'_>) -> S + Send + Sync + 'static,
    ) {
        register_state_only!(self, workspace_changed_handler, handler, query);
    }

    /// Register a workspace save handler.
    ///
    /// Called during workspace layout save. Return `Some(value)` to persist
    /// plugin-specific data alongside the layout. The data will be passed
    /// back to the restore handler when the layout is restored.
    pub fn on_workspace_save(
        &mut self,
        handler: impl Fn(&S) -> Option<serde_json::Value> + Send + Sync + 'static,
    ) {
        self.table.workspace_save_handler = Some(Box::new(move |state| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            handler(s)
        }));
    }

    /// Register a workspace restore handler.
    ///
    /// Called during workspace layout restore with data previously returned
    /// by the save handler.
    pub fn on_workspace_restore(
        &mut self,
        handler: impl Fn(&S, &serde_json::Value) -> S + Send + Sync + 'static,
    ) {
        self.table.workspace_restore_handler = Some(Box::new(move |state, data| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            Box::new(handler(s, data)) as Box<dyn PluginState>
        }));
    }

    /// Register an opaque-bytes hot-reload persistence handler.
    ///
    /// Counterpart to [`Self::on_workspace_save`] for plugins whose
    /// underlying contract is bytes rather than structured JSON —
    /// primarily WASM plugins via the `persist-state` WIT export. Called
    /// during hot-reload save; return `Some(bytes)` to opt into
    /// `restore-state` being invoked after the reload.
    pub fn on_persist_state(
        &mut self,
        handler: impl Fn(&S) -> Option<Vec<u8>> + Send + Sync + 'static,
    ) {
        self.table.persist_state_handler = Some(Box::new(move |state| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            handler(s)
        }));
    }

    /// Register an opaque-bytes hot-reload restore handler.
    ///
    /// Counterpart to [`Self::on_workspace_restore`] for plugins whose
    /// underlying contract is bytes — primarily WASM plugins via the
    /// `restore-state` WIT export. Called with the bytes returned by
    /// the matching `persist-state` from the previous instance. Returns
    /// `true` if the bytes were applied; `false` to signal a
    /// schema/version mismatch (the host then drops them).
    pub fn on_restore_state(
        &mut self,
        handler: impl Fn(&S, &[u8]) -> bool + Send + Sync + 'static,
    ) {
        self.table.restore_state_handler = Some(Box::new(move |state, data| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            handler(s, data)
        }));
    }

    /// Register a shutdown handler.
    pub fn on_shutdown(&mut self, handler: impl Fn(&S) + Send + Sync + 'static) {
        register_void!(self, shutdown_handler, handler);
    }

    /// Declare static surfaces owned by this plugin.
    ///
    /// The factory is called during bootstrap preflight (before `on_init`)
    /// to materialise the plugin's surfaces. Use `declare_workspace_request`
    /// to attach a plugin-wide initial placement to those surfaces.
    pub fn declare_surfaces(
        &mut self,
        factory: impl Fn(&S) -> Vec<Box<dyn crate::surface::Surface>> + Send + Sync + 'static,
    ) {
        let erased: super::super::handler_table::ErasedSurfacesFactory = Box::new(move |state| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            factory(s)
        });
        self.table.surfaces_factory = Some(erased);
    }

    /// Declare a plugin-wide initial workspace placement.
    ///
    /// Evaluated during bootstrap preflight alongside `declare_surfaces`.
    pub fn declare_workspace_request(&mut self, placement: crate::workspace::Placement) {
        self.table.workspace_request = Some(placement);
    }

    /// Opt out of process spawning for this plugin.
    ///
    /// `PluginRuntime::plugin_allows_process_spawn` returns `false` for any
    /// plugin that calls this during `register`. Default is allowed.
    pub fn deny_process_spawn(&mut self) {
        self.table.allows_process_spawn = false;
    }

    /// Declare host-resolved authorities granted to this plugin.
    ///
    /// Replaces any previously declared set; default is empty.
    /// `PluginRuntime::plugin_has_authority` consults this value.
    pub fn declare_authorities(&mut self, authorities: super::super::PluginAuthorities) {
        self.table.authorities = authorities;
    }

    /// Override the auto-inferred [`PluginCapabilities`] set.
    ///
    /// Counterpart to handler-presence inference for adapters whose
    /// capability set is sourced from an external manifest or WIT
    /// export (e.g. WASM plugins via `register-capabilities`). When
    /// set, this value is returned by `PluginBridge::capabilities()`
    /// instead of [`HandlerTable::capabilities()`]'s derivation.
    pub fn declare_capabilities(&mut self, caps: super::super::PluginCapabilities) {
        self.table.capabilities_override = Some(caps);
    }

    /// Override the auto-derived [`CapabilityDescriptor`].
    ///
    /// Counterpart to the handler-presence-derived descriptor for
    /// adapters whose authoritative descriptor lives in a manifest
    /// (e.g. WASM plugins).
    pub fn declare_capability_descriptor(
        &mut self,
        descriptor: super::super::CapabilityDescriptor,
    ) {
        self.table.capability_descriptor_override = Some(descriptor);
    }

    /// Override the bridge's per-mutation generation counter as the
    /// source of `state_hash()`.
    ///
    /// Counterpart for adapters whose authoritative change-detection
    /// signal lives outside the framework's typed `PluginState` — most
    /// notably WASM plugins, which run their own state inside the
    /// wasmtime store and surface a per-call hash via the `state-hash`
    /// WIT export. When set, `PluginBridge::state_hash()` returns the
    /// closure's value; when absent, the bridge falls back to its
    /// generation counter.
    pub fn declare_state_hash(&mut self, handler: impl Fn() -> u64 + Send + Sync + 'static) {
        self.table.state_hash_handler = Some(Box::new(handler));
    }

    /// Set the priority for this plugin's display directives.
    ///
    /// Higher priorities win during `DirectiveSet` resolution. Default 0.
    pub fn declare_display_priority(&mut self, priority: i16) {
        self.table.display_priority = priority;
    }

    /// Declare lenses owned by this plugin.
    ///
    /// The factory is invoked once per `PluginRuntime::sync_lenses` call.
    /// Each returned lens is passed to `LensRegistry::register`.
    pub fn declare_lenses(
        &mut self,
        factory: impl Fn() -> Vec<std::sync::Arc<dyn crate::lens::Lens>> + Send + Sync + 'static,
    ) {
        self.table.lenses_factory = Some(Box::new(factory));
    }

    /// Register a tier-2 update (message) handler
    /// ([ADR-044](../../../../docs/decisions.md#adr-044-handler--effect-tier-hierarchy)).
    ///
    /// Accepts closures returning `(S, ProcessCapableEffects)` or any
    /// narrower tier. The bound `E: Into<ProcessCapableEffects>` rejects
    /// raw [`Effects`] at compile time.
    pub fn on_update_tier2<E: Into<ProcessCapableEffects> + 'static>(
        &mut self,
        handler: impl Fn(&S, &mut dyn Any, &AppView<'_>) -> (S, E) + Send + Sync + 'static,
    ) {
        self.table.update_handler = Some(Box::new(move |state, msg, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            let (new_state, tier) = handler(s, msg, app);
            let tier: ProcessCapableEffects = tier.into();
            let effects: Effects = tier.into();
            (Box::new(new_state) as Box<dyn PluginState>, effects)
        }));
    }

    // =========================================================================
    // `on_command_error` / `on_subscription` (ADR-044). Returning the broad
    // `Effects` type matches the WIT shape; tier enforcement can be added
    // later as a sibling setter.
    // =========================================================================

    /// Register a handler for plugin-attributed Kakoune command failures
    /// ([ADR-042](../../../../docs/decisions.md#adr-042-command-error-event-via-info_show-marker-attribution)).
    ///
    /// The handler fires when an `info_show` with the reserved title
    /// `__kasane_plugin_error__` is observed and the embedded plugin-id
    /// matches this plugin. The handler receives the parsed
    /// [`PluginErrorEvent`] (plugin-id + Kakoune error message) and
    /// returns updated state + effects.
    ///
    /// To opt into command-error attribution, set
    /// `[handlers] command_error_observability = true` in the plugin
    /// manifest — the host then auto-wraps every `EvalCommand` emitted
    /// by this plugin in a `try…catch` that fires the marker on failure.
    pub fn on_command_error<E: Into<Effects> + Transparency + 'static>(
        &mut self,
        handler: impl Fn(&S, &PluginErrorEvent, &AppView<'_>) -> (S, E) + Send + Sync + 'static,
    ) {
        self.table.command_error_handler = Some(Box::new(move |state, error, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            let (new_state, effects) = handler(s, error, app);
            (Box::new(new_state) as Box<dyn PluginState>, effects.into())
        }));
        if E::IS_TRANSPARENT {
            self.table.transparency.command_error_handler = true;
        }
    }

    /// Register a per-topic batch subscription handler.
    ///
    /// Mirrors the WIT `on-subscription(topic, values) -> runtime-effects`
    /// export: the handler fires once per subscribed topic during the
    /// pub/sub delivery phase with **all** values published on that
    /// topic this tick, returning updated state + effects. The effects
    /// flow back through the same `EffectsBatch` pipeline as
    /// `notify_state_changed` so commands and scroll plans land in the
    /// correct frame's `UpdateResult`.
    ///
    /// This is independent of the per-value
    /// [`subscribe`](super::extension::HandlerRegistry::subscribe) setter:
    /// `subscribe` mutates state per published value, while
    /// `on_subscription` lets the plugin emit effects per topic batch
    /// (for example, scheduling a redraw or kicking off a follow-up
    /// command). A plugin may register both kinds on the same topic.
    pub fn on_subscription<E: Into<Effects> + Transparency + 'static>(
        &mut self,
        handler: impl Fn(&S, &str, &[ChannelValue], &AppView<'_>) -> (S, E) + Send + Sync + 'static,
    ) {
        self.table.subscription_handler = Some(Box::new(move |state, topic, values, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            let (new_state, effects) = handler(s, topic, values, app);
            (Box::new(new_state) as Box<dyn PluginState>, effects.into())
        }));
        if E::IS_TRANSPARENT {
            self.table.transparency.subscription_handler = true;
        }
    }
}
