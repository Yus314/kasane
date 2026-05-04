//! Tests for the Composable Lenses MVP.

use super::{CacheStrategy, Lens, LensId, LensRegistry};
use crate::display::DisplayDirective;
use crate::plugin::{AppView, PluginId};
use crate::protocol::WireFace;
use crate::state::AppState;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

// -----------------------------------------------------------------
// Test fixtures
// -----------------------------------------------------------------

fn pid(s: &str) -> PluginId {
    PluginId(s.into())
}

fn id(plugin: &str, name: &str) -> LensId {
    LensId::new(pid(plugin), name)
}

/// A lens that always emits the same directive — useful for
/// verifying registration / toggle behaviour without needing real
/// state.
struct FixedLens {
    id: LensId,
    directive: DisplayDirective,
    priority: i16,
}

impl FixedLens {
    fn new(id: LensId, directive: DisplayDirective) -> Self {
        Self {
            id,
            directive,
            priority: 0,
        }
    }
    fn with_priority(mut self, p: i16) -> Self {
        self.priority = p;
        self
    }
}

impl Lens for FixedLens {
    fn id(&self) -> LensId {
        self.id.clone()
    }
    fn priority(&self) -> i16 {
        self.priority
    }
    fn display(&self, _view: &AppView<'_>) -> Vec<DisplayDirective> {
        vec![self.directive.clone()]
    }
}

fn style_line(line: u32) -> DisplayDirective {
    DisplayDirective::StyleLine {
        line: line as usize,
        face: WireFace::default(),
        z_order: 0,
    }
}

// -----------------------------------------------------------------
// Registry mechanics
// -----------------------------------------------------------------

#[test]
fn empty_registry_collects_nothing() {
    let registry = LensRegistry::new();
    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);
    assert_eq!(registry.enabled_count(), 0);

    let app = AppState::default();
    let view = AppView::new(&app);
    let dirs = registry.collect_directives(&view);
    assert!(dirs.is_empty());
}

#[test]
fn register_inserts_lens_but_starts_disabled() {
    let mut registry = LensRegistry::new();
    let lens_id = id("p1", "ws");
    registry.register(Arc::new(FixedLens::new(lens_id.clone(), style_line(0))));
    assert!(registry.is_registered(&lens_id));
    assert!(!registry.is_enabled(&lens_id));
    assert_eq!(registry.len(), 1);
    assert_eq!(registry.enabled_count(), 0);
}

#[test]
fn enable_disable_toggles_emission() {
    let mut registry = LensRegistry::new();
    let lens_id = id("p1", "ws");
    registry.register(Arc::new(FixedLens::new(lens_id.clone(), style_line(3))));

    let app = AppState::default();
    let view = AppView::new(&app);

    // Disabled → no output.
    assert!(registry.collect_directives(&view).is_empty());

    // Enable → emission.
    registry.enable(&lens_id);
    let dirs = registry.collect_directives(&view);
    assert_eq!(dirs.len(), 1);
    assert!(matches!(
        dirs[0].0,
        DisplayDirective::StyleLine { line: 3, .. }
    ));
    assert_eq!(dirs[0].2, pid("p1"));

    // Disable → silence again.
    registry.disable(&lens_id);
    assert!(registry.collect_directives(&view).is_empty());
}

#[test]
fn toggle_returns_new_state_and_flips() {
    let mut registry = LensRegistry::new();
    let lens_id = id("p1", "x");
    registry.register(Arc::new(FixedLens::new(lens_id.clone(), style_line(0))));

    assert!(!registry.is_enabled(&lens_id));
    assert!(registry.toggle(&lens_id), "toggle off→on returns true");
    assert!(registry.is_enabled(&lens_id));
    assert!(!registry.toggle(&lens_id), "toggle on→off returns false");
    assert!(!registry.is_enabled(&lens_id));
}

#[test]
fn enable_unregistered_lens_is_noop() {
    let mut registry = LensRegistry::new();
    let lens_id = id("ghost", "missing");
    registry.enable(&lens_id);
    assert!(!registry.is_enabled(&lens_id));
    assert_eq!(registry.enabled_count(), 0);
}

