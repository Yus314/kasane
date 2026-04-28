#![allow(clippy::field_reassign_with_default)]
//! Regression test for the Salsa-layer projection of observed state.
//!
//! The `StatusInput` on the Salsa side used to store only the derived
//! `status_line` (plus `status_mode_line` / faces / style). The observed
//! components `status_prompt`, `status_content`, and
//! `status_content_cursor_pos` — all `#[epistemic(observed)]` on `AppState`
//! — were not projected, making the Salsa layer a lossy projection of the
//! observed facts. Under ADR-030 Level 1 (observed/policy separation),
//! the Salsa layer must not drop any observed field on ingress.
//!
//! See `docs/semantics.md` §2.5 and §13.13 for the formal statement.

use kasane_core::protocol::{Atom, Face};
use kasane_core::salsa_db::KasaneDatabase;
use kasane_core::salsa_sync::{SalsaInputHandles, sync_inputs_from_state};
use kasane_core::state::AppState;

fn atom(s: &str) -> Atom {
    Atom::from_face(Face::default(), s)
}

#[test]
fn status_prompt_is_projected_into_salsa() {
    let mut db = KasaneDatabase::default();
    let handles = SalsaInputHandles::new(&mut db);

    let mut state = AppState::default();
    state.observed.status_prompt = vec![atom(":")];
    sync_inputs_from_state(&mut db, &state, &handles);

    let stored = handles.status.status_prompt(&db);
    assert_eq!(
        stored, &state.observed.status_prompt,
        "StatusInput must project AppState::status_prompt verbatim",
    );
}

#[test]
fn status_content_is_projected_into_salsa() {
    let mut db = KasaneDatabase::default();
    let handles = SalsaInputHandles::new(&mut db);

    let mut state = AppState::default();
    state.observed.status_content = vec![atom("edit"), atom(" file.rs")];
    sync_inputs_from_state(&mut db, &state, &handles);

    let stored = handles.status.status_content(&db);
    assert_eq!(
        stored, &state.observed.status_content,
        "StatusInput must project AppState::status_content verbatim",
    );
}

#[test]
fn status_content_cursor_pos_is_projected_into_salsa() {
    let mut db = KasaneDatabase::default();
    let handles = SalsaInputHandles::new(&mut db);

    let mut state = AppState::default();
    state.observed.status_content_cursor_pos = 7;
    sync_inputs_from_state(&mut db, &state, &handles);

    assert_eq!(
        handles.status.status_content_cursor_pos(&db),
        7,
        "StatusInput must project AppState::status_content_cursor_pos verbatim",
    );
}

#[test]
fn distinct_prompt_same_line_is_distinguishable() {
    // Two states that share the same derived `status_line` but differ in
    // `status_prompt` must be distinguishable through the Salsa projection.
    // If Salsa only stored `status_line`, the two would collapse to the
    // same Salsa revision, violating A9-like observability.
    let mut db = KasaneDatabase::default();
    let handles = SalsaInputHandles::new(&mut db);

    let mut s1 = AppState::default();
    s1.observed.status_prompt = vec![atom(":")];
    s1.observed.status_content = vec![atom("edit")];
    s1.inference.status_line = vec![atom(":"), atom("edit")];
    sync_inputs_from_state(&mut db, &s1, &handles);
    let p1 = handles.status.status_prompt(&db).clone();

    let mut s2 = AppState::default();
    s2.observed.status_prompt = vec![atom(">")];
    s2.observed.status_content = vec![atom("edit")];
    // Intentionally leave s2.status_line equal to s1's so only the prompt
    // differs in the projection.
    s2.inference.status_line = vec![atom(":"), atom("edit")];
    sync_inputs_from_state(&mut db, &s2, &handles);
    let p2 = handles.status.status_prompt(&db).clone();

    assert_ne!(
        p1, p2,
        "observed prompt differences must survive the Salsa projection",
    );
}
