//! Salsa tracked functions for derived state computation.
//!
//! These are the Layer 3 declarative queries that Salsa automatically
//! memoizes and revalidates based on input changes.

use crate::protocol::CursorMode;
use crate::render::CursorStyle;
use crate::salsa_db::KasaneDb;
use crate::salsa_inputs::{ConfigInput, CursorInput, StatusInput};

/// Available height (rows - status bar).
#[salsa::tracked]
pub fn available_height(db: &dyn KasaneDb, config: ConfigInput) -> u16 {
    config.rows(db).saturating_sub(1)
}

/// Whether we're in prompt mode.
#[salsa::tracked]
pub fn is_prompt_mode(db: &dyn KasaneDb, cursor: CursorInput) -> bool {
    cursor.cursor_mode(db) == CursorMode::Prompt
}

/// Cursor style derived from config + cursor mode + status mode line.
///
/// This is the default cursor style without plugin overrides.
/// Plugin overrides are applied in Stage 2 (outside Salsa).
#[salsa::tracked]
pub fn cursor_style_query(
    db: &dyn KasaneDb,
    config: ConfigInput,
    cursor: CursorInput,
    status: StatusInput,
) -> CursorStyle {
    crate::state::derived::derive_cursor_style(
        // We don't have ui_options in Salsa inputs yet — pass empty map.
        // The ui_option override is rare and handled by the full cursor_style()
        // function in the rendering pipeline (Stage 2).
        &std::collections::HashMap::new(),
        config.focused(db),
        cursor.cursor_mode(db),
        status.status_mode_line(db),
    )
}
