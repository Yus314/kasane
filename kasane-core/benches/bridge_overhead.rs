//! ADR-034 / ADR-037 bridge overhead bench.
//!
//! Measures `crate::display_algebra::bridge::resolve_via_algebra`
//! against representative directive workloads. Legacy
//! `display::resolve` was deleted under ADR-037 Phase 5; the bench
//! retains the same workload taxonomy but runs only the algebra
//! path. Historical comparison numbers are recorded in
//! `docs/decisions.md` ADR-037 §Acceptance criteria #6.
//!
//! Workload shapes are chosen to span the call patterns the bridge
//! sees in production:
//!
//! - **`hide_only`** — 16 plugins all emitting `Hide`. Pure legacy
//!   path; measures the partition+forwarding overhead alone.
//! - **`fold_only`** — 8 plugins emitting `Fold` over disjoint ranges.
//!   Pure legacy path; the algebra path is empty.
//! - **`mixed_legacy`** — Hide + Fold + EditableVirtualText, the
//!   exact subset legacy already handles. Measures the partition cost
//!   when the algebra step bottoms out trivially.
//! - **`mixed_pass_through`** — Pass-through variants (InsertInline,
//!   StyleInline, Gutter, InlineBox) only. Pure algebra path.
//! - **`mixed_full`** — Realistic mix: Hide + Fold + Gutter +
//!   InsertInline + StyleInline. Hits both code paths concurrently.
//!
//! Acceptance criterion (per ADR-024 perceptual imperceptibility):
//! the bridge must not push `salsa_scaling/full_frame/80x24` past
//! 70 µs warm. The numbers reported here are the *isolated* directive-
//! resolution cost; consumers should add ≤10% to the warm-frame
//! budget for the full pipeline.

use compact_str::CompactString;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hint::black_box;

use kasane_core::display::{
    DirectiveSet, DisplayDirective, GutterSide, InlineBoxAlignment, InlineInteraction,
};
use kasane_core::display_algebra::bridge::resolve_via_algebra;
use kasane_core::element::Element;
use kasane_core::plugin::PluginId;
use kasane_core::protocol::{Atom, WireFace};

const LINE_COUNT: usize = 24;

fn pid(s: &str) -> PluginId {
    PluginId(s.to_string())
}

fn atom(s: &str) -> Atom {
    Atom::with_style(
        CompactString::from(s),
        kasane_core::protocol::Style::default(),
    )
}

// =============================================================================
// Workload builders
// =============================================================================

fn hide_only_set() -> DirectiveSet {
    let mut set = DirectiveSet::default();
    for i in 0..LINE_COUNT {
        set.push(
            DisplayDirective::Hide { range: i..(i + 1) },
            0,
            pid(&format!("hide{}", i)),
        );
    }
    set
}

fn fold_only_set() -> DirectiveSet {
    let mut set = DirectiveSet::default();
    for i in 0..(LINE_COUNT / 3) {
        let start = i * 3;
        set.push(
            DisplayDirective::Fold {
                range: start..(start + 2),
                summary: vec![atom("// folded")],
            },
            0,
            pid(&format!("fold{}", i)),
        );
    }
    set
}

fn mixed_legacy_set() -> DirectiveSet {
    let mut set = DirectiveSet::default();
    set.push(DisplayDirective::Hide { range: 0..3 }, 0, pid("h1"));
    set.push(DisplayDirective::Hide { range: 8..10 }, 0, pid("h2"));
    set.push(
        DisplayDirective::Fold {
            range: 4..7,
            summary: vec![atom("F")],
        },
        0,
        pid("f1"),
    );
    set.push(
        DisplayDirective::Fold {
            range: 12..16,
            summary: vec![atom("F2")],
        },
        0,
        pid("f2"),
    );
    set.push(
        DisplayDirective::EditableVirtualText {
            after: 18,
            content: vec![atom("editable")],
            editable_spans: vec![],
        },
        0,
        pid("evt"),
    );
    set
}

fn mixed_pass_through_set() -> DirectiveSet {
    let mut set = DirectiveSet::default();
    for line in 0..LINE_COUNT {
        // Two pass-through directives per line: an inline insertion
        // and a line-wide style.
        set.push(
            DisplayDirective::InsertInline {
                line,
                byte_offset: 5,
                content: vec![atom("X")],
                interaction: InlineInteraction::None,
            },
            0,
            pid(&format!("ins{}", line)),
        );
        set.push(
            DisplayDirective::StyleLine {
                line,
                face: WireFace::default(),
                z_order: 0,
            },
            0,
            pid(&format!("sty{}", line)),
        );
    }
    set
}

fn mixed_full_set() -> DirectiveSet {
    let mut set = DirectiveSet::default();
    // Some hides (legacy path).
    set.push(DisplayDirective::Hide { range: 0..2 }, 0, pid("h1"));
    set.push(DisplayDirective::Hide { range: 20..22 }, 0, pid("h2"));
    // A fold (legacy path).
    set.push(
        DisplayDirective::Fold {
            range: 5..10,
            summary: vec![atom("// folded section")],
        },
        0,
        pid("f"),
    );
    // Pass-through gutter on every visible line.
    for line in 0..LINE_COUNT {
        set.push(
            DisplayDirective::Gutter {
                line,
                side: GutterSide::Left,
                content: Element::Empty,
                priority: 0,
            },
            0,
            pid(&format!("g{}", line)),
        );
    }
    // Pass-through inline style on a few lines.
    for line in [3, 11, 14, 17] {
        set.push(
            DisplayDirective::StyleInline {
                line,
                byte_range: 0..10,
                face: WireFace::default(),
            },
            0,
            pid(&format!("si{}", line)),
        );
    }
    // One inline box.
    set.push(
        DisplayDirective::InlineBox {
            line: 12,
            byte_offset: 4,
            width_cells: 2.0,
            height_lines: 1.0,
            box_id: 42,
            alignment: InlineBoxAlignment::Center,
        },
        0,
        pid("box"),
    );
    set
}

// =============================================================================
// Bench groups
// =============================================================================

type WorkloadBuilder = fn() -> DirectiveSet;

fn bench_workloads(c: &mut Criterion) {
    let workloads: &[(&str, WorkloadBuilder)] = &[
        ("hide_only", hide_only_set),
        ("fold_only", fold_only_set),
        ("mixed_legacy", mixed_legacy_set),
        ("mixed_pass_through", mixed_pass_through_set),
        ("mixed_full", mixed_full_set),
    ];

    let mut bridge = c.benchmark_group("bridge_overhead/bridge");
    bridge.sample_size(50);
    for (label, builder) in workloads {
        let set = builder();
        bridge.bench_function(BenchmarkId::from_parameter(*label), |b| {
            b.iter(|| {
                let out = resolve_via_algebra(black_box(&set), black_box(LINE_COUNT));
                black_box(out);
            });
        });
    }
    bridge.finish();
}

criterion_group!(bridge_overhead, bench_workloads);
criterion_main!(bridge_overhead);
