//! Structured Kakoune command construction (ADR-043, closes #94).
//!
//! Represents a Kakoune command as a Rust value rather than a string, so
//! that plugin code can construct, inspect, and transform commands
//! without re-implementing Kakoune's quoting and escaping rules.
//!
//! # Relationship to other layers
//!
//! - [`crate::kak`] — one-liner string builders. Use when you want a
//!   single command and no programmatic composition.
//! - [`crate::kak_lint`] — compile-time validator for raw command
//!   strings. Use when you have a literal that won't fit the structured
//!   form.
//! - This module — first-class Rust values that round-trip through
//!   [`KakCommand::render`] into the canonical Kakoune syntax. All
//!   escaping is centralized in the renderer; constructors cannot
//!   produce malformed commands.
//!
//! # Quick example
//!
//! ```
//! use kasane_plugin_sdk::kak_cmd::{KakCommand, DeclareUserMode, DefineCommand, Map, Scope};
//!
//! let setup: Vec<KakCommand> = vec![
//!     DeclareUserMode::new("sprout").into(),
//!     DefineCommand::new("bump", "increment-counter")
//!         .override_existing()
//!         .docstring("bump the sprout counter")
//!         .into(),
//!     Map::new(Scope::Global, "sprout", "b", ":bump<ret>")
//!         .docstring("bump")
//!         .into(),
//! ];
//!
//! // Render each as a Kakoune command string:
//! let lines: Vec<String> = setup.iter().map(KakCommand::render).collect();
//! assert!(lines[0].contains("declare-user-mode"));
//! assert!(lines[1].contains("-override"));
//! ```

pub use crate::kak::{OptionKind, Scope};
use crate::kak::escape_arg;

// ---------------------------------------------------------------------------
// KakCommand enum
// ---------------------------------------------------------------------------

/// A Kakoune command, represented structurally.
///
/// Render to canonical Kakoune syntax with [`Self::render`]. Use the
/// per-variant constructor types (`DeclareUserMode::new(…)` etc.) to
/// build, then `.into()` to wrap as a `KakCommand`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KakCommand {
    DeclareUserMode(DeclareUserMode),
    DefineCommand(DefineCommand),
    Map(Map),
    DeclareOption(DeclareOption),
    SetOption(SetOption),
    UnsetOption(UnsetOption),
    EvaluateCommands(EvaluateCommands),
    Hook(Hook),
    Alias(Alias),
    Echo(Echo),
    Info(Info),
    Try(Box<KakCommand>),
}

impl KakCommand {
    /// Render this command as a canonical Kakoune syntax string.
    ///
    /// The output is guaranteed to:
    /// - Use single-quote escaping for all positional arguments.
    /// - Use balanced `%X…Y` delimiters for command bodies, choosing
    ///   the first pair that's balanced in the body
    ///   (`%{…}` → `%[…]` → `%(…)` → `%<…>` → quoted fallback).
    /// - Emit flags only for fields whose builder method was called
    ///   (no superfluous `-hidden false`).
    /// - Be accepted by [`crate::kak_lint`].
    pub fn render(&self) -> String {
        match self {
            KakCommand::DeclareUserMode(c) => c.render(),
            KakCommand::DefineCommand(c) => c.render(),
            KakCommand::Map(c) => c.render(),
            KakCommand::DeclareOption(c) => c.render(),
            KakCommand::SetOption(c) => c.render(),
            KakCommand::UnsetOption(c) => c.render(),
            KakCommand::EvaluateCommands(c) => c.render(),
            KakCommand::Hook(c) => c.render(),
            KakCommand::Alias(c) => c.render(),
            KakCommand::Echo(c) => c.render(),
            KakCommand::Info(c) => c.render(),
            KakCommand::Try(inner) => format!("try {}", wrap_body(&inner.render())),
        }
    }

    /// Wrap any command in `try %[ … ]` for idempotency.
    pub fn wrapped_in_try(self) -> Self {
        KakCommand::Try(Box::new(self))
    }
}

// ---------------------------------------------------------------------------
// declare-user-mode
// ---------------------------------------------------------------------------

/// `declare-user-mode [-hidden] [-docstring '…'] <name>`.
///
/// Kakoune's `declare-user-mode` does **not** accept `-override` —
/// declaring an existing mode is a hard error. Wrap in
/// [`KakCommand::wrapped_in_try`] (or set `try_idempotent`) to get the
/// `try %[ declare-user-mode … ]` idempotent idiom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclareUserMode {
    pub name: String,
    pub hidden: bool,
    pub docstring: Option<String>,
    pub try_idempotent: bool,
}

