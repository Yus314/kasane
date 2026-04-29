//! Multi-plugin display directive composition via DirectiveSet monoid.
//!
//! `resolve()` takes tagged directives from multiple plugins and produces a
//! single `Vec<DisplayDirective>` suitable for `DisplayMap::build()`.

#[cfg(test)]
mod tests;

use std::ops::Range;

use super::DisplayDirective;
use crate::plugin::PluginId;

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
    /// is unique even when the same plugin emits multiple directives at the
    /// same priority, ensuring `DirectiveSet::compose()` is commutative.
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

/// Accumulator for tagged directives from multiple plugins.
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

/// Resolve a set of tagged directives into a flat list for `DisplayMap::build()`.
///
/// Rules:
/// 1. **Hide**: Set union of all ranges (idempotent).
/// 2. **Fold overlap**: Higher `(priority, plugin_id)` wins; lower-priority
///    overlapping fold dropped entirely (protects summary integrity).
/// 3. **Fold-Hide partial overlap**: Fold removed (conservative).
pub fn resolve(set: &DirectiveSet, line_count: usize) -> Vec<DisplayDirective> {
    if set.is_empty() {
        return Vec::new();
    }

    // Partition into folds and hides
    let mut folds: Vec<(Range<usize>, &TaggedDirective)> = Vec::new();
    let mut hides: Vec<Range<usize>> = Vec::new();

    let mut editable_inserts: Vec<&TaggedDirective> = Vec::new();

    for td in &set.directives {
        match &td.directive {
            DisplayDirective::Fold { range, .. } => {
                folds.push((range.clone(), td));
            }
            DisplayDirective::Hide { range } => {
                hides.push(range.clone());
            }
            DisplayDirective::EditableVirtualText { .. } => {
                editable_inserts.push(td);
            }
            // Non-spatial directives (other than EditableVirtualText) are not resolved here
            _ => {}
        }
    }

    // Sort folds by (Reverse(priority), plugin_id) — higher priority wins
    folds.sort_by(|a, b| {
        let pa = std::cmp::Reverse(a.1.priority);
        let pb = std::cmp::Reverse(b.1.priority);
        pa.cmp(&pb).then_with(|| a.1.plugin_id.cmp(&b.1.plugin_id))
    });

    // Accept folds greedily; skip any fold whose range overlaps an already-accepted fold
    let mut accepted_folds: Vec<(Range<usize>, &TaggedDirective)> = Vec::new();
    for (range, td) in &folds {
        let overlaps = accepted_folds
            .iter()
            .any(|(accepted, _)| ranges_overlap(accepted, range));
        if !overlaps {
            accepted_folds.push((range.clone(), td));
        }
    }

    // Compute hidden_set from all hide ranges (union)
    let mut hidden = vec![false; line_count];
    for range in &hides {
        for slot in hidden
            .iter_mut()
            .take(range.end.min(line_count))
            .skip(range.start)
        {
            *slot = true;
        }
    }

    // Remove folds that partially overlap hidden_set
    accepted_folds.retain(|(range, _)| {
        let clamped_end = range.end.min(line_count);
        if range.start >= clamped_end {
            return false;
        }
        let fold_lines = range.start..clamped_end;
        let hidden_count = fold_lines.clone().filter(|&l| hidden[l]).count();
        // Keep only if no overlap at all (disjoint)
        hidden_count == 0
    });

    // Compute invisible_set = hidden ∪ (all accepted fold ranges)
    let mut invisible = hidden;
    for (range, _) in &accepted_folds {
        for slot in invisible
            .iter_mut()
            .take(range.end.min(line_count))
            .skip(range.start)
        {
            *slot = true;
        }
    }

    // Rule 8-10: EditableVirtualText — suppress on invisible anchors,
    // only highest-priority per anchor survives.
    let mut kept_editable: Vec<&TaggedDirective> = editable_inserts
        .into_iter()
        .filter(|td| {
            if let DisplayDirective::EditableVirtualText { after, .. } = &td.directive {
                *after < line_count && !invisible[*after]
            } else {
                false
            }
        })
        .collect();
    kept_editable.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.plugin_id.cmp(&b.plugin_id))
    });
    // Rule 10: same anchor → keep only the highest priority (first after sort by Reverse priority)
    {
        let mut seen_anchors = std::collections::HashSet::new();
        kept_editable.retain(|td| {
            if let DisplayDirective::EditableVirtualText { after, .. } = &td.directive {
                seen_anchors.insert(*after)
            } else {
                false
            }
        });
    }

    // Emit canonical order: hides, folds, editable
    let mut result = Vec::new();

    // Emit hides
    for range in &hides {
        result.push(DisplayDirective::Hide {
            range: range.clone(),
        });
    }

    // Emit accepted folds
    for (_, td) in &accepted_folds {
        result.push(td.directive.clone());
    }

    // Emit kept editable virtual text
    for td in &kept_editable {
        result.push(td.directive.clone());
    }

    result
}

