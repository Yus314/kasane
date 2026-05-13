//! Pure value types for plugin-declared transformations.
//!
//! Plugin algebra: element patches (`element_patch`), compose laws
//! (`compose`), display-directive safety witnesses (`safe_directive`),
//! and recovery witnesses (`recovery_witness`). These have no
//! side-effecting runtime; consumers in `plugin/effect/` and
//! `plugin/host/` reduce, lift, or witness them.

pub mod compose;
pub mod element_patch;
pub mod recovery_witness;
pub mod safe_directive;
