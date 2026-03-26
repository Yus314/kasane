//! Mode and cursor style inference from protocol state.

use crate::protocol::{CursorMode, Line};
use crate::render::CursorStyle;

use super::EditorMode;

/// Derive the editor mode from cursor mode and status mode line.
///
/// Uses the same heuristic as `derive_cursor_style()` (I-2) but returns
/// a semantic mode enum instead of a cursor shape.
///
/// - `CursorMode::Prompt` → `EditorMode::Prompt`
/// - mode_line contains "insert" → `EditorMode::Insert`
/// - mode_line contains "replace" → `EditorMode::Replace`
/// - otherwise → `EditorMode::Normal`
pub fn derive_editor_mode(cursor_mode: CursorMode, status_mode_line: &Line) -> EditorMode {
    if cursor_mode == CursorMode::Prompt {
        return EditorMode::Prompt;
    }
    status_mode_line
        .iter()
        .find_map(|atom| match atom.contents.as_str() {
            "insert" => Some(EditorMode::Insert),
            "replace" => Some(EditorMode::Replace),
            _ => None,
        })
        .unwrap_or(EditorMode::Normal)
}

/// Derive cursor mode from the status content cursor position.
///
/// # Inference Rule: I-3
/// **Assumption**: `content_cursor_pos >= 0` means Kakoune is in prompt mode
/// (command, search, etc.), while `< 0` means buffer (normal editing) mode.
/// **Failure mode**: If Kakoune changes the sign convention, cursor mode is
/// inverted — prompt commands would be sent to the buffer and vice versa.
/// **Severity**: Degraded (input routing broken)
///
/// `content_cursor_pos >= 0` means prompt mode (`:`, `/`, etc.),
/// `< 0` means buffer (normal editing) mode.
pub fn derive_cursor_mode(content_cursor_pos: i32) -> CursorMode {
    if content_cursor_pos >= 0 {
        CursorMode::Prompt
    } else {
        CursorMode::Buffer
    }
}

/// Derive cursor style from state fields (without plugin override).
///
/// # Inference Rule: I-2
/// **Assumption**: The status mode line contains literal strings "insert" or
/// "replace" to indicate Kakoune's editing mode. Other mode strings (including
/// custom modes) default to Block.
/// **Failure mode**: If Kakoune localizes mode names or changes strings, the
/// wrong cursor shape is displayed.
/// **Severity**: Cosmetic (cursor shape mismatch only)
///
/// Priority:
/// 1. Explicit `kasane_cursor_style` ui_option
/// 2. Unfocused → Outline
/// 3. Prompt mode → Bar
/// 4. Mode line heuristic (`"insert"` → Bar, `"replace"` → Underline)
/// 5. Default → Block
pub fn derive_cursor_style(
    ui_options: &std::collections::HashMap<String, String>,
    focused: bool,
    cursor_mode: CursorMode,
    status_mode_line: &Line,
) -> CursorStyle {
    if let Some(style) = ui_options.get("kasane_cursor_style") {
        return match style.as_str() {
            "bar" => CursorStyle::Bar,
            "underline" => CursorStyle::Underline,
            _ => CursorStyle::Block,
        };
    }
    if !focused {
        return CursorStyle::Outline;
    }
    if cursor_mode == CursorMode::Prompt {
        return CursorStyle::Bar;
    }
    let mode = status_mode_line
        .iter()
        .find_map(|atom| match atom.contents.as_str() {
            "insert" => Some(CursorStyle::Bar),
            "replace" => Some(CursorStyle::Underline),
            _ => None,
        });
    mode.unwrap_or(CursorStyle::Block)
}
