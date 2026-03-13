//! Compiled paint patches — direct cell/command updates that bypass the
//! full Element tree → layout → paint pipeline.
//!
//! This is the "compiled path" from ADR-010 Stage 4, analogous to Svelte
//! generating `element.textContent = count` instead of full vDOM diffing.

use crate::layout::Rect;
use crate::render::LayoutCache;
use crate::render::grid::CellGrid;
use crate::render::scene::{CellSize, DrawCommand};
use crate::state::{AppState, DirtyFlags};

/// A paint patch that can directly update a CellGrid or DrawCommand list
/// without going through the full rendering pipeline.
pub trait PaintPatch {
    /// The DirtyFlags this patch handles.
    fn deps(&self) -> DirtyFlags;

    /// Check if this patch can handle the current dirty state.
    /// Returns false if plugins are active on the affected section,
    /// if the dirty flags don't match exactly, or if preconditions aren't met.
    fn can_apply(&self, dirty: DirtyFlags, state: &AppState) -> bool;

    /// Apply the patch directly to a CellGrid (TUI path).
    fn apply_grid(&self, grid: &mut CellGrid, state: &AppState, layout_cache: &LayoutCache);

    /// Apply the patch as DrawCommands (GPU path).
    fn apply_scene(&self, out: &mut Vec<DrawCommand>, state: &AppState, cell_size: CellSize);
}

// ---------------------------------------------------------------------------
// StatusBarPatch — when only STATUS is dirty
// ---------------------------------------------------------------------------

/// When dirty == STATUS, directly repaint the status bar row.
/// Writes ~2 lines worth of cells (status_line + mode_line) instead of
/// clearing and repainting the entire 1,920-cell grid.
pub struct StatusBarPatch;

impl PaintPatch for StatusBarPatch {
    fn deps(&self) -> DirtyFlags {
        DirtyFlags::STATUS
    }

    fn can_apply(&self, dirty: DirtyFlags, _state: &AppState) -> bool {
        dirty == DirtyFlags::STATUS
    }

    fn apply_grid(&self, grid: &mut CellGrid, state: &AppState, layout_cache: &LayoutCache) {
        let status_y = layout_cache.status_row.unwrap_or_else(|| {
            if state.status_at_top {
                0
            } else {
                state.rows.saturating_sub(1)
            }
        });

        if status_y >= grid.height() {
            return;
        }

        // Clear the status row with the status face
        let status_rect = Rect {
            x: 0,
            y: status_y,
            w: grid.width(),
            h: 1,
        };
        grid.clear_region(&status_rect, &state.status_default_face);

        // Paint status_line from the left
        let status_cols = grid.put_line_with_base(
            status_y,
            0,
            &state.status_line,
            grid.width(),
            Some(&state.status_default_face),
        );

        // Paint mode_line from the right
        let mode_width: u16 = crate::layout::line_display_width(&state.status_mode_line) as u16;
        if mode_width > 0 {
            let mode_x = grid.width().saturating_sub(mode_width);
            grid.put_line_with_base(
                status_y,
                mode_x,
                &state.status_mode_line,
                mode_width,
                Some(&state.status_default_face),
            );
        }

        // Paint cursor count badge if > 1
        if state.cursor_count > 1 {
            let badge_text = format!(" {} sel ", state.cursor_count);
            let badge_width = badge_text.len() as u16;
            let badge_x = grid
                .width()
                .saturating_sub(mode_width)
                .saturating_sub(badge_width);
            if badge_x > status_cols {
                for (i, ch) in badge_text.chars().enumerate() {
                    let mut s = String::new();
                    s.push(ch);
                    grid.put_char(badge_x + i as u16, status_y, &s, &state.status_default_face);
                }
            }
        }
    }

