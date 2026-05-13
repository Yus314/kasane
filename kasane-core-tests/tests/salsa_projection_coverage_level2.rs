//! Level 2 coverage test for ADR-030: every `#[epistemic(derived)]`,
//! `#[epistemic(heuristic)]`, and `#[epistemic(config)]` field on `AppState`
//! must either be surfaced in the Salsa input layer or carry an explicit
//! `salsa_opt_out = "<reason>"` declaration.
//!
//! Level 1 (`salsa_projection_coverage.rs`) witnessed the same property for
//! `#[epistemic(observed)]` fields — namely that the Salsa projection of
//! observed state is total, not lossy. Level 2 extends the invariant to the
//! Inference and Policy components of the world model W = (T, I, Π, S).
//!
//! The set of fields currently surfaced by Salsa is maintained in this test
//! as the hand-curated `SALSA_SURFACED_FIELDS` constant. When you add a new
//! Salsa input or plumb a new `AppState` field through
//! `sync_inputs_from_state`, add the field name here. When you deliberately
//! route a derived/heuristic/config field around Salsa — for example, a
//! paint-only config like `padding_char` — declare it on the field with
//! `#[epistemic(config, salsa_opt_out = "<reason>")]`.
//!
//! See `docs/semantics.md` §2.5 / ADR-030 Level 2 for the formal statement.

use std::collections::BTreeSet;

use kasane_core::state::AppState;

/// Fields of `AppState` that are currently surfaced through a Salsa input
/// struct defined in `kasane-core/src/salsa_inputs.rs`.
///
/// Keep this list in sync with `salsa_inputs.rs`. The set is intentionally
/// curated by hand rather than derived from Salsa's own metadata, so that
/// adding a new Salsa input is a deliberate edit that the reviewer can see.
const SALSA_SURFACED_FIELDS: &[&str] = &[
    // BufferInput
    "lines",
    "default_face",
    "padding_face",
    "cursor_pos",
    "widget_columns",
    // CursorInput
    "cursor_mode",
    "cursor_count",
    "secondary_cursors",
    // StatusInput
    "status_prompt",
    "status_content",
    "status_content_cursor_pos",
    "status_line",
    "status_mode_line",
    "status_default_face",
    "status_style",
    // MenuInput
    "menu",
    // InfoInput
    "infos",
    // ConfigInput (ui_options lives on its own dirty flag, but is observed
    // and therefore out of scope for this test).
    "shadow_enabled",
    "status_at_top",
    "secondary_blend_ratio",
    "menu_position",
    "search_dropdown",
    "scrollbar_thumb",
    "scrollbar_track",
    "assistant_art",
];

fn level2_category_fields() -> Vec<(&'static str, &'static str)> {
    // Only derived, heuristic, and config are in scope for Level 2.
    // Observed is Level 1; session and runtime are out of scope.
    let in_scope = ["derived", "heuristic", "config"];
    let mut out = Vec::new();
    for (cat, fields) in AppState::FIELDS_BY_CATEGORY {
        if in_scope.contains(cat) {
            for f in *fields {
                out.push((*f, *cat));
            }
        }
    }
    out
}

fn salsa_opt_out_set() -> BTreeSet<&'static str> {
    AppState::SALSA_OPT_OUTS
        .iter()
        .map(|(name, _)| *name)
        .collect()
}

fn salsa_surfaced_set() -> BTreeSet<&'static str> {
    SALSA_SURFACED_FIELDS.iter().copied().collect()
}

#[test]
fn every_level2_field_is_surfaced_or_opted_out() {
    let surfaced = salsa_surfaced_set();
    let opted_out = salsa_opt_out_set();

    let mut uncovered = Vec::new();
    for (field, cat) in level2_category_fields() {
        if !surfaced.contains(field) && !opted_out.contains(field) {
            uncovered.push((field, cat));
        }
    }

    assert!(
        uncovered.is_empty(),
        "ADR-030 Level 2: the following `{}` fields are neither surfaced in the Salsa input layer \
         nor declared as `#[epistemic(..., salsa_opt_out = \"<reason>\")]`:\n  {:?}\n\
         Either plumb the field through `salsa_inputs.rs` / `salsa_sync.rs`, or declare an \
         explicit opt-out with a documented reason.",
        "derived|heuristic|config",
        uncovered,
    );
}

#[test]
fn opt_out_reasons_are_nonempty() {
    // Every declared opt-out must carry a non-empty justification — the
    // whole point of the mechanism is to make the decision reviewable.
    for (field, reason) in AppState::SALSA_OPT_OUTS {
        assert!(
            !reason.trim().is_empty(),
            "field `{field}` declares an empty `salsa_opt_out` reason. \
             Provide a short justification of why this field is intentionally \
             not surfaced through Salsa.",
        );
    }
}

#[test]
fn opt_out_fields_are_not_simultaneously_surfaced() {
    // A field cannot be both surfaced in Salsa and opted out — that would be
    // a contradiction in the declaration.
    let surfaced = salsa_surfaced_set();
    let mut conflicts = Vec::new();
    for (field, _) in AppState::SALSA_OPT_OUTS {
        if surfaced.contains(*field) {
            conflicts.push(*field);
        }
    }
    assert!(
        conflicts.is_empty(),
        "fields declared `salsa_opt_out` but also listed in `SALSA_SURFACED_FIELDS`: {conflicts:?}. \
         Pick one: either surface the field in Salsa or opt out, but not both.",
    );
}

#[test]
fn opt_out_only_on_level2_categories() {
    // salsa_opt_out is only meaningful on derived / heuristic / config
    // fields. Observed fields must never be opted out (Level 1 forbids a
    // lossy observed projection). Session / runtime fields are not subject
    // to the projection invariant at all, so declaring opt-out on them is
    // noise.
    let in_scope: BTreeSet<&str> = ["derived", "heuristic", "config"].into_iter().collect();
    let cat_map: std::collections::HashMap<&str, &str> =
        AppState::FIELD_EPISTEMIC_MAP.iter().copied().collect();

    let mut bad = Vec::new();
    for (field, _reason) in AppState::SALSA_OPT_OUTS {
        match cat_map.get(field) {
            Some(cat) if !in_scope.contains(cat) => bad.push((*field, *cat)),
            None => bad.push((*field, "<unknown>")),
            _ => {}
        }
    }
    assert!(
        bad.is_empty(),
        "`salsa_opt_out` declared on fields outside the Level 2 scope \
         (derived|heuristic|config): {bad:?}",
    );
}