#[test]
fn toggle_unregistered_lens_is_noop_and_returns_false() {
    let mut registry = LensRegistry::new();
    let lens_id = id("ghost", "missing");
    assert!(!registry.toggle(&lens_id));
    assert!(!registry.is_enabled(&lens_id));
}

#[test]
fn unregister_removes_from_enabled_set() {
    let mut registry = LensRegistry::new();
    let lens_id = id("p1", "x");
    registry.register(Arc::new(FixedLens::new(lens_id.clone(), style_line(0))));
    registry.enable(&lens_id);
    assert!(registry.is_enabled(&lens_id));

    registry.unregister(&lens_id);
    assert!(!registry.is_registered(&lens_id));
    assert!(!registry.is_enabled(&lens_id));
}

#[test]
fn re_register_replaces_existing_lens() {
    let mut registry = LensRegistry::new();
    let lens_id = id("p1", "x");
    registry.register(Arc::new(FixedLens::new(lens_id.clone(), style_line(0))));
    registry.enable(&lens_id);

    // Replace with a different directive (line 7 instead of 0).
    registry.register(Arc::new(FixedLens::new(lens_id.clone(), style_line(7))));

    let app = AppState::default();
    let view = AppView::new(&app);
    let dirs = registry.collect_directives(&view);
    assert_eq!(dirs.len(), 1);
    assert!(
        matches!(dirs[0].0, DisplayDirective::StyleLine { line: 7, .. }),
        "second register replaces; enable state is preserved"
    );
}

// -----------------------------------------------------------------
// Composition
// -----------------------------------------------------------------

#[test]
fn multiple_enabled_lenses_compose_in_lens_id_order() {
    let mut registry = LensRegistry::new();
    // Register in non-canonical order — the registry sorts by
    // LensId so output order is deterministic.
    let a = id("z-plugin", "a");
    let b = id("a-plugin", "b");
    registry.register(Arc::new(FixedLens::new(a.clone(), style_line(10))));
    registry.register(Arc::new(FixedLens::new(b.clone(), style_line(20))));
    registry.enable(&a);
    registry.enable(&b);

    let app = AppState::default();
    let view = AppView::new(&app);
    let dirs = registry.collect_directives(&view);
    assert_eq!(dirs.len(), 2);
    // a-plugin sorts before z-plugin → b emits first.
    let lines: Vec<u32> = dirs
        .iter()
        .map(|(d, _, _)| match d {
            DisplayDirective::StyleLine { line, .. } => *line as u32,
            _ => panic!("unexpected directive"),
        })
        .collect();
    assert_eq!(lines, vec![20, 10]);
}

#[test]
fn priority_passes_through_to_emission_tuple() {
    let mut registry = LensRegistry::new();
    let lens_id = id("p1", "high");
    registry.register(Arc::new(
        FixedLens::new(lens_id.clone(), style_line(0)).with_priority(42),
    ));
    registry.enable(&lens_id);

    let app = AppState::default();
    let view = AppView::new(&app);
    let dirs = registry.collect_directives(&view);
    assert_eq!(dirs.len(), 1);
    assert_eq!(dirs[0].1, 42, "priority lifts onto the emitted tuple");
}

// -----------------------------------------------------------------
// Identity & introspection
// -----------------------------------------------------------------

#[test]
fn registered_ids_are_sorted() {
    let mut registry = LensRegistry::new();
    let a = id("z", "1");
    let b = id("a", "2");
    let c = id("m", "3");
    registry.register(Arc::new(FixedLens::new(a.clone(), style_line(0))));
    registry.register(Arc::new(FixedLens::new(b.clone(), style_line(0))));
    registry.register(Arc::new(FixedLens::new(c.clone(), style_line(0))));
    let order: Vec<_> = registry.registered_ids().cloned().collect();
    assert_eq!(order, vec![b, c, a]);
}