    fn apply_scene(&self, out: &mut Vec<DrawCommand>, state: &AppState, cell_size: CellSize) {
        use crate::protocol::resolve_face;
        use crate::render::scene::{PixelPos, PixelRect, ResolvedAtom};

        let status_y = if state.status_at_top {
            0
        } else {
            state.rows.saturating_sub(1)
        };

        let py = status_y as f32 * cell_size.height;
        let row_w = state.cols as f32 * cell_size.width;

        // Fill status bar background
        out.push(DrawCommand::FillRect {
            rect: PixelRect {
                x: 0.0,
                y: py,
                w: row_w,
                h: cell_size.height,
            },
            face: state.status_default_face,
        });

        // Draw status line atoms
        let resolved: Vec<ResolvedAtom> = state
            .status_line
            .iter()
            .map(|atom| ResolvedAtom {
                contents: atom.contents.clone().into(),
                face: resolve_face(&atom.face, &state.status_default_face),
            })
            .collect();
        if !resolved.is_empty() {
            out.push(DrawCommand::DrawAtoms {
                pos: PixelPos { x: 0.0, y: py },
                atoms: resolved,
                max_width: row_w,
            });
        }

        // Draw mode line from the right
        let mode_width = crate::layout::line_display_width(&state.status_mode_line) as f32;
        if mode_width > 0.0 {
            let mode_x = row_w - mode_width * cell_size.width;
            let resolved_mode: Vec<ResolvedAtom> = state
                .status_mode_line
                .iter()
                .map(|atom| ResolvedAtom {
                    contents: atom.contents.clone().into(),
                    face: resolve_face(&atom.face, &state.status_default_face),
                })
                .collect();
            out.push(DrawCommand::DrawAtoms {
                pos: PixelPos { x: mode_x, y: py },
                atoms: resolved_mode,
                max_width: mode_width * cell_size.width,
            });
        }
    }
}

// ---------------------------------------------------------------------------
// MenuSelectionPatch — when only MENU_SELECTION is dirty
// ---------------------------------------------------------------------------

/// When dirty == MENU_SELECTION, swap faces on old/new selected items.
/// Writes ~10 cells instead of full repaint.
pub struct MenuSelectionPatch {
    /// Previous selected index (tracked externally).
    pub prev_selected: Option<usize>,
}

impl PaintPatch for MenuSelectionPatch {
    fn deps(&self) -> DirtyFlags {
        DirtyFlags::MENU_SELECTION
    }

    fn can_apply(&self, dirty: DirtyFlags, state: &AppState) -> bool {
        dirty == DirtyFlags::MENU_SELECTION
            && state
                .menu
                .as_ref()
                .is_some_and(|m| m.columns_split.is_none())
    }

    fn apply_grid(&self, grid: &mut CellGrid, state: &AppState, _layout_cache: &LayoutCache) {
        let menu = match state.menu.as_ref() {
            Some(m) => m,
            None => return,
        };

        let menu_rect = match crate::layout::get_menu_rect(state) {
            Some(r) => r,
            None => return,
        };

        let visible_start = menu.first_item;
        let visible_count = menu.win_height as usize;

        // Repaint old selected item with normal face
        if let Some(old_sel) = self.prev_selected
            && old_sel >= visible_start
            && old_sel < visible_start + visible_count
        {
            let row_offset = (old_sel - visible_start) as u16;
            repaint_menu_item_row(
                grid,
                menu_rect,
                row_offset,
                &menu.items[old_sel],
                &menu.menu_face,
            );
        }

        // Repaint new selected item with selected face
        if let Some(new_sel) = menu.selected
            && new_sel >= visible_start
            && new_sel < visible_start + visible_count
        {
            let row_offset = (new_sel - visible_start) as u16;
            repaint_menu_item_row(
                grid,
                menu_rect,
                row_offset,
                &menu.items[new_sel],
                &menu.selected_item_face,
            );
        }
    }

    fn apply_scene(&self, _out: &mut Vec<DrawCommand>, _state: &AppState, _cell_size: CellSize) {
        // For GPU path, fall back to full section repaint.
        // The scene cache already handles MENU_SELECTION efficiently.
    }
}

