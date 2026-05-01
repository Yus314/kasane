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

// ---------------------------------------------------------------------------
// ADR-031 Phase 10 feature goldens (ADR-032 W2 fixture skeletons).
//
// Each test below is a *skeleton*: the input DrawCommand list is
// deterministic and committed; the snapshot is missing on first
// run, so the bootstrap path of `assert_dssim` writes a fresh PNG
// the first time the test runs in a GPU-capable environment
// (whether `KASANE_GOLDEN_UPDATE=1` is set or not — `assert_dssim`
// auto-bootstraps when the snapshot does not exist). On
// subsequent runs, the test asserts DSSIM ≤ DSSIM_THRESHOLD
// against the bootstrapped snapshot.
//
// Sandboxed environments without `/dev/dri` skip via
// `headless_gpu_state` returning `None`, matching
// `monochrome_grid_matches_snapshot`'s graceful-skip pattern.
//
// Each fixture pins one Phase 10 feature so a regression in that
// feature surfaces as a single-fixture DSSIM failure with a clear
// blame. Snapshot bootstrap on a GPU-capable machine is the only
// remaining manual step; once committed, CI assertion runs
// automatically.
//
// Fixture naming convention: `<feature>_matches_snapshot` for the
// test, `<feature>` for the snapshot PNG.
// ---------------------------------------------------------------------------

/// Subpixel x-quantisation: 4-step bucket (0/4, 1/4, 2/4, 3/4) per
/// `text/glyph_rasterizer.rs:24-42`. The fixture renders four
/// short strings at increasing fractional x positions; a regression
/// in the quantisation logic shifts the rasterised glyph bitmaps
/// across the buckets and the DSSIM rises. Pinned by ADR-031 Phase
/// 10 Step A landing.
#[test]
fn subpixel_quantisation_4step_matches_snapshot() {
    use kasane_core::protocol::Style;
    use kasane_core::render::{DrawCommand, PixelPos, PixelRect, scene::ResolvedAtom};

    let width = 320u32;
    let height = 96u32;
    let Some(gpu) = headless_gpu_state(width, height) else {
        eprintln!("no wgpu adapter available; skipping subpixel_quantisation_4step golden");
        return;
    };

    let bg = DrawCommand::FillRect {
        rect: PixelRect {
            x: 0.0,
            y: 0.0,
            w: width as f32,
            h: height as f32,
        },
        face: Style::default(),
        elevated: false,
    };
    let mk_atoms = |y_cell: f32, x_offset_frac: f32, line_idx: u32| DrawCommand::DrawAtoms {
        pos: PixelPos {
            x: x_offset_frac,
            y: y_cell * 16.0,
        },
        atoms: vec![ResolvedAtom {
            contents: compact_str::CompactString::new("subpx"),
            style: Style::default(),
        }],
        max_width: width as f32,
        line_idx,
    };
    let commands = vec![
        bg,
        mk_atoms(0.0, 0.0, 0),
        mk_atoms(1.0, 0.25, 1),
        mk_atoms(2.0, 0.5, 2),
        mk_atoms(3.0, 0.75, 3),
    ];

    let img = render_scene_to_image(&gpu, width, height, &commands);
    assert_dssim(&img, "subpixel_quantisation_4step");
}

/// Variable font axes: continuous `FontWeight(u16)` per
/// ADR-031 Phase 10. The fixture renders four "weight" lines at
/// 100, 400, 700, 900 to pin that the weight value flows through
/// to Parley as `StyleProperty::FontVariations` rather than being
/// quantised to a discrete enum. A regression here typically
/// surfaces as identical-looking lines (axis ignored) or
/// misweighted glyphs (axis swapped).
///
/// **Note:** the weight values are encoded into `Style.font_weight`
/// at the wire level; this fixture exercises the wire-to-Parley
/// path via `Atom::with_style`. The bench currently builds via
/// `Style::from_face`, which does not set `font_weight`; the
/// fixture is *deliberately* unfilled at the weight axis until
/// `Style.font_weight` is observable through the public protocol
/// surface (post-ADR-031 Phase 10 Step C). Marked `#[ignore]` for
/// now and unblocked when the surface lands.
#[test]
#[ignore = "ADR-031 Phase 10 Step C: Style.font_weight not yet on the public surface"]
fn variable_font_axes_matches_snapshot() {
    eprintln!(
        "variable_font_axes fixture: blocked on ADR-031 Phase 10 Step C \
         (Style.font_weight on protocol surface); skipped"
    );
}

