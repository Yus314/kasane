use compact_str::CompactString;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::protocol::{Atom, Face, resolve_face};
use crate::render::terminal_style::TerminalStyle;

// ---------------------------------------------------------------------------
// Cell + CellGrid
// ---------------------------------------------------------------------------

/// One column of a [`CellGrid`].
///
/// Carries the SGR-emit-ready [`TerminalStyle`] projection of the
/// originating styled atom (design δ, ADR-031 Phase 3 follow-up). Cell
/// is the rasterised TUI output; storing the richer [`Style`] would be
/// wasted since terminals cannot render variable-axis font weight,
/// font features, letter-spacing, or bidi overrides anyway. The
/// projection happens once at paint time (in [`CellGrid::put_char`] /
/// [`CellGrid::clear`] / etc.); the backend reads `cell.style` directly
/// and emits SGR without any further conversion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub grapheme: CompactString,
    pub style: TerminalStyle,
    /// Display width: 1 for normal, 2 for wide chars, 0 for continuation cells.
    pub width: u8,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            grapheme: CompactString::const_new(" "),
            style: TerminalStyle::default(),
            width: 1,
        }
    }
}

impl Cell {
    /// Project the cell's [`TerminalStyle`] back to the legacy [`Face`]
    /// representation. Bridge for sites that still consume `Face` (tests,
    /// theme APIs, plugin decoration emit). Drops fields that have no
    /// `Face` equivalent. Retires when Phase B3 removes [`Face`].
    #[inline]
    pub fn face(&self) -> Face {
        terminal_style_to_face(&self.style)
    }

    /// Apply a [`TerminalStyle`]-level mutation to the cell. The mutation
    /// runs directly on the stored style, eliminating the
    /// `TerminalStyle ↔ Face ↔ bitflags` round-trip that the legacy
    /// `with_face_mut` bridge paid on every decoration / ornament merge.
    #[inline]
    pub fn with_style_mut<F: FnOnce(&mut TerminalStyle)>(&mut self, f: F) {
        f(&mut self.style);
    }
}

/// Lossy projection from [`TerminalStyle`] back to legacy [`Face`].
///
/// Bridge that keeps `cell.face()` working during the design-δ migration.
/// Used by tests and any code path that still pattern-matches on `Face`
/// fields. Dropped in Phase B3 along with [`Face`] itself.
fn terminal_style_to_face(ts: &TerminalStyle) -> Face {
    use crate::protocol::Attributes;
    let mut attrs = Attributes::empty();
    if ts.bold {
        attrs |= Attributes::BOLD;
    }
    if ts.italic {
        attrs |= Attributes::ITALIC;
    }
    if ts.dim {
        attrs |= Attributes::DIM;
    }
    if ts.blink {
        attrs |= Attributes::BLINK;
    }
    if ts.reverse {
        attrs |= Attributes::REVERSE;
    }
    if ts.strikethrough {
        attrs |= Attributes::STRIKETHROUGH;
    }
    use crate::render::terminal_style::UnderlineKind;
    match ts.underline {
        UnderlineKind::None => {}
        UnderlineKind::Solid => attrs |= Attributes::UNDERLINE,
        UnderlineKind::Curly => attrs |= Attributes::CURLY_UNDERLINE,
        UnderlineKind::Dotted => attrs |= Attributes::DOTTED_UNDERLINE,
        UnderlineKind::Dashed => attrs |= Attributes::DASHED_UNDERLINE,
        UnderlineKind::Double => attrs |= Attributes::DOUBLE_UNDERLINE,
    }
    Face {
        fg: ts.fg,
        bg: ts.bg,
        underline: ts.underline_color,
        attributes: attrs,
    }
}

pub struct CellGrid {
    width: u16,
    height: u16,
    current: Vec<Cell>,
    previous: Vec<Cell>,
    dirty_rows: Vec<bool>,
    newline_display: CompactString,
    truncation_char: CompactString,
}

