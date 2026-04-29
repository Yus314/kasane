//! Unified paint tree traversal via the Visitor pattern.
//!
//! `walk_paint<V: PaintVisitor>()` handles Element tree traversal,
//! monomorphized for zero-cost dispatch. Two visitor implementations:
//! - `GridPaintVisitor` (walk_grid) — TUI: writes to CellGrid
//! - `ScenePaintVisitor` (walk_scene) — GPU: emits DrawCommands

use std::ops::Range;

use super::CursorStyle;
use super::grid::CellGrid;
use super::scene::{CellSize, DrawCommand};
use super::theme::Theme;
use super::walk_grid::GridPaintVisitor;
use super::walk_scene::ScenePaintVisitor;
use crate::display::DisplayMap;
use crate::element::{
    BorderConfig, BufferRefState, Element, ElementStyle, ImageFit, ImageSource, StyleToken,
};
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
    /// Whether this container is a split divider (fill with box-drawing chars).
    pub is_split_divider: bool,
    /// Vertical divider glyph (from config, default "│").
    pub divider_vertical: &'a str,
    /// Horizontal divider glyph (from config, default "─").
    pub divider_horizontal: &'a str,
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
    #[allow(clippy::too_many_arguments)]
    fn visit_buffer_ref(
        &mut self,
        area: Rect,
        line_range: Range<usize>,
        state: &AppState,
        buffer_state: Option<&BufferRefState>,
        line_backgrounds: Option<&[Option<Face>]>,
        display_map: Option<&DisplayMap>,
        inline_decorations: Option<&[Option<crate::render::InlineDecoration>]>,
        virtual_text: Option<&[Option<Vec<Atom>>]>,
    );

    /// Pre-visit for Container: render shadow, background fill, border, title.
    /// The walk function handles recursing into the child after this returns.
    fn visit_container_pre(&mut self, info: &ContainerPaintInfo);

    /// Pre-visit for Stack overlay: emit layer boundary marker (GPU only).
    fn visit_stack_overlay_pre(&mut self);

    /// Render an Image element. TUI: text placeholder. GPU: DrawImage command.
    fn visit_image(&mut self, source: &ImageSource, fit: ImageFit, opacity: f32, area: Rect);

    /// Render a TextPanel element (plugin-owned scrollable text area).
    fn visit_text_panel(
        &mut self,
        lines: &[Vec<Atom>],
        scroll_offset: usize,
        cursor: Option<(usize, usize)>,
        line_numbers: bool,
        wrap: bool,
        area: Rect,
    );

    /// Render a Canvas element. GPU: convert ops to draw primitives. TUI: no-op.
    fn visit_canvas(&mut self, content: &crate::plugin::canvas::CanvasContent, area: Rect);

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
            let face = theme.resolve(style, &state.observed.default_style.to_face());
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
            inline_decorations,
            virtual_text,
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
                line_backgrounds.as_ref().map(|v| v.as_slice()),
                dm,
                inline_decorations.as_ref().map(|v| v.as_slice()),
                virtual_text.as_ref().map(|v| v.as_slice()),
            );
        }
        Element::TextPanel {
            lines,
            scroll_offset,
            cursor,
            line_numbers,
            wrap,
        } => {
            visitor.visit_text_panel(lines, *scroll_offset, *cursor, *line_numbers, *wrap, area);
        }
        Element::Empty => {}
        Element::Image {
            source,
            fit,
            opacity,
            ..
        } => {
            visitor.visit_image(source, *fit, *opacity, area);
        }
        Element::Canvas { content, .. } => {
            visitor.visit_canvas(content, area);
        }
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
            let face = theme.resolve(el_style, &state.observed.default_style.to_face());
            let border_face = border.as_ref().map(|bc| {
                bc.style
                    .as_ref()
                    .map(|s| theme.resolve(s, &face))
                    .unwrap_or(face)
            });
            let child_area = layout.children.first().map(|cl| cl.area);
            let is_split_divider = matches!(
                el_style,
                ElementStyle::Token(t) if *t == StyleToken::SPLIT_DIVIDER || *t == StyleToken::SPLIT_DIVIDER_FOCUSED
            );
            let info = ContainerPaintInfo {
                area,
                child_area,
                border,
                shadow: *shadow,
                face,
                border_face,
                title: title.as_deref(),
                is_split_divider,
                divider_vertical: &state.config.divider_vertical,
                divider_horizontal: &state.config.divider_horizontal,
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
// Convenience entry points
// ---------------------------------------------------------------------------

/// Paint an element tree into a CellGrid using the walk_paint visitor pattern.
#[allow(clippy::too_many_arguments)]
pub(crate) fn walk_paint_grid(
    element: &Element,
    layout: &LayoutResult,
    grid: &mut CellGrid,
    state: &AppState,
    theme: &Theme,
    halfblock_cache: Option<&mut super::halfblock::HalfblockCache>,
    image_protocol: super::ImageProtocol,
    image_requests: Option<&mut Vec<super::ImageRequest>>,
) {
    let mut visitor =
        GridPaintVisitor::new(grid, theme, halfblock_cache, image_protocol, image_requests);
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
    let mut visitor = ScenePaintVisitor::new(&mut commands, cell_size, cursor_style, theme);
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
    let mut visitor = ScenePaintVisitor::new(&mut commands, cell_size, cursor_style, theme);
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
        BorderConfig, BorderLineStyle, Direction, Edges, Element, ElementStyle, FlexChild, Overlay,
        OverlayAnchor,
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
                        assert_eq!(o.face(), n.face(), "face mismatch at ({x}, {y})");
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
        walk_paint_grid(
            &el,
            &layout,
            &mut new_grid,
            &state,
            &theme,
            None,
            Default::default(),
            None,
        );

        assert_grids_equal(&old_grid, &new_grid);
    }

    #[test]
    fn cross_validate_grid_buffer() {
        let mut state = default_state();
        state.observed.lines = vec![make_line("line1"), make_line("line2")];
        state.runtime.cols = 10;
        state.runtime.rows = 4;
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
        walk_paint_grid(
            &el,
            &layout,
            &mut new_grid,
            &state,
            &theme,
            None,
            Default::default(),
            None,
        );

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
        walk_paint_grid(
            &el,
            &layout,
            &mut new_grid,
            &state,
            &theme,
            None,
            Default::default(),
            None,
        );

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
            style: ElementStyle::from(Face::default()),
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
        walk_paint_grid(
            &el,
            &layout,
            &mut new_grid,
            &state,
            &theme,
            None,
            Default::default(),
            None,
        );

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
            style: ElementStyle::from(Face::default()),
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
        walk_paint_grid(
            &el,
            &layout,
            &mut new_grid,
            &state,
            &theme,
            None,
            Default::default(),
            None,
        );

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
        walk_paint_grid(
            &el,
            &layout,
            &mut new_grid,
            &state,
            &theme,
            None,
            Default::default(),
            None,
        );

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
        walk_paint_grid(
            &el,
            &layout,
            &mut new_grid,
            &state,
            &theme,
            None,
            Default::default(),
            None,
        );

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
        walk_paint_grid(
            &el,
            &layout,
            &mut new_grid,
            &state,
            &theme,
            None,
            Default::default(),
            None,
        );

        assert_grids_equal(&old_grid, &new_grid);
    }

    #[test]
    fn cross_validate_grid_full_view() {
        let mut state = default_state();
        state.runtime.cols = 20;
        state.runtime.rows = 5;
        state.observed.lines = vec![make_line("hello"), make_line("world")];
        state.inference.status_line = make_line(" test ");
        state.observed.status_mode_line = make_line("normal");
        let theme = Theme::default_theme();

        let registry = PluginRuntime::new();
        let element = view::view(&state, &registry.view());
        let root = root_area(state.runtime.cols, state.runtime.rows);
        let layout = place(&element, root, &state);

        let mut old_grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
        paint::paint(&element, &layout, &mut old_grid, &state);

        let mut new_grid = CellGrid::new(state.runtime.cols, state.runtime.rows);
        walk_paint_grid(
            &element,
            &layout,
            &mut new_grid,
            &state,
            &theme,
            None,
            Default::default(),
            None,
        );

        assert_grids_equal(&old_grid, &new_grid);
    }

    // -----------------------------------------------------------------------
    // Scene cross-validation tests
    // -----------------------------------------------------------------------

    #[test]
    fn cross_validate_scene_full_view() {
        let mut state = default_state();
        state.runtime.cols = 20;
        state.runtime.rows = 5;
        state.observed.lines = vec![make_line("hello"), make_line("world")];
        state.inference.status_line = make_line(" test ");
        state.observed.status_mode_line = make_line("normal");

        let theme = Theme::default_theme();
        let cs = default_cell_size();
        let cursor = CursorStyle::Block;

        let registry = PluginRuntime::new();
        let element = view::view(&state, &registry.view());
        let root = root_area(state.runtime.cols, state.runtime.rows);
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
            style: ElementStyle::from(Face::default()),
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
        state.observed.lines = vec![make_line("line1"), make_line("line2")];
        state.runtime.cols = 10;
        state.runtime.rows = 4;
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

    // -----------------------------------------------------------------------
    // Image element tests
    // -----------------------------------------------------------------------

    #[test]
    fn grid_visitor_image_filepath_fallback() {
        let state = default_state();
        let theme = Theme::default_theme();
        let el = Element::image(
            crate::element::ImageSource::FilePath("/path/to/photo.png".into()),
            20,
            3,
        );
        let area = Rect {
            x: 0,
            y: 0,
            w: 20,
            h: 3,
        };
        let layout = place(&el, area, &state);

        let mut grid = CellGrid::new(20, 3);
        walk_paint_grid(
            &el,
            &layout,
            &mut grid,
            &state,
            &theme,
            None,
            Default::default(),
            None,
        );

        // First row should contain the fallback label
        let mut text = String::new();
        for x in 0..20 {
            if let Some(cell) = grid.get(x, 0) {
                text.push_str(&cell.grapheme);
            }
        }
        assert!(
            text.contains("[IMAGE: photo.png]"),
            "expected fallback label, got: {text:?}"
        );
    }

    #[test]
    fn grid_visitor_image_rgba_fallback() {
        let state = default_state();
        let theme = Theme::default_theme();
        let data: std::sync::Arc<[u8]> = vec![0u8; 4 * 8 * 6].into();
        let el = Element::Image {
            source: crate::element::ImageSource::Rgba {
                data,
                width: 8,
                height: 6,
            },
            size: (20, 3),
            fit: crate::element::ImageFit::Contain,
            opacity: 1.0,
        };
        let area = Rect {
            x: 0,
            y: 0,
            w: 20,
            h: 3,
        };
        let layout = place(&el, area, &state);

        let mut grid = CellGrid::new(20, 3);
        walk_paint_grid(
            &el,
            &layout,
            &mut grid,
            &state,
            &theme,
            None,
            Default::default(),
            None,
        );

        let mut text = String::new();
        for x in 0..20 {
            if let Some(cell) = grid.get(x, 0) {
                text.push_str(&cell.grapheme);
            }
        }
        assert!(
            text.contains("[IMAGE: 8\u{00d7}6]"),
            "expected rgba fallback label, got: {text:?}"
        );
    }

    /// With cache=Some and tui-image, an RGBA image should render halfblock chars.
    #[cfg(feature = "tui-image")]
    #[test]
    fn grid_visitor_image_rgba_halfblock() {
        let state = default_state();
        let theme = Theme::default_theme();
        // 2×2 solid green RGBA image
        let data: std::sync::Arc<[u8]> = vec![
            0, 255, 0, 255, 0, 255, 0, 255, 0, 255, 0, 255, 0, 255, 0, 255,
        ]
        .into();
        let el = Element::Image {
            source: crate::element::ImageSource::Rgba {
                data,
                width: 2,
                height: 2,
            },
            size: (4, 2),
            fit: crate::element::ImageFit::Fill,
            opacity: 1.0,
        };
        let area = Rect {
            x: 0,
            y: 0,
            w: 4,
            h: 2,
        };
        let layout = place(&el, area, &state);

        let mut grid = CellGrid::new(4, 2);
        let mut cache = super::super::halfblock::HalfblockCache::new(16);
        walk_paint_grid(
            &el,
            &layout,
            &mut grid,
            &state,
            &theme,
            Some(&mut cache),
            Default::default(),
            None,
        );

        // All cells should be halfblock with green colors
        for y in 0..2u16 {
            for x in 0..4u16 {
                let c = grid.get(x, y).unwrap();
                assert_eq!(
                    c.grapheme.as_str(),
                    "\u{2580}",
                    "expected halfblock at ({x},{y}), got {:?}",
                    c.grapheme
                );
                assert_eq!(
                    c.face().fg,
                    crate::protocol::Color::Rgb { r: 0, g: 255, b: 0 },
                    "fg green at ({x},{y})"
                );
                assert_eq!(
                    c.face().bg,
                    crate::protocol::Color::Rgb { r: 0, g: 255, b: 0 },
                    "bg green at ({x},{y})"
                );
            }
        }
    }

    /// With cache=None, image still falls back to text placeholder.
    #[test]
    fn grid_visitor_image_no_cache_fallback() {
        let state = default_state();
        let theme = Theme::default_theme();
        let data: std::sync::Arc<[u8]> = vec![0u8; 4 * 2 * 2].into();
        let el = Element::Image {
            source: crate::element::ImageSource::Rgba {
                data,
                width: 2,
                height: 2,
            },
            size: (20, 3),
            fit: crate::element::ImageFit::Fill,
            opacity: 1.0,
        };
        let area = Rect {
            x: 0,
            y: 0,
            w: 20,
            h: 3,
        };
        let layout = place(&el, area, &state);

        let mut grid = CellGrid::new(20, 3);
        walk_paint_grid(
            &el,
            &layout,
            &mut grid,
            &state,
            &theme,
            None,
            Default::default(),
            None,
        );

        let mut text = String::new();
        for x in 0..20 {
            if let Some(cell) = grid.get(x, 0) {
                text.push_str(&cell.grapheme);
            }
        }
        assert!(
            text.contains("[IMAGE: 2\u{00d7}2]"),
            "expected fallback label with no cache, got: {text:?}"
        );
    }

    #[test]
    fn scene_visitor_image_emits_draw_image() {
        let state = default_state();
        let theme = Theme::default_theme();
        let cs = default_cell_size();
        let el = Element::image(
            crate::element::ImageSource::FilePath("test.png".into()),
            10,
            5,
        );
        let area = Rect {
            x: 2,
            y: 1,
            w: 10,
            h: 5,
        };
        let layout = place(&el, area, &state);

        let commands = walk_paint_scene(&el, &layout, &state, &theme, cs, CursorStyle::Block);

        assert_eq!(commands.len(), 1);
        match &commands[0] {
            DrawCommand::DrawImage {
                rect,
                source,
                fit,
                opacity,
            } => {
                assert_eq!(rect.x, 20.0); // 2 * 10.0
                assert_eq!(rect.y, 20.0); // 1 * 20.0
                assert_eq!(rect.w, 100.0); // 10 * 10.0
                assert_eq!(rect.h, 100.0); // 5 * 20.0
                assert_eq!(
                    *source,
                    crate::element::ImageSource::FilePath("test.png".into())
                );
                assert_eq!(*fit, crate::element::ImageFit::Contain);
                assert_eq!(*opacity, 1.0);
            }
            other => panic!("expected DrawImage, got: {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // Split divider glyph tests
    // -----------------------------------------------------------------------

    #[test]
    fn grid_divider_vertical_fills_box_drawing() {
        let state = default_state();
        let theme = Theme::default_theme();
        let el = Element::container(
            Element::Empty,
            ElementStyle::Token(StyleToken::SPLIT_DIVIDER),
        );
        let area = Rect {
            x: 5,
            y: 0,
            w: 1,
            h: 5,
        };
        let layout = place(&el, area, &state);
        let mut grid = CellGrid::new(10, 5);
        walk_paint_grid(
            &el,
            &layout,
            &mut grid,
            &state,
            &theme,
            None,
            Default::default(),
            None,
        );

        for y in 0..5u16 {
            let cell = grid.get(5, y).expect("cell should exist");
            assert_eq!(cell.grapheme, "│", "vertical divider at y={y}");
        }
    }

    #[test]
    fn grid_divider_horizontal_fills_box_drawing() {
        let state = default_state();
        let theme = Theme::default_theme();
        let el = Element::container(
            Element::Empty,
            ElementStyle::Token(StyleToken::SPLIT_DIVIDER),
        );
        let area = Rect {
            x: 0,
            y: 3,
            w: 10,
            h: 1,
        };
        let layout = place(&el, area, &state);
        let mut grid = CellGrid::new(10, 5);
        walk_paint_grid(
            &el,
            &layout,
            &mut grid,
            &state,
            &theme,
            None,
            Default::default(),
            None,
        );

        for x in 0..10u16 {
            let cell = grid.get(x, 3).expect("cell should exist");
            assert_eq!(cell.grapheme, "─", "horizontal divider at x={x}");
        }
    }

    #[test]
    fn grid_divider_focused_has_default_fg() {
        use crate::protocol::{Color, NamedColor};
        let state = default_state();
        let theme = Theme::default_theme();
        let area = Rect {
            x: 0,
            y: 0,
            w: 1,
            h: 3,
        };

        // Normal divider: fg matches bg (BrightBlack), chars blend in
        let el_normal = Element::container(
            Element::Empty,
            ElementStyle::Token(StyleToken::SPLIT_DIVIDER),
        );
        let layout = place(&el_normal, area, &state);
        let mut grid = CellGrid::new(5, 3);
        walk_paint_grid(
            &el_normal,
            &layout,
            &mut grid,
            &state,
            &theme,
            None,
            Default::default(),
            None,
        );
        let normal_fg = grid.get(0, 0).expect("cell").face().fg;
        assert_eq!(
            normal_fg,
            Color::Named(NamedColor::BrightBlack),
            "normal divider fg should be BrightBlack"
        );

        // Focused divider: fg is Default (bright), chars stand out
        let el_focused = Element::container(
            Element::Empty,
            ElementStyle::Token(StyleToken::SPLIT_DIVIDER_FOCUSED),
        );
        let layout = place(&el_focused, area, &state);
        let mut grid = CellGrid::new(5, 3);
        walk_paint_grid(
            &el_focused,
            &layout,
            &mut grid,
            &state,
            &theme,
            None,
            Default::default(),
            None,
        );
        let focused_fg = grid.get(0, 0).expect("cell").face().fg;
        assert_eq!(
            focused_fg,
            Color::Default,
            "focused divider fg should be Default (bright)"
        );

        // Verify they differ
        assert_ne!(normal_fg, focused_fg, "normal and focused fg must differ");
    }

    #[test]
    fn scene_divider_vertical_emits_draw_text() {
        let state = default_state();
        let theme = Theme::default_theme();
        let cs = default_cell_size();
        let el = Element::container(
            Element::Empty,
            ElementStyle::Token(StyleToken::SPLIT_DIVIDER),
        );
        let area = Rect {
            x: 5,
            y: 0,
            w: 1,
            h: 3,
        };
        let layout = place(&el, area, &state);
        let commands = walk_paint_scene(&el, &layout, &state, &theme, cs, CursorStyle::Block);

        // FillRect + 3 DrawText (one per row)
        let text_cmds: Vec<_> = commands
            .iter()
            .filter(|c| matches!(c, DrawCommand::DrawText { .. }))
            .collect();
        assert_eq!(text_cmds.len(), 3, "should emit one DrawText per row");
        for cmd in &text_cmds {
            if let DrawCommand::DrawText { text, .. } = cmd {
                assert_eq!(text, "│");
            }
        }
    }

    #[test]
    fn text_panel_paints_visible_lines() {
        let state = default_state();
        let theme = Theme::default_theme();
        let lines: Vec<Vec<Atom>> = vec![
            vec![Atom::from_face(Face::default(), "hello")],
            vec![Atom::from_face(Face::default(), "world")],
            vec![Atom::from_face(Face::default(), "third")],
        ];
        let el = Element::text_panel(lines);
        let area = root_area(20, 3);
        let layout = place(&el, area, &state);

        let mut grid = CellGrid::new(20, 3);
        walk_paint_grid(
            &el,
            &layout,
            &mut grid,
            &state,
            &theme,
            None,
            Default::default(),
            None,
        );

        // First line should start with "hello"
        let cell = grid.get(0, 0).unwrap();
        assert_eq!(cell.grapheme.as_str(), "h");
        let cell = grid.get(0, 1).unwrap();
        assert_eq!(cell.grapheme.as_str(), "w");
        let cell = grid.get(0, 2).unwrap();
        assert_eq!(cell.grapheme.as_str(), "t");
    }

    #[test]
    fn text_panel_scroll_offset() {
        let state = default_state();
        let theme = Theme::default_theme();
        let lines: Vec<Vec<Atom>> = vec![
            vec![Atom::from_face(Face::default(), "line0")],
            vec![Atom::from_face(Face::default(), "line1")],
            vec![Atom::from_face(Face::default(), "line2")],
            vec![Atom::from_face(Face::default(), "line3")],
        ];
        let el = Element::TextPanel {
            lines,
            scroll_offset: 2,
            cursor: None,
            line_numbers: false,
            wrap: false,
        };
        let area = root_area(20, 2);
        let layout = place(&el, area, &state);

        let mut grid = CellGrid::new(20, 2);
        walk_paint_grid(
            &el,
            &layout,
            &mut grid,
            &state,
            &theme,
            None,
            Default::default(),
            None,
        );

        // Row 0 should show "line2", row 1 should show "line3"
        let cell = grid.get(4, 0).unwrap();
        assert_eq!(cell.grapheme.as_str(), "2");
        let cell = grid.get(4, 1).unwrap();
        assert_eq!(cell.grapheme.as_str(), "3");
    }

    #[test]
    fn text_panel_with_line_numbers() {
        let state = default_state();
        let theme = Theme::default_theme();
        let lines: Vec<Vec<Atom>> = vec![
            vec![Atom::from_face(Face::default(), "abc")],
            vec![Atom::from_face(Face::default(), "def")],
        ];
        let el = Element::TextPanel {
            lines,
            scroll_offset: 0,
            cursor: None,
            line_numbers: true,
            wrap: false,
        };
        let area = root_area(20, 2);
        let layout = place(&el, area, &state);

        let mut grid = CellGrid::new(20, 2);
        walk_paint_grid(
            &el,
            &layout,
            &mut grid,
            &state,
            &theme,
            None,
            Default::default(),
            None,
        );

        // Line numbers should appear: "1 " then "2 "
        let cell = grid.get(0, 0).unwrap();
        assert_eq!(cell.grapheme.as_str(), "1");
        let cell = grid.get(0, 1).unwrap();
        assert_eq!(cell.grapheme.as_str(), "2");
        // Content should be offset by gutter width (1 digit + 1 space = 2)
        let cell = grid.get(2, 0).unwrap();
        assert_eq!(cell.grapheme.as_str(), "a");
    }

    #[test]
    fn text_panel_gpu_emits_draw_commands() {
        let state = default_state();
        let theme = Theme::default_theme();
        let cs = default_cell_size();
        let lines: Vec<Vec<Atom>> = vec![
            vec![Atom::from_face(Face::default(), "hello")],
            vec![Atom::from_face(Face::default(), "world")],
        ];
        let el = Element::text_panel(lines);
        let area = root_area(20, 2);
        let layout = place(&el, area, &state);

        let commands = walk_paint_scene(&el, &layout, &state, &theme, cs, CursorStyle::Block);
        let atom_cmds: Vec<_> = commands
            .iter()
            .filter(|c| matches!(c, DrawCommand::DrawAtoms { .. }))
            .collect();
        assert_eq!(
            atom_cmds.len(),
            2,
            "should emit one DrawAtoms per visible line"
        );
    }
}
