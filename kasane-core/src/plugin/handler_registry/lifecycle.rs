//! Carve-outs on the lifecycle axis: `declare_*` config setters +
//! `on_process_task_*` (process-task carve-out, spec §9.5).
//!
//! γ-3.3c-5b: the redundant manual `on_init_tier1` / `on_session_ready_tier1` /
//! `on_state_changed_tier1` / `on_io_event_tier2` / `on_workspace_changed` /
//! `on_workspace_save` / `on_workspace_restore` / `on_persist_state` /
//! `on_restore_state` / `on_shutdown` / `on_update_tier2` /
//! `on_command_error` / `on_subscription` setters were retired —
//! plugin code now invokes the macro-generated counterparts via `Deref`
//! from `HandlerRegistry` to `gen::HandlerRegistry`. The `declare_*`
//! family stays manual because the macro does not auto-emit setters
//! for `config` entries (only the field + initializer); these typed
//! writes are the public configuration API.

use crate::state::DirtyFlags;

use super::super::process_task::{ProcessTaskEntry, ProcessTaskResult, ProcessTaskSpec};
use super::super::{AppView, Effects, PluginState, ProcessCapableEffects};

use super::HandlerRegistry;

impl<S: PluginState + Clone + 'static> HandlerRegistry<S> {
    pub fn declare_interests(&mut self, flags: DirtyFlags) {
        self.inner.table.interests = flags;
    }

    /// Suppress a built-in plugin feature.
    ///
    /// When called, the corresponding built-in plugin will skip its default
    /// behavior, allowing this plugin to provide a full replacement.
    pub fn suppress_builtin(&mut self, target: super::super::BuiltinTarget) {
        self.inner.table.suppressed_builtins.insert(target);
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
        self.on_surfaces(factory);
    }

    /// Declare a plugin-wide initial workspace placement.
    ///
    /// Evaluated during bootstrap preflight alongside `declare_surfaces`.
    pub fn declare_workspace_request(&mut self, placement: crate::workspace::Placement) {
        self.inner.table.workspace_request = Some(placement);
    }

    /// Opt out of process spawning for this plugin.
    ///
    /// `PluginRuntime::plugin_allows_process_spawn` returns `false` for any
    /// plugin that calls this during `register`. Default is allowed.
    pub fn deny_process_spawn(&mut self) {
        self.inner.table.allows_process_spawn = false;
    }

    /// Declare host-resolved authorities granted to this plugin.
    ///
    /// Replaces any previously declared set; default is empty.
    /// `PluginRuntime::plugin_has_authority` consults this value.
    pub fn declare_authorities(&mut self, authorities: super::super::PluginAuthorities) {
        self.inner.table.authorities = authorities;
    }

    /// Override the auto-inferred [`PluginCapabilities`] set.
    ///
    /// Counterpart to handler-presence inference for adapters whose
    /// capability set is sourced from an external manifest or WIT
    /// export (e.g. WASM plugins via `register-capabilities`). When
    /// set, this value is returned by `PluginBridge::capabilities()`
    /// instead of [`HandlerTable::capabilities()`]'s derivation.
    pub fn declare_capabilities(&mut self, caps: super::super::PluginCapabilities) {
        self.inner.table.capabilities_override = Some(caps);
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
        self.inner.table.capability_descriptor_override = Some(descriptor);
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
        self.inner.table.state_hash = Some(Box::new(handler));
    }

    /// Set the priority for this plugin's display directives.
    ///
    /// Higher priorities win during `DirectiveSet` resolution. Default 0.
    pub fn declare_display_priority(&mut self, priority: i16) {
        self.inner.table.display_priority = priority;
    }

    /// Declare lenses owned by this plugin.
    ///
    /// The factory is invoked once per `PluginRuntime::sync_lenses` call.
    /// Each returned lens is passed to `LensRegistry::register`.
    pub fn declare_lenses(
        &mut self,
        factory: impl Fn() -> Vec<std::sync::Arc<dyn crate::lens::Lens>> + Send + Sync + 'static,
    ) {
        // `lenses` is a `stateless` View entry — the generated setter
        // takes `Fn() -> Vec<…>` (no `&S` arg).
        self.on_lenses(factory);
    }

    // =========================================================================
    // Process-task carve-out (spec §9.5)
    // =========================================================================

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
        self.process_tasks.push(ProcessTaskEntry {
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
        self.process_tasks.push(ProcessTaskEntry {
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
}
