//! `kak_lint!` — compile-time Kakoune command string validator.
//!
//! Catches typos like the `-override` flag on `declare-user-mode` (which
//! caused all 11 user-mode keymaps in sprout to silently fail to register —
//! see Issue #81) at compile time, before the plugin is ever loaded.
//!
//! # Approach
//!
//! Each entry in [`CATALOG`] pins down the **flags** a single Kakoune
//! command accepts. Positional arguments and command bodies are passed
//! through (the linter does not type-check argument values). Commands not
//! in the catalog are also passed through — keeping the linter strictly
//! additive: it never produces a false positive against a real Kakoune
//! command. Plugin authors who need stricter validation can add their
//! command to the catalog.
//!
//! # Tokenization
//!
//! Kakoune's command parser recognizes four quotation forms:
//! - `'foo'` — single-quoted string (with `''` escaping `'`)
//! - `%{ … }`, `%[ … ]`, `%( … )`, `%< … >` — balanced delimiters
//! - `"foo"` — double-quoted (rare; not currently handled — bodies that
//!   need it should use `%{...}` instead)
//!
//! The tokenizer keeps just enough state to walk past these blocks so that
//! a `-` inside a quoted body is not mistaken for a flag.

use proc_macro::TokenStream;
use quote::quote;
use syn::{LitStr, parse_macro_input};

// ---------------------------------------------------------------------------
// Catalog
// ---------------------------------------------------------------------------

/// One Kakoune command's flag specification.
pub(crate) struct CommandSpec {
    pub name: &'static str,
    pub flags: &'static [FlagSpec],
}

pub(crate) struct FlagSpec {
    pub name: &'static str,
    /// Whether this flag consumes the next token as its value.
    pub takes_value: bool,
}

/// Helper: flag with no value (e.g., `-hidden`).
const fn flag(name: &'static str) -> FlagSpec {
    FlagSpec {
        name,
        takes_value: false,
    }
}

/// Helper: flag with a value (e.g., `-docstring 'foo'`).
const fn flag_v(name: &'static str) -> FlagSpec {
    FlagSpec {
        name,
        takes_value: true,
    }
}

// --- Per-command flag tables ---
// Keep these alphabetized within each table for ease of audit.

const FL_DECLARE_USER_MODE: &[FlagSpec] = &[flag_v("-docstring"), flag("-hidden")];
//                                          ^ NOTE: explicitly *no* `-override` (Issue #81).

const FL_DECLARE_OPTION: &[FlagSpec] = &[flag_v("-docstring"), flag("-hidden")];

const FL_DEFINE_COMMAND: &[FlagSpec] = &[
    flag("-allow-override"),
    flag_v("-docstring"),
    flag("-hidden"),
    flag("-override"),
    flag_v("-params"),
    flag_v("-menu"),
    flag_v("-file-completion"),
    flag_v("-shell-completion"),
    flag_v("-shell-script-completion"),
    flag_v("-shell-script-candidates"),
    flag_v("-client-completion"),
    flag_v("-command-completion"),
    flag_v("-buffer-completion"),
];

const FL_MAP: &[FlagSpec] = &[flag_v("-docstring")];

const FL_SET_OPTION: &[FlagSpec] = &[flag("-add"), flag("-remove")];

const FL_UNSET_OPTION: &[FlagSpec] = &[];

const FL_EVALUATE_COMMANDS: &[FlagSpec] = &[
    flag_v("-buffer"),
    flag_v("-client"),
    flag("-draft"),
    flag("-itersel"),
    flag("-no-hooks"),
    flag_v("-save-regs"),
    flag_v("-try-client"),
    flag("-verbatim"),
];

const FL_HOOK: &[FlagSpec] = &[flag("-always"), flag_v("-group"), flag("-once")];

const FL_ALIAS: &[FlagSpec] = &[];

const FL_ECHO: &[FlagSpec] = &[flag("-debug"), flag("-markup"), flag("-quoting"), flag_v("-to-file")];

const FL_INFO: &[FlagSpec] = &[
    flag_v("-anchor"),
    flag("-markup"),
    flag_v("-placement"),
    flag_v("-style"),
    flag_v("-title"),
];

const FL_TRY: &[FlagSpec] = &[];

/// Top-level catalog. Keep alphabetized.
pub(crate) const CATALOG: &[CommandSpec] = &[
    CommandSpec { name: "alias", flags: FL_ALIAS },
    CommandSpec { name: "declare-option", flags: FL_DECLARE_OPTION },
    CommandSpec { name: "declare-user-mode", flags: FL_DECLARE_USER_MODE },
    CommandSpec { name: "define-command", flags: FL_DEFINE_COMMAND },
    CommandSpec { name: "echo", flags: FL_ECHO },
    CommandSpec { name: "evaluate-commands", flags: FL_EVALUATE_COMMANDS },
    CommandSpec { name: "hook", flags: FL_HOOK },
    CommandSpec { name: "info", flags: FL_INFO },
    CommandSpec { name: "map", flags: FL_MAP },
    CommandSpec { name: "set-option", flags: FL_SET_OPTION },
    CommandSpec { name: "try", flags: FL_TRY },
    CommandSpec { name: "unset-option", flags: FL_UNSET_OPTION },
];

