//! Backend-agnostic event loop helpers.
//!
//! Extracts the deferred command handling logic that is shared between
//! TUI and GUI backends.

use std::any::Any;
use std::io::Write;
use std::time::Duration;

use crate::input::InputEvent;
use crate::layout::Rect;
use crate::plugin::{
    CommandResult, DeferredCommand, IoEvent, PluginId, PluginRegistry, ProcessDispatcher,
    ProcessEvent, execute_commands, extract_deferred_commands,
};
use crate::session::SessionSpec;
use crate::state::{AppState, DirtyFlags};
use crate::surface::{SourcedSurfaceCommands, SurfaceEvent, SurfaceRegistry};

/// Convert an input event into a surface event.
///
/// Shared between TUI and GUI backends for routing input through the surface system.
pub fn surface_event_from_input(input: &InputEvent) -> Option<SurfaceEvent> {
    match input {
        InputEvent::Key(key) => Some(SurfaceEvent::Key(key.clone())),
        InputEvent::Mouse(mouse) => Some(SurfaceEvent::Mouse(mouse.clone())),
        InputEvent::Resize(cols, rows) => Some(SurfaceEvent::Resize(Rect {
            x: 0,
            y: 0,
            w: *cols,
            h: *rows,
        })),
        InputEvent::FocusGained => Some(SurfaceEvent::FocusGained),
        InputEvent::FocusLost => Some(SurfaceEvent::FocusLost),
        InputEvent::Paste(_) => None,
    }
}

/// Backend-agnostic timer scheduling.
///
/// Implementations spawn a background thread that sleeps for `delay` and then
/// delivers the timer event through the backend's event system.
pub trait TimerScheduler {
    fn schedule_timer(&self, delay: Duration, target: PluginId, payload: Box<dyn Any + Send>);
}

/// Backend-owned session lifecycle hooks used by deferred commands.
pub trait SessionRuntime {
    /// Spawn a new managed session.
    fn spawn_session(
        &mut self,
        spec: SessionSpec,
        activate: bool,
        state: &mut AppState,
        dirty: &mut DirtyFlags,
        initial_resize_sent: &mut bool,
    );

    /// Close a managed session by key, or the active session when `key` is `None`.
    ///
    /// Returns `true` when the application should exit because no session remains.
    fn close_session(
        &mut self,
        key: Option<&str>,
        state: &mut AppState,
        dirty: &mut DirtyFlags,
        initial_resize_sent: &mut bool,
    ) -> bool;
}

/// Backend-owned access to the active session writer plus session lifecycle hooks.
pub trait SessionHost: SessionRuntime {
    fn active_writer(&mut self) -> &mut dyn Write;
}

/// Shared mutable context for deferred command handling.
///
/// Groups the many `&mut` parameters that `handle_deferred_commands` and
/// `handle_sourced_surface_commands` previously accepted individually.
pub struct DeferredContext<'a> {
    pub state: &'a mut AppState,
    pub registry: &'a mut PluginRegistry,
    pub surface_registry: &'a mut SurfaceRegistry,
    pub clipboard_get: &'a mut dyn FnMut() -> Option<String>,
    pub dirty: &'a mut DirtyFlags,
    pub timer: &'a dyn TimerScheduler,
    pub session_host: &'a mut dyn SessionHost,
    pub initial_resize_sent: &'a mut bool,
    pub process_dispatcher: &'a mut dyn ProcessDispatcher,
}

