//! ADR-035 dogfood: SelectionSet algebra from a consumer's perspective.
//!
//! This example imports the new `kasane_core::state::{selection,
//! selection_set}` modules and exercises every operation that a future
//! plugin author would reach for. It runs as a standalone binary with
//! no Kasane runtime — the goal is to validate that the public API is
//! ergonomic and complete enough to support real plugin scenarios.
//!
//! Run with `cargo run` from `examples/selection-algebra-native/`.

use std::sync::Arc;

use kasane_core::history::{HistoryBackend, InMemoryRing, Time, VersionId};
use kasane_core::state::selection::{BufferId, BufferPos, BufferVersion, Selection};
use kasane_core::state::selection_set::SelectionSet;
use kasane_plugin_model::PluginId;

fn buf() -> BufferId {
    BufferId::new("dogfood-buffer")
}

fn ver() -> BufferVersion {
    BufferVersion::INITIAL
}

/// Build a half-open selection on a single line.
fn line_sel(line: u32, start: u32, end: u32) -> Selection {
    Selection::new(BufferPos::new(line, start), BufferPos::new(line, end))
}

fn print_set(label: &str, s: &SelectionSet) {
    print!("  {label:24} = {{");
    for (i, sel) in s.iter().enumerate() {
        if i > 0 {
            print!(", ");
        }
        print!(
            "L{}[{}..{}]",
            sel.min().line,
            sel.min().column,
            sel.max().column
        );
    }
    println!("}} ({} selection{})", s.len(), if s.len() == 1 { "" } else { "s" });
}