impl CellGrid {
    pub fn new(width: u16, height: u16) -> Self {
        let size = width as usize * height as usize;
        CellGrid {
            width,
            height,
            current: vec![Cell::default(); size],
            previous: Vec::new(), // empty means "invalidated — full redraw needed"
            dirty_rows: vec![true; height as usize],
            newline_display: CompactString::const_new(" "),
            truncation_char: CompactString::const_new("\u{2026}"),
        }
    }

    /// Set the replacement string for newline characters.
    pub fn set_newline_display(&mut self, s: &str) {
        self.newline_display = CompactString::new(s);
    }

    /// Set the truncation indicator character.
    pub fn set_truncation_char(&mut self, s: &str) {
        self.truncation_char = CompactString::new(s);
    }

    /// The configured truncation character string.
    pub fn truncation_char(&self) -> &str {
        &self.truncation_char
    }

    pub fn width(&self) -> u16 {
        self.width
    }

    pub fn height(&self) -> u16 {
        self.height
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        let size = width as usize * height as usize;
        self.current = vec![Cell::default(); size];
        self.previous = Vec::new();
        self.dirty_rows = vec![true; height as usize];
    }

    fn idx(&self, x: u16, y: u16) -> usize {
        y as usize * self.width as usize + x as usize
    }

    pub fn put_char(&mut self, x: u16, y: u16, grapheme: &str, face: &Face) {
        if x >= self.width || y >= self.height {
            return;
        }
        self.dirty_rows[y as usize] = true;
        let w = UnicodeWidthStr::width(grapheme) as u8;
        let idx = self.idx(x, y);

        // --- Clean up orphaned wide-character halves before overwriting ---

        // If overwriting a continuation cell (width 0), the wide char at x-1 is orphaned.
        if self.current[idx].width == 0 && x > 0 {
            let prev_idx = self.idx(x - 1, y);
            self.current[prev_idx].grapheme = CompactString::const_new(" ");
            self.current[prev_idx].width = 1;
        }

        // If overwriting a wide char (width 2), its continuation at x+1 is orphaned.
        if self.current[idx].width == 2 && x + 1 < self.width {
            let next_idx = self.idx(x + 1, y);
            self.current[next_idx].grapheme = CompactString::const_new(" ");
            self.current[next_idx].width = 1;
        }

        // If placing a wide char, x+1 will become our continuation.
        // If x+1 is currently a wide char, its continuation at x+2 is orphaned.
        if w == 2 && x + 1 < self.width {
            let next_idx = self.idx(x + 1, y);
            if self.current[next_idx].width == 2 && x + 2 < self.width {
                let next2_idx = self.idx(x + 2, y);
                self.current[next2_idx].grapheme = CompactString::const_new(" ");
                self.current[next2_idx].width = 1;
            }
        }

        // --- Write the new cell ---

        let style = TerminalStyle::from_face(face);
        self.current[idx] = Cell {
            grapheme: CompactString::from(grapheme),
            style,
            width: w,
        };
        // If wide character, mark next cell as continuation
        if w == 2 && x + 1 < self.width {
            let next_idx = self.idx(x + 1, y);
            self.current[next_idx] = Cell {
                grapheme: CompactString::default(),
                style,
                width: 0,
            };
        }
    }

    /// Write a protocol Line into the grid at row `y` starting at column `x_start`.
    /// Returns the number of columns consumed.
    pub fn put_line(&mut self, y: u16, x_start: u16, line: &[Atom], max_width: u16) -> u16 {
        self.put_line_with_base(y, x_start, line, max_width, None)
    }

