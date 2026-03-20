use std::time::Duration;

use crate::bindings::kasane::plugin::types as wit;
use kasane_core::plugin::{
    BootstrapEffects, Command, PluginId, RuntimeEffects, SessionReadyCommand, SessionReadyEffects,
    StdinMode,
};
use kasane_core::protocol::KasaneRequest;
use kasane_core::session::SessionCommand as CoreSessionCommand;
use kasane_core::state::DirtyFlags;

use super::wit_scroll_plan_to_scroll_plan;

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
        wit::Command::SpawnProcess(cfg) => Command::SpawnProcess {
            job_id: cfg.job_id,
            program: cfg.program.clone(),
            args: cfg.args.clone(),
            stdin_mode: match cfg.stdin_mode {
                wit::StdinMode::NullStdin => StdinMode::Null,
                wit::StdinMode::Piped => StdinMode::Piped,
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
        wit::Command::SwitchSession(key) => {
            Command::Session(CoreSessionCommand::Switch { key: key.clone() })
        }
    }
}

pub(crate) fn wit_commands_to_commands(wcs: &[wit::Command]) -> Vec<Command> {
    wcs.iter().map(wit_command_to_command).collect()
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

pub(crate) fn wit_runtime_effects_to_effects(effects: &wit::RuntimeEffects) -> RuntimeEffects {
    RuntimeEffects {
        redraw: DirtyFlags::from_bits_truncate(effects.redraw),
        commands: wit_commands_to_commands(&effects.commands),
        scroll_plans: effects
            .scroll_plans
            .iter()
            .map(wit_scroll_plan_to_scroll_plan)
            .collect(),
    }
}
