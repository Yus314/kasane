//! Inline decoration: byte-range Style/Hide/Insert operations applied to buffer line atoms.

use crate::protocol::{Atom, Face};

/// An inline operation applied within a buffer line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineOp {
    /// Insert virtual text atoms at the given byte gap position.
    Insert { at: usize, content: Vec<Atom> },
    /// Override the face for the given byte range.
    Style {
        range: std::ops::Range<usize>,
        face: Face,
    },
    /// Hide the given byte range (omit from output).
    Hide { range: std::ops::Range<usize> },
}

impl InlineOp {
    /// Unified sort key: (position, variant_order).
    /// Insert (0) sorts before Style/Hide (1) at the same position.
    pub fn sort_key(&self) -> (usize, u8) {
        match self {
            InlineOp::Insert { at, .. } => (*at, 0),
            InlineOp::Style { range, .. } | InlineOp::Hide { range } => (range.start, 1),
        }
    }

    /// Start position in buffer byte coordinates.
    fn start(&self) -> usize {
        self.sort_key().0
    }
}

/// A set of sorted inline operations for a single line.
///
/// Invariants (checked in debug builds):
/// - INV-INLINE-1: ops are sorted by `sort_key()` (position, then Insert before Style/Hide)
/// - INV-INLINE-2: range-based ops (Style/Hide) are non-overlapping
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InlineDecoration {
    ops: Vec<InlineOp>,
}

impl InlineDecoration {
    /// Create a new `InlineDecoration` from a list of ops.
    ///
    /// In debug builds, asserts that ops are sorted by `sort_key()` and
    /// range-based ops are non-overlapping.
    pub fn new(ops: Vec<InlineOp>) -> Self {
        #[cfg(debug_assertions)]
        {
            // INV-INLINE-1: sorted by sort_key
            debug_assert!(
                ops.windows(2).all(|w| w[0].sort_key() <= w[1].sort_key()),
                "InlineDecoration ops must be sorted by sort_key()"
            );
            // INV-INLINE-2: range-based ops are non-overlapping
            let mut prev_end: Option<usize> = None;
            for op in &ops {
                if let InlineOp::Style { range, .. } | InlineOp::Hide { range } = op {
                    if let Some(end) = prev_end {
                        debug_assert!(
                            end <= range.start,
                            "InlineDecoration range ops must be non-overlapping: prev_end={end}, start={}",
                            range.start
                        );
                    }
                    prev_end = Some(range.end);
                }
            }
        }
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
/// - Insert ops inject virtual text atoms at byte gap positions.
/// - Hide ops omit the covered sub-range from output.
/// - Style ops resolve the op face against the atom's face and emit.
/// - Regions not covered by any op pass through unchanged.
///
/// Insert ops inside a Hide range are still emitted (S1 semantics).
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

        // Drain Inserts at atom_start gap
        drain_inserts(ops, &mut op_idx, atom_start, &mut result);

        // Fast path: no more ops or next op beyond this atom
        if op_idx >= ops.len() || ops[op_idx].start() >= atom_end {
            result.push(atom.clone());
            continue;
        }

        // Slow path: this atom overlaps with one or more ops — split it
        let contents = atom.contents.as_str();
        let mut pos = atom_start;

        while pos < atom_end {
            // Drain Inserts at current gap position
            drain_inserts(ops, &mut op_idx, pos, &mut result);

            if op_idx >= ops.len() || ops[op_idx].start() >= atom_end {
                // Remainder of atom: no more ops overlap
                emit_sub_atom(
                    contents,
                    pos - atom_start,
                    atom_end - atom_start,
                    atom.face(),
                    &mut result,
                );
                break;
            }

            let op = &ops[op_idx];
            match op {
                InlineOp::Insert { .. } => {
                    // Insert with at > pos but at < atom_end: emit gap up to it
                    let gap_end = op.start().min(atom_end);
                    emit_sub_atom(
                        contents,
                        pos - atom_start,
                        gap_end - atom_start,
                        atom.face(),
                        &mut result,
                    );
                    pos = gap_end;
                }
                InlineOp::Hide { range } => {
                    let mut cx = InlineOpContext {
                        op_idx: &mut op_idx,
                        pos: &mut pos,
                        atom_end,
                        contents,
                        atom_start,
                        atom_face: atom.face(),
                        result: &mut result,
                    };
                    if advance_hide(range, &mut cx) {
                        continue;
                    }
                }
                InlineOp::Style {
                    range,
                    face: op_face,
                } => {
                    let mut cx = InlineOpContext {
                        op_idx: &mut op_idx,
                        pos: &mut pos,
                        atom_end,
                        contents,
                        atom_start,
                        atom_face: atom.face(),
                        result: &mut result,
                    };
                    if advance_style(range, op_face, &mut cx) {
                        continue;
                    }
                }
            }
        }
    }

