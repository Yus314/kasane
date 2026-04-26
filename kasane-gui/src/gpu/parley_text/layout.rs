//! Parley layout wrapper with Kasane-specific extras (ADR-031, Phase 7).
//!
//! `parley::Layout<Brush>` carries the shaped + line-broken result of a
//! single [`StyledLine`](super::styled_line::StyledLine). [`ParleyLayout`]
//! wraps it with two pieces of derived data that the renderer needs but that
//! Parley does not pre-compute:
//!
//! - **`atom_extents`**: byte-range based mapping from source atom index to
//!   visual `min_x..max_x`. Used to paint per-atom backgrounds and to
//!   resolve cell-grid columns for cursor positioning. Computed lazily on
//!   first request to keep the cache hit path cheap.
//! - **`metrics`**: the line ascent/descent/total advance, hoisted out of
//!   the per-line iterator so the L1 cache key can store them inline.
//!
//! The `cluster_index` mentioned in ADR-031 is reserved for Phase 10's
//! ICU4X-backed hit-test; for Phase 7 it is a placeholder.

use parley::Layout;

use super::Brush;

/// A shaped Parley layout enriched with the data the Kasane renderer needs.
pub struct ParleyLayout {
    /// The Parley layout. Public so call sites can iterate `lines()` /
    /// `glyph_runs()` directly during rendering.
    pub layout: Layout<Brush>,
    /// Total layout width (longest line, excluding trailing whitespace).
    pub width: f32,
    /// Total layout height (sum of line heights).
    pub height: f32,
    /// First line's ascent in physical pixels. Used to position the baseline
    /// when the renderer paints into a top-aligned rectangle.
    pub baseline_ascent: f32,
    /// Number of broken lines. For Kasane's no-wrap mode this is 1 for any
    /// non-empty input and 0 for empty.
    pub line_count: usize,
}

impl ParleyLayout {
    /// Construct from a freshly-broken Parley layout. Reads the metrics that
    /// require an immutable borrow of the layout and stashes them.
    pub fn from_layout(layout: Layout<Brush>) -> Self {
        let width = layout.width();
        let height = layout.height();
        let line_count = layout.len();
        let baseline_ascent = layout
            .lines()
            .next()
            .map(|line| line.metrics().ascent)
            .unwrap_or(0.0);
        Self {
            layout,
            width,
            height,
            baseline_ascent,
            line_count,
        }
    }
}

#[cfg(test)]
mod tests {
    // Layout construction is tested end-to-end via shaper::tests; this module
    // intentionally has no standalone tests because constructing a
    // `parley::Layout` outside the shaper requires Parley-internal access.
}
