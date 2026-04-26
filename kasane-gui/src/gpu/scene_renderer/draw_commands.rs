//! Draw command processing methods for [`SceneRenderer`].
//!
//! Handles dispatch of individual [`DrawCommand`] variants to the quad, text,
//! and image pipelines, plus text shaping helpers.

use cosmic_text::{Buffer as GlyphonBuffer, Metrics, Shaping};

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
        screen_w: f32,
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
                let (_, mut bg, _) = color_resolver.resolve_face_colors_linear(face);

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
                self.process_draw_text(pos.x, pos.y, text, face, *max_width, color_resolver);
            }
            DrawCommand::DrawPaddingRow {
                pos,
                width: _,
                ch,
                face,
            } => {
                self.parley_emit_text(ch, face, pos.x, pos.y, color_resolver);
            }
            DrawCommand::DrawBorder {
                rect,
                line_style,
                face,
                fill_face,
            } => {
                let (visual_fg, _, _) = color_resolver.resolve_face_colors_linear(face);
                let border_color = visual_fg;
                let (corner_radius, border_width) =
                    super::super::text_helpers::border_style_params(line_style.clone(), cell_h);
                let fill = match fill_face {
                    Some(ff) => {
                        let (_, ff_bg, _) = color_resolver.resolve_face_colors_linear(ff);
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

                let (_, mut title_bg, _) = color_resolver.resolve_face_colors_linear(border_face);
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
                color_resolver.resolve_face_colors_linear(&atom.face);

            if actual_w > 0.0
                && needs_bg
                && !self.should_skip_default_bg(&visual_bg, color_resolver)
            {
                self.quad.push_solid(x, py, actual_w, cell_h, visual_bg);
            }
            if actual_w > 0.0 {
                self.emit_decorations(x, py, actual_w, &atom.face, visual_fg, color_resolver);
            }
            x += actual_w;
        }

        // Route glyph rendering through Parley.
        self.parley_emit_atoms(atoms, px, py, color_resolver);
    }

    /// ADR-031 Phase 9b Step 4f — Parley alternative to
    /// `process_render_paragraph`. Renders the paragraph's line bg,
    /// per-atom backgrounds, decorations, glyph runs and cursors using
    /// cell-grid estimates. The cosmic-derived shaping cache and
    /// glyph-accurate cursor metrics are skipped here because Parley's
    /// L1 LayoutCache + per-glyph hit test (Phase 9b Step 7) are not
    /// yet wired into the paragraph path. ASCII rendering matches the
    /// cosmic path; CJK / proportional cursors degrade to cell width.
    fn process_render_paragraph_parley(
        &mut self,
        px: f32,
        py: f32,
        max_width: f32,
        para: &BufferParagraph,
        color_resolver: &ColorResolver,
    ) {
        let cell_h = self.metrics.cell_height;
        let cell_w = self.metrics.cell_width;

        // 1. Line-wide background fill (mirrors the cosmic path).
        let (_, base_bg, _) = color_resolver.resolve_face_colors_linear(&para.base_face);
        if !self.should_skip_default_bg(&base_bg, color_resolver) {
            self.quad.push_solid(px, py, max_width, cell_h, base_bg);
        }

        if para.atoms.is_empty() {
            return;
        }

        // 2. Strip the cursor face on the atom under the primary cursor —
        // render_cursor() owns the cursor block, so the atom must not
        // also paint a REVERSE bg over it.
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

        // 3. Per-atom background quads + decorations + cumulative byte
        // boundaries (used for cursor lookup below). Widths are
        // cell-grid estimates; that matches the snap policy used in
        // parley_emit_atoms so the bg / glyph alignment is consistent.
        let mut atom_byte_starts: Vec<usize> = vec![0];
        let mut atom_x_starts: Vec<f32> = Vec::with_capacity(para.atoms.len());
        let mut x = px;
        let mut byte_accum = 0usize;
        for (i, atom) in para.atoms.iter().enumerate() {
            let face = if clear_cursor_atom_idx == Some(i) {
                para.base_face
            } else {
                atom.face
            };
            let atom_display_w = line_display_width_str(&atom.contents) as f32 * cell_w;
            let remaining = max_width - (x - px);
            if remaining <= 0.0 {
                break;
            }
            let actual_w = atom_display_w.min(remaining);
            let (visual_fg, visual_bg, needs_bg) = color_resolver.resolve_face_colors_linear(&face);

            if actual_w > 0.0
                && needs_bg
                && !self.should_skip_default_bg(&visual_bg, color_resolver)
            {
                self.quad.push_solid(x, py, actual_w, cell_h, visual_bg);
            }
            if actual_w > 0.0 {
                self.emit_decorations(x, py, actual_w, &face, visual_fg, color_resolver);
            }
            atom_x_starts.push(x);
            x += actual_w;
            byte_accum += atom.contents.len();
            atom_byte_starts.push(byte_accum);
        }

        // 4. Cursors. Cell-grid resolution: locate the atom containing
        // the byte offset, then compute the offset within the atom in
        // display columns. Glyph-accurate metrics (CJK, ligatures,
        // RTL) come in Phase 10 once parley hit_test is wired here.
        for ann in &para.annotations {
            match ann {
                ParagraphAnnotation::PrimaryCursor { byte_offset, .. } => {
                    if let Some((cx, cw)) = cell_grid_cursor(
                        &para.atoms,
                        &atom_byte_starts,
                        &atom_x_starts,
                        *byte_offset,
                        cell_w,
                    ) {
                        self.paragraph_cursor = Some((cx, cw));
                    }
                }
                ParagraphAnnotation::SecondaryCursor {
                    byte_offset,
                    blend_ratio,
                } => {
                    if let Some((cx, cw)) = cell_grid_cursor(
                        &para.atoms,
                        &atom_byte_starts,
                        &atom_x_starts,
                        *byte_offset,
                        cell_w,
                    ) {
                        let cursor_color = [1.0_f32, 1.0, 1.0, 1.0];
                        let blended = [
                            cursor_color[0] * blend_ratio + base_bg[0] * (1.0 - blend_ratio),
                            cursor_color[1] * blend_ratio + base_bg[1] * (1.0 - blend_ratio),
                            cursor_color[2] * blend_ratio + base_bg[2] * (1.0 - blend_ratio),
                            1.0,
                        ];
                        self.quad
                            .push_solid(cx, py, cw.max(cell_w), cell_h, blended);
                    }
                }
            }
        }

        // 5. Glyph emission via Parley (snapped per atom to cell grid).
        // When the primary cursor sits inside an atom, swap that atom's
        // face with the paragraph base_face so render_cursor() owns the
        // visual cursor block (matches the cosmic path's stripping).
        if let Some(strip_idx) = clear_cursor_atom_idx {
            let mut stripped = para.atoms.clone();
            if let Some(a) = stripped.get_mut(strip_idx) {
                a.face = para.base_face;
            }
            self.parley_emit_atoms(&stripped, px, py, color_resolver);
        } else {
            self.parley_emit_atoms(&para.atoms, px, py, color_resolver);
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
        _line_idx: u32,
        color_resolver: &ColorResolver,
    ) {
        // Phase 11 — Parley-only path. `_line_idx` was the cosmic
        // shaping cache key; the Parley emitter currently re-shapes
        // each frame and ignores it.
        self.process_render_paragraph_parley(px, py, max_width, para, color_resolver);
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

    /// Push the current clip bounds for the most recently allocated text buffer.
    fn push_text_clip_bounds(&mut self) {
        let bounds = if let Some(clip) = self.current_clip() {
            (
                clip.x as i32,
                clip.y as i32,
                (clip.x + clip.w) as i32,
                (clip.y + clip.h) as i32,
            )
        } else {
            (0, 0, self.frame_screen_w as i32, self.frame_screen_h as i32)
        };
        self.text_clip_bounds.push(bounds);
    }

    /// Allocate (or reuse) a text buffer. Returns the index.
    /// Reserve a text buffer slot for fresh shaping work.
    ///
    /// Slot assignment cooperates with `LineShapingCache`: we pick the lowest
    /// slot that has not been claimed this frame, so cached buffers from
    /// earlier frames remain pinned to their addresses (a hit returns the same
    /// `buffer_idx` repeatedly across frames). When every slot is claimed we
    /// grow the pool.
    fn alloc_text_buffer(&mut self, max_width: f32) -> usize {
        let glyph_metrics = Metrics::new(self.font_size, self.line_height);
        let idx = if let Some(free) = self.line_cache.find_free_slot() {
            self.text_buffers[free].set_metrics(&mut self.font_system, glyph_metrics);
            self.text_buffers[free].set_wrap(&mut self.font_system, cosmic_text::Wrap::None);
            self.text_buffers[free].set_size(
                &mut self.font_system,
                Some(max_width),
                Some(self.metrics.cell_height),
            );
            free
        } else {
            let mut buffer = GlyphonBuffer::new(&mut self.font_system, glyph_metrics);
            buffer.set_hinting(&mut self.font_system, cosmic_text::Hinting::Enabled);
            buffer.set_wrap(&mut self.font_system, cosmic_text::Wrap::None);
            buffer.set_size(
                &mut self.font_system,
                Some(max_width),
                Some(self.metrics.cell_height),
            );
            self.text_buffers.push(buffer);
            self.text_buffers.len() - 1
        };
        // Mark the slot in_use so subsequent allocations skip it. The cache
        // also marks slots in_use on hit/insert; this branch covers the
        // process_draw_padding_row and similar paths that bypass the cache.
        self.line_cache.mark_in_use(idx);
        self.line_cache.note_pool_size(self.text_buffers.len());
        idx
    }
}

/// Cell-grid cursor lookup used by the Phase 9b Step 4f Parley
/// paragraph path. Walks the atom-byte boundaries to find the atom
/// containing `byte_offset`, then advances by the display width of
/// the atom's leading bytes (cell columns) to compute the cursor's
/// pixel x. Width is `cell_w × char_columns` for the char under the
/// cursor; one cell when the cursor sits past EOL.
///
/// Glyph-accurate positions (CJK ligatures, RTL, proportional fonts)
/// are deferred to Phase 10 once `parley_text::hit_test` is wired in.
/// The char-based width estimate is enough for Latin / monospace CJK
/// where one codepoint = one grapheme; complex scripts would need
/// `unicode_segmentation::graphemes`, deferred with the rest of the
/// hit-test work.
fn cell_grid_cursor(
    atoms: &[kasane_core::render::ResolvedAtom],
    atom_byte_starts: &[usize],
    atom_x_starts: &[f32],
    byte_offset: usize,
    cell_w: f32,
) -> Option<(f32, f32)> {
    let owner_idx = atom_byte_starts
        .windows(2)
        .position(|w| byte_offset >= w[0] && byte_offset < w[1]);
    match owner_idx {
        Some(i) if i < atoms.len() && i < atom_x_starts.len() => {
            let atom = &atoms[i];
            let atom_start = atom_byte_starts[i];
            let leading_bytes = byte_offset.saturating_sub(atom_start);
            let leading_text = atom.contents.get(..leading_bytes).unwrap_or("");
            let leading_cols = line_display_width_str(leading_text) as f32;
            let cx = atom_x_starts[i] + leading_cols * cell_w;
            // First char's display columns approximates the grapheme
            // width — Latin / CJK monospace renders one column per
            // codepoint, which covers the common cursor geometry.
            let rest = &atom.contents.as_str()[leading_bytes..];
            let cw_cols = rest
                .chars()
                .next()
                .map(|c| {
                    let mut buf = [0u8; 4];
                    let s: &str = c.encode_utf8(&mut buf);
                    line_display_width_str(s) as f32
                })
                .unwrap_or(1.0);
            Some((cx, cw_cols * cell_w))
        }
        _ => {
            let last_x = *atom_x_starts.last()?;
            let last_atom = atoms.last()?;
            let last_w = line_display_width_str(&last_atom.contents) as f32 * cell_w;
            Some((last_x + last_w, cell_w))
        }
    }
}

/// Find the glyph covering the given byte offset in a shaped buffer.
/// Returns `(glyph_x, glyph_w)` if found.
fn find_glyph_at_byte_offset(buffer: &GlyphonBuffer, byte_offset: usize) -> Option<(f32, f32)> {
    for run in buffer.layout_runs() {
        for glyph in run.glyphs.iter() {
            if byte_offset >= glyph.start && byte_offset < glyph.end {
                return Some((glyph.x, glyph.w));
            }
        }
    }
    None
}
