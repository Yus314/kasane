use std::time::Duration;

use crate::bindings::kasane::plugin::types as wit;
use kasane_core::input::InputEvent;
use kasane_core::plugin::{
    BootstrapEffects, BufferEdit, BufferPosition, Command, PluginId, RuntimeEffects,
    SessionReadyCommand, SessionReadyEffects, StdinMode,
};
use kasane_core::protocol::KasaneRequest;
use kasane_core::session::SessionCommand as CoreSessionCommand;
use kasane_core::state::DirtyFlags;

use super::{wit_key_event_to_key_event, wit_scroll_plan_to_scroll_plan};

pub(crate) fn wit_command_to_command(wc: &wit::Command) -> Command {
    match wc {
        wit::Command::SendKeys(keys) => Command::SendToKakoune(KasaneRequest::Keys(keys.clone())),
        wit::Command::Paste => Command::Paste,
        wit::Command::Quit => Command::Quit,
        wit::Command::RequestRedraw(bits) => {
            Command::RequestRedraw(DirtyFlags::from_bits_truncate(*bits))
        }
        wit::Command::SetConfig(entry) => Command::SetConfig {
            key: entry.key.clone(),
            value: entry.value.clone(),
        },
        wit::Command::ScheduleTimer(tc) => Command::ScheduleTimer {
            delay: Duration::from_millis(tc.delay_ms),
            target: PluginId(tc.target_plugin.clone()),
            payload: Box::new(tc.payload.clone()),
        },
        wit::Command::PluginMessage(mc) => Command::PluginMessage {
            target: PluginId(mc.target_plugin.clone()),
            payload: Box::new(mc.payload.clone()),
        },
        wit::Command::RegisterSurface(_) => {
            unreachable!("register-surface commands require adapter context")
        }
        wit::Command::UnregisterSurface(surface_key) => Command::UnregisterSurfaceKey {
            surface_key: surface_key.clone(),
        },
        wit::Command::SpawnProcess(cfg) => Command::SpawnProcess {
            job_id: cfg.job_id,
            program: cfg.program.clone(),
            args: cfg.args.clone(),
            stdin_mode: match cfg.stdin_mode {
                wit::StdinMode::NullStdin => StdinMode::Null,
                wit::StdinMode::Piped => StdinMode::Piped,
                wit::StdinMode::Pty(ref size) => StdinMode::Pty {
                    rows: size.rows,
                    cols: size.cols,
                },
            },
        },
        wit::Command::SpawnSession(cfg) => Command::Session(CoreSessionCommand::Spawn {
            key: cfg.key.clone(),
            session: cfg.session.clone(),
            args: cfg.args.clone(),
            activate: cfg.activate,
        }),
        wit::Command::CloseSession(key) => {
            Command::Session(CoreSessionCommand::Close { key: key.clone() })
        }
        wit::Command::WriteToProcess(cfg) => Command::WriteToProcess {
            job_id: cfg.job_id,
            data: cfg.data.clone(),
        },
        wit::Command::CloseProcessStdin(job_id) => Command::CloseProcessStdin { job_id: *job_id },
        wit::Command::KillProcess(job_id) => Command::KillProcess { job_id: *job_id },
        wit::Command::ResizePty(cfg) => Command::ResizePty {
            job_id: cfg.job_id,
            rows: cfg.rows,
            cols: cfg.cols,
        },
        wit::Command::InjectKey(key_event) => {
            match wit_key_event_to_key_event(key_event) {
                Ok(native_key) => Command::InjectInput(InputEvent::Key(native_key)),
                Err(msg) => {
                    tracing::warn!(error = %msg, "ignoring inject-key with invalid key event");
                    // Return a no-op command
                    Command::RequestRedraw(DirtyFlags::empty())
                }
            }
        }
        wit::Command::EditBuffer(edits) => Command::EditBuffer {
            edits: edits
                .iter()
                .map(|e| BufferEdit {
                    start: BufferPosition {
                        line: e.start_line,
                        column: e.start_column,
                    },
                    end: BufferPosition {
                        line: e.end_line,
                        column: e.end_column,
                    },
                    replacement: e.replacement.clone(),
                })
                .collect(),
        },
        wit::Command::SwitchSession(key) => {
            Command::Session(CoreSessionCommand::Switch { key: key.clone() })
        }
        wit::Command::RegisterThemeTokens(tokens) => Command::RegisterThemeTokens(
            tokens
                .iter()
                .map(|t| (t.token.clone(), super::wit_face_to_face(&t.face)))
                .collect(),
        ),
        wit::Command::SpawnPaneClient(config) => Command::SpawnPaneClient {
            pane_key: config.pane_key.clone(),
            placement: super::wit_surface_placement_to_placement(&config.placement),
        },
        wit::Command::ClosePaneClient(key) => Command::ClosePaneClient {
            pane_key: key.clone(),
        },
        wit::Command::WorkspaceCommand(ws_cmd) => match ws_cmd {
            wit::WorkspaceCmd::FocusDirection(dir) => {
                Command::Workspace(kasane_core::workspace::WorkspaceCommand::FocusDirection(
                    wit_focus_dir_to_focus_direction(*dir),
                ))
            }
            wit::WorkspaceCmd::Resize(delta) => {
                Command::Workspace(kasane_core::workspace::WorkspaceCommand::Resize {
                    delta: *delta,
                })
            }
            wit::WorkspaceCmd::ResizeDirection(config) => {
                Command::Workspace(kasane_core::workspace::WorkspaceCommand::ResizeDirection {
                    direction: super::wit_split_direction_to_split_direction(config.direction),
                    delta: config.delta,
                })
            }
        },
    }
}

