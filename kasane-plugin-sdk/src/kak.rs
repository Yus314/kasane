//! Idempotent Kakoune-command builders.
//!
//! Each helper returns a Kakoune command **string** intended to be wrapped
//! in `Command::SendKeys(keys::command(&cmd))` or composed into an
//! `evaluate-commands` block. The helpers encode the correct idempotency
//! idiom for each command type so plugin authors cannot accidentally
//! pass invalid flags (notably `-override` to commands that do not
//! accept it).
//!
//! # Why string-typed?
//!
//! Composability. A caller can:
//! - Send each command as its own `SendKeys` (failure isolation — see
//!   [`crate::kakoune_setup_effects!`]).
//! - Concatenate several into a single `evaluate-commands %{ ... }` block
//!   (compact, but cascade-fails on the first error).
//!
//! Returning typed `Command` values would force one of the two patterns.

/// Scope qualifier for `map`, `set-option`, `unset-option`, etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Global,
    Buffer,
    Window,
}

impl Scope {
    fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Buffer => "buffer",
            Self::Window => "window",
        }
    }
}

/// Option type for `declare-option`.
///
/// Intentionally minimal. Add cases as plugins surface a need; each new
/// case is a free SDK addition (no ABI bump).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionKind {
    Int,
    Bool,
    Str,
    Regex,
    IntList,
    StrList,
}

impl OptionKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Int => "int",
            Self::Bool => "bool",
            Self::Str => "str",
            Self::Regex => "regex",
            Self::IntList => "int-list",
            Self::StrList => "str-list",
        }
    }
}

/// Escape a string for use as a single-quoted Kakoune argument.
///
/// Wraps `s` in `'...'` and escapes embedded `'` as `''` (Kakoune's
/// single-quote escape rule). Produces a valid Kakoune token for any
/// input — including empty strings, names with whitespace, and values
/// with embedded quotes.
pub fn escape_arg(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for ch in s.chars() {
        if ch == '\'' {
            out.push('\'');
        }
        out.push(ch);
    }
    out.push('\'');
    out
}

/// `try %[ declare-user-mode <name> ]` — idempotent.
///
/// Kakoune's `declare-user-mode` does **not** accept `-override`; calling
/// it a second time errors with `user mode already declared`. The
/// `try %[ ... ]` wrapper swallows that error. This helper encodes the
/// only correct idempotency idiom — the plugin author cannot accidentally
/// pass `-override`.
pub fn declare_user_mode(name: &str) -> String {
    format!("try %[ declare-user-mode {} ]", escape_arg(name))
}

/// `declare-option [-hidden] <kind> <name> <default>`.
///
/// Naturally idempotent: Kakoune `declare-option` no-ops on same-type
/// re-declaration with the same default. Changing `kind` between calls
/// is a hard error; plugins must keep the kind stable across reloads.
pub fn declare_option(name: &str, kind: OptionKind, default: &str, hidden: bool) -> String {
    let hidden_flag = if hidden { "-hidden " } else { "" };
    format!(
        "declare-option {}{} {} {}",
        hidden_flag,
        kind.as_str(),
        escape_arg(name),
        escape_arg(default),
    )
}

/// `define-command -override <name> [-params N] <body>`.
///
/// `body` is wrapped in a balanced delimiter pair — `%{ ... }` if the
/// body has balanced `{`/`}`, falling back through `%[ ... ]`,
/// `%( ... )`, `%< ... >`, and finally [`escape_arg`].
///
/// Prefer body text with balanced `{}` — by far the most readable form.
pub fn define_command(name: &str, params: Option<u32>, body: &str) -> String {
    let params_flag = match params {
        Some(n) => format!(" -params {}", n),
        None => String::new(),
    };
    format!(
        "define-command -override {}{} {}",
        escape_arg(name),
        params_flag,
        wrap_balanced(body),
    )
}

/// Escape a string for use as a Kakoune **double-quoted** argument.
///
/// Wraps `s` in `"…"` and escapes embedded `"` as `""`. Unlike
/// [`escape_arg`] (single-quoted, literal), inside a double-quoted
/// token Kakoune processes `%X{…}` expansions at command-evaluation
/// time. Use this whenever the value should resolve dynamic
/// substitutions like `%opt{name}`, `%arg{N}`, `%val{name}`, `%sh{…}`,
/// or `%reg{c}`.
///
/// # Why this matters — the bareword foot-gun
///
/// Kakoune's expansion rule (`doc/pages/expansions.asciidoc`):
///
/// > Expansions are processed when unquoted and anywhere inside
/// > double-quoted strings, **but not inside unquoted words**, inside
/// > single-quoted strings, or inside %-strings.
///
/// So `set-option -add window ui_options foo=%opt{bar}` stores the
/// **literal string** `foo=%opt{bar}` (because `foo=%opt{bar}` is a
/// single bareword token). Wrapping in `"…"` is what unlocks
/// expansion: `set-option -add window ui_options "foo=%opt{bar}"`.
///
/// The bug is silent: no Kakoune diagnostic, no compile error, just
/// a propagated literal where the plugin expected a substituted value.
/// This helper is the foot-gun-safe form. Surfaced by sprout
/// dogfooding ([Issue #97](https://github.com/Yus314/kasane/issues/97)).
pub fn escape_arg_expand(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        if ch == '"' {
            out.push('"');
        }
        out.push(ch);
    }
    out.push('"');
    out
}

