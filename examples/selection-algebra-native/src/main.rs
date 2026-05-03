//! ADR-035 dogfood: SelectionSet algebra from a consumer's perspective.
//!
//! This example imports the new `kasane_core::state::{selection,
//! selection_set}` modules and exercises every operation that a future
//! plugin author would reach for. It runs as a standalone binary with
//! no Kasane runtime — the goal is to validate that the public API is
//! ergonomic and complete enough to support real plugin scenarios.
//!
//! Run with `cargo run` from `examples/selection-algebra-native/`.

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

    println!("\n=== Done ===");
}
