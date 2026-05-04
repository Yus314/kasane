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
/// Resolves both ends of the `Time` enum through the Salsa cache:
///
/// - `Time::Now` projects the current `BufferInput.lines` to plain
///   text (lossy: drops style payloads, matches the `lines_to_text`
///   convention used by `AppState::apply`'s auto-commit hook). The
///   cache invalidates when `BufferInput.lines` changes.
/// - `Time::At(v)` resolves through the configured `HistoryInput`'s
///   `InMemoryRing` — returns the snapshot's text if `v` is still
///   in the ring, `None` if it was evicted or never observed. Past
///   snapshots are immutable, so these cache entries are valid
///   forever once computed.
///
/// Cache key: `(BufferInput, HistoryInput, Time)`.
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

/// ADR-035 §2 — Time-aware `SelectionSet` query.
///
/// Companion to [`text_at_time`] — resolves a `SelectionSet` at the
/// requested `Time` through the same `HistoryInput`. Demonstrates
/// that the Time-aware Salsa pattern generalises beyond text.
///
/// - `Time::Now` resolves to `history.current_version(db)` and reads
///   the snapshot's `selection`. The Salsa cache invalidates when
///   the caller pushes a new `current_version` after a commit (see
///   `HistoryInput::set_current_version`).
/// - `Time::At(v)` reads the snapshot for the specific version,
///   returning `None` for evicted or unknown versions.
///
/// Cache key: `(HistoryInput, Time)`. No `BufferInput` dependency —
/// the SelectionSet lives entirely in history (committed alongside
/// text by `AppState::apply`'s auto-commit hook).
#[salsa::tracked]
pub fn selection_at_time(
    db: &dyn KasaneDb,
    history: HistoryInput,
    time: Time,
) -> Option<crate::state::selection_set::SelectionSet> {
    use crate::history::HistoryBackend;
    let v = match time {
        Time::Now => history.current_version(db),
        Time::At(v) => v,
    };
    history.backend(db).snapshot(v).ok().map(|s| s.selection)
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
        let input = HistoryInput::new(db, ring.clone(), VersionId::INITIAL);
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
        let history = HistoryInput::new(&db, ring.clone(), VersionId::INITIAL);

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

    // -----------------------------------------------------------------
    // selection_at_time
    // -----------------------------------------------------------------

    use crate::state::selection::{BufferPos, Selection};

    fn sel(line: u32, c0: u32, c1: u32) -> Selection {
        Selection::new(BufferPos::new(line, c0), BufferPos::new(line, c1))
    }

    #[test]
    fn selection_at_now_empty_history_returns_none() {
        let db = KasaneDatabase::default();
        let history = empty_history(&db);
        assert_eq!(selection_at_time(&db, history, Time::Now), None);
    }

    #[test]
    fn selection_at_returns_committed_payload() {
        use salsa::Setter;

        let mut db = KasaneDatabase::default();
        let (history, ring) = make_history(&db);

        let payload =
            SelectionSet::singleton(sel(2, 5, 10), BufferId::new("test"), BufferVersion(0));
        let v = ring.commit(
            Arc::from("text"),
            payload.clone(),
            BufferId::new("test"),
            BufferVersion(0),
        );

        // Sync current_version so Time::Now picks up v.
        history.set_current_version(&mut db).to(v);

        let now = selection_at_time(&db, history, Time::Now).unwrap();
        let at = selection_at_time(&db, history, Time::At(v)).unwrap();
        assert_eq!(now, payload);
        assert_eq!(at, payload);
        assert_eq!(now, at);
    }

    #[test]
    fn selection_at_now_invalidates_when_current_version_advances() {
        use salsa::Setter;

        let mut db = KasaneDatabase::default();
        let (history, ring) = make_history(&db);

        // Commit two versions with different selections.
        let p0 = SelectionSet::singleton(sel(0, 0, 5), BufferId::new("test"), BufferVersion(0));
        let p1 = SelectionSet::singleton(sel(1, 0, 5), BufferId::new("test"), BufferVersion(1));
        let v0 = ring.commit(Arc::from("a"), p0, BufferId::new("test"), BufferVersion(0));
        let v1 = ring.commit(
            Arc::from("b"),
            p1.clone(),
            BufferId::new("test"),
            BufferVersion(1),
        );

        // Initially Time::Now points at v0.
        history.set_current_version(&mut db).to(v0);
        let first = selection_at_time(&db, history, Time::Now).unwrap();
        assert_eq!(first.primary().unwrap().min().line, 0);

        // Advance current_version → Salsa invalidates Time::Now.
        history.set_current_version(&mut db).to(v1);
        let second = selection_at_time(&db, history, Time::Now).unwrap();
        assert_eq!(second, p1);
    }

    #[test]
    fn selection_at_evicted_returns_none() {
        let db = KasaneDatabase::default();
        let ring = Arc::new(InMemoryRing::with_capacity(1));
        let history = HistoryInput::new(&db, ring.clone(), VersionId::INITIAL);

        let p0 = SelectionSet::default_empty();
        let v0 = ring.commit(Arc::from("a"), p0, BufferId::new("test"), BufferVersion(0));
        let _v1 = ring.commit(
            Arc::from("b"),
            SelectionSet::default_empty(),
            BufferId::new("test"),
            BufferVersion(1),
        );

        // v0 evicted by FIFO at capacity 1.
        assert_eq!(selection_at_time(&db, history, Time::At(v0)), None);
    }
}
