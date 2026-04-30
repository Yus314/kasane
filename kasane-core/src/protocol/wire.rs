//! Kakoune wire-format types — `WireFace`, `Color`, `Attributes`,
//! `NamedColor`, `resolve_face`.
//!
//! These types describe the **on-the-wire** Kakoune `kak -ui json`
//! protocol shape. They are inputs to the parser and outputs of a few
//! legacy bridges (the parser path, `make_secondary_cursor_style`,
//! `detect_cursors`, and the wire-aware test helpers in
//! `kasane-core::test_support::wire`). For application logic — themes,
//! plugins, the GUI / TUI render path — use the resolved types in
//! `protocol::style` (`Style`, `Brush`, `UnresolvedStyle`,
//! `TerminalStyle`).
//!
//! ## Why a dedicated module
//!
//! The ADR-031 closure cascade (`571bff58`) made `WireFace`, `Color`,
//! and `Attributes` `#[doc(hidden)]` so they no longer surfaced in
//! `cargo doc`, but they were still re-exported at `protocol::*` and
//! consumed by 22 external sites across kasane-wasm, kasane-gui,
//! kasane-tui, the binary, benches, and macro tests. The roadmap entry
//! "ADR-031 post-closure visibility tightening" (roadmap.md, §2.2
//! Backlog) tracks the multi-PR migration to `pub(in crate::protocol)`.
//!
//! This module is the migration target: external consumers should
//! prefer `kasane_core::protocol::wire::WireFace` over the historical
//! `kasane_core::protocol::WireFace`. The two paths refer to the same
//! item today; once every external consumer has switched, the
//! top-level re-export can be removed and `WireFace` can be made
//! crate-private.

pub use super::color::{Attributes, Color, NamedColor, WireFace, resolve_face};