#[test]
fn equality_compares_enabled_set_and_registered_ids() {
    let mut a = LensRegistry::new();
    let mut b = LensRegistry::new();

    let lens_a = id("p", "x");
    a.register(Arc::new(FixedLens::new(lens_a.clone(), style_line(0))));
    b.register(Arc::new(FixedLens::new(lens_a.clone(), style_line(99))));
    // Different lens trait objects but same id → equal.
    assert_eq!(a, b);

    a.enable(&lens_a);
    assert_ne!(a, b, "differing enabled sets break equality");

    b.enable(&lens_a);
    assert_eq!(a, b);
}

// -----------------------------------------------------------------
// Integration with AppState
// -----------------------------------------------------------------

#[test]
fn appstate_default_carries_empty_lens_registry() {
    let state = AppState::default();
    assert_eq!(state.lens_registry.len(), 0);
    assert_eq!(state.lens_registry.enabled_count(), 0);
}

#[test]
fn lens_disable_via_appstate_field_takes_effect_immediately() {
    let mut state = AppState::default();
    let lens_id = id("p1", "ws");
    state
        .lens_registry
        .register(Arc::new(FixedLens::new(lens_id.clone(), style_line(5))));
    state.lens_registry.enable(&lens_id);

    let view = AppView::new(&state);
    assert_eq!(state.lens_registry.collect_directives(&view).len(), 1);

    // Mutate via the field on a separate scope — simulates a CLI
    // / UI toggle path.
    state.lens_registry.disable(&lens_id);
    let view = AppView::new(&state);
    assert!(state.lens_registry.collect_directives(&view).is_empty());
}

// -----------------------------------------------------------------
// End-to-end pipeline integration
// -----------------------------------------------------------------

#[test]
fn lens_directives_flow_through_plugin_runtime_collect() {
    use crate::plugin::PluginRuntime;
    use crate::protocol::Line;
    use std::sync::Arc as StdArc;

    // A buffer with at least 4 lines so a `Hide { 3..4 }`
    // directive is in-range. `lines` is `Arc<Vec<Line>>`.
    let mut state = AppState::default();
    state.observed.lines = StdArc::new(vec![Line::default(); 10]);

    // Lens emits a Hide for line 3 — single-leaf, no plugin
    // overlaps, the algebra resolver returns it verbatim.
    let lens_id = id("integration", "hide-3");
    state.lens_registry.register(Arc::new(FixedLens::new(
        lens_id.clone(),
        DisplayDirective::Hide { range: 3..4 },
    )));
    state.lens_registry.enable(&lens_id);

    let registry = PluginRuntime::new();
    let view_handle = registry.view();
    let directives = view_handle.collect_display_directives(&AppView::new(&state));

    // Without enabling the lens, this would be empty (no
    // display-transform plugin AND no lens). With the lens
    // enabled, the Hide directive flows through.
    assert_eq!(directives.len(), 1);
    assert!(matches!(
        directives[0],
        DisplayDirective::Hide { ref range } if *range == (3..4),
    ));

    // Disable → directive disappears.
    state.lens_registry.disable(&lens_id);
    let directives = view_handle.collect_display_directives(&AppView::new(&state));
    assert!(directives.is_empty());
}

// -----------------------------------------------------------------
// PerBuffer cache behaviour
// -----------------------------------------------------------------

/// A lens that records how many times its `display()` is called.
/// Used to verify cache hits skip the invocation entirely.
struct CountingLens {
    id: LensId,
    invocations: Arc<AtomicUsize>,
    strategy: CacheStrategy,
}

impl CountingLens {
    fn new(id: LensId, strategy: CacheStrategy) -> (Self, Arc<AtomicUsize>) {
        let invocations = Arc::new(AtomicUsize::new(0));
        (
            Self {
                id,
                invocations: invocations.clone(),
                strategy,
            },
            invocations,
        )
    }
}

impl Lens for CountingLens {
    fn id(&self) -> LensId {
        self.id.clone()
    }
    fn cache_strategy(&self) -> CacheStrategy {
        self.strategy
    }
    fn display(&self, _view: &AppView<'_>) -> Vec<DisplayDirective> {
        self.invocations.fetch_add(1, Ordering::SeqCst);
        vec![style_line(0)]
    }
}

