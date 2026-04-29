//! GridPaintVisitor — TUI backend PaintVisitor implementation.
//!
//! Writes to a CellGrid for terminal rendering.

use std::ops::Range;

use super::grid::CellGrid;
use super::paint::{
    BufferPaintContext, paint_border, paint_border_title, paint_buffer_ref, paint_shadow,
    paint_text,
};
use super::theme::Theme;
use super::walk::{ContainerPaintInfo, PaintVisitor};
use crate::display::DisplayMap;
use crate::element::{BufferRefState, ImageFit, ImageSource, StyleToken};
use crate::layout::Rect;
use crate::protocol::{Atom, Face};
use crate::state::AppState;

/// PaintVisitor that writes to a CellGrid (TUI rendering).
pub(crate) struct GridPaintVisitor<'a> {
    grid: &'a mut CellGrid,
    theme: &'a Theme,
    #[cfg_attr(not(feature = "tui-image"), allow(dead_code))]
    halfblock_cache: Option<&'a mut super::halfblock::HalfblockCache>,
    image_protocol: super::ImageProtocol,
    image_requests: Option<&'a mut Vec<super::ImageRequest>>,
}

impl<'a> GridPaintVisitor<'a> {
    pub fn new(
        grid: &'a mut CellGrid,
        theme: &'a Theme,
        halfblock_cache: Option<&'a mut super::halfblock::HalfblockCache>,
        image_protocol: super::ImageProtocol,
        image_requests: Option<&'a mut Vec<super::ImageRequest>>,
    ) -> Self {
        Self {
            grid,
            theme,
            halfblock_cache,
            image_protocol,
            image_requests,
        }
    }
}

impl PaintVisitor for GridPaintVisitor<'_> {
    fn visit_text(&mut self, text: &str, face: &Face, area: Rect) {
        paint_text(self.grid, &area, text, face);
    }

    fn visit_image(&mut self, source: &ImageSource, _fit: ImageFit, _opacity: f32, area: Rect) {
        // Kitty Graphics Protocol: collect image requests for the backend,
        // clear the grid region so CellGrid diff doesn't interfere.
        if self.image_protocol != super::ImageProtocol::Off {
            if let Some(ref mut reqs) = self.image_requests {
                reqs.push(super::ImageRequest {
                    source: source.clone(),
                    fit: _fit,
                    opacity: _opacity,
                    area,
                });
            }
            self.grid
                .clear_region(&area, &crate::protocol::Face::default());
            return;
        }

        #[cfg(feature = "tui-image")]
        if let Some(cache) = self.halfblock_cache.as_mut()
            && super::halfblock::render_to_grid(self.grid, source, _fit, &area, cache)
        {
            return;
        }
        // Fallback: text placeholder
        super::halfblock::paint_image_fallback(self.grid, source, &area);
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
        inline_decorations: Option<&[Option<crate::render::InlineDecoration>]>,
        virtual_text: Option<&[Option<Vec<Atom>>]>,
    ) {
        paint_buffer_ref(
            self.grid,
            &area,
            line_range,
            state,
            &BufferPaintContext {
                buffer_state,
                line_backgrounds,
                display_map,
                inline_decorations,
                virtual_text,
            },
        );
    }

    fn visit_container_pre(&mut self, info: &ContainerPaintInfo) {
        // Shadow (drawn first, behind the container)
        if info.shadow {
            let shadow_fallback = crate::protocol::Style::from_face(&Face {
                attributes: crate::protocol::Attributes::DIM,
                ..Face::default()
            });
            let shadow_face = self
                .theme
                .resolve(
                    &crate::element::ElementStyle::Token(crate::element::StyleToken::SHADOW),
                    &shadow_fallback,
                )
                .to_face();
            paint_shadow(self.grid, &info.area, &shadow_face);
        }

        // Fill entire container area with face
        self.grid.clear_region(&info.area, &info.face);

        // Split divider glyphs
        if info.is_split_divider {
            if info.area.w == 1 {
                for y in info.area.y..info.area.y + info.area.h {
                    self.grid
                        .put_char(info.area.x, y, info.divider_vertical, &info.face);
                }
            } else {
                for x in info.area.x..info.area.x + info.area.w {
                    self.grid
                        .put_char(x, info.area.y, info.divider_horizontal, &info.face);
                }
            }
        }

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

    fn visit_text_panel(
        &mut self,
        lines: &[Vec<Atom>],
        scroll_offset: usize,
        cursor: Option<(usize, usize)>,
        line_numbers: bool,
        _wrap: bool,
        area: Rect,
    ) {
        let gutter_w = if line_numbers {
            let digits = (lines.len().max(1) as f64).log10().floor() as u16 + 1;
            digits + 1 // +1 for separator space
        } else {
            0
        };
        let content_x = area.x + gutter_w;
        let content_w = area.w.saturating_sub(gutter_w);

        let gutter_face = self
            .theme
            .get_style(&StyleToken::GUTTER_LINE_NUMBER)
            .map(|s| s.to_face())
            .unwrap_or_default();

        for row in 0..area.h {
            let line_idx = scroll_offset + row as usize;
            let y = area.y + row;

            if line_numbers && line_idx < lines.len() {
                let num_str = format!("{:>width$} ", line_idx + 1, width = (gutter_w - 1) as usize);
                paint_text(
                    self.grid,
                    &Rect {
                        x: area.x,
                        y,
                        w: gutter_w,
                        h: 1,
                    },
                    &num_str,
                    &gutter_face,
                );
            }

            if line_idx < lines.len() {
                self.grid
                    .put_line_with_base(y, content_x, &lines[line_idx], content_w, None);
                // Cursor highlight
                if let Some((cl, _cc)) = cursor
                    && cl == line_idx
                {
                    let cursor_face = self
                        .theme
                        .get_style(&StyleToken::TEXT_PANEL_CURSOR)
                        .map(|s| s.to_face())
                        .unwrap_or_default();
                    self.grid.fill_region(y, content_x, content_w, &cursor_face);
                }
            }
        }
    }

    fn visit_stack_overlay_pre(&mut self) {
        // No-op for TUI: overlays just paint over the base content
    }

    fn visit_scrollable_pre(&mut self, _area: Rect) {
        // No-op for TUI: no pixel-level clipping in cell grid
    }

    fn visit_canvas(&mut self, _content: &crate::plugin::canvas::CanvasContent, _area: Rect) {
        // No-op for TUI: canvas ops are GPU-only
    }

    fn visit_scrollable_post(&mut self) {
        // No-op for TUI
    }
}
