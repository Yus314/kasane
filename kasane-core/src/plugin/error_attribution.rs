//! Plugin command-error attribution (Phase A — inbound recognition only).
//!
//! Implements the marker-based attribution protocol from ADR-042:
//! when a plugin wraps its Kakoune-side command body as
//!
//! ```text
//! try %[ <cmd> ] catch %[
//!     info -title '__kasane_plugin_error__' %{
//!         <plugin-id>
//!         %val{error}
//!     }
//! ]
//! ```
//!
//! Kakoune emits an `info_show` JSON-RPC method when the catch fires.
//! The state-apply layer recognises the reserved title, extracts the
//! plugin-id + error message, logs the attributed failure, and
//! **suppresses** the UI popup so the marker never reaches the end-user.
//!
//! Phase A surfaces errors via `tracing` only — there is no WIT-level
//! plugin-back-dispatch yet. Phase B (post-ADR-041 ABI 4.0.0) adds the
//! `on-command-error-effects` export so plugins observe their own errors
//! programmatically.

use crate::protocol::Line;

/// Reserved `info -title` value that marks an info_show emission as a
/// plugin error rather than a user-visible popup.
///
/// Double-underscore prefix/suffix mirrors Python's reserved-name
/// convention. Plugin authors must not use this title for their own
/// `info` popups.
pub const PLUGIN_ERROR_MARKER: &str = "__kasane_plugin_error__";

/// A plugin-attributed Kakoune command failure parsed from an
/// `info_show` payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginErrorEvent {
    /// The plugin-id reported in the catch body's first content line.
    pub plugin_id: String,
    /// Kakoune's error message (the value of `%val{error}` at catch time).
    ///
    /// Typical shape: `"<line>:<col>: '<cmd>': <reason>"`.
    pub message: String,
}

/// Concatenate atom contents in a Kakoune protocol Line into a plain string.
fn line_to_string(line: &Line) -> String {
    let mut out = String::new();
    for atom in line {
        out.push_str(atom.contents.as_str());
    }
    out
}

/// Returns true if `title` is the reserved plugin-error marker.
pub fn is_plugin_error_marker(title: &Line) -> bool {
    line_to_string(title) == PLUGIN_ERROR_MARKER
}

/// ADR-042 Phase B Step 3: wrap a Kakoune-command body with the
/// catch-info pattern so failures surface as a marker `info_show` to
/// be attributed to `plugin_id`.
///
/// Produces:
///
/// ```text
/// try <delim>OPEN <body> <delim>CLOSE catch %{ info -title '__kasane_plugin_error__' %{ <plugin_id><newline>%val{error} } }
/// ```
///
/// The body delimiter is picked from `%{ … }` / `%[ … ]` / `%( … )` /
/// `%< … >` by depth-counting against the body, mirroring the SDK's
/// `kak::define_command` balanced-delim selector. Falls back to a
/// safe quoted form if no pair balances.
pub fn wrap_command_with_marker(body: &str, plugin_id: &str) -> String {
    let body_wrapped = wrap_balanced_for_body(body);
    format!(
        "try {} catch %{{ info -title '{}' %{{ {}\n%val{{error}} }} }}",
        body_wrapped, PLUGIN_ERROR_MARKER, plugin_id,
    )
}

