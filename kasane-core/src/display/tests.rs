use proptest::prelude::*;

use super::*;
use crate::display::{BufferLine, DisplayLine, InverseResult, resolve};
use crate::plugin::PluginId;
use crate::protocol::{Atom, Face};

#[test]
fn identity_map_roundtrip() {
    let dm = DisplayMap::identity(10);
    assert!(dm.is_identity());
    assert_eq!(dm.display_line_count(), 10);

    for i in 0..10 {
        assert_eq!(
            dm.display_to_buffer(DisplayLine(i)),
            InverseResult::Actionable(BufferLine(i))
        );
        assert_eq!(dm.buffer_to_display(BufferLine(i)), Some(DisplayLine(i)));
    }
}

#[test]
fn identity_map_entry() {
    let dm = DisplayMap::identity(3);
    let entry = dm.entry(DisplayLine(1)).unwrap();
    assert_eq!(*entry.source(), SourceMapping::BufferLine(BufferLine(1)));
    assert_eq!(entry.interaction(), InteractionPolicy::Normal);
    assert!(entry.synthetic().is_none());
}

#[test]
fn identity_equality() {
    let a = DisplayMap::identity(10);
    let b = DisplayMap::identity(10);
    assert_eq!(a, b);

    let c = DisplayMap::identity(5);
    assert_ne!(a, c);
}

#[test]
fn fold_reduces_line_count() {
    // 10 buffer lines, fold lines 3..6 (3 lines → 1 summary)
    let directives = vec![DisplayDirective::Fold {
        range: 3..6,
        summary: vec![Atom {
            face: Face::default(),
            contents: "... 3 lines ...".into(),
        }],
    }];
    let dm = DisplayMap::build(10, &directives);

    assert!(!dm.is_identity());
    // 10 - 3 + 1 = 8 display lines
    assert_eq!(dm.display_line_count(), 8);
}

#[test]
fn fold_mapping_correctness() {
    let directives = vec![DisplayDirective::Fold {
        range: 2..5,
        summary: vec![Atom {
            face: Face::default(),
            contents: "folded".into(),
        }],
    }];
    let dm = DisplayMap::build(8, &directives);
    // Display: [0, 1, fold(2..5), 5, 6, 7] = 6 lines

    assert_eq!(dm.display_line_count(), 6);

    // Lines before fold
    assert_eq!(
        dm.display_to_buffer(DisplayLine(0)),
        InverseResult::Actionable(BufferLine(0))
    );
    assert_eq!(
        dm.display_to_buffer(DisplayLine(1)),
        InverseResult::Actionable(BufferLine(1))
    );

    // Fold summary line maps to first line of range
    assert_eq!(
        dm.display_to_buffer(DisplayLine(2)),
        InverseResult::Informational {
            representative: BufferLine(2),
            range: 2..5,
        }
    );
    let entry = dm.entry(DisplayLine(2)).unwrap();
    assert_eq!(*entry.source(), SourceMapping::LineRange(2..5));
    assert!(entry.synthetic().is_some());
    assert_eq!(entry.interaction(), InteractionPolicy::ReadOnly);

    // Lines after fold
    assert_eq!(
        dm.display_to_buffer(DisplayLine(3)),
        InverseResult::Actionable(BufferLine(5))
    );
    assert_eq!(
        dm.display_to_buffer(DisplayLine(4)),
        InverseResult::Actionable(BufferLine(6))
    );
    assert_eq!(
        dm.display_to_buffer(DisplayLine(5)),
        InverseResult::Actionable(BufferLine(7))
    );

    // Buffer → display
    assert_eq!(dm.buffer_to_display(BufferLine(0)), Some(DisplayLine(0)));
    assert_eq!(dm.buffer_to_display(BufferLine(1)), Some(DisplayLine(1)));
    assert_eq!(dm.buffer_to_display(BufferLine(2)), Some(DisplayLine(2))); // fold start → summary line
    assert_eq!(dm.buffer_to_display(BufferLine(3)), Some(DisplayLine(2))); // inside fold → summary line
    assert_eq!(dm.buffer_to_display(BufferLine(4)), Some(DisplayLine(2))); // inside fold → summary line
    assert_eq!(dm.buffer_to_display(BufferLine(5)), Some(DisplayLine(3)));
}

