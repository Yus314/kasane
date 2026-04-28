//! ScenePaintVisitor — GPU backend PaintVisitor implementation.
//!
//! Emits `DrawCommand`s for GPU rendering via wgpu+glyphon.

use std::ops::Range;

use super::CursorStyle;
use super::paint::{BufferLineAction, BufferRefParams, analyze_buffer_line};
use super::scene::{
    BufferParagraph, CellSize, DrawCommand, ParagraphAnnotation, PixelPos, PixelRect,
    resolve_atoms, to_pixel_rect,
};
use super::theme::Theme;
use super::walk::{ContainerPaintInfo, PaintVisitor};
use crate::display::DisplayMap;
use crate::element::{BufferRefState, ImageFit, ImageSource, StyleToken};
use crate::layout::Rect;
use crate::protocol::{Atom, Face};
use crate::state::AppState;

/// PaintVisitor that emits `DrawCommand`s (GPU rendering).
pub(crate) struct ScenePaintVisitor<'a> {
    out: &'a mut Vec<DrawCommand>,
    cell_size: CellSize,
    cursor_style: CursorStyle,
    theme: &'a Theme,
    /// Monotonic counter handed out as `line_idx` for non-buffer text emissions
    /// (status bar, menu items, gutter, etc.). Starts at `u32::MAX` and
    /// decrements to keep these IDs disjoint from buffer-line `display_line`
    /// values. Stable across frames as long as visit order is deterministic,
    /// which makes it usable as a shaping-cache key in the GPU renderer.
    line_counter: u32,
}

impl<'a> ScenePaintVisitor<'a> {
    pub fn new(
        out: &'a mut Vec<DrawCommand>,
        cell_size: CellSize,
        cursor_style: CursorStyle,
        theme: &'a Theme,
    ) -> Self {
        Self {
            out,
            cell_size,
            cursor_style,
            theme,
            line_counter: u32::MAX,
        }
    }

    fn next_non_buffer_line_idx(&mut self) -> u32 {
        let id = self.line_counter;
        self.line_counter = self.line_counter.saturating_sub(1);
        id
    }
}

