//! ADR-035 §1 tests for `SelectionSet` set algebra.

use crate::plugin::PluginId;

use super::selection::{BufferId, BufferPos, BufferVersion, Direction, Selection};
use super::selection_set::{LoadError, SaveError, SelectionSet};

fn buf() -> BufferId {
    BufferId::new("test")
}

fn ver() -> BufferVersion {
    BufferVersion::INITIAL
}

fn sel(line: u32, c0: u32, c1: u32) -> Selection {
    Selection::new(BufferPos::new(line, c0), BufferPos::new(line, c1))
}

fn set(sels: Vec<Selection>) -> SelectionSet {
    SelectionSet::from_iter(sels, buf(), ver())
}

// =============================================================================
// Construction and normalisation
// =============================================================================

#[test]
fn empty_set_is_empty() {
    let s = SelectionSet::empty(buf(), ver());
    assert!(s.is_empty());
    assert_eq!(s.len(), 0);
    assert!(s.primary().is_none());
}

#[test]
fn singleton_set_has_one_selection() {
    let s = SelectionSet::singleton(sel(0, 0, 5), buf(), ver());
    assert_eq!(s.len(), 1);
    assert_eq!(s.primary().unwrap().min().column, 0);
}

#[test]
fn from_iter_sorts_unsorted_input() {
    let s = set(vec![sel(2, 0, 1), sel(0, 0, 1), sel(1, 0, 1)]);
    let lines: Vec<u32> = s.iter().map(|s| s.min().line).collect();
    assert_eq!(lines, vec![0, 1, 2]);
}

#[test]
fn from_iter_merges_overlapping() {
    let s = set(vec![sel(0, 0, 5), sel(0, 3, 8)]);
    assert_eq!(s.len(), 1);
    assert_eq!(s.primary().unwrap().min().column, 0);
    assert_eq!(s.primary().unwrap().max().column, 8);
}

#[test]
fn from_iter_merges_adjacent() {
    // Half-open [0,5) and [5,10) are adjacent; treat as a single
    // selection so that a plugin extending pieces gets a coherent range.
    let s = set(vec![sel(0, 0, 5), sel(0, 5, 10)]);
    assert_eq!(s.len(), 1);
    assert_eq!(s.primary().unwrap().max().column, 10);
}

// =============================================================================
// Coverage and disjointness
// =============================================================================

#[test]
fn covers_returns_true_for_position_in_range() {
    let s = set(vec![sel(0, 5, 10)]);
    assert!(s.covers(BufferPos::new(0, 7)));
    assert!(!s.covers(BufferPos::new(0, 12)));
    assert!(!s.covers(BufferPos::new(1, 7)));
}

#[test]
fn is_disjoint_when_ranges_dont_overlap() {
    let a = set(vec![sel(0, 0, 5)]);
    let b = set(vec![sel(0, 10, 15)]);
    assert!(a.is_disjoint(&b));
}

#[test]
fn is_disjoint_false_on_overlap() {
    let a = set(vec![sel(0, 0, 10)]);
    let b = set(vec![sel(0, 5, 15)]);
    assert!(!a.is_disjoint(&b));
}

#[test]
fn is_disjoint_false_for_different_buffers() {
    let a = SelectionSet::singleton(sel(0, 0, 5), BufferId::new("b1"), ver());
    let b = SelectionSet::singleton(sel(0, 10, 15), BufferId::new("b2"), ver());
    assert!(
        !a.is_disjoint(&b),
        "different-buffer sets are never disjoint by API contract"
    );
}

// =============================================================================
// Union — commutative, associative, identity (empty set)
// =============================================================================

#[test]
fn union_is_commutative_on_disjoint() {
    let a = set(vec![sel(0, 0, 5)]);
    let b = set(vec![sel(1, 0, 5)]);
    assert_eq!(a.union(&b), b.union(&a));
}

#[test]
fn union_with_empty_is_identity() {
    let a = set(vec![sel(0, 0, 5)]);
    let e = SelectionSet::empty(buf(), ver());
    assert_eq!(a.union(&e), a);
    assert_eq!(e.union(&a), a);
}

#[test]
fn union_merges_overlapping_selections() {
    let a = set(vec![sel(0, 0, 5)]);
    let b = set(vec![sel(0, 3, 8)]);
    let u = a.union(&b);
    assert_eq!(u.len(), 1);
    assert_eq!(u.primary().unwrap().max().column, 8);
}

#[test]
fn union_is_associative() {
    let a = set(vec![sel(0, 0, 2)]);
    let b = set(vec![sel(1, 0, 2)]);
    let c = set(vec![sel(2, 0, 2)]);
    let lhs = a.union(&b).union(&c);
    let rhs = a.union(&b.union(&c));
    assert_eq!(lhs, rhs);
}

// =============================================================================
// Intersect
// =============================================================================

