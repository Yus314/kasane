//! Lens output cache (Composable Lenses follow-up — Roadmap §2.2).
//!
//! Caches the output of opt-in lenses so a frame in which the
//! buffer content didn't change can return the previous output
//! without re-invoking the lens.
//!
//! ## Cache strategies
//!
//! Two opt-in granularities, declared per lens via
//! [`crate::lens::Lens::cache_strategy`]:
//!
//! - **`PerBuffer`** — single cache entry per lens, keyed on a
//!   whole-buffer text hash. Any change to any line invalidates
//!   the entry. The buffer hash is computed once per
//!   `collect_directives` call (lazily — only if at least one
//!   `PerBuffer` lens is enabled) and shared across all
//!   `PerBuffer` lenses.
//! - **`PerLine`** — one cache entry per `(LensId, line_idx)`,
//!   keyed on a per-line text hash. A single-line edit
//!   invalidates one entry per lens; other lines' entries
//!   persist. Requires the lens to implement
//!   `display_line(view, line)`; the default impl filters
//!   whole-buffer output by anchor line (correct but defeats
//!   the cache).
//!
//! ## Soundness contracts
//!
//! - `PerBuffer`: lens output depends on **line text only** —
//!   no cursor / selection / syntax / etc. reads.
//! - `PerLine`: lens per-line output depends on **only that one
//!   line's text** — strictly narrower than `PerBuffer`. A lens
//!   that reads adjacent lines or whole-buffer state cannot use
//!   `PerLine`.
//!
//! The bundled lenses (`TrailingWhitespaceLens`,
//! `LongLineLens`, `IndentGuidesLens`) all satisfy the
//! `PerLine` contract — each line's directive depends only on
//! that line's text — and opt in to `PerLine` for the finest
//! granularity. User lenses opt in by overriding
//! `Lens::cache_strategy`. The `None` default keeps user
//! lenses uncached unless they explicitly say otherwise.
//!
//! ## Concurrency
//!
//! The cache lives behind a `Mutex` for the same reason
//! `InMemoryRing` does: kasane-core types stay `Send + Sync` even
//! though the dispatch loop is single-threaded today. Lock
//! contention is not a concern at frame rate.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use crate::display::DisplayDirective;
use crate::protocol::Atom;

use super::LensId;

/// Cache-invalidation strategy a lens declares to the dispatcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CacheStrategy {
    /// No caching. The lens runs every frame. Default — preserves
    /// the MVP behaviour for lenses that haven't opted in.
    #[default]
    None,
    /// Cache invalidates when **any** line's text changes. Sound
    /// only for lenses whose output depends on line text alone
    /// (no cursor / selection / syntax / etc. reads). The
    /// per-frame buffer hash is shared across all `PerBuffer`
    /// lenses, so the dispatcher amortises hash cost.
    PerBuffer,
    /// Cache invalidates per line: a single-line edit drops one
    /// entry per `PerLine` lens, not the whole-buffer entry. Sound
    /// only for lenses whose per-line output depends on **only that
    /// one line's** text (the soundness contract is strictly
    /// narrower than `PerBuffer`'s — a lens that reads adjacent
    /// lines or whole-buffer state must keep `PerBuffer` or
    /// `None`).
    ///
    /// The lens implementor should override
    /// `Lens::display_line(view, line)` to compute one line's
    /// directives directly. The default impl filters
    /// `display(view)` by anchor line — correct but pays the
    /// whole-buffer cost on every line, defeating the cache's
    /// purpose. Override for actual savings.
    PerLine,
}

/// Per-lens cache entry: the hash of the inputs the entry was
/// computed against, plus the cached output.
#[derive(Debug, Clone)]
struct CacheEntry {
    buffer_hash: u64,
    output: Vec<DisplayDirective>,
}

/// Process-local lens output cache. Held inside `LensRegistry`
/// behind an `Arc<Mutex<...>>` so cloned registries share the
/// cache (matching the session-scoped semantics of `AppState.history`).
///
/// Two backing maps:
/// - `per_buffer_entries` — keyed by `LensId`; one entry per
///   `PerBuffer` lens.
/// - `per_line_entries` — keyed by `(LensId, line_idx)`; one
///   entry per (lens, line) for `PerLine` lenses. May grow
///   unboundedly with very long buffers; for typical
///   thousand-line buffers the memory is bounded and lookup is
///   `HashMap`-O(1).
///
/// A lens is in exactly one map at a time (its current
/// `cache_strategy` determines which). Invalidating by `LensId`
/// drops from both maps so a strategy change between calls
/// doesn't leave stale entries.
#[derive(Debug, Default)]
pub(crate) struct LensCache {
    per_buffer_entries: HashMap<LensId, CacheEntry>,
    per_line_entries: HashMap<(LensId, usize), CacheEntry>,
}