fn lookup(name: &str) -> Option<&'static CommandSpec> {
    CATALOG.iter().find(|c| c.name == name)
}

fn lookup_flag(spec: &CommandSpec, name: &str) -> Option<&'static FlagSpec> {
    spec.flags.iter().find(|f| f.name == name)
}

// ---------------------------------------------------------------------------
// Tokenizer
// ---------------------------------------------------------------------------

/// A token in a Kakoune command line, with its byte range in the source.
#[derive(Debug)]
struct Token<'a> {
    text: &'a str,
}

/// Tokenize a Kakoune command line, treating quoted blocks as opaque single
/// tokens. Whitespace separates tokens.
///
/// Recognized quote forms (Kakoune syntax):
/// - `'…'` — embedded `''` escapes a single `'`.
/// - `%{ … }` / `%[ … ]` / `%( … )` / `%< … >` — balanced.
///
/// Returns `Err(msg)` if the input has an unterminated block. The error
/// message is intended for `compile_error!`.
fn tokenize(input: &str) -> Result<Vec<Token<'_>>, String> {
    let bytes = input.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        // Skip whitespace.
        while i < bytes.len() && (bytes[i] as char).is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let start = i;

        // Detect quote form.
        if bytes[i] == b'\'' {
            // Single-quoted; scan until matching `'` (handling `''` escape).
            i += 1;
            let mut closed = false;
            while i < bytes.len() {
                if bytes[i] == b'\'' {
                    if i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                        i += 2;
                        continue;
                    }
                    i += 1;
                    closed = true;
                    break;
                }
                i += 1;
            }
            if !closed {
                return Err("unterminated single-quoted string".into());
            }
            tokens.push(Token { text: &input[start..i] });
            continue;
        }

        if bytes[i] == b'%' && i + 1 < bytes.len() {
            let (open, close) = match bytes[i + 1] {
                b'{' => (b'{', b'}'),
                b'[' => (b'[', b']'),
                b'(' => (b'(', b')'),
                b'<' => (b'<', b'>'),
                _ => (0, 0),
            };
            if open != 0 {
                let mut depth: i32 = 0;
                i += 1; // past '%'
                let block_start = i;
                while i < bytes.len() {
                    if bytes[i] == open {
                        depth += 1;
                    } else if bytes[i] == close {
                        depth -= 1;
                        if depth == 0 {
                            i += 1;
                            break;
                        }
                    }
                    i += 1;
                }
                if depth != 0 {
                    let kind = open as char;
                    return Err(format!("unterminated %{kind}…{} block", close as char));
                }
                let _ = block_start;
                tokens.push(Token { text: &input[start..i] });
                continue;
            }
        }

        // Bare word: until whitespace.
        while i < bytes.len() && !(bytes[i] as char).is_ascii_whitespace() {
            i += 1;
        }
        tokens.push(Token { text: &input[start..i] });
    }
    Ok(tokens)
}

// ---------------------------------------------------------------------------
// Lint
// ---------------------------------------------------------------------------

/// Check `input` against [`CATALOG`]. Returns `Ok(())` on accept,
/// `Err(msg)` on a known-command flag violation. Unknown commands pass
/// through (additive policy).
fn lint(input: &str) -> Result<(), String> {
    let tokens = tokenize(input)?;
    let mut iter = tokens.into_iter();

    // Find the command name. Strip a leading `try` (commonly wraps idempotent
    // setup) and recurse into its body — the linted command is what `try`
    // is wrapping.
    let cmd_token = match iter.next() {
        Some(t) if t.text == "try" => match iter.next() {
            Some(block) => {
                let inner = strip_block(block.text).unwrap_or(block.text);
                return lint(inner);
            }
            None => return Ok(()),
        },
        Some(t) => t,
        None => return Ok(()),
    };

    let cmd_name = cmd_token.text;
    let spec = match lookup(cmd_name) {
        Some(s) => s,
        None => return Ok(()),
    };

    // Walk subsequent tokens. Each `-flag` is validated against `spec`.
    // Stop walking at the first non-flag token — everything beyond is
    // positional / body and not validated.
    while let Some(tok) = iter.next() {
        let text = tok.text;
        if !text.starts_with('-') {
            break;
        }
        // `-- ` ends flag parsing in Kakoune; treat as bail-out.
        if text == "--" {
            break;
        }
        let f = match lookup_flag(spec, text) {
            Some(f) => f,
            None => {
                return Err(format!(
                    "unknown flag `{text}` for Kakoune command `{cmd_name}`. \
                     accepted flags: {}",
                    spec.flags
                        .iter()
                        .map(|f| f.name)
                        .collect::<Vec<_>>()
                        .join(", "),
                ));
            }
        };
        if f.takes_value {
            // Consume the value token (could be a quoted block).
            if iter.next().is_none() {
                return Err(format!(
                    "flag `{text}` for `{cmd_name}` expects a value"
                ));
            }
        }
    }
    Ok(())
}

