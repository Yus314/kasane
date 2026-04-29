use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use std::ops::Range;

use super::grid::CellGrid;
use super::terminal_style::TerminalStyle;
use super::theme::Theme;
use crate::display::{DisplayLine, DisplayMap, SourceMapping, SyntheticContent};
use crate::element::{BorderLineStyle, BufferRefState, Element};
use crate::layout::Rect;
use crate::layout::flex::LayoutResult;
use crate::protocol::{Atom, Face};
use crate::render::InlineDecoration;
use crate::state::AppState;

/// Paint an element tree into a CellGrid using pre-computed layout results.
pub fn paint(element: &Element, layout: &LayoutResult, grid: &mut CellGrid, state: &AppState) {
    crate::perf::perf_span!("paint");
    let theme = &state.config.theme;
    super::walk::walk_paint_grid(
        element,
        layout,
        grid,
        state,
        theme,
        None,
        Default::default(),
        None,
    );
}

/// Paint with an explicit theme for style resolution.
pub fn paint_themed(
    element: &Element,
    layout: &LayoutResult,
    grid: &mut CellGrid,
    state: &AppState,
    theme: &Theme,
) {
    super::walk::walk_paint_grid(
        element,
        layout,
        grid,
        state,
        theme,
        None,
        Default::default(),
        None,
    );
}

pub(crate) fn paint_text(grid: &mut CellGrid, area: &Rect, text: &str, face: &Face) {
    let style = TerminalStyle::from_face(face);
    let mut x = area.x;
    let limit = area.x + area.w;
    for grapheme in text.graphemes(true) {
        if x >= limit {
            break;
        }
        if grapheme.starts_with(|c: char| c.is_control()) {
            continue;
        }
        let w = UnicodeWidthStr::width(grapheme) as u16;
        if w == 0 {
            continue;
        }
        if x + w > limit {
            break;
        }
        grid.put_char(x, area.y, grapheme, &style);
        x += w;
    }
}

// ---------------------------------------------------------------------------
// BufferRefParams + BufferLineAction: shared decision logic for buffer painting
// ---------------------------------------------------------------------------

/// Common parameters resolved from AppState / BufferRefState for buffer painting.
/// Used by both TUI (CellGrid) and GPU (DrawCommand) backends.
pub(crate) struct BufferRefParams<'a> {
    pub lines: &'a [Vec<Atom>],
    pub lines_dirty: &'a [bool],
    pub default_face: Face,
    pub padding_face: Face,
    pub padding_char: &'a str,
}

impl<'a> BufferRefParams<'a> {
    pub fn resolve(state: &'a AppState, buffer_state: Option<&'a BufferRefState>) -> Self {
        Self {
            lines: buffer_state
                .map(|s| &s.lines[..])
                .unwrap_or(&state.observed.lines),
            lines_dirty: buffer_state
                .map(|s| &s.lines_dirty[..])
                .unwrap_or(&state.inference.lines_dirty),
            default_face: buffer_state
                .map(|s| s.default_face)
                .unwrap_or_else(|| state.observed.default_style.to_face()),
            padding_face: buffer_state
                .map(|s| s.padding_face)
                .unwrap_or_else(|| state.observed.padding_style.to_face()),
            padding_char: buffer_state
                .map(|s| s.padding_char.as_str())
                .unwrap_or(&state.config.padding_char),
        }
    }
}

/// Describes *what* to render for a single buffer display line.
/// Backends pattern-match on this to produce backend-specific output.
#[derive(Debug)]
pub(crate) enum BufferLineAction<'a> {
    /// Skip this line (TUI line-dirty optimization or no content to render).
    Skip,
    /// Render synthetic content (fold summary, virtual text).
    Synthetic { atoms: &'a [Atom] },
    /// Render a buffer line with optional per-line background and inline decorations.
    BufferLine {
        /// Buffer line index (for cursor coordinate matching).
        line_idx: usize,
        line: &'a [Atom],
        base_face: Face,
        /// Pre-computed decorated atoms (TUI flavour) if inline decorations
        /// apply. `InlineOp::InlineBox` ops emit `width_cells` placeholder
        /// spaces here so cell-grid backends keep correct column accounting.
        decorated: Option<Vec<Atom>>,
        /// Pre-computed decorated atoms (GPU flavour) — identical to
        /// `decorated` except `InlineOp::InlineBox` placeholders are
        /// stripped (Parley reserves the geometry via
        /// [`StyledLine::with_inline_boxes`] instead).
        ///
        /// `Some` only when the decoration contains at least one InlineBox
        /// op; otherwise `None` and the GPU path falls back to `decorated`.
        /// ADR-031 Phase 10 Step 2-renderer C.
        decorated_for_gpu: Option<Vec<Atom>>,
        /// Inline-box slot metadata for the GPU path. `byte_offset` is in
        /// the `decorated_for_gpu` (or `line`) coordinate system, ready to
        /// be passed to Parley's `push_inline_box`. Empty when no
        /// `InlineOp::InlineBox` ops are present.
        /// ADR-031 Phase 10 Step 2-renderer C.
        inline_box_slots: Vec<crate::render::inline_decoration::InlineBoxSlotMeta>,
        /// EOL virtual text atoms to append after buffer content.
        virtual_text: Option<&'a [Atom]>,
    },
    /// Render a padding row (beyond buffer content).
    Padding {
        /// Background fill face.
        face: Face,
        /// Face for the padding character (fg adjusted if same as bg).
        char_face: Face,
    },
    /// Render editable synthetic content (editable virtual text).
    /// When a shadow cursor is active on this line, `shadow_override` contains
    /// the replacement text and cursor position for rendering.
    EditableSynthetic {
        atoms: &'a [Atom],
        #[allow(dead_code)] // Will be used when shadow override rendering is connected
        shadow_override: Option<ShadowRenderInfo>,
    },
}

