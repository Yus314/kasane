//! Unified paint tree traversal via the Visitor pattern.
//!
//! `walk_paint<V: PaintVisitor>()` handles Element tree traversal,
//! monomorphized for zero-cost dispatch. Two visitor implementations:
//! - `GridPaintVisitor` — TUI: writes to CellGrid
//! - `ScenePaintVisitor` — GPU: emits DrawCommands

use std::ops::Range;

use super::CursorStyle;
use super::grid::CellGrid;
use super::paint::{paint_border, paint_border_title, paint_buffer_ref, paint_shadow, paint_text};
use super::scene::{
    CellSize, DrawCommand, PixelPos, PixelRect, clear_cursor_atom, dim_cursor_atom, resolve_atoms,
    to_pixel_rect,
};
use super::theme::Theme;
use crate::display::DisplayMap;
use crate::element::{BorderConfig, BufferRefState, Element};
use crate::layout::Rect;
use crate::layout::flex::LayoutResult;
use crate::protocol::{Atom, Face};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// PaintVisitor trait
// ---------------------------------------------------------------------------

/// Data passed to `visit_container_pre` for rendering container chrome
/// (shadow, fill, border, title) before descending into the child element.
pub(crate) struct ContainerPaintInfo<'a> {
    pub area: Rect,
    /// Child content area (from layout), used by GPU for tight border placement.
    pub child_area: Option<Rect>,
    pub border: &'a Option<BorderConfig>,
    pub shadow: bool,
    /// Resolved container face.
    pub face: Face,
    /// Resolved border face (if border is present).
    pub border_face: Option<Face>,
    /// Optional border title atoms.
    pub title: Option<&'a [Atom]>,
}

/// Visitor trait for painting an Element tree. Implementations diverge only at
/// the rendering leaf/chrome points; structural traversal is handled by
/// `walk_paint`.
pub(crate) trait PaintVisitor {
    /// Render a Text element (plain string with resolved face).
    fn visit_text(&mut self, text: &str, face: &Face, area: Rect);

    /// Render a StyledLine element (Kakoune atom spans).
    fn visit_styled_line(&mut self, atoms: &[Atom], area: Rect);

    /// Render a BufferRef element (the main editor buffer area).
    fn visit_buffer_ref(
        &mut self,
        area: Rect,
        line_range: Range<usize>,
        state: &AppState,
        buffer_state: Option<&BufferRefState>,
        line_backgrounds: Option<&[Option<Face>]>,
        display_map: Option<&DisplayMap>,
    );

    /// Pre-visit for Container: render shadow, background fill, border, title.
    /// The walk function handles recursing into the child after this returns.
    fn visit_container_pre(&mut self, info: &ContainerPaintInfo);

    /// Pre-visit for Stack overlay: emit layer boundary marker (GPU only).
    fn visit_stack_overlay_pre(&mut self);

    /// Pre-visit for Scrollable: set up clip region (GPU only).
    fn visit_scrollable_pre(&mut self, area: Rect);

    /// Post-visit for Scrollable: tear down clip region (GPU only).
    fn visit_scrollable_post(&mut self);
}

// ---------------------------------------------------------------------------
// walk_paint: shared structural traversal
// ---------------------------------------------------------------------------

