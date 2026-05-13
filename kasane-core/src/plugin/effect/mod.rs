//! Side-effecting outputs from plugin methods.
//!
//! `effects` is the unified per-tier `Effects` envelope. `effect_tiers`
//! defines the tier-typed projections (Tier 1 / Tier 2 / Tier 3).
//! `command` enumerates the imperative actions a plugin can request;
//! `kakoune_transparent_command` and `kakoune_transparent_effects` host
//! the ADR-030 Level-5 transparent-effect machinery. `error_attribution`
//! captures which plugin produced an offending effect for diagnostic
//! routing.

pub mod command;
pub mod effect_tiers;
pub mod effects;
pub mod error_attribution;
pub mod kakoune_transparent_command;
pub mod kakoune_transparent_effects;
