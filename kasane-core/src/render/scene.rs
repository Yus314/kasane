use unicode_width::UnicodeWidthStr;

use super::grid::resolve_face;
use super::theme::Theme;
use crate::element::{BorderConfig, BorderLineStyle, Element};
use crate::layout::Rect;
use crate::layout::flex::LayoutResult;
use crate::protocol::{Atom, Face};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Pixel-coordinate rectangle.
#[derive(Debug, Clone, PartialEq)]
pub struct PixelRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Pixel-coordinate position.
#[derive(Debug, Clone, PartialEq)]
pub struct PixelPos {
    pub x: f32,
    pub y: f32,
}

/// Cell size for cell→pixel conversion.
#[derive(Debug, Clone, Copy)]
pub struct CellSize {
    pub width: f32,
    pub height: f32,
}

/// An Atom with faces resolved against a base face.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedAtom {
    pub contents: String,
    pub face: Face,
}

/// GPU draw command produced by `scene_paint`.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawCommand {
    /// Fill a rectangle (background).
    FillRect { rect: PixelRect, face: Face },

    /// Draw a sequence of atoms (buffer lines, status line, menu items).
    DrawAtoms {
        pos: PixelPos,
        atoms: Vec<ResolvedAtom>,
        max_width: f32,
    },

    /// Draw plain text (Element::Text).
    DrawText {
        pos: PixelPos,
        text: String,
        face: Face,
        max_width: f32,
    },

    /// Draw a pixel-based border.
    DrawBorder {
        rect: PixelRect,
        line_style: BorderLineStyle,
        face: Face,
        /// Optional interior fill (background inside the border).
        fill_face: Option<Face>,
    },

    /// Draw a border title.
    DrawBorderTitle {
        rect: PixelRect,
        title: Vec<ResolvedAtom>,
        border_face: Face,
    },

    /// Draw a drop shadow.
    DrawShadow {
        rect: PixelRect,
        offset: (f32, f32),
        blur_radius: f32,
        color: [f32; 4],
    },

    /// Draw a padding row (post-buffer "~" rows).
    DrawPaddingRow {
        pos: PixelPos,
        width: f32,
        ch: String,
        face: Face,
    },

    /// Push a clipping rectangle.
    PushClip(PixelRect),
    /// Pop the most recent clipping rectangle.
    PopClip,

    /// Layer boundary: all subsequent commands belong to a new overlay layer.
    ///
    /// The renderer must flush (bg → border → text) before starting the new
    /// layer so that overlay backgrounds occlude base-layer text.
    BeginOverlay,
}

// ---------------------------------------------------------------------------
// scene_paint
// ---------------------------------------------------------------------------

struct SceneContext<'a> {
    state: &'a AppState,
    theme: &'a Theme,
    cell_size: CellSize,
    cursor_style: super::CursorStyle,
}

/// Walk the element tree and produce GPU draw commands.
pub fn scene_paint(
    element: &Element,
    layout: &LayoutResult,
    state: &AppState,
    theme: &Theme,
    cell_size: CellSize,
    cursor_style: super::CursorStyle,
) -> Vec<DrawCommand> {
    let mut commands = Vec::with_capacity(256);
    let ctx = SceneContext {
        state,
        theme,
        cell_size,
        cursor_style,
    };
    scene_paint_inner(&ctx, element, layout, &mut commands);
    commands
}