/// Repaint a single menu item row in the grid with the given face.
fn repaint_menu_item_row(
    grid: &mut CellGrid,
    menu_rect: Rect,
    row_offset: u16,
    item: &[crate::protocol::Atom],
    face: &crate::protocol::Face,
) {
    // Menu items have 1 cell border on each side for inline/prompt menus
    let content_x = menu_rect.x + 1;
    let content_w = menu_rect.w.saturating_sub(2); // exclude border columns
    let y = menu_rect.y + 1 + row_offset; // +1 for top border

    if y >= menu_rect.y + menu_rect.h.saturating_sub(1) || y >= grid.height() {
        return;
    }

    // Clear the row with the item face
    let row_rect = Rect {
        x: content_x,
        y,
        w: content_w,
        h: 1,
    };
    grid.clear_region(&row_rect, face);

    // Paint the item atoms with the new face
    grid.put_line_with_base(y, content_x, item, content_w, Some(face));
}

// ---------------------------------------------------------------------------
// CursorPatch — when dirty is empty but cursor_pos changed
// ---------------------------------------------------------------------------

/// When dirty flags are empty but cursor position changed,
/// swap face at old/new cursor positions. 2 cells.
pub struct CursorPatch {
    /// Previous cursor position.
    pub prev_cursor_x: u16,
    pub prev_cursor_y: u16,
}

impl PaintPatch for CursorPatch {
    fn deps(&self) -> DirtyFlags {
        DirtyFlags::empty()
    }

    fn can_apply(&self, dirty: DirtyFlags, state: &AppState) -> bool {
        if !dirty.is_empty() {
            return false;
        }
        let new_x = state.cursor_pos.column as u16;
        let new_y = state.cursor_pos.line as u16;
        // Only apply if cursor actually moved
        new_x != self.prev_cursor_x || new_y != self.prev_cursor_y
    }

    fn apply_grid(&self, grid: &mut CellGrid, state: &AppState, _layout_cache: &LayoutCache) {
        // Restore old cursor cell: if a secondary cursor sits there, use its face;
        // otherwise restore to default.
        if let Some(cell) = grid.get_mut(self.prev_cursor_x, self.prev_cursor_y) {
            let is_secondary = state.secondary_cursors.iter().any(|c| {
                c.column as u16 == self.prev_cursor_x && c.line as u16 == self.prev_cursor_y
            });
            if is_secondary {
                cell.face = crate::render::cursor::make_secondary_cursor_face(
                    &cell.face,
                    &state.default_face,
                    state.secondary_blend_ratio,
                );
            } else {
                cell.face = state.default_face;
            }
        }

        // The new cursor face will be applied by the caller (show_cursor / clear_block_cursor_face)
        // We just need to mark the new cursor row as dirty.
        let new_y = state.cursor_pos.line as u16;
        if new_y < grid.height() {
            let rect = Rect {
                x: 0,
                y: new_y,
                w: 1,
                h: 1,
            };
            grid.mark_region_dirty(&rect);
        }
    }

    fn apply_scene(&self, _out: &mut Vec<DrawCommand>, _state: &AppState, _cell_size: CellSize) {
        // GPU path handles cursor animation separately.
    }
}

// ---------------------------------------------------------------------------
// Patch registry
// ---------------------------------------------------------------------------