#[test]
fn hide_removes_lines() {
    let directives = vec![DisplayDirective::Hide { range: 1..3 }];
    let dm = DisplayMap::build(5, &directives);

    // 5 - 2 = 3 display lines
    assert_eq!(dm.display_line_count(), 3);
    assert_eq!(
        dm.display_to_buffer(DisplayLine(0)),
        InverseResult::Actionable(BufferLine(0))
    );
    assert_eq!(
        dm.display_to_buffer(DisplayLine(1)),
        InverseResult::Actionable(BufferLine(3))
    );
    assert_eq!(
        dm.display_to_buffer(DisplayLine(2)),
        InverseResult::Actionable(BufferLine(4))
    );

    assert_eq!(dm.buffer_to_display(BufferLine(1)), None);
    assert_eq!(dm.buffer_to_display(BufferLine(2)), None);
}

#[test]
fn dirty_identity() {
    let dm = DisplayMap::identity(5);
    let dirty = vec![false, true, false, true, false];

    assert!(!dm.is_display_line_dirty(DisplayLine(0), &dirty));
    assert!(dm.is_display_line_dirty(DisplayLine(1), &dirty));
    assert!(!dm.is_display_line_dirty(DisplayLine(2), &dirty));
    assert!(dm.is_display_line_dirty(DisplayLine(3), &dirty));
}

#[test]
fn dirty_fold_any_dirty() {
    let directives = vec![DisplayDirective::Fold {
        range: 1..4,
        summary: vec![Atom {
            face: Face::default(),
            contents: "folded".into(),
        }],
    }];
    let dm = DisplayMap::build(5, &directives);
    // Display: [0, fold(1..4), 4] = 3 lines

    // Only line 2 (inside fold) is dirty
    let dirty = vec![false, false, true, false, false];
    assert!(!dm.is_display_line_dirty(DisplayLine(0), &dirty));
    assert!(dm.is_display_line_dirty(DisplayLine(1), &dirty)); // fold summary: line 2 is dirty
    assert!(!dm.is_display_line_dirty(DisplayLine(2), &dirty));
}

#[test]
fn empty_directives_produce_identity() {
    let dm = DisplayMap::build(5, &[]);
    assert!(dm.is_identity());
    assert_eq!(dm.display_line_count(), 5);
}

#[test]
fn out_of_bounds_display_to_buffer() {
    let dm = DisplayMap::identity(3);
    assert_eq!(
        dm.display_to_buffer(DisplayLine(5)),
        InverseResult::OutOfRange
    );
}

#[test]
fn out_of_bounds_buffer_to_display() {
    let dm = DisplayMap::identity(3);
    assert_eq!(dm.buffer_to_display(BufferLine(5)), None);
}

// --- Phase 2: Precondition tests ---

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "precondition")]
fn build_rejects_fold_hide_overlap() {
    let directives = vec![
        DisplayDirective::Fold {
            range: 2..5,
            summary: vec![Atom {
                face: Face::default(),
                contents: "fold".into(),
            }],
        },
        DisplayDirective::Hide { range: 3..6 },
    ];
    let _ = DisplayMap::build(10, &directives);
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "precondition")]
fn build_rejects_empty_fold_range() {
    let directives = vec![DisplayDirective::Fold {
        range: 3..3,
        summary: vec![Atom {
            face: Face::default(),
            contents: "empty".into(),
        }],
    }];
    let _ = DisplayMap::build(10, &directives);
}

// --- Phase 3: PartialEq test ---

#[test]
fn different_line_count_not_equal() {
    let a = DisplayMap::build(5, &[DisplayDirective::Hide { range: 3..5 }]);
    let b = DisplayMap::build(3, &[]);
    assert_ne!(a, b);
}

// --- Phase 4: proptest ---

fn arb_display_directive(max_line: usize) -> impl Strategy<Value = DisplayDirective> {
    let m = max_line.max(1);
    prop_oneof![
        (0usize..m, 1usize..m.min(8).max(1) + 1).prop_map(move |(s, len)| {
            DisplayDirective::Fold {
                range: s..(s + len).min(m),
                summary: vec![Atom {
                    face: Face::default(),
                    contents: "...".into(),
                }],
            }
        }),
        (0usize..m, 1usize..m.min(8).max(1) + 1).prop_map(move |(s, len)| {
            DisplayDirective::Hide {
                range: s..(s + len).min(m),
            }
        }),
    ]
}