fn appstate_with_lines(texts: &[&str]) -> AppState {
    use crate::protocol::{Atom, Line};
    use std::sync::Arc as StdArc;
    let lines: Vec<Line> = texts.iter().map(|t| vec![Atom::plain(*t)]).collect();
    let mut state = AppState::default();
    state.observed.lines = StdArc::new(lines);
    state
}

#[test]
fn perbuffer_lens_invokes_once_per_buffer_then_caches() {
    let (lens, count) = CountingLens::new(id("p", "cache-me"), CacheStrategy::PerBuffer);
    let lens_id = lens.id();
    let state = appstate_with_lines(&["hello", "world"]);

    let mut registry = LensRegistry::new();
    registry.register(Arc::new(lens));
    registry.enable(&lens_id);

    let view = AppView::new(&state);
    // First call: cache miss, lens runs.
    let _ = registry.collect_directives(&view);
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(registry.cache_len(), 1);

    // Subsequent calls against the same buffer: cache hit, lens
    // does not run.
    let _ = registry.collect_directives(&view);
    let _ = registry.collect_directives(&view);
    assert_eq!(
        count.load(Ordering::SeqCst),
        1,
        "lens invoked exactly once across three collects on unchanged buffer"
    );
}

#[test]
fn perbuffer_lens_reinvokes_when_line_text_changes() {
    let (lens, count) = CountingLens::new(id("p", "cache-me"), CacheStrategy::PerBuffer);
    let lens_id = lens.id();

    let mut registry = LensRegistry::new();
    registry.register(Arc::new(lens));
    registry.enable(&lens_id);

    let state_a = appstate_with_lines(&["hello"]);
    let _ = registry.collect_directives(&AppView::new(&state_a));
    assert_eq!(count.load(Ordering::SeqCst), 1);

    // Different buffer text → different hash → cache miss.
    let state_b = appstate_with_lines(&["world"]);
    let _ = registry.collect_directives(&AppView::new(&state_b));
    assert_eq!(count.load(Ordering::SeqCst), 2);

    // Back to the original text → also a cache miss because the
    // entry was overwritten.
    let _ = registry.collect_directives(&AppView::new(&state_a));
    assert_eq!(count.load(Ordering::SeqCst), 3);
}

#[test]
fn none_strategy_lens_runs_every_call() {
    let (lens, count) = CountingLens::new(id("p", "uncached"), CacheStrategy::None);
    let lens_id = lens.id();

    let mut registry = LensRegistry::new();
    registry.register(Arc::new(lens));
    registry.enable(&lens_id);
    let state = appstate_with_lines(&["hello"]);

    for _ in 0..5 {
        let _ = registry.collect_directives(&AppView::new(&state));
    }
    assert_eq!(count.load(Ordering::SeqCst), 5);
    assert_eq!(
        registry.cache_len(),
        0,
        "None strategy never populates the cache"
    );
}

#[test]
fn disable_drops_cache_entry_so_re_enable_re_invokes() {
    let (lens, count) = CountingLens::new(id("p", "cache-me"), CacheStrategy::PerBuffer);
    let lens_id = lens.id();

    let mut registry = LensRegistry::new();
    registry.register(Arc::new(lens));
    registry.enable(&lens_id);
    let state = appstate_with_lines(&["hello"]);

    let _ = registry.collect_directives(&AppView::new(&state));
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(registry.cache_len(), 1);

    registry.disable(&lens_id);
    assert_eq!(
        registry.cache_len(),
        0,
        "disable drops the lens's cache entry"
    );

    registry.enable(&lens_id);
    let _ = registry.collect_directives(&AppView::new(&state));
    assert_eq!(
        count.load(Ordering::SeqCst),
        2,
        "re-enable forces a fresh invocation"
    );
}