impl DeclareUserMode {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            hidden: false,
            docstring: None,
            try_idempotent: false,
        }
    }
    pub fn hidden(mut self) -> Self {
        self.hidden = true;
        self
    }
    pub fn docstring(mut self, s: impl Into<String>) -> Self {
        self.docstring = Some(s.into());
        self
    }
    /// Wrap the rendered command in `try %[ … ]`. Use when registering
    /// a user-mode at session-ready where re-declaration must be a no-op.
    pub fn try_idempotent(mut self) -> Self {
        self.try_idempotent = true;
        self
    }
    fn render(&self) -> String {
        let mut out = String::from("declare-user-mode");
        if self.hidden {
            out.push_str(" -hidden");
        }
        if let Some(d) = &self.docstring {
            out.push_str(" -docstring ");
            out.push_str(&escape_arg(d));
        }
        out.push(' ');
        out.push_str(&escape_arg(&self.name));
        if self.try_idempotent {
            format!("try {}", wrap_body(&out))
        } else {
            out
        }
    }
}

impl From<DeclareUserMode> for KakCommand {
    fn from(c: DeclareUserMode) -> Self {
        KakCommand::DeclareUserMode(c)
    }
}

// ---------------------------------------------------------------------------
// define-command
// ---------------------------------------------------------------------------

/// `define-command [-override] [-hidden] [-params N] [-docstring '…'] <name> <body>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefineCommand {
    pub name: String,
    pub body: String,
    pub override_existing: bool,
    pub hidden: bool,
    pub params: Option<u32>,
    pub docstring: Option<String>,
}

impl DefineCommand {
    pub fn new(name: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            body: body.into(),
            override_existing: false,
            hidden: false,
            params: None,
            docstring: None,
        }
    }
    pub fn override_existing(mut self) -> Self {
        self.override_existing = true;
        self
    }
    pub fn hidden(mut self) -> Self {
        self.hidden = true;
        self
    }
    pub fn params(mut self, n: u32) -> Self {
        self.params = Some(n);
        self
    }
    pub fn docstring(mut self, s: impl Into<String>) -> Self {
        self.docstring = Some(s.into());
        self
    }
    fn render(&self) -> String {
        let mut out = String::from("define-command");
        if self.override_existing {
            out.push_str(" -override");
        }
        if self.hidden {
            out.push_str(" -hidden");
        }
        if let Some(n) = self.params {
            out.push_str(&format!(" -params {n}"));
        }
        if let Some(d) = &self.docstring {
            out.push_str(" -docstring ");
            out.push_str(&escape_arg(d));
        }
        out.push(' ');
        out.push_str(&escape_arg(&self.name));
        out.push(' ');
        out.push_str(&wrap_body(&self.body));
        out
    }
}

impl From<DefineCommand> for KakCommand {
    fn from(c: DefineCommand) -> Self {
        KakCommand::DefineCommand(c)
    }
}

// ---------------------------------------------------------------------------
// map
// ---------------------------------------------------------------------------

/// `map <scope> <mode> <key> <action> [-docstring '…']`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Map {
    pub scope: Scope,
    pub mode: String,
    pub key: String,
    pub action: String,
    pub docstring: Option<String>,
}

impl Map {
    pub fn new(
        scope: Scope,
        mode: impl Into<String>,
        key: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        Self {
            scope,
            mode: mode.into(),
            key: key.into(),
            action: action.into(),
            docstring: None,
        }
    }
    pub fn docstring(mut self, s: impl Into<String>) -> Self {
        self.docstring = Some(s.into());
        self
    }
    fn render(&self) -> String {
        let mut out = format!(
            "map {} {} {} {}",
            scope_to_str(self.scope),
            escape_arg(&self.mode),
            escape_arg(&self.key),
            escape_arg(&self.action),
        );
        if let Some(d) = &self.docstring {
            out.push_str(" -docstring ");
            out.push_str(&escape_arg(d));
        }
        out
    }
}

impl From<Map> for KakCommand {
    fn from(c: Map) -> Self {
        KakCommand::Map(c)
    }
}

// ---------------------------------------------------------------------------
// declare-option
// ---------------------------------------------------------------------------

/// `declare-option [-hidden] [-docstring '…'] <kind> <name> <default>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclareOption {
    pub name: String,
    pub kind: OptionKind,
    pub default: String,
    pub hidden: bool,
    pub docstring: Option<String>,
}

