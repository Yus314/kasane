use super::*;
use crate::protocol::Face;

#[test]
fn identity_map_roundtrip() {
    let dm = DisplayMap::identity(10);
    assert!(dm.is_identity());
    assert_eq!(dm.display_line_count(), 10);

    for i in 0..10 {
        assert_eq!(dm.display_to_buffer(i), Some(i));
        assert_eq!(dm.buffer_to_display(i), Some(i));
    }
}

#[test]
fn identity_map_entry() {
    let dm = DisplayMap::identity(3);
    let entry = dm.entry(1).unwrap();
    assert_eq!(entry.source, SourceMapping::BufferLine(1));
    assert_eq!(entry.interaction, InteractionPolicy::Normal);
    assert!(entry.synthetic.is_none());
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
        summary: "... 3 lines ...".into(),
        face: Face::default(),
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
        summary: "folded".into(),
        face: Face::default(),
    }];
    let dm = DisplayMap::build(8, &directives);
    // Display: [0, 1, fold(2..5), 5, 6, 7] = 6 lines

    assert_eq!(dm.display_line_count(), 6);

    // Lines before fold
    assert_eq!(dm.display_to_buffer(0), Some(0));
    assert_eq!(dm.display_to_buffer(1), Some(1));

    // Fold summary line maps to first line of range
    assert_eq!(dm.display_to_buffer(2), Some(2));
    let entry = dm.entry(2).unwrap();
    assert_eq!(entry.source, SourceMapping::LineRange(2..5));
    assert!(entry.synthetic.is_some());
    assert_eq!(entry.interaction, InteractionPolicy::ReadOnly);

    // Lines after fold
    assert_eq!(dm.display_to_buffer(3), Some(5));
    assert_eq!(dm.display_to_buffer(4), Some(6));
    assert_eq!(dm.display_to_buffer(5), Some(7));

    // Buffer → display
    assert_eq!(dm.buffer_to_display(0), Some(0));
    assert_eq!(dm.buffer_to_display(1), Some(1));
    assert_eq!(dm.buffer_to_display(2), Some(2)); // fold start → summary line
    assert_eq!(dm.buffer_to_display(3), Some(2)); // inside fold → summary line
    assert_eq!(dm.buffer_to_display(4), Some(2)); // inside fold → summary line
    assert_eq!(dm.buffer_to_display(5), Some(3));
}

#[test]
fn hide_removes_lines() {
    let directives = vec![DisplayDirective::Hide { range: 1..3 }];
    let dm = DisplayMap::build(5, &directives);

    // 5 - 2 = 3 display lines
    assert_eq!(dm.display_line_count(), 3);
    assert_eq!(dm.display_to_buffer(0), Some(0));
    assert_eq!(dm.display_to_buffer(1), Some(3));
    assert_eq!(dm.display_to_buffer(2), Some(4));

    assert_eq!(dm.buffer_to_display(1), None);
    assert_eq!(dm.buffer_to_display(2), None);
}

#[test]
fn insert_after_adds_lines() {
    let directives = vec![DisplayDirective::InsertAfter {
        after: 1,
        content: "virtual line".into(),
        face: Face::default(),
    }];
    let dm = DisplayMap::build(3, &directives);

    // 3 + 1 = 4 display lines
    assert_eq!(dm.display_line_count(), 4);

    assert_eq!(dm.display_to_buffer(0), Some(0));
    assert_eq!(dm.display_to_buffer(1), Some(1));
    // Virtual line
    assert_eq!(dm.display_to_buffer(2), None);
    let entry = dm.entry(2).unwrap();
    assert_eq!(entry.source, SourceMapping::None);
    assert!(entry.synthetic.is_some());

    assert_eq!(dm.display_to_buffer(3), Some(2));
}

#[test]
fn dirty_identity() {
    let dm = DisplayMap::identity(5);
    let dirty = vec![false, true, false, true, false];

    assert!(!dm.is_display_line_dirty(0, &dirty));
    assert!(dm.is_display_line_dirty(1, &dirty));
    assert!(!dm.is_display_line_dirty(2, &dirty));
    assert!(dm.is_display_line_dirty(3, &dirty));
}

#[test]
fn dirty_fold_any_dirty() {
    let directives = vec![DisplayDirective::Fold {
        range: 1..4,
        summary: "folded".into(),
        face: Face::default(),
    }];
    let dm = DisplayMap::build(5, &directives);
    // Display: [0, fold(1..4), 4] = 3 lines

    // Only line 2 (inside fold) is dirty
    let dirty = vec![false, false, true, false, false];
    assert!(!dm.is_display_line_dirty(0, &dirty));
    assert!(dm.is_display_line_dirty(1, &dirty)); // fold summary: line 2 is dirty
    assert!(!dm.is_display_line_dirty(2, &dirty));
}

#[test]
fn dirty_virtual_line_never_dirty() {
    let directives = vec![DisplayDirective::InsertAfter {
        after: 0,
        content: "virtual".into(),
        face: Face::default(),
    }];
    let dm = DisplayMap::build(2, &directives);

    let dirty = vec![true, true];
    // Virtual line at display index 1 should not be dirty
    assert!(!dm.is_display_line_dirty(1, &dirty));
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
    assert_eq!(dm.display_to_buffer(5), None);
}

#[test]
fn out_of_bounds_buffer_to_display() {
    let dm = DisplayMap::identity(3);
    assert_eq!(dm.buffer_to_display(5), None);
}