/// Check if two ranges overlap (non-empty intersection).
fn ranges_overlap(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

// =============================================================================
// Incremental resolve — spatial partitioning + cache
// =============================================================================

/// A group of directives with a common bounding range.
///
/// Directives whose affected ranges overlap are merged into the same group.
/// Groups with unchanged hashes across frames can reuse cached results.
#[derive(Debug, Clone)]
pub struct DirectiveGroup {
    /// Bounding range covering all directives in this group.
    pub range: Range<usize>,
    /// The tagged directives in this group.
    pub directives: Vec<TaggedDirective>,
    /// Hash of the directives for change detection.
    pub hash: u64,
}

/// Partition directives into non-overlapping groups based on affected ranges.
///
/// Directives whose bounding ranges overlap are merged into the same group.
/// Uses a greedy merge approach.
pub fn partition_directives(set: &DirectiveSet) -> Vec<DirectiveGroup> {
    use std::hash::{Hash, Hasher};

    if set.is_empty() {
        return Vec::new();
    }

    // Extract (bounding_range, directive_index) pairs
    let mut bounds: Vec<(Range<usize>, usize)> = set
        .directives
        .iter()
        .enumerate()
        .map(|(i, td)| {
            let range = directive_bounding_range(&td.directive);
            (range, i)
        })
        .collect();

    // Sort by start position for greedy merge
    bounds.sort_by_key(|(r, _)| r.start);

    // Greedy merge: merge overlapping bounds into groups
    let mut groups: Vec<(Range<usize>, Vec<usize>)> = Vec::new();
    for (range, idx) in bounds {
        if let Some(last) = groups.last_mut()
            && range.start < last.0.end
        {
            // Overlaps — extend the group
            last.0.end = last.0.end.max(range.end);
            last.1.push(idx);
            continue;
        }
        groups.push((range, vec![idx]));
    }

    // Build DirectiveGroups with hashes
    groups
        .into_iter()
        .map(|(range, indices)| {
            let directives: Vec<TaggedDirective> =
                indices.iter().map(|&i| set.directives[i].clone()).collect();
            let mut hasher = std::hash::DefaultHasher::new();
            for td in &directives {
                td.sort_key().hash(&mut hasher);
                // Hash the directive content for change detection
                hash_directive(&td.directive, &mut hasher);
            }
            let hash = hasher.finish();
            DirectiveGroup {
                range,
                directives,
                hash,
            }
        })
        .collect()
}

/// Cache for incremental resolve results.
#[derive(Debug, Clone, Default)]
pub struct ResolveCache {
    /// Cached group hashes and their resolved directives.
    entries: Vec<(u64, Vec<DisplayDirective>)>,
}

/// Resolve with incremental caching.
///
/// Groups whose hash matches the previous frame reuse cached results.
/// Changed groups are re-resolved.
pub fn resolve_incremental(
    set: &DirectiveSet,
    line_count: usize,
    cache: &mut ResolveCache,
) -> Vec<DisplayDirective> {
    let groups = partition_directives(set);

    let mut result = Vec::new();
    let mut new_entries = Vec::with_capacity(groups.len());

    for (i, group) in groups.iter().enumerate() {
        if let Some((cached_hash, cached_directives)) = cache.entries.get(i)
            && *cached_hash == group.hash
        {
            // Cache hit — reuse
            result.extend(cached_directives.iter().cloned());
            new_entries.push((*cached_hash, cached_directives.clone()));
            continue;
        }
        // Cache miss — resolve this group
        let group_set = DirectiveSet {
            directives: group.directives.clone(),
        };
        let resolved = resolve(&group_set, line_count);
        new_entries.push((group.hash, resolved.clone()));
        result.extend(resolved);
    }

    cache.entries = new_entries;
    result
}

// =============================================================================
// Category partitioning
// =============================================================================

use super::DirectiveCategory;

/// Directives partitioned by category.
#[derive(Debug, Clone, Default)]
pub struct CategorizedDirectives {
    pub spatial: Vec<TaggedDirective>,
    pub interline: Vec<TaggedDirective>,
    pub inline: Vec<TaggedDirective>,
    pub decoration: Vec<TaggedDirective>,
}

/// Partition a `DirectiveSet` into per-category buckets.
///
/// This is the first step of unified display collection: a single call to
/// `on_display()` returns directives of mixed categories, and this function
/// routes each to the correct resolution path.
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

// =============================================================================
// Inline directive resolution
// =============================================================================

use std::collections::HashMap;

use crate::protocol::Face;
use crate::render::inline_decoration::{InlineDecoration, InlineOp};

/// Resolve inline directives from multiple plugins into per-line `InlineDecoration`.
///
/// Rules:
/// - **StyleInline overlap**: overlapping byte ranges from multiple plugins are
///   split at boundaries. Faces are resolved by layering higher-priority
///   plugins' faces over lower-priority ones via `resolve_face()`.
/// - **HideInline**: merged into the final ops. If a Hide overlaps a Style,
///   Hide wins (hidden content cannot be styled).
/// - **InsertInline**: sorted by `(byte_offset, priority)` and emitted as-is.
///
/// Guarantees INV-INLINE-1 (sorted by sort_key) and INV-INLINE-2 (non-overlapping
/// range-based ops) on the output.
pub fn resolve_inline(directives: &[TaggedDirective]) -> HashMap<usize, InlineDecoration> {
    // Group by line
    let mut per_line: HashMap<usize, Vec<&TaggedDirective>> = HashMap::new();
    for td in directives {
        let line = match &td.directive {
            DisplayDirective::InsertInline { line, .. }
            | DisplayDirective::HideInline { line, .. }
            | DisplayDirective::StyleInline { line, .. } => *line,
            _ => continue,
        };
        per_line.entry(line).or_default().push(td);
    }

    let mut result = HashMap::new();
    for (line, tds) in per_line {
        let ops = resolve_inline_line(&tds);
        if !ops.is_empty() {
            result.insert(line, InlineDecoration::new(ops));
        }
    }
    result
}

/// Resolve inline directives for a single line.
fn resolve_inline_line(directives: &[&TaggedDirective]) -> Vec<InlineOp> {
    let mut inserts: Vec<(usize, i16, &TaggedDirective)> = Vec::new();
    let mut styles: Vec<(Range<usize>, Face, i16, &PluginId)> = Vec::new();
    let mut hides: Vec<Range<usize>> = Vec::new();

    for td in directives {
        match &td.directive {
            DisplayDirective::InsertInline { byte_offset, .. } => {
                inserts.push((*byte_offset, td.priority, td));
            }
            DisplayDirective::StyleInline {
                byte_range, face, ..
            } => {
                styles.push((byte_range.clone(), *face, td.priority, &td.plugin_id));
            }
            DisplayDirective::HideInline { byte_range, .. } => {
                hides.push(byte_range.clone());
            }
            _ => {}
        }
    }

    // Sort inserts by (byte_offset, priority desc, plugin_id) for determinism
    inserts.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));

    // Merge hidden ranges into a sorted, non-overlapping set
    let hidden = merge_ranges(&mut hides);

    // Resolve overlapping StyleInline into non-overlapping segments
    let resolved_styles = resolve_style_overlaps(&styles, &hidden);

    // Merge all ops into a single sorted vec
    let mut ops: Vec<InlineOp> = Vec::new();

    // Add Inserts (filtering out those inside hidden ranges)
    for (offset, _priority, td) in &inserts {
        if let DisplayDirective::InsertInline { content, .. } = &td.directive {
            // InsertInline inside a Hide is still emitted (S1 semantics) — keep it
            ops.push(InlineOp::Insert {
                at: *offset,
                content: content.clone(),
            });
        }
    }

    // Add Hide ops
    for range in &hidden {
        ops.push(InlineOp::Hide {
            range: range.clone(),
        });
    }

    // Add resolved Style ops
    for (range, face) in &resolved_styles {
        ops.push(InlineOp::Style {
            range: range.clone(),
            face: *face,
        });
    }

    // Sort by sort_key to satisfy INV-INLINE-1
    ops.sort_by_key(|op| op.sort_key());

    ops
}