/// Handle deferred commands (timers, inter-plugin messages, config overrides).
///
/// Returns `true` if a `Quit` command was encountered.
pub fn handle_deferred_commands(
    deferred: Vec<DeferredCommand>,
    ctx: &mut DeferredContext<'_>,
    command_source_plugin: Option<&PluginId>,
) -> bool {
    for cmd in deferred {
        match cmd {
            DeferredCommand::PluginMessage { target, payload } => {
                let (flags, commands) = ctx.registry.deliver_message(&target, payload, ctx.state);
                *ctx.dirty |= flags;
                let (normal, nested_deferred) = extract_deferred_commands(commands);
                if matches!(
                    execute_commands(normal, ctx.session_host.active_writer(), ctx.clipboard_get),
                    CommandResult::Quit
                ) {
                    return true;
                }
                if handle_deferred_commands(nested_deferred, ctx, Some(&target)) {
                    return true;
                }
            }
            DeferredCommand::ScheduleTimer {
                delay,
                target,
                payload,
            } => {
                ctx.timer.schedule_timer(delay, target, payload);
            }
            DeferredCommand::SetConfig { key, value } => {
                crate::state::apply_set_config(ctx.state, ctx.dirty, &key, &value);
            }
            DeferredCommand::Pane(_) => {
                // Pane commands will be handled in Phase 5a-1
            }
            DeferredCommand::Workspace(ws_cmd) => {
                crate::workspace::dispatch_workspace_command_with_total(
                    ctx.surface_registry,
                    ws_cmd,
                    ctx.dirty,
                    Some(crate::layout::Rect {
                        x: 0,
                        y: 0,
                        w: ctx.state.cols,
                        h: ctx.state.rows,
                    }),
                );
            }
            DeferredCommand::RegisterThemeTokens(_tokens) => {
                // Theme token registration will be handled when Theme is
                // accessible from the event loop (Phase 1 completion).
            }
            DeferredCommand::SpawnProcess {
                job_id,
                program,
                args,
                stdin_mode,
            } => {
                if let Some(plugin_id) = command_source_plugin {
                    if ctx.registry.plugin_allows_process_spawn(plugin_id) {
                        ctx.process_dispatcher
                            .spawn(plugin_id, job_id, &program, &args, stdin_mode);
                    } else {
                        tracing::warn!(
                            plugin = plugin_id.0,
                            "SpawnProcess denied: process capability not granted"
                        );
                        let fail_event = IoEvent::Process(ProcessEvent::SpawnFailed {
                            job_id,
                            error: "process capability not granted".to_string(),
                        });
                        let (flags, fail_cmds) =
                            ctx.registry
                                .deliver_io_event(plugin_id, &fail_event, ctx.state);
                        *ctx.dirty |= flags;
                        let (normal, nested_deferred) = extract_deferred_commands(fail_cmds);
                        if matches!(
                            execute_commands(
                                normal,
                                ctx.session_host.active_writer(),
                                ctx.clipboard_get
                            ),
                            CommandResult::Quit
                        ) {
                            return true;
                        }
                        if handle_deferred_commands(nested_deferred, ctx, Some(plugin_id)) {
                            return true;
                        }
                    }
                }
            }
            DeferredCommand::WriteToProcess { job_id, data } => {
                if let Some(plugin_id) = command_source_plugin {
                    ctx.process_dispatcher.write(plugin_id, job_id, &data);
                }
            }
            DeferredCommand::CloseProcessStdin { job_id } => {
                if let Some(plugin_id) = command_source_plugin {
                    ctx.process_dispatcher.close_stdin(plugin_id, job_id);
                }
            }
            DeferredCommand::KillProcess { job_id } => {
                if let Some(plugin_id) = command_source_plugin {
                    ctx.process_dispatcher.kill(plugin_id, job_id);
                }
            }
            DeferredCommand::Session(cmd) => match cmd {
                crate::session::SessionCommand::Spawn {
                    key,
                    session,
                    args,
                    activate,
                } => {
                    ctx.session_host.spawn_session(
                        SessionSpec::with_fallback_key(key, session, args),
                        activate,
                        ctx.state,
                        ctx.dirty,
                        ctx.initial_resize_sent,
                    );
                }
                crate::session::SessionCommand::Close { key } => {
                    if ctx.session_host.close_session(
                        key.as_deref(),
                        ctx.state,
                        ctx.dirty,
                        ctx.initial_resize_sent,
                    ) {
                        return true;
                    }
                }
            },
        }
    }
    false
}

/// Execute grouped surface commands while preserving each surface owner's plugin identity.
///
/// Returns `true` if a `Quit` command was encountered.
pub fn handle_sourced_surface_commands(
    command_groups: Vec<SourcedSurfaceCommands>,
    ctx: &mut DeferredContext<'_>,
) -> bool {
    for entry in command_groups {
        let (normal, deferred) = extract_deferred_commands(entry.commands);
        if matches!(
            execute_commands(normal, ctx.session_host.active_writer(), ctx.clipboard_get),
            CommandResult::Quit
        ) {
            return true;
        }
        if handle_deferred_commands(deferred, ctx, entry.source_plugin.as_ref()) {
            return true;
        }
    }
    false
}

