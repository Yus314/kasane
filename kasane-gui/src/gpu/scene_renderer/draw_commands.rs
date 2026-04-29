//! Draw command processing methods for [`SceneRenderer`].
//!
//! Handles dispatch of individual [`DrawCommand`] variants to the quad, text,
//! and image pipelines, plus text shaping helpers.

use kasane_core::element::BorderLineStyle;
use kasane_core::protocol::Attributes;
use kasane_core::render::scene::line_display_width_str;
use kasane_core::render::{
    CursorStyle, DrawCommand, PixelRect,
    scene::{BufferParagraph, ParagraphAnnotation},
};

use super::super::quad_pipeline;
use super::super::texture_cache::{LoadState, TextureKey};
use super::super::{CURSOR_BAR_WIDTH, CURSOR_OUTLINE_THICKNESS, CURSOR_UNDERLINE_HEIGHT};
use crate::colors::ColorResolver;

use super::SceneRenderer;

impl SceneRenderer {
    /// Process a single DrawCommand, dispatching to the appropriate pipeline.
    pub(super) fn process_draw_command(
        &mut self,
        cmd: &DrawCommand,
        gpu: &super::super::GpuState,
        color_resolver: &ColorResolver,
        cell_w: f32,
        cell_h: f32,
    ) {
        match cmd {
            DrawCommand::FillRect {
                rect,
                face,
                elevated,
            } => {
                let Some((cx, cy, cw, ch)) = self.clip_rect(rect.x, rect.y, rect.w, rect.h) else {
                    return;
                };
                let (_, mut bg, _) = color_resolver.resolve_face_colors_linear(&face.to_face());

                // When gradient is active, skip fills matching default bg
                // so the gradient shows through.
                if !*elevated && self.should_skip_default_bg(&bg, color_resolver) {
                    return;
                }
                if *elevated {
                    // Subtle elevation: ~10/255 in sRGB ≈ VS Code's floating window offset
                    bg[0] = (bg[0] + 0.003).min(1.0);
                    bg[1] = (bg[1] + 0.003).min(1.0);
                    bg[2] = (bg[2] + 0.003).min(1.0);
                    tracing::debug!(
                        "elevated FillRect: bg=[{:.3},{:.3},{:.3}] rect=({:.0},{:.0},{:.0},{:.0})",
                        bg[0],
                        bg[1],
                        bg[2],
                        cx,
                        cy,
                        cw,
                        ch,
                    );
                }
                self.quad.push_solid(cx, cy, cw, ch, bg);
            }
            DrawCommand::DrawAtoms {
                pos,
                atoms,
                max_width,
                line_idx,
            } => {
                self.process_draw_atoms(pos.x, pos.y, atoms, *max_width, *line_idx, color_resolver);
            }
            DrawCommand::DrawText {
                pos,
                text,
                face,
                max_width,
            } => {
                self.process_draw_text(
                    pos.x,
                    pos.y,
                    text,
                    &face.to_face(),
                    *max_width,
                    color_resolver,
                );
            }
            DrawCommand::DrawPaddingRow {
                pos,
                width: _,
                ch,
                face,
            } => {
                self.parley_emit_text(ch, &face.to_face(), pos.x, pos.y, color_resolver);
            }
            DrawCommand::DrawBorder {
                rect,
                line_style,
                face,
                fill_face,
            } => {
                let (visual_fg, _, _) = color_resolver.resolve_face_colors_linear(&face.to_face());
                let border_color = visual_fg;
                let (corner_radius, border_width) = border_style_params(line_style.clone(), cell_h);
                let fill = match fill_face {
                    Some(ff) => {
                        let (_, ff_bg, _) =
                            color_resolver.resolve_face_colors_linear(&ff.to_face());
                        ff_bg
                    }
                    None => [0.0, 0.0, 0.0, 0.0],
                };
                if *line_style == BorderLineStyle::Double {
                    self.quad.push_rounded_rect(
                        rect.x,
                        rect.y,
                        rect.w,
                        rect.h,
                        corner_radius,
                        border_width,
                        fill,
                        border_color,
                    );
                    let inset = border_width + 1.0;
                    if rect.w > inset * 2.0 && rect.h > inset * 2.0 {
                        self.quad.push_rounded_rect(
                            rect.x + inset,
                            rect.y + inset,
                            rect.w - inset * 2.0,
                            rect.h - inset * 2.0,
                            corner_radius,
                            border_width,
                            fill,
                            border_color,
                        );
                    }
                } else {
                    self.quad.push_rounded_rect(
                        rect.x,
                        rect.y,
                        rect.w,
                        rect.h,
                        corner_radius,
                        border_width,
                        fill,
                        border_color,
                    );
                }
            }
            DrawCommand::DrawBorderTitle {
                rect,
                title,
                border_face,
                elevated,
            } => {
                let title_w: f32 = title
                    .iter()
                    .map(|a| line_display_width_str(&a.contents) as f32 * cell_w)
                    .sum();
                let pad_x = cell_w * 0.5;
                let title_x = rect.x + (rect.w - title_w) / 2.0;
                let title_y = rect.y - cell_h * 0.35;

                let (_, mut title_bg, _) =
                    color_resolver.resolve_face_colors_linear(&border_face.to_face());
                if *elevated {
                    // Subtle elevation: ~10/255 in sRGB ≈ VS Code's floating window offset
                    title_bg[0] = (title_bg[0] + 0.003).min(1.0);
                    title_bg[1] = (title_bg[1] + 0.003).min(1.0);
                    title_bg[2] = (title_bg[2] + 0.003).min(1.0);
                }

                self.quad.push_rounded_rect(
                    title_x - pad_x,
                    title_y,
                    title_w + pad_x * 2.0,
                    cell_h,
                    0.0,
                    0.0,
                    title_bg,
                    [0.0, 0.0, 0.0, 0.0],
                );

                // Border title: use a sentinel line_idx (cache miss every frame).
                // Titles are short and rarely repeated, so caching adds little value.
                self.process_draw_atoms(title_x, title_y, title, title_w, u32::MAX, color_resolver);
            }
            DrawCommand::DrawShadow {
                rect,
                offset,
                blur_radius,
                color,
            } => {
                let expand = *blur_radius;
                self.quad.push_rounded_rect(
                    rect.x + offset.0 - expand,
                    rect.y + offset.1 - expand,
                    rect.w + expand * 2.0,
                    rect.h + expand * 2.0,
                    expand,
                    0.0,
                    crate::colors::srgb_color_to_linear(*color),
                    [0.0, 0.0, 0.0, 0.0],
                );
            }
            DrawCommand::PushClip(rect) => {
                // Intersect with current clip (if any) to handle nested clips
                let new_clip = if let Some(cur) = self.current_clip() {
                    let x1 = rect.x.max(cur.x);
                    let y1 = rect.y.max(cur.y);
                    let x2 = (rect.x + rect.w).min(cur.x + cur.w);
                    let y2 = (rect.y + rect.h).min(cur.y + cur.h);
                    PixelRect {
                        x: x1,
                        y: y1,
                        w: (x2 - x1).max(0.0),
                        h: (y2 - y1).max(0.0),
                    }
                } else {
                    rect.clone()
                };
                self.clip_stack.push(new_clip);
            }
            DrawCommand::PopClip => {
                self.clip_stack.pop();
            }
            DrawCommand::DrawImage {
                rect,
                source,
                fit,
                opacity,
            } => {
                let Some((cx, cy, cw, ch)) = self.clip_rect(rect.x, rect.y, rect.w, rect.h) else {
                    return;
                };
                let key = match source {
                    kasane_core::element::ImageSource::FilePath(path) => {
                        TextureKey::FilePath(path.clone())
                    }
                    kasane_core::element::ImageSource::Rgba {
                        data,
                        width,
                        height,
                    } => {
                        let k = TextureKey::inline_from_data(data, *width, *height);
                        // Ensure inline data is in the cache (synchronous for inline)
                        if !self.texture_cache.insert_rgba(
                            k.clone(),
                            data,
                            *width,
                            *height,
                            &gpu.device,
                            &gpu.queue,
                        ) {
                            return;
                        }
                        k
                    }
                    kasane_core::element::ImageSource::SvgData { data } => {
                        let k = TextureKey::inline_from_svg_data(data);
                        // Rasterize inline SVG synchronously (like RGBA inline data)
                        if self.texture_cache.get_view(&k).is_none() {
                            match kasane_core::render::svg::render_svg_to_rgba_intrinsic(data, 8192)
                            {
                                Ok(r) => {
                                    if !self.texture_cache.insert_rgba(
                                        k.clone(),
                                        &r.data,
                                        r.width,
                                        r.height,
                                        &gpu.device,
                                        &gpu.queue,
                                    ) {
                                        return;
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!("SVG render failed: {e}");
                                    self.quad.push_solid(
                                        cx,
                                        cy,
                                        cw,
                                        ch,
                                        crate::colors::srgb_color_to_linear([0.2, 0.2, 0.2, 1.0]),
                                    );
                                    return;
                                }
                            }
                        }
                        k
                    }
                };
                // Look up or dispatch async load
                match self.texture_cache.get_or_load(&key, &self.event_proxy) {
                    LoadState::Ready(tex_w, tex_h) => {
                        let bind_group = self.texture_cache.get_bind_group(&key).unwrap().clone();
                        self.image.push_textured_quad(
                            bind_group,
                            tex_w as f32,
                            tex_h as f32,
                            cx,
                            cy,
                            cw,
                            ch,
                            *fit,
                            *opacity,
                        );
                    }
                    LoadState::Pending => {
                        // Loading in progress — draw semi-transparent placeholder
                        self.quad.push_solid(
                            cx,
                            cy,
                            cw,
                            ch,
                            crate::colors::srgb_color_to_linear([0.15, 0.15, 0.15, 0.6]),
                        );
                    }
                    LoadState::Failed => {
                        // Failed — draw grey placeholder
                        self.quad.push_solid(
                            cx,
                            cy,
                            cw,
                            ch,
                            crate::colors::srgb_color_to_linear([0.2, 0.2, 0.2, 1.0]),
                        );
                    }
                }
            }
            DrawCommand::DrawCanvas { rect, content } => {
                // Convert canvas ops to quad pipeline instances.
                // Canvas coordinates are relative to rect origin.
                for op in &content.ops {
                    match op {
                        kasane_core::plugin::canvas::CanvasDrawOp::FillRect {
                            x,
                            y,
                            w,
                            h,
                            color,
                        } => {
                            let c = color_resolver.resolve_linear(*color, false);
                            self.quad.push_solid(rect.x + x, rect.y + y, *w, *h, c);
                        }
                        kasane_core::plugin::canvas::CanvasDrawOp::RoundedRect {
                            x,
                            y,
                            w,
                            h,
                            corner_radius,
                            border_width,
                            fill_color,
                            border_color,
                        } => {
                            let fill = color_resolver.resolve_linear(*fill_color, false);
                            let border = color_resolver.resolve_linear(*border_color, true);
                            self.quad.push_rounded_rect(
                                rect.x + x,
                                rect.y + y,
                                *w,
                                *h,
                                *corner_radius,
                                *border_width,
                                fill,
                                border,
                            );
                        }
                        kasane_core::plugin::canvas::CanvasDrawOp::Line {
                            x1,
                            y1,
                            x2,
                            y2,
                            color,
                            width,
                        } => {
                            // Approximate line as a thin solid rect
                            let c = color_resolver.resolve_linear(*color, true);
                            let dx = x2 - x1;
                            let dy = y2 - y1;
                            let len = (dx * dx + dy * dy).sqrt();
                            if len > 0.0 {
                                // For simplicity, draw horizontal/vertical lines as rects
                                let min_x = x1.min(*x2);
                                let min_y = y1.min(*y2);
                                let w = dx.abs().max(*width);
                                let h = dy.abs().max(*width);
                                self.quad
                                    .push_solid(rect.x + min_x, rect.y + min_y, w, h, c);
                            }
                        }
                        kasane_core::plugin::canvas::CanvasDrawOp::Text {
                            x,
                            y,
                            text,
                            color,
                            ..
                        } => {
                            let fg = color_resolver.resolve_linear(*color, true);
                            let face = kasane_core::protocol::Face {
                                fg: *color,
                                ..Default::default()
                            };
                            self.process_draw_text(
                                rect.x + x,
                                rect.y + y,
                                text,
                                &face,
                                rect.w,
                                color_resolver,
                            );
                            let _ = fg; // text rendering uses face-based color
                        }
                        kasane_core::plugin::canvas::CanvasDrawOp::Circle {
                            cx,
                            cy,
                            radius,
                            fill_color,
                            stroke_color,
                            stroke_width,
                        } => {
                            // Approximate circle as a rounded rect with radius = half-extent
                            let fill = fill_color
                                .map(|c| color_resolver.resolve_linear(c, false))
                                .unwrap_or([0.0, 0.0, 0.0, 0.0]);
                            let border = stroke_color
                                .map(|c| color_resolver.resolve_linear(c, true))
                                .unwrap_or([0.0, 0.0, 0.0, 0.0]);
                            self.quad.push_rounded_rect(
                                rect.x + cx - radius,
                                rect.y + cy - radius,
                                radius * 2.0,
                                radius * 2.0,
                                *radius,
                                *stroke_width,
                                fill,
                                border,
                            );
                        }
                    }
                }
            }
            DrawCommand::RenderParagraph {
                pos,
                max_width,
                paragraph,
                line_idx,
            } => {
                self.process_render_paragraph(
                    pos.x,
                    pos.y,
                    *max_width,
                    paragraph,
                    *line_idx,
                    color_resolver,
                );
                // ADR-031 Phase 10 Step 2-renderer (Step A.2b): drain the
                // inline-box deferred queue accumulated during paragraph
                // painting. Each entry is an already-translated copy of
                // the slot's plugin paint content; recursing through
                // process_draw_command lets the same dispatch table apply
                // (so plugin content can include FillRect, DrawText, etc.).
                if !self.deferred_inline_box_cmds.is_empty() {
                    let drained = std::mem::take(&mut self.deferred_inline_box_cmds);
                    for sub in &drained {
                        self.process_draw_command(sub, gpu, color_resolver, cell_w, cell_h);
                    }
                }
            }
            DrawCommand::BeginOverlay => {} // handled by layer splitting
        }
    }

    /// Render the cursor into the bg pipeline.
    ///
    /// When `paragraph_cursor` is set (from RenderParagraph shaping),
    /// the glyph-accurate x position and width are used instead of the
    /// cell-based values. This ensures correct cursor placement and size
    /// with proportional fonts, CJK characters, and RTL text.
    pub(super) fn render_cursor(
        &mut self,
        cursor: Option<(f32, f32, f32, CursorStyle, kasane_core::protocol::Color)>,
        color_resolver: &ColorResolver,
        cell_w: f32,
        cell_h: f32,
    ) {
        let Some((cell_x, y, opacity, style, cursor_color)) = cursor else {
            return;
        };
        let (x, w) = self
            .paragraph_cursor
            .map(|(gx, gw)| (gx, gw.max(CURSOR_BAR_WIDTH)))
            .unwrap_or((cell_x, cell_w));
        let mut cc = color_resolver.resolve_linear(cursor_color, true);
        cc[3] = opacity;
        match style {
            CursorStyle::Block => {
                self.quad.push_solid(x, y, w, cell_h, cc);
            }
            CursorStyle::Bar => {
                self.quad.push_solid(x, y, CURSOR_BAR_WIDTH, cell_h, cc);
            }
            CursorStyle::Underline => {
                self.quad.push_solid(
                    x,
                    y + cell_h - CURSOR_UNDERLINE_HEIGHT,
                    w,
                    CURSOR_UNDERLINE_HEIGHT,
                    cc,
                );
            }
            CursorStyle::Outline => {
                let t = CURSOR_OUTLINE_THICKNESS;
                self.quad.push_solid(x, y, w, t, cc);
                self.quad.push_solid(x, y + cell_h - t, w, t, cc);
                self.quad.push_solid(x, y, t, cell_h, cc);
                self.quad.push_solid(x + w - t, y, t, cell_h, cc);
            }
        }
    }

    /// Emit decoration instances for a face's text attributes.
    fn emit_decorations(
        &mut self,
        x: f32,
        py: f32,
        w: f32,
        face: &kasane_core::protocol::Face,
        fg: [f32; 4],
        color_resolver: &ColorResolver,
    ) {
        let attrs = face.attributes;
        if !attrs.intersects(
            Attributes::UNDERLINE
                | Attributes::CURLY_UNDERLINE
                | Attributes::DOUBLE_UNDERLINE
                | Attributes::DOTTED_UNDERLINE
                | Attributes::DASHED_UNDERLINE
                | Attributes::STRIKETHROUGH,
        ) {
            return;
        }

        let baseline = self.metrics.baseline;
        let cell_h = self.metrics.cell_height;
        // Phase 10 — prefer the font's own underline geometry when the
        // metrics layer captured it (Parley path). Falls back to the
        // historical `cell_h × ratio` heuristic when zero (cosmic path).
        let ul_thickness = if self.metrics.underline_thickness > 0.0 {
            self.metrics.underline_thickness
        } else {
            (cell_h * 0.06).max(1.0)
        };
        // Parley reports underline_offset as the distance from the
        // baseline to the *top* of the underline; positive = below.
        let ul_top_below_baseline = if self.metrics.underline_offset > 0.0 {
            self.metrics.underline_offset
        } else {
            ul_thickness
        };

        // Underline color: use face.underline if set, otherwise fallback to fg
        let ul_color = if face.underline != kasane_core::protocol::Color::Default {
            color_resolver.resolve_linear(face.underline, true)
        } else {
            fg
        };

        let ul_y = py + baseline + ul_top_below_baseline;

        if attrs.contains(Attributes::UNDERLINE) {
            self.quad.push_decoration(
                x,
                ul_y,
                w,
                ul_thickness,
                ul_color,
                quad_pipeline::DECO_SOLID,
            );
        }
        if attrs.contains(Attributes::CURLY_UNDERLINE) {
            // Curly needs more height for the wave amplitude. Anchor the
            // wave's mid-line on the underline's top so the visual
            // weight stays close to where a solid underline would sit.
            let wave_h = (cell_h * 0.2).max(4.0);
            let y = ul_y - wave_h * 0.25;
            self.quad
                .push_decoration(x, y, w, wave_h, ul_color, quad_pipeline::DECO_CURLY);
        }
        if attrs.contains(Attributes::DOUBLE_UNDERLINE) {
            let double_h = (cell_h * 0.15).max(4.0);
            let y = ul_y - double_h * 0.1;
            self.quad
                .push_decoration(x, y, w, double_h, ul_color, quad_pipeline::DECO_DOUBLE);
        }
        if attrs.contains(Attributes::DOTTED_UNDERLINE) {
            let dot_h = (cell_h * 0.15).max(4.0);
            let y = ul_y - dot_h * 0.1;
            self.quad
                .push_decoration(x, y, w, dot_h, ul_color, quad_pipeline::DECO_DOTTED);
        }
        if attrs.contains(Attributes::DASHED_UNDERLINE) {
            let dash_h = (cell_h * 0.08).max(2.0);
            self.quad
                .push_decoration(x, ul_y, w, dash_h, ul_color, quad_pipeline::DECO_DASHED);
        }
        if attrs.contains(Attributes::STRIKETHROUGH) {
            let st_thickness = if self.metrics.strikethrough_thickness > 0.0 {
                self.metrics.strikethrough_thickness
            } else {
                ul_thickness
            };
            // Parley strikethrough_offset is positive *above* the
            // baseline (font convention). The historical fallback uses
            // ~55% of the baseline height as a stand-in.
            let st_top_above_baseline = if self.metrics.strikethrough_offset > 0.0 {
                self.metrics.strikethrough_offset
            } else {
                baseline * 0.45
            };
            let y = py + baseline - st_top_above_baseline;
            self.quad
                .push_decoration(x, y, w, st_thickness, fg, quad_pipeline::DECO_SOLID);
        }
    }

    /// Process DrawAtoms: shaping-first approach for proportional font support.
    ///
    /// Adjacent atoms with the same foreground color are merged into a single
    /// shaping span so that ligatures (e.g. `->`, `!=`, `=>`) can form across
    /// atom boundaries. Background rectangles and decorations are computed from
    /// glyph metrics after shaping, so they adapt to proportional fonts.
    fn process_draw_atoms(
        &mut self,
        px: f32,
        py: f32,
        atoms: &[kasane_core::render::ResolvedAtom],
        max_width: f32,
        _line_idx: u32,
        color_resolver: &ColorResolver,
    ) {
        // Phase 11 — Parley is the only backend; the `line_idx`
        // parameter was used by the cosmic line shaping cache and is
        // no longer needed. Kept on the signature to avoid touching
        // the DrawCommand match arms in this commit.
        self.process_draw_atoms_parley(px, py, atoms, max_width, color_resolver);
    }

    /// ADR-031 Phase 9b Step 4e — Parley alternative to process_draw_atoms.
    /// Runs the per-atom backgrounds + decorations using cell-grid
    /// estimates (no glyph extents from cosmic), then routes glyph
    /// rendering through Parley.
    fn process_draw_atoms_parley(
        &mut self,
        px: f32,
        py: f32,
        atoms: &[kasane_core::render::ResolvedAtom],
        max_width: f32,
        color_resolver: &ColorResolver,
    ) {
        let cell_h = self.metrics.cell_height;
        let cell_w = self.metrics.cell_width;
        let mut x = px;

        // Per-atom backgrounds + decorations using cell-grid estimates.
        // Glyph-accurate widths from Parley are deferred (would require
        // a second pass collecting parley::Layout extents).
        for atom in atoms {
            let atom_display_w = line_display_width_str(&atom.contents) as f32 * cell_w;
            let remaining = max_width - (x - px);
            if remaining <= 0.0 {
                break;
            }
            let actual_w = atom_display_w.min(remaining);
            let (visual_fg, visual_bg, needs_bg) =
                color_resolver.resolve_face_colors_linear(&atom.face());

            if actual_w > 0.0
                && needs_bg
                && !self.should_skip_default_bg(&visual_bg, color_resolver)
            {
                self.quad.push_solid(x, py, actual_w, cell_h, visual_bg);
            }
            if actual_w > 0.0 {
                self.emit_decorations(x, py, actual_w, &atom.face(), visual_fg, color_resolver);
            }
            x += actual_w;
        }

        // Route glyph rendering through Parley.
        self.parley_emit_atoms(atoms, px, py, color_resolver);
    }

    /// Render a buffer paragraph through Parley with **whole-line
    /// shaping**: builds one `StyledLine` from all atoms, shapes it
    /// (with L1 cache reuse on cursor-only frames), and uses the
    /// resulting `ParleyLayout` for both glyph emission and
    /// glyph-accurate per-atom backgrounds + cursor placement.
    ///
    /// This replaces the earlier per-atom snap path. Per-atom shaping
    /// snapped each atom to cell-grid x positions; the resulting
    /// emit_decorations bg widths and `cell_grid_cursor` lookups
    /// matched the snap, but cursors landed at cell boundaries instead
    /// of true glyph edges (visible on CJK / ligature boundaries).
    /// Whole-line shaping fixes that — bg widths come from glyph
    /// extents, cursors come from `parley::Cluster::visual_offset`.
    fn process_render_paragraph_parley(
        &mut self,
        px: f32,
        py: f32,
        max_width: f32,
        para: &BufferParagraph,
        line_idx: u32,
        color_resolver: &ColorResolver,
    ) {
        use super::super::parley_text::Brush as PBrush;
        use super::super::parley_text::frame_builder::DrawableGlyph;
        use super::super::parley_text::glyph_rasterizer::SubpixelX;
        use super::super::parley_text::hit_test::byte_to_advance;
        use super::super::parley_text::styled_line::StyledLine;
        use kasane_core::protocol::{Atom, Style};
        use parley::PositionedLayoutItem;

        let cell_h = self.metrics.cell_height;
        let cell_w = self.metrics.cell_width;

        // 1. Line-wide background fill.
        let (base_visual_fg, base_bg, _) =
            color_resolver.resolve_face_colors_linear(&para.base_face.to_face());
        if !self.should_skip_default_bg(&base_bg, color_resolver) {
            self.quad.push_solid(px, py, max_width, cell_h, base_bg);
        }

        if para.atoms.is_empty() {
            return;
        }

        // 2. Locate the atom under the primary cursor (its face is
        // stripped so render_cursor() owns the visual cursor block).
        let mut clear_cursor_atom_idx: Option<usize> = None;
        for ann in &para.annotations {
            if let ParagraphAnnotation::PrimaryCursor { byte_offset, .. } = ann {
                let mut accum = 0usize;
                for (i, atom) in para.atoms.iter().enumerate() {
                    let atom_end = accum + atom.contents.len();
                    if *byte_offset >= accum && *byte_offset < atom_end {
                        clear_cursor_atom_idx = Some(i);
                        break;
                    }
                    accum = atom_end;
                }
            }
        }

        // 3. Build a StyledLine from all atoms (face-stripped if needed)
        // and shape it once via Parley. The L1 LayoutCache returns the
        // same `Arc<ParleyLayout>` on cursor-only frames where the line
        // text + style + width + size are unchanged.
        let kasane_atoms: Vec<Atom> = para
            .atoms
            .iter()
            .enumerate()
            .map(|(i, atom)| {
                let face = if clear_cursor_atom_idx == Some(i) {
                    para.base_face.to_face()
                } else {
                    atom.face()
                };
                Atom::with_style(atom.contents.clone(), Style::from_face(&face))
            })
            .collect();
        let fallback_brush = PBrush::rgba(
            (base_visual_fg[0].clamp(0.0, 1.0) * 255.0).round() as u8,
            (base_visual_fg[1].clamp(0.0, 1.0) * 255.0).round() as u8,
            (base_visual_fg[2].clamp(0.0, 1.0) * 255.0).round() as u8,
            (base_visual_fg[3].clamp(0.0, 1.0) * 255.0).round() as u8,
        );
        let mut line = StyledLine::from_atoms(
            &kasane_atoms,
            &Style::default(),
            fallback_brush,
            self.font_size,
            None,
        );
        // ADR-031 Phase 10 Step 2-renderer C: thread inline-box slot
        // metadata into the StyledLine so Parley reserves the declared
        // geometry via push_inline_box. Cells → physical pixels via
        // current cell metrics. The L1 LayoutCache content_hash is
        // recomputed inside `with_inline_boxes` so a slot change correctly
        // invalidates a cached layout.
        if !para.inline_box_slots.is_empty() {
            use super::super::parley_text::styled_line::InlineBoxSlot;
            let cw = cell_w;
            let ch = cell_h;
            let slots: Vec<InlineBoxSlot> = para
                .inline_box_slots
                .iter()
                .map(|m| InlineBoxSlot {
                    id: m.box_id,
                    byte_offset: m.byte_offset as u32,
                    width: m.width_cells * cw,
                    height: m.height_lines * ch,
                })
                .collect();
            line = line.with_inline_boxes(slots);
        }
        let parley_text = &mut self.parley_text;
        let layout = self
            .parley_layout_cache
            .get_or_compute(line_idx, &line, |l| parley_text.shape(l));

        // 4. Build atom-byte-start prefix sums + per-atom (x_min, x_max)
        // from the shaped layout. atom_x ranges are absolute pixel
        // positions (already include `px`).
        let atom_count = para.atoms.len();
        let mut atom_byte_starts: Vec<usize> = vec![0];
        let mut byte_accum = 0usize;
        for atom in &para.atoms {
            byte_accum += atom.contents.len();
            atom_byte_starts.push(byte_accum);
        }
        let mut atom_x_min: Vec<f32> = vec![f32::MAX; atom_count];
        let mut atom_x_max: Vec<f32> = vec![f32::MIN; atom_count];
        for layout_line in layout.layout.lines() {
            for item in layout_line.items() {
                let PositionedLayoutItem::GlyphRun(run) = item else {
                    continue;
                };
                let parley_run = run.run();
                for cluster in parley_run.clusters() {
                    let byte = cluster.text_range().start;
                    let idx = atom_byte_starts
                        .partition_point(|&b| b <= byte)
                        .saturating_sub(1);
                    if idx >= atom_count {
                        continue;
                    }
                    if let Some(advance) = cluster.visual_offset() {
                        let cluster_w = cluster.advance();
                        let x0 = px + advance;
                        let x1 = x0 + cluster_w;
                        if x0 < atom_x_min[idx] {
                            atom_x_min[idx] = x0;
                        }
                        if x1 > atom_x_max[idx] {
                            atom_x_max[idx] = x1;
                        }
                    }
                }
            }
        }

        // 5. Per-atom background + decorations using the glyph extents
        // we just measured. Atoms whose extent is unset (no glyphs:
        // pure whitespace handled by parley as advance-only, or face
        // stripped by max_width clipping) fall back to a cell-grid
        // estimate so non-default backgrounds + decorations still
        // render.
        let mut cell_x_cursor = px;
        for i in 0..atom_count {
            let face = if clear_cursor_atom_idx == Some(i) {
                para.base_face.to_face()
            } else {
                para.atoms[i].face()
            };
            let atom_display_w = line_display_width_str(&para.atoms[i].contents) as f32 * cell_w;
            let (x, w) = if atom_x_min[i] <= atom_x_max[i] {
                let w = (atom_x_max[i] - atom_x_min[i]).min(max_width);
                (atom_x_min[i], w)
            } else {
                let remaining = max_width - (cell_x_cursor - px);
                if remaining <= 0.0 {
                    cell_x_cursor += atom_display_w;
                    continue;
                }
                (cell_x_cursor, atom_display_w.min(remaining))
            };
            cell_x_cursor += atom_display_w;
            if w <= 0.0 {
                continue;
            }
            let (visual_fg, visual_bg, needs_bg) = color_resolver.resolve_face_colors_linear(&face);
            if needs_bg && !self.should_skip_default_bg(&visual_bg, color_resolver) {
                self.quad.push_solid(x, py, w, cell_h, visual_bg);
            }
            self.emit_decorations(x, py, w, &face, visual_fg, color_resolver);
        }

        // 6. Cursors via parley hit_test. byte_to_advance returns the
        // x advance of the cluster covering the byte offset; cursor
        // width is the cluster's own advance. Falls back to one cell
        // when the offset lies past the last cluster (EOL).
        let cursor_width_at = |offset: usize| -> Option<f32> {
            for layout_line in layout.layout.lines() {
                for item in layout_line.items() {
                    let PositionedLayoutItem::GlyphRun(run) = item else {
                        continue;
                    };
                    for cluster in run.run().clusters() {
                        let r = cluster.text_range();
                        if offset >= r.start && offset < r.end {
                            return Some(cluster.advance());
                        }
                    }
                }
            }
            None
        };
        for ann in &para.annotations {
            match ann {
                ParagraphAnnotation::PrimaryCursor { byte_offset, .. } => {
                    if let Some(advance) = byte_to_advance(&layout, *byte_offset) {
                        let cw = cursor_width_at(*byte_offset).unwrap_or(cell_w);
                        self.paragraph_cursor = Some((px + advance, cw));
                    }
                }
                ParagraphAnnotation::SecondaryCursor {
                    byte_offset,
                    blend_ratio,
                } => {
                    if let Some(advance) = byte_to_advance(&layout, *byte_offset) {
                        let cw = cursor_width_at(*byte_offset).unwrap_or(cell_w);
                        let cursor_color = [1.0_f32, 1.0, 1.0, 1.0];
                        let blended = [
                            cursor_color[0] * blend_ratio + base_bg[0] * (1.0 - blend_ratio),
                            cursor_color[1] * blend_ratio + base_bg[1] * (1.0 - blend_ratio),
                            cursor_color[2] * blend_ratio + base_bg[2] * (1.0 - blend_ratio),
                            1.0,
                        ];
                        self.quad
                            .push_solid(px + advance, py, cw.max(cell_w), cell_h, blended);
                    }
                }
            }
        }

        // 7. Emit glyphs from the shared layout. Brush is read **per
        // glyph** via the layout's style table (`Glyph::style_index`).
        // Reading just the run's first cluster brush would collapse a
        // multi-style shaping run (multiple syntax-coloured atoms that
        // share a font and so end up in one shape Run) onto the first
        // atom's colour — visible as "all text in one colour" / "no
        // syntax highlighting".
        let styles_table = layout.layout.styles();
        let rasterizer = &mut self.parley_glyph_rasterizer;
        let cache = &mut self.parley_raster_cache;
        let mut atlases = super::super::parley_text::raster_cache_glue::ParleyAtlasPair {
            mask: &mut self.parley_mask_atlas,
            color: &mut self.parley_color_atlas,
        };
        let drawables = &mut self.parley_drawables;
        for layout_line in layout.layout.lines() {
            let lm = layout_line.metrics();
            let leading = (cell_h - lm.line_height).max(0.0);
            let layout_origin_y = py + leading * 0.5;
            for item in layout_line.items() {
                let run = match item {
                    PositionedLayoutItem::GlyphRun(run) => run,
                    PositionedLayoutItem::InlineBox(pos_box) => {
                        // ADR-031 Phase 10 Step 2-renderer (Step A.2b):
                        // Parley reserved geometry for the inline box.
                        // If the host pre-painted the slot's plugin
                        // content (origin (0,0) Vec<DrawCommand>), enqueue
                        // a translated copy onto the deferred-emit queue.
                        // Otherwise emit a translucent placeholder so the
                        // reserved space is still visually observable.
                        let abs_x = px + pos_box.x;
                        let abs_y = layout_origin_y + pos_box.y;
                        let slot_idx = para
                            .inline_box_slots
                            .iter()
                            .position(|m| m.box_id == pos_box.id);
                        let painted = slot_idx.and_then(|i| para.inline_box_paint_commands.get(i));
                        match painted {
                            Some(sub_cmds) if !sub_cmds.is_empty() => {
                                let mut cloned = sub_cmds.clone();
                                kasane_core::render::scene::translate_draw_commands(
                                    &mut cloned,
                                    abs_x,
                                    abs_y,
                                );
                                self.deferred_inline_box_cmds.extend(cloned);
                            }
                            _ => {
                                let placeholder = [0.5, 0.5, 0.55, 0.35];
                                self.quad.push_solid(
                                    abs_x,
                                    abs_y,
                                    pos_box.width,
                                    pos_box.height,
                                    placeholder,
                                );
                            }
                        }
                        continue;
                    }
                };
                let parley_run = run.run();
                let font = parley_run.font();
                let font_id = super::super::parley_text::font_id::font_id_from_data(font);
                let var_hash = super::super::parley_text::font_id::var_hash_from_coords(
                    parley_run.normalized_coords(),
                );
                let font_size = parley_run.font_size();
                let size_q = (font_size * 64.0).round().clamp(0.0, u16::MAX as f32) as u16;
                let Some(font_ref) =
                    swash::FontRef::from_index(font.data.data(), font.index as usize)
                else {
                    continue;
                };

                for glyph in run.positioned_glyphs() {
                    let abs_x = px + glyph.x;
                    let abs_y = layout_origin_y + glyph.y;
                    let subpx = SubpixelX::from_fract(abs_x);
                    let glyph_id = glyph.id as u16;
                    let brush = styles_table
                        .get(glyph.style_index())
                        .map(|s| s.brush)
                        .unwrap_or(fallback_brush);
                    let key = super::super::parley_text::raster_cache::GlyphRasterKey {
                        font_id,
                        glyph_id,
                        size_q,
                        subpx_x: subpx.0,
                        var_hash,
                        hint: true,
                    };
                    let entry = cache.get_or_insert(key, &mut atlases, || {
                        rasterizer.rasterize(font_ref, glyph_id, font_size, subpx, true)
                    });
                    let Some(entry) = entry else {
                        continue;
                    };
                    drawables.push(DrawableGlyph {
                        px: abs_x,
                        py: abs_y,
                        width: entry.width,
                        height: entry.height,
                        left: entry.left,
                        top: entry.top,
                        content: entry.content,
                        atlas_slot: entry.atlas_slot,
                        brush,
                    });
                }
            }
        }
    }

    /// Process RenderParagraph: buffer line with semantic annotations.
    ///
    /// Shapes text first, then draws per-atom backgrounds and decorations from
    /// glyph metrics, and finally resolves annotation positions (cursors) from
    /// the shaping result.
    fn process_render_paragraph(
        &mut self,
        px: f32,
        py: f32,
        max_width: f32,
        para: &BufferParagraph,
        line_idx: u32,
        color_resolver: &ColorResolver,
    ) {
        self.process_render_paragraph_parley(px, py, max_width, para, line_idx, color_resolver);
    }

    /// Process DrawText: simple single-face text.
    fn process_draw_text(
        &mut self,
        px: f32,
        py: f32,
        text: &str,
        face: &kasane_core::protocol::Face,
        max_width: f32,
        color_resolver: &ColorResolver,
    ) {
        if text.is_empty() {
            return;
        }

        let text_w = line_display_width_str(text) as f32 * self.metrics.cell_width;
        let actual_w = text_w.min(max_width);
        let (visual_fg, visual_bg, needs_bg) = color_resolver.resolve_face_colors_linear(face);

        // Background — skip when not needed (parent bg shows through)
        if actual_w > 0.0 && needs_bg && !self.should_skip_default_bg(&visual_bg, color_resolver) {
            self.quad
                .push_solid(px, py, actual_w, self.metrics.cell_height, visual_bg);
        }

        // Text decorations
        if actual_w > 0.0 {
            self.emit_decorations(px, py, actual_w, face, visual_fg, color_resolver);
        }

        self.parley_emit_text(text, face, px, py, color_resolver);
    }
}

/// Map [`BorderLineStyle`] to the `(corner_radius, border_width)` pair
/// the quad pipeline expects. Width scales with cell height so the
/// border looks proportional across DPI / font sizes. Was previously in
/// `text_helpers`; inlined here as the only caller.
fn border_style_params(style: BorderLineStyle, cell_height: f32) -> (f32, f32) {
    let base = (cell_height * 0.08).max(1.5);
    match style {
        BorderLineStyle::Single => (0.0, base),
        BorderLineStyle::Rounded => (cell_height * 0.3, base),
        BorderLineStyle::Double => (0.0, base),
        BorderLineStyle::Heavy => (0.0, base * 2.0),
        BorderLineStyle::Ascii => (0.0, base),
        BorderLineStyle::Custom(_) => (0.0, base),
    }
}
