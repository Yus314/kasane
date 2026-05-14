//! Aggregate error type for `kasane-core`.
//!
//! `CoreError` is the unified library-boundary error: per-module typed
//! errors flow into it via `#[from]`, and callers downstream of the library
//! (binaries, integration tests, embedders) can ergonomically `?`-bubble
//! anything originating in `kasane-core` through `error::Result<T>`.
//!
//! Marked `#[non_exhaustive]` so subsequent δ-4 sub-stages can extend the
//! variant set without a breaking-change bump as more per-module errors
//! gain `thiserror::Error` derives.
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

use crate::config::kdl_parser::ConfigError;
use crate::config::unified::UnifiedParseError;
use crate::history::HistoryError;
use crate::render::theme::ThemeError;
use crate::session::SessionManagerError;
use crate::state::selection_set::{LoadError, SaveError};
use crate::surface::SurfaceRegistrationError;
use crate::surface::resolve::OwnerValidationError;
use crate::widget::condition::CondParseError;
use crate::widget::parse::WidgetParseError;
use crate::widget::template::TemplateParseError;

/// Aggregate error type for `kasane-core` operations.
///
/// Each variant corresponds to a per-module typed error that `?`-bubbles
/// into a uniform library-boundary error via `#[from]`. Callers downstream
/// of `kasane-core` (binaries, embedders, integration tests) typically
/// alias `kasane_core::error::Result<T>` to drop the explicit error
/// parameter.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CoreError {
    /// Unified config (`kasane.kdl`) parse failure (fatal).
    #[error(transparent)]
    Config(#[from] UnifiedParseError),

    /// Field-level config parse error (per-section, recoverable when
    /// emitted as a diagnostic but surfaced here for callers that prefer
    /// strict mode).
    #[error(transparent)]
    ConfigField(#[from] ConfigError),

    /// Theme construction failure (token reference / face spec).
    #[error(transparent)]
    Theme(#[from] ThemeError),

    /// Widget file parsing.
    #[error(transparent)]
    WidgetParse(#[from] WidgetParseError),

    /// Template string parsing (`{cursor_line}:{cursor_col}` etc).
    #[error(transparent)]
    TemplateParse(#[from] TemplateParseError),

    /// Condition expression parsing (`?cond => then` predicates).
    #[error(transparent)]
    CondParse(#[from] CondParseError),

    /// Session manager lifecycle errors (duplicate keys, missing
    /// sessions, no active session).
    #[error(transparent)]
    Session(#[from] SessionManagerError),

    /// History backend errors (evicted / unknown version).
    #[error(transparent)]
    History(#[from] HistoryError),

    /// Selection-set persistence: save side.
    #[error(transparent)]
    SelectionSetSave(#[from] SaveError),

    /// Selection-set persistence: load side.
    #[error(transparent)]
    SelectionSetLoad(#[from] LoadError),

    /// Surface ownership / contributor validation failure.
    #[error(transparent)]
    SurfaceOwnerValidation(#[from] OwnerValidationError),

    /// Surface registration conflict (duplicate id / key / slot).
    #[error(transparent)]
    SurfaceRegistration(#[from] SurfaceRegistrationError),
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
    use compact_str::CompactString;

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

    #[test]
    fn from_session_manager_error() {
        let err = SessionManagerError::NoActiveSession;
        let core: CoreError = err.into();
        assert!(matches!(core, CoreError::Session(_)));
    }

    #[test]
    fn from_history_error() {
        let err = HistoryError::Evicted;
        let core: CoreError = err.into();
        assert!(matches!(core, CoreError::History(_)));
    }

    #[test]
    fn from_save_error() {
        let err = SaveError::InvalidName;
        let core: CoreError = err.into();
        assert!(matches!(core, CoreError::SelectionSetSave(_)));
    }

    #[test]
    fn from_widget_parse_error() {
        let err = WidgetParseError::TooManyWidgets;
        let core: CoreError = err.into();
        assert!(matches!(core, CoreError::WidgetParse(_)));
    }

    #[test]
    fn from_template_parse_error() {
        let err = TemplateParseError::UnclosedBrace;
        let core: CoreError = err.into();
        assert!(matches!(core, CoreError::TemplateParse(_)));
    }

    #[test]
    fn from_cond_parse_error() {
        let err = CondParseError::UnexpectedEnd;
        let core: CoreError = err.into();
        assert!(matches!(core, CoreError::CondParse(_)));
    }

    #[test]
    fn from_config_error() {
        let err = ConfigError {
            section: "ui".into(),
            field: "menu_position".into(),
            message: "unknown value".into(),
        };
        let core: CoreError = err.into();
        assert!(matches!(core, CoreError::ConfigField(_)));
    }

    #[test]
    fn from_surface_registration_error() {
        let err = SurfaceRegistrationError::DuplicateSurfaceKey {
            surface_key: CompactString::from("buffer"),
        };
        let core: CoreError = err.into();
        assert!(matches!(core, CoreError::SurfaceRegistration(_)));
    }

    #[test]
    fn display_passes_through_inner() {
        let inner = ThemeError::UndefinedTokenReference {
            token: "ui.text".into(),
            referenced: "missing".into(),
        };
        let inner_msg = inner.to_string();
        let wrapped: CoreError = inner.into();
        assert_eq!(wrapped.to_string(), inner_msg);
    }
}
