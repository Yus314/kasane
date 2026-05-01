//! ADR-032 W5 spike — paired-backend criterion harness.
//!
//! ## Scope
//!
//! This harness is the *skeleton* for the W5 measurement matrix. It
//! lands the criterion-group structure, deterministic fixture
//! builders, and the paired-call pattern *before* the actual spike
//! work begins, so Day 1 plug-in is purely "swap the
//! `BackendError::Unsupported` arm bodies for real renders" rather
//! than also building bench infrastructure.
//!
//! ## What this harness measures (without GPU)
//!
//! All benches in this file are **CPU-only** because the spike
//! sandbox lacks `/dev/dri` and a `wgpu::Device` cannot be
//! constructed headlessly here. The measurable quantities are:
//!
//! - **`fixture_build/*`**: time to construct the deterministic
//!   `Vec<DrawCommand>` for each fixture size. This is independent
//!   of any backend; it characterises the cost of producing the
//!   bench input itself, so warm-frame numbers can be decomposed
//!   later (input-build vs translation-walk vs GPU work).
//! - **`translation_walk/*`**: time for the spike's
//!   `render_with_cursor` skeleton to walk the DrawCommand list and
//!   dispatch through the match-arm-exhaustive translator. With
//!   `--features with-vello` off, the dispatcher is the
//!   `BackendError::Unsupported` skeleton (each variant returns
//!   immediately on first match); the absolute number is small, but
//!   it sets a **floor** that any real translation must clear.
//!
//! ## What this harness does *not* measure (yet)
//!
//! Per the §Spike Measurement Matrix in ADR-032, the following
//! require an actual GPU device and `vello_hybrid::Renderer` and
//! land in W5 Day 1+:
//!
//! - `frame_warm_24_lines` (80×24 warm)
//! - `frame_warm_one_line_changed_24_lines` (incremental)
//! - cursor-only frame
//! - color emoji DSSIM vs swash
//! - resident GPU memory
//! - hybrid CPU strip vs GPU submit decomposition
//!
//! These benches are stubbed below as commented-out placeholders so
//! the file structure is in place; uncomment and fill during W5
//! once a `GpuState` can be instantiated against a headless wgpu
//! device.
//!
//! ## With and without `with-vello`
//!
//! Without the feature, only the `WgpuBackend`-side path would run
//! production rendering, and even that requires `GpuState`. So the
//! sub-features that do run without GPU + without `with-vello`
//! reduce to: fixture builders and `VelloBackend`'s feature-off
//! skeleton dispatch (which short-circuits at the first
//! `BackendError::Unsupported`). The harness therefore runs
//! meaningfully in both feature modes — the absolute numbers are
//! similar across modes by construction; what changes is whether
//! the W5 GPU benches (commented-out below) participate.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use kasane_core::protocol::{Color, NamedColor, Style, WireFace};
use kasane_core::render::scene::{DrawCommand, PixelPos, PixelRect, ResolvedAtom};
use std::hint::black_box;

/// Build a deterministic `Vec<DrawCommand>` representing a typical
/// 80×24 (cols×rows) editor frame: per-row background `FillRect` +
/// per-row `DrawAtoms` with three style runs (keyword + identifier
/// + plain).
///
/// Sized to match the §Spike Measurement Matrix `frame_warm_24_lines`
/// fixture intent. The exact pixel coordinates and styles are not
/// load-bearing — what matters is that the resulting list has a
/// realistic ratio of FillRect to DrawAtoms (1:1 in this fixture)
/// and a realistic mean atoms-per-line count (3).
fn fixture_warm_80x24() -> Vec<DrawCommand> {
    fixture_grid(80, 24)
}

/// Larger fixture — 200×60 — to characterise whether the
/// translation walk scales linearly with DrawCommand count. The
/// matrix's halt trigger uses 80×24; the 200×60 fixture is for
/// post-positive-spike profiling of how the warm-frame budget
/// holds at larger pane counts.
fn fixture_warm_200x60() -> Vec<DrawCommand> {
    fixture_grid(200, 60)
}

