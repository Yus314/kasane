use std::any::Any;
use std::io::Write;
use std::time::Duration;

use crate::pane::PaneCommand;
use crate::protocol::{Face, KasaneRequest};
use crate::state::DirtyFlags;
use crate::workspace::WorkspaceCommand;

use super::PluginId;
use super::io::StdinMode;

/// A post-paint hook that can modify the CellGrid after the standard paint pass.
///
/// PaintHooks enable plugins to apply custom rendering effects (e.g., highlights,
/// overlays, visual indicators) directly on the cell grid without needing to
/// participate in the Element tree.
pub trait PaintHook: Send {
    /// Unique identifier for this hook (typically `"plugin_id.hook_name"`).
    fn id(&self) -> &str;

    /// DirtyFlags that trigger this hook. The hook is skipped when none of these
    /// flags are set.
    fn deps(&self) -> DirtyFlags;

    /// Optional surface filter. When `Some(id)`, only apply when that surface
    /// was rendered. When `None`, apply on every paint pass.
    fn surface_filter(&self) -> Option<crate::surface::SurfaceId> {
        None
    }

    /// Apply the hook to the cell grid.
    ///
    /// `region` is the rectangular area that was painted (typically the full screen).
    fn apply(
        &self,
        grid: &mut crate::render::CellGrid,
        region: &crate::layout::Rect,
        state: &crate::state::AppState,
    );
}

pub enum Command {
    SendToKakoune(KasaneRequest),
    Paste,
    Quit,
    RequestRedraw(DirtyFlags),
    /// Schedule a timer that fires after `delay`, delivering `payload` to `target` plugin.
    ScheduleTimer {
        delay: Duration,
        target: PluginId,
        payload: Box<dyn Any + Send>,
    },
    /// Send a message directly to another plugin.
    PluginMessage {
        target: PluginId,
        payload: Box<dyn Any + Send>,
    },
    /// Override a configuration value at runtime.
    SetConfig {
        key: String,
        value: String,
    },
    /// Pane management command (split, close, focus, etc.).
    Pane(PaneCommand),
    /// Workspace layout command (add/remove surface, focus, split, float, etc.).
    Workspace(WorkspaceCommand),
    /// Register custom theme tokens with default faces.
    RegisterThemeTokens(Vec<(String, Face)>),
    /// Spawn an external process.
    SpawnProcess {
        job_id: u64,
        program: String,
        args: Vec<String>,
        stdin_mode: StdinMode,
    },
    /// Write data to a spawned process's stdin.
    WriteToProcess {
        job_id: u64,
        data: Vec<u8>,
    },
    /// Close a spawned process's stdin (signals EOF).
    CloseProcessStdin {
        job_id: u64,
    },
    /// Kill a spawned process.
    KillProcess {
        job_id: u64,
    },
}

/// Commands that require event-loop-level handling (timers, inter-plugin messages, config).
pub enum DeferredCommand {
    ScheduleTimer {
        delay: Duration,
        target: PluginId,
        payload: Box<dyn Any + Send>,
    },
    PluginMessage {
        target: PluginId,
        payload: Box<dyn Any + Send>,
    },
    SetConfig {
        key: String,
        value: String,
    },
    Pane(PaneCommand),
    Workspace(WorkspaceCommand),
    RegisterThemeTokens(Vec<(String, Face)>),
    SpawnProcess {
        job_id: u64,
        program: String,
        args: Vec<String>,
        stdin_mode: StdinMode,
    },
    WriteToProcess {
        job_id: u64,
        data: Vec<u8>,
    },
    CloseProcessStdin {
        job_id: u64,
    },
    KillProcess {
        job_id: u64,
    },
}

/// Separate deferred commands from normal commands.
/// Returns (normal_commands, deferred_commands).
pub fn extract_deferred_commands(commands: Vec<Command>) -> (Vec<Command>, Vec<DeferredCommand>) {
    let mut normal = Vec::new();
    let mut deferred = Vec::new();
    for cmd in commands {
        match cmd {
            Command::ScheduleTimer {
                delay,
                target,
                payload,
            } => deferred.push(DeferredCommand::ScheduleTimer {
                delay,
                target,
                payload,
            }),
            Command::PluginMessage { target, payload } => {
                deferred.push(DeferredCommand::PluginMessage { target, payload })
            }
            Command::SetConfig { key, value } => {
                deferred.push(DeferredCommand::SetConfig { key, value })
            }
            Command::Pane(cmd) => deferred.push(DeferredCommand::Pane(cmd)),
            Command::Workspace(cmd) => deferred.push(DeferredCommand::Workspace(cmd)),
            Command::RegisterThemeTokens(tokens) => {
                deferred.push(DeferredCommand::RegisterThemeTokens(tokens))
            }
            Command::SpawnProcess {
                job_id,
                program,
                args,
                stdin_mode,
            } => deferred.push(DeferredCommand::SpawnProcess {
                job_id,
                program,
                args,
                stdin_mode,
            }),
            Command::WriteToProcess { job_id, data } => {
                deferred.push(DeferredCommand::WriteToProcess { job_id, data })
            }
            Command::CloseProcessStdin { job_id } => {
                deferred.push(DeferredCommand::CloseProcessStdin { job_id })
            }
            Command::KillProcess { job_id } => {
                deferred.push(DeferredCommand::KillProcess { job_id })
            }
            other => normal.push(other),
        }
    }
    (normal, deferred)
}

/// コマンド実行の結果。
pub enum CommandResult {
    /// すべてのコマンドを処理した。
    Continue,
    /// Quit コマンドを受信した。
    Quit,
}

/// Side-effect コマンドを実行する。
/// `clipboard_get` はクリップボード読み取りのクロージャ。
pub fn execute_commands(
    commands: Vec<Command>,
    kak_writer: &mut (impl Write + ?Sized),
    clipboard_get: &mut dyn FnMut() -> Option<String>,
) -> CommandResult {
    use crate::input::paste_text_to_keys;

    for cmd in commands {
        match cmd {
            Command::SendToKakoune(req) => {
                crate::io::send_request(kak_writer, &req);
            }
            Command::Paste => {
                if let Some(text) = clipboard_get() {
                    let keys = paste_text_to_keys(&text);
                    if !keys.is_empty() {
                        crate::io::send_request(kak_writer, &KasaneRequest::Keys(keys));
                    }
                }
            }
            Command::Quit => return CommandResult::Quit,
            Command::RequestRedraw(_) => {} // handled earlier by extract_redraw_flags
            // Deferred commands should be extracted before reaching execute_commands
            Command::ScheduleTimer { .. }
            | Command::PluginMessage { .. }
            | Command::SetConfig { .. }
            | Command::Pane(_)
            | Command::Workspace(_)
            | Command::RegisterThemeTokens(_)
            | Command::SpawnProcess { .. }
            | Command::WriteToProcess { .. }
            | Command::CloseProcessStdin { .. }
            | Command::KillProcess { .. } => {}
        }
    }
    CommandResult::Continue
}

/// Extract RequestRedraw commands, merging their flags.
/// Returns the merged DirtyFlags; the input Vec retains only non-redraw commands.
pub fn extract_redraw_flags(commands: &mut Vec<Command>) -> DirtyFlags {
    let mut flags = DirtyFlags::empty();
    commands.retain(|cmd| {
        if let Command::RequestRedraw(f) = cmd {
            flags |= *f;
            false
        } else {
            true
        }
    });
    flags
}
