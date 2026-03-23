use proptest::prelude::*;

use super::*;
use crate::display::assert_display_map_invariants;
use crate::protocol::{Atom, Face};

fn pid(name: &str) -> PluginId {
    PluginId(name.to_string())
}

#[test]
fn resolve_empty() {
    let set = DirectiveSet::default();
    assert_eq!(resolve(&set, 10), vec![]);
}

#[test]
fn resolve_single_plugin() {
    let mut set = DirectiveSet::default();
    set.push(DisplayDirective::Hide { range: 1..3 }, 0, pid("a"));
    let result = resolve(&set, 10);
    assert_eq!(result, vec![DisplayDirective::Hide { range: 1..3 }]);
}

#[test]
fn resolve_inserts_accumulate() {
    let mut set = DirectiveSet::default();
    set.push(
        DisplayDirective::InsertAfter {
            after: 0,
            content: vec![Atom {
                face: Face::default(),
                contents: "from-a".into(),
            }],
        },
        0,
        pid("a"),
    );
    set.push(
        DisplayDirective::InsertAfter {
            after: 0,
            content: vec![Atom {
                face: Face::default(),
                contents: "from-b".into(),
            }],
        },
        0,
        pid("b"),
    );
    let result = resolve(&set, 5);
    assert_eq!(result.len(), 2);
    // Both inserts are kept
    assert!(result.iter().any(|d| matches!(d,
        DisplayDirective::InsertAfter { content, .. } if content.first().map(|a| a.contents.as_str()) == Some("from-a")
    )));
    assert!(result.iter().any(|d| matches!(d,
        DisplayDirective::InsertAfter { content, .. } if content.first().map(|a| a.contents.as_str()) == Some("from-b")
    )));
}

#[test]
fn resolve_inserts_same_line_ordered_by_priority() {
    let mut set = DirectiveSet::default();
    set.push(
        DisplayDirective::InsertAfter {
            after: 0,
            content: vec![Atom {
                face: Face::default(),
                contents: "low".into(),
            }],
        },
        10,
        pid("b"),
    );
    set.push(
        DisplayDirective::InsertAfter {
            after: 0,
            content: vec![Atom {
                face: Face::default(),
                contents: "high".into(),
            }],
        },
        0,
        pid("a"),
    );
    let result = resolve(&set, 5);
    // Sorted by (priority, plugin_id): priority 0 < 10
    let insert_contents: Vec<&str> = result
        .iter()
        .filter_map(|d| match d {
            DisplayDirective::InsertAfter { content, .. } => {
                content.first().map(|a| a.contents.as_str())
            }
            _ => None,
        })
        .collect();
    assert_eq!(insert_contents, vec!["high", "low"]);
}

#[test]
fn resolve_hides_union() {
    let mut set = DirectiveSet::default();
    set.push(DisplayDirective::Hide { range: 1..3 }, 0, pid("a"));
    set.push(DisplayDirective::Hide { range: 4..6 }, 0, pid("b"));
    let result = resolve(&set, 10);
    assert_eq!(result.len(), 2);
}

#[test]
fn resolve_hides_overlap_idempotent() {
    let mut set = DirectiveSet::default();
    set.push(DisplayDirective::Hide { range: 1..4 }, 0, pid("a"));
    set.push(DisplayDirective::Hide { range: 2..5 }, 0, pid("b"));
    let result = resolve(&set, 10);
    // Both hides are emitted; DisplayMap::build handles the union via its hidden[] array
    assert_eq!(result.len(), 2);
    // Build the map to verify net effect: lines 1..5 hidden
    let dm = crate::display::DisplayMap::build(10, &result);
    assert_eq!(dm.buffer_to_display(1), None);
    assert_eq!(dm.buffer_to_display(2), None);
    assert_eq!(dm.buffer_to_display(3), None);
    assert_eq!(dm.buffer_to_display(4), None);
    assert!(dm.buffer_to_display(0).is_some());
    assert!(dm.buffer_to_display(5).is_some());
}

#[test]
fn resolve_folds_disjoint_both_kept() {
    let mut set = DirectiveSet::default();
    set.push(
        DisplayDirective::Fold {
            range: 1..3,
            summary: vec![Atom {
                face: Face::default(),
                contents: "fold-a".into(),
            }],
        },
        0,
        pid("a"),
    );
    set.push(
        DisplayDirective::Fold {
            range: 5..7,
            summary: vec![Atom {
                face: Face::default(),
                contents: "fold-b".into(),
            }],
        },
        0,
        pid("b"),
    );
    let result = resolve(&set, 10);
    let fold_count = result
        .iter()
        .filter(|d| matches!(d, DisplayDirective::Fold { .. }))
        .count();
    assert_eq!(fold_count, 2);
}