/// Walk an Element tree, dispatching to the visitor at each divergence point.
/// Structural recursion (Flex children, Grid children, etc.) is handled here.
pub(crate) fn walk_paint<V: PaintVisitor>(
    visitor: &mut V,
    element: &Element,
    layout: &LayoutResult,
    state: &AppState,
    theme: &Theme,
) {
    let area = layout.area;

    match element {
        Element::Text(text, style) => {
            let face = theme.resolve(style, &state.default_face);
            visitor.visit_text(text, &face, area);
        }
        Element::StyledLine(atoms) => {
            visitor.visit_styled_line(atoms, area);
        }
        Element::BufferRef {
            line_range,
            line_backgrounds,
            display_map,
            state: buffer_state,
        } => {
            let dm = display_map
                .as_ref()
                .map(|dm| dm.as_ref())
                .filter(|dm| !dm.is_identity());
            visitor.visit_buffer_ref(
                area,
                line_range.clone(),
                state,
                buffer_state.as_deref(),
                line_backgrounds.as_deref(),
                dm,
            );
        }
        Element::Empty => {}
        Element::SlotPlaceholder { .. } => {
            debug_assert!(false, "unresolved SlotPlaceholder reached walk_paint");
        }
        Element::Flex { children, .. } => {
            for (i, child) in children.iter().enumerate() {
                if let Some(child_layout) = layout.children.get(i) {
                    walk_paint(visitor, &child.element, child_layout, state, theme);
                }
            }
        }
        Element::ResolvedSlot { children, .. } => {
            for (i, child) in children.iter().enumerate() {
                if let Some(child_layout) = layout.children.get(i) {
                    walk_paint(visitor, &child.element, child_layout, state, theme);
                }
            }
        }
        Element::Grid { children, .. } => {
            for (i, child) in children.iter().enumerate() {
                if let Some(child_layout) = layout.children.get(i) {
                    walk_paint(visitor, child, child_layout, state, theme);
                }
            }
        }
        Element::Stack { base, overlays } => {
            if let Some(base_layout) = layout.children.first() {
                walk_paint(visitor, base, base_layout, state, theme);
            }
            for (i, overlay) in overlays.iter().enumerate() {
                if let Some(overlay_layout) = layout.children.get(i + 1) {
                    visitor.visit_stack_overlay_pre();
                    walk_paint(visitor, &overlay.element, overlay_layout, state, theme);
                }
            }
        }
        Element::Container {
            child,
            border,
            shadow,
            padding: _,
            style: el_style,
            title,
        } => {
            let face = theme.resolve(el_style, &state.default_face);
            let border_face = border.as_ref().map(|bc| {
                bc.face
                    .as_ref()
                    .map(|s| theme.resolve(s, &face))
                    .unwrap_or(face)
            });
            let child_area = layout.children.first().map(|cl| cl.area);
            let info = ContainerPaintInfo {
                area,
                child_area,
                border,
                shadow: *shadow,
                face,
                border_face,
                title: title.as_deref(),
            };
            visitor.visit_container_pre(&info);
            if let Some(child_layout) = layout.children.first() {
                walk_paint(visitor, child, child_layout, state, theme);
            }
        }
        Element::Interactive { child, .. } => {
            if let Some(child_layout) = layout.children.first() {
                walk_paint(visitor, child, child_layout, state, theme);
            }
        }
        Element::Scrollable {
            child,
            offset: _,
            direction: _,
        } => {
            visitor.visit_scrollable_pre(area);
            if let Some(child_layout) = layout.children.first() {
                walk_paint(visitor, child, child_layout, state, theme);
            }
            visitor.visit_scrollable_post();
        }
    }
}

// ---------------------------------------------------------------------------
// GridPaintVisitor — TUI backend
// ---------------------------------------------------------------------------

/// PaintVisitor that writes to a CellGrid (TUI rendering).
pub(crate) struct GridPaintVisitor<'a> {
    grid: &'a mut CellGrid,
}

impl<'a> GridPaintVisitor<'a> {
    pub fn new(grid: &'a mut CellGrid) -> Self {
        Self { grid }
    }
}