    // Trailing Inserts (at or past end of all atoms)
    drain_inserts(ops, &mut op_idx, usize::MAX, &mut result);
    result
}

/// Shared mutable context for inline op processing within a single atom.
struct InlineOpContext<'a> {
    op_idx: &'a mut usize,
    pos: &'a mut usize,
    atom_end: usize,
    contents: &'a str,
    atom_start: usize,
    atom_face: Face,
    result: &'a mut Vec<Atom>,
}

/// Process a Hide op: emit any gap before the hidden range, then skip hidden bytes.
/// Returns `true` when the caller's `while` loop should `continue`.
fn advance_hide(range: &std::ops::Range<usize>, cx: &mut InlineOpContext<'_>) -> bool {
    if range.end <= *cx.pos {
        *cx.op_idx += 1;
        return true;
    }
    if range.start > *cx.pos {
        let gap_end = range.start.min(cx.atom_end);
        emit_sub_atom(
            cx.contents,
            *cx.pos - cx.atom_start,
            gap_end - cx.atom_start,
            cx.atom_face,
            cx.result,
        );
        *cx.pos = gap_end;
        return true;
    }
    // Hide overlaps current position — skip
    let effective_end = cx.atom_end.min(range.end);
    *cx.pos = effective_end;
    if *cx.pos >= range.end {
        *cx.op_idx += 1;
    }
    false
}

/// Process a Style op: emit any gap before the styled range, then emit styled bytes.
/// Returns `true` when the caller's `while` loop should `continue`.
fn advance_style(
    range: &std::ops::Range<usize>,
    op_face: &Face,
    cx: &mut InlineOpContext<'_>,
) -> bool {
    if range.end <= *cx.pos {
        *cx.op_idx += 1;
        return true;
    }
    if range.start > *cx.pos {
        let gap_end = range.start.min(cx.atom_end);
        emit_sub_atom(
            cx.contents,
            *cx.pos - cx.atom_start,
            gap_end - cx.atom_start,
            cx.atom_face,
            cx.result,
        );
        *cx.pos = gap_end;
        return true;
    }
    // Style overlaps — emit with resolved face
    let effective_start = (*cx.pos).max(range.start);
    let effective_end = cx.atom_end.min(range.end);
    let local_start = clamp_to_char_boundary(cx.contents, effective_start - cx.atom_start);
    let local_end = clamp_to_char_boundary(cx.contents, effective_end - cx.atom_start);
    if local_start < local_end {
        cx.result.push(Atom::from_face(
            crate::protocol::resolve_face(op_face, &cx.atom_face),
            &cx.contents[local_start..local_end],
        ));
    }
    *cx.pos = effective_end;
    if *cx.pos >= range.end {
        *cx.op_idx += 1;
    }
    false
}

/// Emit all consecutive Insert ops whose `at <= pos`.
fn drain_inserts(ops: &[InlineOp], op_idx: &mut usize, pos: usize, result: &mut Vec<Atom>) {
    while *op_idx < ops.len() {
        if let InlineOp::Insert { at, content } = &ops[*op_idx]
            && *at <= pos
        {
            result.extend(content.iter().cloned());
            *op_idx += 1;
            continue;
        }
        break;
    }
}

