mod cache;

pub use cache::SceneCache;

use unicode_width::UnicodeWidthStr;

use super::theme::Theme;
use crate::element::{BorderLineStyle, Element, ImageFit, ImageSource};
use crate::layout::Rect;
use crate::layout::flex::LayoutResult;
use crate::protocol::resolve_face;
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

/// Semantic annotation on a buffer paragraph (positions are byte offsets in
/// the original buffer text, resolved to pixel positions by the GPU renderer
/// after shaping).
#[derive(Debug, Clone, PartialEq)]
pub enum ParagraphAnnotation {
    /// Primary cursor at the given byte offset.
    PrimaryCursor {
        byte_offset: usize,
        style: super::CursorStyle,
    },
    /// Secondary (multi-) cursor at the given byte offset.
    SecondaryCursor {
        byte_offset: usize,
        blend_ratio: f32,
    },
}

/// Rendering information for a buffer line, carrying styled atoms and semantic
/// annotations. The GPU renderer shapes the text first, then resolves annotation
/// positions (cursor, selection) from glyph metrics.
#[derive(Debug, Clone, PartialEq)]
pub struct BufferParagraph {
    /// Styled atoms (resolved against base face).
    pub atoms: Vec<ResolvedAtom>,
    /// Base face for the line (used for background fill).
    pub base_face: Face,
    /// Semantic annotations (cursors, etc.).
    pub annotations: Vec<ParagraphAnnotation>,
}

/// GPU draw command produced by `scene_paint`.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawCommand {
    /// Fill a rectangle (background).
    /// When `elevated` is true, the GPU renderer lightens the background color
    /// slightly to make floating popups visually distinct from the editor
    /// background (similar to VS Code's Command Palette).
    FillRect {
        rect: PixelRect,
        face: Face,
        elevated: bool,
    },

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
        /// Whether the parent container is elevated (shadow=true).
        elevated: bool,
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

    /// Draw a raster image in a pixel-coordinate rectangle.
    DrawImage {
        rect: PixelRect,
        source: ImageSource,
        fit: ImageFit,
        opacity: f32,
    },

    /// Render a buffer line paragraph with annotations.
    ///
    /// The GPU renderer shapes text first, then resolves annotation positions
    /// (cursor rectangles, selection highlights) from glyph metrics. This
    /// enables correct rendering with proportional fonts and BiDi text.
    RenderParagraph {
        pos: PixelPos,
        max_width: f32,
        paragraph: BufferParagraph,
    },

    /// Draw plugin canvas operations within a pixel rectangle.
    DrawCanvas {
        rect: PixelRect,
        content: crate::plugin::canvas::CanvasContent,
    },

    /// Layer boundary: all subsequent commands belong to a new overlay layer.
    ///
    /// The renderer must flush (bg → border → text) before starting the new
    /// layer so that overlay backgrounds occlude base-layer text.
    BeginOverlay,
}

// ---------------------------------------------------------------------------
// scene_paint — delegates to walk::walk_paint<ScenePaintVisitor>
// ---------------------------------------------------------------------------

/// Walk the element tree and produce GPU draw commands.
pub fn scene_paint(
    element: &Element,
    layout: &LayoutResult,
    state: &AppState,
    theme: &Theme,
    cell_size: CellSize,
    cursor_style: super::CursorStyle,
) -> Vec<DrawCommand> {
    super::walk::walk_paint_scene(element, layout, state, theme, cell_size, cursor_style)
}

/// Paint a single element subtree into a command buffer.
/// Reusable for painting individual view sections (base, menu overlay, info overlay).
pub fn scene_paint_section(
    element: &Element,
    layout: &LayoutResult,
    state: &AppState,
    theme: &Theme,
    cell_size: CellSize,
    cursor_style: super::CursorStyle,
) -> Vec<DrawCommand> {
    super::walk::walk_paint_scene_section(element, layout, state, theme, cell_size, cursor_style)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert a cell-coordinate Rect to a PixelRect.
pub(crate) fn to_pixel_rect(rect: &Rect, cs: CellSize) -> PixelRect {
    PixelRect {
        x: rect.x as f32 * cs.width,
        y: rect.y as f32 * cs.height,
        w: rect.w as f32 * cs.width,
        h: rect.h as f32 * cs.height,
    }
}

/// Resolve atom faces against an optional base face.
pub(crate) fn resolve_atoms(atoms: &[Atom], base_face: Option<&Face>) -> Vec<ResolvedAtom> {
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
    use crate::plugin::PluginRuntime;
    use crate::protocol::Face;
    use crate::render::CursorStyle;
    use crate::render::view;
    use crate::test_utils::*;

    fn cell_size() -> CellSize {
        CellSize {
            width: 10.0,
            height: 20.0,
        }
    }

    fn scene_render(state: &AppState) -> Vec<DrawCommand> {
        let registry = PluginRuntime::new();
        let element = view::view(state, &registry.view());
        let root = root_area(state.runtime.cols, state.runtime.rows);
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
        state.runtime.cols = 20;
        state.runtime.rows = 5;
        state.observed.lines = vec![make_line("hello"), make_line("world")];
        state.inference.status_line = make_line(" test ");
        state.observed.status_mode_line = make_line("normal");

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
        state.runtime.cols = 20;
        state.runtime.rows = 5;
        state.observed.lines = vec![];
        state.inference.status_line = make_line(" test ");
        state.observed.status_mode_line = make_line("normal");

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
        state.runtime.cols = 20;
        state.runtime.rows = 3;
        state.observed.lines = vec![make_line("line1")];
        state.inference.status_line = make_line(" main.rs ");
        state.observed.status_mode_line = make_line("normal");

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
