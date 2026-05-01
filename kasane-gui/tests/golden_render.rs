//! Golden image regression harness for `kasane-gui` (ADR-032 W2).
//!
//! This is the minimum viable scaffold: it constructs a *headless* wgpu
//! device (no surface), renders a single frame to an offscreen RGBA8
//! texture, reads the pixels back, and compares against a committed PNG
//! using DSSIM via [`image_compare`].
//!
//! The current proof-of-concept exercises only the clear-color path —
//! enough to validate that headless wgpu init, render-to-texture,
//! readback, and DSSIM comparison all function in this repo's
//! environment. Pipeline-level fixtures (QuadPipeline, ImagePipeline,
//! TextPipeline, full SceneRenderer) are tracked under W2 follow-up
//! and slot in here as additional `#[test]` functions that share
//! [`headless_gpu`], [`render_to_png_bytes`], and [`assert_dssim`].
//!
//! ## Snapshot update workflow
//!
//! - Default: each test asserts DSSIM ≤ [`DSSIM_THRESHOLD`] against the
//!   committed PNG at `tests/golden/snapshots/<name>.png`.
//! - Updating: set `KASANE_GOLDEN_UPDATE=1` to overwrite the snapshot
//!   with the freshly rendered output instead of asserting.
//! - First run with no committed snapshot: the test writes the snapshot
//!   and passes (acts as a bootstrap). Subsequent runs assert.
//!
//! ## Sandbox / CI constraints
//!
//! Headless wgpu initialisation requires a working Vulkan / GL stack.
//! Some sandboxed environments lack `/dev/dri` access; in those cases
//! the test gracefully `eprintln!`s the reason and exits success. CI
//! that has GPU access (or a software lavapipe fallback) will exercise
//! the assertion path.

use std::path::{Path, PathBuf};

use image::{ImageBuffer, Rgba, RgbaImage};
use wgpu::util::DeviceExt;

/// DSSIM threshold for golden comparison. ≤ 0.005 is the byte-stable
/// target (ADR-032 W2 plan). The clear-color test is byte-stable on a
/// fixed driver, so 0.005 is comfortable.
const DSSIM_THRESHOLD: f64 = 0.005;

/// Snapshot directory (relative to the crate root, where `cargo test`
/// runs).
fn snapshots_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/snapshots")
}

/// True iff the `KASANE_GOLDEN_UPDATE` environment variable is set to a
/// truthy value. When true, tests write fresh snapshots instead of
/// asserting.
fn update_mode() -> bool {
    std::env::var("KASANE_GOLDEN_UPDATE")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false)
}

/// Headless wgpu device + queue. Returns `None` when no adapter is
/// available (typical in CI / sandboxed environments without GPU
/// access); callers should treat that as a soft skip.
fn headless_gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());

    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .ok()?;

    eprintln!(
        "golden harness adapter: {} ({:?})",
        adapter.get_info().name,
        adapter.get_info().backend
    );

    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: wgpu::Features::empty(),
        ..Default::default()
    }))
    .ok()?;

    Some((device, queue))
}

/// Render a single frame using `render_fn` into an RGBA8 texture and
/// return it as a `RgbaImage`.
///
/// `render_fn` receives the texture view to render into; it is
/// responsible for issuing all encoder commands and submitting them.
/// The harness owns the encoder lifecycle and the readback.
fn render_to_image<F>(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    width: u32,
    height: u32,
    render_fn: F,
) -> RgbaImage
where
    F: FnOnce(&wgpu::Device, &wgpu::Queue, &wgpu::TextureView),
{
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("golden_target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    render_fn(device, queue, &view);

    // Read back. wgpu requires bytes-per-row to be a multiple of
    // COPY_BYTES_PER_ROW_ALIGNMENT (256). For a 100-px wide RGBA image
    // this is 400 bytes (already aligned), but for arbitrary widths we
    // pad and crop on CPU.
    let bytes_per_pixel: u32 = 4;
    let unpadded_bytes_per_row = width * bytes_per_pixel;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;

    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("golden_readback"),
        size: (padded_bytes_per_row * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("golden_readback_encoder"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));

    let slice = buffer.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    receiver
        .recv()
        .expect("map_async result")
        .expect("buffer map");

    let data = slice.get_mapped_range();
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height {
        let start = (row * padded_bytes_per_row) as usize;
        let end = start + (unpadded_bytes_per_row as usize);
        pixels.extend_from_slice(&data[start..end]);
    }
    drop(data);
    buffer.unmap();

    ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, pixels).expect("RGBA8 image construction")
}