fn main() {
    println!("=== ADR-035 SelectionSet dogfood ===\n");

    // --- Construction -------------------------------------------------------
    println!("Construction:");
    let empty = SelectionSet::empty(buf(), ver());
    print_set("empty", &empty);

    let single = SelectionSet::singleton(line_sel(0, 5, 10), buf(), ver());
    print_set("singleton(L0[5..10])", &single);

    // from_iter normalises: sorts, then coalesces overlapping/adjacent.
    let multi = SelectionSet::from_iter(
        vec![
            line_sel(2, 0, 5),
            line_sel(0, 0, 5),
            line_sel(0, 4, 8),  // overlaps with L0[0..5] → coalesce to L0[0..8]
        ],
        buf(),
        ver(),
    );
    print_set("from_iter(unsorted)", &multi);
    println!();

    // --- Set algebra --------------------------------------------------------
    println!("Set algebra:");
    let a = SelectionSet::from_iter(
        vec![line_sel(0, 0, 10), line_sel(2, 0, 10)],
        buf(),
        ver(),
    );
    let b = SelectionSet::from_iter(
        vec![line_sel(0, 5, 15), line_sel(3, 0, 5)],
        buf(),
        ver(),
    );
    print_set("a", &a);
    print_set("b", &b);

    print_set("a ∪ b", &a.union(&b));
    print_set("a ∩ b", &a.intersect(&b));
    print_set("a − b", &a.difference(&b));
    print_set("a △ b", &a.symmetric_difference(&b));
    println!("  a.is_disjoint(&b)        = {}", a.is_disjoint(&b));
    println!();

    // --- Pointwise transformation ------------------------------------------
    println!("Transformation:");
    let shifted = a.map(|sel| {
        Selection::new(
            BufferPos::new(sel.anchor.line + 10, sel.anchor.column),
            BufferPos::new(sel.cursor.line + 10, sel.cursor.column),
        )
    });
    print_set("a.map(line += 10)", &shifted);

    let only_long = a.filter(|sel| sel.max().column - sel.min().column >= 8);
    print_set("a.filter(width≥8)", &only_long);

    let split_in_two = a.flat_map(|sel| {
        let mid = (sel.min().column + sel.max().column) / 2;
        vec![
            Selection::new(sel.min(), BufferPos::new(sel.min().line, mid)),
            Selection::new(BufferPos::new(sel.min().line, mid), sel.max()),
        ]
    });
    // The split halves are adjacent → from_iter coalesces them back.
    print_set("a.flat_map(split-mid)", &split_in_two);
    println!();

    // --- Persistence (named registers) -------------------------------------
    println!("Persistence (named save/load):");
    let plugin = PluginId("dogfood".into());
    a.save(plugin.clone(), "saved-a").expect("save");
    println!("  a.save(\"saved-a\")        = ok");

    let loaded = SelectionSet::load(plugin.clone(), "saved-a", buf()).expect("load");
    print_set("load(\"saved-a\")", &loaded);

    println!(
        "  load(\"missing\")          = {:?}",
        SelectionSet::load(plugin, "missing", buf()).map(|_| ())
    );
    println!();

    // --- Algebraic-law spot checks ----------------------------------------
    println!("Algebraic laws (spot check):");
    let assert = |name: &str, ok: bool| {
        println!("  {name:32} = {}", if ok { "✓" } else { "✗" });
    };

    assert("a ∪ a == a (idempotency)", a.union(&a) == a);
    assert("a ∩ a == a (idempotency)", a.intersect(&a) == a);
    assert("a − a == ∅", a.difference(&a).is_empty());
    assert("a ∪ b == b ∪ a (comm.)", a.union(&b) == b.union(&a));
    assert("a ∩ b == b ∩ a (comm.)", a.intersect(&b) == b.intersect(&a));
    assert(
        "a ∪ (a ∩ b) == a (absorpt.)",
        a.union(&a.intersect(&b)) == a,
    );
    assert(
        "a ∩ (b ∪ c) == (a∩b)∪(a∩c)",
        a.intersect(&b.union(&single)) == a.intersect(&b).union(&a.intersect(&single)),
    );

    println!();

    // --- Time-aware history (ADR-035 §2) -----------------------------------
    println!("Time-aware history (InMemoryRing):");
    let ring = InMemoryRing::with_capacity(3);

    // Commit 4 snapshots; the ring's capacity-3 should evict the first.
    let v0 = ring.commit(
        Arc::from("alpha"),
        SelectionSet::singleton(line_sel(0, 0, 5), buf(), BufferVersion(0)),
        buf(),
        BufferVersion(0),
    );
    let v1 = ring.commit(
        Arc::from("beta"),
        SelectionSet::singleton(line_sel(1, 0, 5), buf(), BufferVersion(1)),
        buf(),
        BufferVersion(1),
    );
    let v2 = ring.commit(
        Arc::from("gamma"),
        SelectionSet::singleton(line_sel(2, 0, 5), buf(), BufferVersion(2)),
        buf(),
        BufferVersion(2),
    );
    let v3 = ring.commit(
        Arc::from("delta"),
        SelectionSet::singleton(line_sel(3, 0, 5), buf(), BufferVersion(3)),
        buf(),
        BufferVersion(3),
    );

    println!(
        "  earliest = {:?}, current = {:?}",
        ring.earliest_version(),
        ring.current_version()
    );

    // Query each version. v0 evicted by FIFO at capacity 3.
    let lookup = |v: VersionId| match ring.snapshot(v) {
        Ok(snap) => format!(
            "text=\"{}\", sel @ L{}",
            snap.text,
            snap.selection.primary().unwrap().min().line
        ),
        Err(e) => format!("Err({:?})", e),
    };
    println!("  v0 ({}) = {}", v0.0, lookup(v0));
    println!("  v1 ({}) = {}", v1.0, lookup(v1));
    println!("  v2 ({}) = {}", v2.0, lookup(v2));
    println!("  v3 ({}) = {}", v3.0, lookup(v3));

    // Time::Now → latest version.
    let now_v = match Time::Now {
        Time::Now => ring.current_version(),
        Time::At(v) => v,
    };
    println!("  Time::Now resolves to v{} → {}", now_v.0, lookup(now_v));

    // Plugin pattern: walk versions to inspect selection history.
    println!("  walking earliest..current:");
    let mut v = ring.earliest_version();
    while v <= ring.current_version() {
        if let Ok(snap) = ring.snapshot(v) {
            println!(
                "    v{}: text={:?}, primary L{}[{}..{}]",
                v.0,
                &*snap.text,
                snap.selection.primary().unwrap().min().line,
                snap.selection.primary().unwrap().min().column,
                snap.selection.primary().unwrap().max().column,
            );
        }
        v.0 += 1;
    }

    println!("\n=== Done ===");
}
