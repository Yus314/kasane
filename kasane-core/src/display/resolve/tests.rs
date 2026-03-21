use super::*;
use crate::protocol::Face;

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
            content: "from-a".into(),
            face: Face::default(),
        },
        0,
        pid("a"),
    );
    set.push(
        DisplayDirective::InsertAfter {
            after: 0,
            content: "from-b".into(),
            face: Face::default(),
        },
        0,
        pid("b"),
    );
    let result = resolve(&set, 5);
    assert_eq!(result.len(), 2);
    // Both inserts are kept
    assert!(result.iter().any(|d| matches!(d,
        DisplayDirective::InsertAfter { content, .. } if content == "from-a"
    )));
    assert!(result.iter().any(|d| matches!(d,
        DisplayDirective::InsertAfter { content, .. } if content == "from-b"
    )));
}

#[test]
fn resolve_inserts_same_line_ordered_by_priority() {
    let mut set = DirectiveSet::default();
    set.push(
        DisplayDirective::InsertAfter {
            after: 0,
            content: "low".into(),
            face: Face::default(),
        },
        10,
        pid("b"),
    );
    set.push(
        DisplayDirective::InsertAfter {
            after: 0,
            content: "high".into(),
            face: Face::default(),
        },
        0,
        pid("a"),
    );
    let result = resolve(&set, 5);
    // Sorted by (priority, plugin_id): priority 0 < 10
    let insert_contents: Vec<&str> = result
        .iter()
        .filter_map(|d| match d {
            DisplayDirective::InsertAfter { content, .. } => Some(content.as_str()),
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
            summary: "fold-a".into(),
            face: Face::default(),
        },
        0,
        pid("a"),
    );
    set.push(
        DisplayDirective::Fold {
            range: 5..7,
            summary: "fold-b".into(),
            face: Face::default(),
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
            summary: "low".into(),
            face: Face::default(),
        },
        0,
        pid("low"),
    );
    set.push(
        DisplayDirective::Fold {
            range: 3..8,
            summary: "high".into(),
            face: Face::default(),
        },
        10,
        pid("high"),
    );
    let result = resolve(&set, 10);
    let folds: Vec<&str> = result
        .iter()
        .filter_map(|d| match d {
            DisplayDirective::Fold { summary, .. } => Some(summary.as_str()),
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
            summary: "alpha".into(),
            face: Face::default(),
        },
        0,
        pid("alpha"),
    );
    set.push(
        DisplayDirective::Fold {
            range: 3..7,
            summary: "beta".into(),
            face: Face::default(),
        },
        0,
        pid("beta"),
    );
    let result = resolve(&set, 10);
    let folds: Vec<&str> = result
        .iter()
        .filter_map(|d| match d {
            DisplayDirective::Fold { summary, .. } => Some(summary.as_str()),
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
            summary: "fold".into(),
            face: Face::default(),
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
            summary: "fold".into(),
            face: Face::default(),
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
            summary: "fold".into(),
            face: Face::default(),
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
            content: "suppressed".into(),
            face: Face::default(),
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
            summary: "fold".into(),
            face: Face::default(),
        },
        0,
        pid("a"),
    );
    set.push(
        DisplayDirective::InsertAfter {
            after: 3,
            content: "suppressed".into(),
            face: Face::default(),
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
            summary: "fold".into(),
            face: Face::default(),
        },
        0,
        pid("a"),
    );
    set.push(
        DisplayDirective::InsertAfter {
            after: 0,
            content: "kept".into(),
            face: Face::default(),
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