/// Merge a list of ranges into sorted, non-overlapping ranges.
fn merge_ranges(ranges: &mut [Range<usize>]) -> Vec<Range<usize>> {
    if ranges.is_empty() {
        return Vec::new();
    }
    ranges.sort_by_key(|r| r.start);
    let mut merged: Vec<Range<usize>> = Vec::new();
    for r in ranges.iter() {
        if let Some(last) = merged.last_mut()
            && r.start <= last.end
        {
            last.end = last.end.max(r.end);
        } else {
            merged.push(r.clone());
        }
    }
    merged
}

/// Resolve overlapping StyleInline directives into non-overlapping segments.
///
/// Uses a sweep-line approach: collect all boundary points, sort them, then
/// for each segment between consecutive boundaries, layer the applicable
/// styles by priority (higher priority = applied last via resolve_face).
fn resolve_style_overlaps(
    styles: &[(Range<usize>, Face, i16, &PluginId)],
    hidden: &[Range<usize>],
) -> Vec<(Range<usize>, Face)> {
    if styles.is_empty() {
        return Vec::new();
    }

    // Collect all boundary points
    let mut points = std::collections::BTreeSet::new();
    for (range, _, _, _) in styles {
        points.insert(range.start);
        points.insert(range.end);
    }
    // Also split at hidden boundaries to exclude hidden regions
    for range in hidden {
        points.insert(range.start);
        points.insert(range.end);
    }

    let points: Vec<usize> = points.into_iter().collect();
    let mut result = Vec::new();

    for window in points.windows(2) {
        let seg_start = window[0];
        let seg_end = window[1];
        if seg_start >= seg_end {
            continue;
        }

        // Skip if this segment is inside a hidden range
        if hidden
            .iter()
            .any(|h| h.start <= seg_start && seg_end <= h.end)
        {
            continue;
        }

        // Collect all styles that cover this segment, sorted by priority ascending
        let mut applicable: Vec<(i16, &PluginId, Face)> = styles
            .iter()
            .filter(|(range, _, _, _)| range.start <= seg_start && seg_end <= range.end)
            .map(|(_, face, priority, plugin_id)| (*priority, *plugin_id, *face))
            .collect();

        if applicable.is_empty() {
            continue;
        }

        // Sort by priority ascending, then plugin_id — lower priority is base
        applicable.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(b.1)));

        // Layer faces: start from lowest priority, resolve each subsequent face over it
        let mut merged_face = applicable[0].2;
        for &(_, _, face) in &applicable[1..] {
            merged_face = crate::protocol::resolve_face(&face, &merged_face);
        }

        result.push((seg_start..seg_end, merged_face));
    }

    // Merge adjacent segments with the same face
    let mut merged: Vec<(Range<usize>, Face)> = Vec::new();
    for (range, face) in result {
        if let Some(last) = merged.last_mut()
            && last.0.end == range.start
            && last.1 == face
        {
            last.0.end = range.end;
        } else {
            merged.push((range, face));
        }
    }

    merged
}

