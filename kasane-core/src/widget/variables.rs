//! Variable resolution for widget templates and conditions.

use compact_str::CompactString;

use crate::plugin::AppView;
use crate::plugin::variable_store::PluginVariableStore;
use crate::protocol::{CursorMode, StatusStyle};
use crate::state::DirtyFlags;
use crate::state::derived::EditorMode;

use super::types::Value;

/// Trait for resolving variable names to typed values.
pub trait VariableResolver {
    fn resolve(&self, name: &str) -> Value;
}

/// Resolves variables from the current application state.
pub struct AppViewResolver<'a> {
    app: &'a AppView<'a>,
    plugin_store: Option<&'a PluginVariableStore>,
}

impl<'a> AppViewResolver<'a> {
    pub fn new(app: &'a AppView<'a>) -> Self {
        Self {
            app,
            plugin_store: None,
        }
    }

    pub fn with_plugin_store(app: &'a AppView<'a>, store: &'a PluginVariableStore) -> Self {
        Self {
            app,
            plugin_store: Some(store),
        }
    }
}

impl VariableResolver for AppViewResolver<'_> {
    fn resolve(&self, name: &str) -> Value {
        match name {
            "cursor_line" => Value::Int(self.app.cursor_line() as i64 + 1),
            "cursor_col" => Value::Int(self.app.cursor_col() as i64 + 1),
            "cursor_count" => Value::Int(self.app.cursor_count() as i64),
            "editor_mode" => {
                Value::Str(CompactString::from(editor_mode_str(self.app.editor_mode())))
            }
            "line_count" => Value::Int(self.app.line_count() as i64),
            "is_focused" => Value::Bool(self.app.focused()),
            "cols" => Value::Int(self.app.cols() as i64),
            "rows" => Value::Int(self.app.rows() as i64),
            "has_menu" => Value::Bool(self.app.has_menu()),
            "has_info" => Value::Bool(self.app.has_info()),
            "is_prompt" => Value::Bool(self.app.is_prompt_mode()),
            "status_style" => Value::Str(CompactString::from(status_style_str(
                self.app.status_style(),
            ))),
            "cursor_mode" => {
                Value::Str(CompactString::from(cursor_mode_str(self.app.cursor_mode())))
            }
            "is_dark" => Value::Bool(self.app.is_dark_background()),
            "session_count" => Value::Int(self.app.session_descriptors().len() as i64),
            "active_session" => self
                .app
                .active_session_key()
                .map(|s| Value::Str(CompactString::from(s)))
                .unwrap_or(Value::Empty),
            // Aliases
            "filetype" => self.resolve("opt.filetype"),
            "bufname" => self.resolve("opt.bufname"),
            name if name.starts_with("opt.") => self
                .app
                .ui_options()
                .get(&name[4..])
                .map(|v| parse_option_value(v.as_str()))
                .unwrap_or(Value::Empty),
            name if name.starts_with("plugin.") => self
                .plugin_store
                .and_then(|store| store.get(&name[7..]))
                .cloned()
                .unwrap_or(Value::Empty),
            _ => Value::Empty,
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

/// Parse an option string into a typed `Value`.
///
/// - `"true"` / `"false"` → `Value::Bool`
/// - Integer strings → `Value::Int`
/// - Everything else → `Value::Str`
fn parse_option_value(s: &str) -> Value {
    match s {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => {
            if let Ok(n) = s.parse::<i64>() {
                Value::Int(n)
            } else {
                Value::Str(CompactString::from(s))
            }
        }
    }
}

/// Scope of a variable (where it can be resolved).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableScope {
    /// Available in all widget contexts.
    Global,
    /// Only available in per-line contexts (gutter widgets).
    PerLine,
}

/// Metadata for a single built-in variable.
#[derive(Debug, Clone)]
pub struct VariableDefinition {
    pub name: &'static str,
    pub dirty_flag: DirtyFlags,
    pub scope: VariableScope,
}

/// Registry of known variable names, their dirty flags, and scopes.
///
/// Centralizes variable metadata that was previously spread across
/// `KNOWN_VARIABLES`, `LINE_VARIABLES`, and `variable_dirty_flag()`.
pub struct VariableRegistry {
    builtins: Vec<VariableDefinition>,
}

impl VariableRegistry {
    /// Create a registry with all built-in variables.
    pub fn new() -> Self {
        Self {
            builtins: vec![
                VariableDefinition {
                    name: "cursor_line",
                    dirty_flag: DirtyFlags::BUFFER_CURSOR,
                    scope: VariableScope::Global,
                },
                VariableDefinition {
                    name: "cursor_col",
                    dirty_flag: DirtyFlags::BUFFER_CURSOR,
                    scope: VariableScope::Global,
                },
                VariableDefinition {
                    name: "cursor_count",
                    dirty_flag: DirtyFlags::BUFFER_CURSOR,
                    scope: VariableScope::Global,
                },
                VariableDefinition {
                    name: "editor_mode",
                    dirty_flag: DirtyFlags::STATUS,
                    scope: VariableScope::Global,
                },
                VariableDefinition {
                    name: "line_count",
                    dirty_flag: DirtyFlags::BUFFER_CONTENT,
                    scope: VariableScope::Global,
                },
                VariableDefinition {
                    name: "is_focused",
                    dirty_flag: DirtyFlags::BUFFER_CONTENT,
                    scope: VariableScope::Global,
                },
                VariableDefinition {
                    name: "cols",
                    dirty_flag: DirtyFlags::BUFFER_CONTENT,
                    scope: VariableScope::Global,
                },
                VariableDefinition {
                    name: "rows",
                    dirty_flag: DirtyFlags::BUFFER_CONTENT,
                    scope: VariableScope::Global,
                },
                VariableDefinition {
                    name: "has_menu",
                    dirty_flag: DirtyFlags::MENU_STRUCTURE,
                    scope: VariableScope::Global,
                },
                VariableDefinition {
                    name: "has_info",
                    dirty_flag: DirtyFlags::INFO,
                    scope: VariableScope::Global,
                },
                VariableDefinition {
                    name: "is_prompt",
                    dirty_flag: DirtyFlags::STATUS,
                    scope: VariableScope::Global,
                },
                VariableDefinition {
                    name: "status_style",
                    dirty_flag: DirtyFlags::STATUS,
                    scope: VariableScope::Global,
                },
                VariableDefinition {
                    name: "cursor_mode",
                    dirty_flag: DirtyFlags::BUFFER_CURSOR,
                    scope: VariableScope::Global,
                },
                VariableDefinition {
                    name: "is_dark",
                    dirty_flag: DirtyFlags::OPTIONS,
                    scope: VariableScope::Global,
                },
                VariableDefinition {
                    name: "session_count",
                    dirty_flag: DirtyFlags::SESSION,
                    scope: VariableScope::Global,
                },
                VariableDefinition {
                    name: "active_session",
                    dirty_flag: DirtyFlags::SESSION,
                    scope: VariableScope::Global,
                },
                VariableDefinition {
                    name: "filetype",
                    dirty_flag: DirtyFlags::OPTIONS,
                    scope: VariableScope::Global,
                },
                VariableDefinition {
                    name: "bufname",
                    dirty_flag: DirtyFlags::OPTIONS,
                    scope: VariableScope::Global,
                },
                // Per-line variables
                VariableDefinition {
                    name: "line_number",
                    dirty_flag: DirtyFlags::BUFFER_CURSOR,
                    scope: VariableScope::PerLine,
                },
                VariableDefinition {
                    name: "relative_line",
                    dirty_flag: DirtyFlags::BUFFER_CURSOR,
                    scope: VariableScope::PerLine,
                },
                VariableDefinition {
                    name: "is_cursor_line",
                    dirty_flag: DirtyFlags::BUFFER_CURSOR,
                    scope: VariableScope::PerLine,
                },
            ],
        }
    }

    /// Look up the dirty flag for a variable name.
    pub fn dirty_flag(&self, name: &str) -> DirtyFlags {
        if let Some(def) = self.builtins.iter().find(|d| d.name == name) {
            return def.dirty_flag;
        }
        if name.starts_with("opt.") {
            return DirtyFlags::OPTIONS;
        }
        if name.starts_with("plugin.") {
            // Plugin variables can change on any state update, so use a broad flag.
            return DirtyFlags::BUFFER_CONTENT;
        }
        DirtyFlags::BUFFER_CONTENT
    }

    /// Validate a variable name.
    ///
    /// Returns `None` if the variable is valid, or `Some(warning_message)` if unknown.
    /// `line_context` should be `true` for gutter widgets where per-line variables are valid.
    pub fn validate(&self, name: &str, line_context: bool) -> Option<String> {
        if name.starts_with("opt.") || name.starts_with("plugin.") {
            return None;
        }

        if let Some(def) = self.builtins.iter().find(|d| d.name == name) {
            if def.scope == VariableScope::PerLine && !line_context {
                return Some(format!(
                    "variable '{name}' is only available in gutter widgets (kind=\"gutter\")"
                ));
            }
            return None;
        }

        // Fuzzy match
        let candidates: Vec<&str> = self
            .builtins
            .iter()
            .filter(|d| line_context || d.scope == VariableScope::Global)
            .map(|d| d.name)
            .collect();

        let mut best: Option<(&str, usize)> = None;
        for &known in &candidates {
            let dist = edit_distance(name, known);
            if dist <= 3 && (best.is_none() || dist < best.unwrap().1) {
                best = Some((known, dist));
            }
        }

        if let Some((suggestion, _)) = best {
            Some(format!(
                "unknown variable '{name}', did you mean '{suggestion}'?"
            ))
        } else {
            Some(format!(
                "unknown variable '{name}' (use opt.<name> for Kakoune options)"
            ))
        }
    }

    /// Iterate over all built-in variable definitions.
    pub fn iter(&self) -> impl Iterator<Item = &VariableDefinition> {
        self.builtins.iter()
    }

    /// Return known variable names visible in the given context.
    pub fn known_names(&self, line_context: bool) -> Vec<&str> {
        self.builtins
            .iter()
            .filter(|d| line_context || d.scope == VariableScope::Global)
            .map(|d| d.name)
            .collect()
    }
}

impl Default for VariableRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple Levenshtein edit distance.
pub(crate) fn edit_distance(a: &str, b: &str) -> usize {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let m = a_bytes.len();
    let n = b_bytes.len();

    let mut prev = (0..=n).collect::<Vec<_>>();
    let mut curr = vec![0; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a_bytes[i - 1] == b_bytes[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

/// Singleton registry instance.
///
/// Thread-safe because `VariableRegistry` is immutable after construction.
fn global_registry() -> &'static VariableRegistry {
    use std::sync::LazyLock;
    static REGISTRY: LazyLock<VariableRegistry> = LazyLock::new(VariableRegistry::new);
    &REGISTRY
}

/// Map a variable name to the DirtyFlags it depends on.
pub fn variable_dirty_flag(name: &str) -> DirtyFlags {
    global_registry().dirty_flag(name)
}

/// Validate a variable name and return a warning message if it's unknown.
///
/// Delegates to the global `VariableRegistry`.
pub fn validate_variable(name: &str, line_context: bool) -> Option<String> {
    global_registry().validate(name, line_context)
}

/// A resolver that returns `Value::Empty` for all variables.
///
/// Used when evaluating predicates in a transform context where no
/// variable resolver is available.
pub struct NullResolver;

impl VariableResolver for NullResolver {
    fn resolve(&self, _name: &str) -> Value {
        Value::Empty
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
    fn resolve(&self, name: &str) -> Value {
        match name {
            "line_number" => Value::Int(self.line as i64 + 1),
            "relative_line" => Value::Int(self.line.abs_diff(self.cursor_line) as i64),
            "is_cursor_line" => Value::Bool(self.line == self.cursor_line),
            _ => self.app_resolver.resolve(name),
        }
    }
}