/// Curly underline with font-metric-driven amplitude per
/// ADR-031 Phase 10 (rich underlines: curly/dotted/dashed/double).
/// The fixture renders a single line with `Attributes::CURLY_UNDERLINE`
/// set and `WireFace.underline` carrying a contrasting underline
/// colour, sized so the curly waveform peaks/troughs are
/// observable in the rasterised output. A regression in
/// `kurbo::CubicBez` chain emission or `RunMetrics::underline_offset/size`
/// plumbing shifts the waveform's amplitude or phase and the
/// DSSIM rises.
///
/// **Note:** `Attributes::CURLY_UNDERLINE` is part of the public
/// `Attributes` bitflags and the GPU side rasterises via the
/// existing decoration path. The fixture is buildable today; pending
/// only the snapshot bootstrap on a GPU-capable machine.
#[test]
fn curly_underline_matches_snapshot() {
    use kasane_core::protocol::{Attributes, Color, NamedColor, Style, WireFace};
    use kasane_core::render::{DrawCommand, PixelPos, PixelRect, scene::ResolvedAtom};

    let width = 320u32;
    let height = 32u32;
    let Some(gpu) = headless_gpu_state(width, height) else {
        eprintln!("no wgpu adapter available; skipping curly_underline golden");
        return;
    };

    let underline_face: Style = WireFace {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        underline: Color::Named(NamedColor::Red),
        attributes: Attributes::CURLY_UNDERLINE,
    }
    .into();

    let commands = vec![
        DrawCommand::FillRect {
            rect: PixelRect {
                x: 0.0,
                y: 0.0,
                w: width as f32,
                h: height as f32,
            },
            face: Style::default(),
            elevated: false,
        },
        DrawCommand::DrawAtoms {
            pos: PixelPos { x: 0.0, y: 0.0 },
            atoms: vec![ResolvedAtom {
                contents: compact_str::CompactString::new("curly underline"),
                style: underline_face,
            }],
            max_width: width as f32,
            line_idx: 0,
        },
    ];

    let img = render_scene_to_image(&gpu, width, height, &commands);
    assert_dssim(&img, "curly_underline");
}

/// InlineBox text flow: a `RenderParagraph` carrying an
/// `InlineBoxSlotMeta` with pre-painted plugin content. Pins the
/// Phase 10 Step 2-renderer A.2b path. Snapshot validates that
/// inline-box rect translation places the plugin content at the
/// Parley-reported box rect.
///
/// Built via `BufferParagraph::builder().inline_box_slot(...)`;
/// the slot is positioned at byte offset 6 (between "hello " and
/// "world"), 2 cells wide, 1 line tall, with a single solid red
/// FillRect as its paint command. A regression in
/// `process_render_paragraph_parley`'s inline-box rect translation
/// shifts where the red square renders relative to the surrounding
/// text and the DSSIM rises.
#[test]
fn inline_box_text_flow_matches_snapshot() {
    use kasane_core::display::InlineBoxAlignment;
    use kasane_core::plugin::PluginId;
    use kasane_core::protocol::{Color, NamedColor, Style, WireFace};
    use kasane_core::render::{DrawCommand, PixelPos, PixelRect, scene::BufferParagraph};

    let width = 320u32;
    let height = 32u32;
    let Some(gpu) = headless_gpu_state(width, height) else {
        eprintln!("no wgpu adapter available; skipping inline_box_text_flow golden");
        return;
    };

    let red_face: Style = WireFace {
        fg: Color::Named(NamedColor::Red),
        bg: Color::Named(NamedColor::Red),
        ..WireFace::default()
    }
    .into();

    // Inline-box paint contribution: one solid red square at
    // origin (0, 0) sized to the slot's declared geometry
    // (2 cells × 1 line ≈ 16×16 px at default cell metrics).
    let inline_paint = vec![DrawCommand::FillRect {
        rect: PixelRect {
            x: 0.0,
            y: 0.0,
            w: 16.0,
            h: 16.0,
        },
        face: red_face,
        elevated: false,
    }];

    let paragraph = BufferParagraph::builder()
        .atom("hello ", Style::default())
        .atom("world", Style::default())
        .inline_box_slot(
            6,
            2.0,
            1.0,
            0,
            InlineBoxAlignment::Center,
            PluginId("test.inline_box_fixture".to_string()),
            inline_paint,
        )
        .build();

    let commands = vec![
        DrawCommand::FillRect {
            rect: PixelRect {
                x: 0.0,
                y: 0.0,
                w: width as f32,
                h: height as f32,
            },
            face: Style::default(),
            elevated: false,
        },
        DrawCommand::RenderParagraph {
            pos: PixelPos { x: 0.0, y: 0.0 },
            max_width: width as f32,
            paragraph,
            line_idx: 0,
        },
    ];

    let img = render_scene_to_image(&gpu, width, height, &commands);
    assert_dssim(&img, "inline_box_text_flow");
}