/// Shared handle. Cloning the handle shares the underlying cache;
/// dropping all handles drops the cache.
pub(crate) type SharedCache = Arc<Mutex<LensCache>>;

pub(crate) fn empty_cache() -> SharedCache {
    Arc::new(Mutex::new(LensCache::default()))
}

impl LensCache {
    /// Look up the per-buffer cached output for `lens_id` against
    /// the supplied `buffer_hash`. Returns `Some(output_clone)` on
    /// hit, `None` on miss (no entry, or entry hash mismatches).
    pub(crate) fn get(&self, lens_id: &LensId, buffer_hash: u64) -> Option<Vec<DisplayDirective>> {
        self.per_buffer_entries.get(lens_id).and_then(|e| {
            if e.buffer_hash == buffer_hash {
                Some(e.output.clone())
            } else {
                None
            }
        })
    }

    /// Store the lens's per-buffer output keyed on
    /// `(lens_id, buffer_hash)`. Overwrites any existing entry
    /// for `lens_id` (per-buffer slot).
    pub(crate) fn put(&mut self, lens_id: LensId, buffer_hash: u64, output: Vec<DisplayDirective>) {
        self.per_buffer_entries.insert(
            lens_id,
            CacheEntry {
                buffer_hash,
                output,
            },
        );
    }

    /// Per-line lookup. Cache key is `(LensId, line_idx)`; entry
    /// stores `(line_content_hash, output)`. Returns
    /// `Some(output_clone)` on hash match, `None` otherwise.
    pub(crate) fn get_line(
        &self,
        lens_id: &LensId,
        line_idx: usize,
        line_hash: u64,
    ) -> Option<Vec<DisplayDirective>> {
        self.per_line_entries
            .get(&(lens_id.clone(), line_idx))
            .and_then(|e| {
                if e.buffer_hash == line_hash {
                    Some(e.output.clone())
                } else {
                    None
                }
            })
    }

    /// Per-line store. Overwrites any existing entry for
    /// `(lens_id, line_idx)`.
    pub(crate) fn put_line(
        &mut self,
        lens_id: LensId,
        line_idx: usize,
        line_hash: u64,
        output: Vec<DisplayDirective>,
    ) {
        self.per_line_entries.insert(
            (lens_id, line_idx),
            CacheEntry {
                buffer_hash: line_hash,
                output,
            },
        );
    }

    /// Drop ALL cache entries for `lens_id` (per-buffer + every
    /// per-line entry). Used by the registry's `disable` /
    /// `unregister` / re-register paths so a lens that returns to
    /// the dispatcher gets a fresh re-invocation rather than a
    /// stale cached output, regardless of strategy.
    pub(crate) fn invalidate(&mut self, lens_id: &LensId) {
        self.per_buffer_entries.remove(lens_id);
        self.per_line_entries.retain(|(id, _), _| id != lens_id);
    }

    /// Number of cache entries currently held across both maps.
    /// Test-facing introspection.
    pub(crate) fn len(&self) -> usize {
        self.per_buffer_entries.len() + self.per_line_entries.len()
    }
}

/// Compute a stable hash of a single line's text content.
/// Used by the per-line cache; the per-line dispatcher hashes
/// each line as it iterates so a single-line edit invalidates
/// exactly one cache entry per `PerLine` lens.
pub(crate) fn hash_line_text(atoms: &[Atom]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let total_bytes: usize = atoms.iter().map(|a| a.contents.len()).sum();
    total_bytes.hash(&mut hasher);
    for atom in atoms {
        hasher.write(atom.contents.as_bytes());
    }
    hasher.finish()
}

/// Anchor line for a `DisplayDirective`: the smallest buffer
/// line index the directive touches. Used by the default
/// `Lens::display_line` impl to filter whole-buffer output by
/// the line that "owns" each directive.
///
/// Multi-line directives (`Hide` / `Fold`) anchor at
/// `range.start`. Single-line directives anchor at their `line`
/// field. `EditableVirtualText` anchors at `after`.
pub(crate) fn anchor_line(d: &DisplayDirective) -> usize {
    use DisplayDirective::*;
    match d {
        Hide { range } => range.start,
        Fold { range, .. } => range.start,
        InsertBefore { line, .. }
        | InsertAfter { line, .. }
        | InsertInline { line, .. }
        | HideInline { line, .. }
        | InlineBox { line, .. }
        | StyleInline { line, .. }
        | StyleLine { line, .. }
        | Gutter { line, .. }
        | VirtualText { line, .. } => *line,
        EditableVirtualText { after, .. } => *after,
    }
}