fn scene_paint_inner(
    ctx: &SceneContext,
    element: &Element,
    layout: &LayoutResult,
    out: &mut Vec<DrawCommand>,
) {
    let area = layout.area;

    match element {
        Element::Text(text, style) => {
            let face = ctx.theme.resolve(style, &ctx.state.default_face);
            let pr = to_pixel_rect(&area, ctx.cell_size);
            out.push(DrawCommand::DrawText {
                pos: PixelPos { x: pr.x, y: pr.y },
                text: text.clone(),
                face,
                max_width: pr.w,
            });
        }
        Element::StyledLine(atoms) => {
            let pr = to_pixel_rect(&area, ctx.cell_size);
            let resolved = resolve_atoms(atoms, None);
            out.push(DrawCommand::DrawAtoms {
                pos: PixelPos { x: pr.x, y: pr.y },
                atoms: resolved,
                max_width: pr.w,
            });
        }
        Element::BufferRef { line_range } => {
            scene_paint_buffer_ref(ctx, &area, line_range.clone(), out);
        }
        Element::Empty => {}
        Element::Flex { children, .. } => {
            for (i, child) in children.iter().enumerate() {
                if let Some(child_layout) = layout.children.get(i) {
                    scene_paint_inner(ctx, &child.element, child_layout, out);
                }
            }
        }
        Element::Stack { base, overlays } => {
            if let Some(base_layout) = layout.children.first() {
                scene_paint_inner(ctx, base, base_layout, out);
            }
            for (i, overlay) in overlays.iter().enumerate() {
                if let Some(overlay_layout) = layout.children.get(i + 1) {
                    out.push(DrawCommand::BeginOverlay);
                    scene_paint_inner(ctx, &overlay.element, overlay_layout, out);
                }
            }
        }
        Element::Container {
            child,
            border,
            shadow,
            padding: _,
            style,
            title,
        } => {
            let face = ctx.theme.resolve(style, &ctx.state.default_face);
            scene_paint_container(
                ctx,
                &area,
                child,
                border,
                *shadow,
                &face,
                title.as_deref(),
                layout,
                out,
            );
        }
        Element::Interactive { child, .. } => {
            if let Some(child_layout) = layout.children.first() {
                scene_paint_inner(ctx, child, child_layout, out);
            }
        }
        Element::Scrollable {
            child,
            offset: _,
            direction: _,
        } => {
            let pr = to_pixel_rect(&area, ctx.cell_size);
            out.push(DrawCommand::PushClip(pr));
            if let Some(child_layout) = layout.children.first() {
                scene_paint_inner(ctx, child, child_layout, out);
            }
            out.push(DrawCommand::PopClip);
        }
    }
}

// ---------------------------------------------------------------------------
// Element-specific paint helpers
// ---------------------------------------------------------------------------

