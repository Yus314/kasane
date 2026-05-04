//! Lens output cache (Composable Lenses follow-up — Roadmap §2.2).
//!
//! Caches the output of opt-in lenses so a frame in which the
//! buffer content didn't change can return the previous output
//! without re-invoking the lens.
//!
//! ## Cache key shape
//!
//! Keyed on `LensId`; each entry stores `(buffer_content_hash,
//! Vec<DisplayDirective>)`. The hash is computed once per frame
//! over all line texts in `view.lines()` (amortised across all
//! `PerBuffer`-strategy lenses).
//!
//! When the supplied buffer hash matches the cached entry's hash,
//! the cached output is returned verbatim. Otherwise the lens is
//! re-invoked and the cache entry is overwritten with the new
//! `(hash, output)`.
//!
//! ## Granularity
//!
//! The MVP caches at the **per-buffer** level — any change to any
//! line's text invalidates every `PerBuffer` lens's cache entry.
//! A future `PerLine` strategy could cache per `(LensId,
//! line_idx)` so a single-line edit invalidates exactly one entry
//! per lens; that requires a per-line method on the `Lens` trait
//! (output partitioning by line index doesn't match the trait's
//! whole-buffer `display()` signature ergonomically).
//!
//! ## Soundness caveat
//!
//! `PerBuffer` is sound only for lenses whose output depends on
//! line text **only** — not on cursor position, selection set,
//! syntax tree, or any other `AppView` state. The bundled lenses
//! (`TrailingWhitespaceLens`, `LongLineLens`, `IndentGuidesLens`)
//! satisfy this; user lenses opt in by overriding
//! `Lens::cache_strategy`. The `None` default keeps user lenses
//! uncached unless they explicitly say otherwise.
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
#[derive(Debug, Default)]
pub(crate) struct LensCache {
    entries: HashMap<LensId, CacheEntry>,
}

/// Shared handle. Cloning the handle shares the underlying cache;
/// dropping all handles drops the cache.
pub(crate) type SharedCache = Arc<Mutex<LensCache>>;

pub(crate) fn empty_cache() -> SharedCache {
    Arc::new(Mutex::new(LensCache::default()))
}

impl LensCache {
    /// Look up the cached output for `lens_id` against the
    /// supplied `buffer_hash`. Returns `Some(output_clone)` on
    /// hit, `None` on miss (no entry, or entry hash mismatches).
    pub(crate) fn get(&self, lens_id: &LensId, buffer_hash: u64) -> Option<Vec<DisplayDirective>> {
        self.entries.get(lens_id).and_then(|e| {
            if e.buffer_hash == buffer_hash {
                Some(e.output.clone())
            } else {
                None
            }
        })
    }

    /// Store the lens's output keyed on `(lens_id, buffer_hash)`.
    /// Overwrites any existing entry for `lens_id`.
    pub(crate) fn put(&mut self, lens_id: LensId, buffer_hash: u64, output: Vec<DisplayDirective>) {
        self.entries.insert(
            lens_id,
            CacheEntry {
                buffer_hash,
                output,
            },
        );
    }

    /// Drop the cache entry for `lens_id` (no-op if absent). Used
    /// by the registry's `disable` / `unregister` / re-register
    /// paths so a lens that returns to the dispatcher gets a
    /// fresh re-invocation rather than a stale cached output.
    pub(crate) fn invalidate(&mut self, lens_id: &LensId) {
        self.entries.remove(lens_id);
    }

    /// Number of cache entries currently held. Test-facing
    /// introspection.
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
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
