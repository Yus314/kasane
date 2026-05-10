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
}