fn scene_paint_buffer_ref(
    ctx: &SceneContext,
    area: &Rect,
    line_range: std::ops::Range<usize>,
    out: &mut Vec<DrawCommand>,
) {
    let cs = ctx.cell_size;

    for y_offset in 0..area.h {
        let line_idx = line_range.start + y_offset as usize;
        let py = (area.y + y_offset) as f32 * cs.height;
        let px = area.x as f32 * cs.width;
        let row_w = area.w as f32 * cs.width;

        if let Some(line) = ctx.state.lines.get(line_idx) {
            // Background fill for the row
            out.push(DrawCommand::FillRect {
                rect: PixelRect {
                    x: px,
                    y: py,
                    w: row_w,
                    h: cs.height,
                },
                face: ctx.state.default_face,
            });
            // Atoms — clear PrimaryCursor face at the cursor cell in
            // non-block cursor modes so the thin bar/underline is visible.
            let mut resolved = resolve_atoms(line, Some(&ctx.state.default_face));
            if !matches!(
                ctx.cursor_style,
                super::CursorStyle::Block | super::CursorStyle::Outline
            ) && ctx.state.cursor_mode == crate::protocol::CursorMode::Buffer
                && line_idx == ctx.state.cursor_pos.line as usize
            {
                clear_cursor_atom(
                    &mut resolved,
                    ctx.state.cursor_pos.column as u16,
                    &ctx.state.default_face,
                );
            }
            out.push(DrawCommand::DrawAtoms {
                pos: PixelPos { x: px, y: py },
                atoms: resolved,
                max_width: row_w,
            });
        } else {
            // Padding row
            out.push(DrawCommand::FillRect {
                rect: PixelRect {
                    x: px,
                    y: py,
                    w: row_w,
                    h: cs.height,
                },
                face: ctx.state.padding_face,
            });
            let mut pad_face = ctx.state.padding_face;
            if pad_face.fg == pad_face.bg {
                pad_face.fg = ctx.state.default_face.fg;
            }
            out.push(DrawCommand::DrawPaddingRow {
                pos: PixelPos { x: px, y: py },
                width: row_w,
                ch: ctx.state.padding_char.clone(),
                face: pad_face,
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn scene_paint_container(
    ctx: &SceneContext,
    area: &Rect,
    child: &Element,
    border: &Option<BorderConfig>,
    shadow: bool,
    face: &Face,
    title: Option<&[Atom]>,
    layout: &LayoutResult,
    out: &mut Vec<DrawCommand>,
) {
    let pr = to_pixel_rect(area, ctx.cell_size);

    // For bordered containers, position the border tight around the content
    // area rather than at the outer container edge.  In TUI the border
    // characters (╭─╮) sit at the same cell row as text, so the frame hugs
    // the content.  GPU borders are thin pixel lines, so we derive the
    // border rect from the child layout area with a small margin to match
    // the TUI visual appearance.
    let border_rect = if border.is_some() {
        if let Some(child_layout) = layout.children.first() {
            let cr = to_pixel_rect(&child_layout.area, ctx.cell_size);
            let margin = ctx.cell_size.height * 0.15;
            PixelRect {
                x: cr.x - margin,
                y: cr.y - margin,
                w: cr.w + margin * 2.0,
                h: cr.h + margin * 2.0,
            }
        } else {
            pr.clone()
        }
    } else {
        pr.clone()
    };

    // Shadow (matches the border frame, not the full container area)
    if shadow {
        let offset = ctx.cell_size.width;
        out.push(DrawCommand::DrawShadow {
            rect: border_rect.clone(),
            offset: (offset, offset),
            blur_radius: offset * 2.0,
            color: [0.0, 0.0, 0.0, 0.4],
        });
    }

    // Background fill — always covers the full container area so the popup
    // hides the content underneath.
    out.push(DrawCommand::FillRect {
        rect: pr.clone(),
        face: *face,
    });

    // Border — drawn tight around the content area
    if let Some(border_config) = border {
        let border_face = border_config
            .face
            .as_ref()
            .map(|s| ctx.theme.resolve(s, face))
            .unwrap_or(*face);

        out.push(DrawCommand::DrawBorder {
            rect: border_rect.clone(),
            line_style: border_config.line_style,
            face: border_face,
            fill_face: None,
        });

        // Title
        if let Some(title_atoms) = title {
            let resolved_title = resolve_atoms(title_atoms, Some(&border_face));
            out.push(DrawCommand::DrawBorderTitle {
                rect: border_rect,
                title: resolved_title,
                border_face,
            });
        }
    }

    // Child
    if let Some(child_layout) = layout.children.first() {
        scene_paint_inner(ctx, child, child_layout, out);
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert a cell-coordinate Rect to a PixelRect.
fn to_pixel_rect(rect: &Rect, cs: CellSize) -> PixelRect {
    PixelRect {
        x: rect.x as f32 * cs.width,
        y: rect.y as f32 * cs.height,
        w: rect.w as f32 * cs.width,
        h: rect.h as f32 * cs.height,
    }
}

/// Resolve atom faces against an optional base face.
fn resolve_atoms(atoms: &[Atom], base_face: Option<&Face>) -> Vec<ResolvedAtom> {
    atoms
        .iter()
        .map(|atom| {
            let face = match base_face {
                Some(base) => resolve_face(&atom.face, base),
                None => atom.face,
            };
            ResolvedAtom {
                contents: atom.contents.to_string(),
                face,
            }
        })
        .collect()
}

/// Clear the PrimaryCursor face from the atom at the given column so that
/// non-block cursor shapes (bar, underline) are visible.
fn clear_cursor_atom(atoms: &mut [ResolvedAtom], cursor_col: u16, base_face: &Face) {
    let mut col: u16 = 0;
    for atom in atoms.iter_mut() {
        let w = line_display_width_str(&atom.contents) as u16;
        if cursor_col >= col && cursor_col < col + w {
            atom.face = *base_face;
            return;
        }
        col += w;
    }
}

/// Compute display width of a string (for atom width calculations).
pub fn line_display_width_str(s: &str) -> usize {
    s.split(|c: char| c.is_control())
        .map(UnicodeWidthStr::width)
        .sum()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Rect;
    use crate::layout::flex::place;
    use crate::plugin::PluginRegistry;
    use crate::protocol::{Atom, Face};
    use crate::render::CursorStyle;
    use crate::render::view;

    fn default_state() -> AppState {
        AppState::default()
    }

    fn root_area(w: u16, h: u16) -> Rect {
        Rect { x: 0, y: 0, w, h }
    }

    fn make_line(s: &str) -> Vec<Atom> {
        vec![Atom {
            face: Face::default(),
            contents: s.into(),
        }]
    }

    fn cell_size() -> CellSize {
        CellSize {
            width: 10.0,
            height: 20.0,
        }
    }

    fn scene_render(state: &AppState) -> Vec<DrawCommand> {
        let registry = PluginRegistry::new();
        let element = view::view(state, &registry);
        let root = root_area(state.cols, state.rows);
        let layout = place(&element, root, state);
        let theme = Theme::default_theme();
        scene_paint(
            &element,
            &layout,
            state,
            &theme,
            cell_size(),
            CursorStyle::Block,
        )
    }

    #[test]
    fn test_basic_buffer_produces_fill_and_atoms() {
        let mut state = default_state();
        state.cols = 20;
        state.rows = 5;
        state.lines = vec![make_line("hello"), make_line("world")];
        state.status_line = make_line(" test ");
        state.status_mode_line = make_line("normal");

        let commands = scene_render(&state);

        // Should have FillRect and DrawAtoms for each buffer line
        let fill_count = commands
            .iter()
            .filter(|c| matches!(c, DrawCommand::FillRect { .. }))
            .count();
        let atom_count = commands
            .iter()
            .filter(|c| matches!(c, DrawCommand::DrawAtoms { .. }))
            .count();

        assert!(
            fill_count >= 2,
            "expected at least 2 FillRect, got {fill_count}"
        );
        assert!(
            atom_count >= 2,
            "expected at least 2 DrawAtoms, got {atom_count}"
        );
    }

    #[test]
    fn test_empty_buffer_produces_padding_rows() {
        let mut state = default_state();
        state.cols = 20;
        state.rows = 5;
        state.lines = vec![];
        state.status_line = make_line(" test ");
        state.status_mode_line = make_line("normal");

        let commands = scene_render(&state);

        let padding_count = commands
            .iter()
            .filter(|c| matches!(c, DrawCommand::DrawPaddingRow { .. }))
            .count();
        // All buffer rows should be padding
        assert!(
            padding_count >= 4,
            "expected at least 4 padding rows, got {padding_count}"
        );
    }

    #[test]
    fn test_status_bar_produces_commands() {
        let mut state = default_state();
        state.cols = 20;
        state.rows = 3;
        state.lines = vec![make_line("line1")];
        state.status_line = make_line(" main.rs ");
        state.status_mode_line = make_line("normal");

        let commands = scene_render(&state);

        // Status bar should produce DrawAtoms or DrawText
        let has_text_commands = commands.iter().any(|c| {
            matches!(
                c,
                DrawCommand::DrawAtoms { .. } | DrawCommand::DrawText { .. }
            )
        });
        assert!(has_text_commands, "expected text commands from status bar");
    }

    #[test]
    fn test_container_produces_border_and_fill() {
        use crate::element::{BorderConfig, BorderLineStyle, Edges, Element, Style};

        let state = default_state();
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
        let theme = Theme::default_theme();
        let commands = scene_paint(
            &el,
            &layout,
            &state,
            &theme,
            cell_size(),
            CursorStyle::Block,
        );

        let has_fill = commands
            .iter()
            .any(|c| matches!(c, DrawCommand::FillRect { .. }));
        let has_border = commands
            .iter()
            .any(|c| matches!(c, DrawCommand::DrawBorder { .. }));
        assert!(has_fill, "container should produce FillRect");
        assert!(has_border, "container should produce DrawBorder");
    }

    #[test]
    fn test_container_with_shadow() {
        use crate::element::{BorderConfig, BorderLineStyle, Edges, Element, Style};

        let state = default_state();
        let el = Element::Container {
            child: Box::new(Element::Empty),
            border: Some(BorderConfig::from(BorderLineStyle::Single)),
            shadow: true,
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
        let theme = Theme::default_theme();
        let commands = scene_paint(
            &el,
            &layout,
            &state,
            &theme,
            cell_size(),
            CursorStyle::Block,
        );

        let has_shadow = commands
            .iter()
            .any(|c| matches!(c, DrawCommand::DrawShadow { .. }));
        assert!(
            has_shadow,
            "container with shadow=true should produce DrawShadow"
        );
    }

    #[test]
    fn test_container_with_title() {
        use crate::element::{BorderConfig, BorderLineStyle, Edges, Element, Style};

        let state = default_state();
        let el = Element::Container {
            child: Box::new(Element::Empty),
            border: Some(BorderConfig::from(BorderLineStyle::Rounded)),
            shadow: false,
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
        let theme = Theme::default_theme();
        let commands = scene_paint(
            &el,
            &layout,
            &state,
            &theme,
            cell_size(),
            CursorStyle::Block,
        );

        let has_title = commands
            .iter()
            .any(|c| matches!(c, DrawCommand::DrawBorderTitle { .. }));
        assert!(
            has_title,
            "container with title should produce DrawBorderTitle"
        );
    }

    #[test]
    fn test_scrollable_produces_clips() {
        use crate::element::{Direction, Element};

        let state = default_state();
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
        let theme = Theme::default_theme();
        let commands = scene_paint(
            &el,
            &layout,
            &state,
            &theme,
            cell_size(),
            CursorStyle::Block,
        );

        let clip_count = commands
            .iter()
            .filter(|c| matches!(c, DrawCommand::PushClip(_)))
            .count();
        let pop_count = commands
            .iter()
            .filter(|c| matches!(c, DrawCommand::PopClip))
            .count();
        assert_eq!(clip_count, 1, "Scrollable should push one clip");
        assert_eq!(pop_count, 1, "Scrollable should pop one clip");
    }

    #[test]
    fn test_pixel_rect_conversion() {
        let rect = Rect {
            x: 2,
            y: 3,
            w: 10,
            h: 5,
        };
        let cs = CellSize {
            width: 8.0,
            height: 16.0,
        };
        let pr = to_pixel_rect(&rect, cs);
        assert_eq!(pr.x, 16.0);
        assert_eq!(pr.y, 48.0);
        assert_eq!(pr.w, 80.0);
        assert_eq!(pr.h, 80.0);
    }

    #[test]
    fn test_resolve_atoms_no_base() {
        let atoms = vec![Atom {
            face: Face::default(),
            contents: "hello".into(),
        }];
        let resolved = resolve_atoms(&atoms, None);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].contents, "hello");
        assert_eq!(resolved[0].face, Face::default());
    }

    #[test]
    fn test_line_display_width_str_basic() {
        assert_eq!(line_display_width_str("hello"), 5);
        assert_eq!(line_display_width_str("abc\ndef"), 6);
        assert_eq!(line_display_width_str("漢字"), 4);
    }
}
