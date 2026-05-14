//! Aggregate error type for `kasane-core`.
//!
//! `CoreError` is the unified library-boundary error: per-module typed
//! errors flow into it via `#[from]`, and callers downstream of the library
//! (binaries, integration tests, embedders) can ergonomically `?`-bubble
//! anything originating in `kasane-core` through `error::Result<T>`.
//!
//! Marked `#[non_exhaustive]` so δ-4.2 / δ-4.3 can extend the variant set
//! without a breaking-change bump as more per-module errors gain
//! `thiserror::Error` derives.
//!
//! See roadmap entry δ-4 for the broader unification plan.
//!
//! ## Boundary policy
//!
//! - `kasane-core` and other library crates return typed errors (this
//!   `CoreError`, or per-module `thiserror::Error` enums for leaf modules).
//! - `anyhow` is allowed only at the **binary boundary** (`kasane` crate's
//!   `run()` entry point) and inside `#[source]` slots of `thiserror`
//!   variants that bridge external crates with opaque error surfaces
//!   (e.g. wasmtime).

use thiserror::Error;

use crate::config::unified::UnifiedParseError;
use crate::render::theme::ThemeError;

/// Aggregate error type for `kasane-core` operations.
///
/// Variant set is intentionally minimal at δ-4.1 — only types that already
/// implement `std::error::Error` are wired in. Subsequent δ-4 sub-stages
/// migrate the remaining scattered errors (`SessionManagerError`,
/// `HistoryError`, `WidgetParseError`, `ConfigError`, `SaveError`,
/// `LoadError`, `OwnerValidationError`, `SurfaceRegistrationError`,
/// `TemplateParseError`, `CondParseError`) to `thiserror::Error` and then
/// add them here.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CoreError {
    /// Unified config (`kasane.kdl`) parse failure.
    #[error(transparent)]
    Config(#[from] UnifiedParseError),

    /// Theme construction failure (token reference / face spec).
    #[error(transparent)]
    Theme(#[from] ThemeError),
}

/// Convenience alias: `core::result::Result<T, CoreError>`.
///
/// Imported via `use kasane_core::error::Result;` (intentionally **not**
/// re-exported from the crate root to avoid shadowing
/// `std::result::Result` when callers `use kasane_core::*`).
pub type Result<T> = core::result::Result<T, CoreError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_theme_error_compiles() {
        let theme_err = ThemeError::UndefinedTokenReference {
            token: "foo".into(),
            referenced: "bar".into(),
        };
        let core: CoreError = theme_err.into();
        assert!(matches!(core, CoreError::Theme(_)));
    }

    #[test]
    fn from_unified_parse_error_compiles() {
        let cfg_err = UnifiedParseError::Syntax("oops".into());
        let core: CoreError = cfg_err.into();
        assert!(matches!(core, CoreError::Config(_)));
    }

    #[test]
    fn result_alias_propagates_via_question_mark() {
        fn inner() -> Result<()> {
            let cfg_err = UnifiedParseError::Syntax("propagate me".into());
            Err(cfg_err)?;
            Ok(())
        }
        let err = inner().unwrap_err();
        assert!(matches!(err, CoreError::Config(_)));
    }
}