impl DeclareOption {
    pub fn new(name: impl Into<String>, kind: OptionKind, default: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind,
            default: default.into(),
            hidden: false,
            docstring: None,
        }
    }
    pub fn hidden(mut self) -> Self {
        self.hidden = true;
        self
    }
    pub fn docstring(mut self, s: impl Into<String>) -> Self {
        self.docstring = Some(s.into());
        self
    }
    fn render(&self) -> String {
        let mut out = String::from("declare-option");
        if self.hidden {
            out.push_str(" -hidden");
        }
        if let Some(d) = &self.docstring {
            out.push_str(" -docstring ");
            out.push_str(&escape_arg(d));
        }
        out.push(' ');
        out.push_str(option_kind_str(self.kind));
        out.push(' ');
        out.push_str(&escape_arg(&self.name));
        out.push(' ');
        out.push_str(&escape_arg(&self.default));
        out
    }
}

impl From<DeclareOption> for KakCommand {
    fn from(c: DeclareOption) -> Self {
        KakCommand::DeclareOption(c)
    }
}

// ---------------------------------------------------------------------------
// set-option / unset-option
// ---------------------------------------------------------------------------

/// `set-option [-add | -remove] <scope> <name> <value>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetOption {
    pub scope: Scope,
    pub name: String,
    pub value: String,
    pub mode: SetMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SetMode {
    #[default]
    Replace,
    Add,
    Remove,
}

impl SetOption {
    pub fn new(scope: Scope, name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            scope,
            name: name.into(),
            value: value.into(),
            mode: SetMode::Replace,
        }
    }
    pub fn add(mut self) -> Self {
        self.mode = SetMode::Add;
        self
    }
    pub fn remove(mut self) -> Self {
        self.mode = SetMode::Remove;
        self
    }
    fn render(&self) -> String {
        let mode_flag = match self.mode {
            SetMode::Replace => "",
            SetMode::Add => " -add",
            SetMode::Remove => " -remove",
        };
        format!(
            "set-option{} {} {} {}",
            mode_flag,
            scope_to_str(self.scope),
            escape_arg(&self.name),
            escape_arg(&self.value),
        )
    }
}

impl From<SetOption> for KakCommand {
    fn from(c: SetOption) -> Self {
        KakCommand::SetOption(c)
    }
}

/// `unset-option <scope> <name>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsetOption {
    pub scope: Scope,
    pub name: String,
}

impl UnsetOption {
    pub fn new(scope: Scope, name: impl Into<String>) -> Self {
        Self {
            scope,
            name: name.into(),
        }
    }
    fn render(&self) -> String {
        format!(
            "unset-option {} {}",
            scope_to_str(self.scope),
            escape_arg(&self.name),
        )
    }
}

impl From<UnsetOption> for KakCommand {
    fn from(c: UnsetOption) -> Self {
        KakCommand::UnsetOption(c)
    }
}

// ---------------------------------------------------------------------------
// evaluate-commands
// ---------------------------------------------------------------------------

/// `evaluate-commands [flags] <body>`.
///
/// `body` is one or more `KakCommand`s joined by `\n`. The full body is
/// wrapped in a balanced `%X…Y` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluateCommands {
    pub body: Vec<KakCommand>,
    pub no_hooks: bool,
    pub draft: bool,
    pub itersel: bool,
    pub verbatim: bool,
    pub client: Option<String>,
    pub buffer: Option<String>,
    pub save_regs: Option<String>,
}