impl PaintVisitor for GridPaintVisitor<'_> {
    fn visit_text(&mut self, text: &str, face: &Face, area: Rect) {
        paint_text(self.grid, &area, text, face);
    }

    fn visit_styled_line(&mut self, atoms: &[Atom], area: Rect) {
        self.grid
            .put_line_with_base(area.y, area.x, atoms, area.w, None);
    }

    fn visit_buffer_ref(
        &mut self,
        area: Rect,
        line_range: Range<usize>,
        state: &AppState,
        buffer_state: Option<&BufferRefState>,
        line_backgrounds: Option<&[Option<Face>]>,
        display_map: Option<&DisplayMap>,
    ) {
        paint_buffer_ref(
            self.grid,
            &area,
            line_range,
            state,
            buffer_state,
            line_backgrounds,
            display_map,
        );
    }

    fn visit_container_pre(&mut self, info: &ContainerPaintInfo) {
        // Shadow (drawn first, behind the container)
        if info.shadow {
            paint_shadow(self.grid, &info.area);
        }

        // Fill entire container area with face
        self.grid.clear_region(&info.area, &info.face);

        // Border
        if let Some(border_config) = info.border {
            let border_face = info.border_face.unwrap_or(info.face);
            paint_border(
                self.grid,
                &info.area,
                &border_face,
                false,
                border_config.line_style.clone(),
            );
            // Title on top border
            if let Some(title_atoms) = info.title {
                paint_border_title(self.grid, &info.area, &border_face, title_atoms);
            }
        }
    }

    fn visit_stack_overlay_pre(&mut self) {
        // No-op for TUI: overlays just paint over the base content
    }

    fn visit_scrollable_pre(&mut self, _area: Rect) {
        // No-op for TUI: no pixel-level clipping in cell grid
    }

    fn visit_scrollable_post(&mut self) {
        // No-op for TUI
    }
}

// ---------------------------------------------------------------------------
// ScenePaintVisitor — GPU backend
// ---------------------------------------------------------------------------

/// PaintVisitor that emits `DrawCommand`s (GPU rendering).
pub(crate) struct ScenePaintVisitor<'a> {
    out: &'a mut Vec<DrawCommand>,
    cell_size: CellSize,
    cursor_style: CursorStyle,
}

impl<'a> ScenePaintVisitor<'a> {
    pub fn new(
        out: &'a mut Vec<DrawCommand>,
        cell_size: CellSize,
        cursor_style: CursorStyle,
    ) -> Self {
        Self {
            out,
            cell_size,
            cursor_style,
        }
    }
}

