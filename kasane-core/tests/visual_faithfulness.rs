//! Visual Faithfulness witness (§10.2a) — ADR-030 Level 4.
//!
//! From `docs/semantics.md` §10.2a:
//!
//! > Preserving transformations are visually faithful iff the spatial
//! > restructuring is reversible; Fold with a summary line that responds
//! > to an unfold command satisfies this because the fold toggle is a
//! > single interaction.

use kasane_core::display::{BufferLine, DisplayDirective, DisplayMap, FoldToggleState};
use kasane_core::protocol::{Atom, Face};

#[test]
fn fold_toggle_recovers_all_folded_lines() {
    let line_count = 30;
    let fold_range = 10..20;
    let directives = vec![DisplayDirective::Fold {
        range: fold_range.clone(),
        summary: vec![Atom::plain("…")],
    }];

    // Before toggle: folded lines map to a single summary display line.
    let dm_before = DisplayMap::build(line_count, &directives);
    // Lines 10..20 share a single display line (the fold summary).
    let summary_dl = dm_before.buffer_to_display(BufferLine(fold_range.start));
    assert!(summary_dl.is_some(), "fold summary must exist");
    for bl in fold_range.clone() {
        assert_eq!(
            dm_before.buffer_to_display(BufferLine(bl)),
            summary_dl,
            "all folded lines must map to the same summary"
        );
    }

    // Toggle: expand the fold.
    let mut toggle = FoldToggleState::default();
    toggle.toggle(&fold_range);
    let mut filtered = directives;
    toggle.filter_directives(&mut filtered);

    // After toggle: every previously-folded line has its own display line.
    let dm_after = DisplayMap::build(line_count, &filtered);
    for bl in fold_range {
        assert!(
            dm_after.buffer_to_display(BufferLine(bl)).is_some(),
            "after toggle, buffer line {bl} must be individually visible"
        );
    }
}

#[test]
fn fold_toggle_is_single_interaction() {
    let fold_range = 5..15;
    let line_count = 20;
    let directives = vec![DisplayDirective::Fold {
        range: fold_range.clone(),
        summary: vec![Atom::plain("…")],
    }];

    // A single toggle call is sufficient to expand the fold.
    let mut toggle = FoldToggleState::default();
    toggle.toggle(&fold_range);
    assert!(
        toggle.is_expanded(&fold_range),
        "one toggle must expand the fold"
    );

    let mut filtered = directives;
    toggle.filter_directives(&mut filtered);
    assert!(
        filtered.is_empty(),
        "expanded fold must be filtered out in one step"
    );

    // Verify all lines are now visible.
    let dm = DisplayMap::build(line_count, &filtered);
    for bl in fold_range {
        assert!(
            dm.buffer_to_display(BufferLine(bl)).is_some(),
            "line {bl} must be visible after single toggle"
        );
    }
}
