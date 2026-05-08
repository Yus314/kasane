//! Lifecycle / state-change / I/O / workspace / process-task / update handlers.

use std::any::Any;

use crate::state::DirtyFlags;
use crate::workspace::WorkspaceQuery;

use super::super::process_task::{ProcessTaskEntry, ProcessTaskResult, ProcessTaskSpec};
use super::super::{AppView, Effects, IoEvent, PluginState};

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
    /// Accepts closures returning `(S, Effects)` or `(S, KakouneSafeEffects)`.
    /// Using `KakouneSafeEffects` provides a compile-time guarantee of no
    /// Kakoune writes (ADR-030 Level 5).
    pub fn on_init<E: Into<Effects> + Transparency + 'static>(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>) -> (S, E) + Send + Sync + 'static,
    ) {
        register_state_effect!(self, init_handler, handler, app);
        if E::IS_TRANSPARENT {
            self.table.transparency.init_handler = true;
        }
    }

    /// Register a session-ready handler.
    ///
    /// Accepts closures returning `(S, Effects)` or `(S, KakouneSafeEffects)`.
    pub fn on_session_ready<E: Into<Effects> + Transparency + 'static>(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>) -> (S, E) + Send + Sync + 'static,
    ) {
        register_state_effect!(self, session_ready_handler, handler, app);
        if E::IS_TRANSPARENT {
            self.table.transparency.session_ready_handler = true;
        }
    }

    /// Register a state-changed handler.
    ///
    /// Accepts closures returning `(S, Effects)` or `(S, KakouneSafeEffects)`.
    pub fn on_state_changed<E: Into<Effects> + Transparency + 'static>(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>, DirtyFlags) -> (S, E) + Send + Sync + 'static,
    ) {
        register_state_effect!(self, state_changed_handler, handler, app, dirty);
        if E::IS_TRANSPARENT {
            self.table.transparency.state_changed_handler = true;
        }
    }

    /// Register an I/O event handler.
    ///
    /// Accepts closures returning `(S, Effects)` or `(S, KakouneSafeEffects)`.
    pub fn on_io_event<E: Into<Effects> + Transparency + 'static>(
        &mut self,
        handler: impl Fn(&S, &IoEvent, &AppView<'_>) -> (S, E) + Send + Sync + 'static,
    ) {
        register_state_effect!(self, io_event_handler, handler, event, app);
        if E::IS_TRANSPARENT {
            self.table.transparency.io_event_handler = true;
        }
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
    /// Accepts closures returning `(S, Effects)` or `(S, KakouneSafeEffects)`.
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
    /// Accepts closures returning `(S, Effects)` or `(S, KakouneSafeEffects)`.
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
    /// Accepts closures returning `(S, Effects)` or `(S, KakouneSafeEffects)`.
    pub fn on_update<E: Into<Effects> + Transparency + 'static>(
        &mut self,
        handler: impl Fn(&S, &mut dyn Any, &AppView<'_>) -> (S, E) + Send + Sync + 'static,
    ) {
        register_state_effect!(self, update_handler, handler, msg, app);
        if E::IS_TRANSPARENT {
            self.table.transparency.update_handler = true;
        }
    }

    // =========================================================================
}