/// If `s` is a `%X…Y` block, return the inner text; otherwise `None`.
fn strip_block(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    if bytes.len() < 3 || bytes[0] != b'%' {
        return None;
    }
    let close = match bytes[1] {
        b'{' => b'}',
        b'[' => b']',
        b'(' => b')',
        b'<' => b'>',
        _ => return None,
    };
    if bytes[bytes.len() - 1] != close {
        return None;
    }
    Some(&s[2..s.len() - 1])
}

// ---------------------------------------------------------------------------
// Proc macro entry point
// ---------------------------------------------------------------------------

/// Implementation of `kasane_kak_lint!(literal)`.
pub(crate) fn kak_lint_impl(input: TokenStream) -> TokenStream {
    let lit = parse_macro_input!(input as LitStr);
    let s = lit.value();
    match lint(&s) {
        Ok(()) => quote! { #lit }.into(),
        Err(msg) => syn::Error::new_spanned(&lit, msg)
            .into_compile_error()
            .into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- tokenizer ---
    #[test]
    fn tokenize_simple() {
        let t = tokenize("write").unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].text, "write");
    }

    #[test]
    fn tokenize_multi() {
        let t = tokenize("map global user b :bump<ret>").unwrap();
        assert_eq!(t.iter().map(|t| t.text).collect::<Vec<_>>(),
            vec!["map", "global", "user", "b", ":bump<ret>"]);
    }

    #[test]
    fn tokenize_single_quoted() {
        let t = tokenize("define-command 'a b' %{ body }").unwrap();
        let texts: Vec<_> = t.iter().map(|t| t.text).collect();
        assert_eq!(texts, vec!["define-command", "'a b'", "%{ body }"]);
    }

    #[test]
    fn tokenize_pct_brackets_nested() {
        let t = tokenize("evaluate-commands %{ map global user b %{ :bump<ret> } }").unwrap();
        assert_eq!(t.len(), 2);
        assert_eq!(t[1].text, "%{ map global user b %{ :bump<ret> } }");
    }

    #[test]
    fn tokenize_single_quote_escape() {
        let t = tokenize("echo 'it''s ok'").unwrap();
        assert_eq!(t.iter().map(|t| t.text).collect::<Vec<_>>(),
            vec!["echo", "'it''s ok'"]);
    }

    #[test]
    fn tokenize_unterminated_block_errors() {
        let err = tokenize("define-command 'a %{ body").unwrap_err();
        assert!(err.contains("unterminated"), "got: {err}");
    }

    // --- lint ---
    #[test]
    fn lint_accepts_known_command_no_flags() {
        assert!(lint("write").is_ok());
        assert!(lint("declare-user-mode 'sprout'").is_ok());
    }

    #[test]
    fn lint_accepts_valid_flag() {
        assert!(lint("declare-user-mode -hidden 'foo'").is_ok());
        assert!(lint("define-command -override -hidden 'foo' %{ body }").is_ok());
    }

    #[test]
    fn lint_rejects_override_on_declare_user_mode() {
        // The Issue #81 motivating bug.
        let err = lint("declare-user-mode -override 'foo'").unwrap_err();
        assert!(err.contains("`-override`"), "got: {err}");
        assert!(err.contains("`declare-user-mode`"), "got: {err}");
    }

    #[test]
    fn lint_rejects_unknown_flag() {
        let err = lint("map -bogus global user k a").unwrap_err();
        assert!(err.contains("`-bogus`"), "got: {err}");
    }

    #[test]
    fn lint_passes_through_unknown_command() {
        // Conservative policy: don't error on commands we don't catalog.
        assert!(lint("super-fancy-command -with -unknown -flags").is_ok());
    }

    #[test]
    fn lint_walks_into_try_block() {
        // `try %[ declare-user-mode -override foo ]` — should still flag.
        let err = lint("try %[ declare-user-mode -override 'foo' ]").unwrap_err();
        assert!(err.contains("`-override`"));
    }

    #[test]
    fn lint_handles_flag_with_value() {
        assert!(lint("declare-user-mode -docstring 'help text' 'foo'").is_ok());
        // `-docstring` takes a value — the value is consumed and not
        // mis-validated as a flag even though it starts with a letter.
        assert!(lint("map -docstring 'help' global user k a").is_ok());
    }

    #[test]
    fn lint_value_arg_starting_with_dash_is_not_a_flag() {
        // Edge case: a value happens to start with `-`. The walker consumes
        // it because `takes_value: true` doesn't re-check.
        assert!(lint("declare-user-mode -docstring '-not-a-flag' 'foo'").is_ok());
    }

    #[test]
    fn lint_stops_at_non_flag_positional() {
        // After 'sprout' (positional), we stop looking at flags. So a typo
        // *after* a positional is not caught — that's the intentional
        // additive policy.
        assert!(lint("map global sprout b -not-actually-validated").is_ok());
    }
}
