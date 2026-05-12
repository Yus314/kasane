use std::time::Duration;

use crate::bindings::kasane::plugin::types as wit;
use kasane_core::input::InputEvent;
use kasane_core::plugin::{
    BufferEdit, BufferPosition, Command, Effects, KakouneSideCommand, KakouneSideEffects,
    ObservationEffects, PluginId, ProcessCapableEffects, ProcessCommand, StateUpdates, StdinMode,
};
use kasane_core::protocol::KasaneRequest;
use kasane_core::session::SessionCommand as CoreSessionCommand;
use kasane_core::state::DirtyFlags;

use super::{wit_key_event_to_key_event, wit_scroll_plan_to_scroll_plan};

pub(crate) fn wit_command_to_command(wc: &wit::Command) -> Command {
    match wc {
        wit::Command::SendKeys(keys) => Command::SendToKakoune(KasaneRequest::Keys(keys.clone())),
        wit::Command::EvalCommand(cmd) => Command::kakoune_command(cmd),
        wit::Command::PasteClipboard => Command::PasteClipboard,
        wit::Command::Quit => Command::Quit,
        wit::Command::RequestRedraw(bits) => {
            Command::RequestRedraw(DirtyFlags::from_bits_truncate(*bits))
        }
        wit::Command::SetConfig(entry) => Command::SetConfig {
            key: entry.key.clone(),
            value: entry.value.clone(),
        },
        wit::Command::ScheduleTimer(tc) => Command::ScheduleTimer {
            timer_id: tc.timer_id,
            delay: Duration::from_millis(tc.delay_ms),
            target: PluginId(tc.target_plugin.clone()),
            payload: Box::new(tc.payload.clone()),
        },
        wit::Command::CancelTimer(timer_id) => Command::CancelTimer {
            timer_id: *timer_id,
        },
        wit::Command::PluginMessage(mc) => Command::PluginMessage {
            target: PluginId(mc.target_plugin.clone()),
            payload: Box::new(mc.payload.clone()),
        },
        wit::Command::RegisterSurface(_) => {
            unreachable!("register-surface commands require adapter context")
        }
        wit::Command::SetSetting(_) => {
            unreachable!("set-setting commands require adapter context for plugin_id")
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
                .map(|t| (t.token.clone(), super::wit_style_to_style(&t.style)))
                .collect(),
        ),
        wit::Command::SpawnPaneClient(config) => Command::SpawnPaneClient {
            pane_key: config.pane_key.clone(),
            placement: super::wit_surface_placement_to_placement(&config.placement),
        },
        wit::Command::ClosePaneClient(key) => Command::ClosePaneClient {
            pane_key: key.clone(),
        },
        wit::Command::HttpRequest(config) => Command::HttpRequest {
            job_id: config.job_id,
            config: wit_http_request_config_to_config(config),
        },
        wit::Command::CancelHttpRequest(job_id) => Command::CancelHttpRequest { job_id: *job_id },
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

fn wit_http_request_config_to_config(
    config: &wit::HttpRequestConfig,
) -> kasane_core::plugin::HttpRequestConfig {
    use kasane_core::plugin::{HttpMethod, StreamingMode};
    kasane_core::plugin::HttpRequestConfig {
        url: config.url.clone(),
        method: match config.method {
            wit::HttpMethod::Get => HttpMethod::Get,
            wit::HttpMethod::Post => HttpMethod::Post,
            wit::HttpMethod::Put => HttpMethod::Put,
            wit::HttpMethod::Delete => HttpMethod::Delete,
            wit::HttpMethod::Patch => HttpMethod::Patch,
            wit::HttpMethod::Head => HttpMethod::Head,
        },
        headers: config.headers.clone(),
        body: config.body.clone(),
        timeout_ms: config.timeout_ms,
        idle_timeout_ms: config.idle_timeout_ms,
        streaming: match config.streaming {
            wit::StreamingMode::Buffered => StreamingMode::Buffered,
            wit::StreamingMode::Chunked => StreamingMode::Chunked,
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

/// Lift a tier-1 wire command (ADR-044) into the broader `wit::Command`
/// enum. The tier-1 variant is a strict subset of `wit::Command`, so
/// the projection is total. Adapter code routes the lifted command
/// through its existing `convert_command` so adapter-context-dependent
/// rewrites (`set-setting`, `register-surface`, `command-error`
/// `eval-command` wrapping) apply uniformly.
pub(crate) fn wit_kakoune_side_command_to_wit_command(
    wc: &wit::KakouneSideCommand,
) -> wit::Command {
    match wc {
        wit::KakouneSideCommand::SendKeys(keys) => wit::Command::SendKeys(keys.clone()),
        wit::KakouneSideCommand::EvalCommand(cmd) => wit::Command::EvalCommand(cmd.clone()),
        wit::KakouneSideCommand::PasteClipboard => wit::Command::PasteClipboard,
        wit::KakouneSideCommand::Quit => wit::Command::Quit,
        wit::KakouneSideCommand::RequestRedraw(bits) => wit::Command::RequestRedraw(*bits),
        wit::KakouneSideCommand::SetConfig(entry) => wit::Command::SetConfig(entry.clone()),
        wit::KakouneSideCommand::SetSetting(entry) => wit::Command::SetSetting(entry.clone()),
        wit::KakouneSideCommand::ScheduleTimer(tc) => wit::Command::ScheduleTimer(tc.clone()),
        wit::KakouneSideCommand::CancelTimer(timer_id) => wit::Command::CancelTimer(*timer_id),
        wit::KakouneSideCommand::PluginMessage(mc) => wit::Command::PluginMessage(mc.clone()),
        wit::KakouneSideCommand::RegisterSurface(cfg) => wit::Command::RegisterSurface(cfg.clone()),
        wit::KakouneSideCommand::UnregisterSurface(key) => {
            wit::Command::UnregisterSurface(key.clone())
        }
        wit::KakouneSideCommand::EditBuffer(edits) => wit::Command::EditBuffer(edits.clone()),
        wit::KakouneSideCommand::InjectKey(key) => wit::Command::InjectKey(*key),
        wit::KakouneSideCommand::RegisterThemeTokens(tokens) => {
            wit::Command::RegisterThemeTokens(tokens.clone())
        }
    }
}

/// Lift a tier-2 wire process command (ADR-044) into the broader
/// `wit::Command` enum. Like the tier-1 lift, this is total because
/// `process-command` is a strict subset of `wit::Command`. The adapter
/// routes the lifted command through `convert_command` for uniform
/// attribution / rewrite behaviour.
pub(crate) fn wit_process_command_to_wit_command(wc: &wit::ProcessCommand) -> wit::Command {
    match wc {
        wit::ProcessCommand::SpawnProcess(cfg) => wit::Command::SpawnProcess(cfg.clone()),
        wit::ProcessCommand::SpawnSession(cfg) => wit::Command::SpawnSession(cfg.clone()),
        wit::ProcessCommand::CloseSession(key) => wit::Command::CloseSession(key.clone()),
        wit::ProcessCommand::SwitchSession(key) => wit::Command::SwitchSession(key.clone()),
        wit::ProcessCommand::WriteToProcess(cfg) => wit::Command::WriteToProcess(cfg.clone()),
        wit::ProcessCommand::CloseProcessStdin(job) => wit::Command::CloseProcessStdin(*job),
        wit::ProcessCommand::KillProcess(job) => wit::Command::KillProcess(*job),
        wit::ProcessCommand::ResizePty(cfg) => wit::Command::ResizePty(*cfg),
        wit::ProcessCommand::SpawnPaneClient(cfg) => wit::Command::SpawnPaneClient(cfg.clone()),
        wit::ProcessCommand::ClosePaneClient(key) => wit::Command::ClosePaneClient(key.clone()),
        wit::ProcessCommand::WorkspaceCommand(cmd) => wit::Command::WorkspaceCommand(*cmd),
        wit::ProcessCommand::HttpRequest(cfg) => wit::Command::HttpRequest(cfg.clone()),
        wit::ProcessCommand::CancelHttpRequest(job) => wit::Command::CancelHttpRequest(*job),
    }
}

/// Convert tier-2 process-capable wire effects (ADR-044) into the
/// unified [`Effects`] struct via a caller-supplied command projector.
/// `process-capable-effects.base` carries the tier-1 surface and
/// `process-commands` carries the process-side surface; both are
/// projected through the same `convert_command` closure so adapter-
/// context rewrites (`set-setting`, `register-surface`,
/// `command-error` wrap) apply uniformly.
pub(crate) fn wit_process_capable_effects_to_effects_with(
    effects: &wit::ProcessCapableEffects,
    mut convert_command: impl FnMut(&wit::Command) -> Vec<Command>,
) -> Effects {
    let base = &effects.base;
    let mut commands: Vec<Command> = base
        .commands
        .iter()
        .flat_map(|c| convert_command(&wit_kakoune_side_command_to_wit_command(c)))
        .collect();
    commands.extend(
        effects
            .process_commands
            .iter()
            .flat_map(|c| convert_command(&wit_process_command_to_wit_command(c))),
    );
    Effects {
        redraw: DirtyFlags::from_bits_truncate(base.redraw),
        commands,
        scroll_plans: base
            .scroll_plans
            .iter()
            .map(wit_scroll_plan_to_scroll_plan)
            .collect(),
        state_updates: kasane_core::plugin::StateUpdates::default(),
    }
}

/// Convert tier-1 wire effects (ADR-044) into the broader [`Effects`]
/// struct via a caller-supplied command projector.
pub(crate) fn wit_kakoune_side_effects_to_effects_with(
    effects: &wit::KakouneSideEffects,
    mut convert_command: impl FnMut(&wit::Command) -> Vec<Command>,
) -> Effects {
    Effects {
        redraw: DirtyFlags::from_bits_truncate(effects.redraw),
        commands: effects
            .commands
            .iter()
            .flat_map(|c| convert_command(&wit_kakoune_side_command_to_wit_command(c)))
            .collect(),
        scroll_plans: effects
            .scroll_plans
            .iter()
            .map(wit_scroll_plan_to_scroll_plan)
            .collect(),
        state_updates: kasane_core::plugin::StateUpdates::default(),
    }
}

pub(crate) fn wit_bootstrap_effects_to_effects(effects: &wit::BootstrapEffects) -> Effects {
    Effects {
        redraw: DirtyFlags::from_bits_truncate(effects.redraw),
        ..Effects::default()
    }
}

/// Tier-1-typed projection of [`wit::BootstrapEffects`].
///
/// Bootstrap carries no commands, so the projection is just the redraw
/// bits lifted into [`KakouneSideEffects`]. Used by the
/// `Plugin::register` path on `WasmPlugin` where the typed setter
/// `on_init_tier1` requires `Into<KakouneSideEffects>`.
pub(crate) fn wit_bootstrap_effects_to_kakoune_side_effects(
    effects: &wit::BootstrapEffects,
) -> KakouneSideEffects {
    KakouneSideEffects::from(ObservationEffects {
        redraw: DirtyFlags::from_bits_truncate(effects.redraw),
        scroll_plans: Vec::new(),
        state_updates: StateUpdates::default(),
    })
}

/// Tier-1-typed counterpart of [`wit_kakoune_side_effects_to_effects_with`].
///
/// Re-tags the projector's [`Vec<Command>`] output as
/// [`Vec<KakouneSideCommand>`] via
/// [`KakouneSideCommand::from_command_unchecked`]. The wire-side
/// `wit::KakouneSideEffects` already enforces Tier-1 narrowness, so the
/// "unchecked" wrap is sound at this boundary. Used by the
/// `Plugin::register` path on `WasmPlugin`.
pub(crate) fn wit_kakoune_side_effects_to_kakoune_side_effects_with(
    effects: &wit::KakouneSideEffects,
    mut convert_command: impl FnMut(&wit::Command) -> Vec<Command>,
) -> KakouneSideEffects {
    let commands: Vec<KakouneSideCommand> = effects
        .commands
        .iter()
        .flat_map(|c| convert_command(&wit_kakoune_side_command_to_wit_command(c)))
        .map(KakouneSideCommand::from_command_unchecked)
        .collect();
    let mut typed = KakouneSideEffects::from(ObservationEffects {
        redraw: DirtyFlags::from_bits_truncate(effects.redraw),
        scroll_plans: effects
            .scroll_plans
            .iter()
            .map(wit_scroll_plan_to_scroll_plan)
            .collect(),
        state_updates: StateUpdates::default(),
    });
    for cmd in commands {
        typed.push(cmd);
    }
    typed
}

/// Tier-2-typed counterpart of [`wit_process_capable_effects_to_effects_with`].
///
/// The Tier-1 base is wrapped through
/// [`KakouneSideCommand::from_command_unchecked`] and the Tier-2
/// `process-commands` slice through
/// [`ProcessCommand::from_command_unchecked`]. Both wrappers are sound
/// because the wire variants are Tier-narrow by construction.
pub(crate) fn wit_process_capable_effects_to_process_capable_effects_with(
    effects: &wit::ProcessCapableEffects,
    mut convert_command: impl FnMut(&wit::Command) -> Vec<Command>,
) -> ProcessCapableEffects {
    let base = wit_kakoune_side_effects_to_kakoune_side_effects_with(&effects.base, |c| {
        convert_command(c)
    });
    let process_commands: Vec<ProcessCommand> = effects
        .process_commands
        .iter()
        .flat_map(|c| convert_command(&wit_process_command_to_wit_command(c)))
        .map(ProcessCommand::from_command_unchecked)
        .collect();
    ProcessCapableEffects {
        base,
        process_commands,
    }
}

/// Tier-1-typed projection of [`wit::SessionReadyEffects`].
pub(crate) fn wit_session_ready_effects_to_kakoune_side_effects(
    effects: &wit::SessionReadyEffects,
) -> KakouneSideEffects {
    let mut typed = KakouneSideEffects::from(ObservationEffects {
        redraw: DirtyFlags::from_bits_truncate(effects.redraw),
        scroll_plans: effects
            .scroll_plans
            .iter()
            .map(wit_scroll_plan_to_scroll_plan)
            .collect(),
        state_updates: StateUpdates::default(),
    });
    for command in &effects.commands {
        let cmd = match command {
            wit::SessionReadyCommand::SendKeys(keys) => {
                KakouneSideCommand::send_to_kakoune(KasaneRequest::Keys(keys.clone()))
            }
            wit::SessionReadyCommand::EvalCommand(cmd) => {
                KakouneSideCommand::from_command_unchecked(Command::kakoune_command(cmd))
            }
            wit::SessionReadyCommand::PasteClipboard => KakouneSideCommand::paste_clipboard(),
            wit::SessionReadyCommand::PluginMessage(message) => KakouneSideCommand::plugin_message(
                PluginId(message.target_plugin.clone()),
                Box::new(message.payload.clone()),
            ),
        };
        typed.push(cmd);
    }
    typed
}

pub(crate) fn wit_session_ready_effects_to_effects(effects: &wit::SessionReadyEffects) -> Effects {
    Effects {
        redraw: DirtyFlags::from_bits_truncate(effects.redraw),
        commands: effects
            .commands
            .iter()
            .map(|command| match command {
                wit::SessionReadyCommand::SendKeys(keys) => {
                    Command::SendToKakoune(KasaneRequest::Keys(keys.clone()))
                }
                wit::SessionReadyCommand::EvalCommand(cmd) => Command::kakoune_command(cmd),
                wit::SessionReadyCommand::PasteClipboard => Command::PasteClipboard,
                wit::SessionReadyCommand::PluginMessage(message) => Command::PluginMessage {
                    target: PluginId(message.target_plugin.clone()),
                    payload: Box::new(message.payload.clone()),
                },
            })
            .collect(),
        scroll_plans: effects
            .scroll_plans
            .iter()
            .map(wit_scroll_plan_to_scroll_plan)
            .collect(),
        state_updates: kasane_core::plugin::StateUpdates::default(),
    }
}

#[cfg(test)]
pub(crate) fn wit_kakoune_side_effects_to_effects(effects: &wit::KakouneSideEffects) -> Effects {
    wit_kakoune_side_effects_to_effects_with(effects, |command| {
        vec![wit_command_to_command(command)]
    })
}

#[cfg(test)]
pub(crate) fn wit_process_capable_effects_to_effects(
    effects: &wit::ProcessCapableEffects,
) -> Effects {
    wit_process_capable_effects_to_effects_with(effects, |command| {
        vec![wit_command_to_command(command)]
    })
}
