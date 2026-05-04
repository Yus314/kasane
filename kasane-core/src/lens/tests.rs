//! Tests for the Composable Lenses MVP.

use super::{Lens, LensId, LensRegistry};
use crate::display::DisplayDirective;
use crate::plugin::{AppView, PluginId};
use crate::protocol::WireFace;
use crate::state::AppState;
use std::sync::Arc;

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