#[test]
fn unregister_drops_cache_entry() {
    let (lens, _count) = CountingLens::new(id("p", "cache-me"), CacheStrategy::PerBuffer);
    let lens_id = lens.id();
    let mut registry = LensRegistry::new();
    registry.register(Arc::new(lens));
    registry.enable(&lens_id);
    let state = appstate_with_lines(&["hello"]);

    let _ = registry.collect_directives(&AppView::new(&state));
    assert_eq!(registry.cache_len(), 1);
    registry.unregister(&lens_id);
    assert_eq!(registry.cache_len(), 0);
}

#[test]
fn re_register_invalidates_previous_cache_entry() {
    let (lens_a, count_a) = CountingLens::new(id("p", "cache-me"), CacheStrategy::PerBuffer);
    let lens_id = lens_a.id();
    let mut registry = LensRegistry::new();
    registry.register(Arc::new(lens_a));
    registry.enable(&lens_id);

    let state = appstate_with_lines(&["hello"]);
    let _ = registry.collect_directives(&AppView::new(&state));
    assert_eq!(count_a.load(Ordering::SeqCst), 1);
    assert_eq!(registry.cache_len(), 1);

    // Replace with a fresh lens at the same id. The new lens
    // should be invoked even against the unchanged buffer (its
    // output may differ from the cached output of the previous
    // instance).
    let (lens_b, count_b) = CountingLens::new(id("p", "cache-me"), CacheStrategy::PerBuffer);
    registry.register(Arc::new(lens_b));

    let _ = registry.collect_directives(&AppView::new(&state));
    assert_eq!(
        count_a.load(Ordering::SeqCst),
        1,
        "old instance not invoked"
    );
    assert_eq!(
        count_b.load(Ordering::SeqCst),
        1,
        "new instance invoked from scratch"
    );
}

#[test]
fn cache_amortizes_buffer_hash_across_multiple_perbuffer_lenses() {
    // Two PerBuffer lenses; the buffer hash is computed once per
    // collect_directives call. Both lenses should populate cache
    // entries on the first call and skip invocation on the
    // second.
    let (lens_a, count_a) = CountingLens::new(id("a", "x"), CacheStrategy::PerBuffer);
    let (lens_b, count_b) = CountingLens::new(id("b", "y"), CacheStrategy::PerBuffer);
    let id_a = lens_a.id();
    let id_b = lens_b.id();

    let mut registry = LensRegistry::new();
    registry.register(Arc::new(lens_a));
    registry.register(Arc::new(lens_b));
    registry.enable(&id_a);
    registry.enable(&id_b);

    let state = appstate_with_lines(&["hello", "world"]);
    let _ = registry.collect_directives(&AppView::new(&state));
    let _ = registry.collect_directives(&AppView::new(&state));
    assert_eq!(count_a.load(Ordering::SeqCst), 1);
    assert_eq!(count_b.load(Ordering::SeqCst), 1);
    assert_eq!(registry.cache_len(), 2);
}

#[test]
fn empty_enabled_set_does_not_compute_buffer_hash() {
    // No enabled lenses → no hash computation, no cache writes.
    // We can't directly observe the hash compute, but we can
    // observe cache_len stays 0 and collect_directives returns
    // empty.
    let registry = LensRegistry::new();
    let state = appstate_with_lines(&["hello"]);
    let dirs = registry.collect_directives(&AppView::new(&state));
    assert!(dirs.is_empty());
    assert_eq!(registry.cache_len(), 0);
}

// -----------------------------------------------------------------
// PerLine cache behaviour
// -----------------------------------------------------------------

/// Lens that records which (line_idx) values its display_line was
/// called against. Used to verify per-line cache hits skip
/// invocations for unchanged lines.
struct PerLineCounter {
    id: LensId,
    invoked_lines: Arc<std::sync::Mutex<Vec<usize>>>,
}

impl PerLineCounter {
    fn new(id: LensId) -> (Self, Arc<std::sync::Mutex<Vec<usize>>>) {
        let invoked_lines = Arc::new(std::sync::Mutex::new(Vec::new()));
        (
            Self {
                id,
                invoked_lines: invoked_lines.clone(),
            },
            invoked_lines,
        )
    }
}

