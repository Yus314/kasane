use unicode_width::UnicodeWidthStr;

use crate::protocol::{Attribute, Color, Face, Line};

// ---------------------------------------------------------------------------
// Cell + CellGrid
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub grapheme: String,
    pub face: Face,
    /// Display width: 1 for normal, 2 for wide chars, 0 for continuation cells.
    pub width: u8,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            grapheme: " ".to_string(),
            face: Face::default(),
            width: 1,
        }
    }
}

pub struct CellGrid {
    pub width: u16,
    pub height: u16,
    current: Vec<Cell>,
    previous: Vec<Cell>,
}

impl CellGrid {
    pub fn new(width: u16, height: u16) -> Self {
        let size = width as usize * height as usize;
        CellGrid {
            width,
            height,
            current: vec![Cell::default(); size],
            previous: Vec::new(), // empty means "invalidated — full redraw needed"
        }
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        let size = width as usize * height as usize;
        self.current = vec![Cell::default(); size];
        self.previous = Vec::new();
    }

    fn idx(&self, x: u16, y: u16) -> usize {
        y as usize * self.width as usize + x as usize
    }

    pub fn put_char(&mut self, x: u16, y: u16, grapheme: &str, face: &Face) {
        if x >= self.width || y >= self.height {
            return;
        }
        let w = UnicodeWidthStr::width(grapheme) as u8;
        let idx = self.idx(x, y);

        // --- Clean up orphaned wide-character halves before overwriting ---

        // If overwriting a continuation cell (width 0), the wide char at x-1 is orphaned.
        if self.current[idx].width == 0 && x > 0 {
            let prev_idx = self.idx(x - 1, y);
            self.current[prev_idx].grapheme = " ".to_string();
            self.current[prev_idx].width = 1;
        }

        // If overwriting a wide char (width 2), its continuation at x+1 is orphaned.
        if self.current[idx].width == 2 && x + 1 < self.width {
            let next_idx = self.idx(x + 1, y);
            self.current[next_idx].grapheme = " ".to_string();
            self.current[next_idx].width = 1;
        }

        // If placing a wide char, x+1 will become our continuation.
        // If x+1 is currently a wide char, its continuation at x+2 is orphaned.
        if w == 2 && x + 1 < self.width {
            let next_idx = self.idx(x + 1, y);
            if self.current[next_idx].width == 2 && x + 2 < self.width {
                let next2_idx = self.idx(x + 2, y);
                self.current[next2_idx].grapheme = " ".to_string();
                self.current[next2_idx].width = 1;
            }
        }

        // --- Write the new cell ---

        self.current[idx] = Cell {
            grapheme: grapheme.to_string(),
            face: face.clone(),
            width: w,
        };
        // If wide character, mark next cell as continuation
        if w == 2 && x + 1 < self.width {
            let next_idx = self.idx(x + 1, y);
            self.current[next_idx] = Cell {
                grapheme: String::new(),
                face: face.clone(),
                width: 0,
            };
        }
    }

    /// Write a protocol Line into the grid at row `y` starting at column `x_start`.
    /// Returns the number of columns consumed.
    pub fn put_line(&mut self, y: u16, x_start: u16, line: &Line, max_width: u16) -> u16 {
        self.put_line_with_base(y, x_start, line, max_width, None)
    }

