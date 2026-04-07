//! Variable resolution for widget templates and conditions.

use compact_str::CompactString;

use crate::plugin::AppView;
use crate::protocol::{CursorMode, StatusStyle};
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
            "is_focused" => bool_str(self.app.focused()),
            "cols" => CompactString::from(format!("{}", self.app.cols())),
            "rows" => CompactString::from(format!("{}", self.app.rows())),
            // Phase 1D: protocol-derived variables
            "has_menu" => bool_str(self.app.has_menu()),
            "has_info" => bool_str(self.app.has_info()),
            "is_prompt" => bool_str(self.app.is_prompt_mode()),
            "status_style" => CompactString::from(status_style_str(self.app.status_style())),
            "cursor_mode" => CompactString::from(cursor_mode_str(self.app.cursor_mode())),
            "is_dark" => bool_str(self.app.is_dark_background()),
            "session_count" => {
                CompactString::from(format!("{}", self.app.session_descriptors().len()))
            }
            "active_session" => self
                .app
                .active_session_key()
                .map(CompactString::from)
                .unwrap_or_default(),
            // Phase 1E: aliases
            "filetype" => self.resolve("opt.filetype"),
            "bufname" => self.resolve("opt.bufname"),
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

fn bool_str(b: bool) -> CompactString {
    if b {
        CompactString::from("true")
    } else {
        CompactString::default()
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

fn status_style_str(style: StatusStyle) -> &'static str {
    match style {
        StatusStyle::Status => "status",
        StatusStyle::Command => "command",
        StatusStyle::Search => "search",
        StatusStyle::Prompt => "prompt",
    }
}

fn cursor_mode_str(mode: CursorMode) -> &'static str {
    match mode {
        CursorMode::Buffer => "buffer",
        CursorMode::Prompt => "prompt",
    }
}

/// Map a variable name to the DirtyFlags it depends on.
pub fn variable_dirty_flag(name: &str) -> DirtyFlags {
    match name {
        "cursor_line" | "cursor_col" | "cursor_count" => DirtyFlags::BUFFER_CURSOR,
        "editor_mode" | "is_prompt" | "status_style" => DirtyFlags::STATUS,
        "line_count" | "is_focused" | "cols" | "rows" => DirtyFlags::BUFFER_CONTENT,
        "has_menu" => DirtyFlags::MENU_STRUCTURE,
        "has_info" => DirtyFlags::INFO,
        "cursor_mode" => DirtyFlags::BUFFER_CURSOR,
        "is_dark" => DirtyFlags::OPTIONS,
        "session_count" | "active_session" => DirtyFlags::SESSION,
        "filetype" | "bufname" => DirtyFlags::OPTIONS,
        name if name.starts_with("opt.") => DirtyFlags::OPTIONS,
        _ => DirtyFlags::BUFFER_CONTENT,
    }
}

/// Resolver that adds per-line context variables on top of AppViewResolver.
pub struct LineContextResolver<'a> {
    app_resolver: AppViewResolver<'a>,
    line: usize,
    cursor_line: usize,
}

impl<'a> LineContextResolver<'a> {
    pub fn new(app: &'a AppView<'a>, line: usize, cursor_line: usize) -> Self {
        Self {
            app_resolver: AppViewResolver::new(app),
            line,
            cursor_line,
        }
    }
}

impl VariableResolver for LineContextResolver<'_> {
    fn resolve(&self, name: &str) -> CompactString {
        match name {
            "line_number" => CompactString::from(format!("{}", self.line + 1)),
            "relative_line" => {
                CompactString::from(format!("{}", self.line.abs_diff(self.cursor_line)))
            }
            "is_cursor_line" => bool_str(self.line == self.cursor_line),
            _ => self.app_resolver.resolve(name),
        }
    }
}