fn wit_focus_dir_to_focus_direction(dir: wit::FocusDir) -> kasane_core::workspace::FocusDirection {
    use kasane_core::workspace::FocusDirection;
    match dir {
        wit::FocusDir::NextDir => FocusDirection::Next,
        wit::FocusDir::PrevDir => FocusDirection::Prev,
        wit::FocusDir::LeftDir => FocusDirection::Left,
        wit::FocusDir::RightDir => FocusDirection::Right,
        wit::FocusDir::UpDir => FocusDirection::Up,
        wit::FocusDir::DownDir => FocusDirection::Down,
    }
}

pub(crate) fn wit_runtime_effects_to_effects_with(
    effects: &wit::RuntimeEffects,
    mut convert_command: impl FnMut(&wit::Command) -> Vec<Command>,
) -> RuntimeEffects {
    RuntimeEffects {
        redraw: DirtyFlags::from_bits_truncate(effects.redraw),
        commands: effects
            .commands
            .iter()
            .flat_map(&mut convert_command)
            .collect(),
        scroll_plans: effects
            .scroll_plans
            .iter()
            .map(wit_scroll_plan_to_scroll_plan)
            .collect(),
    }
}

pub(crate) fn wit_bootstrap_effects_to_effects(
    effects: &wit::BootstrapEffects,
) -> BootstrapEffects {
    BootstrapEffects {
        redraw: DirtyFlags::from_bits_truncate(effects.redraw),
    }
}

pub(crate) fn wit_session_ready_effects_to_effects(
    effects: &wit::SessionReadyEffects,
) -> SessionReadyEffects {
    SessionReadyEffects {
        redraw: DirtyFlags::from_bits_truncate(effects.redraw),
        commands: effects
            .commands
            .iter()
            .map(wit_session_ready_command_to_command)
            .collect(),
        scroll_plans: effects
            .scroll_plans
            .iter()
            .map(wit_scroll_plan_to_scroll_plan)
            .collect(),
    }
}

fn wit_session_ready_command_to_command(command: &wit::SessionReadyCommand) -> SessionReadyCommand {
    match command {
        wit::SessionReadyCommand::SendKeys(keys) => {
            SessionReadyCommand::SendToKakoune(KasaneRequest::Keys(keys.clone()))
        }
        wit::SessionReadyCommand::Paste => SessionReadyCommand::Paste,
        wit::SessionReadyCommand::PluginMessage(message) => SessionReadyCommand::PluginMessage {
            target: PluginId(message.target_plugin.clone()),
            payload: Box::new(message.payload.clone()),
        },
    }
}

#[cfg(test)]
pub(crate) fn wit_runtime_effects_to_effects(effects: &wit::RuntimeEffects) -> RuntimeEffects {
    wit_runtime_effects_to_effects_with(effects, |command| vec![wit_command_to_command(command)])
}
