//! Canonical Kasane plugin WIT (`kasane:plugin`).
//!
//! This crate owns the single copy of `plugin.wit`. Three consumers read it:
//!
//! - `kasane-wasm` — host side, via `wasmtime::component::bindgen!` with
//!   `path = "../kasane-wit/wit"`.
//! - `kasane-plugin-sdk` — guest SDK, re-exports [`WIT`] for documentation
//!   and inline `wit_bindgen::generate!` use.
//! - `kasane-plugin-sdk-macros` — proc macros, emit `wit_bindgen::generate!`
//!   blocks with [`WIT`] inlined as a string literal.
//!
//! Prior to Phase γ-0.4 the canonical WIT lived at `kasane-wasm/wit/plugin.wit`
//! and the two SDK crates kept symlinks pointing at it. Consolidating into a
//! standalone crate lets Cargo manage the cross-crate dependency directly and
//! retires the symlinks + the matching `WIT symlink check` CI step.

/// The full text of the Kasane plugin WIT package.
///
/// The first line is `package kasane:plugin@MAJOR.MINOR.PATCH;` — see
/// `docs/abi-versioning.md` for the SemVer policy that governs bumps.
pub const WIT: &str = include_str!("../wit/plugin.wit");
