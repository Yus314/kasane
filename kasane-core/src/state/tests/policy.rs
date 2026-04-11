//! Structural witness for the `Policy<'a>` projection.
//!
//! These tests pin the invariant that `Policy::POLICY_ACCESSOR_NAMES`
//! exactly matches the set of `#[epistemic(config)]` fields declared on
//! `AppState`. Adding, removing, or reclassifying a config field without
//! updating `Policy` will fail one of these tests.

use std::collections::BTreeSet;

use crate::state::{AppState, Policy};

fn config_field_set() -> BTreeSet<&'static str> {
    AppState::FIELDS_BY_CATEGORY
        .iter()
        .find(|(cat, _)| *cat == "config")
        .map(|(_, fields)| fields.iter().copied().collect())
        .unwrap_or_default()
}

fn accessor_name_set() -> BTreeSet<&'static str> {
    Policy::POLICY_ACCESSOR_NAMES.iter().copied().collect()
}

#[test]
fn accessor_set_matches_config_fields() {
    let expected = config_field_set();
    let accessors = accessor_name_set();
    assert_eq!(
        accessors,
        expected,
        "Policy::POLICY_ACCESSOR_NAMES must match AppState's \
         #[epistemic(config)] fields exactly. \
         Missing from Policy: {:?}. Extra on Policy: {:?}.",
        expected.difference(&accessors).collect::<Vec<_>>(),
        accessors.difference(&expected).collect::<Vec<_>>(),
    );
}

#[test]
fn policy_exposes_no_non_config_fields() {
    // No accessor on Policy may correspond to a non-config field. Observed /
    // derived / heuristic / session / runtime are projected by Truth /
    // Inference / (no projection) respectively.
    let accessors = accessor_name_set();
    for (field, category) in AppState::FIELD_EPISTEMIC_MAP {
        if accessors.contains(*field) {
            assert_eq!(
                *category, "config",
                "Policy exposes field `{field}` but its epistemic category \
                 is `{category}`, not `config`.",
            );
        }
    }
}

#[test]
fn config_fields_are_all_reachable_via_policy() {
    let expected = config_field_set();
    let accessors = accessor_name_set();
    let missing: Vec<&&str> = expected.difference(&accessors).collect();
    assert!(
        missing.is_empty(),
        "config fields not reachable through Policy: {missing:?}",
    );
}
