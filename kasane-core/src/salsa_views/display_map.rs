//! Salsa tracked function for DisplayMap construction.
//!
//! The `display_map_query` function builds a `DisplayMapRef` from
//! `DisplayDirectivesInput`. When directives haven't changed, Salsa's
//! memoization returns the cached map without rebuilding.

use std::sync::Arc;

use crate::display::{DisplayMap, DisplayMapRef};
use crate::salsa_db::KasaneDb;
use crate::salsa_inputs::DisplayDirectivesInput;

/// Build a `DisplayMapRef` from display directives.
///
/// Returns an identity map when no directives are present (common case).
/// Uses `no_eq` because `DisplayMapRef` (Arc<DisplayMap>) doesn't implement
/// Salsa's `Update` trait, and we rely on input-level memoization.
#[salsa::tracked(no_eq)]
pub fn display_map_query(db: &dyn KasaneDb, input: DisplayDirectivesInput) -> DisplayMapRef {
    let directives = input.directives(db);
    let line_count = input.buffer_line_count(db);
    if directives.is_empty() {
        Arc::new(DisplayMap::identity(line_count))
    } else {
        Arc::new(DisplayMap::build(line_count, directives))
    }
}