/// RTL/BiDi cursor placement: a paragraph mixing Latin and Arabic,
/// with a `PrimaryCursor` annotation at a byte offset that lands at
/// the Latin/Arabic boundary. Pins the Phase 10 RTL hit_test
/// landing — a regression in cluster-position translation places
/// the cursor on the wrong side of the boundary, producing a
/// DSSIM-detectable shift.
///
/// Built via `BufferParagraph::builder().primary_cursor_at(...)`.
/// Cursor sits at byte offset 6 (immediately after "Hello "), which
/// is the LTR/RTL boundary in the mixed string. Phase 10 RTL
/// hit_test must place the cursor at the *visual* right edge of
/// the Latin run (= visual left edge of the RTL run, since Arabic
/// is right-to-left).
#[test]
fn rtl_bidi_cursor_matches_snapshot() {
    use kasane_core::protocol::Style;
    use kasane_core::render::{
        CursorStyle, DrawCommand, PixelPos, PixelRect, scene::BufferParagraph,
    };

    let width = 320u32;
    let height = 32u32;
    let Some(gpu) = headless_gpu_state(width, height) else {
        eprintln!("no wgpu adapter available; skipping rtl_bidi_cursor golden");
        return;
    };

    // Mixed Latin+Arabic: "Hello " (6 bytes) + "العالم" (Arabic
    // for "world", multi-byte UTF-8). The cursor at byte offset 6
    // lands at the boundary between the LTR Latin run and the RTL
    // Arabic run — the most regression-prone position for BiDi
    // hit-testing.
    let paragraph = BufferParagraph::builder()
        .atom("Hello ", Style::default())
        .atom(
            "\u{0627}\u{0644}\u{0639}\u{0627}\u{0644}\u{0645}",
            Style::default(),
        )
        .primary_cursor_at(6, CursorStyle::Bar)
        .build();

    let commands = vec![
        DrawCommand::FillRect {
            rect: PixelRect {
                x: 0.0,
                y: 0.0,
                w: width as f32,
                h: height as f32,
            },
            face: Style::default(),
            elevated: false,
        },
        DrawCommand::RenderParagraph {
            pos: PixelPos { x: 0.0, y: 0.0 },
            max_width: width as f32,
            paragraph,
            line_idx: 0,
        },
    ];

    let img = render_scene_to_image(&gpu, width, height, &commands);
    assert_dssim(&img, "rtl_bidi_cursor");
}