/// Rendering information for a shadow cursor override on an editable line.
#[derive(Debug)]
#[allow(dead_code)] // Fields will be used when shadow override rendering is connected
pub(crate) struct ShadowRenderInfo {
    /// Byte range within the synthetic content to replace.
    pub span_byte_range: Range<usize>,
    /// Replacement text (working_text from the shadow cursor).
    pub replacement_text: String,
    /// Cursor position as grapheme offset from span start.
    pub cursor_grapheme_offset: usize,
}

/// Analyze a single display line and return a `BufferLineAction` describing what to render.
///
/// This captures the shared decision logic between TUI and GPU buffer painting:
/// DisplayMap resolution, dirty-line skipping, synthetic detection, inline decoration.
///
/// - `skip_clean`: when `true` (TUI), clean lines return `Skip`. GPU always passes `false`.
pub(crate) fn analyze_buffer_line<'a>(
    params: &'a BufferRefParams<'a>,
    display_line: usize,
    display_map: Option<&'a DisplayMap>,
    line_backgrounds: Option<&[Option<Face>]>,
    inline_decorations: Option<&[Option<InlineDecoration>]>,
    virtual_text: Option<&'a [Option<Vec<Atom>>]>,
    skip_clean: bool,
) -> BufferLineAction<'a> {
    // Step 1: Resolve display line → buffer line via DisplayMap
    let (buffer_line_idx, synthetic): (Option<usize>, Option<&SyntheticContent>) =
        if let Some(dm) = display_map {
            if let Some(entry) = dm.entry(DisplayLine(display_line)) {
                let buf_line = match entry.source() {
                    SourceMapping::BufferLine(l) => Some(l.0),
                    SourceMapping::LineRange(r) => Some(r.start),
                    SourceMapping::Projected { .. } => None,
                };
                (buf_line, entry.synthetic())
            } else {
                // Beyond display map range
                (None, None)
            }
        } else {
            (Some(display_line), None)
        };

    // Step 2: Skip clean lines (TUI optimization).
    // Synthetic lines are always repainted: lines_dirty tracks buffer lines only.
    if skip_clean && synthetic.is_none() {
        let is_dirty = if let Some(dm) = display_map {
            dm.is_display_line_dirty(DisplayLine(display_line), params.lines_dirty)
        } else {
            params
                .lines_dirty
                .get(display_line)
                .copied()
                .unwrap_or(true)
        };
        if !is_dirty {
            return BufferLineAction::Skip;
        }
    }

    // Step 3: Synthetic content (fold summary, virtual text, editable virtual text)
    if let Some(syn) = synthetic {
        // Check if this is an editable entry (Projected source)
        if let Some(dm) = display_map
            && let Some(entry) = dm.entry(DisplayLine(display_line))
            && matches!(entry.source(), SourceMapping::Projected { .. })
        {
            return BufferLineAction::EditableSynthetic {
                atoms: &syn.atoms,
                shadow_override: None, // Caller fills in from ShadowCursor state
            };
        }
        return BufferLineAction::Synthetic { atoms: &syn.atoms };
    }

    // Step 4: No buffer source line
    let line_idx = match buffer_line_idx {
        Some(idx) => idx,
        None => return BufferLineAction::Skip,
    };

    // Step 5: Buffer line or padding
    if let Some(line) = params.lines.get(line_idx) {
        let base_face = line_backgrounds
            .and_then(|bgs| bgs.get(line_idx).copied().flatten())
            .unwrap_or(params.default_face);
        let active_deco = inline_decorations
            .and_then(|ds| ds.get(line_idx))
            .and_then(|d| d.as_ref())
            .filter(|deco| !deco.is_empty());
        let decorated =
            active_deco.map(|deco| crate::render::inline_decoration::apply_inline_ops(line, deco));
        // Compute GPU flavour only when the decoration carries InlineBox
        // ops — otherwise the TUI flavour is byte-identical and `None`
        // signals the GPU path to fall back to `decorated`.
        let (decorated_for_gpu, inline_box_slots) = match active_deco {
            Some(deco) => {
                let has_inline_box = deco.ops().iter().any(|op| {
                    matches!(
                        op,
                        crate::render::inline_decoration::InlineOp::InlineBox { .. }
                    )
                });
                if has_inline_box {
                    let (gpu_atoms, slots) =
                        crate::render::inline_decoration::apply_inline_ops_for_gpu(line, deco);
                    (Some(gpu_atoms), slots)
                } else {
                    (None, Vec::new())
                }
            }
            None => (None, Vec::new()),
        };
        let vt = virtual_text
            .and_then(|vts| vts.get(line_idx))
            .and_then(|v| v.as_deref());
        BufferLineAction::BufferLine {
            line_idx,
            line,
            base_face,
            decorated,
            decorated_for_gpu,
            inline_box_slots,
            virtual_text: vt,
        }
    } else {
        let mut char_face = params.padding_face;
        if char_face.fg == char_face.bg {
            char_face.fg = params.default_face.fg;
        }
        BufferLineAction::Padding {
            face: params.padding_face,
            char_face,
        }
    }
}

