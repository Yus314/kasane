//! Built-in lenses bundled with `kasane-core`.
//!
//! These are concrete `Lens` implementations that demonstrate the
//! API by usage and ship as opt-in user-facing capabilities.
//! Register them on `AppState.lens_registry` from your embedder /
//! plugin / native bootstrap to make them available; `enable` to
//! activate.
//!
//! Naming convention: `("kasane.builtin", "<kebab-name>")`. The
//! `kasane.builtin` plugin id is reserved for built-in lenses;
//! third-party plugins register under their own plugin id.

pub mod long_line;
pub mod trailing_whitespace;

pub use long_line::LongLineLens;
pub use trailing_whitespace::TrailingWhitespaceLens;

/// Plugin id namespace for built-in lenses.
pub const BUILTIN_PLUGIN_ID: &str = "kasane.builtin";
