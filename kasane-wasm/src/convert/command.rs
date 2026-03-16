use std::time::Duration;

use crate::bindings::kasane::plugin::types as wit;
use kasane_core::plugin::{Command, PluginId, StdinMode};
use kasane_core::protocol::KasaneRequest;
use kasane_core::session::SessionCommand;
use kasane_core::state::DirtyFlags;

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
        wit::Command::SpawnSession(cfg) => Command::Session(SessionCommand::Spawn {
            key: cfg.key.clone(),
            session: cfg.session.clone(),
            args: cfg.args.clone(),
            activate: cfg.activate,
        }),
        wit::Command::CloseSession(key) => {
            Command::Session(SessionCommand::Close { key: key.clone() })
        }
        wit::Command::WriteToProcess(cfg) => Command::WriteToProcess {
            job_id: cfg.job_id,
            data: cfg.data.clone(),
        },
        wit::Command::CloseProcessStdin(job_id) => Command::CloseProcessStdin { job_id: *job_id },
        wit::Command::KillProcess(job_id) => Command::KillProcess { job_id: *job_id },
        wit::Command::SwitchSession(key) => {
            Command::Session(SessionCommand::Switch { key: key.clone() })
        }
    }
}

pub(crate) fn wit_commands_to_commands(wcs: &[wit::Command]) -> Vec<Command> {
    wcs.iter().map(wit_command_to_command).collect()
}
