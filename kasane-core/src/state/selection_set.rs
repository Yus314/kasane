//! ADR-035 §1 first-class `SelectionSet` — the set algebra over
//! Kakoune-style multi-selections.
//!
//! A `SelectionSet` is *anchored to a (BufferId, BufferVersion)*; set
//! operations are defined on the same anchor, and cross-version
//! operations require explicit projection (deferred to a follow-up).
//!
//! Internal invariant: `selections` is sorted by `Selection::min()`
//! and any overlapping selections have been merged. This makes
//! `union`/`intersect`/`difference` linear in the input sizes.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::plugin::PluginId;

use super::selection::{BufferId, BufferPos, BufferVersion, Direction, Selection};

/// A set of disjoint, sorted selections within a single buffer at a
/// single version. The set is empty by construction when no selection
/// exists; the framework always maintains at least the primary
/// selection in the protocol-derived input set, but plugin-derived
/// sets may be empty.
///
/// **Point selections** (`anchor == cursor`) are not first-class set
/// members in this algebra. They represent a *position*, not selected
/// content; mixing them into set operations produces ambiguous results
/// (e.g. "subtract a single point from an interval" has no
/// representation in half-open intervals without open boundaries).
/// `from_iter` does not filter them out today, but a future ADR will
/// either lift them to a separate `PositionSet` type or define explicit
/// semantics. Until then, plugin authors should avoid mixing point and
/// range selections within one set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionSet {
    selections: Vec<Selection>,
    buffer: BufferId,
    generation: BufferVersion,
}

/// Errors returned by named-set persistence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveError {
    /// The supplied name is empty or contains a `:` (reserved for
    /// scoping by `(plugin_id, name)`).
    InvalidName,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    NotFound,
    BufferMismatch {
        saved_in: BufferId,
        requested: BufferId,
    },
}

/// Process-global named-set store. Session-scoped: cleared on editor
/// restart unless `save_persistent` is used (deferred).
fn store() -> &'static Mutex<HashMap<(PluginId, String), SelectionSet>> {
    static STORE: OnceLock<Mutex<HashMap<(PluginId, String), SelectionSet>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

impl SelectionSet {
    // -------------------------------------------------------------------------
    // Construction
    // -------------------------------------------------------------------------

    /// An empty set anchored to the given buffer and version.
    pub fn empty(buffer: BufferId, generation: BufferVersion) -> Self {
        Self {
            selections: Vec::new(),
            buffer,
            generation,
        }
    }

    /// A set containing exactly one selection.
    pub fn singleton(sel: Selection, buffer: BufferId, generation: BufferVersion) -> Self {
        Self {
            selections: vec![sel],
            buffer,
            generation,
        }
    }

    /// Build from an unsorted, possibly-overlapping iterator of
    /// selections, normalising in-place to the canonical
    /// sorted-disjoint form.
    pub fn from_iter(
        sels: impl IntoIterator<Item = Selection>,
        buffer: BufferId,
        generation: BufferVersion,
    ) -> Self {
        let mut sels: Vec<Selection> = sels.into_iter().collect();
        sels.sort_by_key(|s| s.min());
        let merged = Self::merge_overlapping(sels);
        Self {
            selections: merged,
            buffer,
            generation,
        }
    }

    /// Coalesce a sorted-by-min selection vector into disjoint runs.
    fn merge_overlapping(sels: Vec<Selection>) -> Vec<Selection> {
        let mut out: Vec<Selection> = Vec::with_capacity(sels.len());
        for s in sels {
            match out.last() {
                Some(prev) if prev.overlaps(&s) || prev.max() == s.min() => {
                    let merged = out.pop().unwrap().merge_with(&s);
                    out.push(merged);
                }
                _ => out.push(s),
            }
        }
        out
    }

    // -------------------------------------------------------------------------
    // Introspection
    // -------------------------------------------------------------------------

    pub fn buffer(&self) -> &BufferId {
        &self.buffer
    }

    pub fn generation(&self) -> BufferVersion {
        self.generation
    }

    pub fn len(&self) -> usize {
        self.selections.len()
    }

