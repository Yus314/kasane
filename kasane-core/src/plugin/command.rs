use std::any::Any;
use std::io::Write;
use std::time::Duration;

use crate::input::InputEvent;
use crate::protocol::{Face, KasaneRequest};
use crate::session::{SessionCommand, SessionId};
use crate::state::DirtyFlags;
use crate::surface::Surface;
use crate::surface::SurfaceId;
use crate::surface::SurfacePlacementRequest;
use crate::workspace::{Placement, WorkspaceCommand};

use super::PluginId;
use super::io::StdinMode;

/// Buffer edit coordinates in Kakoune's editing coordinate space.
///
/// This type is intentionally separate from `protocol::Coord`.
/// `protocol::Coord` represents observed protocol state in `AppState`;
/// `BufferPosition` represents an outbound editing intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferPosition {
    /// 1-indexed line number in Kakoune coordinate space.
    pub line: u32,
    /// 1-indexed column number in Kakoune coordinate space.
    pub column: u32,
}

/// A structured buffer edit operation.
///
/// The framework translates this into a Kakoune-side editing transaction.
/// Multiple edits in a single `EditBuffer` command are applied in one
/// host-mediated translation pass.
#[derive(Debug, Clone, PartialEq)]
pub struct BufferEdit {
    /// Start position, 1-indexed in Kakoune coordinate space.
    pub start: BufferPosition,
    /// End position, 1-indexed and inclusive in Kakoune coordinate space.
    pub end: BufferPosition,
    /// Replacement text. Empty string means deletion.
    pub replacement: String,
}

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
    /// Register a new plugin-owned surface and place it into the workspace.
    RegisterSurface {
        surface: Box<dyn Surface>,
        placement: Placement,
    },
    /// Register a new plugin-owned surface using a keyed placement request.
    RegisterSurfaceRequested {
        surface: Box<dyn Surface>,
        placement: SurfacePlacementRequest,
    },
    /// Unregister a plugin-owned surface previously created at runtime.
    UnregisterSurface {
        surface_id: SurfaceId,
    },
    /// Unregister a plugin-owned surface by stable surface key.
    UnregisterSurfaceKey {
        surface_key: String,
    },
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
    /// Resize the PTY of a spawned process.
    /// Only valid for processes spawned with `StdinMode::Pty`.
    /// Ignored for piped/null processes.
    ResizePty {
        job_id: u64,
        rows: u16,
        cols: u16,
    },
    /// Apply structured edits to the Kakoune buffer.
    ///
    /// Edits are sorted by position (bottom-up) and applied atomically.
    /// Each edit selects the range [start, end] and replaces it with `replacement`.
    ///
    /// The plugin must not issue conflicting edits (overlapping ranges).
    EditBuffer {
        edits: Vec<BufferEdit>,
    },
    /// Inject a synthetic input event into the event pipeline.
    ///
    /// The event is processed as if it came from the terminal/window system,
    /// going through the full plugin middleware pipeline.
    /// This enables macro playback and programmatic key injection.
    InjectInput(InputEvent),
    /// Spawn a new pane backed by an independent Kakoune client connection.
    SpawnPaneClient {
        pane_key: String,
        placement: Placement,
    },
    /// Close a pane and terminate its Kakoune client connection.
    ClosePaneClient {
        pane_key: String,
    },
    /// Bind a surface to a Kakoune session (low-level).
    ///
    /// Plugins can use this to control the Surface→Session mapping directly.
    /// For the common case of spawning a new pane, prefer `SpawnPaneClient`.
    BindSurfaceSession {
        surface_id: SurfaceId,
        session_id: SessionId,
    },
    /// Unbind a surface from its Kakoune session (low-level).
    UnbindSurfaceSession {
        surface_id: SurfaceId,
    },
}