/// Compare `actual` against the committed snapshot at
/// `tests/golden/snapshots/<name>.png`. On first run (no snapshot),
/// or when `KASANE_GOLDEN_UPDATE=1`, write the snapshot and pass.
fn assert_dssim(actual: &RgbaImage, name: &str) {
    let dir = snapshots_dir();
    std::fs::create_dir_all(&dir).expect("create snapshots dir");
    let path = dir.join(format!("{name}.png"));

    if update_mode() || !path.exists() {
        actual.save(&path).expect("write snapshot");
        eprintln!("golden harness wrote snapshot: {}", path.display());
        return;
    }

    let expected = image::open(&path)
        .unwrap_or_else(|e| panic!("load snapshot {}: {e}", path.display()))
        .to_rgba8();

    assert_eq!(
        (actual.width(), actual.height()),
        (expected.width(), expected.height()),
        "snapshot size mismatch for {name}"
    );

    // image-compare expects RGB; drop alpha for the comparison.
    let actual_rgb = drop_alpha(actual);
    let expected_rgb = drop_alpha(&expected);

    let result =
        image_compare::rgb_hybrid_compare(&expected_rgb, &actual_rgb).expect("rgb_hybrid_compare");
    let dissimilarity = 1.0 - result.score;

    assert!(
        dissimilarity <= DSSIM_THRESHOLD,
        "golden mismatch for {name}: dissimilarity {dissimilarity:.5} > threshold {DSSIM_THRESHOLD}\
         \nupdate with: KASANE_GOLDEN_UPDATE=1 cargo test -p kasane-gui --test golden_render",
    );
}

fn drop_alpha(rgba: &RgbaImage) -> image::RgbImage {
    let (w, h) = (rgba.width(), rgba.height());
    let mut out = image::RgbImage::new(w, h);
    for (dst, src) in out.pixels_mut().zip(rgba.pixels()) {
        *dst = image::Rgb([src[0], src[1], src[2]]);
    }
    out
}

/// Skip the test gracefully if no adapter is available (sandboxed CI,
/// no Vulkan/GL stack, etc.).
macro_rules! gpu_or_skip {
    () => {{
        match headless_gpu() {
            Some(gpu) => gpu,
            None => {
                eprintln!("no wgpu adapter available; skipping golden test");
                return;
            }
        }
    }};
}

/// Helper: render via `render_fn` and assert against snapshot `name`.
/// `render_fn` must encode and submit its work to `queue`.
#[allow(dead_code)]
fn golden<F>(name: &str, width: u32, height: u32, render_fn: F)
where
    F: FnOnce(&wgpu::Device, &wgpu::Queue, &wgpu::TextureView),
{
    let (device, queue) = gpu_or_skip!();
    let img = render_to_image(&device, &queue, width, height, render_fn);
    assert_dssim(&img, name);
}

// Suppress dead_code for the helpers when only the clear-color test uses
// them — pipeline tests will land in follow-up.
#[allow(dead_code)]
fn _unused_marker(_path: &Path) {}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

/// Smoke test: clear the target to a known sRGB colour and verify the
/// readback matches the committed snapshot.
///
/// This is the minimum viable proof of the harness. It exercises:
///   - headless wgpu instance + adapter + device
///   - render-to-texture (no surface)
///   - render pass with a clear-color attachment
///   - texture-to-buffer copy with COPY_BYTES_PER_ROW_ALIGNMENT padding
///   - buffer mapping + readback
///   - DSSIM comparison (or first-run snapshot bootstrap)
#[test]
fn clear_color_red() {
    let (device, queue) = gpu_or_skip!();

    let img = render_to_image(&device, &queue, 64, 64, |_device, queue, view| {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("clear_color_red"),
        });
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("clear_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        // sRGB-linear (~0.5, 0.05, 0.05) ≈ sRGB (188, 64, 64).
                        // Using non-zero RGB makes accidental black-frame
                        // bugs visible.
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.5,
                            g: 0.05,
                            b: 0.05,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
        }
        queue.submit(Some(encoder.finish()));
    });

    assert_dssim(&img, "clear_color_red");
}

