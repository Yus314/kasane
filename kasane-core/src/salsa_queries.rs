//! Salsa tracked functions for derived state computation.
//!
//! These are the Layer 3 declarative queries that Salsa automatically
//! memoizes and revalidates based on input changes.

use std::sync::Arc;

use crate::history::Time;
use crate::protocol::CursorMode;
use crate::render::CursorStyle;
use crate::salsa_db::KasaneDb;
use crate::salsa_inputs::{BufferInput, ConfigInput, CursorInput, HistoryInput, StatusInput};

/// Available height (rows - status bar).
#[salsa::tracked]
pub fn available_height(db: &dyn KasaneDb, config: ConfigInput) -> u16 {
    config.rows(db).saturating_sub(1)
}

/// Whether we're in prompt mode.
#[salsa::tracked]
pub fn is_prompt_mode(db: &dyn KasaneDb, cursor: CursorInput) -> bool {
    cursor.cursor_mode(db) == CursorMode::Prompt
}

/// Cursor style derived from config + cursor mode + status mode line.
///
/// This is the default cursor style without plugin overrides.
/// Plugin overrides are applied in Stage 2 (outside Salsa).
#[salsa::tracked]
pub fn cursor_style_query(
    db: &dyn KasaneDb,
    config: ConfigInput,
    cursor: CursorInput,
    status: StatusInput,
) -> CursorStyle {
    crate::state::derived::derive_cursor_style(
        // We don't have ui_options in Salsa inputs yet — pass empty map.
        // The ui_option override is rare and handled by the full cursor_style()
        // function in the rendering pipeline (Stage 2).
        &std::collections::HashMap::new(),
        config.focused(db),
        cursor.cursor_mode(db),
        status.status_mode_line(db),
    )
}

/// ADR-035 §2 — Time-aware text query.
///
/// Demonstrates that `Time` integrates as a Salsa tracked-function
/// parameter and resolves both ends of the `Time` enum:
///
/// - `Time::Now` projects the current `BufferInput.lines` to plain
///   text (lossy: drops style payloads, matches the `lines_to_text`
///   convention used by `AppState::apply`'s auto-commit hook).
/// - `Time::At(v)` resolves through the configured `HistoryInput`'s
///   `InMemoryRing` — returns the snapshot's text if `v` is still
///   in the ring, `None` if it was evicted or never observed.
///
/// The Salsa cache keys on `(BufferInput, HistoryInput, Time)`. The
/// `HistoryInput`'s `Arc` is stable for the session, so cache
/// entries for `Time::At(v)` are valid forever once computed (past
/// snapshots are immutable). `Time::Now` invalidates correctly when
/// the underlying `BufferInput.lines` changes.
#[salsa::tracked]
pub fn text_at_time(
    db: &dyn KasaneDb,
    buffer: BufferInput,
    history: HistoryInput,
    time: Time,
) -> Option<Arc<str>> {
    use crate::history::HistoryBackend;
    match time {
        Time::Now => {
            let lines = buffer.lines(db);
            let mut out = String::new();
            for (i, line) in lines.iter().enumerate() {
                if i > 0 {
                    out.push('\n');
                }
                for atom in line {
                    out.push_str(&atom.contents);
                }
            }
            Some(out.into())
        }
        Time::At(v) => history.backend(db).snapshot(v).ok().map(|s| s.text),
    }
}

#[cfg(test)]
mod time_query_tests {
    use std::sync::Arc;

    use compact_str::CompactString;

    use super::*;
    use crate::history::{HistoryBackend, InMemoryRing, Time, VersionId};
    use crate::protocol::Atom;
    use crate::salsa_db::KasaneDatabase;
    use crate::salsa_inputs::{BufferInput, HistoryInput};
    use crate::state::selection::{BufferId, BufferVersion};
    use crate::state::selection_set::SelectionSet;

    fn atom(s: &str) -> Atom {
        Atom::with_style(CompactString::from(s), crate::protocol::Style::default())
    }

