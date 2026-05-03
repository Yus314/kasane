//! Plugin-emitted directive types and the canonical category
//! partitioner.
//!
//! This module historically hosted the legacy single-pipeline
//! resolver (`resolve()`, `resolve_incremental()`, helpers, plus
//! `resolve_inline()` for the inline-only path). All of those were
//! deprecated and removed under ADR-037 (Phases 1–4): `Hide`,
//! `Fold`, `EditableVirtualText`, and the other nine directive
//! variants now flow through the unified algebra in
//! `crate::display_algebra` (entry point:
//! `crate::display_algebra::bridge::resolve_via_algebra`).
//!
//! What remains here are the **input types** that plugins still
//! emit (`TaggedDirective`, `DirectiveSet`) and the **category
//! partitioner** that the plugin runtime uses to bucket directives
//! before handing them to the algebra.

use crate::plugin::PluginId;

use super::{DirectiveCategory, DisplayDirective};

// =============================================================================
// TaggedDirective — plugin emission record
// =============================================================================

/// A display directive tagged with its source plugin and priority.
#[derive(Debug, Clone, PartialEq)]
pub struct TaggedDirective {
    pub directive: DisplayDirective,
    pub priority: i16,
    pub plugin_id: PluginId,
}

impl TaggedDirective {
    /// Total-order sort key for deterministic composition.
    ///
    /// The 4-tuple `(priority, plugin_id, variant_ordinal, positional_anchor)`
    /// is unique even when the same plugin emits multiple directives at
    /// the same priority, ensuring composition is commutative.
    ///
    /// Retained for compatibility with consumers that still group by
    /// the legacy sort key (e.g. external plugin-internal ordering);
    /// the algebra path uses `display_algebra::TaggedDisplay::cmp_key`
    /// (a structurally analogous key without the variant_ordinal).
    pub fn sort_key(&self) -> (i16, &PluginId, u8, usize) {
        let (variant, anchor) = match &self.directive {
            DisplayDirective::Hide { range } => (0, range.start),
            DisplayDirective::Fold { range, .. } => (1, range.start),
            DisplayDirective::InsertBefore { line, .. } => (2, *line),
            DisplayDirective::InsertAfter { line, .. } => (3, *line),
            DisplayDirective::InsertInline {
                line, byte_offset, ..
            } => (4, *line + *byte_offset),
            DisplayDirective::HideInline {
                line, byte_range, ..
            } => (5, *line + byte_range.start),
            DisplayDirective::StyleInline {
                line, byte_range, ..
            } => (6, *line + byte_range.start),
            DisplayDirective::InlineBox {
                line, byte_offset, ..
            } => (7, *line + *byte_offset),
            DisplayDirective::StyleLine { line, .. } => (8, *line),
            DisplayDirective::Gutter { line, .. } => (9, *line),
            DisplayDirective::VirtualText { line, .. } => (10, *line),
            DisplayDirective::EditableVirtualText { after, .. } => (11, *after),
        };
        (self.priority, &self.plugin_id, variant, anchor)
    }
}

// =============================================================================
// DirectiveSet — accumulator for tagged directives from one frame
// =============================================================================

/// Accumulator for tagged directives from multiple plugins. The
/// algebra resolver consumes this through
/// `crate::display_algebra::bridge::resolve_via_algebra`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DirectiveSet {
    pub directives: Vec<TaggedDirective>,
}

impl DirectiveSet {
    pub fn push(&mut self, directive: DisplayDirective, priority: i16, plugin_id: PluginId) {
        self.directives.push(TaggedDirective {
            directive,
            priority,
            plugin_id,
        });
    }

    pub fn is_empty(&self) -> bool {
        self.directives.is_empty()
    }
}

// =============================================================================
// Category partitioning
// =============================================================================

/// Directives partitioned by category. Used by the plugin runtime to
/// route different directive families through their respective
/// downstream consumers.
#[derive(Debug, Clone, Default)]
pub struct CategorizedDirectives {
    pub spatial: Vec<TaggedDirective>,
    pub interline: Vec<TaggedDirective>,
    pub inline: Vec<TaggedDirective>,
    pub decoration: Vec<TaggedDirective>,
}

/// Partition a `DirectiveSet` into per-category buckets.
///
/// Used by `plugin::registry` to route each directive family through
/// its appropriate downstream pipeline. Categories are defined on
/// `DisplayDirective::category()` (see `display::DirectiveCategory`).
pub fn partition_by_category(set: &DirectiveSet) -> CategorizedDirectives {
    let mut result = CategorizedDirectives::default();
    for td in &set.directives {
        match td.directive.category() {
            DirectiveCategory::Spatial => result.spatial.push(td.clone()),
            DirectiveCategory::InterLine => result.interline.push(td.clone()),
            DirectiveCategory::Inline => result.inline.push(td.clone()),
            DirectiveCategory::Decoration => result.decoration.push(td.clone()),
        }
    }
    result
}