fn wrap_balanced_for_body(body: &str) -> String {
    for (open, close) in [('{', '}'), ('[', ']'), ('(', ')'), ('<', '>')] {
        if is_balanced(body, open, close) {
            return format!("%{} {} {}", open, body, close);
        }
    }
    // Fallback: single-quote with embedded-quote doubling.
    let mut out = String::with_capacity(body.len() + 2);
    out.push('\'');
    for ch in body.chars() {
        if ch == '\'' {
            out.push('\'');
        }
        out.push(ch);
    }
    out.push('\'');
    out
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

/// Parse a plugin-error info_show payload.
///
/// Expects `content` to have at least two lines: plugin-id (line 0)
/// and error message (line 1+, joined with `\n` for multi-line errors).
/// Returns `None` if the payload doesn't match the expected shape.
pub fn parse_plugin_error(content: &[Line]) -> Option<PluginErrorEvent> {
    if content.len() < 2 {
        return None;
    }
    let plugin_id = line_to_string(&content[0]);
    if plugin_id.is_empty() {
        return None;
    }
    let message = content[1..]
        .iter()
        .map(line_to_string)
        .collect::<Vec<_>>()
        .join("\n");
    Some(PluginErrorEvent { plugin_id, message })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Atom;
    use crate::protocol::Style;

    fn line(s: &str) -> Line {
        vec![Atom::with_style(s, Style::default())]
    }

    #[test]
    fn marker_recognized() {
        assert!(is_plugin_error_marker(&line(PLUGIN_ERROR_MARKER)));
    }

    #[test]
    fn non_marker_rejected() {
        assert!(!is_plugin_error_marker(&line("some other title")));
        assert!(!is_plugin_error_marker(&line("")));
    }

    #[test]
    fn marker_split_across_atoms_concatenates() {
        // info_show can deliver title as multiple atoms with different faces.
        let style = Style::default();
        let split: Line = vec![
            Atom::with_style("__kasane_", style.clone()),
            Atom::with_style("plugin_error__", style),
        ];
        assert!(is_plugin_error_marker(&split));
    }

    #[test]
    fn parse_two_line_payload() {
        let content = vec![
            line("sprout"),
            line("1:2: 'unknown-command': no such command"),
        ];
        let ev = parse_plugin_error(&content).expect("should parse");
        assert_eq!(ev.plugin_id, "sprout");
        assert_eq!(ev.message, "1:2: 'unknown-command': no such command");
    }

    #[test]
    fn parse_multi_line_error_joined() {
        let content = vec![
            line("sprout"),
            line("first line of error"),
            line("second line of error"),
        ];
        let ev = parse_plugin_error(&content).expect("should parse");
        assert_eq!(ev.plugin_id, "sprout");
        assert_eq!(ev.message, "first line of error\nsecond line of error");
    }

    #[test]
    fn parse_rejects_one_line() {
        let content = vec![line("sprout")];
        assert_eq!(parse_plugin_error(&content), None);
    }

    #[test]
    fn parse_rejects_empty_plugin_id() {
        let content = vec![line(""), line("error msg")];
        assert_eq!(parse_plugin_error(&content), None);
    }

    #[test]
    fn parse_rejects_empty_content() {
        assert_eq!(parse_plugin_error(&[]), None);
    }

    // --- wrap_command_with_marker tests (Phase B Step 3) ---

    #[test]
    fn wrap_simple_body_uses_braces() {
        let out = wrap_command_with_marker("declare-user-mode demo", "sprout");
        assert!(
            out.starts_with("try %{ declare-user-mode demo } catch %{ info -title '__kasane_plugin_error__' %{ sprout\n%val{error} } }"),
            "got: {out}"
        );
    }

    #[test]
    fn wrap_unbalanced_braces_falls_back_to_brackets() {
        let body = "echo {";
        let out = wrap_command_with_marker(body, "p");
        assert!(out.starts_with("try %[ echo { ]"), "got: {out}");
    }

    #[test]
    fn wrap_unbalanced_all_falls_back_to_quoted() {
        let body = "{ [ ( <";
        let out = wrap_command_with_marker(body, "p");
        assert!(out.starts_with("try '{ [ ( <'"), "got: {out}");
    }

    #[test]
    fn wrap_body_with_quotes_round_trips() {
        let body = "echo 'hi'";
        let out = wrap_command_with_marker(body, "p");
        // balanced %{ } chosen since braces in body are balanced (none here)
        assert!(out.contains("echo 'hi'"));
    }
}
