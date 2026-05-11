//! Lifecycle / state-change / I/O / workspace / process-task / update handlers.

use std::any::Any;

use crate::state::DirtyFlags;
use crate::workspace::WorkspaceQuery;

use super::super::process_task::{ProcessTaskEntry, ProcessTaskResult, ProcessTaskSpec};
use super::super::{
    AppView, Effects, IoEvent, KakouneSideEffects, PluginState, ProcessCapableEffects,
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

    /// Register an initialization handler.
    ///
    /// Accepts closures returning `(S, Effects)` or `(S, KakouneTransparentEffects)`.
    /// Using `KakouneTransparentEffects` provides a compile-time guarantee of no
    /// Kakoune writes (ADR-030 Level 5).
    ///
    /// **For new code, prefer [`Self::on_init_tier1`]** — it enforces the
    /// Tier-1 contract from
    /// [ADR-044](../../../../docs/decisions.md#adr-044-handler--effect-tier-hierarchy)
    /// at compile time, rejecting `ProcessCommand` variants. `on_init` is
    /// narrow at the WIT level already (Bootstrap phase rejects most
    /// commands), but the type-level pin further reduces ambiguity.
    pub fn on_init<E: Into<Effects> + Transparency + 'static>(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>) -> (S, E) + Send + Sync + 'static,
    ) {
        register_state_effect!(self, init_handler, handler, app);
        if E::IS_TRANSPARENT {
            self.table.transparency.init_handler = true;
        }
    }

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

    /// Register a session-ready handler.
    ///
    /// Accepts closures returning `(S, Effects)` or `(S, KakouneTransparentEffects)`.
    ///
    /// **For new code, prefer [`Self::on_session_ready_tier1`]** — it
    /// enforces the Tier-1 contract from
    /// [ADR-044](../../../../docs/decisions.md#adr-044-handler--effect-tier-hierarchy)
    /// at compile time.
    pub fn on_session_ready<E: Into<Effects> + Transparency + 'static>(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>) -> (S, E) + Send + Sync + 'static,
    ) {
        register_state_effect!(self, session_ready_handler, handler, app);
        if E::IS_TRANSPARENT {
            self.table.transparency.session_ready_handler = true;
        }
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

    /// Register a state-changed handler.
    ///
    /// Accepts closures returning `(S, Effects)` or `(S, KakouneTransparentEffects)`.
    ///
    /// **For new code, prefer [`Self::on_state_changed_tier1`]** — it enforces
    /// the Tier-1 contract from
    /// [ADR-044](../../../../docs/decisions.md#adr-044-handler--effect-tier-hierarchy)
    /// at compile time, rejecting `ProcessCommand` variants (`SpawnProcess`,
    /// `HttpRequest`, etc.) that re-entrance-prone handlers should not emit.
    /// This setter remains for migration; a future PR will deprecate it.
    pub fn on_state_changed<E: Into<Effects> + Transparency + 'static>(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>, DirtyFlags) -> (S, E) + Send + Sync + 'static,
    ) {
        register_state_effect!(self, state_changed_handler, handler, app, dirty);
        if E::IS_TRANSPARENT {
            self.table.transparency.state_changed_handler = true;
        }
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

    /// Register an I/O event handler.
    ///
    /// Accepts closures returning `(S, Effects)` or `(S, KakouneTransparentEffects)`.
    ///
    /// **For new code, prefer [`Self::on_io_event_tier2`]** — it pins the
    /// Tier-2 contract from
    /// [ADR-044](../../../../docs/decisions.md#adr-044-handler--effect-tier-hierarchy)
    /// at compile time. Tier 2 admits process commands, which I/O handlers
    /// naturally need for spawn chains, but the typed return still beats
    /// `Effects` for review readability and migration to the WIT tier
    /// split (Phase B).
    pub fn on_io_event<E: Into<Effects> + Transparency + 'static>(
        &mut self,
        handler: impl Fn(&S, &IoEvent, &AppView<'_>) -> (S, E) + Send + Sync + 'static,
    ) {
        register_state_effect!(self, io_event_handler, handler, event, app);
        if E::IS_TRANSPARENT {
            self.table.transparency.io_event_handler = true;
        }
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

    /// Register a declarative process task.
    ///
    /// The framework manages job ID allocation, stdout buffering, fallback on
    /// spawn failure, and state machine transitions. The handler receives a
    /// [`ProcessTaskResult`] when the task completes, fails, or (in streaming
    /// mode) produces output.
    ///
    /// The task is started by calling [`start_process_task`] on the plugin bridge.
    ///
    /// Accepts closures returning `(S, Effects)` or `(S, KakouneTransparentEffects)`.
    ///
    /// ```ignore
    /// r.on_process_task(
    ///     "file_list",
    ///     ProcessTaskSpec::new("fd", &["--type", "f"])
    ///         .fallback(ProcessTaskSpec::new("find", &[".", "-type", "f"])),
    ///     |state, result, _app| match result {
    ///         ProcessTaskResult::Completed { stdout, .. } => { /* ... */ }
    ///         ProcessTaskResult::Failed(msg) => { /* ... */ }
    ///         _ => (state.clone(), Effects::none()),
    ///     },
    /// );
    /// ```
    ///
    /// **For new code, prefer [`Self::on_process_task_tier2`]**.
    pub fn on_process_task<E: Into<Effects> + Transparency + 'static>(
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
                let (new_state, effects) = handler(s, result, app);
                (Box::new(new_state) as Box<dyn PluginState>, effects.into())
            }),
            streaming: false,
            transparent: E::IS_TRANSPARENT,
        });
    }

    /// Register a streaming process task.
    ///
    /// Like [`on_process_task`](Self::on_process_task), but delivers stdout
    /// chunks incrementally via [`ProcessTaskResult::Stdout`] instead of
    /// accumulating them until process exit.
    ///
    /// Accepts closures returning `(S, Effects)` or `(S, KakouneTransparentEffects)`.
    ///
    /// **For new code, prefer [`Self::on_process_task_streaming_tier2`]**.
    pub fn on_process_task_streaming<E: Into<Effects> + Transparency + 'static>(
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
                let (new_state, effects) = handler(s, result, app);
                (Box::new(new_state) as Box<dyn PluginState>, effects.into())
            }),
            streaming: true,
            transparent: E::IS_TRANSPARENT,
        });
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

    /// Register a shutdown handler.
    pub fn on_shutdown(&mut self, handler: impl Fn(&S) + Send + Sync + 'static) {
        register_void!(self, shutdown_handler, handler);
    }

    /// Register an update (message) handler.
    ///
    /// Accepts closures returning `(S, Effects)` or `(S, KakouneTransparentEffects)`.
    ///
    /// **For new code, prefer [`Self::on_update_tier2`]** — it pins the
    /// Tier-2 contract from
    /// [ADR-044](../../../../docs/decisions.md#adr-044-handler--effect-tier-hierarchy)
    /// at compile time. The command-handler pattern legitimately spawns
    /// processes, so Tier 2 is the appropriate enforcement.
    pub fn on_update<E: Into<Effects> + Transparency + 'static>(
        &mut self,
        handler: impl Fn(&S, &mut dyn Any, &AppView<'_>) -> (S, E) + Send + Sync + 'static,
    ) {
        register_state_effect!(self, update_handler, handler, msg, app);
        if E::IS_TRANSPARENT {
            self.table.transparency.update_handler = true;
        }
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
}
