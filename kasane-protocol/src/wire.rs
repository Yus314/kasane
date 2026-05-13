//! Kakoune wire-format types — `WireFace`, `Color`, `Attributes`,
//! `NamedColor`, `resolve_face`.
//!
//! These types describe the **on-the-wire** Kakoune `kak -ui json`
//! protocol shape. They are inputs to the parser and outputs of a few
//! legacy bridges (the parser path, `make_secondary_cursor_style`,
//! `detect_cursors`, and the wire-aware test helpers in
//! `kasane-core::test_support::wire`). For application logic — themes,
//! plugins, the GUI / TUI render path — use the resolved types in
//! `kasane_protocol::style` (`Style`, `Brush`, `UnresolvedStyle`,
//! `TerminalStyle`).
//!
//! Prefer `kasane_protocol::wire::WireFace` (or
//! `kasane_internal::WireFace`) for new external consumers; the legacy
//! re-exports at `kasane_protocol::*` exist only for `cargo doc`-hidden
//! compatibility (see ADR-031).

pub use super::color::{Attributes, Color, NamedColor, WireFace, resolve_face};