/// Try to apply a paint patch for the given dirty flags.
/// Returns true if a patch was applied, false to fall through to the full pipeline.
pub fn try_apply_grid_patch(
    patches: &[&dyn PaintPatch],
    grid: &mut CellGrid,
    state: &AppState,
    dirty: DirtyFlags,
    layout_cache: &LayoutCache,
) -> bool {
    for patch in patches {
        if patch.can_apply(dirty, state) {
            patch.apply_grid(grid, state, layout_cache);
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Atom, Color, Coord, Face, MenuStyle, NamedColor};
    use crate::test_utils::make_line;

    fn test_state() -> AppState {
        let mut state = crate::render::test_helpers::test_state_80x24();
        state.lines = vec![make_line("hello"), make_line("world")];
        state.status_line = vec![Atom {
            face: Face::default(),
            contents: " NORMAL ".into(),
        }];
        state.status_mode_line = vec![Atom {
            face: Face::default(),
            contents: "normal".into(),
        }];
        state
    }

    #[test]
    fn test_status_bar_patch_can_apply() {
        let patch = StatusBarPatch;
        let state = test_state();

        assert!(patch.can_apply(DirtyFlags::STATUS, &state));
        assert!(!patch.can_apply(DirtyFlags::BUFFER, &state));
        assert!(!patch.can_apply(DirtyFlags::STATUS | DirtyFlags::BUFFER, &state));
        assert!(!patch.can_apply(DirtyFlags::ALL, &state));
    }

    #[test]
    fn test_status_bar_patch_apply_grid() {
        let patch = StatusBarPatch;
        let state = test_state();
        let layout_cache = LayoutCache {
            base_layout: None,
            status_row: Some(23),
            root_area: None,
        };
        let mut grid = CellGrid::new(state.cols, state.rows);

        // Fill the grid with initial content
        grid.clear(&state.default_face);
        grid.swap();

        // Apply the patch
        patch.apply_grid(&mut grid, &state, &layout_cache);

        // Status bar row should have been updated
        let cell = grid.get(1, 23).unwrap();
        assert_eq!(cell.grapheme, "N");
        assert_eq!(cell.face.fg, Color::Named(NamedColor::Cyan));

        // Mode line should be at the right
        let mode_x = state.cols - 6; // "normal" = 6 chars
        let cell_mode = grid.get(mode_x, 23).unwrap();
        assert_eq!(cell_mode.grapheme, "n");
    }

    #[test]
    fn test_menu_selection_patch_can_apply() {
        let mut state = test_state();
        let patch = MenuSelectionPatch {
            prev_selected: Some(0),
        };

        // No menu → cannot apply
        assert!(!patch.can_apply(DirtyFlags::MENU_SELECTION, &state));

        // With menu → can apply
        state.apply(crate::protocol::KakouneRequest::MenuShow {
            items: vec![make_line("item1"), make_line("item2")],
            anchor: Coord { line: 1, column: 0 },
            selected_item_face: Face::default(),
            menu_face: Face::default(),
            style: MenuStyle::Inline,
        });
        assert!(patch.can_apply(DirtyFlags::MENU_SELECTION, &state));
        assert!(!patch.can_apply(DirtyFlags::STATUS, &state));
    }

    #[test]
    fn test_cursor_patch_can_apply() {
        let state = test_state();
        let patch = CursorPatch {
            prev_cursor_x: 5,
            prev_cursor_y: 3,
        };

        // Dirty flags must be empty and cursor must have moved
        assert!(patch.can_apply(DirtyFlags::empty(), &state));

        // Same cursor position → cannot apply
        let patch_same = CursorPatch {
            prev_cursor_x: state.cursor_pos.column as u16,
            prev_cursor_y: state.cursor_pos.line as u16,
        };
        assert!(!patch_same.can_apply(DirtyFlags::empty(), &state));

        // Non-empty dirty flags → cannot apply
        assert!(!patch.can_apply(DirtyFlags::BUFFER, &state));
    }

    #[test]
    fn test_try_apply_grid_patch() {
        let state = test_state();
        let layout_cache = LayoutCache::new();
        let mut grid = CellGrid::new(state.cols, state.rows);

        let status_patch = StatusBarPatch;
        let patches: Vec<&dyn PaintPatch> = vec![&status_patch];

        // STATUS dirty → patch applied
        assert!(try_apply_grid_patch(
            &patches,
            &mut grid,
            &state,
            DirtyFlags::STATUS,
            &layout_cache,
        ));

        // BUFFER dirty → no patch
        assert!(!try_apply_grid_patch(
            &patches,
            &mut grid,
            &state,
            DirtyFlags::BUFFER,
            &layout_cache,
        ));
    }
}
