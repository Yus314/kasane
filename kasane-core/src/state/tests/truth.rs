//! Structural witness for the `Truth<'a>` projection.
//!
//! These tests pin the invariant that `Truth::ACCESSOR_NAMES` exactly
//! matches the set of `#[epistemic(observed)]` fields declared on
//! `AppState`. Adding, removing, or reclassifying an observed field
//! without updating `Truth` will fail one of these tests.

use std::collections::BTreeSet;

use crate::state::{AppState, Truth};

fn observed_field_set() -> BTreeSet<&'static str> {
    AppState::FIELDS_BY_CATEGORY
        .iter()
        .find(|(cat, _)| *cat == "observed")
        .map(|(_, fields)| fields.iter().copied().collect())
        .unwrap_or_default()
}

fn accessor_name_set() -> BTreeSet<&'static str> {
    Truth::ACCESSOR_NAMES.iter().copied().collect()
}

#[test]
fn accessor_set_matches_observed_fields() {
    let observed = observed_field_set();
    let accessors = accessor_name_set();
    assert_eq!(
        accessors,
        observed,
        "Truth::ACCESSOR_NAMES must match AppState's #[epistemic(observed)] fields exactly. \
         Missing from Truth: {:?}. Extra on Truth: {:?}.",
        observed.difference(&accessors).collect::<Vec<_>>(),
        accessors.difference(&observed).collect::<Vec<_>>(),
    );
}

#[test]
fn truth_exposes_no_non_observed_fields() {
    // No accessor name on Truth may correspond to a non-observed AppState field.
    let accessors = accessor_name_set();
    for (field, category) in AppState::FIELD_EPISTEMIC_MAP {
        if accessors.contains(*field) {
            assert_eq!(
                *category, "observed",
                "Truth exposes field `{field}` but its epistemic category is `{category}`, \
                 not `observed`. The projection must only touch observed fields.",
            );
        }
    }
}

#[test]
fn observed_fields_are_all_reachable_via_truth() {
    // Every observed field must be covered by some Truth accessor.
    let observed = observed_field_set();
    let accessors = accessor_name_set();
    let missing: Vec<&&str> = observed.difference(&accessors).collect();
    assert!(
        missing.is_empty(),
        "observed fields not reachable through Truth: {missing:?}",
    );
}
