mod cache;

pub use cache::SceneCache;

use unicode_width::UnicodeWidthStr;

use super::theme::Theme;
use crate::element::{BorderLineStyle, Element, ImageFit, ImageSource};
use crate::layout::Rect;
use crate::layout::flex::LayoutResult;
use crate::protocol::{Atom, Face, Style};
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
///
/// ADR-031 Phase A.3.6: only `style` is stored (the Parley-native
/// representation). The `face()` accessor projects to the legacy
/// `Face` for consumers that still expect it.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedAtom {
    pub contents: String,
    pub style: Style,
}

impl ResolvedAtom {
    /// Project this atom's style to the legacy [`Face`] representation.
    /// Bridge for consumers that still consume `Face`; pure projection
    /// (cheap, no allocation).
    #[inline]
    pub fn face(&self) -> Face {
        self.style.to_face()
    }
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
    /// Base style for the line (used for background fill).
    /// ADR-031 Phase A.3: migrated from `Face`.
    pub base_face: Style,
    /// Semantic annotations (cursors, etc.).
    pub annotations: Vec<ParagraphAnnotation>,
    /// Inline-box slots reserved within the line — passed to Parley's
    /// `push_inline_box` so the layout engine accounts for the declared
    /// geometry. `byte_offset` is in `atoms` (post-decoration) coords.
    /// ADR-031 Phase 10 Step 2-renderer C.
    pub inline_box_slots: Vec<crate::render::inline_decoration::InlineBoxSlotMeta>,
    /// Pre-painted draw commands for each inline-box slot — origin (0,0),
    /// laid out at the slot's declared cell geometry. The GPU renderer
    /// translates each command's position by the Parley-reported box rect
    /// and appends them to the scene at paint time.
    ///
    /// Length matches `inline_box_slots`. An empty inner `Vec` indicates
    /// the owning plugin returned `None` from `paint_inline_box`; the
    /// renderer falls back to a placeholder fill in that case.
    /// ADR-031 Phase 10 Step 2-renderer (Step A.2b).
    pub inline_box_paint_commands: Vec<Vec<DrawCommand>>,
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
        /// ADR-031 Phase A.3: Style.
        face: Style,
        elevated: bool,
    },

    /// Draw a sequence of atoms (buffer lines, status line, menu items).
    DrawAtoms {
        pos: PixelPos,
        atoms: Vec<ResolvedAtom>,
        max_width: f32,
        /// Stable identity used by the GPU renderer's line-shaping cache.
        ///
        /// The exact numeric value carries no semantic meaning beyond being
        /// stable across frames for a given line: callers should reserve
        /// distinct values for distinct logical lines. `u32::MAX` opts out
        /// of caching (the renderer reshapes unconditionally).
        line_idx: u32,
    },

    /// Draw plain text (Element::Text).
    DrawText {
        pos: PixelPos,
        text: String,
        /// ADR-031 Phase A.3: Style.
        face: Style,
        max_width: f32,
    },

    /// Draw a pixel-based border.
    DrawBorder {
        rect: PixelRect,
        line_style: BorderLineStyle,
        /// ADR-031 Phase A.3: Style.
        face: Style,
        /// Optional interior fill (background inside the border).
        fill_face: Option<Style>,
    },

    /// Draw a border title.
    DrawBorderTitle {
        rect: PixelRect,
        title: Vec<ResolvedAtom>,
        /// ADR-031 Phase A.3: Style.
        border_face: Style,
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
        /// ADR-031 Phase A.3: Style.
        face: Style,
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
        /// Stable identity used by the GPU renderer's line-shaping cache.
        /// See [`DrawCommand::DrawAtoms::line_idx`].
        line_idx: u32,
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

/// Translate every position-bearing draw command in `cmds` by `(dx, dy)`.
///
/// Used to relocate a sub-tree of pre-painted DrawCommands (e.g. inline-box
/// content rendered at origin (0, 0)) to its final on-screen position once
/// the host knows the rect from the layout engine. Commands without a
/// position field (`PushClip` clips, `PopClip`, `BeginOverlay`) pass
/// through unchanged.
///
/// ADR-031 Phase 10 Step 2-renderer (Step A.2b).
pub fn translate_draw_commands(cmds: &mut [DrawCommand], dx: f32, dy: f32) {
    if dx == 0.0 && dy == 0.0 {
        return;
    }
    for cmd in cmds {
        match cmd {
            DrawCommand::FillRect { rect, .. }
            | DrawCommand::DrawBorder { rect, .. }
            | DrawCommand::DrawBorderTitle { rect, .. }
            | DrawCommand::DrawShadow { rect, .. }
            | DrawCommand::DrawImage { rect, .. }
            | DrawCommand::DrawCanvas { rect, .. }
            | DrawCommand::PushClip(rect) => {
                rect.x += dx;
                rect.y += dy;
            }
            DrawCommand::DrawAtoms { pos, .. }
            | DrawCommand::DrawText { pos, .. }
            | DrawCommand::DrawPaddingRow { pos, .. }
            | DrawCommand::RenderParagraph { pos, .. } => {
                pos.x += dx;
                pos.y += dy;
            }
            DrawCommand::PopClip | DrawCommand::BeginOverlay => {}
        }
    }
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

/// Resolve atom styles against an optional base style.
///
/// Operates entirely in `UnresolvedStyle` / `Style` space — no `Face`
/// round-trip, no per-atom bitflag conversion. Callers that hold a
/// `Face` should convert it once at the call boundary
/// (`Style::from_face(face)`) rather than passing the `Face` and
/// forcing per-atom conversions inside the loop.
pub(crate) fn resolve_atoms(atoms: &[Atom], base_style: Option<&Style>) -> Vec<ResolvedAtom> {
    let default_base = Style::default();
    let base = base_style.unwrap_or(&default_base);
    atoms
        .iter()
        .map(|atom| ResolvedAtom {
            contents: atom.contents.to_string(),
            style: super::super::protocol::resolve_style(&atom.style, base),
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
        use crate::element::{BorderConfig, BorderLineStyle, Edges, Element, ElementStyle};

        let state = default_state();
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
        use crate::element::{BorderConfig, BorderLineStyle, Edges, Element, ElementStyle};

        let state = default_state();
        let el = Element::Container {
            child: Box::new(Element::Empty),
            border: Some(BorderConfig::from(BorderLineStyle::Single)),
            shadow: true,
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
        use crate::element::{BorderConfig, BorderLineStyle, Edges, Element, ElementStyle};

        let state = default_state();
        let el = Element::Container {
            child: Box::new(Element::Empty),
            border: Some(BorderConfig::from(BorderLineStyle::Rounded)),
            shadow: false,
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
        let atoms = vec![Atom::from_face(Face::default(), "hello")];
        let resolved = resolve_atoms(&atoms, None);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].contents, "hello");
        assert_eq!(resolved[0].face(), Face::default());
    }

    #[test]
    fn test_line_display_width_str_basic() {
        assert_eq!(line_display_width_str("hello"), 5);
        assert_eq!(line_display_width_str("abc\ndef"), 6);
        assert_eq!(line_display_width_str("漢字"), 4);
    }
}
