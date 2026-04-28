//! Shared atom width and face lookup utilities.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::protocol::{Atom, Face};

/// Compute the display width of an atom's contents.
pub(crate) fn atom_display_width(atom: &Atom) -> u32 {
    let mut width: u32 = 0;
    for grapheme in atom.contents.as_str().graphemes(true) {
        if grapheme.starts_with(|c: char| c.is_control()) {
            continue;
        }
        width += UnicodeWidthStr::width(grapheme) as u32;
    }
    debug_assert!(
        width < 10_000,
        "atom_display_width: unreasonable width {width} for atom {:?}",
        atom.contents.as_str(),
    );
    width
}

/// Compute the total display width of a line by summing atom widths.
pub(crate) fn line_atom_display_width(line: &[Atom]) -> u32 {
    line.iter().map(atom_display_width).sum()
}

/// Look up the face of the atom at a given (line, column) coordinate.
pub(super) fn face_at_coord(
    lines: &[crate::protocol::Line],
    pos: crate::protocol::Coord,
) -> Option<Face> {
    let line = lines.get(pos.line as usize)?;
    let target_col = pos.column as u32;
    let mut col: u32 = 0;
    for atom in line.iter() {
        let width = atom_display_width(atom);
        if col <= target_col && target_col < col + width.max(1) {
            return Some(atom.face());
        }
        col += width;
    }
    None
}