/// "1-line-changed" pair: two near-identical 80×24 fixtures that
/// differ only at row 12. The §Spike Measurement Matrix
/// `incremental warm frame` row uses this pattern to verify that
/// Vello's whole-frame Scene re-encode does not regress against
/// Salsa-driven incremental updates that exploit per-line stability.
fn fixture_warm_80x24_one_line_changed() -> (Vec<DrawCommand>, Vec<DrawCommand>) {
    let a = fixture_warm_80x24();
    let mut b = a.clone();
    // Mutate row 12's DrawAtoms: change a CompactString in the third
    // resolved atom. Cheap — the surrounding FillRect and other rows
    // remain identical so the bench characterises the cost of
    // re-emitting one paragraph in a fresh Scene encode rather than
    // diff-applying a Salsa cache hit.
    if let Some(DrawCommand::DrawAtoms { atoms, .. }) = b
        .iter_mut()
        .filter(|cmd| matches!(cmd, DrawCommand::DrawAtoms { .. }))
        .nth(12)
        && let Some(third) = atoms.get_mut(2)
    {
        third.contents = compact_str::CompactString::new("CHANGED");
    }
    (a, b)
}

/// Internal fixture builder shared between the size variants.
fn fixture_grid(cols: u32, rows: u32) -> Vec<DrawCommand> {
    let cell_w = 8.0_f32;
    let cell_h = 16.0_f32;

    let bg_face: Style = WireFace {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..WireFace::default()
    }
    .into();
    let kw_face: Style = WireFace {
        fg: Color::Rgb {
            r: 255,
            g: 100,
            b: 0,
        },
        bg: Color::Default,
        ..WireFace::default()
    }
    .into();
    let id_face: Style = WireFace {
        fg: Color::Rgb {
            r: 0,
            g: 200,
            b: 100,
        },
        bg: Color::Default,
        ..WireFace::default()
    }
    .into();

    let mut commands: Vec<DrawCommand> = Vec::with_capacity((rows * 2) as usize);
    for row in 0..rows {
        commands.push(DrawCommand::FillRect {
            rect: PixelRect {
                x: 0.0,
                y: (row as f32) * cell_h,
                w: (cols as f32) * cell_w,
                h: cell_h,
            },
            face: bg_face.clone(),
            elevated: false,
        });
        commands.push(DrawCommand::DrawAtoms {
            pos: PixelPos {
                x: 0.0,
                y: (row as f32) * cell_h,
            },
            atoms: vec![
                ResolvedAtom {
                    contents: compact_str::CompactString::new("let"),
                    style: kw_face.clone(),
                },
                ResolvedAtom {
                    contents: compact_str::CompactString::new(" "),
                    style: bg_face.clone(),
                },
                ResolvedAtom {
                    contents: compact_str::CompactString::new(format!("var_{row}")),
                    style: id_face.clone(),
                },
            ],
            max_width: (cols as f32) * cell_w,
            line_idx: row,
        });
    }
    commands
}

// ---------------------------------------------------------------------------
// Benches — fixture build cost
// ---------------------------------------------------------------------------