/// `set-option <scope> <name> "<value>"`.
///
/// Double-quotes the value so Kakoune processes `%X{…}` expansions
/// inside it. For a **literal** value where `%`-sequences should be
/// stored verbatim, use [`escape_arg`] and compose the command yourself
/// (single-quoted form disables expansion).
///
/// See [`escape_arg_expand`] for the bareword foot-gun explanation.
pub fn set_option(scope: Scope, name: &str, value: &str) -> String {
    format!(
        "set-option {} {} {}",
        scope.as_str(),
        escape_arg(name),
        escape_arg_expand(value),
    )
}

/// `set-option -add <scope> <name> "k1=v1" "k2=v2" …`.
///
/// Each `key=value` entry is emitted as its own double-quoted token,
/// so `%X{…}` expansions inside any `value` are processed.
///
/// Without the double-quote wrapping each entry, Kakoune treats
/// `key=%opt{x}` as a bareword and stores the literal text — see
/// [`escape_arg_expand`] for the full explanation.
///
/// # Example
///
/// ```
/// use kasane_plugin_sdk::kak::{self, Scope};
///
/// let cmd = kak::set_option_add(Scope::Window, "ui_options", &[
///     ("sprout_request_id", "%opt{sprout_request_seq}"),
///     ("sprout_request_kind", "pick"),
///     ("sprout_request_arg", "%arg{1}"),
/// ]);
/// assert_eq!(
///     cmd,
///     "set-option -add window 'ui_options' \
///      \"sprout_request_id=%opt{sprout_request_seq}\" \
///      \"sprout_request_kind=pick\" \
///      \"sprout_request_arg=%arg{1}\"",
/// );
/// ```
pub fn set_option_add(scope: Scope, name: &str, entries: &[(&str, &str)]) -> String {
    let mut out = format!(
        "set-option -add {} {}",
        scope.as_str(),
        escape_arg(name),
    );
    for (k, v) in entries {
        out.push(' ');
        out.push_str(&escape_arg_expand(&format!("{k}={v}")));
    }
    out
}

/// `map <scope> <mode> <key> <action> [-docstring '...']`.
pub fn map(
    scope: Scope,
    mode: &str,
    key: &str,
    action: &str,
    docstring: Option<&str>,
) -> String {
    let mut out = format!(
        "map {} {} {} {}",
        scope.as_str(),
        escape_arg(mode),
        escape_arg(key),
        escape_arg(action),
    );
    if let Some(d) = docstring {
        out.push_str(" -docstring ");
        out.push_str(&escape_arg(d));
    }
    out
}