impl Lens for PerLineCounter {
    fn id(&self) -> LensId {
        self.id.clone()
    }
    fn cache_strategy(&self) -> CacheStrategy {
        CacheStrategy::PerLine
    }
    fn display(&self, _view: &AppView<'_>) -> Vec<DisplayDirective> {
        // Whole-buffer fallback — the dispatcher uses display_line
        // for PerLine, so this should not be called from the
        // PerLine path. Implementing it for the trait is required.
        Vec::new()
    }
    fn display_line(&self, view: &AppView<'_>, line: usize) -> Vec<DisplayDirective> {
        self.invoked_lines.lock().unwrap().push(line);
        // Emit one StyleLine directive per line so we have something
        // to assert on output as well.
        let _ = view;
        vec![DisplayDirective::StyleLine {
            line,
            face: WireFace::default(),
            z_order: 0,
        }]
    }
}

#[test]
fn perline_lens_invokes_each_line_once_then_caches_all() {
    let (lens, invoked) = PerLineCounter::new(id("p", "per-line"));
    let lens_id = lens.id();
    let mut registry = LensRegistry::new();
    registry.register(Arc::new(lens));
    registry.enable(&lens_id);

    let state = appstate_with_lines(&["a", "b", "c"]);

    let _ = registry.collect_directives(&AppView::new(&state));
    assert_eq!(*invoked.lock().unwrap(), vec![0, 1, 2]);
    assert_eq!(registry.cache_len(), 3, "one cache entry per line");

    // Second call against unchanged buffer: zero new invocations.
    invoked.lock().unwrap().clear();
    let _ = registry.collect_directives(&AppView::new(&state));
    assert!(
        invoked.lock().unwrap().is_empty(),
        "cache hits everywhere → zero invocations"
    );
}

#[test]
fn perline_invalidates_only_changed_line() {
    let (lens, invoked) = PerLineCounter::new(id("p", "per-line"));
    let lens_id = lens.id();
    let mut registry = LensRegistry::new();
    registry.register(Arc::new(lens));
    registry.enable(&lens_id);

    let state_a = appstate_with_lines(&["a", "b", "c"]);
    let _ = registry.collect_directives(&AppView::new(&state_a));
    assert_eq!(*invoked.lock().unwrap(), vec![0, 1, 2]);
    invoked.lock().unwrap().clear();

    // Change only line 1. Lines 0 and 2 keep their cache entries.
    let state_b = appstate_with_lines(&["a", "BB", "c"]);
    let _ = registry.collect_directives(&AppView::new(&state_b));
    assert_eq!(
        *invoked.lock().unwrap(),
        vec![1],
        "only line 1 re-invoked; lines 0 and 2 hit cache"
    );
    // Cache still has 3 entries (one per line; line 1's was
    // overwritten with the new hash).
    assert_eq!(registry.cache_len(), 3);
}

#[test]
fn perline_default_display_line_filters_whole_buffer_by_anchor_line() {
    // A lens that overrides only `display()` (not `display_line`)
    // — the default `display_line` impl filters by anchor line.
    struct WholeBufferLens {
        id: LensId,
    }
    impl Lens for WholeBufferLens {
        fn id(&self) -> LensId {
            self.id.clone()
        }
        fn cache_strategy(&self) -> CacheStrategy {
            CacheStrategy::PerLine
        }
        fn display(&self, _view: &AppView<'_>) -> Vec<DisplayDirective> {
            vec![
                DisplayDirective::StyleLine {
                    line: 0,
                    face: WireFace::default(),
                    z_order: 0,
                },
                DisplayDirective::StyleLine {
                    line: 2,
                    face: WireFace::default(),
                    z_order: 0,
                },
            ]
        }
    }

    let lens_id = id("p", "default-display-line");
    let mut registry = LensRegistry::new();
    registry.register(Arc::new(WholeBufferLens {
        id: lens_id.clone(),
    }));
    registry.enable(&lens_id);

    let state = appstate_with_lines(&["a", "b", "c"]);
    let dirs = registry.collect_directives(&AppView::new(&state));
    // Two emissions (lines 0 and 2 only — line 1 produced none).
    assert_eq!(dirs.len(), 2);
    let lines: Vec<usize> = dirs
        .iter()
        .map(|(d, _, _)| match d {
            DisplayDirective::StyleLine { line, .. } => *line,
            _ => panic!(),
        })
        .collect();
    assert_eq!(lines, vec![0, 2]);
    // 3 cache entries (one per line — line 1 cached as empty).
    assert_eq!(registry.cache_len(), 3);
}