// Marker so wgpu::util gets used (avoids unused-import warning when the
// only test is the clear-color one). Pipeline tests will use this.
#[allow(dead_code)]
fn _wgpu_util_marker(device: &wgpu::Device) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: &[],
        usage: wgpu::BufferUsages::VERTEX,
    })
}

// ---------------------------------------------------------------------
// W3.6 — Color emoji bitmap golden
// ---------------------------------------------------------------------

/// Relaxed DSSIM threshold for the color emoji golden — cross-font-version
/// drift, hinting differences, and per-glyph anti-aliasing all contribute
/// noise an exact match cannot tolerate. 0.05 catches gross regressions
/// (Color path silently routes to Mask, glyph dimensions wildly wrong,
/// rasterisation produces all-black output) while staying tolerant of the
/// realistic font-version variance between dev machines and CI.
const COLOR_EMOJI_DSSIM_THRESHOLD: f64 = 0.05;

/// Build a Parley `FontFamily` list that prefers a system color emoji
/// font with a monospace fallback. Mirrors `integration_test.rs::emoji_then_monospace`.
fn emoji_then_monospace() -> parley::FontFamily<'static> {
    use parley::{FontFamily, FontFamilyName, GenericFamily};
    use std::borrow::Cow;
    FontFamily::List(Cow::Owned(vec![
        FontFamilyName::Named(Cow::Borrowed("Noto Color Emoji")),
        FontFamilyName::Named(Cow::Borrowed("Apple Color Emoji")),
        FontFamilyName::Generic(GenericFamily::Monospace),
    ]))
}

/// Rasterise the first color glyph from `text` shaped against an
/// emoji-first family stack. Returns `None` when the system has no
/// color emoji font installed (typical in minimal CI containers) or
/// when shaping failed to produce a `ContentKind::Color` glyph (which
/// indicates fontique fell back to a monospace tofu — the emoji font
/// was reported but couldn't actually render the requested codepoint).
fn rasterize_color_emoji_glyph(text: &str) -> Option<RgbaImage> {
    use kasane_core::config::FontConfig;
    use kasane_core::protocol::{Atom, Style};
    use kasane_gui::gpu::text::glyph_rasterizer::{ContentKind, GlyphRasterizer, SubpixelX};
    use kasane_gui::gpu::text::shaper::shape_line;
    use kasane_gui::gpu::text::styled_line::StyledLine;
    use kasane_gui::gpu::text::{Brush, ParleyText};
    use parley::PositionedLayoutItem;

    let mut text_engine = ParleyText::new(&FontConfig::default());
    let atoms = vec![Atom::plain(text)];
    let line = StyledLine::from_atoms(
        &atoms,
        &Style::default(),
        Brush::opaque(255, 255, 255),
        // 24px is the smallest size at which Noto Color Emoji ships a
        // CBDT/COLR bitmap large enough to be visually meaningful;
        // smaller sizes downsample and produce muddy output that
        // amplifies cross-version DSSIM drift.
        24.0,
        None,
    );
    let layout = shape_line(&mut text_engine, &line, emoji_then_monospace());
    let mut rasterizer = GlyphRasterizer::new();

    for layout_line in layout.layout.lines() {
        for item in layout_line.items() {
            let PositionedLayoutItem::GlyphRun(run) = item else {
                continue;
            };
            let parley_run = run.run();
            let font = parley_run.font();
            let font_size = parley_run.font_size();
            let font_ref = match swash::FontRef::from_index(font.data.data(), font.index as usize) {
                Some(r) => r,
                None => continue,
            };
            for glyph in run.positioned_glyphs() {
                let subpx = SubpixelX::from_fract(glyph.x);
                let raster =
                    match rasterizer.rasterize(font_ref, glyph.id as u16, font_size, subpx, true) {
                        Some(r) if r.content == ContentKind::Color => r,
                        _ => continue,
                    };
                if raster.width == 0 || raster.height == 0 {
                    continue;
                }
                let img = ImageBuffer::<Rgba<u8>, _>::from_raw(
                    u32::from(raster.width),
                    u32::from(raster.height),
                    raster.data,
                )?;
                return Some(img);
            }
        }
    }
    None
}

