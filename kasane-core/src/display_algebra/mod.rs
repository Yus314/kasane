//! Display algebra — composable primitives for plugin-declared display
//! transformations (ADR-034).
//!
//! This module is the typed replacement for the legacy `DisplayDirective`
//! enum and its variant-aware resolver. The algebra has five primitives
//! (`Identity`, `Replace`, `Decorate`, `Anchor`) and two composition
//! operators (`Then`, `Merge`), unified as constructors of a single
//! `Display` enum. The 12 directives of the previous design are recovered
//! as named smart constructors over this algebra; see `derived.rs`.
//!
//! Algebraic properties (witnessed in `tests.rs`):
//!
//! - **L1** Identity: `then(I, d) ≡ d` and `merge(I, d) ≡ d`.
//! - **L2** Then-associativity: `then(then(a, b), c) ≡ then(a, then(b, c))`.
//! - **L3** Merge-associativity: `merge(merge(a, b), c) ≡ merge(a, merge(b, c))`.
//! - **L4** Merge-commutativity (disjoint): `support(a) ∩ support(b) = ∅
//!   ⟹ merge(a, b) ≡ merge(b, a)`.
//! - **L5** Decorate-commutativity: `Decorate ∘ Decorate` always commutes;
//!   conflicts on overlap resolve by tagged-priority style stacking.
//! - **L6** Replace-conflict-determinism: overlapping `Replace` merges
//!   produce a deterministic `MergeConflict { winner, displaced }`.
//!
//! Coexistence: while ADR-034 calls for the eventual deletion of the
//! legacy `display::DisplayDirective`, this module ships first in
//! parallel; the legacy module is untouched until the migration ADR
//! moves to `Accepted`.

pub mod apply;
pub mod bridge;
pub mod derived;
pub mod normalize;
pub mod primitives;
#[cfg(test)]
mod proptests;
#[cfg(test)]
mod tests;

pub use apply::{
    Anchor, BufferLine, Decoration, LineRender, Replacement, apply, replacement_atoms,
};
pub use derived::*;
pub use normalize::{
    MergeConflict, NormalizedDisplay, TaggedDisplay, normalize, pass_c_filter_evt,
};
pub use primitives::{AnchorPosition, Content, Display, EditSpec, Side, Span};
