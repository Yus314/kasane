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

    for td in &set.directives {
        match &td.directive {
            DisplayDirective::Fold { range, .. } => {
                folds.push((range.clone(), td));
            }
            DisplayDirective::Hide { range } => {
                hides.push(range.clone());
            }
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

    // Emit canonical order: hides, folds
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
            let range = match &td.directive {
                DisplayDirective::Fold { range, .. } => range.clone(),
                DisplayDirective::Hide { range } => range.clone(),
            };
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
                match &td.directive {
                    DisplayDirective::Fold { range, summary } => {
                        range.start.hash(&mut hasher);
                        range.end.hash(&mut hasher);
                        summary.len().hash(&mut hasher);
                    }
                    DisplayDirective::Hide { range } => {
                        range.start.hash(&mut hasher);
                        range.end.hash(&mut hasher);
                    }
                }
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