// --- compute_display_scroll_offset tests ---

#[test]
fn scroll_offset_identity_map_returns_zero() {
    let dm = DisplayMap::identity(20);
    assert_eq!(
        super::compute_display_scroll_offset(&dm, BufferLine(15), 10),
        DisplayLine(0)
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn build_invariants_hold(
        (line_count, directives) in (1usize..50).prop_flat_map(|lc| {
            (Just(lc), prop::collection::vec(arb_display_directive(lc), 0..8))
        })
    ) {
        let mut set = DirectiveSet::default();
        for (i, d) in directives.into_iter().enumerate() {
            set.push(d, 0, PluginId(format!("p{i}")));
        }
        let resolved = resolve::resolve(&set, line_count);
        let dm = DisplayMap::build(line_count, &resolved);
        assert_display_map_invariants(&dm, line_count);
    }

    #[test]
    fn identity_invariants_hold(n in 0usize..200) {
        let dm = DisplayMap::identity(n);
        assert_display_map_invariants(&dm, n);
    }

    /// C2 (Mapping Faithfulness): InverseResult variant matches SourceMapping.
    ///
    /// - Actionable(bl) ⟺ entry.source == BufferLine(bl)
    /// - Informational { representative, range } ⟺ entry.source == LineRange(range)
    /// - OutOfRange ⟺ display line index beyond entries
    #[test]
    fn inverse_result_matches_source_mapping(
        (line_count, directives) in (1usize..50).prop_flat_map(|lc| {
            (Just(lc), prop::collection::vec(arb_display_directive(lc), 0..8))
        })
    ) {
        let mut set = DirectiveSet::default();
        for (i, d) in directives.into_iter().enumerate() {
            set.push(d, 0, PluginId(format!("p{i}")));
        }
        let resolved = resolve::resolve(&set, line_count);
        let dm = DisplayMap::build(line_count, &resolved);

        for dl in 0..dm.display_line_count() {
            let result = dm.display_to_buffer(DisplayLine(dl));
            let entry = dm.entry(DisplayLine(dl)).unwrap();
            match (&result, entry.source()) {
                (InverseResult::Actionable(bl), SourceMapping::BufferLine(src_bl)) => {
                    prop_assert_eq!(bl, src_bl, "C2: Actionable line mismatch at dl={}", dl);
                }
                (InverseResult::Informational { range, .. }, SourceMapping::LineRange(src_range)) => {
                    prop_assert_eq!(range, src_range, "C2: Informational range mismatch at dl={}", dl);
                }
                (InverseResult::Projected { anchor, .. }, SourceMapping::Projected { anchor: src_anchor, .. }) => {
                    prop_assert_eq!(anchor, src_anchor, "C2: Projected anchor mismatch at dl={}", dl);
                }
                (result, source) => {
                    prop_assert!(false, "C2 violated at dl={}: result={:?}, source={:?}", dl, result, source);
                }
            }
        }
        // Out of range
        prop_assert_eq!(
            dm.display_to_buffer(DisplayLine(dm.display_line_count())),
            InverseResult::OutOfRange
        );
    }

    /// C3 (Action Safety): .actionable() returns Some only for Actionable.
    #[test]
    fn actionable_accessor_soundness(
        (line_count, directives) in (1usize..50).prop_flat_map(|lc| {
            (Just(lc), prop::collection::vec(arb_display_directive(lc), 0..8))
        })
    ) {
        let mut set = DirectiveSet::default();
        for (i, d) in directives.into_iter().enumerate() {
            set.push(d, 0, PluginId(format!("p{i}")));
        }
        let resolved = resolve::resolve(&set, line_count);
        let dm = DisplayMap::build(line_count, &resolved);

        for dl in 0..dm.display_line_count() {
            let result = dm.display_to_buffer(DisplayLine(dl));
            let entry = dm.entry(DisplayLine(dl)).unwrap();
            let actionable = result.clone().actionable();
            match entry.source() {
                SourceMapping::BufferLine(bl) => {
                    prop_assert_eq!(actionable, Some(*bl), "C3: Strong source must be actionable at dl={}", dl);
                }
                _ => {
                    prop_assert!(actionable.is_none(), "C3: Non-strong source must not be actionable at dl={}", dl);
                }
            }
        }
    }
}