/// Emit a sub-range of atom contents if non-empty, with char boundary clamping.
fn emit_sub_atom(
    contents: &str,
    local_start: usize,
    local_end: usize,
    face: Face,
    result: &mut Vec<Atom>,
) {
    let start = clamp_to_char_boundary(contents, local_start);
    let end = clamp_to_char_boundary(contents, local_end);
    if start < end {
        let sub = &contents[start..end];
        if !sub.is_empty() {
            result.push(Atom::from_face(face, sub));
        }
    }
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
    use crate::protocol::{Color, Face, NamedColor};

    fn default_face() -> Face {
        Face::default()
    }

    fn red_face() -> Face {
        Face {
            fg: Color::Named(NamedColor::Red),
            ..Face::default()
        }
    }

    fn blue_face() -> Face {
        Face {
            fg: Color::Named(NamedColor::Blue),
            ..Face::default()
        }
    }

    fn make_atom(text: &str, face: Face) -> Atom {
        Atom::from_face(face, text)
    }

    // ---- Existing tests (Style/Hide) ----

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
        assert_eq!(result[0].face(), default_face());
        assert_eq!(result[1].contents.as_str(), "world");
        assert_eq!(result[1].face().fg, Color::Named(NamedColor::Red));
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
        assert_eq!(result[2].face().fg, Color::Named(NamedColor::Red));
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
        assert_eq!(result[0].face().fg, Color::Named(NamedColor::Red));
        assert_eq!(result[1].contents.as_str(), "lo");
        assert_eq!(result[1].face().fg, Color::Named(NamedColor::Red));
        assert_eq!(result[2].contents.as_str(), " world");
        assert_eq!(result[2].face(), default_face());
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
        assert_eq!(result[0].face().fg, Color::Named(NamedColor::Red));
        assert_eq!(result[1].contents.as_str(), " world");
        assert_eq!(result[1].face(), default_face());
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
        assert_eq!(result[1].face().fg, Color::Named(NamedColor::Red));
        assert_eq!(result[2].contents.as_str(), "b");
    }

    #[test]
    #[cfg(debug_assertions)]
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

    // ---- Insert tests ----

    #[test]
    fn insert_at_start() {
        let atoms = vec![make_atom("hello", default_face())];
        let deco = InlineDecoration::new(vec![InlineOp::Insert {
            at: 0,
            content: vec![make_atom(">>", red_face())],
        }]);
        let result = apply_inline_ops(&atoms, &deco);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].contents.as_str(), ">>");
        assert_eq!(result[0].face(), red_face());
        assert_eq!(result[1].contents.as_str(), "hello");
    }

    #[test]
    fn insert_at_end() {
        let atoms = vec![make_atom("hello", default_face())];
        let deco = InlineDecoration::new(vec![InlineOp::Insert {
            at: 5,
            content: vec![make_atom("<<", red_face())],
        }]);
        let result = apply_inline_ops(&atoms, &deco);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].contents.as_str(), "hello");
        assert_eq!(result[1].contents.as_str(), "<<");
    }

    #[test]
    fn insert_in_middle() {
        let atoms = vec![make_atom("hello", default_face())];
        let deco = InlineDecoration::new(vec![InlineOp::Insert {
            at: 3,
            content: vec![make_atom("|", red_face())],
        }]);
        let result = apply_inline_ops(&atoms, &deco);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].contents.as_str(), "hel");
        assert_eq!(result[1].contents.as_str(), "|");
        assert_eq!(result[1].face(), red_face());
        assert_eq!(result[2].contents.as_str(), "lo");
    }

    #[test]
    fn insert_at_atom_boundary() {
        // Two atoms: "hel" + "lo" — insert at byte 3 (boundary)
        let atoms = vec![
            make_atom("hel", default_face()),
            make_atom("lo", default_face()),
        ];
        let deco = InlineDecoration::new(vec![InlineOp::Insert {
            at: 3,
            content: vec![make_atom("|", red_face())],
        }]);
        let result = apply_inline_ops(&atoms, &deco);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].contents.as_str(), "hel");
        assert_eq!(result[1].contents.as_str(), "|");
        assert_eq!(result[2].contents.as_str(), "lo");
    }

    #[test]
    fn insert_inside_hide() {
        // S1 semantics: Hide{2..8} + Insert{at:5} on "abcdefghij"
        // → "ab" + [Insert content] + "ij"
        let atoms = vec![make_atom("abcdefghij", default_face())];
        let deco = InlineDecoration::new(vec![
            InlineOp::Hide { range: 2..8 },
            InlineOp::Insert {
                at: 5,
                content: vec![make_atom("NEW", red_face())],
            },
        ]);
        let result = apply_inline_ops(&atoms, &deco);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].contents.as_str(), "ab");
        assert_eq!(result[1].contents.as_str(), "NEW");
        assert_eq!(result[1].face(), red_face());
        assert_eq!(result[2].contents.as_str(), "ij");
    }

    #[test]
    fn insert_at_hide_start() {
        // Insert{at:2} + Hide{2..5} on "abcde"
        // → "ab" + [Insert] (rest hidden)
        let atoms = vec![make_atom("abcde", default_face())];
        let deco = InlineDecoration::new(vec![
            InlineOp::Insert {
                at: 2,
                content: vec![make_atom("X", red_face())],
            },
            InlineOp::Hide { range: 2..5 },
        ]);
        let result = apply_inline_ops(&atoms, &deco);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].contents.as_str(), "ab");
        assert_eq!(result[1].contents.as_str(), "X");
    }

    #[test]
    fn insert_with_style() {
        // Insert{at:3} + Style{3..6, red} on "abcdef"
        // → "abc" + [Insert] + "def"(red)
        let atoms = vec![make_atom("abcdef", default_face())];
        let deco = InlineDecoration::new(vec![
            InlineOp::Insert {
                at: 3,
                content: vec![make_atom("!", blue_face())],
            },
            InlineOp::Style {
                range: 3..6,
                face: red_face(),
            },
        ]);
        let result = apply_inline_ops(&atoms, &deco);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].contents.as_str(), "abc");
        assert_eq!(result[1].contents.as_str(), "!");
        assert_eq!(result[1].face(), blue_face());
        assert_eq!(result[2].contents.as_str(), "def");
        assert_eq!(result[2].face().fg, Color::Named(NamedColor::Red));
    }

    #[test]
    fn multiple_inserts_same_position() {
        // Two Insert ops at position 3 — both should appear in order
        let atoms = vec![make_atom("hello", default_face())];
        let deco = InlineDecoration::new(vec![
            InlineOp::Insert {
                at: 3,
                content: vec![make_atom("X", red_face())],
            },
            InlineOp::Insert {
                at: 3,
                content: vec![make_atom("Y", blue_face())],
            },
        ]);
        let result = apply_inline_ops(&atoms, &deco);
        assert_eq!(result.len(), 4);
        assert_eq!(result[0].contents.as_str(), "hel");
        assert_eq!(result[1].contents.as_str(), "X");
        assert_eq!(result[2].contents.as_str(), "Y");
        assert_eq!(result[3].contents.as_str(), "lo");
    }

    #[test]
    fn insert_multibyte() {
        // "あいう" — insert after "あ" (byte 3)
        let atoms = vec![make_atom("あいう", default_face())];
        let deco = InlineDecoration::new(vec![InlineOp::Insert {
            at: 3,
            content: vec![make_atom("|", red_face())],
        }]);
        let result = apply_inline_ops(&atoms, &deco);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].contents.as_str(), "あ");
        assert_eq!(result[1].contents.as_str(), "|");
        assert_eq!(result[2].contents.as_str(), "いう");
    }

    #[test]
    fn insert_content_multiple_atoms() {
        // Insert with multiple atoms in content
        let atoms = vec![make_atom("hello", default_face())];
        let deco = InlineDecoration::new(vec![InlineOp::Insert {
            at: 3,
            content: vec![
                make_atom("[", red_face()),
                make_atom("new", blue_face()),
                make_atom("]", red_face()),
            ],
        }]);
        let result = apply_inline_ops(&atoms, &deco);
        assert_eq!(result.len(), 5);
        assert_eq!(result[0].contents.as_str(), "hel");
        assert_eq!(result[1].contents.as_str(), "[");
        assert_eq!(result[2].contents.as_str(), "new");
        assert_eq!(result[3].contents.as_str(), "]");
        assert_eq!(result[4].contents.as_str(), "lo");
    }

    #[test]
    fn insert_empty_content() {
        // Insert with empty content — no change to output
        let atoms = vec![make_atom("hello", default_face())];
        let deco = InlineDecoration::new(vec![InlineOp::Insert {
            at: 3,
            content: vec![],
        }]);
        let result = apply_inline_ops(&atoms, &deco);
        // With the current algorithm, "hel" + "lo" (split at insert point)
        // This is acceptable — the split doesn't change semantic content
        let text: String = result.iter().map(|a| a.contents.as_str()).collect();
        assert_eq!(text, "hello");
    }

    #[test]
    fn insert_past_end() {
        // Insert at position beyond text — trailing drain catches it
        let atoms = vec![make_atom("hello", default_face())];
        let deco = InlineDecoration::new(vec![InlineOp::Insert {
            at: 100,
            content: vec![make_atom("!", red_face())],
        }]);
        let result = apply_inline_ops(&atoms, &deco);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].contents.as_str(), "hello");
        assert_eq!(result[1].contents.as_str(), "!");
    }

    #[test]
    fn invariant_insert_before_style_same_pos() {
        // Insert at 3, Style at 3..5 — Insert sorts first, should be accepted
        let deco = InlineDecoration::new(vec![
            InlineOp::Insert {
                at: 3,
                content: vec![make_atom("X", red_face())],
            },
            InlineOp::Style {
                range: 3..5,
                face: red_face(),
            },
        ]);
        assert_eq!(deco.ops().len(), 2);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "sorted by sort_key")]
    fn invariant_unsorted_panics() {
        // Style at 3..5 before Insert at 2 — unsorted, should panic
        InlineDecoration::new(vec![
            InlineOp::Style {
                range: 3..5,
                face: red_face(),
            },
            InlineOp::Insert {
                at: 2,
                content: vec![make_atom("X", red_face())],
            },
        ]);
    }

    #[test]
    fn hide_plus_insert_replace_pattern() {
        // Replace pattern: Hide{3..6} + Insert{at:3, "new"} on "abcdefghi"
        // → "abc" + "new" + "ghi"
        let atoms = vec![make_atom("abcdefghi", default_face())];
        let deco = InlineDecoration::new(vec![
            InlineOp::Insert {
                at: 3,
                content: vec![make_atom("new", red_face())],
            },
            InlineOp::Hide { range: 3..6 },
        ]);
        let result = apply_inline_ops(&atoms, &deco);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].contents.as_str(), "abc");
        assert_eq!(result[1].contents.as_str(), "new");
        assert_eq!(result[1].face(), red_face());
        assert_eq!(result[2].contents.as_str(), "ghi");
    }
}