#[test]
fn intersect_disjoint_is_empty() {
    let a = set(vec![sel(0, 0, 5)]);
    let b = set(vec![sel(1, 0, 5)]);
    assert!(a.intersect(&b).is_empty());
}

#[test]
fn intersect_overlapping_returns_overlap() {
    let a = set(vec![sel(0, 0, 10)]);
    let b = set(vec![sel(0, 5, 15)]);
    let i = a.intersect(&b);
    assert_eq!(i.len(), 1);
    let only = i.primary().unwrap();
    assert_eq!(only.min().column, 5);
    assert_eq!(only.max().column, 10);
}

#[test]
fn intersect_with_self_is_self() {
    let a = set(vec![sel(0, 0, 5), sel(1, 0, 5)]);
    assert_eq!(a.intersect(&a), a);
}

#[test]
fn intersect_with_empty_is_empty() {
    let a = set(vec![sel(0, 0, 5)]);
    let e = SelectionSet::empty(buf(), ver());
    assert!(a.intersect(&e).is_empty());
}

// =============================================================================
// Difference
// =============================================================================

#[test]
fn difference_with_self_is_empty() {
    let a = set(vec![sel(0, 0, 5)]);
    assert!(a.difference(&a).is_empty());
}

#[test]
fn difference_with_empty_is_self() {
    let a = set(vec![sel(0, 0, 5)]);
    let e = SelectionSet::empty(buf(), ver());
    assert_eq!(a.difference(&e), a);
}

#[test]
fn difference_subtracts_overlap() {
    let a = set(vec![sel(0, 0, 10)]);
    let b = set(vec![sel(0, 3, 7)]);
    let d = a.difference(&b);
    assert_eq!(d.len(), 2);
    assert_eq!(d.iter().next().unwrap().max().column, 3);
    assert_eq!(d.iter().nth(1).unwrap().min().column, 7);
}

#[test]
fn difference_when_subtrahend_covers_minuend_yields_empty() {
    let a = set(vec![sel(0, 5, 8)]);
    let b = set(vec![sel(0, 0, 10)]);
    assert!(a.difference(&b).is_empty());
}

// =============================================================================
// Symmetric difference
// =============================================================================

#[test]
fn symmetric_difference_with_self_is_empty() {
    let a = set(vec![sel(0, 0, 5)]);
    assert!(a.symmetric_difference(&a).is_empty());
}

#[test]
fn symmetric_difference_disjoint_is_union() {
    let a = set(vec![sel(0, 0, 5)]);
    let b = set(vec![sel(1, 0, 5)]);
    assert_eq!(a.symmetric_difference(&b), a.union(&b));
}

// =============================================================================
// Pointwise transformation
// =============================================================================

#[test]
fn map_transforms_each_selection() {
    let a = set(vec![sel(0, 0, 5), sel(1, 0, 5)]);
    let mapped = a.map(|s| Selection {
        anchor: BufferPos::new(s.anchor.line + 10, s.anchor.column),
        cursor: BufferPos::new(s.cursor.line + 10, s.cursor.column),
        direction: Direction::Forward,
    });
    let lines: Vec<u32> = mapped.iter().map(|s| s.min().line).collect();
    assert_eq!(lines, vec![10, 11]);
}

#[test]
fn filter_drops_non_matching() {
    let a = set(vec![sel(0, 0, 5), sel(1, 0, 5), sel(2, 0, 5)]);
    let f = a.filter(|s| s.min().line != 1);
    assert_eq!(f.len(), 2);
}

#[test]
fn flat_map_can_split_one_into_many() {
    let a = set(vec![sel(0, 0, 10)]);
    let split = a.flat_map(|s| {
        let mid = (s.min().column + s.max().column) / 2;
        vec![
            Selection::new(s.min(), BufferPos::new(s.min().line, mid)),
            Selection::new(BufferPos::new(s.min().line, mid), s.max()),
        ]
    });
    // The two halves are adjacent, so `from_iter` (used internally by
    // flat_map) coalesces them back into one.
    assert_eq!(split.len(), 1);
}

// =============================================================================
// Persistence
// =============================================================================

// Persistence tests use distinct (plugin_id, name) keys so they can run
// in parallel without contending on the global store. No `_clear_store`
// is needed — each test owns its own namespace.

#[test]
fn save_and_load_round_trips() {
    let a = set(vec![sel(0, 0, 5)]);
    a.save(PluginId("persistence-roundtrip".into()), "k")
        .unwrap();
    let loaded = SelectionSet::load(PluginId("persistence-roundtrip".into()), "k", buf()).unwrap();
    assert_eq!(loaded, a);
}