    /// Write a protocol Line, resolving `Color::Default` against `base_face`.
    /// When base_face is Some, atom Default fg/bg inherit from it (Kakoune semantics).
    pub fn put_line_with_base(
        &mut self,
        y: u16,
        x_start: u16,
        line: &[Atom],
        max_width: u16,
        base_face: Option<&Face>,
    ) -> u16 {
        let mut x = x_start;
        let limit = x_start.saturating_add(max_width).min(self.width);

        for atom in line {
            let atom_face = atom.face();
            let face = match base_face {
                Some(base) => resolve_face(&atom_face, base),
                None => atom_face,
            };
            for grapheme in atom.contents.graphemes(true) {
                if grapheme.is_empty() {
                    continue;
                }
                // Skip control characters: UnicodeWidthStr::width() returns 1
                // for \n, \r, etc. in unicode-width 0.2.x, but they must never
                // be placed in the grid (printing them would corrupt the terminal).
                if grapheme == "\n" {
                    let nl = self.newline_display.clone();
                    let nl_w = UnicodeWidthStr::width(nl.as_str()).max(1) as u16;
                    if x + nl_w > limit {
                        break;
                    }
                    self.put_char(x, y, &nl, &face);
                    x += nl_w;
                    continue;
                }
                if grapheme.starts_with(|c: char| c.is_control()) {
                    continue;
                }
                let w = UnicodeWidthStr::width(grapheme) as u16;
                if w == 0 {
                    // Zero-width character — skip for now
                    continue;
                }
                if x + w > limit {
                    break;
                }
                self.put_char(x, y, grapheme, &face);
                x += w;
            }
        }

        x - x_start
    }

    pub fn clear(&mut self, face: &Face) {
        let style = TerminalStyle::from_face(face);
        for cell in &mut self.current {
            cell.grapheme = CompactString::const_new(" ");
            cell.style = style;
            cell.width = 1;
        }
        for d in &mut self.dirty_rows {
            *d = true;
        }
    }

    pub fn fill_row(&mut self, y: u16, face: &Face) {
        if y >= self.height {
            return;
        }
        let style = TerminalStyle::from_face(face);
        self.dirty_rows[y as usize] = true;
        for x in 0..self.width {
            let idx = self.idx(x, y);
            self.current[idx] = Cell {
                grapheme: CompactString::const_new(" "),
                style,
                width: 1,
            };
        }
    }

    /// Fill a horizontal span of a single row with a face.
    /// Only affects columns `x_start..x_start+width`, leaving other columns untouched.
    pub fn fill_region(&mut self, y: u16, x_start: u16, width: u16, face: &Face) {
        if y >= self.height {
            return;
        }
        let style = TerminalStyle::from_face(face);
        self.dirty_rows[y as usize] = true;
        let x_end = (x_start + width).min(self.width);
        for x in x_start..x_end {
            let idx = self.idx(x, y);
            self.current[idx] = Cell {
                grapheme: CompactString::const_new(" "),
                style,
                width: 1,
            };
        }
    }

    /// Clear only a rectangular region of the grid to the given face.
    pub fn clear_region(&mut self, rect: &crate::layout::Rect, face: &Face) {
        let style = TerminalStyle::from_face(face);
        let x_end = (rect.x + rect.w).min(self.width);
        let y_end = (rect.y + rect.h).min(self.height);
        for y in rect.y..y_end {
            self.dirty_rows[y as usize] = true;
            for x in rect.x..x_end {
                let idx = self.idx(x, y);
                self.current[idx] = Cell {
                    grapheme: CompactString::const_new(" "),
                    style,
                    width: 1,
                };
            }
        }
    }

    /// Mark rows within a rectangular region as dirty for diff.
    pub fn mark_region_dirty(&mut self, rect: &crate::layout::Rect) {
        let y_end = (rect.y + rect.h).min(self.height);
        for y in rect.y..y_end {
            self.dirty_rows[y as usize] = true;
        }
    }

