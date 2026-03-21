use std::any::Any;
use std::io::Write;
use std::time::Duration;

use crate::protocol::{Face, KasaneRequest};
use crate::session::SessionCommand;
use crate::state::DirtyFlags;
use crate::surface::SurfaceId;
use crate::workspace::{Placement, WorkspaceCommand};

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
    /// Manage Kakoune sessions owned by the host runtime.
    Session(SessionCommand),
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
    /// Spawn a new pane backed by an independent Kakoune client connection.
    SpawnPaneClient {
        surface_id: SurfaceId,
        placement: Placement,
    },
    /// Close a pane and terminate its Kakoune client connection.
    ClosePaneClient {
        surface_id: SurfaceId,
    },
}

impl Command {
    /// Returns true if this command requires event-loop-level handling
    /// (timers, inter-plugin messages, config, workspace, processes, sessions).
    pub fn is_deferred(&self) -> bool {
        !matches!(
            self,
            Command::SendToKakoune(_) | Command::Paste | Command::Quit | Command::RequestRedraw(_)
        )
    }
}

/// Separate deferred commands from immediate commands.
/// Returns (immediate_commands, deferred_commands).
pub fn partition_commands(commands: Vec<Command>) -> (Vec<Command>, Vec<Command>) {
    commands.into_iter().partition(|cmd| !cmd.is_deferred())
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
            | Command::Workspace(_)
            | Command::RegisterThemeTokens(_)
            | Command::SpawnProcess { .. }
            | Command::Session(_)
            | Command::WriteToProcess { .. }
            | Command::CloseProcessStdin { .. }
            | Command::KillProcess { .. }
            | Command::SpawnPaneClient { .. }
            | Command::ClosePaneClient { .. } => {}
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