/// Decoration overlays applied to a buffer paint pass.
#[derive(Default)]
pub(crate) struct BufferPaintContext<'a> {
    pub buffer_state: Option<&'a BufferRefState>,
    pub line_backgrounds: Option<&'a [Option<Face>]>,
    pub display_map: Option<&'a DisplayMap>,
    pub inline_decorations: Option<&'a [Option<InlineDecoration>]>,
    pub virtual_text: Option<&'a [Option<Vec<Atom>>]>,
}

pub(crate) fn paint_buffer_ref(
    grid: &mut CellGrid,
    area: &Rect,
    line_range: std::ops::Range<usize>,
    state: &AppState,
    ctx: &BufferPaintContext<'_>,
) {
    let params = BufferRefParams::resolve(state, ctx.buffer_state);
    let skip_clean = !params.lines_dirty.is_empty();

    for y_offset in 0..area.h {
        let display_line = line_range.start + y_offset as usize;
        let y = area.y + y_offset;

        match analyze_buffer_line(
            &params,
            display_line,
            ctx.display_map,
            ctx.line_backgrounds,
            ctx.inline_decorations,
            ctx.virtual_text,
            skip_clean,
        ) {
            BufferLineAction::Skip => continue,
            BufferLineAction::Synthetic { atoms }
            | BufferLineAction::EditableSynthetic {
                atoms,
                shadow_override: _,
            } => {
                let fill_face = atoms
                    .first()
                    .map(|a| a.face())
                    .unwrap_or(params.default_face);
                let fill_style = TerminalStyle::from_face(&fill_face);
                grid.fill_region(y, area.x, area.w, &fill_style);
                grid.put_line_with_base(y, area.x, atoms, area.w, None);
            }
            BufferLineAction::BufferLine {
                line,
                base_face,
                decorated,
                virtual_text: vt,
                ..
            } => {
                let base_term_style = TerminalStyle::from_face(&base_face);
                grid.fill_region(y, area.x, area.w, &base_term_style);
                let atoms = decorated.as_deref().unwrap_or(line);
                let base_proto_style = crate::protocol::Style::from_face(&base_face);
                let used =
                    grid.put_line_with_base(y, area.x, atoms, area.w, Some(&base_proto_style));
                // EOL virtual text: append after buffer content
                if let Some(vt_atoms) = vt
                    && used < area.w
                {
                    grid.put_line_with_base(
                        y,
                        area.x + used,
                        vt_atoms,
                        area.w - used,
                        Some(&base_proto_style),
                    );
                }
            }
            BufferLineAction::Padding { face, char_face } => {
                let fill_style = TerminalStyle::from_face(&face);
                let char_style = TerminalStyle::from_face(&char_face);
                grid.fill_region(y, area.x, area.w, &fill_style);
                grid.put_char(area.x, y, params.padding_char, &char_style);
            }
        }
    }
}

pub(crate) fn paint_border(
    grid: &mut CellGrid,
    area: &Rect,
    face: &Face,
    truncated: bool,
    border_style: BorderLineStyle,
) {
    if area.w < 2 || area.h < 2 {
        return;
    }

    // Custom border chars storage (must outlive the match)
    let custom_strs: [String; 6];
    // (top-left, top-right, bottom-left, bottom-right, horizontal, vertical)
    let (tl, tr, bl, br, horiz, vert): (&str, &str, &str, &str, &str, &str) = match border_style {
        BorderLineStyle::Single => ("┌", "┐", "└", "┘", "─", "│"),
        BorderLineStyle::Rounded => ("╭", "╮", "╰", "╯", "─", "│"),
        BorderLineStyle::Double => ("╔", "╗", "╚", "╝", "═", "║"),
        BorderLineStyle::Heavy => ("┏", "┓", "┗", "┛", "━", "┃"),
        BorderLineStyle::Ascii => ("+", "+", "+", "+", "-", "|"),
        BorderLineStyle::Custom(ref chars) => {
            // chars: [TL, T, TR, R, BR, B, BL, L, title-left, title-right, shadow]
            custom_strs = [
                chars[0].to_string(), // TL
                chars[2].to_string(), // TR
                chars[6].to_string(), // BL
                chars[4].to_string(), // BR
                chars[1].to_string(), // T (horizontal)
                chars[7].to_string(), // L (vertical)
            ];
            (
                custom_strs[0].as_str(),
                custom_strs[1].as_str(),
                custom_strs[2].as_str(),
                custom_strs[3].as_str(),
                custom_strs[4].as_str(),
                custom_strs[5].as_str(),
            )
        }
    };

    let x1 = area.x;
    let y1 = area.y;
    let x2 = area.x + area.w - 1;
    let y2 = area.y + area.h - 1;
    let bottom_dash = if truncated {
        match border_style {
            BorderLineStyle::Double => "┄",
            BorderLineStyle::Heavy => "┅",
            BorderLineStyle::Ascii => ".",
            BorderLineStyle::Custom(_) => horiz,
            _ => "┄",
        }
    } else {
        horiz
    };

    let style = TerminalStyle::from_face(face);

    // Corners
    grid.put_char(x1, y1, tl, &style);
    grid.put_char(x2, y1, tr, &style);
    grid.put_char(x1, y2, bl, &style);
    grid.put_char(x2, y2, br, &style);

    // Top and bottom edges
    for x in (x1 + 1)..x2 {
        grid.put_char(x, y1, horiz, &style);
        grid.put_char(x, y2, bottom_dash, &style);
    }

    // Left and right edges
    for y in (y1 + 1)..y2 {
        grid.put_char(x1, y, vert, &style);
        grid.put_char(x2, y, vert, &style);
    }
}

