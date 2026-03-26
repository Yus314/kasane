//! Pure functions for derived state computation.
//!
//! These functions extract deterministic computations from `apply.rs` into
//! standalone, testable pure functions. They form the Layer 2 boundary
//! for Salsa tracked function integration.
//!
//! # Inference Catalog
//!
//! Kasane infers semantic information from Kakoune's display-only JSON-RPC
//! protocol. Each inference rule is documented with its assumptions, failure
//! modes, and severity rating.
//!
//! | ID  | Function                     | Assumption                                              | Severity    | Cross-validated | Proptest |
//! |-----|------------------------------|---------------------------------------------------------|-------------|-----------------|----------|
//! | I-1 | `detect_cursors`             | Cursor atoms have `FINAL_FG+REVERSE` or matching fg     | Degraded    | Yes (Phase C)   | Yes      |
//! | I-2 | `derive_cursor_style`        | Mode line contains "insert"/"replace"/other             | Cosmetic    | No              | Yes      |
//! | I-3 | `derive_cursor_mode`         | `content_cursor_pos >= 0` means prompt mode             | Degraded    | No              | Yes      |
//! | I-4 | `split_single_item` (menu)   | Docstring atoms have non-Default fg after padding       | Cosmetic    | No              | No       |
//! | I-6 | `make_secondary_cursor_face` | Cursor face uses `REVERSE` for visual highlight         | Cosmetic    | No              | No       |
//! | I-7 | `detect_selections`          | Selection atoms have non-default bg adjacent to cursor  | Degraded    | No              | No       |
//! | R-1 | `check_cursor_width_consistency` | `atom_display_width` matches Kakoune's width calc    | Catastrophic| Yes (Phase B)   | Yes      |
//! | R-3 | `compute_lines_dirty`        | Line equality implies visual equality                   | Degraded    | No              | Yes      |

mod atom_metrics;
mod cursor;
mod mode;
mod selection;
mod validation;

#[cfg(test)]
mod tests;

pub(crate) use atom_metrics::line_atom_display_width;
pub use cursor::{
    CursorCache, check_primary_cursor_in_set, detect_cursors, detect_cursors_incremental,
};
pub use mode::{derive_cursor_mode, derive_cursor_style, derive_editor_mode};
pub use selection::{Selection, detect_selections};
pub use validation::{
    WidthDivergence, build_status_line, check_cursor_width_consistency, compute_lines_dirty,
};

/// Parsed editor mode derived from cursor mode and status mode line.
///
/// Provides a higher-level abstraction than `CursorMode` (which only distinguishes
/// Buffer vs Prompt). `EditorMode` further classifies Buffer mode into Normal,
/// Insert, and Replace based on the mode line heuristic (I-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum EditorMode {
    #[default]
    Normal,
    Insert,
    Replace,
    Prompt,
    Unknown,
}