/// Compute a stable hash of the concatenated line texts in
/// `lines`. The hash domain covers the `Atom::contents` strings;
/// styles, atom counts, and atom boundaries do not affect the
/// hash. This matches the `PerBuffer` strategy's "text only"
/// soundness contract.
///
/// Computed once per frame and shared across all `PerBuffer`
/// lenses by the dispatcher.
pub(crate) fn hash_buffer_text(lines: &[Vec<Atom>]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    // Mix in line count first so two arrangements with the same
    // total bytes but different line breaks hash differently.
    lines.len().hash(&mut hasher);
    for atoms in lines {
        // Mark line boundaries with a per-line length prefix so
        // splitting one line into two won't collide with the
        // unsplit version. Within a line, write atom bytes
        // directly (no per-atom length prefix) so atom
        // segmentation doesn't affect the hash — what matters
        // is the concatenated text.
        let total_bytes: usize = atoms.iter().map(|a| a.contents.len()).sum();
        total_bytes.hash(&mut hasher);
        for atom in atoms {
            hasher.write(atom.contents.as_bytes());
        }
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::PluginId;

    fn lid(plugin: &str, name: &str) -> LensId {
        LensId::new(PluginId(plugin.into()), name)
    }

    fn line(text: &str) -> Vec<Atom> {
        vec![Atom::plain(text)]
    }

    // -----------------------------------------------------------------
    // Hash properties
    // -----------------------------------------------------------------

    #[test]
    fn hash_is_deterministic() {
        let buf = vec![line("hello"), line("world")];
        assert_eq!(hash_buffer_text(&buf), hash_buffer_text(&buf));
    }

    #[test]
    fn hash_changes_when_a_line_changes() {
        let a = vec![line("hello"), line("world")];
        let b = vec![line("hello"), line("WORLD")];
        assert_ne!(hash_buffer_text(&a), hash_buffer_text(&b));
    }

    #[test]
    fn hash_changes_when_line_count_changes() {
        let a = vec![line("hello"), line("world")];
        let b = vec![line("hello"), line("world"), line("!")];
        assert_ne!(hash_buffer_text(&a), hash_buffer_text(&b));
    }

    #[test]
    fn hash_distinguishes_split_vs_unsplit_with_same_bytes() {
        // Same total bytes, different line layout — must hash
        // distinct. Without the per-line length prefix this
        // would collide.
        let one = vec![line("helloworld")];
        let two = vec![line("hello"), line("world")];
        assert_ne!(hash_buffer_text(&one), hash_buffer_text(&two));
    }

    #[test]
    fn hash_ignores_atom_boundaries_within_a_line() {
        // One line with the same content but split across two
        // atoms — same hash. (PerBuffer strategy doesn't care
        // about atom segmentation; it cares about the text.)
        let one_atom = vec![vec![Atom::plain("hello")]];
        let two_atoms = vec![vec![Atom::plain("hel"), Atom::plain("lo")]];
        assert_eq!(hash_buffer_text(&one_atom), hash_buffer_text(&two_atoms),);
    }

    // -----------------------------------------------------------------
    // Cache get / put / invalidate
    // -----------------------------------------------------------------

    #[test]
    fn get_on_empty_cache_returns_none() {
        let cache = LensCache::default();
        assert_eq!(cache.get(&lid("p", "x"), 42), None);
    }

    #[test]
    fn put_then_get_with_matching_hash_returns_output() {
        let mut cache = LensCache::default();
        let id = lid("p", "x");
        cache.put(id.clone(), 42, vec![]);
        assert_eq!(cache.get(&id, 42), Some(vec![]));
    }

    #[test]
    fn get_with_mismatched_hash_returns_none() {
        let mut cache = LensCache::default();
        let id = lid("p", "x");
        cache.put(id.clone(), 42, vec![]);
        assert_eq!(cache.get(&id, 43), None, "stale entry → cache miss");
    }

    #[test]
    fn put_overwrites_existing_entry() {
        let mut cache = LensCache::default();
        let id = lid("p", "x");
        cache.put(id.clone(), 42, vec![]);
        cache.put(id.clone(), 99, vec![]);
        assert_eq!(cache.get(&id, 42), None);
        assert_eq!(cache.get(&id, 99), Some(vec![]));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn invalidate_drops_entry() {
        let mut cache = LensCache::default();
        let id = lid("p", "x");
        cache.put(id.clone(), 42, vec![]);
        cache.invalidate(&id);
        assert_eq!(cache.get(&id, 42), None);
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn invalidate_missing_entry_is_noop() {
        let mut cache = LensCache::default();
        cache.invalidate(&lid("ghost", "missing")); // does not panic
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn cache_strategy_default_is_none() {
        let strategy: CacheStrategy = Default::default();
        assert_eq!(strategy, CacheStrategy::None);
    }
}