/// Consume an input event that targets a workspace split divider.
///
/// Divider drag is handled before normal input routing so divider presses do
/// not leak through to Kakoune or plugin mouse handlers.
pub fn handle_workspace_divider_input(
    input: &InputEvent,
    surface_registry: &mut SurfaceRegistry,
    total: Rect,
) -> Option<DirtyFlags> {
    match input {
        InputEvent::Mouse(mouse) => surface_registry.handle_workspace_divider_mouse(mouse, total),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::plugin::{Command, Plugin, StdinMode};

    struct TestPlugin {
        id: PluginId,
        allow_spawn: bool,
    }

    impl Plugin for TestPlugin {
        fn id(&self) -> PluginId {
            self.id.clone()
        }

        fn allows_process_spawn(&self) -> bool {
            self.allow_spawn
        }
    }

    struct NoopTimer;

    impl TimerScheduler for NoopTimer {
        fn schedule_timer(
            &self,
            _delay: Duration,
            _target: PluginId,
            _payload: Box<dyn Any + Send>,
        ) {
        }
    }

    #[derive(Default)]
    struct NoopSessionRuntime {
        writer: Vec<u8>,
    }

    impl SessionRuntime for NoopSessionRuntime {
        fn spawn_session(
            &mut self,
            _spec: SessionSpec,
            _activate: bool,
            _state: &mut AppState,
            _dirty: &mut DirtyFlags,
            _initial_resize_sent: &mut bool,
        ) {
        }

        fn close_session(
            &mut self,
            _key: Option<&str>,
            _state: &mut AppState,
            _dirty: &mut DirtyFlags,
            _initial_resize_sent: &mut bool,
        ) -> bool {
            false
        }
    }

    impl SessionHost for NoopSessionRuntime {
        fn active_writer(&mut self) -> &mut dyn Write {
            &mut self.writer
        }
    }

    #[derive(Default)]
    struct RecordingDispatcher {
        spawned: Vec<(PluginId, u64, String, Vec<String>, StdinMode)>,
    }

    impl ProcessDispatcher for RecordingDispatcher {
        fn spawn(
            &mut self,
            plugin_id: &PluginId,
            job_id: u64,
            program: &str,
            args: &[String],
            stdin_mode: StdinMode,
        ) {
            self.spawned.push((
                plugin_id.clone(),
                job_id,
                program.to_string(),
                args.to_vec(),
                stdin_mode,
            ));
        }

        fn write(&mut self, _plugin_id: &PluginId, _job_id: u64, _data: &[u8]) {}

        fn close_stdin(&mut self, _plugin_id: &PluginId, _job_id: u64) {}

        fn kill(&mut self, _plugin_id: &PluginId, _job_id: u64) {}

        fn remove_finished_job(&mut self, _plugin_id: &PluginId, _job_id: u64) {}
    }

    #[test]
    fn sourced_surface_commands_preserve_plugin_for_spawn_process() {
        let plugin_id = PluginId("surface-owner".to_string());
        let mut registry = PluginRegistry::new();
        registry.register(Box::new(TestPlugin {
            id: plugin_id.clone(),
            allow_spawn: true,
        }));

        let mut state = AppState::default();
        let mut surface_registry = SurfaceRegistry::new();
        let mut dirty = DirtyFlags::empty();
        let timer = NoopTimer;
        let mut sessions = NoopSessionRuntime::default();
        let mut initial_resize_sent = false;
        let mut dispatcher = RecordingDispatcher::default();

        let quit = handle_sourced_surface_commands(
            vec![SourcedSurfaceCommands {
                source_plugin: Some(plugin_id.clone()),
                commands: vec![Command::SpawnProcess {
                    job_id: 42,
                    program: "fd".to_string(),
                    args: vec!["foo".to_string()],
                    stdin_mode: StdinMode::Null,
                }],
            }],
            &mut DeferredContext {
                state: &mut state,
                registry: &mut registry,
                surface_registry: &mut surface_registry,
                clipboard_get: &mut || None,
                dirty: &mut dirty,
                timer: &timer,
                session_host: &mut sessions,
                initial_resize_sent: &mut initial_resize_sent,
                process_dispatcher: &mut dispatcher,
            },
        );

        assert!(!quit);
        assert_eq!(dispatcher.spawned.len(), 1);
        assert_eq!(dispatcher.spawned[0].0, plugin_id);
        assert_eq!(dispatcher.spawned[0].1, 42);
        assert_eq!(dispatcher.spawned[0].2, "fd");
        assert_eq!(dispatcher.spawned[0].3, vec!["foo".to_string()]);
        assert_eq!(dispatcher.spawned[0].4, StdinMode::Null);
    }

    #[derive(Default)]
    struct RecordingSessionHost {
        writer: Vec<u8>,
        spawned: Vec<(SessionSpec, bool)>,
        closed: Vec<Option<String>>,
        close_returns_quit: bool,
    }

    impl SessionRuntime for RecordingSessionHost {
        fn spawn_session(
            &mut self,
            spec: SessionSpec,
            activate: bool,
            _state: &mut AppState,
            _dirty: &mut DirtyFlags,
            _initial_resize_sent: &mut bool,
        ) {
            self.spawned.push((spec, activate));
        }

        fn close_session(
            &mut self,
            key: Option<&str>,
            _state: &mut AppState,
            _dirty: &mut DirtyFlags,
            _initial_resize_sent: &mut bool,
        ) -> bool {
            self.closed.push(key.map(ToOwned::to_owned));
            self.close_returns_quit
        }
    }

    impl SessionHost for RecordingSessionHost {
        fn active_writer(&mut self) -> &mut dyn Write {
            &mut self.writer
        }
    }

    #[test]
    fn deferred_session_spawn_is_routed_to_session_host() {
        let mut state = AppState::default();
        let mut registry = PluginRegistry::new();
        let mut surface_registry = SurfaceRegistry::new();
        let mut dirty = DirtyFlags::empty();
        let timer = NoopTimer;
        let mut sessions = RecordingSessionHost::default();
        let mut initial_resize_sent = true;
        let mut dispatcher = RecordingDispatcher::default();

        let quit = handle_deferred_commands(
            vec![DeferredCommand::Session(
                crate::session::SessionCommand::Spawn {
                    key: Some("work".to_string()),
                    session: Some("project".to_string()),
                    args: vec!["file.txt".to_string()],
                    activate: true,
                },
            )],
            &mut DeferredContext {
                state: &mut state,
                registry: &mut registry,
                surface_registry: &mut surface_registry,
                clipboard_get: &mut || None,
                dirty: &mut dirty,
                timer: &timer,
                session_host: &mut sessions,
                initial_resize_sent: &mut initial_resize_sent,
                process_dispatcher: &mut dispatcher,
            },
            None,
        );

        assert!(!quit);
        assert_eq!(sessions.spawned.len(), 1);
        assert_eq!(sessions.spawned[0].0.key, "work");
        assert_eq!(sessions.spawned[0].0.session.as_deref(), Some("project"));
        assert_eq!(sessions.spawned[0].0.args, vec!["file.txt".to_string()]);
        assert!(sessions.spawned[0].1);
    }

    #[test]
    fn deferred_session_close_is_routed_to_session_host() {
        let mut state = AppState::default();
        let mut registry = PluginRegistry::new();
        let mut surface_registry = SurfaceRegistry::new();
        let mut dirty = DirtyFlags::empty();
        let timer = NoopTimer;
        let mut sessions = RecordingSessionHost::default();
        let mut initial_resize_sent = false;
        let mut dispatcher = RecordingDispatcher::default();

        let quit = handle_deferred_commands(
            vec![DeferredCommand::Session(
                crate::session::SessionCommand::Close {
                    key: Some("work".to_string()),
                },
            )],
            &mut DeferredContext {
                state: &mut state,
                registry: &mut registry,
                surface_registry: &mut surface_registry,
                clipboard_get: &mut || None,
                dirty: &mut dirty,
                timer: &timer,
                session_host: &mut sessions,
                initial_resize_sent: &mut initial_resize_sent,
                process_dispatcher: &mut dispatcher,
            },
            None,
        );

        assert!(!quit);
        assert_eq!(sessions.closed, vec![Some("work".to_string())]);
    }

    #[test]
    fn deferred_session_close_returns_quit_when_host_requests_shutdown() {
        let mut state = AppState::default();
        let mut registry = PluginRegistry::new();
        let mut surface_registry = SurfaceRegistry::new();
        let mut dirty = DirtyFlags::empty();
        let timer = NoopTimer;
        let mut sessions = RecordingSessionHost {
            close_returns_quit: true,
            ..RecordingSessionHost::default()
        };
        let mut initial_resize_sent = false;
        let mut dispatcher = RecordingDispatcher::default();

        let quit = handle_deferred_commands(
            vec![DeferredCommand::Session(
                crate::session::SessionCommand::Close { key: None },
            )],
            &mut DeferredContext {
                state: &mut state,
                registry: &mut registry,
                surface_registry: &mut surface_registry,
                clipboard_get: &mut || None,
                dirty: &mut dirty,
                timer: &timer,
                session_host: &mut sessions,
                initial_resize_sent: &mut initial_resize_sent,
                process_dispatcher: &mut dispatcher,
            },
            None,
        );

        assert!(quit);
        assert_eq!(sessions.closed, vec![None]);
    }
}