impl PaintVisitor for ScenePaintVisitor<'_> {
    fn visit_image(&mut self, source: &ImageSource, fit: ImageFit, opacity: f32, area: Rect) {
        let pr = to_pixel_rect(&area, self.cell_size);
        self.out.push(DrawCommand::DrawImage {
            rect: pr,
            source: source.clone(),
            fit,
            opacity,
        });
    }

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
        let line_idx = self.next_non_buffer_line_idx();
        self.out.push(DrawCommand::DrawAtoms {
            pos: PixelPos { x: pr.x, y: pr.y },
            atoms: resolved,
            max_width: pr.w,
            line_idx,
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
        inline_decorations: Option<&[Option<crate::render::InlineDecoration>]>,
        virtual_text: Option<&[Option<Vec<Atom>>]>,
    ) {
        let cs = self.cell_size;
        let params = BufferRefParams::resolve(state, buffer_state);

        for y_offset in 0..area.h {
            let display_line = line_range.start + y_offset as usize;
            let py = (area.y + y_offset) as f32 * cs.height;
            let px = area.x as f32 * cs.width;
            let row_w = area.w as f32 * cs.width;

            match analyze_buffer_line(
                &params,
                display_line,
                display_map,
                line_backgrounds,
                inline_decorations,
                virtual_text,
                false, // GPU never skips clean lines
            ) {
                BufferLineAction::Skip => continue,
                BufferLineAction::Synthetic { atoms } => {
                    let fill_face = atoms
                        .first()
                        .map(|a| a.face())
                        .unwrap_or(params.default_face);
                    self.out.push(DrawCommand::FillRect {
                        rect: PixelRect {
                            x: px,
                            y: py,
                            w: row_w,
                            h: cs.height,
                        },
                        face: fill_face,
                        elevated: false,
                    });
                    let resolved = resolve_atoms(atoms, None);
                    self.out.push(DrawCommand::DrawAtoms {
                        pos: PixelPos { x: px, y: py },
                        atoms: resolved,
                        max_width: row_w,
                        line_idx: display_line as u32,
                    });
                }
                BufferLineAction::BufferLine {
                    line_idx,
                    line,
                    base_face,
                    decorated,
                    virtual_text: vt,
                } => {
                    let atoms = decorated.as_deref().unwrap_or(line);
                    let mut resolved = resolve_atoms(atoms, Some(&base_face));

                    // EOL virtual text: append after buffer content
                    if let Some(vt_atoms) = vt {
                        let vt_resolved = resolve_atoms(vt_atoms, Some(&base_face));
                        resolved.extend(vt_resolved);
                    }

                    // Build semantic annotations.
                    // cursor_pos.column and secondary_cursors[].column are display
                    // columns (unicode width), not byte offsets. Convert to byte
                    // offsets so the GPU renderer can match against glyph byte ranges.
                    let mut annotations = Vec::new();
                    if state.inference.cursor_mode == crate::protocol::CursorMode::Buffer
                        && line_idx == state.observed.cursor_pos.line as usize
                        && let Some(bo) = display_col_to_byte_offset(
                            &resolved,
                            state.observed.cursor_pos.column as usize,
                        )
                    {
                        annotations.push(ParagraphAnnotation::PrimaryCursor {
                            byte_offset: bo,
                            style: self.cursor_style,
                        });
                    }
                    for coord in &state.inference.secondary_cursors {
                        if coord.line as usize == line_idx
                            && let Some(bo) =
                                display_col_to_byte_offset(&resolved, coord.column as usize)
                        {
                            annotations.push(ParagraphAnnotation::SecondaryCursor {
                                byte_offset: bo,
                                blend_ratio: state.config.secondary_blend_ratio,
                            });
                        }
                    }

                    self.out.push(DrawCommand::RenderParagraph {
                        pos: PixelPos { x: px, y: py },
                        max_width: row_w,
                        paragraph: BufferParagraph {
                            atoms: resolved,
                            base_face,
                            annotations,
                        },
                        line_idx: display_line as u32,
                    });
                }
                BufferLineAction::Padding { face, char_face } => {
                    self.out.push(DrawCommand::FillRect {
                        rect: PixelRect {
                            x: px,
                            y: py,
                            w: row_w,
                            h: cs.height,
                        },
                        face,
                        elevated: false,
                    });
                    self.out.push(DrawCommand::DrawPaddingRow {
                        pos: PixelPos { x: px, y: py },
                        width: row_w,
                        ch: params.padding_char.to_string(),
                        face: char_face,
                    });
                }
                BufferLineAction::EditableSynthetic { atoms, .. } => {
                    // Render identically to Synthetic in GPU path
                    let fill_face = atoms
                        .first()
                        .map(|a| a.face())
                        .unwrap_or(params.default_face);
                    self.out.push(DrawCommand::FillRect {
                        rect: PixelRect {
                            x: px,
                            y: py,
                            w: row_w,
                            h: cs.height,
                        },
                        face: fill_face,
                        elevated: false,
                    });
                    let resolved = resolve_atoms(atoms, None);
                    self.out.push(DrawCommand::DrawAtoms {
                        pos: PixelPos { x: px, y: py },
                        atoms: resolved,
                        max_width: row_w,
                        line_idx: display_line as u32,
                    });
                }
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

        // Split divider glyphs
        if info.is_split_divider {
            let cs = self.cell_size;
            if info.area.w == 1 {
                for row in 0..info.area.h {
                    self.out.push(DrawCommand::DrawText {
                        pos: PixelPos {
                            x: info.area.x as f32 * cs.width,
                            y: (info.area.y + row) as f32 * cs.height,
                        },
                        text: info.divider_vertical.to_string(),
                        face: info.face,
                        max_width: cs.width,
                    });
                }
            } else {
                self.out.push(DrawCommand::DrawText {
                    pos: PixelPos {
                        x: info.area.x as f32 * cs.width,
                        y: info.area.y as f32 * cs.height,
                    },
                    text: info.divider_horizontal.repeat(info.area.w as usize),
                    face: info.face,
                    max_width: info.area.w as f32 * cs.width,
                });
            }
        }

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

    fn visit_text_panel(
        &mut self,
        lines: &[Vec<Atom>],
        scroll_offset: usize,
        cursor: Option<(usize, usize)>,
        line_numbers: bool,
        _wrap: bool,
        area: Rect,
    ) {
        let cs = self.cell_size;
        let gutter_w = if line_numbers {
            let digits = (lines.len().max(1) as f64).log10().floor() as u16 + 1;
            digits + 1
        } else {
            0
        };
        let content_x = area.x + gutter_w;
        let content_w = area.w.saturating_sub(gutter_w);

        let gutter_face = self
            .theme
            .get(&StyleToken::GUTTER_LINE_NUMBER)
            .copied()
            .unwrap_or_default();

        for row in 0..area.h {
            let line_idx = scroll_offset + row as usize;
            let y = area.y + row;

            if line_numbers && line_idx < lines.len() {
                let num_str = format!("{:>width$} ", line_idx + 1, width = (gutter_w - 1) as usize);
                let gutter_area = Rect {
                    x: area.x,
                    y,
                    w: gutter_w,
                    h: 1,
                };
                let pr = to_pixel_rect(&gutter_area, cs);
                let gutter_atoms = resolve_atoms(&[Atom::from_face(gutter_face, num_str)], None);
                let gutter_line_idx = self.next_non_buffer_line_idx();
                self.out.push(DrawCommand::DrawAtoms {
                    pos: PixelPos { x: pr.x, y: pr.y },
                    atoms: gutter_atoms,
                    max_width: pr.w,
                    line_idx: gutter_line_idx,
                });
            }

            if line_idx < lines.len() {
                let line_area = Rect {
                    x: content_x,
                    y,
                    w: content_w,
                    h: 1,
                };
                let pr = to_pixel_rect(&line_area, cs);
                let resolved = resolve_atoms(&lines[line_idx], None);
                let panel_line_idx = self.next_non_buffer_line_idx();
                self.out.push(DrawCommand::DrawAtoms {
                    pos: PixelPos { x: pr.x, y: pr.y },
                    atoms: resolved,
                    max_width: pr.w,
                    line_idx: panel_line_idx,
                });

                if let Some((cl, _cc)) = cursor
                    && cl == line_idx
                {
                    let cursor_pr = to_pixel_rect(&line_area, cs);
                    let cursor_face = self
                        .theme
                        .get(&StyleToken::TEXT_PANEL_CURSOR)
                        .copied()
                        .unwrap_or_default();
                    self.out.push(DrawCommand::FillRect {
                        rect: cursor_pr,
                        face: cursor_face,
                        elevated: false,
                    });
                }
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

    fn visit_canvas(&mut self, content: &crate::plugin::canvas::CanvasContent, area: Rect) {
        if content.is_empty() {
            return;
        }
        let pr = to_pixel_rect(&area, self.cell_size);
        self.out.push(DrawCommand::DrawCanvas {
            rect: pr,
            content: content.clone(),
        });
    }

    fn visit_scrollable_post(&mut self) {
        self.out.push(DrawCommand::PopClip);
    }
}

/// Convert a display column (unicode width offset) to a byte offset in the
/// concatenated text of resolved atoms.
///
/// Kakoune's `cursor_pos.column` is a display column, not a byte offset.
/// This walks atoms accumulating display widths and returns the byte offset
/// of the first character at or after the target display column.
fn display_col_to_byte_offset(
    atoms: &[super::scene::ResolvedAtom],
    display_col: usize,
) -> Option<usize> {
    use unicode_segmentation::UnicodeSegmentation;
    use unicode_width::UnicodeWidthStr;
    let mut col = 0usize;
    let mut byte = 0usize;
    for atom in atoms {
        for grapheme in atom.contents.graphemes(true) {
            let w = UnicodeWidthStr::width(grapheme);
            if col == display_col || (w > 1 && display_col > col && display_col < col + w) {
                return Some(byte);
            }
            col += w;
            byte += grapheme.len();
        }
    }
    // display_col at or past end → return total byte length
    if display_col >= col {
        return Some(byte);
    }
    None
}