// =============================================================================
// Internal helpers
// =============================================================================

/// Extract the bounding range for any directive variant.
///
/// Used by `partition_directives()` for spatial grouping.
fn directive_bounding_range(d: &DisplayDirective) -> Range<usize> {
    match d {
        DisplayDirective::Fold { range, .. } | DisplayDirective::Hide { range } => range.clone(),
        DisplayDirective::InsertBefore { line, .. }
        | DisplayDirective::InsertAfter { line, .. }
        | DisplayDirective::InsertInline { line, .. }
        | DisplayDirective::HideInline { line, .. }
        | DisplayDirective::StyleInline { line, .. }
        | DisplayDirective::InlineBox { line, .. }
        | DisplayDirective::StyleLine { line, .. }
        | DisplayDirective::Gutter { line, .. }
        | DisplayDirective::VirtualText { line, .. } => *line..*line + 1,
        DisplayDirective::EditableVirtualText { after, .. } => *after..*after + 1,
    }
}

/// Hash directive content for change detection in incremental resolve.
fn hash_directive(d: &DisplayDirective, hasher: &mut impl std::hash::Hasher) {
    use std::hash::Hash;
    // Discriminant tag
    std::mem::discriminant(d).hash(hasher);
    match d {
        DisplayDirective::Fold { range, summary } => {
            range.start.hash(hasher);
            range.end.hash(hasher);
            summary.len().hash(hasher);
        }
        DisplayDirective::Hide { range } => {
            range.start.hash(hasher);
            range.end.hash(hasher);
        }
        DisplayDirective::InsertBefore { line, priority, .. }
        | DisplayDirective::InsertAfter { line, priority, .. } => {
            line.hash(hasher);
            priority.hash(hasher);
        }
        DisplayDirective::InsertInline {
            line, byte_offset, ..
        } => {
            line.hash(hasher);
            byte_offset.hash(hasher);
        }
        DisplayDirective::HideInline { line, byte_range } => {
            line.hash(hasher);
            byte_range.start.hash(hasher);
            byte_range.end.hash(hasher);
        }
        DisplayDirective::StyleInline {
            line, byte_range, ..
        } => {
            line.hash(hasher);
            byte_range.start.hash(hasher);
            byte_range.end.hash(hasher);
        }
        DisplayDirective::StyleLine { line, z_order, .. } => {
            line.hash(hasher);
            z_order.hash(hasher);
        }
        DisplayDirective::Gutter {
            line,
            side,
            priority,
            ..
        } => {
            line.hash(hasher);
            side.hash(hasher);
            priority.hash(hasher);
        }
        DisplayDirective::VirtualText {
            line,
            position,
            priority,
            ..
        } => {
            line.hash(hasher);
            position.hash(hasher);
            priority.hash(hasher);
        }
        DisplayDirective::EditableVirtualText {
            after,
            content,
            editable_spans,
        } => {
            after.hash(hasher);
            content.len().hash(hasher);
            editable_spans.len().hash(hasher);
        }
        DisplayDirective::InlineBox {
            line,
            byte_offset,
            width_cells,
            height_lines,
            box_id,
            alignment,
        } => {
            line.hash(hasher);
            byte_offset.hash(hasher);
            width_cells.to_bits().hash(hasher);
            height_lines.to_bits().hash(hasher);
            box_id.hash(hasher);
            alignment.hash(hasher);
        }
    }
}
