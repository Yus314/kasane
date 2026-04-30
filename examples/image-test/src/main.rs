//! Image test plugin — displays an image overlay for verifying the GPU image pipeline.
//!
//! Usage:
//!   cargo run --features gui -- [kak args...]              # inline RGBA checkerboard
//!   IMAGE_PATH=/tmp/test.png cargo run --features gui --   # file-based async loading
//!
//! The plugin renders a 10×6 cell overlay anchored at the cursor position.

use std::sync::Arc;

use kasane::kasane_core::plugin_prelude::*;

/// Generate a checkerboard RGBA image: red and green 16×16 tiles.
fn checkerboard_rgba(width: u32, height: u32) -> Vec<u8> {
    let tile = 16u32;
    let mut data = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let is_even = ((x / tile) + (y / tile)) % 2 == 0;
            if is_even {
                data.extend_from_slice(&[220, 40, 40, 255]); // red
            } else {
                data.extend_from_slice(&[40, 180, 40, 255]); // green
            }
        }
    }
    data
}

#[derive(Clone)]
enum ImageMode {
    /// Inline RGBA checkerboard (synchronous, always works).
    Inline {
        data: Arc<[u8]>,
        width: u32,
        height: u32,
    },
    /// File path (async loading via Phase 4).
    File(String),
}

struct ImageTestPlugin {
    mode: ImageMode,
}

impl ImageTestPlugin {
    fn new() -> Self {
        let mode = if let Ok(path) = std::env::var("IMAGE_PATH") {
            ImageMode::File(path)
        } else {
            let (w, h) = (64, 48);
            let data = checkerboard_rgba(w, h);
            ImageMode::Inline {
                data: data.into(),
                width: w,
                height: h,
            }
        };
        Self { mode }
    }
}

impl Plugin for ImageTestPlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId("image_test".into())
    }

    fn register(&self, r: &mut HandlerRegistry<()>) {
        r.declare_interests(DirtyFlags::BUFFER_CURSOR);
        let mode = self.mode.clone();
        r.on_overlay(move |_state, app, ctx| {
            let cursor = app.cursor_pos();

            // Build avoid list from existing overlays + menu
            let mut avoid: Vec<_> = ctx.existing_overlays.clone();
            if let Some(menu) = ctx.menu_rect {
                avoid.push(menu);
            }

            let source = match &mode {
                ImageMode::Inline {
                    data,
                    width,
                    height,
                } => ImageSource::Rgba {
                    data: data.clone(),
                    width: *width,
                    height: *height,
                },
                ImageMode::File(path) => ImageSource::FilePath(path.clone()),
            };

            let image = Element::Image {
                source,
                size: (10, 6),
                fit: ImageFit::Fill,
                opacity: 1.0,
            };

            // Wrap in a container with a border for visibility
            let element = Element::column(vec![
                FlexChild::fixed(Element::text_with_style(
                    " Image Test ".to_string(),
                    Style {
                        fg: Brush::Named(NamedColor::Black),
                        bg: Brush::Named(NamedColor::Yellow),
                        ..Style::default()
                    },
                )),
                FlexChild::fixed(image),
            ]);

            Some(OverlayContribution {
                element,
                anchor: OverlayAnchor::AnchorPoint {
                    coord: Coord {
                        line: cursor.line,
                        column: cursor.column,
                    },
                    prefer_above: false,
                    avoid,
                },
                z_index: 50,
                plugin_id: PluginId("image_test".into()),
            })
        });
    }
}

fn main() {
    kasane::run_with_factories([host_plugin("image_test", || {
        PluginBridge::new(ImageTestPlugin::new())
    })]);
}