    fn make_buffer(db: &KasaneDatabase, lines: Vec<&str>) -> BufferInput {
        let lines = Arc::new(lines.into_iter().map(|s| vec![atom(s)]).collect::<Vec<_>>());
        BufferInput::new(
            db,
            lines,
            crate::protocol::Style::default(),
            crate::protocol::Style::default(),
            crate::protocol::Coord::default(),
            0,
        )
    }

    fn make_history(db: &KasaneDatabase) -> (HistoryInput, Arc<InMemoryRing>) {
        let ring = Arc::new(InMemoryRing::new());
        let input = HistoryInput::new(db, ring.clone());
        (input, ring)
    }

    fn empty_history(db: &KasaneDatabase) -> HistoryInput {
        make_history(db).0
    }

    #[test]
    fn time_now_projects_current_buffer() {
        let db = KasaneDatabase::default();
        let buffer = make_buffer(&db, vec!["hello", "world"]);
        let history = empty_history(&db);
        let text = text_at_time(&db, buffer, history, Time::Now).unwrap();
        assert_eq!(&*text, "hello\nworld");
    }

    #[test]
    fn time_at_returns_none_for_empty_history() {
        let db = KasaneDatabase::default();
        let buffer = make_buffer(&db, vec!["hello"]);
        let history = empty_history(&db);
        let text = text_at_time(&db, buffer, history, Time::At(VersionId(0)));
        assert_eq!(text, None);
    }

    #[test]
    fn time_at_returns_committed_snapshot() {
        let db = KasaneDatabase::default();
        let buffer = make_buffer(&db, vec!["live"]);
        let (history, ring) = make_history(&db);

        let v = ring.commit(
            Arc::from("past"),
            SelectionSet::default_empty(),
            BufferId::new("test"),
            BufferVersion(0),
        );

        let now = text_at_time(&db, buffer, history, Time::Now).unwrap();
        let at = text_at_time(&db, buffer, history, Time::At(v)).unwrap();
        assert_eq!(&*now, "live");
        assert_eq!(&*at, "past");
    }

    #[test]
    fn time_at_evicted_returns_none() {
        let db = KasaneDatabase::default();
        let buffer = make_buffer(&db, vec!["x"]);
        let ring = Arc::new(InMemoryRing::with_capacity(1));
        let history = HistoryInput::new(&db, ring.clone());

        let v0 = ring.commit(
            Arc::from("a"),
            SelectionSet::default_empty(),
            BufferId::new("test"),
            BufferVersion(0),
        );
        let v1 = ring.commit(
            Arc::from("b"),
            SelectionSet::default_empty(),
            BufferId::new("test"),
            BufferVersion(1),
        );

        // v0 evicted by FIFO at capacity 1.
        assert_eq!(text_at_time(&db, buffer, history, Time::At(v0)), None);
        assert_eq!(
            text_at_time(&db, buffer, history, Time::At(v1)).as_deref(),
            Some("b"),
        );
    }

    #[test]
    fn distinct_time_values_are_distinct_cache_keys() {
        let db = KasaneDatabase::default();
        let buffer = make_buffer(&db, vec!["one"]);
        let history = empty_history(&db);
        let now = text_at_time(&db, buffer, history, Time::Now);
        let at0 = text_at_time(&db, buffer, history, Time::At(VersionId(0)));
        assert_ne!(now, at0);
        assert!(now.is_some());
        assert!(at0.is_none());
    }

    #[test]
    fn buffer_input_change_invalidates_time_now() {
        use salsa::Setter;

        let mut db = KasaneDatabase::default();
        let buffer = make_buffer(&db, vec!["before"]);
        let history = empty_history(&db);
        assert_eq!(
            &*text_at_time(&db, buffer, history, Time::Now).unwrap(),
            "before"
        );

        let new_lines = Arc::new(vec![vec![atom("after")]]);
        buffer.set_lines(&mut db).to(new_lines);

        assert_eq!(
            &*text_at_time(&db, buffer, history, Time::Now).unwrap(),
            "after"
        );
    }

    #[test]
    fn empty_buffer_yields_empty_text() {
        let db = KasaneDatabase::default();
        let buffer = make_buffer(&db, vec![]);
        let history = empty_history(&db);
        let text = text_at_time(&db, buffer, history, Time::Now).unwrap();
        assert_eq!(&*text, "");
    }
}
