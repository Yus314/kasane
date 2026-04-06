//! Variable resolution for widget templates and conditions.

use compact_str::CompactString;

use crate::plugin::AppView;
use crate::state::DirtyFlags;
use crate::state::derived::EditorMode;

/// Trait for resolving variable names to string values.
pub trait VariableResolver {
    fn resolve(&self, name: &str) -> CompactString;
}

/// Resolves variables from the current application state.
pub struct AppViewResolver<'a> {
    app: &'a AppView<'a>,
}

impl<'a> AppViewResolver<'a> {
    pub fn new(app: &'a AppView<'a>) -> Self {
        Self { app }
    }
}

impl VariableResolver for AppViewResolver<'_> {
    fn resolve(&self, name: &str) -> CompactString {
        match name {
            "cursor_line" => CompactString::from(format!("{}", self.app.cursor_line() + 1)),
            "cursor_col" => CompactString::from(format!("{}", self.app.cursor_col() + 1)),
            "cursor_count" => CompactString::from(format!("{}", self.app.cursor_count())),
            "editor_mode" => CompactString::from(editor_mode_str(self.app.editor_mode())),
            "line_count" => CompactString::from(format!("{}", self.app.line_count())),
            "is_focused" => {
                if self.app.focused() {
                    CompactString::from("true")
                } else {
                    CompactString::default()
                }
            }
            "cols" => CompactString::from(format!("{}", self.app.cols())),
            "rows" => CompactString::from(format!("{}", self.app.rows())),
            name if name.starts_with("opt.") => self
                .app
                .ui_options()
                .get(&name[4..])
                .map(|v| CompactString::from(v.as_str()))
                .unwrap_or_default(),
            _ => CompactString::default(),
        }
    }
}

fn editor_mode_str(mode: EditorMode) -> &'static str {
    match mode {
        EditorMode::Normal => "normal",
        EditorMode::Insert => "insert",
        EditorMode::Replace => "replace",
        EditorMode::Prompt => "prompt",
        EditorMode::Unknown => "unknown",
    }
}

/// Map a variable name to the DirtyFlags it depends on.
pub fn variable_dirty_flag(name: &str) -> DirtyFlags {
    match name {
        "cursor_line" | "cursor_col" | "cursor_count" => DirtyFlags::BUFFER_CURSOR,
        "editor_mode" => DirtyFlags::STATUS,
        "line_count" | "is_focused" | "cols" | "rows" => DirtyFlags::BUFFER_CONTENT,
        name if name.starts_with("opt.") => DirtyFlags::OPTIONS,
        _ => DirtyFlags::BUFFER_CONTENT,
    }
}