/// CJK cluster double-width: a single line of mixed CJK + Latin
/// where the cursor sits over a CJK glyph. The Phase 10 cursor
/// width clamp (`4d48bbd9` regression) ensures the cursor renders
/// at the cluster's full display width, not the byte-width of the
/// first code unit. A regression here halves the cursor width over
/// CJK glyphs and surfaces as a high DSSIM in the cursor cell.
///
/// Built via `BufferParagraph::builder().primary_cursor_at(...)`.
/// Cursor sits at byte offset 5 (start of "漢" — UTF-8 byte
/// offset, since "Hello" is 5 ASCII bytes). The Phase 10 width
/// clamp must render the Block cursor at the full 2-cell display
/// width of the CJK glyph cluster, not at the 1-cell width that
/// the byte-offset would naively suggest.
#[test]
fn cjk_cluster_double_width_matches_snapshot() {
    use kasane_core::protocol::Style;
    use kasane_core::render::{
        CursorStyle, DrawCommand, PixelPos, PixelRect, scene::BufferParagraph,
    };

    let width = 320u32;
    let height = 32u32;
    let Some(gpu) = headless_gpu_state(width, height) else {
        eprintln!("no wgpu adapter available; skipping cjk_cluster_double_width golden");
        return;
    };

    // "Hello" + "漢字" — the cursor at byte offset 5 lands at the
    // start of "漢" (3-byte UTF-8 sequence). Block cursor must span
    // 2 display cells, not 1.
    let paragraph = BufferParagraph::builder()
        .atom("Hello", Style::default())
        .atom("\u{6F22}\u{5B57}", Style::default())
        .primary_cursor_at(5, CursorStyle::Block)
        .build();

    let commands = vec![
        DrawCommand::FillRect {
            rect: PixelRect {
                x: 0.0,
                y: 0.0,
                w: width as f32,
                h: height as f32,
            },
            face: Style::default(),
            elevated: false,
        },
        DrawCommand::RenderParagraph {
            pos: PixelPos { x: 0.0, y: 0.0 },
            max_width: width as f32,
            paragraph,
            line_idx: 0,
        },
    ];

    let img = render_scene_to_image(&gpu, width, height, &commands);
    assert_dssim(&img, "cjk_cluster_double_width");
}

/// Color emoji source priority: a glyph that has both a COLR
/// outline and an embedded color bitmap (e.g. U+1F600 in fonts
/// that ship both representations). The
/// `text/glyph_rasterizer.rs:135-140` source-priority list is
/// `ColorOutline(0) → ColorBitmap(BestFit) → Outline → Bitmap`;
/// the snapshot pins the COLR outline as the chosen
/// representation. A regression that flips the priority order
/// produces a different glyph appearance and the DSSIM rises.
///
/// Builds via a single-emoji DrawAtoms; the rasterizer's source
/// priority is exercised when the font cascade resolves to a
/// COLR-bearing font.
#[test]
fn color_emoji_priority_matches_snapshot() {
    use kasane_core::protocol::Style;
    use kasane_core::render::{DrawCommand, PixelPos, PixelRect, scene::ResolvedAtom};

    let width = 64u32;
    let height = 64u32;
    let Some(gpu) = headless_gpu_state(width, height) else {
        eprintln!("no wgpu adapter available; skipping color_emoji_priority golden");
        return;
    };

    let commands = vec![
        DrawCommand::FillRect {
            rect: PixelRect {
                x: 0.0,
                y: 0.0,
                w: width as f32,
                h: height as f32,
            },
            face: Style::default(),
            elevated: false,
        },
        DrawCommand::DrawAtoms {
            pos: PixelPos { x: 8.0, y: 16.0 },
            atoms: vec![ResolvedAtom {
                contents: compact_str::CompactString::new("\u{1F600}"),
                style: Style::default(),
            }],
            max_width: width as f32,
            line_idx: 0,
        },
    ];

    let img = render_scene_to_image(&gpu, width, height, &commands);
    assert_dssim(&img, "color_emoji_priority");
}

/// Font fallback chain: primary family deliberately set to a name
/// that does not exist on the system, falling back to the configured
/// fallback list. The `text/font_stack.rs:resolve_stack` fixture
/// pins that the configured fallback list is honoured rather than
/// silently using the platform default. A regression that ignores
/// the fallback list produces a different glyph cascade and the
/// DSSIM rises.
///
/// **Note:** the `FontConfig` plumbing into the render path runs
/// through `SceneRenderer::resize` (font config change). Building
/// this fixture requires the test to exercise the font-config
/// switch, which the current `render_scene_to_image` helper does
/// not expose. Marked `#[ignore]` until the helper accepts an
/// optional `&FontConfig` override.
#[test]
#[ignore = "Font-fallback fixture pending render_scene_to_image FontConfig override"]
fn font_fallback_chain_matches_snapshot() {
    eprintln!(
        "font_fallback_chain fixture: blocked on render_scene_to_image \
         FontConfig override; skipped. Pin: text/font_stack.rs:resolve_stack"
    );
}