#[test]
fn save_invalid_name_rejected() {
    let a = set(vec![sel(0, 0, 5)]);
    assert_eq!(
        a.save(PluginId("persistence-invalid".into()), ""),
        Err(SaveError::InvalidName)
    );
    assert_eq!(
        a.save(PluginId("persistence-invalid".into()), "scoped:bad"),
        Err(SaveError::InvalidName)
    );
}

#[test]
fn load_missing_returns_not_found() {
    let result = SelectionSet::load(
        PluginId("persistence-missing".into()),
        "definitely-not-there",
        buf(),
    );
    assert_eq!(result, Err(LoadError::NotFound));
}

#[test]
fn load_buffer_mismatch_surfaces_error() {
    let a = set(vec![sel(0, 0, 5)]);
    a.save(PluginId("persistence-mismatch".into()), "k")
        .unwrap();
    let result = SelectionSet::load(
        PluginId("persistence-mismatch".into()),
        "k",
        BufferId::new("other-buffer"),
    );
    assert!(matches!(result, Err(LoadError::BufferMismatch { .. })));
}

#[test]
fn save_under_different_plugins_does_not_collide() {
    let a = set(vec![sel(0, 0, 5)]);
    let b = set(vec![sel(1, 0, 5)]);
    a.save(PluginId("persistence-collide-1".into()), "shared")
        .unwrap();
    b.save(PluginId("persistence-collide-2".into()), "shared")
        .unwrap();

    let la = SelectionSet::load(PluginId("persistence-collide-1".into()), "shared", buf()).unwrap();
    let lb = SelectionSet::load(PluginId("persistence-collide-2".into()), "shared", buf()).unwrap();
    assert_eq!(la, a);
    assert_eq!(lb, b);
}

// =============================================================================
// Projection back to Kakoune (ADR-035 §Decision)
// =============================================================================

/// Decode a `Command::SendToKakoune(Keys(...))` back into the
/// keysym-substituted command string. Used by the projection tests
/// to assert against the readable form of the issued command.
fn render_kakoune_command(cmd: &crate::plugin::Command) -> String {
    use crate::plugin::Command;
    use crate::protocol::KasaneRequest;
    let keys = match cmd {
        Command::SendToKakoune(KasaneRequest::Keys(k)) => k,
        _ => panic!("expected SendToKakoune(Keys)"),
    };
    let mut s = String::new();
    for k in keys {
        match k.as_str() {
            "<space>" => s.push(' '),
            "<minus>" => s.push('-'),
            "<lt>" => s.push('<'),
            "<gt>" => s.push('>'),
            "<ret>" => s.push('\n'),
            "<esc>" => s.push_str("<esc>"),
            other => s.push_str(other),
        }
    }
    s
}

#[test]
fn to_kakoune_command_empty_set_returns_none() {
    let s = SelectionSet::empty(buf(), ver());
    assert!(s.to_kakoune_command().is_none());
}

#[test]
fn to_kakoune_command_singleton_emits_one_range() {
    let s = set(vec![sel(2, 0, 5)]);
    let cmd = s.to_kakoune_command().expect("non-empty set");
    let rendered = render_kakoune_command(&cmd);
    assert!(
        rendered.contains("select 3.1,3.6"),
        "expected `select 3.1,3.6`; got {rendered}"
    );
}

#[test]
fn to_kakoune_command_multi_selection_space_separates_ranges() {
    let s = set(vec![sel(0, 0, 3), sel(4, 1, 7)]);
    let cmd = s.to_kakoune_command().expect("non-empty set");
    let rendered = render_kakoune_command(&cmd);
    assert!(
        rendered.contains("select 1.1,1.4 5.2,5.8"),
        "expected `select 1.1,1.4 5.2,5.8`; got {rendered}"
    );
}

#[test]
fn to_kakoune_command_preserves_direction_via_anchor_first() {
    use super::selection::{BufferPos, Selection};
    let backward = Selection::new(BufferPos::new(0, 5), BufferPos::new(0, 1));
    assert_eq!(backward.direction, Direction::Backward);
    let s = SelectionSet::singleton(backward, buf(), ver());
    let cmd = s.to_kakoune_command().expect("non-empty set");
    let rendered = render_kakoune_command(&cmd);
    // anchor=(0,5) → 1.6, cursor=(0,1) → 1.2
    assert!(
        rendered.contains("select 1.6,1.2"),
        "backward selection must emit anchor before cursor; got {rendered}"
    );
}

#[test]
fn to_kakoune_command_multi_line_anchor_cursor() {
    use super::selection::{BufferPos, Selection};
    let multi = Selection::new(BufferPos::new(2, 3), BufferPos::new(7, 4));
    let s = SelectionSet::singleton(multi, buf(), ver());
    let cmd = s.to_kakoune_command().expect("non-empty set");
    let rendered = render_kakoune_command(&cmd);
    assert!(
        rendered.contains("select 3.4,8.5"),
        "expected `select 3.4,8.5`; got {rendered}"
    );
}
