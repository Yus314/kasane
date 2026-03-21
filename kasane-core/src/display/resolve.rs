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
/// 1. **InsertAfter**: All kept. Same-line ordering by `(priority, plugin_id)`.
/// 2. **Hide**: Set union of all ranges (idempotent).
/// 3. **Fold overlap**: Higher `(priority, plugin_id)` wins; lower-priority
///    overlapping fold dropped entirely (protects summary integrity).
/// 4. **Fold-Hide partial overlap**: Fold removed (conservative).
/// 5. **InsertAfter suppression**: Inserts targeting hidden or folded lines removed.
pub fn resolve(set: &DirectiveSet, line_count: usize) -> Vec<DisplayDirective> {
    if set.is_empty() {
        return Vec::new();
    }

    // Partition into folds, hides, inserts
    let mut folds: Vec<(Range<usize>, &TaggedDirective)> = Vec::new();
    let mut hides: Vec<Range<usize>> = Vec::new();
    let mut inserts: Vec<&TaggedDirective> = Vec::new();

    for td in &set.directives {
        match &td.directive {
            DisplayDirective::Fold { range, .. } => {
                folds.push((range.clone(), td));
            }
            DisplayDirective::Hide { range } => {
                hides.push(range.clone());
            }
            DisplayDirective::InsertAfter { .. } => {
                inserts.push(td);
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

    // Remove inserts whose target line is invisible
    let mut kept_inserts: Vec<&TaggedDirective> = inserts
        .into_iter()
        .filter(|td| {
            if let DisplayDirective::InsertAfter { after, .. } = &td.directive {
                *after < line_count && !invisible[*after]
            } else {
                false
            }
        })
        .collect();

    // Sort inserts for same-line ordering by (priority, plugin_id)
    kept_inserts.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.plugin_id.cmp(&b.plugin_id))
    });

    // Emit canonical order: hides first, then folds, then inserts
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

    // Emit kept inserts
    for td in &kept_inserts {
        result.push(td.directive.clone());
    }

    result
}

/// Check if two ranges overlap (non-empty intersection).
fn ranges_overlap(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}