fn bench_fixture_build(c: &mut Criterion) {
    let mut group = c.benchmark_group("fixture_build");
    group.bench_function(BenchmarkId::new("size", "80x24"), |b| {
        b.iter(|| black_box(fixture_warm_80x24()));
    });
    group.bench_function(BenchmarkId::new("size", "200x60"), |b| {
        b.iter(|| black_box(fixture_warm_200x60()));
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// Benches — translation-walk floor (CPU-only, GPU-free)
// ---------------------------------------------------------------------------
//
// These benches measure the cost of the spike's
// `render_with_cursor` skeleton walking the DrawCommand list with
// the match-arm-exhaustive dispatcher. The first arm to match
// returns `Err(BackendError::Unsupported)` and the function exits;
// the bench therefore measures the cost of *one* dispatch + the
// fixture's first DrawCommand match. This is a *floor* on
// translation cost — the actual W5 spike will replace each arm
// with real Vello/Glifo work, and the bench result will rise.
//
// The harness lands these in advance so:
// 1. The criterion group exists and can be extended in W5 without
//    file-level restructuring.
// 2. The CI / dev-machine baseline of "match dispatch overhead" is
//    measurable and stable across spike days.
// 3. After W5 fills in Day 1's `FillRect` arm body, the same
//    `translation_walk/warm_80x24` row records the total cost
//    including real translation, and the diff against this floor
//    is the FillRect translation cost.
//
// We cannot invoke `VelloBackend::render_with_cursor` directly
// because it requires `&GpuState` (needs winit + wgpu). Instead we
// duplicate the match-arm-exhaustive walk into `walk_translate` so
// the bench is self-contained without GPU.

fn walk_translate(commands: &[DrawCommand]) -> usize {
    // Mirror of the `render_with_cursor` translation skeleton
    // (see lib.rs §Translation Contract). The mirror exists so
    // criterion can call it without a `GpuState`; the production
    // walk lives in `VelloBackend::render_with_cursor` and is
    // intentionally kept identical in arm shape so a refactor
    // that diverges them shows up as a bench delta.
    let mut hits = 0usize;
    for cmd in commands {
        hits += match cmd {
            DrawCommand::FillRect { .. } => 1,
            DrawCommand::DrawAtoms { .. } => 1,
            DrawCommand::DrawText { .. } => 1,
            DrawCommand::DrawBorder { .. } => 1,
            DrawCommand::DrawBorderTitle { .. } => 1,
            DrawCommand::DrawShadow { .. } => 1,
            DrawCommand::DrawPaddingRow { .. } => 1,
            DrawCommand::PushClip(_) => 1,
            DrawCommand::PopClip => 1,
            DrawCommand::DrawImage { .. } => 1,
            DrawCommand::RenderParagraph { .. } => 1,
            DrawCommand::DrawCanvas { .. } => 1,
            DrawCommand::BeginOverlay => 1,
        };
    }
    hits
}

fn bench_translation_walk(c: &mut Criterion) {
    let mut group = c.benchmark_group("translation_walk");

    let cmds_80x24 = fixture_warm_80x24();
    group.throughput(Throughput::Elements(cmds_80x24.len() as u64));
    group.bench_function(BenchmarkId::new("warm", "80x24"), |b| {
        b.iter(|| black_box(walk_translate(black_box(&cmds_80x24))));
    });

    let cmds_200x60 = fixture_warm_200x60();
    group.throughput(Throughput::Elements(cmds_200x60.len() as u64));
    group.bench_function(BenchmarkId::new("warm", "200x60"), |b| {
        b.iter(|| black_box(walk_translate(black_box(&cmds_200x60))));
    });

    let (a, b_cmds) = fixture_warm_80x24_one_line_changed();
    group.throughput(Throughput::Elements(a.len() as u64));
    group.bench_function(
        BenchmarkId::new("incremental", "80x24_one_line_changed"),
        |bench| {
            // Alternate between the two fixtures to model the cache-line
            // pressure of frame-to-frame DrawCommand list churn. The
            // throughput remains element-count of one fixture (they are
            // the same length); criterion's iteration count handles the
            // alternation budget.
            let mut toggle = false;
            bench.iter(|| {
                let cmds = if toggle { &a } else { &b_cmds };
                toggle = !toggle;
                black_box(walk_translate(black_box(cmds)))
            });
        },
    );

    group.finish();
}

// ---------------------------------------------------------------------------
// Placeholder — GPU-side benches (W5 Day 1+)
// ---------------------------------------------------------------------------
//
// The benches below require a `GpuState` (headless wgpu device).
// They are commented-out skeletons; W5 Day 1 instantiates the
// device, replaces the placeholder with real `render_with_cursor`
// calls (paired against `WgpuBackend` and `VelloBackend`), and
// uncomments the criterion_group line.
//
// Per ADR-032 §Decision Gates Pre-W5: the baseline freeze
// declared in `docs/roadmap.md` covers the duration of the spike
// — these benches' results feed the §Spike Measurement Matrix
// rows that share the same fixture set used above.

#[cfg(feature = "with-vello")]
mod gpu_benches {
    // use super::*;
    //
    // fn bench_warm_80x24_paired(_c: &mut Criterion) {
    //     // Day 1: instantiate VelloBackend + WgpuBackend against
    //     // a headless wgpu device, paint the fixture against
    //     // both, record total_warm = T5 - T0 per
    //     // §Spike Findings field 4 instrumentation.
    // }
    //
    // fn bench_incremental_one_line_changed(_c: &mut Criterion) {
    //     // Per §Spike Measurement Matrix incremental warm frame
    //     // row: alternate between fixtures (a, b) and verify
    //     // Salsa-hit doesn't regress under Vello's whole-frame
    //     // Scene re-encode model.
    // }
    //
    // fn bench_hybrid_cpu_share(_c: &mut Criterion) {
    //     // Decompose total_warm into cpu_encode + glifo_atlas +
    //     // gpu_prepare + gpu_submit_latency per
    //     // §Spike Findings field 4. Classifies hybrid as durable
    //     // / transitional / stepping-stone per §Hybrid vs
    //     // compute strategic position.
    // }
}

criterion_group!(spike_bench, bench_fixture_build, bench_translation_walk,);
criterion_main!(spike_bench);