    /// Same logic as `diff()` but reuses the provided buffer, avoiding per-frame allocation.
    #[doc(hidden)]
    pub fn diff_into(&self, buf: &mut Vec<CellDiff>) {
        crate::perf::perf_span!("grid_diff_into");
        buf.clear();
        if self.previous.is_empty() {
            // Full redraw
            for (i, cell) in self.current.iter().enumerate() {
                if cell.width > 0 {
                    let x = (i % self.width as usize) as u16;
                    let y = (i / self.width as usize) as u16;
                    buf.push(CellDiff {
                        x,
                        y,
                        cell: cell.clone(),
                    });
                }
            }
            return;
        }

        let w = self.width as usize;
        for row in 0..self.height as usize {
            if !self.dirty_rows[row] {
                continue;
            }
            let row_start = row * w;
            let row_end = row_start + w;
            for i in row_start..row_end {
                let curr = &self.current[i];
                let prev = &self.previous[i];
                if curr != prev && curr.width > 0 {
                    buf.push(CellDiff {
                        x: (i % w) as u16,
                        y: row as u16,
                        cell: curr.clone(),
                    });
                }
            }
        }
    }

    /// Zero-copy iterator yielding `(x, y, &Cell)` for changed cells.
    /// Uses the same dirty-row and previous-comparison logic as `diff()`,
    /// but yields references instead of cloning.
    #[doc(hidden)]
    pub fn iter_diffs(&self) -> impl Iterator<Item = (u16, u16, &Cell)> + '_ {
        let w = self.width as usize;
        let full_redraw = self.previous.is_empty();
        self.current
            .iter()
            .enumerate()
            .filter(move |(i, cell)| {
                if cell.width == 0 {
                    return false;
                }
                if full_redraw {
                    return true;
                }
                let row = i / w;
                if !self.dirty_rows[row] {
                    return false;
                }
                **cell != self.previous[*i]
            })
            .map(move |(i, cell)| {
                let x = (i % w) as u16;
                let y = (i / w) as u16;
                (x, y, cell)
            })
    }

    /// Returns true if this is the first frame (no previous buffer yet).
    #[doc(hidden)]
    pub fn is_first_frame(&self) -> bool {
        self.previous.is_empty()
    }

    #[doc(hidden)]
    pub fn diff(&self) -> Vec<CellDiff> {
        let mut ops = Vec::new();
        self.diff_into(&mut ops);
        ops
    }

    /// Swap only dirty rows from current into previous, preserving clean rows
    /// in both buffers. After this call, `current` retains all painted content
    /// (clean rows keep valid data from the previous frame for paint to skip),
    /// and `previous` is updated only for dirty rows.
    #[doc(hidden)]
    pub fn swap_with_dirty(&mut self) {
        let w = self.width as usize;
        let size = w * self.height as usize;
        if self.previous.len() != size {
            // First frame or resize: fall back to full swap
            self.swap();
            return;
        }
        for y in 0..self.height as usize {
            if self.dirty_rows[y] {
                let start = y * w;
                let end = start + w;
                self.previous[start..end].clone_from_slice(&self.current[start..end]);
            }
        }
        for d in &mut self.dirty_rows {
            *d = false;
        }
    }

    #[doc(hidden)]
    pub fn swap(&mut self) {
        crate::perf::perf_span!("grid_swap");
        std::mem::swap(&mut self.previous, &mut self.current);
        let size = self.width as usize * self.height as usize;
        if self.current.len() == size {
            for cell in &mut self.current {
                cell.grapheme = CompactString::const_new(" ");
                cell.style = TerminalStyle::default();
                cell.width = 1;
            }
        } else {
            self.current.clear();
            self.current.resize(size, Cell::default());
        }
        for d in &mut self.dirty_rows {
            *d = false;
        }
    }

    #[doc(hidden)]
    pub fn invalidate_all(&mut self) {
        self.previous.clear();
        for d in &mut self.dirty_rows {
            *d = true;
        }
    }

    /// Access the raw cell buffer (read-only).
    pub fn cells(&self) -> &[Cell] {
        &self.current
    }

    /// Per-row dirty flags set by paint operations.
    pub fn dirty_rows(&self) -> &[bool] {
        &self.dirty_rows
    }

    /// Clear all dirty-row flags.
    pub fn clear_dirty(&mut self) {
        for d in &mut self.dirty_rows {
            *d = false;
        }
    }

    /// Mark all rows as dirty (e.g. after resize).
    pub fn mark_all_dirty(&mut self) {
        for d in &mut self.dirty_rows {
            *d = true;
        }
    }

    /// Direct access to a cell in the current buffer.
    pub fn get(&self, x: u16, y: u16) -> Option<&Cell> {
        if x < self.width && y < self.height {
            Some(&self.current[self.idx(x, y)])
        } else {
            None
        }
    }

    /// Mutable access to a cell in the current buffer.
    pub fn get_mut(&mut self, x: u16, y: u16) -> Option<&mut Cell> {
        if x < self.width && y < self.height {
            self.dirty_rows[y as usize] = true;
            let idx = self.idx(x, y);
            Some(&mut self.current[idx])
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// CellDiff
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CellDiff {
    pub x: u16,
    pub y: u16,
    pub cell: Cell,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Atom, Attributes, Color, Face, NamedColor, Style, resolve_face};
    use crate::test_utils::make_line;

    fn default_face() -> Face {
        Face::default()
    }

    #[test]
    fn test_grid_new() {
        let grid = CellGrid::new(10, 5);
        assert_eq!(grid.width(), 10);
        assert_eq!(grid.height(), 5);
        assert_eq!(grid.current.len(), 50);
    }

    #[test]
    fn test_put_char() {
        let mut grid = CellGrid::new(10, 5);
        let face = default_face();
        grid.put_char(0, 0, "A", &face);
        assert_eq!(grid.get(0, 0).unwrap().grapheme, "A");
        assert_eq!(grid.get(0, 0).unwrap().width, 1);
    }

    #[test]
    fn test_put_wide_char() {
        let mut grid = CellGrid::new(10, 5);
        let face = default_face();
        grid.put_char(0, 0, "漢", &face);
        assert_eq!(grid.get(0, 0).unwrap().grapheme, "漢");
        assert_eq!(grid.get(0, 0).unwrap().width, 2);
        // Continuation cell
        assert_eq!(grid.get(1, 0).unwrap().width, 0);
    }

    #[test]
    fn test_put_line() {
        let mut grid = CellGrid::new(20, 5);
        let line = make_line("hello");
        let cols = grid.put_line(0, 0, &line, 20);
        assert_eq!(cols, 5);
        assert_eq!(grid.get(0, 0).unwrap().grapheme, "h");
        assert_eq!(grid.get(4, 0).unwrap().grapheme, "o");
    }

    #[test]
    fn test_put_line_cjk() {
        let mut grid = CellGrid::new(20, 5);
        let line = make_line("漢字");
        let cols = grid.put_line(0, 0, &line, 20);
        assert_eq!(cols, 4); // 2 wide chars × 2
    }

    #[test]
    fn test_put_line_truncation() {
        let mut grid = CellGrid::new(5, 1);
        let line = make_line("hello world");
        let cols = grid.put_line(0, 0, &line, 5);
        assert_eq!(cols, 5);
    }

    #[test]
    fn test_diff_full_redraw() {
        let grid = CellGrid::new(3, 2);
        let diffs = grid.diff();
        // All non-continuation cells should be in the diff
        assert_eq!(diffs.len(), 6);
    }

    #[test]
    fn test_diff_after_swap() {
        let mut grid = CellGrid::new(3, 1);
        grid.swap(); // previous = current, current = blank
        // Now current and previous are the same (both blank)
        let diffs = grid.diff();
        assert_eq!(diffs.len(), 0);
    }

    #[test]
    fn test_diff_detects_change() {
        let mut grid = CellGrid::new(3, 1);
        grid.swap(); // previous = blank
        let face = default_face();
        grid.put_char(1, 0, "X", &face);
        let diffs = grid.diff();
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].x, 1);
        assert_eq!(diffs[0].cell.grapheme, "X");
    }

    #[test]
    fn test_clear() {
        let mut grid = CellGrid::new(3, 1);
        let face = Face {
            fg: Color::Named(NamedColor::Red),
            ..Face::default()
        };
        grid.put_char(0, 0, "A", &face);
        grid.clear(&Face::default());
        assert_eq!(grid.get(0, 0).unwrap().grapheme, " ");
    }

    #[test]
    fn test_resize() {
        let mut grid = CellGrid::new(10, 5);
        grid.resize(20, 10);
        assert_eq!(grid.width(), 20);
        assert_eq!(grid.height(), 10);
        assert_eq!(grid.current.len(), 200);
    }

    #[test]
    fn test_invalidate_all() {
        let mut grid = CellGrid::new(3, 1);
        grid.swap();
        assert!(!grid.previous.is_empty());
        grid.invalidate_all();
        assert!(grid.previous.is_empty());
        // After invalidation, diff should return all cells
        assert_eq!(grid.diff().len(), 3);
    }

    #[test]
    fn test_resolve_face_fg_bg() {
        let base = Face {
            fg: Color::Named(NamedColor::Red),
            bg: Color::Named(NamedColor::Blue),
            ..Face::default()
        };
        let atom = Face {
            fg: Color::Default,
            bg: Color::Named(NamedColor::Green),
            ..Face::default()
        };
        let resolved = resolve_face(&atom, &base);
        assert_eq!(resolved.fg, base.fg); // inherited
        assert_eq!(resolved.bg, atom.bg); // kept
    }

    #[test]
    fn test_resolve_face_underline() {
        let base = Face {
            underline: Color::Named(NamedColor::Red),
            ..Face::default()
        };
        // Default underline inherits from base
        let atom_default = Face::default();
        let resolved = resolve_face(&atom_default, &base);
        assert_eq!(resolved.underline, base.underline);

        // Explicit underline is kept
        let atom_explicit = Face {
            underline: Color::Named(NamedColor::Green),
            ..Face::default()
        };
        let resolved2 = resolve_face(&atom_explicit, &base);
        assert_eq!(resolved2.underline, atom_explicit.underline);
    }

    #[test]
    fn test_resolve_face_attributes_merge() {
        let base = Face {
            attributes: Attributes::BOLD,
            ..Face::default()
        };
        let atom = Face {
            attributes: Attributes::ITALIC,
            ..Face::default()
        };
        let resolved = resolve_face(&atom, &base);
        assert!(resolved.attributes.contains(Attributes::BOLD));
        assert!(resolved.attributes.contains(Attributes::ITALIC));
        assert_eq!(resolved.attributes, Attributes::BOLD | Attributes::ITALIC);
    }

    #[test]
    fn test_resolve_face_attributes_final() {
        let base = Face {
            attributes: Attributes::BOLD,
            ..Face::default()
        };
        let atom = Face {
            attributes: Attributes::ITALIC | Attributes::FINAL_ATTR,
            ..Face::default()
        };
        let resolved = resolve_face(&atom, &base);
        // FinalAttr means atom attributes replace base entirely
        assert!(!resolved.attributes.contains(Attributes::BOLD));
        assert!(resolved.attributes.contains(Attributes::ITALIC));
        assert!(resolved.attributes.contains(Attributes::FINAL_ATTR));
    }

    #[test]
    fn test_put_line_skips_control_chars() {
        let mut grid = CellGrid::new(20, 1);
        // Line with embedded newline and carriage return
        // \n renders as a space (1 cell), \r is skipped
        let line = vec![Atom::with_style(
            "ab\ncd\ref",
            Style::from_face(&default_face()),
        )];
        let cols = grid.put_line(0, 0, &line, 20);
        assert_eq!(cols, 7); // "ab" + space(\n) + "cd" + "ef"
        assert_eq!(grid.get(0, 0).unwrap().grapheme, "a");
        assert_eq!(grid.get(1, 0).unwrap().grapheme, "b");
        assert_eq!(grid.get(2, 0).unwrap().grapheme, " "); // \n → space
        assert_eq!(grid.get(3, 0).unwrap().grapheme, "c");
        assert_eq!(grid.get(4, 0).unwrap().grapheme, "d");
        assert_eq!(grid.get(5, 0).unwrap().grapheme, "e");
        assert_eq!(grid.get(6, 0).unwrap().grapheme, "f");
    }

    #[test]
    fn test_put_line_combining_character() {
        let mut grid = CellGrid::new(20, 1);
        // e + combining acute accent → single grapheme cluster "é"
        let line = make_line("e\u{0301}x");
        let cols = grid.put_line(0, 0, &line, 20);
        assert_eq!(cols, 2); // "é" (1 cell) + "x" (1 cell)
        assert_eq!(grid.get(0, 0).unwrap().grapheme, "e\u{0301}");
        assert_eq!(grid.get(0, 0).unwrap().width, 1);
        assert_eq!(grid.get(1, 0).unwrap().grapheme, "x");
    }

    #[test]
    fn test_put_line_cjk_with_variation_selector() {
        let mut grid = CellGrid::new(20, 1);
        // CJK character + variation selector 16 (VS16)
        let line = make_line("\u{4e16}\u{fe0f}a");
        let cols = grid.put_line(0, 0, &line, 20);
        // The grapheme "\u{4e16}\u{fe0f}" should be treated as one cluster
        assert_eq!(grid.get(0, 0).unwrap().grapheme, "\u{4e16}\u{fe0f}");
        assert!(cols >= 2); // wide char + 'a'
    }

    #[test]
    fn test_put_line_newline_renders_as_space() {
        let mut grid = CellGrid::new(20, 1);
        let face = Face {
            attributes: Attributes::STRIKETHROUGH,
            ..Face::default()
        };
        let line = vec![Atom::with_style("};\n", Style::from_face(&face))];
        let cols = grid.put_line(0, 0, &line, 20);
        assert_eq!(cols, 3); // "}" + ";" + space(\n)
        assert_eq!(grid.get(0, 0).unwrap().grapheme, "}");
        assert_eq!(grid.get(1, 0).unwrap().grapheme, ";");
        assert_eq!(grid.get(2, 0).unwrap().grapheme, " ");
        // The space from \n carries the atom's strikethrough attribute
        assert!(
            grid.get(2, 0)
                .unwrap()
                .face()
                .attributes
                .contains(Attributes::STRIKETHROUGH)
        );
    }

    #[test]
    fn test_swap_with_dirty_preserves_clean_rows() {
        let mut grid = CellGrid::new(3, 3);
        let face = default_face();
        // Paint all rows
        grid.put_char(0, 0, "A", &face);
        grid.put_char(0, 1, "B", &face);
        grid.put_char(0, 2, "C", &face);
        // First frame: full swap to populate previous
        grid.swap();

        // Second frame: only modify row 1
        grid.put_char(0, 0, "A", &face); // same
        grid.put_char(0, 1, "X", &face); // changed
        grid.put_char(0, 2, "C", &face); // same
        // Mark only row 1 dirty (rows 0 and 2 are clean)
        grid.dirty_rows[0] = false;
        grid.dirty_rows[1] = true;
        grid.dirty_rows[2] = false;

        grid.swap_with_dirty();

        // After swap_with_dirty: current retains all content
        assert_eq!(grid.current[0].grapheme, "A");
        assert_eq!(grid.current[3].grapheme, "X"); // row 1, col 0
        assert_eq!(grid.current[6].grapheme, "C");
        // previous was updated only for dirty row 1
        assert_eq!(grid.previous[3].grapheme, "X");
    }

    #[test]
    fn test_swap_with_dirty_first_frame_fallback() {
        let mut grid = CellGrid::new(3, 2);
        let face = default_face();
        grid.put_char(0, 0, "A", &face);
        // previous is empty → swap_with_dirty falls back to swap()
        assert!(grid.previous.is_empty());
        grid.swap_with_dirty();
        // After fallback swap: previous is populated, current is reset
        assert!(!grid.previous.is_empty());
        assert_eq!(grid.previous[0].grapheme, "A");
    }

    #[test]
    fn test_diff_into_matches_diff() {
        // Full redraw case
        let mut grid = CellGrid::new(3, 2);
        let face = default_face();
        grid.put_char(0, 0, "A", &face);
        grid.put_char(1, 0, "B", &face);

        let diffs = grid.diff();
        let mut buf = Vec::new();
        grid.diff_into(&mut buf);
        assert_eq!(diffs.len(), buf.len());
        for (d, b) in diffs.iter().zip(buf.iter()) {
            assert_eq!(d.x, b.x);
            assert_eq!(d.y, b.y);
            assert_eq!(d.cell, b.cell);
        }

        // Incremental case
        grid.swap();
        grid.put_char(1, 0, "X", &face);
        let diffs = grid.diff();
        grid.diff_into(&mut buf);
        assert_eq!(diffs.len(), buf.len());
        for (d, b) in diffs.iter().zip(buf.iter()) {
            assert_eq!(d.x, b.x);
            assert_eq!(d.y, b.y);
            assert_eq!(d.cell, b.cell);
        }

        // Empty diff case: swap then reproduce same content
        grid.swap();
        grid.put_char(0, 0, "A", &face);
        grid.put_char(1, 0, "X", &face);
        let diffs = grid.diff();
        grid.diff_into(&mut buf);
        assert_eq!(diffs.len(), buf.len(), "empty diff: lengths should match");
        for (d, b) in diffs.iter().zip(buf.iter()) {
            assert_eq!(d.x, b.x);
            assert_eq!(d.y, b.y);
            assert_eq!(d.cell, b.cell);
        }
    }

    #[test]
    fn test_diff_into_reuses_capacity() {
        let mut grid = CellGrid::new(10, 5);
        let face = default_face();
        grid.put_char(0, 0, "A", &face);

        let mut buf = Vec::new();
        grid.diff_into(&mut buf);
        let cap_after_first = buf.capacity();
        assert!(cap_after_first > 0);

        // Second call with same-size result shouldn't grow
        grid.diff_into(&mut buf);
        assert_eq!(buf.capacity(), cap_after_first);
    }

    #[test]
    fn test_iter_diffs_matches_diff() {
        // Full redraw
        let mut grid = CellGrid::new(3, 2);
        let face = default_face();
        grid.put_char(0, 0, "A", &face);
        grid.put_char(2, 1, "Z", &face);

        let diffs = grid.diff();
        let iter_results: Vec<_> = grid.iter_diffs().collect();
        assert_eq!(diffs.len(), iter_results.len());
        for (d, (x, y, cell)) in diffs.iter().zip(iter_results.iter()) {
            assert_eq!(d.x, *x);
            assert_eq!(d.y, *y);
            assert_eq!(&d.cell, *cell);
        }

        // Incremental
        grid.swap();
        grid.put_char(1, 0, "X", &face);
        let diffs = grid.diff();
        let iter_results: Vec<_> = grid.iter_diffs().collect();
        assert_eq!(diffs.len(), iter_results.len());
        for (d, (x, y, cell)) in diffs.iter().zip(iter_results.iter()) {
            assert_eq!(d.x, *x);
            assert_eq!(d.y, *y);
            assert_eq!(&d.cell, *cell);
        }
    }

    #[test]
    fn test_is_first_frame() {
        let mut grid = CellGrid::new(3, 2);
        assert!(grid.is_first_frame());
        grid.swap();
        assert!(!grid.is_first_frame());
        grid.invalidate_all();
        assert!(grid.is_first_frame());
    }

    #[test]
    fn test_swap_with_dirty_dirty_rows_reset() {
        let mut grid = CellGrid::new(3, 3);
        let face = default_face();
        grid.put_char(0, 0, "A", &face);
        grid.swap(); // populate previous

        grid.put_char(0, 1, "B", &face);
        assert!(grid.dirty_rows[1]);
        grid.swap_with_dirty();
        // All dirty_rows should be false after swap_with_dirty
        assert!(grid.dirty_rows.iter().all(|d| !d));
    }
}