/// Paint title on the top border: ╭─┤title├─╮
pub(crate) fn paint_border_title(
    grid: &mut CellGrid,
    area: &Rect,
    face: &Face,
    title: &[crate::protocol::Atom],
) {
    use crate::layout::line_display_width;
    let title_width = line_display_width(title);
    if title_width == 0 || area.w < 6 {
        return;
    }
    // Max title chars that fit: border_w - 2 corners - 2 min dashes - 2 delimiters (┤├)
    let max_title = ((area.w as usize).saturating_sub(6)).min(title_width) as u16;
    // Total dashes available on top border (excluding corners)
    let total_dashes = (area.w as usize).saturating_sub(2);
    // Dashes consumed by title + delimiters
    let title_slot = max_title as usize + 2; // ┤ + title + ├
    let dash_count = total_dashes.saturating_sub(title_slot);
    let left_dashes = dash_count / 2;
    // Position: corner(1) + left_dashes + ┤
    let tx = area.x + 1 + left_dashes as u16;
    let term_style = TerminalStyle::from_face(face);
    let proto_style = crate::protocol::Style::from_face(face);
    grid.put_char(tx, area.y, "┤", &term_style);
    grid.put_line_with_base(area.y, tx + 1, title, max_title, Some(&proto_style));
    let after = tx + 1 + max_title;
    if after < area.x + area.w - 1 {
        grid.put_char(after, area.y, "├", &term_style);
    }
}