    pub fn is_empty(&self) -> bool {
        self.selections.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Selection> {
        self.selections.iter()
    }

    /// The primary selection — by Kakoune convention this is the first
    /// element after sorting; plugins that need a different policy
    /// reorder via `map`.
    pub fn primary(&self) -> Option<&Selection> {
        self.selections.first()
    }

    pub fn covers(&self, pos: BufferPos) -> bool {
        self.selections.iter().any(|s| s.covers(pos))
    }

    /// Whether two sets share no positions. Both must reference the
    /// same buffer and generation; otherwise `false`.
    pub fn is_disjoint(&self, other: &SelectionSet) -> bool {
        if self.buffer != other.buffer || self.generation != other.generation {
            return false;
        }
        for a in &self.selections {
            for b in &other.selections {
                if a.overlaps(b) {
                    return false;
                }
            }
        }
        true
    }

    fn require_same_anchor(&self, other: &SelectionSet) {
        debug_assert_eq!(
            self.buffer, other.buffer,
            "set algebra requires both sides anchored to the same buffer",
        );
        debug_assert_eq!(
            self.generation, other.generation,
            "set algebra requires both sides anchored to the same generation",
        );
    }

    // -------------------------------------------------------------------------
    // Set algebra
    // -------------------------------------------------------------------------

    pub fn union(&self, other: &SelectionSet) -> SelectionSet {
        self.require_same_anchor(other);
        let mut merged: Vec<Selection> =
            Vec::with_capacity(self.selections.len() + other.selections.len());
        merged.extend(self.selections.iter().copied());
        merged.extend(other.selections.iter().copied());
        merged.sort_by_key(|s| s.min());
        Self {
            selections: Self::merge_overlapping(merged),
            buffer: self.buffer.clone(),
            generation: self.generation,
        }
    }

    pub fn intersect(&self, other: &SelectionSet) -> SelectionSet {
        self.require_same_anchor(other);
        let mut out: Vec<Selection> = Vec::new();
        // Walk both sorted-disjoint vectors with two indices.
        let mut i = 0;
        let mut j = 0;
        while i < self.selections.len() && j < other.selections.len() {
            let a = self.selections[i];
            let b = other.selections[j];
            if !a.overlaps(&b) {
                if a.max() <= b.min() {
                    i += 1;
                } else {
                    j += 1;
                }
                continue;
            }
            let lo = std::cmp::max(a.min(), b.min());
            let hi = std::cmp::min(a.max(), b.max());
            if lo < hi {
                out.push(Selection {
                    anchor: lo,
                    cursor: hi,
                    direction: Direction::Forward,
                });
            } else if a.anchor == a.cursor && b.covers(a.anchor) {
                out.push(a);
            } else if b.anchor == b.cursor && a.covers(b.anchor) {
                out.push(b);
            }
            // Advance whichever ends first.
            if a.max() <= b.max() {
                i += 1;
            } else {
                j += 1;
            }
        }
        Self {
            selections: out,
            buffer: self.buffer.clone(),
            generation: self.generation,
        }
    }

    pub fn difference(&self, other: &SelectionSet) -> SelectionSet {
        self.require_same_anchor(other);
        let mut result: Vec<Selection> = Vec::new();
        for &a in &self.selections {
            let mut current = vec![a];
            for &b in &other.selections {
                let mut next = Vec::with_capacity(current.len());
                for s in current {
                    next.extend(subtract(s, b));
                }
                current = next;
                if current.is_empty() {
                    break;
                }
            }
            result.extend(current);
        }
        Self {
            selections: result,
            buffer: self.buffer.clone(),
            generation: self.generation,
        }
    }

    pub fn symmetric_difference(&self, other: &SelectionSet) -> SelectionSet {
        self.union(other).difference(&self.intersect(other))
    }

    // -------------------------------------------------------------------------
    // Pointwise transformation
    // -------------------------------------------------------------------------

    pub fn map<F>(&self, mut f: F) -> SelectionSet
    where
        F: FnMut(Selection) -> Selection,
    {
        let mapped: Vec<Selection> = self.selections.iter().copied().map(&mut f).collect();
        Self::from_iter(mapped, self.buffer.clone(), self.generation)
    }

    pub fn filter<F>(&self, mut f: F) -> SelectionSet
    where
        F: FnMut(&Selection) -> bool,
    {
        Self {
            selections: self.selections.iter().copied().filter(|s| f(s)).collect(),
            buffer: self.buffer.clone(),
            generation: self.generation,
        }
    }

    pub fn flat_map<F>(&self, mut f: F) -> SelectionSet
    where
        F: FnMut(Selection) -> Vec<Selection>,
    {
        let collected: Vec<Selection> = self
            .selections
            .iter()
            .copied()
            .flat_map(|s| f(s).into_iter())
            .collect();
        Self::from_iter(collected, self.buffer.clone(), self.generation)
    }

    // -------------------------------------------------------------------------
    // Persistence (named registers, session-scoped)
    // -------------------------------------------------------------------------

    /// Save this set under `(plugin, name)`. Replaces any existing
    /// entry under the same key.
    pub fn save(&self, plugin: PluginId, name: &str) -> Result<(), SaveError> {
        if name.is_empty() || name.contains(':') {
            return Err(SaveError::InvalidName);
        }
        let mut s = store().lock().expect("named-set store poisoned");
        s.insert((plugin, name.to_string()), self.clone());
        Ok(())
    }

    /// Load a previously-saved set, requiring the buffer to match (the
    /// generation may differ — saved sets are usable across edits to
    /// the same buffer because their positions are addresses, not
    /// content; a future PR can add an explicit `project_to` that
    /// rewrites positions through edit history).
    pub fn load(plugin: PluginId, name: &str, buffer: BufferId) -> Result<SelectionSet, LoadError> {
        let s = store().lock().expect("named-set store poisoned");
        let key = (plugin, name.to_string());
        let saved = s.get(&key).ok_or(LoadError::NotFound)?;
        if saved.buffer != buffer {
            return Err(LoadError::BufferMismatch {
                saved_in: saved.buffer.clone(),
                requested: buffer,
            });
        }
        Ok(saved.clone())
    }
}

/// Subtract `b` from `a`, returning 0–2 leftover selections.
fn subtract(a: Selection, b: Selection) -> Vec<Selection> {
    if !a.overlaps(&b) {
        return vec![a];
    }
    let mut out = Vec::new();
    if a.min() < b.min() {
        out.push(Selection {
            anchor: a.min(),
            cursor: b.min(),
            direction: a.direction,
        });
    }
    if b.max() < a.max() {
        out.push(Selection {
            anchor: b.max(),
            cursor: a.max(),
            direction: a.direction,
        });
    }
    out
}
