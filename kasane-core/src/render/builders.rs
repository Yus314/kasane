//! Shared builder functions for menu and info overlays.
//!
//! These are used by both the non-cached `render::view` path and the Salsa-cached
//! `salsa_views` path, avoiding code duplication.

use compact_str::CompactString;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::element::{Element, FlexChild};
use crate::protocol::resolve_face;
use crate::protocol::{Atom, Face, Line};

/// Width of the scrollbar column (1 cell).
pub(crate) const SCROLLBAR_WIDTH: u16 = 1;

/// Width of the "< " prefix indicator in prompt-style menus.
pub(crate) const PREFIX_WIDTH: usize = 2;

/// Width reserved for the " >" suffix indicator in prompt-style menus.
pub(crate) const SUFFIX_RESERVE: usize = 2;

/// Maximum height for the search dropdown menu.
pub(crate) const MAX_DROPDOWN_HEIGHT: u16 = 10;

/// Truncate a slice of atoms to fit within `max_width` display columns.
///
/// If the content fits, resolves faces against `base_face` and returns as-is.
/// If it exceeds, truncates at `max_width - 1` and appends `truncation_char`.
pub(crate) fn truncate_atoms(
    atoms: &[Atom],
    max_width: u16,
    base_face: &Face,
    truncation_char: &str,
) -> Vec<Atom> {
    let max_w = max_width as usize;
    let total: usize = atoms
        .iter()
        .map(|a| {
            a.contents
                .split(|c: char| c.is_control())
                .map(UnicodeWidthStr::width)
                .sum::<usize>()
        })
        .sum();

    if total <= max_w {
        return atoms
            .iter()
            .map(|a| Atom {
                face: resolve_face(&a.face, base_face),
                contents: a.contents.clone(),
            })
            .collect();
    }

    // Truncate at max_width - 1 to leave room for "\u{2026}"
    let limit = max_w.saturating_sub(1);
    let mut result = Vec::new();
    let mut used = 0usize;
    for atom in atoms {
        let face = resolve_face(&atom.face, base_face);
        let mut buf = String::new();
        for grapheme in atom.contents.graphemes(true) {
            let cw = if grapheme.starts_with(|c: char| c.is_control()) {
                0
            } else {
                UnicodeWidthStr::width(grapheme)
            };
            if used + cw > limit {
                break;
            }
            buf.push_str(grapheme);
            used += cw;
        }
        if !buf.is_empty() {
            result.push(Atom {
                face,
                contents: buf.into(),
            });
        }
        if used >= limit {
            break;
        }
    }
    // Append truncation indicator with the base face
    result.push(Atom {
        face: *base_face,
        contents: truncation_char.into(),
    });
    result
}

/// Build a scrollbar column element from explicit parameters.
pub(crate) fn build_scrollbar(
    win_height: u16,
    item_count: usize,
    columns: u16,
    first_item: usize,
    face: &Face,
    thumb: &str,
    track: &str,
) -> Element {
    let wh = win_height as usize;
    if wh == 0 || item_count == 0 {
        return Element::Empty;
    }

    let menu_lines = item_count.div_ceil(columns as usize);
    let mark_h = (wh * wh).div_ceil(menu_lines).min(wh);
    let menu_cols = item_count.div_ceil(wh);
    let first_col = first_item / wh;
    let denom = menu_cols.saturating_sub(columns as usize).max(1);
    let mark_y = ((wh - mark_h) * first_col / denom).min(wh - mark_h);

    let mut rows: Vec<FlexChild> = Vec::new();
    for row in 0..wh {
        let ch = if row >= mark_y && row < mark_y + mark_h {
            thumb
        } else {
            track
        };
        rows.push(FlexChild::fixed(Element::text(ch, *face)));
    }

    Element::column(rows)
}

/// Word-wrap content lines and produce resolved StyledLine atoms per visual row.
pub(crate) fn wrap_content_lines(
    content: &[Line],
    max_width: u16,
    max_rows: u16,
    base_face: &Face,
) -> Vec<Vec<Atom>> {
    use crate::layout;

    if max_width == 0 {
        return vec![];
    }

    let mut result = Vec::new();

    for line in content {
        if result.len() >= max_rows as usize {
            break;
        }

        // Collect graphemes with resolved faces
        let mut graphemes: Vec<(&str, Face, u16)> = Vec::new();
        for atom in line {
            let face = resolve_face(&atom.face, base_face);
            for grapheme in atom.contents.graphemes(true) {
                if grapheme.is_empty() || grapheme.starts_with(|c: char| c.is_control()) {
                    continue;
                }
                let w = UnicodeWidthStr::width(grapheme) as u16;
                if w == 0 {
                    continue;
                }
                graphemes.push((grapheme, face, w));
            }
        }

        if graphemes.is_empty() {
            result.push(vec![Atom {
                face: *base_face,
                contents: CompactString::default(),
            }]);
            continue;
        }

        let metrics: Vec<(u16, bool)> = graphemes
            .iter()
            .map(|(text, _, w)| (*w, !layout::is_word_char(text)))
            .collect();
        let segments = layout::word_wrap_segments(&metrics, max_width);

        for seg in &segments {
            if result.len() >= max_rows as usize {
                break;
            }
            let mut row_atoms = Vec::new();
            let mut current_face: Option<Face> = None;
            let mut current_text = CompactString::default();

            for &(grapheme, face, _) in &graphemes[seg.start..seg.end] {
                if current_face == Some(face) {
                    current_text.push_str(grapheme);
                } else {
                    if let Some(cf) = current_face {
                        row_atoms.push(Atom {
                            face: cf,
                            contents: std::mem::take(&mut current_text),
                        });
                    }
                    current_face = Some(face);
                    current_text = CompactString::from(grapheme);
                }
            }
            if let Some(cf) = current_face {
                row_atoms.push(Atom {
                    face: cf,
                    contents: current_text,
                });
            }

            result.push(row_atoms);
        }
    }

    result
}

/// Wrap content lines and build a column element.
pub(crate) fn build_content_column(
    content: &[Line],
    max_w: u16,
    max_h: u16,
    face: &Face,
) -> Element {
    let wrapped_lines = wrap_content_lines(content, max_w, max_h, face);
    let content_rows: Vec<FlexChild> = wrapped_lines
        .iter()
        .map(|line| FlexChild::fixed(Element::StyledLine(line.clone())))
        .collect();
    Element::column(content_rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::make_line;

    #[test]
    fn test_wrap_content_lines_combining_character() {
        let line = make_line("e\u{0301}x");
        let base_face = Face::default();
        let result = wrap_content_lines(&[line], 20, 10, &base_face);
        assert_eq!(result.len(), 1);
        let text: String = result[0].iter().map(|a| a.contents.as_str()).collect();
        assert_eq!(text, "e\u{0301}x");
    }

    #[test]
    fn test_truncate_atoms_combining_character() {
        let atoms = vec![Atom {
            face: Face::default(),
            contents: "e\u{0301}xyz".into(),
        }];
        let base_face = Face::default();
        // width 3 limit → "éx" (2 cols) + "…" (1 col) = 3
        let result = truncate_atoms(&atoms, 3, &base_face, "\u{2026}");
        let text: String = result.iter().map(|a| a.contents.as_str()).collect();
        assert_eq!(text, "e\u{0301}x\u{2026}");
    }
}