#[test]
fn perline_cache_length_grows_with_line_count() {
    let (lens, _) = PerLineCounter::new(id("p", "x"));
    let lens_id = lens.id();
    let mut registry = LensRegistry::new();
    registry.register(Arc::new(lens));
    registry.enable(&lens_id);

    // 5 lines → 5 cache entries after one collect.
    let state = appstate_with_lines(&["a", "b", "c", "d", "e"]);
    let _ = registry.collect_directives(&AppView::new(&state));
    assert_eq!(registry.cache_len(), 5);

    // Disable drops them all.
    registry.disable(&lens_id);
    assert_eq!(registry.cache_len(), 0);
}

#[test]
fn invalidate_drops_per_line_entries_too() {
    let (lens, _) = PerLineCounter::new(id("p", "x"));
    let lens_id = lens.id();
    let mut registry = LensRegistry::new();
    registry.register(Arc::new(lens));
    registry.enable(&lens_id);

    let state = appstate_with_lines(&["a", "b"]);
    let _ = registry.collect_directives(&AppView::new(&state));
    assert_eq!(registry.cache_len(), 2);

    registry.unregister(&lens_id);
    assert_eq!(
        registry.cache_len(),
        0,
        "unregister drops per-line entries for the lens"
    );
}

// -----------------------------------------------------------------
// Auto-wired lifecycle: unregister_by_plugin + sync_lenses
// -----------------------------------------------------------------

#[test]
fn unregister_by_plugin_drops_all_lenses_for_that_plugin() {
    let mut registry = LensRegistry::new();
    let alpha_a = id("alpha", "a");
    let alpha_b = id("alpha", "b");
    let beta_c = id("beta", "c");
    registry.register(Arc::new(FixedLens::new(alpha_a.clone(), style_line(0))));
    registry.register(Arc::new(FixedLens::new(alpha_b.clone(), style_line(1))));
    registry.register(Arc::new(FixedLens::new(beta_c.clone(), style_line(2))));
    registry.enable(&alpha_a);
    registry.enable(&beta_c);
    assert_eq!(registry.len(), 3);
    assert_eq!(registry.enabled_count(), 2);

    let dropped = registry.unregister_by_plugin(&pid("alpha"));
    assert_eq!(dropped, 2, "two lenses owned by alpha");
    assert!(!registry.is_registered(&alpha_a));
    assert!(!registry.is_registered(&alpha_b));
    assert!(registry.is_registered(&beta_c), "beta untouched");
    assert!(registry.is_enabled(&beta_c));
    assert_eq!(registry.len(), 1);
}

#[test]
fn unregister_by_plugin_unknown_plugin_is_noop() {
    let mut registry = LensRegistry::new();
    let lens_id = id("p", "x");
    registry.register(Arc::new(FixedLens::new(lens_id.clone(), style_line(0))));
    let dropped = registry.unregister_by_plugin(&pid("ghost"));
    assert_eq!(dropped, 0);
    assert!(registry.is_registered(&lens_id));
}

#[test]
fn unregister_by_plugin_drops_per_line_cache_entries() {
    let (lens, _) = PerLineCounter::new(id("p", "x"));
    let lens_id = lens.id();
    let mut registry = LensRegistry::new();
    registry.register(Arc::new(lens));
    registry.enable(&lens_id);

    let state = appstate_with_lines(&["a", "b", "c"]);
    let _ = registry.collect_directives(&AppView::new(&state));
    assert_eq!(registry.cache_len(), 3);

    let dropped = registry.unregister_by_plugin(&pid("p"));
    assert_eq!(dropped, 1);
    assert_eq!(registry.cache_len(), 0);
}
