//! Salsa tracked functions for derived state computation.
//!
//! These are the Layer 3 declarative queries that Salsa automatically
//! memoizes and revalidates based on input changes.

use std::sync::Arc;

use crate::history::Time;
use crate::protocol::CursorMode;
use crate::render::CursorStyle;
use crate::salsa_db::KasaneDb;
use crate::salsa_inputs::{BufferInput, ConfigInput, CursorInput, StatusInput};

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

/// ADR-035 §2 PoC — Time-aware text query.
///
/// Demonstrates that `Time` integrates as a Salsa tracked-function
/// parameter. The cache keys on `(BufferInput, Time)`, so successive
/// calls with the same `Time` and unchanged `BufferInput` hit the
/// memoized result.
///
/// - `Time::Now` projects the current `BufferInput.lines` to plain
///   text (lossy: drops style payloads, matches the `lines_to_text`
///   convention used by `AppState::apply`'s auto-commit hook).
/// - `Time::At(_v)` returns `None`. The `HistoryBackend` is held on
///   `AppState`, not as a Salsa input, so this query cannot reach
///   past snapshots without a richer wiring. A follow-up PR (or a
///   dedicated `HistoryInput` Salsa input) will lift the backend
///   into the Salsa world; for now `Time::At` is a *contract slot*
///   that documents how the API will work once the backend is
///   reachable.
#[salsa::tracked]
pub fn text_at_time(db: &dyn KasaneDb, buffer: BufferInput, time: Time) -> Option<Arc<str>> {
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
        Time::At(_) => None,
    }
}

#[cfg(test)]
mod time_query_tests {
    use std::sync::Arc;

    use compact_str::CompactString;

    use super::*;
    use crate::history::{Time, VersionId};
    use crate::protocol::Atom;
    use crate::salsa_db::KasaneDatabase;
    use crate::salsa_inputs::BufferInput;

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

    #[test]
    fn time_now_projects_current_buffer() {
        let db = KasaneDatabase::default();
        let buffer = make_buffer(&db, vec!["hello", "world"]);
        let text = text_at_time(&db, buffer, Time::Now).unwrap();
        assert_eq!(&*text, "hello\nworld");
    }

    #[test]
    fn time_at_returns_none_until_history_is_a_salsa_input() {
        let db = KasaneDatabase::default();
        let buffer = make_buffer(&db, vec!["hello"]);
        let text = text_at_time(&db, buffer, Time::At(VersionId(0)));
        assert_eq!(text, None);
    }

    #[test]
    fn distinct_time_values_are_distinct_cache_keys() {
        // Time::Now and Time::At(0) must produce different results
        // even though they share the same BufferInput.
        let db = KasaneDatabase::default();
        let buffer = make_buffer(&db, vec!["one"]);
        let now = text_at_time(&db, buffer, Time::Now);
        let at0 = text_at_time(&db, buffer, Time::At(VersionId(0)));
        assert_ne!(now, at0);
        assert!(now.is_some());
        assert!(at0.is_none());
    }

    #[test]
    fn buffer_input_change_invalidates_time_now() {
        use salsa::Setter;

        // Updating the BufferInput's lines must cause Time::Now to
        // recompute and reflect the new content.
        let mut db = KasaneDatabase::default();
        let buffer = make_buffer(&db, vec!["before"]);
        assert_eq!(&*text_at_time(&db, buffer, Time::Now).unwrap(), "before");

        let new_lines = Arc::new(vec![vec![atom("after")]]);
        buffer.set_lines(&mut db).to(new_lines);

        assert_eq!(&*text_at_time(&db, buffer, Time::Now).unwrap(), "after");
    }

    #[test]
    fn empty_buffer_yields_empty_text() {
        let db = KasaneDatabase::default();
        let buffer = make_buffer(&db, vec![]);
        let text = text_at_time(&db, buffer, Time::Now).unwrap();
        assert_eq!(&*text, "");
    }
}
