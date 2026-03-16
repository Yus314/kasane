//! Pure functions for derived state computation.
//!
//! These functions extract deterministic computations from `apply.rs` into
//! standalone, testable pure functions. They form the Layer 2 boundary
//! for Salsa tracked function integration.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::protocol::{Atom, Attributes, Color, Coord, CursorMode, Face, Line};
use crate::render::CursorStyle;

/// Detect all cursor positions (primary + secondary) from draw atoms.
///
/// Returns `(cursor_count, secondary_cursors)` where `secondary_cursors`
/// excludes the primary cursor at `primary_cursor_pos`.
///
/// Uses two strategies:
/// 1. **Attribute heuristic**: scan for `FINAL_FG + REVERSE` (Kakoune's default
///    PrimaryCursor face uses `+rfg`).
/// 2. **Face-matching fallback**: if (1) finds nothing, identify the face at
///    `primary_cursor_pos` and scan for atoms with the same foreground color
///    (covers third-party themes that omit `+rfg` from cursor faces).
pub fn detect_cursors(lines: &[Line], primary_cursor_pos: Coord) -> (usize, Vec<Coord>) {
    let all_cursors = detect_cursors_by_attributes(lines);
    if !all_cursors.is_empty() {
        let cursor_count = all_cursors.len();
        let secondary_cursors = all_cursors
            .into_iter()
            .filter(|c| *c != primary_cursor_pos)
            .collect();
        return (cursor_count, secondary_cursors);
    }

    // Fallback: use the face at primary_cursor_pos as a template to find
    // secondary cursors.  Third-party themes typically set PrimaryCursor and
    // SecondaryCursor with the same fg but different bg; matching on fg
    // catches both.
    let all_cursors = detect_cursors_by_face(lines, primary_cursor_pos);
    let cursor_count = all_cursors.len();
    let secondary_cursors = all_cursors
        .into_iter()
        .filter(|c| *c != primary_cursor_pos)
        .collect();
    (cursor_count, secondary_cursors)
}

/// Scan atoms for the traditional `FINAL_FG + REVERSE` attribute pattern.
fn detect_cursors_by_attributes(lines: &[Line]) -> Vec<Coord> {
    let mut cursors = Vec::new();
    for (line_idx, line) in lines.iter().enumerate() {
        let mut col: u32 = 0;
        for atom in line.iter() {
            let is_cursor = atom.face.attributes.contains(Attributes::FINAL_FG)
                && atom.face.attributes.contains(Attributes::REVERSE);
            if is_cursor {
                cursors.push(Coord {
                    line: line_idx as i32,
                    column: col as i32,
                });
            }
            col += atom_display_width(atom);
        }
    }
    cursors
}

/// Find the face at a given coordinate, then scan for atoms with matching fg.
fn detect_cursors_by_face(lines: &[Line], primary_pos: Coord) -> Vec<Coord> {
    let primary_face = match face_at_coord(lines, primary_pos) {
        Some(f) => f,
        None => return vec![],
    };

    // Only use fallback if the primary cursor has a distinctive face
    // (explicit fg — not Default).
    if primary_face.fg == Color::Default {
        return vec![primary_pos];
    }

    let mut cursors = Vec::new();
    for (line_idx, line) in lines.iter().enumerate() {
        let mut col: u32 = 0;
        for atom in line.iter() {
            if atom.face.fg == primary_face.fg && atom.face.bg != Color::Default {
                cursors.push(Coord {
                    line: line_idx as i32,
                    column: col as i32,
                });
            }
            col += atom_display_width(atom);
        }
    }

    // If matching found too many positions (>64), the heuristic is unreliable;
    // fall back to just the primary cursor.
    if cursors.len() > 64 {
        return vec![primary_pos];
    }

    if cursors.is_empty() {
        vec![primary_pos]
    } else {
        cursors
    }
}

/// Look up the face of the atom at a given (line, column) coordinate.
fn face_at_coord(lines: &[Line], pos: Coord) -> Option<Face> {
    let line = lines.get(pos.line as usize)?;
    let target_col = pos.column as u32;
    let mut col: u32 = 0;
    for atom in line.iter() {
        let width = atom_display_width(atom);
        if col <= target_col && target_col < col + width.max(1) {
            return Some(atom.face);
        }
        col += width;
    }
    None
}

/// Compute the display width of an atom's contents.
fn atom_display_width(atom: &Atom) -> u32 {
    let mut width: u32 = 0;
    for grapheme in atom.contents.as_str().graphemes(true) {
        if grapheme.starts_with(|c: char| c.is_control()) {
            continue;
        }
        width += UnicodeWidthStr::width(grapheme) as u32;
    }
    width
}

