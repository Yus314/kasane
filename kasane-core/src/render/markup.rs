//! Parser for Kakoune face markup: `{face_spec}text{default}`.
//!
//! Markup syntax:
//! - `{face_spec}` — apply the given face for subsequent text
//! - `{default}`   — reset to the base face
//! - `\{`          — literal `{`
//! - Any text outside braces is rendered with the current face
//!
//! WireFace spec format: `fg,bg+attrs` (see `render::theme::parse_face_spec`).

use compact_str::CompactString;

use crate::protocol::{Atom, Line, Style, WireFace};
use crate::render::theme::parse_face_spec;

/// Parse a markup string into a vector of Atoms.
///
/// The `base` face is used as the starting face and for `{default}`.
pub fn parse_markup(input: &str, base: &WireFace) -> Line {
    let mut atoms: Vec<Atom> = Vec::new();
    let mut current_face = *base;
    let mut current_text = CompactString::default();
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            // Escape: next char is literal
            if let Some(next) = chars.next() {
                current_text.push(next);
            }
        } else if ch == '{' {
            // Start of face spec — flush current text
            if !current_text.is_empty() {
                atoms.push(Atom::with_style(
                    std::mem::take(&mut current_text),
                    Style::from_face(&current_face),
                ));
            }

            // Read until '}'
            let mut spec = String::new();
            for c in chars.by_ref() {
                if c == '}' {
                    break;
                }
                spec.push(c);
            }

            if spec == "default" || spec.is_empty() {
                current_face = *base;
            } else if let Some(face) = parse_face_spec(&spec) {
                current_face = face;
            }
            // If parse failed, just ignore the tag and continue with current face
        } else {
            current_text.push(ch);
        }
    }

    // Flush remaining text
    if !current_text.is_empty() {
        atoms.push(Atom::with_style(
            current_text,
            Style::from_face(&current_face),
        ));
    }

    atoms
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Attributes, Color, NamedColor};

    #[test]
    fn test_plain_text() {
        let base = WireFace::default();
        let atoms = parse_markup("hello world", &base);
        assert_eq!(atoms.len(), 1);
        assert_eq!(atoms[0].contents, "hello world");
        assert_eq!(atoms[0].unresolved_style().to_face(), base);
    }

    #[test]
    fn test_single_face() {
        let base = WireFace::default();
        let atoms = parse_markup("{red,blue}colored", &base);
        assert_eq!(atoms.len(), 1);
        assert_eq!(atoms[0].contents, "colored");
        assert_eq!(
            atoms[0].unresolved_style().to_face().fg,
            Color::Named(NamedColor::Red)
        );
        assert_eq!(
            atoms[0].unresolved_style().to_face().bg,
            Color::Named(NamedColor::Blue)
        );
    }

    #[test]
    fn test_face_then_default() {
        let base = WireFace::default();
        let atoms = parse_markup("{green,default}ok{default} done", &base);
        assert_eq!(atoms.len(), 2);
        assert_eq!(atoms[0].contents, "ok");
        assert_eq!(
            atoms[0].unresolved_style().to_face().fg,
            Color::Named(NamedColor::Green)
        );
        assert_eq!(atoms[1].contents, " done");
        assert_eq!(atoms[1].unresolved_style().to_face(), base);
    }

    #[test]
    fn test_attributes() {
        let base = WireFace::default();
        let atoms = parse_markup("{default,default+b}bold", &base);
        assert_eq!(atoms.len(), 1);
        assert!(
            atoms[0]
                .unresolved_style()
                .to_face()
                .attributes
                .contains(Attributes::BOLD)
        );
    }

    #[test]
    fn test_escaped_brace() {
        let base = WireFace::default();
        let atoms = parse_markup("use \\{braces\\}", &base);
        assert_eq!(atoms.len(), 1);
        assert_eq!(atoms[0].contents, "use {braces}");
    }

    #[test]
    fn test_multiple_faces() {
        let base = WireFace::default();
        let atoms = parse_markup("{red,default}err{yellow,default}warn{default}ok", &base);
        assert_eq!(atoms.len(), 3);
        assert_eq!(
            atoms[0].unresolved_style().to_face().fg,
            Color::Named(NamedColor::Red)
        );
        assert_eq!(
            atoms[1].unresolved_style().to_face().fg,
            Color::Named(NamedColor::Yellow)
        );
        assert_eq!(atoms[2].unresolved_style().to_face(), base);
    }

    #[test]
    fn test_empty_input() {
        let base = WireFace::default();
        let atoms = parse_markup("", &base);
        assert!(atoms.is_empty());
    }

    #[test]
    fn test_empty_spec_resets() {
        let base = WireFace::default();
        let atoms = parse_markup("{red,default}colored{}reset", &base);
        assert_eq!(atoms.len(), 2);
        assert_eq!(atoms[1].unresolved_style().to_face(), base);
    }
}
