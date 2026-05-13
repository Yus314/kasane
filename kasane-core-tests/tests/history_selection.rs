//! ADR-035 §2 — selection round-trip through the history backend.
//!
//! Tests that a `SelectionSet` committed alongside text can be
//! recovered via `text_at(Time::At(v))` partner method
//! `selection_at(Time::At(v))`, both at the AppState and AppView
//! layers.

use std::sync::Arc;

use kasane_core::history::{HistoryBackend, Time};
use kasane_core::plugin::AppView;
use kasane_core::state::AppState;
use kasane_core::state::selection::{BufferId, BufferPos, BufferVersion, Selection};
use kasane_core::state::selection_set::SelectionSet;

fn buf() -> BufferId {
    BufferId::new("history-selection-test")
}

fn sel(line: u32, c0: u32, c1: u32) -> Selection {
    Selection::new(BufferPos::new(line, c0), BufferPos::new(line, c1))
}

fn build_set(sels: Vec<Selection>, version: BufferVersion) -> SelectionSet {
    SelectionSet::from_iter(sels, buf(), version)
}

#[test]
fn selection_round_trips_via_app_state() {
    let state = AppState::default();
    let bv = BufferVersion(0);
    let payload = build_set(vec![sel(0, 0, 5), sel(2, 0, 5)], bv);

    let v = state.commit_snapshot(buf(), bv, Arc::from("text"), payload.clone());
    let recovered = state.selection_at(Time::At(v)).expect("snapshot exists");

    assert_eq!(recovered, payload);
}

#[test]
fn selection_at_now_returns_latest() {
    let state = AppState::default();

    state.commit_snapshot(
        buf(),
        BufferVersion(0),
        Arc::from("v0"),
        build_set(vec![sel(0, 0, 5)], BufferVersion(0)),
    );
    state.commit_snapshot(
        buf(),
        BufferVersion(1),
        Arc::from("v1"),
        build_set(vec![sel(1, 0, 5)], BufferVersion(1)),
    );
    let v2 = state.commit_snapshot(
        buf(),
        BufferVersion(2),
        Arc::from("v2"),
        build_set(vec![sel(2, 0, 5)], BufferVersion(2)),
    );

    let now = state.selection_at(Time::Now).expect("Time::Now");
    let at_v2 = state.selection_at(Time::At(v2)).expect("Time::At(v2)");
    assert_eq!(now, at_v2);
    assert_eq!(now.primary().unwrap().min().line, 2);
}

#[test]
fn selection_round_trips_via_app_view() {
    let state = AppState::default();
    let bv = BufferVersion(0);
    let payload = build_set(vec![sel(3, 7, 12)], bv);
    let v = state.commit_snapshot(buf(), bv, Arc::from("plugin-perspective"), payload.clone());

    // Plugin handler perspective.
    let view = AppView::new(&state);
    let from_view = view.selection_at(Time::At(v)).expect("Time::At");
    assert_eq!(from_view, payload);
}

#[test]
fn selection_set_returns_some_for_matching_buffer() {
    let state = AppState::default();
    let bv = BufferVersion(0);
    let payload = build_set(vec![sel(0, 0, 5)], bv);
    let v = state.commit_snapshot(buf(), bv, Arc::from("hello"), payload.clone());

    let view = AppView::new(&state);
    let got = view.selection_set(&buf(), Time::At(v)).expect("matches");
    assert_eq!(got, payload);
}

#[test]
fn selection_set_returns_none_for_mismatched_buffer() {
    let state = AppState::default();
    let bv = BufferVersion(0);
    let payload = build_set(vec![sel(0, 0, 5)], bv);
    let v = state.commit_snapshot(buf(), bv, Arc::from("hello"), payload);

    let view = AppView::new(&state);
    let other = BufferId::new("a-different-buffer");
    assert_eq!(view.selection_set(&other, Time::At(v)), None);
}

#[test]
fn selection_set_at_now_uses_latest_snapshot_buffer() {
    let state = AppState::default();

    // Two commits to the same (default test) buffer.
    let payload_a = build_set(vec![sel(0, 0, 5)], BufferVersion(0));
    let payload_b = build_set(vec![sel(1, 0, 5)], BufferVersion(1));

    state.commit_snapshot(buf(), BufferVersion(0), Arc::from("a"), payload_a);
    state.commit_snapshot(buf(), BufferVersion(1), Arc::from("b"), payload_b.clone());

    let view = AppView::new(&state);
    let now = view.selection_set(&buf(), Time::Now).expect("now");
    assert_eq!(now, payload_b);
}