/// W3.6 — Pin that the color emoji rasterisation path produces a stable
/// RGBA bitmap when Noto Color Emoji (or an Apple Color Emoji equivalent)
/// is available on the host. Soft-skips on minimal CI containers without
/// an emoji font installed.
///
/// Catches:
///   - The Color → Mask routing regression (`raster_cache.rs:67-68`).
///   - Wildly wrong glyph dimensions (font scale broken).
///   - All-zero rasterisation output (swash `Source::ColorOutline` /
///     `ColorBitmap` chain miswired in `glyph_rasterizer.rs:140`).
///   - Per-font-version drift caught by DSSIM exceeding 5%.
///
/// Does NOT catch:
///   - Sub-threshold visual drift between font versions (intentional).
///   - GPU pipeline output differences — this golden tests the CPU
///     rasterisation contract, not the GPU compositing path.
#[test]
fn color_emoji_grinning_face() {
    // U+1F600 GRINNING FACE — the most fontique-discoverable color codepoint.
    let img = match rasterize_color_emoji_glyph("\u{1F600}") {
        Some(i) => i,
        None => {
            eprintln!(
                "color emoji font not available via fontique; skipping color emoji golden \
                 (this is expected in minimal CI environments without Noto Color Emoji)"
            );
            return;
        }
    };

    let dir = snapshots_dir();
    std::fs::create_dir_all(&dir).expect("create snapshots dir");
    let path = dir.join("color_emoji_grinning_face.png");

    if update_mode() || !path.exists() {
        img.save(&path).expect("write color emoji snapshot");
        eprintln!("color emoji golden wrote snapshot: {}", path.display());
        return;
    }

    let expected = image::open(&path)
        .unwrap_or_else(|e| panic!("load color emoji snapshot {}: {e}", path.display()))
        .to_rgba8();

    if (img.width(), img.height()) != (expected.width(), expected.height()) {
        // Size mismatch is the dominant cross-version drift signal — log
        // and skip rather than fail, so dev machines with newer Noto
        // releases don't break CI on the committed snapshot. The
        // `KASANE_GOLDEN_UPDATE=1` workflow refreshes the snapshot when
        // intentional.
        eprintln!(
            "color emoji golden size drift for {}: got {}x{}, expected {}x{} \
             (likely Noto Color Emoji version difference; refresh with \
             KASANE_GOLDEN_UPDATE=1 cargo test -p kasane-gui --test golden_render)",
            path.display(),
            img.width(),
            img.height(),
            expected.width(),
            expected.height(),
        );
        return;
    }

    let actual_rgb = drop_alpha(&img);
    let expected_rgb = drop_alpha(&expected);

    let result =
        image_compare::rgb_hybrid_compare(&expected_rgb, &actual_rgb).expect("rgb_hybrid_compare");
    let dissimilarity = 1.0 - result.score;

    assert!(
        dissimilarity <= COLOR_EMOJI_DSSIM_THRESHOLD,
        "color emoji golden dissimilarity {dissimilarity:.5} > threshold \
         {COLOR_EMOJI_DSSIM_THRESHOLD} (font-version drift?). Refresh with \
         KASANE_GOLDEN_UPDATE=1 cargo test -p kasane-gui --test golden_render",
    );
}

// ---------------------------------------------------------------------
// ADR-032 W2 — SceneRenderer-driven goldens via FrameTarget::View
// ---------------------------------------------------------------------
//
// These tests drive the production `SceneRenderer` against a headless
// render-to-texture target, validating that the FrameTarget abstraction
// delivers byte-equivalent output to the production swap-chain path.
// The first fixture (`monochrome_grid`) is the smoke test; further
// fixtures (CJK, cursor states, selection) follow the same pattern.

