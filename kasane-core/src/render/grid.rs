use compact_str::CompactString;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::protocol::{Atom, Attributes, Color, Face};

// ---------------------------------------------------------------------------
// Cell + CellGrid
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub grapheme: CompactString,
    pub face: Face,
    /// Display width: 1 for normal, 2 for wide chars, 0 for continuation cells.
    pub width: u8,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            grapheme: CompactString::const_new(" "),
            face: Face::default(),
            width: 1,
        }
    }
}

pub struct CellGrid {
    width: u16,
    height: u16,
    current: Vec<Cell>,
    previous: Vec<Cell>,
    dirty_rows: Vec<bool>,
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
        }
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

        self.current[idx] = Cell {
            grapheme: CompactString::from(grapheme),
            face: *face,
            width: w,
        };
        // If wide character, mark next cell as continuation
        if w == 2 && x + 1 < self.width {
            let next_idx = self.idx(x + 1, y);
            self.current[next_idx] = Cell {
                grapheme: CompactString::default(),
                face: *face,
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
            let face = match base_face {
                Some(base) => resolve_face(&atom.face, base),
                None => atom.face,
            };
            for grapheme in atom.contents.graphemes(true) {
                if grapheme.is_empty() {
                    continue;
                }
                // Skip control characters: UnicodeWidthStr::width() returns 1
                // for \n, \r, etc. in unicode-width 0.2.x, but they must never
                // be placed in the grid (printing them would corrupt the terminal).
                if grapheme == "\n" {
                    if x + 1 > limit {
                        break;
                    }
                    self.put_char(x, y, " ", &face);
                    x += 1;
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
        for cell in &mut self.current {
            cell.grapheme = CompactString::const_new(" ");
            cell.face = *face;
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
        self.dirty_rows[y as usize] = true;
        for x in 0..self.width {
            let idx = self.idx(x, y);
            self.current[idx] = Cell {
                grapheme: CompactString::const_new(" "),
                face: *face,
                width: 1,
            };
        }
    }

    pub fn diff(&self) -> Vec<CellDiff> {
        crate::perf::perf_span!("grid_diff");
        if self.previous.is_empty() {
            // Full redraw
            return self
                .current
                .iter()
                .enumerate()
                .filter(|(_, c)| c.width > 0) // skip continuation cells
                .map(|(i, cell)| {
                    let x = (i % self.width as usize) as u16;
                    let y = (i / self.width as usize) as u16;
                    CellDiff {
                        x,
                        y,
                        cell: cell.clone(),
                    }
                })
                .collect();
        }

        let mut diffs = Vec::new();
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
                    diffs.push(CellDiff {
                        x: (i % w) as u16,
                        y: row as u16,
                        cell: curr.clone(),
                    });
                }
            }
        }
        diffs
    }

    pub fn swap(&mut self) {
        crate::perf::perf_span!("grid_swap");
        std::mem::swap(&mut self.previous, &mut self.current);
        let size = self.width as usize * self.height as usize;
        if self.current.len() == size {
            for cell in &mut self.current {
                cell.grapheme = CompactString::const_new(" ");
                cell.face = Face::default();
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

    pub fn invalidate_all(&mut self) {
        self.previous.clear();
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
// Face resolution
// ---------------------------------------------------------------------------

/// Resolve Default colors in an atom face against a base face.
/// In Kakoune, `default` means "inherit from the containing context".
pub(crate) fn resolve_face(atom_face: &Face, base: &Face) -> Face {
    Face {
        fg: if atom_face.fg == Color::Default {
            base.fg
        } else {
            atom_face.fg
        },
        bg: if atom_face.bg == Color::Default {
            base.bg
        } else {
            atom_face.bg
        },
        underline: if atom_face.underline == Color::Default {
            base.underline
        } else {
            atom_face.underline
        },
        attributes: if atom_face.attributes.contains(Attributes::FINAL_ATTR)
            || base.attributes.is_empty()
        {
            atom_face.attributes
        } else {
            base.attributes | atom_face.attributes
        },
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Atom, Attributes, Color, Face, NamedColor};
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
        let line = vec![Atom {
            face: default_face(),
            contents: "ab\ncd\ref".into(),
        }];
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
        let line = vec![Atom {
            face,
            contents: "};\n".into(),
        }];
        let cols = grid.put_line(0, 0, &line, 20);
        assert_eq!(cols, 3); // "}" + ";" + space(\n)
        assert_eq!(grid.get(0, 0).unwrap().grapheme, "}");
        assert_eq!(grid.get(1, 0).unwrap().grapheme, ";");
        assert_eq!(grid.get(2, 0).unwrap().grapheme, " ");
        // The space from \n carries the atom's strikethrough attribute
        assert!(
            grid.get(2, 0)
                .unwrap()
                .face
                .attributes
                .contains(Attributes::STRIKETHROUGH)
        );
    }
}