impl EvaluateCommands {
    pub fn new<I, C>(body: I) -> Self
    where
        I: IntoIterator<Item = C>,
        C: Into<KakCommand>,
    {
        Self {
            body: body.into_iter().map(Into::into).collect(),
            no_hooks: false,
            draft: false,
            itersel: false,
            verbatim: false,
            client: None,
            buffer: None,
            save_regs: None,
        }
    }
    pub fn no_hooks(mut self) -> Self {
        self.no_hooks = true;
        self
    }
    pub fn draft(mut self) -> Self {
        self.draft = true;
        self
    }
    pub fn itersel(mut self) -> Self {
        self.itersel = true;
        self
    }
    pub fn verbatim(mut self) -> Self {
        self.verbatim = true;
        self
    }
    pub fn client(mut self, name: impl Into<String>) -> Self {
        self.client = Some(name.into());
        self
    }
    pub fn buffer(mut self, name: impl Into<String>) -> Self {
        self.buffer = Some(name.into());
        self
    }
    pub fn save_regs(mut self, regs: impl Into<String>) -> Self {
        self.save_regs = Some(regs.into());
        self
    }
    fn render(&self) -> String {
        let mut out = String::from("evaluate-commands");
        if self.no_hooks {
            out.push_str(" -no-hooks");
        }
        if self.draft {
            out.push_str(" -draft");
        }
        if self.itersel {
            out.push_str(" -itersel");
        }
        if self.verbatim {
            out.push_str(" -verbatim");
        }
        if let Some(c) = &self.client {
            out.push_str(" -client ");
            out.push_str(&escape_arg(c));
        }
        if let Some(b) = &self.buffer {
            out.push_str(" -buffer ");
            out.push_str(&escape_arg(b));
        }
        if let Some(r) = &self.save_regs {
            out.push_str(" -save-regs ");
            out.push_str(&escape_arg(r));
        }
        let body: String = self
            .body
            .iter()
            .map(KakCommand::render)
            .collect::<Vec<_>>()
            .join("\n");
        out.push(' ');
        out.push_str(&wrap_body(&body));
        out
    }
}

impl From<EvaluateCommands> for KakCommand {
    fn from(c: EvaluateCommands) -> Self {
        KakCommand::EvaluateCommands(c)
    }
}

// ---------------------------------------------------------------------------
// hook
// ---------------------------------------------------------------------------

/// `hook [-group <g>] [-once] [-always] <scope> <event> <regex> <command>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hook {
    pub scope: Scope,
    pub event: String,
    pub regex: String,
    pub command: String,
    pub group: Option<String>,
    pub once: bool,
    pub always: bool,
}

impl Hook {
    pub fn new(
        scope: Scope,
        event: impl Into<String>,
        regex: impl Into<String>,
        command: impl Into<String>,
    ) -> Self {
        Self {
            scope,
            event: event.into(),
            regex: regex.into(),
            command: command.into(),
            group: None,
            once: false,
            always: false,
        }
    }
    pub fn group(mut self, g: impl Into<String>) -> Self {
        self.group = Some(g.into());
        self
    }
    pub fn once(mut self) -> Self {
        self.once = true;
        self
    }
    pub fn always(mut self) -> Self {
        self.always = true;
        self
    }
    fn render(&self) -> String {
        let mut out = String::from("hook");
        if let Some(g) = &self.group {
            out.push_str(" -group ");
            out.push_str(&escape_arg(g));
        }
        if self.once {
            out.push_str(" -once");
        }
        if self.always {
            out.push_str(" -always");
        }
        out.push_str(&format!(
            " {} {} {} {}",
            scope_to_str(self.scope),
            escape_arg(&self.event),
            escape_arg(&self.regex),
            wrap_body(&self.command),
        ));
        out
    }
}

impl From<Hook> for KakCommand {
    fn from(c: Hook) -> Self {
        KakCommand::Hook(c)
    }
}

// ---------------------------------------------------------------------------
// alias
// ---------------------------------------------------------------------------

/// `alias <scope> <new> <old>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alias {
    pub scope: Scope,
    pub new_name: String,
    pub old_name: String,
}

impl Alias {
    pub fn new(scope: Scope, new_name: impl Into<String>, old_name: impl Into<String>) -> Self {
        Self {
            scope,
            new_name: new_name.into(),
            old_name: old_name.into(),
        }
    }
    fn render(&self) -> String {
        format!(
            "alias {} {} {}",
            scope_to_str(self.scope),
            escape_arg(&self.new_name),
            escape_arg(&self.old_name),
        )
    }
}

impl From<Alias> for KakCommand {
    fn from(c: Alias) -> Self {
        KakCommand::Alias(c)
    }
}

// ---------------------------------------------------------------------------
// echo
// ---------------------------------------------------------------------------

/// `echo [-markup] [-debug] [-quoting <q>] <text>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Echo {
    pub text: String,
    pub markup: bool,
    pub debug: bool,
    pub quoting: Option<String>,
}

impl Echo {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            markup: false,
            debug: false,
            quoting: None,
        }
    }
    pub fn markup(mut self) -> Self {
        self.markup = true;
        self
    }
    pub fn debug(mut self) -> Self {
        self.debug = true;
        self
    }
    pub fn quoting(mut self, q: impl Into<String>) -> Self {
        self.quoting = Some(q.into());
        self
    }
    fn render(&self) -> String {
        let mut out = String::from("echo");
        if self.markup {
            out.push_str(" -markup");
        }
        if self.debug {
            out.push_str(" -debug");
        }
        if let Some(q) = &self.quoting {
            out.push_str(" -quoting ");
            out.push_str(&escape_arg(q));
        }
        out.push(' ');
        out.push_str(&escape_arg(&self.text));
        out
    }
}