    /// Write a protocol Line, resolving `Color::Default` against `base_face`.
    /// When base_face is Some, atom Default fg/bg inherit from it (Kakoune semantics).
    pub fn put_line_with_base(
        &mut self,
        y: u16,
        x_start: u16,
        line: &Line,
        max_width: u16,
        base_face: Option<&Face>,
    ) -> u16 {
        let mut x = x_start;
        let limit = x_start.saturating_add(max_width).min(self.width);

        for atom in line {
            let face = match base_face {
                Some(base) => resolve_face(&atom.face, base),
                None => atom.face.clone(),
            };
            for grapheme in atom.contents.split_inclusive(|_: char| true) {
                if grapheme.is_empty() {
                    continue;
                }
                // Skip control characters: UnicodeWidthStr::width() returns 1
                // for \n, \r, etc. in unicode-width 0.2.x, but they must never
                // be placed in the grid (printing them would corrupt the terminal).
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
            cell.grapheme = " ".to_string();
            cell.face = face.clone();
            cell.width = 1;
        }
    }

    pub fn fill_row(&mut self, y: u16, face: &Face) {
        if y >= self.height {
            return;
        }
        for x in 0..self.width {
            let idx = self.idx(x, y);
            self.current[idx] = Cell {
                grapheme: " ".to_string(),
                face: face.clone(),
                width: 1,
            };
        }
    }

    pub fn diff(&self) -> Vec<CellDiff> {
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
        for (i, (curr, prev)) in self.current.iter().zip(self.previous.iter()).enumerate() {
            if curr != prev && curr.width > 0 {
                let x = (i % self.width as usize) as u16;
                let y = (i / self.width as usize) as u16;
                diffs.push(CellDiff {
                    x,
                    y,
                    cell: curr.clone(),
                });
            }
        }
        diffs
    }

    pub fn swap(&mut self) {
        std::mem::swap(&mut self.previous, &mut self.current);
        // Reset current to blank
        let size = self.width as usize * self.height as usize;
        self.current.clear();
        self.current.resize(size, Cell::default());
    }

    pub fn invalidate_all(&mut self) {
        self.previous.clear();
    }

    /// Direct access to a cell in the current buffer.
    pub fn get(&self, x: u16, y: u16) -> Option<&Cell> {
        if x < self.width && y < self.height {
            Some(&self.current[self.idx(x, y)])
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
pub(super) fn resolve_face(atom_face: &Face, base: &Face) -> Face {
    let has_final_attr = atom_face.attributes.contains(&Attribute::FinalAttr);
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
        attributes: if has_final_attr || base.attributes.is_empty() {
            atom_face.attributes.clone()
        } else {
            let mut attrs = base.attributes.clone();
            for attr in &atom_face.attributes {
                if !attrs.contains(attr) {
                    attrs.push(*attr);
                }
            }
            attrs
        },
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Atom, Attribute, Color, Face, NamedColor};

    fn default_face() -> Face {
        Face::default()
    }

    fn make_line(text: &str) -> Line {
        vec![Atom {
            face: default_face(),
            contents: text.to_string(),
        }]
    }

    #[test]
    fn test_grid_new() {
        let grid = CellGrid::new(10, 5);
        assert_eq!(grid.width, 10);
        assert_eq!(grid.height, 5);
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
        assert_eq!(grid.width, 20);
        assert_eq!(grid.height, 10);
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
            attributes: vec![Attribute::Bold],
            ..Face::default()
        };
        let atom = Face {
            attributes: vec![Attribute::Italic],
            ..Face::default()
        };
        let resolved = resolve_face(&atom, &base);
        assert!(resolved.attributes.contains(&Attribute::Bold));
        assert!(resolved.attributes.contains(&Attribute::Italic));
        assert_eq!(resolved.attributes.len(), 2);
    }

    #[test]
    fn test_resolve_face_attributes_final() {
        let base = Face {
            attributes: vec![Attribute::Bold],
            ..Face::default()
        };
        let atom = Face {
            attributes: vec![Attribute::Italic, Attribute::FinalAttr],
            ..Face::default()
        };
        let resolved = resolve_face(&atom, &base);
        // FinalAttr means atom attributes replace base entirely
        assert!(!resolved.attributes.contains(&Attribute::Bold));
        assert!(resolved.attributes.contains(&Attribute::Italic));
        assert!(resolved.attributes.contains(&Attribute::FinalAttr));
    }


    #[test]
    fn test_put_line_skips_control_chars() {
        let mut grid = CellGrid::new(20, 1);
        // Line with embedded newline and carriage return
        let line = vec![Atom {
            face: default_face(),
            contents: "ab\ncd\ref".to_string(),
        }];
        let cols = grid.put_line(0, 0, &line, 20);
        assert_eq!(cols, 6); // "ab" + "cd" + "ef", control chars skipped
        assert_eq!(grid.get(0, 0).unwrap().grapheme, "a");
        assert_eq!(grid.get(1, 0).unwrap().grapheme, "b");
        assert_eq!(grid.get(2, 0).unwrap().grapheme, "c");
        assert_eq!(grid.get(3, 0).unwrap().grapheme, "d");
        assert_eq!(grid.get(4, 0).unwrap().grapheme, "e");
        assert_eq!(grid.get(5, 0).unwrap().grapheme, "f");
    }
}
