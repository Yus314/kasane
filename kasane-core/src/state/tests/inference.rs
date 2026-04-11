//! Structural witness for the `Inference<'a>` projection.
//!
//! These tests pin the invariant that `Inference::INFERENCE_ACCESSOR_NAMES`
//! exactly matches the union of `#[epistemic(derived)]` and
//! `#[epistemic(heuristic)]` fields declared on `AppState`. Adding, removing,
//! or reclassifying a derived/heuristic field without updating `Inference`
//! will fail one of these tests.

use std::collections::BTreeSet;

use crate::state::{AppState, Inference};

fn level2_inference_field_set() -> BTreeSet<&'static str> {
    let mut out = BTreeSet::new();
    for (cat, fields) in AppState::FIELDS_BY_CATEGORY {
        if *cat == "derived" || *cat == "heuristic" {
            for f in *fields {
                out.insert(*f);
            }
        }
    }
    out
}

fn accessor_name_set() -> BTreeSet<&'static str> {
    Inference::INFERENCE_ACCESSOR_NAMES
        .iter()
        .copied()
        .collect()
}

#[test]
fn accessor_set_matches_derived_and_heuristic_fields() {
    let expected = level2_inference_field_set();
    let accessors = accessor_name_set();
    assert_eq!(
        accessors,
        expected,
        "Inference::INFERENCE_ACCESSOR_NAMES must match AppState's \
         #[epistemic(derived)] ∪ #[epistemic(heuristic)] fields exactly. \
         Missing from Inference: {:?}. Extra on Inference: {:?}.",
        expected.difference(&accessors).collect::<Vec<_>>(),
        accessors.difference(&expected).collect::<Vec<_>>(),
    );
}

#[test]
fn inference_exposes_no_out_of_scope_fields() {
    // No accessor on Inference may correspond to a field outside the
    // derived ∪ heuristic scope. Observed / config / session / runtime are
    // projected by Truth / Policy / (no projection) / (no projection)
    // respectively.
    let accessors = accessor_name_set();
    for (field, category) in AppState::FIELD_EPISTEMIC_MAP {
        if accessors.contains(*field) {
            assert!(
                *category == "derived" || *category == "heuristic",
                "Inference exposes field `{field}` but its epistemic category \
                 is `{category}`. Level 2 restricts Inference to derived and \
                 heuristic fields.",
            );
        }
    }
}

#[test]
fn derived_and_heuristic_fields_are_all_reachable_via_inference() {
    let expected = level2_inference_field_set();
    let accessors = accessor_name_set();
    let missing: Vec<&&str> = expected.difference(&accessors).collect();
    assert!(
        missing.is_empty(),
        "derived/heuristic fields not reachable through Inference: {missing:?}",
    );
}