#[test]
fn resolve_folds_overlap_higher_priority_wins() {
    let mut set = DirectiveSet::default();
    set.push(
        DisplayDirective::Fold {
            range: 2..6,
            summary: vec![Atom {
                face: Face::default(),
                contents: "low".into(),
            }],
        },
        0,
        pid("low"),
    );
    set.push(
        DisplayDirective::Fold {
            range: 3..8,
            summary: vec![Atom {
                face: Face::default(),
                contents: "high".into(),
            }],
        },
        10,
        pid("high"),
    );
    let result = resolve(&set, 10);
    let folds: Vec<&str> = result
        .iter()
        .filter_map(|d| match d {
            DisplayDirective::Fold { summary, .. } => summary.first().map(|a| a.contents.as_str()),
            _ => None,
        })
        .collect();
    // Higher priority fold kept, lower dropped (overlap)
    assert_eq!(folds, vec!["high"]);
}

#[test]
fn resolve_folds_overlap_same_priority_plugin_id_tiebreak() {
    let mut set = DirectiveSet::default();
    set.push(
        DisplayDirective::Fold {
            range: 1..5,
            summary: vec![Atom {
                face: Face::default(),
                contents: "alpha".into(),
            }],
        },
        0,
        pid("alpha"),
    );
    set.push(
        DisplayDirective::Fold {
            range: 3..7,
            summary: vec![Atom {
                face: Face::default(),
                contents: "beta".into(),
            }],
        },
        0,
        pid("beta"),
    );
    let result = resolve(&set, 10);
    let folds: Vec<&str> = result
        .iter()
        .filter_map(|d| match d {
            DisplayDirective::Fold { summary, .. } => summary.first().map(|a| a.contents.as_str()),
            _ => None,
        })
        .collect();
    // Same priority → plugin_id "alpha" < "beta" → "alpha" wins
    assert_eq!(folds, vec!["alpha"]);
}

#[test]
fn resolve_fold_hide_partial_overlap_fold_removed() {
    let mut set = DirectiveSet::default();
    set.push(
        DisplayDirective::Fold {
            range: 2..6,
            summary: vec![Atom {
                face: Face::default(),
                contents: "fold".into(),
            }],
        },
        0,
        pid("a"),
    );
    // Hide partially overlaps the fold range
    set.push(DisplayDirective::Hide { range: 4..8 }, 0, pid("b"));
    let result = resolve(&set, 10);
    // Fold should be removed (partial hide invalidates fold summary)
    let fold_count = result
        .iter()
        .filter(|d| matches!(d, DisplayDirective::Fold { .. }))
        .count();
    assert_eq!(fold_count, 0);
    // Hide still present
    assert!(
        result
            .iter()
            .any(|d| matches!(d, DisplayDirective::Hide { .. }))
    );
}

#[test]
fn resolve_fold_hide_full_cover_fold_removed() {
    let mut set = DirectiveSet::default();
    set.push(
        DisplayDirective::Fold {
            range: 2..5,
            summary: vec![Atom {
                face: Face::default(),
                contents: "fold".into(),
            }],
        },
        0,
        pid("a"),
    );
    // Hide fully covers the fold range
    set.push(DisplayDirective::Hide { range: 1..6 }, 0, pid("b"));
    let result = resolve(&set, 10);
    let fold_count = result
        .iter()
        .filter(|d| matches!(d, DisplayDirective::Fold { .. }))
        .count();
    assert_eq!(fold_count, 0);
}

#[test]
fn resolve_fold_hide_disjoint_both_kept() {
    let mut set = DirectiveSet::default();
    set.push(
        DisplayDirective::Fold {
            range: 1..3,
            summary: vec![Atom {
                face: Face::default(),
                contents: "fold".into(),
            }],
        },
        0,
        pid("a"),
    );
    set.push(DisplayDirective::Hide { range: 5..7 }, 0, pid("b"));
    let result = resolve(&set, 10);
    let fold_count = result
        .iter()
        .filter(|d| matches!(d, DisplayDirective::Fold { .. }))
        .count();
    let hide_count = result
        .iter()
        .filter(|d| matches!(d, DisplayDirective::Hide { .. }))
        .count();
    assert_eq!(fold_count, 1);
    assert_eq!(hide_count, 1);
}

