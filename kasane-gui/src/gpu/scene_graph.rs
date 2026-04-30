//! GPU-only scene graph: typed primitives for the GPU renderer.
//!
//! `GpuPrimitive` replaces `DrawCommand` on the GPU side, adding GPU-specific
//! features (gradient fills, corner radii, z-index, blend modes) that the
//! TUI-agnostic `DrawCommand` cannot represent.
//!
//! For the initial integration, `SceneBuilder::from_draw_commands()` converts
//! `DrawCommand` slices into `GpuPrimitive` lists. Future phases will generate
//! `GpuPrimitive` directly from the element tree.

use kasane_core::element::{BorderLineStyle, ImageFit, ImageSource};
use kasane_core::render::{DrawCommand, PixelRect};

/// A GPU-native rendering primitive.
#[derive(Debug, Clone)]
pub enum GpuPrimitive {
    /// A filled or bordered quad.
    Quad {
        rect: PixelRect,
        fill: QuadFill,
        corner_radius: [f32; 4],
        border_width: f32,
        border_color: [f32; 4],
        z_index: i32,
    },

    /// A text run (atoms or plain text).
    TextRun {
        x: f32,
        y: f32,
        spans: Vec<TextSpan>,
        max_width: f32,
        z_index: i32,
    },

    /// A raster image.
    Image {
        rect: PixelRect,
        source: ImageSource,
        fit: ImageFit,
        opacity: f32,
        z_index: i32,
    },

    /// A drop shadow.
    Shadow {
        rect: PixelRect,
        offset: (f32, f32),
        blur_radius: f32,
        color: [f32; 4],
    },

    /// A text decoration (underline, strikethrough, curly, etc).
    Decoration {
        rect: PixelRect,
        color: [f32; 4],
        style: DecorationStyle,
    },

    /// Push a clipping rectangle onto the clip stack.
    PushClip {
        rect: PixelRect,
        corner_radius: [f32; 4],
    },

    /// Pop the most recent clip.
    PopClip,

    /// Begin a new overlay layer (flush and re-composite).
    BeginLayer { blend_mode: BlendMode, opacity: f32 },

    /// End the current layer.
    EndLayer,
}

/// Fill mode for a quad.
#[derive(Debug, Clone)]
pub enum QuadFill {
    /// Solid color (linear RGBA).
    Solid([f32; 4]),
    /// Linear gradient with start/end colors and angle.
    LinearGradient {
        start: [f32; 4],
        end: [f32; 4],
        /// Angle in radians (0 = top-to-bottom).
        angle: f32,
    },
}

/// A single text span within a TextRun.
#[derive(Debug, Clone)]
pub struct TextSpan {
    pub text: String,
    pub color: [f32; 4],
}

/// Decoration style for underlines/strikethrough.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DecorationStyle {
    Solid,
    Curly,
    Double,
    Dotted,
    Dashed,
}

/// Layer blend mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
}

/// Style resolution function: (fg, bg, attributes).
type ResolveStyleFn = dyn Fn(&kasane_core::protocol::Style) -> ([f32; 4], [f32; 4], u32);

/// Builds a list of `GpuPrimitive`s from `DrawCommand`s.
///
/// This is the transitional bridge: later phases will generate `GpuPrimitive`
/// directly from the element tree, bypassing `DrawCommand` entirely.
pub struct SceneBuilder {
    primitives: Vec<GpuPrimitive>,
    z_counter: i32,
}

impl SceneBuilder {
    pub fn new() -> Self {
        Self {
            primitives: Vec::with_capacity(256),
            z_counter: 0,
        }
    }

    /// Convert a slice of DrawCommands into GpuPrimitives.
    pub fn from_draw_commands(
        commands: &[DrawCommand],
        resolve_style: &ResolveStyleFn,
    ) -> Vec<GpuPrimitive> {
        let mut builder = Self::new();
        for cmd in commands {
            builder.push_draw_command(cmd, resolve_style);
        }
        builder.build()
    }

