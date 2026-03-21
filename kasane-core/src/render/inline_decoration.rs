//! Inline decoration: byte-range Style/Hide operations applied to buffer line atoms.

use std::ops::Range;

use crate::protocol::{Atom, Face};

/// An inline operation applied to a byte range within a buffer line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineOp {
    /// Override the face for the given byte range.
    Style { range: Range<usize>, face: Face },
    /// Hide the given byte range (omit from output).
    Hide { range: Range<usize> },
}

impl InlineOp {
    /// The byte range this operation covers.
    pub fn range(&self) -> &Range<usize> {
        match self {
            InlineOp::Style { range, .. } | InlineOp::Hide { range } => range,
        }
    }
}

/// A set of non-overlapping, sorted inline operations for a single line.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InlineDecoration {
    ops: Vec<InlineOp>,
}

impl InlineDecoration {
    /// Create a new `InlineDecoration` from a list of ops.
    ///
    /// In debug builds, asserts that ops are sorted by `range.start` and non-overlapping.
    pub fn new(ops: Vec<InlineOp>) -> Self {
        debug_assert!(
            ops.windows(2)
                .all(|w| w[0].range().end <= w[1].range().start),
            "InlineDecoration ops must be sorted by range.start and non-overlapping"
        );
        Self { ops }
    }

    /// Returns true if there are no operations.
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Access the operations slice.
    pub fn ops(&self) -> &[InlineOp] {
        &self.ops
    }
}

/// Apply inline operations to a slice of atoms, producing a new atom vector.
///
/// Algorithm: single-pass sweep maintaining a byte cursor across atoms and ops.
/// - Hide ops omit the covered sub-range from output.
/// - Style ops resolve the op face against the atom's face and emit.
/// - Regions not covered by any op pass through unchanged.
///
/// If `decoration` is empty, returns a clone of `atoms`.
pub fn apply_inline_ops(atoms: &[Atom], decoration: &InlineDecoration) -> Vec<Atom> {
    if decoration.is_empty() {
        return atoms.to_vec();
    }

    let ops = decoration.ops();
    let mut result = Vec::with_capacity(atoms.len());
    let mut op_idx = 0;
    let mut byte_cursor: usize = 0;

    for atom in atoms {
        let atom_start = byte_cursor;
        let atom_end = atom_start + atom.contents.len();
        byte_cursor = atom_end;

        // Fast path: no more ops or current op is beyond this atom
        if op_idx >= ops.len() || ops[op_idx].range().start >= atom_end {
            result.push(atom.clone());
            continue;
        }

        // This atom overlaps with one or more ops — split it
        let contents = atom.contents.as_str();
        let mut pos = atom_start; // absolute byte position

        while pos < atom_end {
            if op_idx >= ops.len() || ops[op_idx].range().start >= atom_end {
                // Remainder of atom: no more ops overlap
                let local_start = pos - atom_start;
                let local_end = atom_end - atom_start;
                if local_start < local_end {
                    let sub = &contents[local_start..local_end];
                    if !sub.is_empty() {
                        result.push(Atom {
                            face: atom.face,
                            contents: sub.into(),
                        });
                    }
                }
                break;
            }

            let op = &ops[op_idx];
            let op_range = op.range();

            if op_range.end <= pos {
                // Op is entirely before current position — advance
                op_idx += 1;
                continue;
            }

            if op_range.start > pos {
                // Gap before the op: emit unchanged sub-atom
                let gap_end = op_range.start.min(atom_end);
                let local_start = pos - atom_start;
                let local_end = gap_end - atom_start;
                // Clamp to char boundary
                let local_start = clamp_to_char_boundary(contents, local_start);
                let local_end = clamp_to_char_boundary(contents, local_end);
                if local_start < local_end {
                    result.push(Atom {
                        face: atom.face,
                        contents: contents[local_start..local_end].into(),
                    });
                }
                pos = gap_end;
                continue;
            }

            // Op overlaps with current position
            let effective_start = pos.max(op_range.start);
            let effective_end = atom_end.min(op_range.end);
            let local_start = clamp_to_char_boundary(contents, effective_start - atom_start);
            let local_end = clamp_to_char_boundary(contents, effective_end - atom_start);

            match op {
                InlineOp::Hide { .. } => {
                    // Skip this range
                }
                InlineOp::Style { face: op_face, .. } => {
                    if local_start < local_end {
                        result.push(Atom {
                            face: crate::protocol::resolve_face(op_face, &atom.face),
                            contents: contents[local_start..local_end].into(),
                        });
                    }
                }
            }

            pos = effective_end;
            if pos >= op_range.end {
                op_idx += 1;
            }
        }
    }

    result
}