pub(crate) fn paint_shadow(grid: &mut CellGrid, area: &Rect, shadow_face: &Face) {
    let style = TerminalStyle::from_face(shadow_face);
    // Right shadow (1 cell wide)
    let sx = area.x + area.w;
    if sx < grid.width() {
        for y in (area.y + 1)..=(area.y + area.h) {
            if y < grid.height() {
                grid.put_char(sx, y, " ", &style);
            }
        }
    }

    // Bottom shadow (1 cell tall)
    let sy = area.y + area.h;
    if sy < grid.height() {
        for x in (area.x + 1)..=(area.x + area.w) {
            if x < grid.width() {
                grid.put_char(x, sy, " ", &style);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::{
        BorderConfig, BorderLineStyle, Edges, Element, ElementStyle, FlexChild, Overlay,
        OverlayAnchor,
    };
    use crate::layout::flex::place;
    use crate::protocol::{Face, Style};
    use crate::test_utils::*;

    #[test]
    fn test_paint_text() {
        let state = default_state();
        let mut grid = CellGrid::new(20, 5);
        let el = Element::plain_text("hello");
        let area = root_area(20, 5);
        let layout = place(&el, area, &state);
        paint(&el, &layout, &mut grid, &state);
        assert_eq!(grid.get(0, 0).unwrap().grapheme, "h");
        assert_eq!(grid.get(4, 0).unwrap().grapheme, "o");
    }

    #[test]
    fn test_paint_buffer_ref() {
        let mut state = default_state();
        state.observed.lines = vec![make_line("line1"), make_line("line2")];
        state.runtime.cols = 10;
        state.runtime.rows = 4;

        let mut grid = CellGrid::new(10, 4);
        let el = Element::buffer_ref(0..3);
        let area = Rect {
            x: 0,
            y: 0,
            w: 10,
            h: 3,
        };
        let layout = place(&el, area, &state);
        paint(&el, &layout, &mut grid, &state);

        assert_eq!(grid.get(0, 0).unwrap().grapheme, "l"); // "line1"
        assert_eq!(grid.get(0, 1).unwrap().grapheme, "l"); // "line2"
        assert_eq!(grid.get(0, 2).unwrap().grapheme, "~"); // padding
    }

    #[test]
    fn test_paint_flex_column() {
        let state = default_state();
        let mut grid = CellGrid::new(20, 5);
        let el = Element::column(vec![
            FlexChild::fixed(Element::plain_text("aaa")),
            FlexChild::fixed(Element::plain_text("bbb")),
        ]);
        let area = root_area(20, 5);
        let layout = place(&el, area, &state);
        paint(&el, &layout, &mut grid, &state);

        assert_eq!(grid.get(0, 0).unwrap().grapheme, "a");
        assert_eq!(grid.get(0, 1).unwrap().grapheme, "b");
    }

    #[test]
    fn test_paint_container_border() {
        let state = default_state();
        let mut grid = CellGrid::new(20, 10);
        let el = Element::Container {
            child: Box::new(Element::plain_text("hi")),
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
        paint(&el, &layout, &mut grid, &state);

        assert_eq!(grid.get(0, 0).unwrap().grapheme, "╭");
        assert_eq!(grid.get(5, 0).unwrap().grapheme, "╮");
        assert_eq!(grid.get(0, 2).unwrap().grapheme, "╰");
        assert_eq!(grid.get(5, 2).unwrap().grapheme, "╯");
        assert_eq!(grid.get(1, 0).unwrap().grapheme, "─");
        assert_eq!(grid.get(0, 1).unwrap().grapheme, "│");
    }

    #[test]
    fn test_paint_buffer_ref_custom_padding_char() {
        let mut state = default_state();
        state.observed.lines = vec![make_line("line1")];
        state.runtime.cols = 10;
        state.runtime.rows = 4;
        state.config.padding_char = "@".to_string();

        let mut grid = CellGrid::new(10, 4);
        let el = Element::buffer_ref(0..3);
        let area = Rect {
            x: 0,
            y: 0,
            w: 10,
            h: 3,
        };
        let layout = place(&el, area, &state);
        paint(&el, &layout, &mut grid, &state);

        // Row 0 = line1, rows 1-2 = padding with "@"
        assert_eq!(grid.get(0, 0).unwrap().grapheme, "l");
        assert_eq!(grid.get(0, 1).unwrap().grapheme, "@");
        assert_eq!(grid.get(0, 2).unwrap().grapheme, "@");
    }

    #[test]
    fn test_paint_container_border_title() {
        let state = default_state();
        let mut grid = CellGrid::new(20, 10);
        let el = Element::Container {
            child: Box::new(Element::plain_text("content")),
            border: Some(BorderConfig::from(BorderLineStyle::Rounded)),
            shadow: false,
            padding: Edges::ZERO,
            style: ElementStyle::from(Face::default()),
            title: Some(make_line("Hi")),
        };
        let area = Rect {
            x: 0,
            y: 0,
            w: 12,
            h: 3,
        };
        let layout = place(&el, area, &state);
        paint(&el, &layout, &mut grid, &state);

        // Top border: ╭───┤Hi├───╮  (title centered)
        // w=12, total_dashes=10, title_slot=4(┤Hi├), dash_count=6, left=3, right=3
        assert_eq!(grid.get(0, 0).unwrap().grapheme, "╭");
        assert_eq!(grid.get(1, 0).unwrap().grapheme, "─");
        assert_eq!(grid.get(2, 0).unwrap().grapheme, "─");
        assert_eq!(grid.get(3, 0).unwrap().grapheme, "─");
        assert_eq!(grid.get(4, 0).unwrap().grapheme, "┤");
        assert_eq!(grid.get(5, 0).unwrap().grapheme, "H");
        assert_eq!(grid.get(6, 0).unwrap().grapheme, "i");
        assert_eq!(grid.get(7, 0).unwrap().grapheme, "├");
        assert_eq!(grid.get(8, 0).unwrap().grapheme, "─");
        assert_eq!(grid.get(9, 0).unwrap().grapheme, "─");
        assert_eq!(grid.get(10, 0).unwrap().grapheme, "─");
        assert_eq!(grid.get(11, 0).unwrap().grapheme, "╮");
    }

    #[test]
    fn test_paint_grid() {
        let state = default_state();
        let mut grid = CellGrid::new(20, 5);
        let el = Element::Grid {
            columns: vec![
                crate::element::GridColumn::fixed(5),
                crate::element::GridColumn::fixed(5),
            ],
            children: vec![Element::plain_text("hello"), Element::plain_text("world")],
            col_gap: 0,
            row_gap: 0,
            align: crate::element::Align::Start,
            cross_align: crate::element::Align::Start,
        };
        let area = root_area(20, 5);
        let layout = place(&el, area, &state);
        paint(&el, &layout, &mut grid, &state);

        // "hello" at (0,0)
        assert_eq!(grid.get(0, 0).unwrap().grapheme, "h");
        assert_eq!(grid.get(4, 0).unwrap().grapheme, "o");
        // "world" at (5,0)
        assert_eq!(grid.get(5, 0).unwrap().grapheme, "w");
        assert_eq!(grid.get(9, 0).unwrap().grapheme, "d");
    }

    #[test]
    fn test_paint_container_over_wide_chars() {
        let state = default_state();
        let mut grid = CellGrid::new(20, 10);
        // Place wide chars in the grid before painting container
        let wide_style = TerminalStyle::default();
        grid.put_char(1, 0, "漢", &wide_style);
        grid.put_char(3, 0, "字", &wide_style);
        grid.put_char(1, 1, "あ", &wide_style);

        let el = Element::Container {
            child: Box::new(Element::plain_text("hi")),
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
        paint(&el, &layout, &mut grid, &state);

        // Container fill should have replaced wide chars with spaces
        assert_eq!(grid.get(0, 0).unwrap().grapheme, "╭");
        assert_eq!(grid.get(1, 0).unwrap().grapheme, "─");
        assert_eq!(grid.get(0, 1).unwrap().grapheme, "│");
        // Interior: child "hi" is painted, no leftover wide chars
        assert_eq!(grid.get(1, 1).unwrap().grapheme, "h");
        assert_eq!(grid.get(1, 1).unwrap().width, 1);
        assert_eq!(grid.get(2, 1).unwrap().grapheme, "i");
    }

    #[test]
    fn test_paint_stack_overlays() {
        let state = default_state();
        let mut grid = CellGrid::new(20, 10);
        let el = Element::stack(
            Element::plain_text("base_text"),
            vec![Overlay {
                element: Element::plain_text("pop"),
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
        paint(&el, &layout, &mut grid, &state);

        // Base text at (0,0)
        assert_eq!(grid.get(0, 0).unwrap().grapheme, "b");
        // Overlay at (5,3)
        assert_eq!(grid.get(5, 3).unwrap().grapheme, "p");
        assert_eq!(grid.get(6, 3).unwrap().grapheme, "o");
        assert_eq!(grid.get(7, 3).unwrap().grapheme, "p");
    }

    // -----------------------------------------------------------------------
    // analyze_buffer_line unit tests
    // -----------------------------------------------------------------------

    fn make_params<'a>(
        lines: &'a [Vec<crate::protocol::Atom>],
        lines_dirty: &'a [bool],
    ) -> BufferRefParams<'a> {
        BufferRefParams {
            lines,
            lines_dirty,
            default_face: Face::default(),
            padding_face: Face::default(),
            padding_char: "~",
        }
    }

    #[test]
    fn analyze_identity_no_display_map() {
        let lines = vec![make_line("hello"), make_line("world")];
        let params = make_params(&lines, &[]);
        match analyze_buffer_line(&params, 0, None, None, None, None, false) {
            BufferLineAction::BufferLine {
                line_idx,
                base_face,
                decorated,
                ..
            } => {
                assert_eq!(line_idx, 0);
                assert_eq!(base_face, Face::default());
                assert!(decorated.is_none());
            }
            other => panic!("expected BufferLine, got {other:?}"),
        }
    }

    #[test]
    fn analyze_display_map_with_synthetic() {
        use crate::display::{DisplayDirective, DisplayMap};
        let lines = vec![make_line("line0"), make_line("line1"), make_line("line2")];
        let params = make_params(&lines, &[]);
        let syn_face = Face {
            fg: crate::protocol::Color::Rgb { r: 255, g: 0, b: 0 },
            ..Face::default()
        };
        let dm = DisplayMap::build(
            3,
            &[DisplayDirective::Fold {
                range: 0..2,
                summary: vec![Atom::with_style("folded", Style::from_face(&syn_face))],
            }],
        );
        // Display line 0 should be the fold summary (synthetic)
        match analyze_buffer_line(&params, 0, Some(&dm), None, None, None, false) {
            BufferLineAction::Synthetic { atoms } => {
                let text: String = atoms.iter().map(|a| a.contents.as_str()).collect();
                assert_eq!(text, "folded");
                assert_eq!(atoms[0].face(), syn_face);
            }
            other => panic!("expected Synthetic, got {other:?}"),
        }
    }

    #[test]
    fn analyze_display_map_beyond_range() {
        use crate::display::DisplayMap;
        let lines = vec![make_line("only")];
        let params = make_params(&lines, &[]);
        let dm = DisplayMap::build(1, &[]);
        // Display line 5 is beyond the map
        match analyze_buffer_line(&params, 5, Some(&dm), None, None, None, false) {
            BufferLineAction::Skip => {}
            other => panic!("expected Skip for beyond-range, got {other:?}"),
        }
    }

    #[test]
    fn analyze_lines_dirty_skip_when_skip_clean() {
        let lines = vec![make_line("line0"), make_line("line1")];
        let lines_dirty = vec![false, true]; // line 0 clean, line 1 dirty
        let params = make_params(&lines, &lines_dirty);
        // skip_clean=true → clean line should Skip
        match analyze_buffer_line(&params, 0, None, None, None, None, true) {
            BufferLineAction::Skip => {}
            other => panic!("expected Skip for clean line, got {other:?}"),
        }
        // skip_clean=true → dirty line should render
        match analyze_buffer_line(&params, 1, None, None, None, None, true) {
            BufferLineAction::BufferLine { line_idx, .. } => assert_eq!(line_idx, 1),
            other => panic!("expected BufferLine for dirty line, got {other:?}"),
        }
        // skip_clean=false → clean line should still render (GPU mode)
        match analyze_buffer_line(&params, 0, None, None, None, None, false) {
            BufferLineAction::BufferLine { line_idx, .. } => assert_eq!(line_idx, 0),
            other => panic!("expected BufferLine with skip_clean=false, got {other:?}"),
        }
    }

    #[test]
    fn analyze_inline_decoration_applied() {
        use crate::render::InlineDecoration;
        use crate::render::inline_decoration::InlineOp;
        let lines = vec![make_line("hello")];
        let params = make_params(&lines, &[]);
        let deco_face = Face {
            fg: crate::protocol::Color::Rgb { r: 0, g: 255, b: 0 },
            ..Face::default()
        };
        let deco = InlineDecoration::new(vec![InlineOp::Style {
            range: 0..5,
            face: deco_face,
        }]);
        let decos: Vec<Option<InlineDecoration>> = vec![Some(deco)];
        match analyze_buffer_line(&params, 0, None, None, Some(&decos), None, false) {
            BufferLineAction::BufferLine { decorated, .. } => {
                assert!(decorated.is_some(), "expected decorated atoms");
            }
            other => panic!("expected BufferLine with decoration, got {other:?}"),
        }
    }

    #[test]
    fn analyze_padding_row() {
        let lines = vec![make_line("only")];
        let params = make_params(&lines, &[]);
        // Line index 1 is beyond the buffer → padding
        match analyze_buffer_line(&params, 1, None, None, None, None, false) {
            BufferLineAction::Padding { face, char_face } => {
                assert_eq!(face, Face::default());
                // When fg == bg, char_face.fg gets default_face.fg
                assert_eq!(char_face.fg, params.default_face.fg);
            }
            other => panic!("expected Padding, got {other:?}"),
        }
    }

    #[test]
    fn analyze_line_background_override() {
        let lines = vec![make_line("hello")];
        let params = make_params(&lines, &[]);
        let bg_face = Face {
            bg: crate::protocol::Color::Rgb { r: 0, g: 0, b: 128 },
            ..Face::default()
        };
        let bgs: Vec<Option<Face>> = vec![Some(bg_face)];
        match analyze_buffer_line(&params, 0, None, Some(&bgs), None, None, false) {
            BufferLineAction::BufferLine { base_face, .. } => {
                assert_eq!(base_face, bg_face);
            }
            other => panic!("expected BufferLine with bg override, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // Virtual text (EOL) tests
    // -----------------------------------------------------------------------

    #[test]
    fn analyze_virtual_text_attached() {
        let lines = vec![make_line("hello")];
        let params = make_params(&lines, &[]);
        let vt_face = Face {
            fg: crate::protocol::Color::Rgb { r: 255, g: 0, b: 0 },
            ..Face::default()
        };
        let vt: Vec<Option<Vec<Atom>>> = vec![Some(vec![Atom::with_style(
            "  err",
            Style::from_face(&vt_face),
        )])];
        match analyze_buffer_line(&params, 0, None, None, None, Some(&vt), false) {
            BufferLineAction::BufferLine { virtual_text, .. } => {
                let vt_atoms = virtual_text.expect("expected virtual text");
                assert_eq!(vt_atoms.len(), 1);
                assert_eq!(vt_atoms[0].contents.as_str(), "  err");
                assert_eq!(vt_atoms[0].face(), vt_face);
            }
            other => panic!("expected BufferLine with virtual text, got {other:?}"),
        }
    }

    #[test]
    fn analyze_virtual_text_none_when_absent() {
        let lines = vec![make_line("hello")];
        let params = make_params(&lines, &[]);
        // No virtual text at all
        match analyze_buffer_line(&params, 0, None, None, None, None, false) {
            BufferLineAction::BufferLine { virtual_text, .. } => {
                assert!(virtual_text.is_none());
            }
            other => panic!("expected BufferLine without virtual text, got {other:?}"),
        }
    }

    #[test]
    fn analyze_virtual_text_none_for_line_without_vt() {
        let lines = vec![make_line("hello"), make_line("world")];
        let params = make_params(&lines, &[]);
        // Only line 0 has VT, line 1 has None
        let vt: Vec<Option<Vec<Atom>>> = vec![Some(vec![Atom::plain(" hint")]), None];
        match analyze_buffer_line(&params, 1, None, None, None, Some(&vt), false) {
            BufferLineAction::BufferLine { virtual_text, .. } => {
                assert!(virtual_text.is_none(), "line 1 should have no virtual text");
            }
            other => panic!("expected BufferLine, got {other:?}"),
        }
    }

    #[test]
    fn analyze_virtual_text_skipped_for_fold() {
        use crate::display::{DisplayDirective, DisplayMap};
        let lines = vec![make_line("line0"), make_line("line1"), make_line("line2")];
        let params = make_params(&lines, &[]);
        let dm = DisplayMap::build(
            3,
            &[DisplayDirective::Fold {
                range: 0..2,
                summary: vec![Atom::plain("folded")],
            }],
        );
        // VT for buffer lines
        let vt: Vec<Option<Vec<Atom>>> = vec![
            Some(vec![Atom::plain(" vt0")]),
            Some(vec![Atom::plain(" vt1")]),
            None,
        ];
        // Display line 0 = fold summary → Synthetic, no virtual text
        match analyze_buffer_line(&params, 0, Some(&dm), None, None, Some(&vt), false) {
            BufferLineAction::Synthetic { .. } => {} // fold summary, VT not applied
            other => panic!("expected Synthetic for folded line, got {other:?}"),
        }
    }

    #[test]
    fn paint_buffer_ref_with_virtual_text() {
        let mut state = default_state();
        state.observed.lines = vec![make_line("hello")];
        state.runtime.cols = 20;
        state.runtime.rows = 3;

        let vt_face = Face {
            fg: crate::protocol::Color::Rgb { r: 255, g: 0, b: 0 },
            ..Face::default()
        };
        let vt: Vec<Option<Vec<Atom>>> = vec![Some(vec![Atom::with_style(
            "  err",
            Style::from_face(&vt_face),
        )])];

        let mut grid = CellGrid::new(20, 3);
        let area = Rect {
            x: 0,
            y: 0,
            w: 20,
            h: 3,
        };
        paint_buffer_ref(
            &mut grid,
            &area,
            0..3,
            &state,
            &BufferPaintContext {
                virtual_text: Some(&vt),
                ..Default::default()
            },
        );

        // Buffer content "hello" at columns 0-4
        assert_eq!(grid.get(0, 0).unwrap().grapheme, "h");
        assert_eq!(grid.get(4, 0).unwrap().grapheme, "o");
        // Virtual text "  err" at columns 5-9
        assert_eq!(grid.get(5, 0).unwrap().grapheme, " ");
        assert_eq!(grid.get(6, 0).unwrap().grapheme, " ");
        assert_eq!(grid.get(7, 0).unwrap().grapheme, "e");
        assert_eq!(grid.get(8, 0).unwrap().grapheme, "r");
        assert_eq!(grid.get(9, 0).unwrap().grapheme, "r");
    }

    #[test]
    fn paint_buffer_ref_virtual_text_clipped_when_full_width() {
        let mut state = default_state();
        // "hello" is 5 chars, width is 5 → no room for VT
        state.observed.lines = vec![make_line("hello")];
        state.runtime.cols = 5;
        state.runtime.rows = 1;

        let vt: Vec<Option<Vec<Atom>>> = vec![Some(vec![Atom::plain("  err")])];

        let mut grid = CellGrid::new(5, 1);
        let area = Rect {
            x: 0,
            y: 0,
            w: 5,
            h: 1,
        };
        paint_buffer_ref(
            &mut grid,
            &area,
            0..1,
            &state,
            &BufferPaintContext {
                virtual_text: Some(&vt),
                ..Default::default()
            },
        );

        // Buffer content fills the entire width
        assert_eq!(grid.get(0, 0).unwrap().grapheme, "h");
        assert_eq!(grid.get(4, 0).unwrap().grapheme, "o");
        // No VT visible (clipped)
    }

    #[test]
    fn paint_buffer_ref_inline_deco_plus_virtual_text() {
        use crate::render::InlineDecoration;
        use crate::render::inline_decoration::InlineOp;

        let mut state = default_state();
        state.observed.lines = vec![make_line("hello")];
        state.runtime.cols = 20;
        state.runtime.rows = 1;

        let deco_face = Face {
            fg: crate::protocol::Color::Rgb { r: 0, g: 255, b: 0 },
            ..Face::default()
        };
        let deco = InlineDecoration::new(vec![InlineOp::Style {
            range: 0..5,
            face: deco_face,
        }]);
        let decos: Vec<Option<InlineDecoration>> = vec![Some(deco)];

        let vt_face = Face {
            fg: crate::protocol::Color::Rgb { r: 255, g: 0, b: 0 },
            ..Face::default()
        };
        let vt: Vec<Option<Vec<Atom>>> = vec![Some(vec![Atom::with_style(
            " vt",
            Style::from_face(&vt_face),
        )])];

        let mut grid = CellGrid::new(20, 1);
        let area = Rect {
            x: 0,
            y: 0,
            w: 20,
            h: 1,
        };
        paint_buffer_ref(
            &mut grid,
            &area,
            0..1,
            &state,
            &BufferPaintContext {
                inline_decorations: Some(&decos),
                virtual_text: Some(&vt),
                ..Default::default()
            },
        );

        // Decorated content present
        assert_eq!(grid.get(0, 0).unwrap().grapheme, "h");
        assert_eq!(grid.get(0, 0).unwrap().face().fg, deco_face.fg);
        // Virtual text after content
        assert_eq!(grid.get(5, 0).unwrap().grapheme, " ");
        assert_eq!(grid.get(6, 0).unwrap().grapheme, "v");
        assert_eq!(grid.get(7, 0).unwrap().grapheme, "t");
    }

    #[test]
    fn paint_buffer_ref_no_virtual_text_matches_baseline() {
        let mut state = default_state();
        state.observed.lines = vec![make_line("hello"), make_line("world")];
        state.runtime.cols = 10;
        state.runtime.rows = 3;

        // Without VT
        let mut grid_no_vt = CellGrid::new(10, 3);
        let area = Rect {
            x: 0,
            y: 0,
            w: 10,
            h: 3,
        };
        paint_buffer_ref(
            &mut grid_no_vt,
            &area,
            0..3,
            &state,
            &BufferPaintContext::default(),
        );

        // With empty VT (no entries)
        let vt: Vec<Option<Vec<Atom>>> = vec![None, None];
        let mut grid_empty_vt = CellGrid::new(10, 3);
        paint_buffer_ref(
            &mut grid_empty_vt,
            &area,
            0..3,
            &state,
            &BufferPaintContext {
                virtual_text: Some(&vt),
                ..Default::default()
            },
        );

        // Both grids should be identical
        for y in 0..3u16 {
            for x in 0..10u16 {
                assert_eq!(
                    grid_no_vt.get(x, y).unwrap().grapheme,
                    grid_empty_vt.get(x, y).unwrap().grapheme,
                    "mismatch at ({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn analyze_inline_decoration_with_inline_box_populates_gpu_fields() {
        // ADR-031 Phase 10 Step 2-renderer C: when the InlineDecoration
        // contains an InlineBox op, analyze_buffer_line populates BOTH
        // `decorated` (TUI flavour with placeholder spaces) and
        // `decorated_for_gpu` (no placeholders) plus the translated slot
        // list.
        use crate::display::InlineBoxAlignment;
        use crate::plugin::PluginId;
        use crate::render::InlineDecoration;
        use crate::render::inline_decoration::InlineOp;
        let lines = vec![make_line("hello world")];
        let params = make_params(&lines, &[]);
        let deco = InlineDecoration::new(vec![InlineOp::InlineBox {
            at: 6,
            width_cells: 3.0,
            height_lines: 1.0,
            box_id: 0xABCD,
            alignment: InlineBoxAlignment::Center,
            owner: PluginId("color.preview".into()),
        }]);
        let decos: Vec<Option<InlineDecoration>> = vec![Some(deco)];
        match analyze_buffer_line(&params, 0, None, None, Some(&decos), None, false) {
            BufferLineAction::BufferLine {
                decorated,
                decorated_for_gpu,
                inline_box_slots,
                ..
            } => {
                let tui_atoms = decorated.expect("TUI decorated stream");
                let tui_text: String = tui_atoms.iter().map(|a| a.contents.as_str()).collect();
                assert_eq!(
                    tui_text, "hello    world",
                    "3 placeholder spaces inserted at byte 6"
                );

                let gpu_atoms = decorated_for_gpu.expect("GPU decorated stream");
                let gpu_text: String = gpu_atoms.iter().map(|a| a.contents.as_str()).collect();
                assert_eq!(gpu_text, "hello world", "GPU stream has no placeholders");

                assert_eq!(inline_box_slots.len(), 1);
                assert_eq!(inline_box_slots[0].byte_offset, 6);
                assert_eq!(inline_box_slots[0].box_id, 0xABCD);
                assert_eq!(inline_box_slots[0].width_cells, 3.0);
            }
            other => panic!("expected BufferLine with InlineBox decoration, got {other:?}"),
        }
    }
}