    fn push_draw_command(&mut self, cmd: &DrawCommand, resolve_style: &ResolveStyleFn) {
        match cmd {
            DrawCommand::FillRect {
                rect,
                face,
                elevated,
            } => {
                let (_, mut bg, _) = resolve_style(face);
                if *elevated {
                    bg[0] = (bg[0] + 0.003).min(1.0);
                    bg[1] = (bg[1] + 0.003).min(1.0);
                    bg[2] = (bg[2] + 0.003).min(1.0);
                }
                self.primitives.push(GpuPrimitive::Quad {
                    rect: rect.clone(),
                    fill: QuadFill::Solid(bg),
                    corner_radius: [0.0; 4],
                    border_width: 0.0,
                    border_color: [0.0; 4],
                    z_index: self.z_counter,
                });
            }
            DrawCommand::DrawAtoms {
                pos,
                atoms,
                max_width,
                line_idx: _,
            } => {
                let spans: Vec<TextSpan> = atoms
                    .iter()
                    .map(|atom| {
                        let (fg, _, _) = resolve_style(&atom.style);
                        TextSpan {
                            text: atom.contents.clone(),
                            color: fg,
                        }
                    })
                    .collect();
                self.primitives.push(GpuPrimitive::TextRun {
                    x: pos.x,
                    y: pos.y,
                    spans,
                    max_width: *max_width,
                    z_index: self.z_counter,
                });
            }
            DrawCommand::DrawText {
                pos,
                text,
                face,
                max_width,
            } => {
                let (fg, _, _) = resolve_style(face);
                self.primitives.push(GpuPrimitive::TextRun {
                    x: pos.x,
                    y: pos.y,
                    spans: vec![TextSpan {
                        text: text.clone(),
                        color: fg,
                    }],
                    max_width: *max_width,
                    z_index: self.z_counter,
                });
            }
            DrawCommand::DrawPaddingRow { pos, ch, face, .. } => {
                let (fg, _, _) = resolve_style(face);
                self.primitives.push(GpuPrimitive::TextRun {
                    x: pos.x,
                    y: pos.y,
                    spans: vec![TextSpan {
                        text: ch.clone(),
                        color: fg,
                    }],
                    max_width: f32::MAX,
                    z_index: self.z_counter,
                });
            }
            DrawCommand::DrawBorder {
                rect,
                line_style,
                face,
                fill_face,
            } => {
                let (border_fg, _, _) = resolve_style(face);
                let fill = match fill_face {
                    Some(ff) => {
                        let (_, bg, _) = resolve_style(ff);
                        bg
                    }
                    None => [0.0, 0.0, 0.0, 0.0],
                };
                let (corner_radius, border_width) = match line_style {
                    BorderLineStyle::Rounded => (6.0, 1.0),
                    BorderLineStyle::Double => (0.0, 3.0),
                    _ => (0.0, 1.0),
                };
                self.primitives.push(GpuPrimitive::Quad {
                    rect: rect.clone(),
                    fill: QuadFill::Solid(fill),
                    corner_radius: [corner_radius; 4],
                    border_width,
                    border_color: border_fg,
                    z_index: self.z_counter,
                });
            }
            DrawCommand::DrawBorderTitle { .. } => {
                // Border titles are handled as DrawAtoms after conversion
                // in the existing pipeline; keep as-is for now.
            }
            DrawCommand::DrawShadow {
                rect,
                offset,
                blur_radius,
                color,
            } => {
                self.primitives.push(GpuPrimitive::Shadow {
                    rect: rect.clone(),
                    offset: *offset,
                    blur_radius: *blur_radius,
                    color: *color,
                });
            }
            DrawCommand::PushClip(rect) => {
                self.primitives.push(GpuPrimitive::PushClip {
                    rect: rect.clone(),
                    corner_radius: [0.0; 4],
                });
            }
            DrawCommand::PopClip => {
                self.primitives.push(GpuPrimitive::PopClip);
            }
            DrawCommand::DrawImage {
                rect,
                source,
                fit,
                opacity,
            } => {
                self.primitives.push(GpuPrimitive::Image {
                    rect: rect.clone(),
                    source: source.clone(),
                    fit: *fit,
                    opacity: *opacity,
                    z_index: self.z_counter,
                });
            }
            DrawCommand::DrawCanvas { .. } => {
                // Canvas ops are processed directly in scene_renderer
                // via the quad pipeline; scene_graph pass-through for now.
            }
            DrawCommand::RenderParagraph { .. } => {
                // RenderParagraph is handled directly in scene_renderer
                // via the shaping-first approach; scene_graph pass-through.
            }
            DrawCommand::BeginOverlay => {
                self.z_counter += 1;
                self.primitives.push(GpuPrimitive::BeginLayer {
                    blend_mode: BlendMode::Normal,
                    opacity: 1.0,
                });
            }
        }
    }