impl From<Echo> for KakCommand {
    fn from(c: Echo) -> Self {
        KakCommand::Echo(c)
    }
}

// ---------------------------------------------------------------------------
// info
// ---------------------------------------------------------------------------

/// `info [-markup] [-title '…'] [-anchor '…'] [-style '…'] [-placement '…'] <text>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Info {
    pub text: String,
    pub markup: bool,
    pub title: Option<String>,
    pub anchor: Option<String>,
    pub style: Option<String>,
    pub placement: Option<String>,
}

impl Info {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            markup: false,
            title: None,
            anchor: None,
            style: None,
            placement: None,
        }
    }
    pub fn markup(mut self) -> Self {
        self.markup = true;
        self
    }
    pub fn title(mut self, t: impl Into<String>) -> Self {
        self.title = Some(t.into());
        self
    }
    pub fn anchor(mut self, a: impl Into<String>) -> Self {
        self.anchor = Some(a.into());
        self
    }
    pub fn style(mut self, s: impl Into<String>) -> Self {
        self.style = Some(s.into());
        self
    }
    pub fn placement(mut self, p: impl Into<String>) -> Self {
        self.placement = Some(p.into());
        self
    }
    fn render(&self) -> String {
        let mut out = String::from("info");
        if self.markup {
            out.push_str(" -markup");
        }
        if let Some(t) = &self.title {
            out.push_str(" -title ");
            out.push_str(&escape_arg(t));
        }
        if let Some(a) = &self.anchor {
            out.push_str(" -anchor ");
            out.push_str(&escape_arg(a));
        }
        if let Some(s) = &self.style {
            out.push_str(" -style ");
            out.push_str(&escape_arg(s));
        }
        if let Some(p) = &self.placement {
            out.push_str(" -placement ");
            out.push_str(&escape_arg(p));
        }
        out.push(' ');
        out.push_str(&escape_arg(&self.text));
        out
    }
}

impl From<Info> for KakCommand {
    fn from(c: Info) -> Self {
        KakCommand::Info(c)
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn scope_to_str(s: Scope) -> &'static str {
    match s {
        Scope::Global => "global",
        Scope::Buffer => "buffer",
        Scope::Window => "window",
    }
}

fn option_kind_str(k: OptionKind) -> &'static str {
    match k {
        OptionKind::Int => "int",
        OptionKind::Bool => "bool",
        OptionKind::Str => "str",
        OptionKind::Regex => "regex",
        OptionKind::IntList => "int-list",
        OptionKind::StrList => "str-list",
    }
}

/// Wrap a command body in a balanced `%X…Y` block, picking the first
/// pair that isn't unbalanced inside the body. Falls back to single-
/// quoted form if none of the four standard pairs work.
fn wrap_body(body: &str) -> String {
    for (open, close) in [('{', '}'), ('[', ']'), ('(', ')'), ('<', '>')] {
        if is_balanced(body, open, close) {
            return format!("%{open} {body} {close}");
        }
    }
    escape_arg(body)
}