impl Command {
    /// Returns true if this command requires event-loop-level handling
    /// (timers, inter-plugin messages, config, workspace, processes, sessions).
    pub fn is_deferred(&self) -> bool {
        !matches!(
            self,
            Command::SendToKakoune(_)
                | Command::Paste
                | Command::Quit
                | Command::RequestRedraw(_)
                | Command::EditBuffer { .. }
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
    clipboard: &mut crate::clipboard::SystemClipboard,
) -> CommandResult {
    use crate::input::paste_text_to_keys;

    for cmd in commands {
        match cmd {
            Command::SendToKakoune(req) => {
                crate::io::send_request(kak_writer, &req);
            }
            Command::Paste => {
                if let Some(text) = clipboard.get() {
                    let keys = paste_text_to_keys(&text);
                    if !keys.is_empty() {
                        crate::io::send_request(kak_writer, &KasaneRequest::Keys(keys));
                    }
                }
            }
            Command::EditBuffer { edits } => {
                if !edits.is_empty() {
                    let keys = edits_to_keys(&edits);
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
            | Command::RegisterSurface { .. }
            | Command::RegisterSurfaceRequested { .. }
            | Command::UnregisterSurface { .. }
            | Command::UnregisterSurfaceKey { .. }
            | Command::RegisterThemeTokens(_)
            | Command::SpawnProcess { .. }
            | Command::Session(_)
            | Command::WriteToProcess { .. }
            | Command::CloseProcessStdin { .. }
            | Command::KillProcess { .. }
            | Command::ResizePty { .. }
            | Command::InjectInput(_)
            | Command::SpawnPaneClient { .. }
            | Command::ClosePaneClient { .. }
            | Command::BindSurfaceSession { .. }
            | Command::UnbindSurfaceSession { .. } => {}
        }
    }
    CommandResult::Continue
}

/// Escape text for insertion into Kakoune's insert mode.
///
/// Characters with special meaning in Kakoune's key specification language
/// are translated to their key name equivalents.
pub fn escape_kakoune_insert_text(text: &str) -> Vec<String> {
    text.chars()
        .map(|c| match c {
            '<' => "<lt>".to_string(),
            '>' => "<gt>".to_string(),
            '\n' => "<ret>".to_string(),
            '\t' => "<tab>".to_string(),
            '\x1b' => "<esc>".to_string(),
            ' ' => "<space>".to_string(),
            '-' => "<minus>".to_string(),
            c => c.to_string(),
        })
        .collect()
}

/// Translate structured buffer edits into Kakoune key sequences.
///
/// Edits are sorted bottom-up (highest line first, then highest column first)
/// to ensure earlier edits don't shift the coordinates of later ones.
///
/// # Panics
///
/// This function will panic if called before characterization tests against
/// a real Kakoune instance have validated the translation strategy.
/// See `docs/design-plugin-extensibility.md` §5.3.2 for details.
pub fn edits_to_keys(edits: &[BufferEdit]) -> Vec<String> {
    if edits.is_empty() {
        return vec![];
    }

    let mut sorted: Vec<&BufferEdit> = edits.iter().collect();
    sorted.sort_by(|a, b| {
        b.start
            .line
            .cmp(&a.start.line)
            .then(b.start.column.cmp(&a.start.column))
    });

    let mut keys = Vec::new();

    // Ensure we start in normal mode
    keys.push("<esc>".to_string());

    for edit in &sorted {
        // Select the range: move to start, then extend to end
        // Using Kakoune's goto-line + goto-column + extend
        keys.push(format!("{}g", edit.start.line));
        keys.push(format!("{}l", edit.start.column));

        if edit.start == edit.end && edit.replacement.is_empty() {
            // Zero-width deletion at a point: nothing to delete
            continue;
        }

        if edit.start != edit.end {
            // Select from start to end (inclusive)
            keys.push(format!("{}g", edit.end.line));
            keys.push(format!("{}l", edit.end.column));
        }

        if edit.replacement.is_empty() {
            // Deletion: select then delete
            keys.push("d".to_string());
        } else {
            // Replace: change selection (enters insert mode), type text, exit
            keys.push("c".to_string());
            keys.extend(escape_kakoune_insert_text(&edit.replacement));
            keys.push("<esc>".to_string());
        }
    }

    keys
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