    /// Consume the builder and return the primitive list.
    pub fn build(self) -> Vec<GpuPrimitive> {
        self.primitives
    }
}

impl Default for SceneBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kasane_core::protocol::Style;
    use kasane_core::render::scene::{PixelPos, ResolvedAtom};

    fn dummy_resolve(style: &Style) -> ([f32; 4], [f32; 4], u32) {
        let _ = style;
        ([1.0, 1.0, 1.0, 1.0], [0.0, 0.0, 0.0, 1.0], 0)
    }

    #[test]
    fn fill_rect_converts() {
        let commands = vec![DrawCommand::FillRect {
            rect: PixelRect {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 20.0,
            },
            face: Style::default(),
            elevated: false,
        }];
        let prims = SceneBuilder::from_draw_commands(&commands, &dummy_resolve);
        assert_eq!(prims.len(), 1);
        assert!(matches!(prims[0], GpuPrimitive::Quad { .. }));
    }

    #[test]
    fn draw_atoms_converts_to_text_run() {
        let commands = vec![DrawCommand::DrawAtoms {
            pos: PixelPos { x: 0.0, y: 0.0 },
            atoms: vec![ResolvedAtom {
                contents: "hello".into(),
                style: kasane_core::protocol::Style::default(),
            }],
            max_width: 100.0,
            line_idx: 0,
        }];
        let prims = SceneBuilder::from_draw_commands(&commands, &dummy_resolve);
        assert_eq!(prims.len(), 1);
        match &prims[0] {
            GpuPrimitive::TextRun { spans, .. } => {
                assert_eq!(spans.len(), 1);
                assert_eq!(spans[0].text, "hello");
            }
            _ => panic!("expected TextRun"),
        }
    }

    #[test]
    fn begin_overlay_increments_z() {
        let commands = vec![
            DrawCommand::FillRect {
                rect: PixelRect {
                    x: 0.0,
                    y: 0.0,
                    w: 10.0,
                    h: 10.0,
                },
                face: Style::default(),
                elevated: false,
            },
            DrawCommand::BeginOverlay,
            DrawCommand::FillRect {
                rect: PixelRect {
                    x: 0.0,
                    y: 0.0,
                    w: 10.0,
                    h: 10.0,
                },
                face: Style::default(),
                elevated: false,
            },
        ];
        let prims = SceneBuilder::from_draw_commands(&commands, &dummy_resolve);
        assert_eq!(prims.len(), 3);
        if let GpuPrimitive::Quad { z_index, .. } = &prims[0] {
            assert_eq!(*z_index, 0);
        }
        if let GpuPrimitive::Quad { z_index, .. } = &prims[2] {
            assert_eq!(*z_index, 1);
        }
    }

    #[test]
    fn push_pop_clip_converts() {
        let commands = vec![
            DrawCommand::PushClip(PixelRect {
                x: 10.0,
                y: 20.0,
                w: 100.0,
                h: 50.0,
            }),
            DrawCommand::PopClip,
        ];
        let prims = SceneBuilder::from_draw_commands(&commands, &dummy_resolve);
        assert_eq!(prims.len(), 2);
        assert!(matches!(prims[0], GpuPrimitive::PushClip { .. }));
        assert!(matches!(prims[1], GpuPrimitive::PopClip));
    }

    #[test]
    fn image_converts() {
        let commands = vec![DrawCommand::DrawImage {
            rect: PixelRect {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 100.0,
            },
            source: ImageSource::Rgba {
                data: vec![0u8; 4].into(),
                width: 1,
                height: 1,
            },
            fit: ImageFit::Fill,
            opacity: 0.8,
        }];
        let prims = SceneBuilder::from_draw_commands(&commands, &dummy_resolve);
        assert_eq!(prims.len(), 1);
        match &prims[0] {
            GpuPrimitive::Image { opacity, .. } => {
                assert!((*opacity - 0.8).abs() < 0.001);
            }
            _ => panic!("expected Image"),
        }
    }
}