fn is_balanced(s: &str, open: char, close: char) -> bool {
    let mut depth: i32 = 0;
    for ch in s.chars() {
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth -= 1;
            if depth < 0 {
                return false;
            }
        }
    }
    depth == 0
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declare_user_mode_minimal() {
        let cmd: KakCommand = DeclareUserMode::new("sprout").into();
        assert_eq!(cmd.render(), "declare-user-mode 'sprout'");
    }

    #[test]
    fn declare_user_mode_hidden_with_docstring() {
        let cmd: KakCommand = DeclareUserMode::new("sprout")
            .hidden()
            .docstring("sprout root mode")
            .into();
        assert_eq!(
            cmd.render(),
            "declare-user-mode -hidden -docstring 'sprout root mode' 'sprout'"
        );
    }

    #[test]
    fn declare_user_mode_try_idempotent() {
        let cmd: KakCommand = DeclareUserMode::new("sprout").try_idempotent().into();
        assert!(cmd.render().starts_with("try %{"));
        assert!(cmd.render().contains("declare-user-mode 'sprout'"));
    }

    #[test]
    fn define_command_with_override_and_params() {
        let cmd: KakCommand = DefineCommand::new("bump", "increment-counter")
            .override_existing()
            .params(1)
            .docstring("bump")
            .into();
        let out = cmd.render();
        assert!(out.starts_with("define-command -override -params 1 -docstring 'bump' 'bump' %{"));
        assert!(out.ends_with('}'));
        assert!(out.contains("increment-counter"));
    }

    #[test]
    fn define_command_body_with_unbalanced_braces_falls_back() {
        let cmd: KakCommand = DefineCommand::new("c", "echo {").into();
        let out = cmd.render();
        // first balanced pair is `%[..]`
        assert!(out.contains("%["), "got: {out}");
    }

    #[test]
    fn map_basic_and_docstring() {
        let cmd: KakCommand = Map::new(Scope::Global, "sprout", "b", ":bump<ret>")
            .docstring("bump")
            .into();
        assert_eq!(
            cmd.render(),
            "map global 'sprout' 'b' ':bump<ret>' -docstring 'bump'"
        );
    }

    #[test]
    fn declare_option_int_hidden() {
        let cmd: KakCommand = DeclareOption::new("counter", OptionKind::Int, "0")
            .hidden()
            .into();
        assert_eq!(
            cmd.render(),
            "declare-option -hidden int 'counter' '0'"
        );
    }

    #[test]
    fn set_option_modes() {
        let r: KakCommand = SetOption::new(Scope::Global, "x", "1").into();
        let a: KakCommand = SetOption::new(Scope::Buffer, "x", "1").add().into();
        let m: KakCommand = SetOption::new(Scope::Window, "x", "1").remove().into();
        assert_eq!(r.render(), "set-option global 'x' '1'");
        assert_eq!(a.render(), "set-option -add buffer 'x' '1'");
        assert_eq!(m.render(), "set-option -remove window 'x' '1'");
    }

    #[test]
    fn unset_option_simple() {
        let cmd: KakCommand = UnsetOption::new(Scope::Global, "x").into();
        assert_eq!(cmd.render(), "unset-option global 'x'");
    }

    #[test]
    fn evaluate_commands_wraps_body() {
        let cmd: KakCommand = EvaluateCommands::new([
            KakCommand::from(DeclareUserMode::new("sprout")),
            KakCommand::from(Map::new(Scope::Global, "sprout", "b", ":bump<ret>")),
        ])
        .no_hooks()
        .into();
        let out = cmd.render();
        assert!(out.starts_with("evaluate-commands -no-hooks %{"));
        assert!(out.contains("declare-user-mode 'sprout'"));
        assert!(out.contains("map global 'sprout' 'b'"));
    }

    #[test]
    fn hook_basic() {
        let cmd: KakCommand = Hook::new(Scope::Global, "BufCreate", ".*\\.rs", ":echo rust<ret>")
            .group("rust-setup")
            .once()
            .into();
        let out = cmd.render();
        assert!(out.starts_with("hook -group 'rust-setup' -once global"));
        assert!(out.contains("'BufCreate'"));
    }

    #[test]
    fn alias_simple() {
        let cmd: KakCommand = Alias::new(Scope::Global, "x", "echo").into();
        assert_eq!(cmd.render(), "alias global 'x' 'echo'");
    }

    #[test]
    fn echo_with_flags() {
        let cmd: KakCommand = Echo::new("hello").markup().debug().into();
        assert_eq!(cmd.render(), "echo -markup -debug 'hello'");
    }

    #[test]
    fn info_with_title() {
        let cmd: KakCommand = Info::new("body").title("Title").markup().into();
        let out = cmd.render();
        assert!(out.contains("-title 'Title'"));
        assert!(out.contains("-markup"));
        assert!(out.ends_with(" 'body'"));
    }

    #[test]
    fn try_wraps_any_command() {
        let inner: KakCommand = DefineCommand::new("c", "echo hi").into();
        let cmd = inner.wrapped_in_try();
        match &cmd {
            KakCommand::Try(_) => {}
            _ => panic!("expected Try"),
        }
        let out = cmd.render();
        assert!(out.starts_with("try %{"));
    }

    #[test]
    fn escape_arg_preserves_quotes() {
        // Sanity: positional args go through escape_arg.
        let cmd: KakCommand = DeclareUserMode::new("it's mine").into();
        assert_eq!(cmd.render(), "declare-user-mode 'it''s mine'");
    }
}