#[test]
fn selection_set_at_now_returns_none_for_other_buffer_after_commit() {
    let state = AppState::default();
    let payload = build_set(vec![sel(0, 0, 5)], BufferVersion(0));
    state.commit_snapshot(buf(), BufferVersion(0), Arc::from("x"), payload);

    let view = AppView::new(&state);
    let other = BufferId::new("nope");
    assert_eq!(view.selection_set(&other, Time::Now), None);
}

#[test]
fn selection_set_returns_none_for_empty_history() {
    let state = AppState::default();
    let view = AppView::new(&state);
    assert_eq!(view.selection_set(&buf(), Time::Now), None);
}

#[test]
fn empty_state_returns_none_for_selection() {
    let state = AppState::default();
    assert_eq!(state.selection_at(Time::Now), None);

    let view = AppView::new(&state);
    assert_eq!(view.selection_at(Time::Now), None);
}

#[test]
fn text_and_selection_share_version_id() {
    // After committing both text and selection in one call, querying
    // each at the same VersionId returns the values that were
    // committed together — an explicit witness that the snapshot
    // bundles the two payloads.
    let state = AppState::default();
    let bv = BufferVersion(0);
    let payload = build_set(vec![sel(0, 0, 10), sel(1, 0, 10)], bv);

    let v = state.commit_snapshot(buf(), bv, Arc::from("paired"), payload.clone());

    let text = state.text_at(Time::At(v)).expect("text");
    let selection = state.selection_at(Time::At(v)).expect("selection");

    assert_eq!(&*text, "paired");
    assert_eq!(selection, payload);
}

#[test]
fn auto_commit_via_apply_pairs_text_with_projected_selection() {
    // The apply auto-commit hook now projects `inference.selections`
    // (the heuristic-detected ranges) into the canonical
    // `SelectionSet` via `selections_to_set` before committing. With
    // the default-style atoms used in this test, the heuristic does
    // not detect any selections — selection_at therefore returns an
    // empty set. The "real" projection path (with styled selection
    // atoms) is exercised in
    // `auto_commit_apply_with_styled_atoms_projects_selection` below.
    use compact_str::CompactString;
    use kasane_core::protocol::{Atom, Coord, KakouneRequest, UnresolvedStyle};

    let mut state = AppState::default();
    state.runtime.rows = 5;
    state.runtime.cols = 80;

    let atom_text = |s: &str| {
        Atom::with_style(
            CompactString::from(s),
            kasane_core::protocol::Style::default(),
        )
    };

    state.apply(KakouneRequest::Draw {
        lines: vec![vec![atom_text("auto-committed")]],
        cursor_pos: Coord { line: 0, column: 0 },
        default_style: Arc::new(UnresolvedStyle::default()),
        padding_style: Arc::new(UnresolvedStyle::default()),
        widget_columns: 0,
    });

    let v = state.history.current_version();
    let selection = state
        .selection_at(Time::At(v))
        .expect("auto-commit snapshot");
    assert!(
        selection.is_empty(),
        "default-style draw produces no heuristic selections, so projection is empty",
    );
}

#[test]
fn empty_state_has_empty_current_selection_set() {
    let state = AppState::default();
    let view = AppView::new(&state);
    assert!(view.current_selection_set().is_empty());
}

#[test]
fn current_selection_set_consistent_with_history_after_apply() {
    // After apply(), AppView::current_selection_set() and
    // AppView::selection_at(Time::Now) should agree on the projected
    // SelectionSet — both are populated from the same heuristic
    // detector output.
    use compact_str::CompactString;
    use kasane_core::protocol::{
        Atom, Brush, Coord, KakouneRequest, NamedColor, Style, UnresolvedStyle,
    };

    let mut state = AppState::default();
    state.runtime.rows = 5;
    state.runtime.cols = 80;

    let selection_bg_style = Style {
        bg: Brush::Named(NamedColor::Blue),
        ..Style::default()
    };
    let selected = |s: &str| Atom::with_style(CompactString::from(s), selection_bg_style.clone());

    state.apply(KakouneRequest::Draw {
        lines: vec![vec![selected("hello"), selected(" "), selected("world")]],
        cursor_pos: Coord { line: 0, column: 5 },
        default_style: Arc::new(UnresolvedStyle::default()),
        padding_style: Arc::new(UnresolvedStyle::default()),
        widget_columns: 0,
    });

    let view = AppView::new(&state);
    let current = view.current_selection_set().clone();
    let from_history = view.selection_at(Time::Now).expect("Time::Now");

    assert_eq!(
        current, from_history,
        "current_selection_set must match the latest history snapshot",
    );
    assert!(
        !current.is_empty(),
        "heuristic should detect a selection from styled atoms",
    );
}

