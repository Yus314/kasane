//! `kasane-internal` — façade for internal-only `kasane-core` items (ε-1).
//!
//! See `Cargo.toml` for the rationale. This crate re-exports the items
//! that `kasane-core` historically published as `#[doc(hidden)] pub`
//! plus the Salsa surface (which is `pub` for crate-graph reasons but
//! is not part of `kasane-core`'s plugin-author API).
//!
//! # Stability
//!
//! Items exported from this crate are **not** part of Kasane's public
//! API. They may break across patch releases. Plugin authors must
//! depend on `kasane-core` (and `kasane-plugin-sdk` / SDK-macros) only.
//!
//! `kasane-tui`, `kasane-gui`, `kasane-wasm`, and the in-tree benches
//! that need lower-level primitives (Salsa inputs, render-pipeline
//! recovery hooks, etc.) consume this crate directly.

// ---- Salsa runtime surface ------------------------------------------------
//
// `salsa_db`, `salsa_sync` are pub in kasane-core (Salsa's input macros
// require accessible types). `salsa_queries` and `salsa_views` are
// `#[doc(hidden)] pub` for the same reason. They all flow through here
// so internal consumers don't reach into kasane-core directly.
pub use kasane_core::salsa_db;
pub use kasane_core::salsa_queries;
pub use kasane_core::salsa_sync;
pub use kasane_core::salsa_views;

// ---- Display algebra ------------------------------------------------------
//
// `display::algebra` is the post-resolve composable-primitive layer.
// Plugin authors should target the higher-level `DisplayDirective`
// surface in `kasane-core::display`; backend renderers and the host
// dispatch path go through algebra primitives.
pub use kasane_core::display::algebra;

// ---- Wire / safety types --------------------------------------------------
//
// `WireFace` is the `Atom`'s on-the-wire face record (ADR-031 retained
// it as the JSON-RPC representation; post-resolve `Style` is the
// in-memory canonical). Backends serialising / replaying wire data
// take `WireFace` directly.
pub use kasane_core::protocol::WireFace;

// `RecoveryWitness` and `SafeDisplayDirective` are the type-system
// witnesses guarding plugin display recovery. Plugin authors interact
// with them through derived helpers in the SDK; backend integration
// tests touch them directly.
pub use kasane_core::plugin::algebra::recovery_witness::RecoveryWitness;
pub use kasane_core::plugin::algebra::safe_directive::SafeDisplayDirective;