impl PaintVisitor for ScenePaintVisitor<'_> {
    fn visit_text(&mut self, text: &str, face: &Face, area: Rect) {
        let pr = to_pixel_rect(&area, self.cell_size);
        self.out.push(DrawCommand::DrawText {
            pos: PixelPos { x: pr.x, y: pr.y },
            text: text.to_string(),
            face: *face,
            max_width: pr.w,
        });
    }

    fn visit_styled_line(&mut self, atoms: &[Atom], area: Rect) {
        let pr = to_pixel_rect(&area, self.cell_size);
        let resolved = resolve_atoms(atoms, None);
        self.out.push(DrawCommand::DrawAtoms {
            pos: PixelPos { x: pr.x, y: pr.y },
            atoms: resolved,
            max_width: pr.w,
        });
    }

    fn visit_buffer_ref(
        &mut self,
        area: Rect,
        line_range: Range<usize>,
        state: &AppState,
        buffer_state: Option<&BufferRefState>,
        line_backgrounds: Option<&[Option<Face>]>,
        display_map: Option<&DisplayMap>,
    ) {
        let cs = self.cell_size;
        let lines = buffer_state.map(|s| &s.lines).unwrap_or(&state.lines);
        let default_face = buffer_state
            .map(|s| s.default_face)
            .unwrap_or(state.default_face);
        let padding_face = buffer_state
            .map(|s| s.padding_face)
            .unwrap_or(state.padding_face);
        let padding_char = buffer_state
            .map(|s| s.padding_char.as_str())
            .unwrap_or(&state.padding_char);

        for y_offset in 0..area.h {
            let display_line = line_range.start + y_offset as usize;
            let py = (area.y + y_offset) as f32 * cs.height;
            let px = area.x as f32 * cs.width;
            let row_w = area.w as f32 * cs.width;

            // Resolve display line → buffer line via DisplayMap
            let (buffer_line_idx, synthetic) = if let Some(dm) = display_map {
                if let Some(entry) = dm.entry(display_line) {
                    let buf_line = match &entry.source {
                        crate::display::SourceMapping::BufferLine(l) => Some(*l),
                        crate::display::SourceMapping::LineRange(r) => Some(r.start),
                        crate::display::SourceMapping::None => None,
                    };
                    (buf_line, entry.synthetic.as_ref())
                } else {
                    // Beyond display map range — render padding, not a buffer line
                    (None, None)
                }
            } else {
                (Some(display_line), None)
            };

            // Render synthetic content (fold summary, virtual text)
            if let Some(syn) = synthetic {
                self.out.push(DrawCommand::FillRect {
                    rect: PixelRect {
                        x: px,
                        y: py,
                        w: row_w,
                        h: cs.height,
                    },
                    face: syn.face,
                    elevated: false,
                });
                self.out.push(DrawCommand::DrawText {
                    pos: PixelPos { x: px, y: py },
                    text: syn.text.clone(),
                    face: syn.face,
                    max_width: row_w,
                });
                continue;
            }

            let line_idx = match buffer_line_idx {
                Some(idx) => idx,
                None => continue, // virtual text with no buffer source
            };

            if let Some(line) = lines.get(line_idx) {
                // Background fill for the row (with optional per-line override)
                let base_face = line_backgrounds
                    .and_then(|bgs| bgs.get(line_idx).copied().flatten())
                    .unwrap_or(default_face);
                self.out.push(DrawCommand::FillRect {
                    rect: PixelRect {
                        x: px,
                        y: py,
                        w: row_w,
                        h: cs.height,
                    },
                    face: base_face,
                    elevated: false,
                });
                // Atoms — clear PrimaryCursor face at the cursor cell in
                // non-block cursor modes so the thin bar/underline is visible.
                let mut resolved = resolve_atoms(line, Some(&base_face));
                if !matches!(self.cursor_style, CursorStyle::Block | CursorStyle::Outline)
                    && state.cursor_mode == crate::protocol::CursorMode::Buffer
                    && line_idx == state.cursor_pos.line as usize
                {
                    clear_cursor_atom(&mut resolved, state.cursor_pos.column as u16, &base_face);
                }
                // Differentiate secondary cursor faces
                for coord in &state.secondary_cursors {
                    if coord.line as usize == line_idx {
                        dim_cursor_atom(
                            &mut resolved,
                            coord.column as u16,
                            &base_face,
                            state.secondary_blend_ratio,
                        );
                    }
                }
                self.out.push(DrawCommand::DrawAtoms {
                    pos: PixelPos { x: px, y: py },
                    atoms: resolved,
                    max_width: row_w,
                });
            } else {
                // Padding row
                self.out.push(DrawCommand::FillRect {
                    rect: PixelRect {
                        x: px,
                        y: py,
                        w: row_w,
                        h: cs.height,
                    },
                    face: padding_face,
                    elevated: false,
                });
                let mut pad_face = padding_face;
                if pad_face.fg == pad_face.bg {
                    pad_face.fg = default_face.fg;
                }
                self.out.push(DrawCommand::DrawPaddingRow {
                    pos: PixelPos { x: px, y: py },
                    width: row_w,
                    ch: padding_char.to_string(),
                    face: pad_face,
                });
            }
        }
    }

    fn visit_container_pre(&mut self, info: &ContainerPaintInfo) {
        let cs = self.cell_size;
        let pr = to_pixel_rect(&info.area, cs);

        // Compute tight border rect from child area (GPU-specific)
        let border_rect = if info.border.is_some() {
            if let Some(child_area) = info.child_area {
                let cr = to_pixel_rect(&child_area, cs);
                let margin = cs.height * 0.15;
                // Extra top margin when a title is present so the title text
                // on the border line doesn't crowd the first content line.
                let top_margin = if info.title.is_some() {
                    cs.height * 0.5
                } else {
                    margin
                };
                PixelRect {
                    x: cr.x - margin,
                    y: cr.y - top_margin,
                    w: cr.w + margin * 2.0,
                    h: cr.h + top_margin + margin,
                }
            } else {
                pr.clone()
            }
        } else {
            pr.clone()
        };

        // Drop shadow
        if info.shadow {
            let cw = cs.width;
            self.out.push(DrawCommand::DrawShadow {
                rect: border_rect.clone(),
                offset: (cw * 0.25, cw * 0.3),
                blur_radius: cw * 0.7,
                color: [0.0, 0.0, 0.0, 0.35],
            });
        }

        // Background fill — floating containers (shadow=true) use elevated
        self.out.push(DrawCommand::FillRect {
            rect: pr,
            face: info.face,
            elevated: info.shadow,
        });

        // Border
        if let Some(border_config) = info.border {
            let border_face = info.border_face.unwrap_or(info.face);

            self.out.push(DrawCommand::DrawBorder {
                rect: border_rect.clone(),
                line_style: border_config.line_style.clone(),
                face: border_face,
                fill_face: None,
            });

            // Title
            if let Some(title_atoms) = info.title {
                let resolved_title = resolve_atoms(title_atoms, Some(&border_face));
                self.out.push(DrawCommand::DrawBorderTitle {
                    rect: border_rect,
                    title: resolved_title,
                    border_face,
                    elevated: info.shadow,
                });
            }
        }
    }

    fn visit_stack_overlay_pre(&mut self) {
        self.out.push(DrawCommand::BeginOverlay);
    }

    fn visit_scrollable_pre(&mut self, area: Rect) {
        let pr = to_pixel_rect(&area, self.cell_size);
        self.out.push(DrawCommand::PushClip(pr));
    }

    fn visit_scrollable_post(&mut self) {
        self.out.push(DrawCommand::PopClip);
    }
}