/// Pick a balanced `%X..Y` delimiter pair that doesn't conflict with
/// body content. Falls back to [`escape_arg`] if none of the four
/// standard pairs work.
fn wrap_balanced(body: &str) -> String {
    for (open, close) in [('{', '}'), ('[', ']'), ('(', ')'), ('<', '>')] {
        if is_balanced(body, open, close) {
            return format!("%{} {} {}", open, body, close);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_arg_plain() {
        assert_eq!(escape_arg("foo"), "'foo'");
    }

    #[test]
    fn escape_arg_with_quote() {
        assert_eq!(escape_arg("it's"), "'it''s'");
    }

    #[test]
    fn escape_arg_empty() {
        assert_eq!(escape_arg(""), "''");
    }

    #[test]
    fn escape_arg_with_spaces() {
        assert_eq!(escape_arg("a b c"), "'a b c'");
    }

    #[test]
    fn declare_user_mode_uses_try_idiom() {
        assert_eq!(
            declare_user_mode("sprout"),
            "try %[ declare-user-mode 'sprout' ]"
        );
    }

    #[test]
    fn declare_option_hidden() {
        assert_eq!(
            declare_option("counter", OptionKind::Int, "0", true),
            "declare-option -hidden int 'counter' '0'"
        );
    }

    #[test]
    fn declare_option_not_hidden() {
        assert_eq!(
            declare_option("name", OptionKind::Str, "default", false),
            "declare-option str 'name' 'default'"
        );
    }

    #[test]
    fn declare_option_kind_str_variants() {
        assert_eq!(OptionKind::Int.as_str(), "int");
        assert_eq!(OptionKind::Bool.as_str(), "bool");
        assert_eq!(OptionKind::Str.as_str(), "str");
        assert_eq!(OptionKind::Regex.as_str(), "regex");
        assert_eq!(OptionKind::IntList.as_str(), "int-list");
        assert_eq!(OptionKind::StrList.as_str(), "str-list");
    }

    #[test]
    fn define_command_no_params() {
        let out = define_command("demo", None, "echo hi");
        assert!(
            out.starts_with("define-command -override 'demo' %{"),
            "got: {out}"
        );
        assert!(out.contains("echo hi"));
        assert!(out.ends_with('}'));
    }

    #[test]
    fn define_command_with_params() {
        let out = define_command("greet", Some(1), "echo arg");
        assert!(
            out.starts_with("define-command -override 'greet' -params 1 %{"),
            "got: {out}"
        );
    }

    #[test]
    fn define_command_unbalanced_braces_falls_back_to_brackets() {
        let body = "echo {";
        let out = define_command("c", None, body);
        assert!(out.contains("%["), "expected fallback to %[..], got: {out}");
    }

    #[test]
    fn define_command_unbalanced_all_falls_back_to_quoted() {
        let body = "{ [ ( <";
        let out = define_command("c", None, body);
        assert!(out.contains("'{ [ ( <'"), "got: {out}");
    }

    #[test]
    fn map_basic() {
        assert_eq!(
            map(Scope::Global, "sprout", "?", ":info ok<ret>", None),
            "map global 'sprout' '?' ':info ok<ret>'"
        );
    }

    #[test]
    fn map_with_docstring() {
        let out = map(
            Scope::Global,
            "sprout",
            "b",
            ":bump<ret>",
            Some("bump counter"),
        );
        assert!(
            out.ends_with(" -docstring 'bump counter'"),
            "got: {out}"
        );
    }

    #[test]
    fn map_all_scopes() {
        assert!(map(Scope::Global, "m", "k", "a", None).starts_with("map global "));
        assert!(map(Scope::Buffer, "m", "k", "a", None).starts_with("map buffer "));
        assert!(map(Scope::Window, "m", "k", "a", None).starts_with("map window "));
    }

    #[test]
    fn wrap_balanced_picks_braces_for_balanced_body() {
        let out = wrap_balanced("info %sh{ echo hi }");
        assert!(out.starts_with("%{"), "got: {out}");
        assert!(out.ends_with('}'));
    }

    #[test]
    fn escape_arg_expand_plain() {
        assert_eq!(escape_arg_expand("foo"), "\"foo\"");
    }

    #[test]
    fn escape_arg_expand_doubles_dquote() {
        assert_eq!(escape_arg_expand(r#"a"b"#), r#""a""b""#);
    }

    #[test]
    fn escape_arg_expand_preserves_percent_sequences() {
        // %opt{} must survive verbatim — that is the entire point of the
        // expand form. The helper does not auto-double `%`.
        assert_eq!(
            escape_arg_expand("k=%opt{counter}"),
            "\"k=%opt{counter}\""
        );
    }

    #[test]
    fn set_option_plain() {
        assert_eq!(
            set_option(Scope::Global, "myopt", "hello"),
            "set-option global 'myopt' \"hello\""
        );
    }

    #[test]
    fn set_option_with_expansion() {
        assert_eq!(
            set_option(Scope::Window, "myopt", "%val{buffile}"),
            "set-option window 'myopt' \"%val{buffile}\""
        );
    }

    #[test]
    fn set_option_add_single_entry() {
        assert_eq!(
            set_option_add(Scope::Window, "ui_options", &[("k", "v")]),
            "set-option -add window 'ui_options' \"k=v\""
        );
    }

    #[test]
    fn set_option_add_sprout_regression() {
        // Exact shape from Issue #97. Without the double-quote wrapping
        // each entry, %opt{} and %arg{} would have been stored as literal
        // text in sprout's session-ready setup.
        let cmd = set_option_add(
            Scope::Window,
            "ui_options",
            &[
                ("sprout_request_id", "%opt{sprout_request_seq}"),
                ("sprout_request_kind", "pick"),
                ("sprout_request_arg", "%arg{1}"),
            ],
        );
        assert_eq!(
            cmd,
            "set-option -add window 'ui_options' \
             \"sprout_request_id=%opt{sprout_request_seq}\" \
             \"sprout_request_kind=pick\" \
             \"sprout_request_arg=%arg{1}\""
        );
    }

    #[test]
    fn set_option_add_no_entries() {
        // Empty list is a no-op set-option -add (Kakoune accepts it).
        assert_eq!(
            set_option_add(Scope::Global, "list", &[]),
            "set-option -add global 'list'"
        );
    }

    #[test]
    fn set_option_add_value_with_embedded_dquote() {
        let cmd = set_option_add(Scope::Global, "list", &[("k", r#"say "hi""#)]);
        // Inside `"..."`, embedded `"` is doubled.
        assert_eq!(cmd, "set-option -add global 'list' \"k=say \"\"hi\"\"\"");
    }

    #[test]
    fn is_balanced_basic() {
        assert!(is_balanced("a b c", '{', '}'));
        assert!(is_balanced("{a}", '{', '}'));
        assert!(is_balanced("{a {b} c}", '{', '}'));
        assert!(!is_balanced("{a", '{', '}'));
        assert!(!is_balanced("a}", '{', '}'));
        assert!(!is_balanced("}a{", '{', '}'));
    }
}