#[test]
fn resolve_insert_suppressed_by_hide() {
    let mut set = DirectiveSet::default();
    set.push(DisplayDirective::Hide { range: 2..4 }, 0, pid("a"));
    set.push(
        DisplayDirective::InsertAfter {
            after: 3,
            content: vec![Atom {
                face: Face::default(),
                contents: "suppressed".into(),
            }],
        },
        0,
        pid("b"),
    );
    let result = resolve(&set, 10);
    let insert_count = result
        .iter()
        .filter(|d| matches!(d, DisplayDirective::InsertAfter { .. }))
        .count();
    assert_eq!(insert_count, 0);
}

#[test]
fn resolve_insert_suppressed_by_fold() {
    let mut set = DirectiveSet::default();
    set.push(
        DisplayDirective::Fold {
            range: 2..5,
            summary: vec![Atom {
                face: Face::default(),
                contents: "fold".into(),
            }],
        },
        0,
        pid("a"),
    );
    set.push(
        DisplayDirective::InsertAfter {
            after: 3,
            content: vec![Atom {
                face: Face::default(),
                contents: "suppressed".into(),
            }],
        },
        0,
        pid("b"),
    );
    let result = resolve(&set, 10);
    let insert_count = result
        .iter()
        .filter(|d| matches!(d, DisplayDirective::InsertAfter { .. }))
        .count();
    assert_eq!(insert_count, 0);
}

#[test]
fn resolve_insert_outside_fold_kept() {
    let mut set = DirectiveSet::default();
    set.push(
        DisplayDirective::Fold {
            range: 2..5,
            summary: vec![Atom {
                face: Face::default(),
                contents: "fold".into(),
            }],
        },
        0,
        pid("a"),
    );
    set.push(
        DisplayDirective::InsertAfter {
            after: 0,
            content: vec![Atom {
                face: Face::default(),
                contents: "kept".into(),
            }],
        },
        0,
        pid("b"),
    );
    let result = resolve(&set, 10);
    let insert_count = result
        .iter()
        .filter(|d| matches!(d, DisplayDirective::InsertAfter { .. }))
        .count();
    assert_eq!(insert_count, 1);
}

// --- Phase 5: proptest for resolve → build pipeline ---

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
        (0usize..m).prop_map(|after| {
            DisplayDirective::InsertAfter {
                after,
                content: vec![Atom {
                    face: Face::default(),
                    contents: "virtual".into(),
                }],
            }
        }),
    ]
}

fn arb_tagged_directives(max_line: usize) -> impl Strategy<Value = DirectiveSet> {
    let m = max_line.max(1);
    prop::collection::vec(
        (
            arb_display_directive(m),
            -10i16..10i16,
            "[a-z]{1,4}".prop_map(PluginId),
        ),
        0..8,
    )
    .prop_map(|items| {
        let mut set = DirectiveSet::default();
        for (d, priority, plugin_id) in items {
            set.push(d, priority, plugin_id);
        }
        set
    })
}

fn ranges_overlap(a: &std::ops::Range<usize>, b: &std::ops::Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn resolve_build_invariants(
        (line_count, set) in (1usize..50).prop_flat_map(|lc| {
            (Just(lc), arb_tagged_directives(lc))
        })
    ) {
        let resolved = resolve(&set, line_count);
        let dm = crate::display::DisplayMap::build(line_count, &resolved);
        assert_display_map_invariants(&dm, line_count);
    }

    #[test]
    fn resolve_no_fold_hide_overlap(
        (line_count, set) in (1usize..50).prop_flat_map(|lc| {
            (Just(lc), arb_tagged_directives(lc))
        })
    ) {
        let resolved = resolve(&set, line_count);
        for d1 in &resolved {
            if let DisplayDirective::Fold { range: fold_r, .. } = d1 {
                for d2 in &resolved {
                    if let DisplayDirective::Hide { range: hide_r } = d2 {
                        prop_assert!(
                            !ranges_overlap(fold_r, hide_r),
                            "resolve produced fold {fold_r:?} overlapping hide {hide_r:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn resolve_no_overlapping_folds(
        (line_count, set) in (1usize..50).prop_flat_map(|lc| {
            (Just(lc), arb_tagged_directives(lc))
        })
    ) {
        let resolved = resolve(&set, line_count);
        let folds: Vec<_> = resolved.iter().filter_map(|d| {
            if let DisplayDirective::Fold { range, .. } = d {
                Some(range.clone())
            } else {
                None
            }
        }).collect();
        for i in 0..folds.len() {
            for j in i + 1..folds.len() {
                prop_assert!(
                    !ranges_overlap(&folds[i], &folds[j]),
                    "resolve produced overlapping folds {:?} and {:?}",
                    folds[i], folds[j]
                );
            }
        }
    }
}