#[test]
fn current_selection_set_clears_when_styling_disappears() {
    use compact_str::CompactString;
    use kasane_core::protocol::{
        Atom, Brush, Coord, KakouneRequest, NamedColor, Style, UnresolvedStyle,
    };

    let mut state = AppState::default();
    state.runtime.rows = 5;
    state.runtime.cols = 80;

    // First apply: styled selection present.
    let selection_bg_style = Style {
        bg: Brush::Named(NamedColor::Blue),
        ..Style::default()
    };
    let selected = |s: &str| Atom::with_style(CompactString::from(s), selection_bg_style.clone());

    state.apply(KakouneRequest::Draw {
        lines: vec![vec![selected("hi")]],
        cursor_pos: Coord { line: 0, column: 1 },
        default_style: Arc::new(UnresolvedStyle::default()),
        padding_style: Arc::new(UnresolvedStyle::default()),
        widget_columns: 0,
    });

    assert!(!AppView::new(&state).current_selection_set().is_empty());

    // Second apply: plain atoms, no selection styling.
    let plain = |s: &str| Atom::with_style(CompactString::from(s), Style::default());
    state.apply(KakouneRequest::Draw {
        lines: vec![vec![plain("hi")]],
        cursor_pos: Coord { line: 0, column: 1 },
        default_style: Arc::new(UnresolvedStyle::default()),
        padding_style: Arc::new(UnresolvedStyle::default()),
        widget_columns: 0,
    });

    let view = AppView::new(&state);
    assert!(
        view.current_selection_set().is_empty(),
        "current_selection_set should clear when heuristic finds no styled atoms",
    );
}

#[test]
fn auto_commit_apply_with_styled_atoms_projects_selection() {
    // When the heuristic detector finds a selection (atoms with a
    // non-default bg adjacent to the cursor), the apply auto-commit
    // projects it into the SelectionSet. This test forces a
    // selection-bg face into the atom stream and confirms the
    // projection lands in history.
    use compact_str::CompactString;
    use kasane_core::protocol::{
        Atom, Brush, Coord, KakouneRequest, NamedColor, Style, UnresolvedStyle,
    };

    let mut state = AppState::default();
    state.runtime.rows = 5;
    state.runtime.cols = 80;

    // Build atoms with a distinct selection background. The heuristic
    // requires bg != default to recognise them as selection.
    let selection_bg_style = Style {
        bg: Brush::Named(NamedColor::Blue), // arbitrary non-default colour
        ..Style::default()
    };
    let plain = |s: &str| Atom::with_style(CompactString::from(s), Style::default());
    let selected = |s: &str| Atom::with_style(CompactString::from(s), selection_bg_style.clone());

    // Cursor at column 5 on a line with selection-bg atoms covering
    // columns 0..10.
    let line: Vec<Atom> = vec![
        selected("hello"),
        selected(" "),
        selected("world"),
        plain("!"),
    ];

    state.apply(KakouneRequest::Draw {
        lines: vec![line],
        cursor_pos: Coord { line: 0, column: 5 },
        default_style: Arc::new(UnresolvedStyle::default()),
        padding_style: Arc::new(UnresolvedStyle::default()),
        widget_columns: 0,
    });

    let v = state.history.current_version();
    let selection = state.selection_at(Time::At(v)).expect("snapshot");

    // The heuristic detected at least one selection range. The
    // exact range depends on the detector's scan, which we don't
    // pin here — only that the projection produced *something*
    // non-empty proves the wire-through.
    assert!(
        !selection.is_empty(),
        "heuristic should detect at least one selection from styled atoms",
    );
    // And the projected set must reference the correct buffer.
    assert_eq!(selection.buffer().0, "active");
}
