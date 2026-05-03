//! ADR-035 §2 — auto-commit hook in `AppState::apply()`.
//!
//! Verifies that a `KakouneRequest::Draw` round-trips through
//! `AppState::apply` into the history backend, so `text_at(Time::Now)`
//! returns the line text Kakoune just sent.

use std::sync::Arc;

use compact_str::CompactString;
use kasane_core::history::{HistoryBackend, Time};
use kasane_core::protocol::{Atom, Coord, KakouneRequest, UnresolvedStyle};
use kasane_core::state::AppState;

fn atom(s: &str) -> Atom {
    Atom::with_style(
        CompactString::from(s),
        kasane_core::protocol::Style::default(),
    )
}

fn default_style() -> Arc<UnresolvedStyle> {
    Arc::new(UnresolvedStyle::default())
}

fn draw(lines: Vec<Vec<Atom>>) -> KakouneRequest {
    KakouneRequest::Draw {
        lines,
        cursor_pos: Coord { line: 0, column: 0 },
        default_style: default_style(),
        padding_style: default_style(),
        widget_columns: 0,
    }
}

#[test]
fn apply_draw_commits_text_to_history() {
    let mut state = AppState::default();
    state.runtime.rows = 5;
    state.runtime.cols = 80;

    state.apply(draw(vec![
        vec![atom("hello "), atom("world")],
        vec![atom("second line")],
    ]));

    let now = state.text_at(Time::Now).expect("Time::Now after apply");
    assert_eq!(&*now, "hello world\nsecond line");
}

#[test]
fn multiple_applies_produce_distinct_versions() {
    let mut state = AppState::default();
    state.runtime.rows = 5;
    state.runtime.cols = 80;

    state.apply(draw(vec![vec![atom("first")]]));
    let v_first = state.history.current_version();

    state.apply(draw(vec![vec![atom("second")]]));
    let v_second = state.history.current_version();

    state.apply(draw(vec![vec![atom("third")]]));
    let v_third = state.history.current_version();

    assert_ne!(v_first, v_second);
    assert_ne!(v_second, v_third);

    assert_eq!(state.text_at(Time::At(v_first)).as_deref(), Some("first"));
    assert_eq!(state.text_at(Time::At(v_second)).as_deref(), Some("second"));
    assert_eq!(state.text_at(Time::At(v_third)).as_deref(), Some("third"));
    assert_eq!(state.text_at(Time::Now).as_deref(), Some("third"));
}

#[test]
fn empty_buffer_commits_empty_text() {
    let mut state = AppState::default();
    state.runtime.rows = 5;
    state.runtime.cols = 80;

    state.apply(draw(vec![vec![]]));

    assert_eq!(state.text_at(Time::Now).as_deref(), Some(""));
}

#[test]
fn lines_are_joined_with_newline() {
    let mut state = AppState::default();
    state.runtime.rows = 5;
    state.runtime.cols = 80;

    state.apply(draw(vec![
        vec![atom("a")],
        vec![atom("b")],
        vec![atom("c")],
    ]));

    assert_eq!(state.text_at(Time::Now).as_deref(), Some("a\nb\nc"));
}

#[test]
fn non_buffer_messages_do_not_commit() {
    // A status-only update should not bump the history version, since
    // BUFFER_CONTENT is not set by DrawStatus.
    let mut state = AppState::default();
    state.runtime.rows = 5;
    state.runtime.cols = 80;

    state.apply(draw(vec![vec![atom("buffer")]]));
    let v_after_buffer = state.history.current_version();

    state.apply(KakouneRequest::DrawStatus {
        prompt: vec![],
        content: vec![atom("status")],
        content_cursor_pos: 0,
        mode_line: vec![],
        default_style: default_style(),
        style: kasane_core::protocol::StatusStyle::default(),
    });
    let v_after_status = state.history.current_version();

    assert_eq!(
        v_after_buffer, v_after_status,
        "DrawStatus must not commit to history",
    );
}
