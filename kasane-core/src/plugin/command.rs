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
use super::setting::SettingValue;

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

pub enum Command {
    SendToKakoune(KasaneRequest),
    InsertText(String),
    PasteClipboard,
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
    /// Set a typed plugin setting at runtime.
    SetSetting {
        plugin_id: PluginId,
        key: String,
        value: SettingValue,
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
    /// Start a registered process task by name.
    ///
    /// The framework looks up the task spec in the plugin's `HandlerTable`,
    /// allocates a job ID, and spawns the process.
    StartProcessTask {
        task_name: String,
    },
    /// Expose a variable to the widget system.
    ///
    /// Widget templates can reference it as `{plugin.<name>}`.
    /// The variable persists in the `PluginVariableStore` until overwritten
    /// or cleared by the owning plugin.
    ExposeVariable {
        name: String,
        value: crate::widget::types::Value,
    },
}

impl Command {
    /// The three variants that write to Kakoune (A9, T10, §9.1).
    pub const KAKOUNE_WRITING_VARIANTS: &'static [&'static str] =
        &["SendToKakoune", "InsertText", "EditBuffer"];

    /// All variant names of this enum.
    pub const ALL_VARIANT_NAMES: &'static [&'static str] = &[
        "BindSurfaceSession",
        "ClosePaneClient",
        "CloseProcessStdin",
        "EditBuffer",
        "ExposeVariable",
        "InjectInput",
        "InsertText",
        "KillProcess",
        "PasteClipboard",
        "PluginMessage",
        "Quit",
        "RegisterSurface",
        "RegisterSurfaceRequested",
        "RegisterThemeTokens",
        "RequestRedraw",
        "ResizePty",
        "ScheduleTimer",
        "SendToKakoune",
        "Session",
        "SetConfig",
        "SetSetting",
        "SpawnPaneClient",
        "SpawnProcess",
        "StartProcessTask",
        "UnbindSurfaceSession",
        "UnregisterSurface",
        "UnregisterSurfaceKey",
        "Workspace",
        "WriteToProcess",
    ];

    /// Convenience: execute a Kakoune command string.
    ///
    /// Wraps the command in the key sequence `<esc>:cmd<ret>` and sends it
    /// as `SendToKakoune(Keys(...))`. This is the native-side equivalent of
    /// `kasane_plugin_sdk::keys::command()`.
    pub fn kakoune_command(cmd: &str) -> Self {
        let mut keys = vec!["<esc>".to_string(), ":".to_string()];
        for c in cmd.chars() {
            match c {
                '<' => keys.push("<lt>".to_string()),
                '>' => keys.push("<gt>".to_string()),
                ' ' => keys.push("<space>".to_string()),
                '-' => keys.push("<minus>".to_string()),
                '\n' => keys.push("<ret>".to_string()),
                c => keys.push(c.to_string()),
            }
        }
        keys.push("<ret>".to_string());
        Command::SendToKakoune(KasaneRequest::Keys(keys))
    }

    /// Convenience: insert literal text into Kakoune.
    pub fn insert_text(text: impl Into<String>) -> Self {
        Command::InsertText(text.into())
    }

    /// Returns true if this command writes to Kakoune.
    ///
    /// Exhaustive match ensures new variants force explicit classification.
    pub const fn is_kakoune_writing(&self) -> bool {
        match self {
            Command::SendToKakoune(_) => true,
            Command::InsertText(_) => true,
            Command::EditBuffer { .. } => true,
            Command::PasteClipboard => false,
            Command::Quit => false,
            Command::RequestRedraw(_) => false,
            Command::ScheduleTimer { .. } => false,
            Command::PluginMessage { .. } => false,
            Command::SetConfig { .. } => false,
            Command::SetSetting { .. } => false,
            Command::Workspace(_) => false,
            Command::RegisterSurface { .. } => false,
            Command::RegisterSurfaceRequested { .. } => false,
            Command::UnregisterSurface { .. } => false,
            Command::UnregisterSurfaceKey { .. } => false,
            Command::RegisterThemeTokens(_) => false,
            Command::SpawnProcess { .. } => false,
            Command::Session(_) => false,
            Command::WriteToProcess { .. } => false,
            Command::CloseProcessStdin { .. } => false,
            Command::KillProcess { .. } => false,
            Command::ResizePty { .. } => false,
            Command::InjectInput(_) => false,
            Command::SpawnPaneClient { .. } => false,
            Command::ClosePaneClient { .. } => false,
            Command::BindSurfaceSession { .. } => false,
            Command::UnbindSurfaceSession { .. } => false,
            Command::StartProcessTask { .. } => false,
            Command::ExposeVariable { .. } => false,
        }
    }

    /// Returns true if this command commutes with other commands of the same kind.
    ///
    /// Commutative commands can be deduplicated or reordered without affecting
    /// the final result. Exhaustive match ensures new variants force explicit
    /// classification.
    pub fn is_commutative(&self) -> bool {
        match self {
            Command::RequestRedraw(_) => true,
            Command::RegisterThemeTokens(_) => true,
            Command::SetConfig { .. } => true,
            Command::SetSetting { .. } => true,
            Command::SendToKakoune(_) => false,
            Command::InsertText(_) => false,
            Command::PasteClipboard => false,
            Command::Quit => false,
            Command::ScheduleTimer { .. } => false,
            Command::PluginMessage { .. } => false,
            Command::Workspace(_) => false,
            Command::RegisterSurface { .. } => false,
            Command::RegisterSurfaceRequested { .. } => false,
            Command::UnregisterSurface { .. } => false,
            Command::UnregisterSurfaceKey { .. } => false,
            Command::SpawnProcess { .. } => false,
            Command::Session(_) => false,
            Command::WriteToProcess { .. } => false,
            Command::CloseProcessStdin { .. } => false,
            Command::KillProcess { .. } => false,
            Command::ResizePty { .. } => false,
            Command::EditBuffer { .. } => false,
            Command::InjectInput(_) => false,
            Command::SpawnPaneClient { .. } => false,
            Command::ClosePaneClient { .. } => false,
            Command::BindSurfaceSession { .. } => false,
            Command::UnbindSurfaceSession { .. } => false,
            Command::StartProcessTask { .. } => false,
            Command::ExposeVariable { .. } => false,
        }
    }

    /// Returns true if this command requires event-loop-level handling
    /// (timers, inter-plugin messages, config, workspace, processes, sessions).
    /// Exhaustive match ensures new variants force explicit classification.
    pub fn is_deferred(&self) -> bool {
        match self {
            Command::SendToKakoune(_) => false,
            Command::InsertText(_) => false,
            Command::PasteClipboard => false,
            Command::Quit => false,
            Command::RequestRedraw(_) => false,
            Command::EditBuffer { .. } => false,
            Command::ScheduleTimer { .. } => true,
            Command::PluginMessage { .. } => true,
            Command::SetConfig { .. } => true,
            Command::SetSetting { .. } => true,
            Command::Workspace(_) => true,
            Command::RegisterSurface { .. } => true,
            Command::RegisterSurfaceRequested { .. } => true,
            Command::UnregisterSurface { .. } => true,
            Command::UnregisterSurfaceKey { .. } => true,
            Command::RegisterThemeTokens(_) => true,
            Command::SpawnProcess { .. } => true,
            Command::Session(_) => true,
            Command::WriteToProcess { .. } => true,
            Command::CloseProcessStdin { .. } => true,
            Command::KillProcess { .. } => true,
            Command::ResizePty { .. } => true,
            Command::InjectInput(_) => true,
            Command::SpawnPaneClient { .. } => true,
            Command::ClosePaneClient { .. } => true,
            Command::BindSurfaceSession { .. } => true,
            Command::UnbindSurfaceSession { .. } => true,
            Command::StartProcessTask { .. } => true,
            Command::ExposeVariable { .. } => true,
        }
    }

    /// Returns the variant name as a string (for classification tests).
    pub fn variant_name(&self) -> &'static str {
        match self {
            Command::SendToKakoune(_) => "SendToKakoune",
            Command::InsertText(_) => "InsertText",
            Command::PasteClipboard => "PasteClipboard",
            Command::Quit => "Quit",
            Command::RequestRedraw(_) => "RequestRedraw",
            Command::ScheduleTimer { .. } => "ScheduleTimer",
            Command::PluginMessage { .. } => "PluginMessage",
            Command::SetConfig { .. } => "SetConfig",
            Command::SetSetting { .. } => "SetSetting",
            Command::Workspace(_) => "Workspace",
            Command::RegisterSurface { .. } => "RegisterSurface",
            Command::RegisterSurfaceRequested { .. } => "RegisterSurfaceRequested",
            Command::UnregisterSurface { .. } => "UnregisterSurface",
            Command::UnregisterSurfaceKey { .. } => "UnregisterSurfaceKey",
            Command::RegisterThemeTokens(_) => "RegisterThemeTokens",
            Command::SpawnProcess { .. } => "SpawnProcess",
            Command::Session(_) => "Session",
            Command::WriteToProcess { .. } => "WriteToProcess",
            Command::CloseProcessStdin { .. } => "CloseProcessStdin",
            Command::KillProcess { .. } => "KillProcess",
            Command::ResizePty { .. } => "ResizePty",
            Command::EditBuffer { .. } => "EditBuffer",
            Command::InjectInput(_) => "InjectInput",
            Command::SpawnPaneClient { .. } => "SpawnPaneClient",
            Command::ClosePaneClient { .. } => "ClosePaneClient",
            Command::BindSurfaceSession { .. } => "BindSurfaceSession",
            Command::UnbindSurfaceSession { .. } => "UnbindSurfaceSession",
            Command::StartProcessTask { .. } => "StartProcessTask",
            Command::ExposeVariable { .. } => "ExposeVariable",
        }
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
    let _ = clipboard;

    for cmd in commands {
        match cmd {
            Command::SendToKakoune(req) => {
                crate::io::send_request(kak_writer, &req);
            }
            Command::InsertText(text) => {
                let keys = escape_kakoune_insert_text(&text);
                if !keys.is_empty() {
                    crate::io::send_request(kak_writer, &KasaneRequest::Keys(keys));
                }
            }
            Command::PasteClipboard => {
                // PasteClipboard is intercepted by handle_command_batch_inner and
                // apply_ready_batch before reaching execute_commands. This arm is
                // kept as a defensive fallback.
                debug_assert!(
                    false,
                    "PasteClipboard should be intercepted before execute_commands"
                );
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
            | Command::SetSetting { .. }
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
            | Command::UnbindSurfaceSession { .. }
            | Command::StartProcessTask { .. }
            | Command::ExposeVariable { .. } => {}
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