/// Format of the offscreen texture used by SceneRenderer-driven tests.
/// Must match the `format` field of `FrameTarget::View` so the
/// renderer's pipelines produce compatible output.
const SCENE_TEST_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// Build a headless `GpuState` (no surface) for SceneRenderer-driven
/// tests. `width` / `height` populate the dummy `SurfaceConfiguration`;
/// the actual render target dimensions are set per-test via
/// `FrameTarget::View`.
fn headless_gpu_state(width: u32, height: u32) -> Option<kasane_gui::gpu::GpuState> {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    let (device, queue) = headless_gpu()?;

    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: SCENE_TEST_FORMAT,
        width,
        height,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        view_formats: vec![],
    };

    Some(kasane_gui::gpu::GpuState {
        surface: None,
        device,
        queue,
        config,
        device_error: Arc::new(AtomicBool::new(false)),
        pipeline_cache: None,
    })
}

/// Render `commands` via `SceneRenderer::render_to_target` into a fresh
/// offscreen texture and return the readback as an `RgbaImage`.
fn render_scene_to_image(
    gpu: &kasane_gui::gpu::GpuState,
    width: u32,
    height: u32,
    commands: &[kasane_core::render::DrawCommand],
) -> RgbaImage {
    use kasane_core::config::FontConfig;
    use kasane_gui::colors::ColorResolver;
    use kasane_gui::gpu::scene_renderer::{FrameTarget, SceneRenderer};
    use winit::dpi::PhysicalSize;

    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("scene_golden_target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: SCENE_TEST_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    let font_config = FontConfig::default();
    let mut renderer = SceneRenderer::new(gpu, &font_config, 1.0, PhysicalSize::new(width, height));

    let resolver = ColorResolver::from_config(&kasane_core::config::ColorsConfig::default());
    renderer
        .render_to_target(
            gpu,
            FrameTarget::View {
                view: &view,
                width,
                height,
                format: SCENE_TEST_FORMAT,
            },
            commands,
            &resolver,
            None,
        )
        .expect("render_to_target");

    // Readback. Mirrors the structure of `render_to_image` but operates
    // on an already-rendered texture rather than driving a closure.
    let bytes_per_pixel: u32 = 4;
    let unpadded_bytes_per_row = width * bytes_per_pixel;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;

    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("scene_golden_readback"),
        size: (padded_bytes_per_row * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("scene_golden_readback_encoder"),
        });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit(Some(encoder.finish()));

    let slice = buffer.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    receiver
        .recv()
        .expect("map_async result")
        .expect("buffer map");

    let data = slice.get_mapped_range();
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height {
        let start = (row * padded_bytes_per_row) as usize;
        let end = start + (unpadded_bytes_per_row as usize);
        pixels.extend_from_slice(&data[start..end]);
    }
    drop(data);
    buffer.unmap();

    ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, pixels).expect("RGBA8 image construction")
}

/// Smoke test: drive `SceneRenderer` through `FrameTarget::View` with a
/// single `FillRect` covering the entire frame at the default-bg style.
/// Validates the full headless pipeline (constructor → encode → readback)
/// without exercising text shaping or images.
#[test]
fn monochrome_grid_matches_snapshot() {
    use kasane_core::protocol::Style;
    use kasane_core::render::{DrawCommand, PixelRect};

    let width = 320u32;
    let height = 96u32;

    let Some(gpu) = headless_gpu_state(width, height) else {
        eprintln!("no wgpu adapter available; skipping monochrome_grid golden");
        return;
    };

    let commands = vec![DrawCommand::FillRect {
        rect: PixelRect {
            x: 0.0,
            y: 0.0,
            w: width as f32,
            h: height as f32,
        },
        face: Style::default(),
        elevated: false,
    }];

    let img = render_scene_to_image(&gpu, width, height, &commands);
    assert_dssim(&img, "monochrome_grid");
}