/// Compute per-line dirty flags by comparing old and new line data.
///
/// Returns a `Vec<bool>` with one entry per new line. If face or line count
/// changed, all lines are marked dirty.
pub fn compute_lines_dirty(
    old_lines: &[Line],
    new_lines: &[Line],
    old_default_face: &Face,
    new_default_face: &Face,
    old_padding_face: &Face,
    new_padding_face: &Face,
) -> Vec<bool> {
    let face_changed =
        *old_default_face != *new_default_face || *old_padding_face != *new_padding_face;
    let len_changed = old_lines.len() != new_lines.len();

    if face_changed || len_changed {
        vec![true; new_lines.len()]
    } else {
        old_lines
            .iter()
            .zip(new_lines.iter())
            .map(|(old, new)| old != new)
            .collect()
    }
}

/// Derive cursor mode from the status content cursor position.
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

/// Concatenate status prompt and content atoms into a single status line.
pub fn build_status_line(prompt: &[Atom], content: &[Atom]) -> Line {
    let mut combined = prompt.to_vec();
    combined.extend_from_slice(content);
    combined
}

/// Derive cursor style from state fields (without plugin override).
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Atom, Color, NamedColor};

    fn make_atom(text: &str) -> Atom {
        Atom {
            face: Face::default(),
            contents: text.into(),
        }
    }

    fn make_cursor_atom(text: &str) -> Atom {
        Atom {
            face: Face {
                attributes: Attributes::FINAL_FG | Attributes::REVERSE,
                ..Face::default()
            },
            contents: text.into(),
        }
    }

    // --- detect_cursors tests ---

    #[test]
    fn detect_cursors_empty_buffer() {
        let (count, secondary) = detect_cursors(&[], Coord::default());
        assert_eq!(count, 0);
        assert!(secondary.is_empty());
    }

    #[test]
    fn detect_cursors_single_primary() {
        let lines = vec![vec![
            make_atom("hel"),
            make_cursor_atom("l"),
            make_atom("o"),
        ]];
        let primary = Coord { line: 0, column: 3 };
        let (count, secondary) = detect_cursors(&lines, primary);
        assert_eq!(count, 1);
        assert!(secondary.is_empty());
    }

    #[test]
    fn detect_cursors_with_secondary() {
        let lines = vec![
            vec![make_cursor_atom("h"), make_atom("ello")],
            vec![make_atom("wor"), make_cursor_atom("l"), make_atom("d")],
        ];
        let primary = Coord { line: 0, column: 0 };
        let (count, secondary) = detect_cursors(&lines, primary);
        assert_eq!(count, 2);
        assert_eq!(secondary.len(), 1);
        assert_eq!(secondary[0], Coord { line: 1, column: 3 });
    }

    #[test]
    fn detect_cursors_cjk_width() {
        // CJK character "漢" is 2 cells wide
        let lines = vec![vec![make_atom("漢"), make_cursor_atom("x")]];
        let primary = Coord { line: 0, column: 2 };
        let (count, secondary) = detect_cursors(&lines, primary);
        assert_eq!(count, 1);
        assert!(secondary.is_empty());
    }

    // --- detect_cursors face-matching fallback tests ---

    /// Helper: create an atom with an explicit fg+bg face (no REVERSE/FINAL_FG),
    /// mimicking third-party themes like anhsirk0/kakoune-themes.
    fn make_themed_cursor_atom(text: &str, fg: Color, bg: Color) -> Atom {
        Atom {
            face: Face {
                fg,
                bg,
                ..Face::default()
            },
            contents: text.into(),
        }
    }

    #[test]
    fn detect_cursors_fallback_single_primary() {
        // Theme: PrimaryCursor = dark,purple (no +rfg)
        let dark = Color::Rgb {
            r: 0x1e,
            g: 0x21,
            b: 0x27,
        };
        let purple = Color::Rgb {
            r: 0xc6,
            g: 0x78,
            b: 0xdd,
        };
        let lines = vec![vec![
            make_atom("hel"),
            make_themed_cursor_atom("l", dark, purple),
            make_atom("o"),
        ]];
        let primary = Coord { line: 0, column: 3 };
        let (count, secondary) = detect_cursors(&lines, primary);
        assert_eq!(count, 1);
        assert!(secondary.is_empty());
    }

    #[test]
    fn detect_cursors_fallback_with_secondary() {
        // PrimaryCursor = dark,purple; SecondaryCursor = dark,blue
        let dark = Color::Rgb {
            r: 0x1e,
            g: 0x21,
            b: 0x27,
        };
        let purple = Color::Rgb {
            r: 0xc6,
            g: 0x78,
            b: 0xdd,
        };
        let blue = Color::Rgb {
            r: 0x61,
            g: 0xaf,
            b: 0xef,
        };
        let lines = vec![
            vec![
                make_themed_cursor_atom("h", dark, purple),
                make_atom("ello"),
            ],
            vec![
                make_atom("wor"),
                make_themed_cursor_atom("l", dark, blue),
                make_atom("d"),
            ],
        ];
        let primary = Coord { line: 0, column: 0 };
        let (count, secondary) = detect_cursors(&lines, primary);
        assert_eq!(count, 2);
        assert_eq!(secondary.len(), 1);
        assert_eq!(secondary[0], Coord { line: 1, column: 3 });
    }

    // --- compute_lines_dirty tests ---

    #[test]
    fn lines_dirty_same_content() {
        let lines = vec![vec![make_atom("hello")]];
        let face = Face::default();
        let dirty = compute_lines_dirty(&lines, &lines, &face, &face, &face, &face);
        assert_eq!(dirty, vec![false]);
    }

    #[test]
    fn lines_dirty_changed_content() {
        let old = vec![vec![make_atom("hello")]];
        let new = vec![vec![make_atom("world")]];
        let face = Face::default();
        let dirty = compute_lines_dirty(&old, &new, &face, &face, &face, &face);
        assert_eq!(dirty, vec![true]);
    }

    #[test]
    fn lines_dirty_length_change_marks_all() {
        let old = vec![vec![make_atom("a")]];
        let new = vec![vec![make_atom("a")], vec![make_atom("b")]];
        let face = Face::default();
        let dirty = compute_lines_dirty(&old, &new, &face, &face, &face, &face);
        assert_eq!(dirty, vec![true, true]);
    }

    #[test]
    fn lines_dirty_face_change_marks_all() {
        let lines = vec![vec![make_atom("hello")]];
        let old_face = Face::default();
        let new_face = Face {
            fg: Color::Named(NamedColor::Red),
            ..Face::default()
        };
        let dirty = compute_lines_dirty(&lines, &lines, &old_face, &new_face, &old_face, &old_face);
        assert_eq!(dirty, vec![true]);
    }

    // --- derive_cursor_mode tests ---

    #[test]
    fn cursor_mode_prompt() {
        assert_eq!(derive_cursor_mode(0), CursorMode::Prompt);
        assert_eq!(derive_cursor_mode(5), CursorMode::Prompt);
    }

    #[test]
    fn cursor_mode_buffer() {
        assert_eq!(derive_cursor_mode(-1), CursorMode::Buffer);
        assert_eq!(derive_cursor_mode(-100), CursorMode::Buffer);
    }

    // --- build_status_line tests ---

    #[test]
    fn build_status_line_combines() {
        let prompt = vec![make_atom(":")];
        let content = vec![make_atom("edit foo")];
        let result = build_status_line(&prompt, &content);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].contents.as_str(), ":");
        assert_eq!(result[1].contents.as_str(), "edit foo");
    }

    #[test]
    fn build_status_line_empty_prompt() {
        let content = vec![make_atom("normal")];
        let result = build_status_line(&[], &content);
        assert_eq!(result.len(), 1);
    }

    // --- derive_cursor_style tests ---

    #[test]
    fn cursor_style_ui_option_override() {
        let mut opts = std::collections::HashMap::new();
        opts.insert("kasane_cursor_style".to_string(), "bar".to_string());
        assert_eq!(
            derive_cursor_style(&opts, true, CursorMode::Buffer, &vec![]),
            CursorStyle::Bar
        );
    }

    #[test]
    fn cursor_style_unfocused() {
        let opts = std::collections::HashMap::new();
        assert_eq!(
            derive_cursor_style(&opts, false, CursorMode::Buffer, &vec![]),
            CursorStyle::Outline
        );
    }

    #[test]
    fn cursor_style_prompt_mode() {
        let opts = std::collections::HashMap::new();
        assert_eq!(
            derive_cursor_style(&opts, true, CursorMode::Prompt, &vec![]),
            CursorStyle::Bar
        );
    }

    #[test]
    fn cursor_style_insert_mode() {
        let opts = std::collections::HashMap::new();
        let mode_line = vec![make_atom("insert")];
        assert_eq!(
            derive_cursor_style(&opts, true, CursorMode::Buffer, &mode_line),
            CursorStyle::Bar
        );
    }

    #[test]
    fn cursor_style_replace_mode() {
        let opts = std::collections::HashMap::new();
        let mode_line = vec![make_atom("replace")];
        assert_eq!(
            derive_cursor_style(&opts, true, CursorMode::Buffer, &mode_line),
            CursorStyle::Underline
        );
    }

    #[test]
    fn cursor_style_normal_mode() {
        let opts = std::collections::HashMap::new();
        let mode_line = vec![make_atom("normal")];
        assert_eq!(
            derive_cursor_style(&opts, true, CursorMode::Buffer, &mode_line),
            CursorStyle::Block
        );
    }
}