/// Clamp a byte offset to the nearest char boundary (floor).
fn clamp_to_char_boundary(s: &str, offset: usize) -> usize {
    if offset >= s.len() {
        return s.len();
    }
    if s.is_char_boundary(offset) {
        return offset;
    }
    // Walk backward to find a valid boundary
    debug_assert!(
        false,
        "InlineOp byte range {offset} is not on a char boundary in {:?}",
        s
    );
    let mut i = offset;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Attributes, Color, Face, NamedColor};

    fn default_face() -> Face {
        Face::default()
    }

    fn red_face() -> Face {
        Face {
            fg: Color::Named(NamedColor::Red),
            ..Face::default()
        }
    }

    fn make_atom(text: &str, face: Face) -> Atom {
        Atom {
            face,
            contents: text.into(),
        }
    }

    #[test]
    fn empty_decoration_returns_clone() {
        let atoms = vec![make_atom("hello", default_face())];
        let deco = InlineDecoration::default();
        let result = apply_inline_ops(&atoms, &deco);
        assert_eq!(result, atoms);
    }

    #[test]
    fn single_hide() {
        // "hello world" — hide "world" (bytes 6..11)
        let atoms = vec![make_atom("hello world", default_face())];
        let deco = InlineDecoration::new(vec![InlineOp::Hide { range: 6..11 }]);
        let result = apply_inline_ops(&atoms, &deco);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].contents.as_str(), "hello ");
    }

    #[test]
    fn single_style() {
        // "hello world" — style "world" (bytes 6..11) red
        let atoms = vec![make_atom("hello world", default_face())];
        let deco = InlineDecoration::new(vec![InlineOp::Style {
            range: 6..11,
            face: red_face(),
        }]);
        let result = apply_inline_ops(&atoms, &deco);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].contents.as_str(), "hello ");
        assert_eq!(result[0].face, default_face());
        assert_eq!(result[1].contents.as_str(), "world");
        assert_eq!(result[1].face.fg, Color::Named(NamedColor::Red));
    }

    #[test]
    fn multiple_ops() {
        // "abcdefgh" — hide "cd" (2..4), style "gh" (6..8) red
        let atoms = vec![make_atom("abcdefgh", default_face())];
        let deco = InlineDecoration::new(vec![
            InlineOp::Hide { range: 2..4 },
            InlineOp::Style {
                range: 6..8,
                face: red_face(),
            },
        ]);
        let result = apply_inline_ops(&atoms, &deco);
        // "ab" + "ef" + "gh"(red)
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].contents.as_str(), "ab");
        assert_eq!(result[1].contents.as_str(), "ef");
        assert_eq!(result[2].contents.as_str(), "gh");
        assert_eq!(result[2].face.fg, Color::Named(NamedColor::Red));
    }

    #[test]
    fn op_spans_multiple_atoms() {
        // Two atoms: "hel" + "lo world"
        // Style bytes 0..5 ("hello") red — spans both atoms
        let atoms = vec![
            make_atom("hel", default_face()),
            make_atom("lo world", default_face()),
        ];
        let deco = InlineDecoration::new(vec![InlineOp::Style {
            range: 0..5,
            face: red_face(),
        }]);
        let result = apply_inline_ops(&atoms, &deco);
        // "hel"(red) + "lo"(red) + " world"
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].contents.as_str(), "hel");
        assert_eq!(result[0].face.fg, Color::Named(NamedColor::Red));
        assert_eq!(result[1].contents.as_str(), "lo");
        assert_eq!(result[1].face.fg, Color::Named(NamedColor::Red));
        assert_eq!(result[2].contents.as_str(), " world");
        assert_eq!(result[2].face, default_face());
    }

    #[test]
    fn op_on_atom_boundary() {
        // Two atoms: "hello" + " world" — style first atom exactly (0..5)
        let atoms = vec![
            make_atom("hello", default_face()),
            make_atom(" world", default_face()),
        ];
        let deco = InlineDecoration::new(vec![InlineOp::Style {
            range: 0..5,
            face: red_face(),
        }]);
        let result = apply_inline_ops(&atoms, &deco);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].contents.as_str(), "hello");
        assert_eq!(result[0].face.fg, Color::Named(NamedColor::Red));
        assert_eq!(result[1].contents.as_str(), " world");
        assert_eq!(result[1].face, default_face());
    }

    #[test]
    fn utf8_multibyte() {
        // "あいう" — each char is 3 bytes. Hide "い" (bytes 3..6)
        let atoms = vec![make_atom("あいう", default_face())];
        let deco = InlineDecoration::new(vec![InlineOp::Hide { range: 3..6 }]);
        let result = apply_inline_ops(&atoms, &deco);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].contents.as_str(), "あ");
        assert_eq!(result[1].contents.as_str(), "う");
    }

    #[test]
    fn hide_at_line_start() {
        let atoms = vec![make_atom("hello", default_face())];
        let deco = InlineDecoration::new(vec![InlineOp::Hide { range: 0..3 }]);
        let result = apply_inline_ops(&atoms, &deco);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].contents.as_str(), "lo");
    }

    #[test]
    fn hide_at_line_end() {
        let atoms = vec![make_atom("hello", default_face())];
        let deco = InlineDecoration::new(vec![InlineOp::Hide { range: 3..5 }]);
        let result = apply_inline_ops(&atoms, &deco);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].contents.as_str(), "hel");
    }

    #[test]
    fn hide_entire_line() {
        let atoms = vec![make_atom("hello", default_face())];
        let deco = InlineDecoration::new(vec![InlineOp::Hide { range: 0..5 }]);
        let result = apply_inline_ops(&atoms, &deco);
        assert!(result.is_empty());
    }

    #[test]
    fn emoji_multibyte() {
        // "a🎉b" — 🎉 is 4 bytes (offset 1..5)
        let text = "a🎉b";
        assert_eq!(text.len(), 6); // a(1) + 🎉(4) + b(1)
        let atoms = vec![make_atom(text, default_face())];
        let deco = InlineDecoration::new(vec![InlineOp::Style {
            range: 1..5,
            face: red_face(),
        }]);
        let result = apply_inline_ops(&atoms, &deco);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].contents.as_str(), "a");
        assert_eq!(result[1].contents.as_str(), "🎉");
        assert_eq!(result[1].face.fg, Color::Named(NamedColor::Red));
        assert_eq!(result[2].contents.as_str(), "b");
    }

    #[test]
    #[should_panic(expected = "non-overlapping")]
    fn overlapping_ops_asserts() {
        InlineDecoration::new(vec![
            InlineOp::Hide { range: 0..5 },
            InlineOp::Hide { range: 3..8 },
        ]);
    }

    #[test]
    fn new_validates_sorted() {
        // Adjacent but non-overlapping — should be fine
        let deco = InlineDecoration::new(vec![
            InlineOp::Hide { range: 0..3 },
            InlineOp::Style {
                range: 3..6,
                face: red_face(),
            },
        ]);
        assert_eq!(deco.ops().len(), 2);
    }
}