// ---------------------------------------------------------------------------
// Convenience entry points
// ---------------------------------------------------------------------------

/// Paint an element tree into a CellGrid using the walk_paint visitor pattern.
pub(crate) fn walk_paint_grid(
    element: &Element,
    layout: &LayoutResult,
    grid: &mut CellGrid,
    state: &AppState,
    theme: &Theme,
) {
    let mut visitor = GridPaintVisitor::new(grid);
    walk_paint(&mut visitor, element, layout, state, theme);
}

/// Paint an element tree into a `Vec<DrawCommand>` using the walk_paint visitor pattern.
/// Matches `scene::scene_paint_section` signature for 1:1 replacement in the pipeline.
pub(crate) fn walk_paint_scene_section(
    element: &Element,
    layout: &LayoutResult,
    state: &AppState,
    theme: &Theme,
    cell_size: CellSize,
    cursor_style: CursorStyle,
) -> Vec<DrawCommand> {
    let mut commands = Vec::with_capacity(64);
    let mut visitor = ScenePaintVisitor::new(&mut commands, cell_size, cursor_style);
    walk_paint(&mut visitor, element, layout, state, theme);
    commands
}

/// Paint a full element tree into a `Vec<DrawCommand>`.
/// Matches `scene::scene_paint` signature for 1:1 replacement.
pub(crate) fn walk_paint_scene(
    element: &Element,
    layout: &LayoutResult,
    state: &AppState,
    theme: &Theme,
    cell_size: CellSize,
    cursor_style: CursorStyle,
) -> Vec<DrawCommand> {
    let mut commands = Vec::with_capacity(256);
    let mut visitor = ScenePaintVisitor::new(&mut commands, cell_size, cursor_style);
    walk_paint(&mut visitor, element, layout, state, theme);
    commands
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::{
        BorderConfig, BorderLineStyle, Direction, Edges, Element, FlexChild, Overlay,
        OverlayAnchor, Style,
    };
    use crate::layout::flex::place;
    use crate::plugin::PluginRuntime;
    use crate::protocol::Face;
    use crate::render::paint;
    use crate::render::scene;
    use crate::render::view;
    use crate::test_utils::*;

    fn default_cell_size() -> CellSize {
        CellSize {
            width: 10.0,
            height: 20.0,
        }
    }

    /// Assert two CellGrids are cell-by-cell identical.
    fn assert_grids_equal(old: &CellGrid, new: &CellGrid) {
        assert_eq!(old.width(), new.width(), "grid width mismatch");
        assert_eq!(old.height(), new.height(), "grid height mismatch");
        for y in 0..old.height() {
            for x in 0..old.width() {
                let o = old.get(x, y);
                let n = new.get(x, y);
                match (o, n) {
                    (Some(o), Some(n)) => {
                        assert_eq!(
                            o.grapheme, n.grapheme,
                            "grapheme mismatch at ({x}, {y}): old={:?} new={:?}",
                            o.grapheme, n.grapheme
                        );
                        assert_eq!(o.face, n.face, "face mismatch at ({x}, {y})");
                        assert_eq!(o.width, n.width, "width mismatch at ({x}, {y})");
                    }
                    (None, None) => {}
                    _ => panic!("cell presence mismatch at ({x}, {y})"),
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Grid cross-validation tests
    // -----------------------------------------------------------------------

    #[test]
    fn cross_validate_grid_text() {
        let state = default_state();
        let theme = Theme::default_theme();
        let el = Element::text("hello", Face::default());
        let area = root_area(20, 5);
        let layout = place(&el, area, &state);

        let mut old_grid = CellGrid::new(20, 5);
        paint::paint(&el, &layout, &mut old_grid, &state);

        let mut new_grid = CellGrid::new(20, 5);
        walk_paint_grid(&el, &layout, &mut new_grid, &state, &theme);

        assert_grids_equal(&old_grid, &new_grid);
    }

    #[test]
    fn cross_validate_grid_buffer() {
        let mut state = default_state();
        state.lines = vec![make_line("line1"), make_line("line2")];
        state.cols = 10;
        state.rows = 4;
        let theme = Theme::default_theme();

        let el = Element::buffer_ref(0..3);
        let area = Rect {
            x: 0,
            y: 0,
            w: 10,
            h: 3,
        };
        let layout = place(&el, area, &state);

        let mut old_grid = CellGrid::new(10, 4);
        paint::paint(&el, &layout, &mut old_grid, &state);

        let mut new_grid = CellGrid::new(10, 4);
        walk_paint_grid(&el, &layout, &mut new_grid, &state, &theme);

        assert_grids_equal(&old_grid, &new_grid);
    }

    #[test]
    fn cross_validate_grid_flex() {
        let state = default_state();
        let theme = Theme::default_theme();
        let el = Element::column(vec![
            FlexChild::fixed(Element::text("aaa", Face::default())),
            FlexChild::fixed(Element::text("bbb", Face::default())),
        ]);
        let area = root_area(20, 5);
        let layout = place(&el, area, &state);

        let mut old_grid = CellGrid::new(20, 5);
        paint::paint(&el, &layout, &mut old_grid, &state);

        let mut new_grid = CellGrid::new(20, 5);
        walk_paint_grid(&el, &layout, &mut new_grid, &state, &theme);

        assert_grids_equal(&old_grid, &new_grid);
    }

    #[test]
    fn cross_validate_grid_container_border() {
        let state = default_state();
        let theme = Theme::default_theme();
        let el = Element::Container {
            child: Box::new(Element::text("hi", Face::default())),
            border: Some(BorderConfig::from(BorderLineStyle::Rounded)),
            shadow: false,
            padding: Edges::ZERO,
            style: Style::from(Face::default()),
            title: None,
        };
        let area = Rect {
            x: 0,
            y: 0,
            w: 6,
            h: 3,
        };
        let layout = place(&el, area, &state);

        let mut old_grid = CellGrid::new(20, 10);
        paint::paint(&el, &layout, &mut old_grid, &state);

        let mut new_grid = CellGrid::new(20, 10);
        walk_paint_grid(&el, &layout, &mut new_grid, &state, &theme);

        assert_grids_equal(&old_grid, &new_grid);
    }

    #[test]
    fn cross_validate_grid_container_shadow_title() {
        let state = default_state();
        let theme = Theme::default_theme();
        let el = Element::Container {
            child: Box::new(Element::text("content", Face::default())),
            border: Some(BorderConfig::from(BorderLineStyle::Rounded)),
            shadow: true,
            padding: Edges::ZERO,
            style: Style::from(Face::default()),
            title: Some(make_line("Title")),
        };
        let area = Rect {
            x: 0,
            y: 0,
            w: 12,
            h: 3,
        };
        let layout = place(&el, area, &state);

        let mut old_grid = CellGrid::new(20, 10);
        paint::paint(&el, &layout, &mut old_grid, &state);

        let mut new_grid = CellGrid::new(20, 10);
        walk_paint_grid(&el, &layout, &mut new_grid, &state, &theme);

        assert_grids_equal(&old_grid, &new_grid);
    }

    #[test]
    fn cross_validate_grid_stack_overlay() {
        let state = default_state();
        let theme = Theme::default_theme();
        let el = Element::stack(
            Element::text("base_text", Face::default()),
            vec![Overlay {
                element: Element::text("pop", Face::default()),
                anchor: OverlayAnchor::Absolute {
                    x: 5,
                    y: 3,
                    w: 3,
                    h: 1,
                },
            }],
        );
        let area = root_area(20, 10);
        let layout = place(&el, area, &state);

        let mut old_grid = CellGrid::new(20, 10);
        paint::paint(&el, &layout, &mut old_grid, &state);

        let mut new_grid = CellGrid::new(20, 10);
        walk_paint_grid(&el, &layout, &mut new_grid, &state, &theme);

        assert_grids_equal(&old_grid, &new_grid);
    }

    #[test]
    fn cross_validate_grid_grid_layout() {
        let state = default_state();
        let theme = Theme::default_theme();
        let el = Element::Grid {
            columns: vec![
                crate::element::GridColumn::fixed(5),
                crate::element::GridColumn::fixed(5),
            ],
            children: vec![
                Element::text("hello", Face::default()),
                Element::text("world", Face::default()),
            ],
            col_gap: 0,
            row_gap: 0,
            align: crate::element::Align::Start,
            cross_align: crate::element::Align::Start,
        };
        let area = root_area(20, 5);
        let layout = place(&el, area, &state);

        let mut old_grid = CellGrid::new(20, 5);
        paint::paint(&el, &layout, &mut old_grid, &state);

        let mut new_grid = CellGrid::new(20, 5);
        walk_paint_grid(&el, &layout, &mut new_grid, &state, &theme);

        assert_grids_equal(&old_grid, &new_grid);
    }

    #[test]
    fn cross_validate_grid_scrollable() {
        let state = default_state();
        let theme = Theme::default_theme();
        let el = Element::Scrollable {
            child: Box::new(Element::text("content", Face::default())),
            offset: 0,
            direction: Direction::Column,
        };
        let area = Rect {
            x: 0,
            y: 0,
            w: 10,
            h: 5,
        };
        let layout = place(&el, area, &state);

        let mut old_grid = CellGrid::new(10, 5);
        paint::paint(&el, &layout, &mut old_grid, &state);

        let mut new_grid = CellGrid::new(10, 5);
        walk_paint_grid(&el, &layout, &mut new_grid, &state, &theme);

        assert_grids_equal(&old_grid, &new_grid);
    }

    #[test]
    fn cross_validate_grid_full_view() {
        let mut state = default_state();
        state.cols = 20;
        state.rows = 5;
        state.lines = vec![make_line("hello"), make_line("world")];
        state.status_line = make_line(" test ");
        state.status_mode_line = make_line("normal");
        let theme = Theme::default_theme();

        let registry = PluginRuntime::new();
        let element = view::view(&state, &registry.view());
        let root = root_area(state.cols, state.rows);
        let layout = place(&element, root, &state);

        let mut old_grid = CellGrid::new(state.cols, state.rows);
        paint::paint(&element, &layout, &mut old_grid, &state);

        let mut new_grid = CellGrid::new(state.cols, state.rows);
        walk_paint_grid(&element, &layout, &mut new_grid, &state, &theme);

        assert_grids_equal(&old_grid, &new_grid);
    }

    // -----------------------------------------------------------------------
    // Scene cross-validation tests
    // -----------------------------------------------------------------------

    #[test]
    fn cross_validate_scene_full_view() {
        let mut state = default_state();
        state.cols = 20;
        state.rows = 5;
        state.lines = vec![make_line("hello"), make_line("world")];
        state.status_line = make_line(" test ");
        state.status_mode_line = make_line("normal");

        let theme = Theme::default_theme();
        let cs = default_cell_size();
        let cursor = CursorStyle::Block;

        let registry = PluginRuntime::new();
        let element = view::view(&state, &registry.view());
        let root = root_area(state.cols, state.rows);
        let layout = place(&element, root, &state);

        let old_commands = scene::scene_paint(&element, &layout, &state, &theme, cs, cursor);
        let new_commands = walk_paint_scene(&element, &layout, &state, &theme, cs, cursor);

        assert_eq!(old_commands, new_commands);
    }

    #[test]
    fn cross_validate_scene_container() {
        let state = default_state();
        let theme = Theme::default_theme();
        let cs = default_cell_size();
        let el = Element::Container {
            child: Box::new(Element::text("hi", Face::default())),
            border: Some(BorderConfig::from(BorderLineStyle::Rounded)),
            shadow: true,
            padding: Edges::ZERO,
            style: Style::from(Face::default()),
            title: Some(make_line("Title")),
        };
        let area = Rect {
            x: 0,
            y: 0,
            w: 12,
            h: 3,
        };
        let layout = place(&el, area, &state);

        let old_commands = scene::scene_paint(&el, &layout, &state, &theme, cs, CursorStyle::Block);
        let new_commands = walk_paint_scene(&el, &layout, &state, &theme, cs, CursorStyle::Block);

        assert_eq!(old_commands, new_commands);
    }

    #[test]
    fn cross_validate_scene_scrollable() {
        let state = default_state();
        let theme = Theme::default_theme();
        let cs = default_cell_size();
        let el = Element::Scrollable {
            child: Box::new(Element::text("content", Face::default())),
            offset: 0,
            direction: Direction::Column,
        };
        let area = Rect {
            x: 0,
            y: 0,
            w: 10,
            h: 5,
        };
        let layout = place(&el, area, &state);

        let old_commands = scene::scene_paint(&el, &layout, &state, &theme, cs, CursorStyle::Block);
        let new_commands = walk_paint_scene(&el, &layout, &state, &theme, cs, CursorStyle::Block);

        assert_eq!(old_commands, new_commands);
    }

    #[test]
    fn cross_validate_scene_stack_overlay() {
        let state = default_state();
        let theme = Theme::default_theme();
        let cs = default_cell_size();
        let el = Element::stack(
            Element::text("base_text", Face::default()),
            vec![Overlay {
                element: Element::text("pop", Face::default()),
                anchor: OverlayAnchor::Absolute {
                    x: 5,
                    y: 3,
                    w: 3,
                    h: 1,
                },
            }],
        );
        let area = root_area(20, 10);
        let layout = place(&el, area, &state);

        let old_commands = scene::scene_paint(&el, &layout, &state, &theme, cs, CursorStyle::Block);
        let new_commands = walk_paint_scene(&el, &layout, &state, &theme, cs, CursorStyle::Block);

        assert_eq!(old_commands, new_commands);
    }

    #[test]
    fn cross_validate_scene_buffer() {
        let mut state = default_state();
        state.lines = vec![make_line("line1"), make_line("line2")];
        state.cols = 10;
        state.rows = 4;
        let theme = Theme::default_theme();
        let cs = default_cell_size();

        let el = Element::buffer_ref(0..3);
        let area = Rect {
            x: 0,
            y: 0,
            w: 10,
            h: 3,
        };
        let layout = place(&el, area, &state);

        let old_commands = scene::scene_paint(&el, &layout, &state, &theme, cs, CursorStyle::Block);
        let new_commands = walk_paint_scene(&el, &layout, &state, &theme, cs, CursorStyle::Block);

        assert_eq!(old_commands, new_commands);
    }
}
