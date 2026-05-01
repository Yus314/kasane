//! End-to-end integration tests for the Semantic Zoom plugin.
//!
//! Verifies: plugin registration → key dispatch → directive generation →
//! `resolve()` → `DisplayMap::build()` → invariant compliance (INV-1 to INV-7).

use kasane_core::display::{
    DisplayDirective, DisplayLine, DisplayMap, InverseResult, ProjectionId,
};
use kasane_core::input::{Key, KeyEvent, Modifiers};
use kasane_core::plugin::{AppView, Command, PluginBridge, PluginRuntime};
use kasane_core::state::{AppState, DirtyFlags};
use kasane_core::test_support::make_line;

use kasane_core::plugin::semantic_zoom::SemanticZoomPlugin;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup_state(lines: Vec<kasane_core::protocol::Line>) -> AppState {
    let mut state = kasane_core::test_support::test_state_80x24();
    state.observed.lines = (lines).into();
    state.observed.status_default_style = state.observed.default_style.clone();
    state.inference.status_line = make_line(" main.rs ");
    state.observed.status_mode_line = make_line("normal");
    state
}

fn zoom_projection_id() -> ProjectionId {
    ProjectionId::new("kasane.semantic-zoom")
}

fn register_zoom_plugin(state: &AppState) -> PluginRuntime {
    let mut runtime = PluginRuntime::new();
    runtime.register_backend(Box::new(PluginBridge::new(SemanticZoomPlugin)));
    let _ = runtime.init_all_batch(&AppView::new(state));
    runtime.prepare_plugin_cache(DirtyFlags::ALL);
    runtime
}

fn ctrl_plus() -> KeyEvent {
    KeyEvent {
        key: Key::Char('+'),
        modifiers: Modifiers::CTRL,
    }
}

fn ctrl_zero() -> KeyEvent {
    KeyEvent {
        key: Key::Char('0'),
        modifiers: Modifiers::CTRL,
    }
}

/// Dispatch a key event and apply returned commands (e.g. SetStructuralProjection)
/// to the state, mirroring what the real event loop does.
fn dispatch_key(runtime: &mut PluginRuntime, state: &mut AppState, key: &KeyEvent) {
    use kasane_core::plugin::KeyDispatchResult;

    let app = AppView::new(state);
    let result = runtime.dispatch_key_middleware(key, &app);

    if let KeyDispatchResult::Consumed { commands, .. } = result {
        for cmd in commands {
            match cmd {
                Command::SetStructuralProjection(id) => {
                    state.config.projection_policy.set_structural(id);
                }
                _ => {}
            }
        }
    }

    runtime.prepare_plugin_cache(DirtyFlags::PLUGIN_STATE);
}

/// Collect resolved directives from the plugin runtime for the given state.
fn collect_directives(runtime: &PluginRuntime, state: &AppState) -> Vec<DisplayDirective> {
    let app = AppView::new(state);
    runtime.view().collect_display_directives(&app)
}

/// Build a DisplayMap from directives, verifying that `build()` succeeds
/// (debug_assertions check INV-1 to INV-7 internally).
fn build_display_map(line_count: usize, directives: &[DisplayDirective]) -> DisplayMap {
    DisplayMap::build(line_count, directives)
}

fn code_lines() -> Vec<kasane_core::protocol::Line> {
    [
        "fn main() {",         // 0
        "    let x = 1;",      // 1
        "    let y = 2;",      // 2
        "    if x > 0 {",      // 3
        "        println!();", // 4
        "    }",               // 5
        "}",                   // 6
        "",                    // 7
        "fn helper() {",       // 8
        "    return 42;",      // 9
        "}",                   // 10
    ]
    .iter()
    .map(|s| make_line(s))
    .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn raw_level_produces_no_directives() {
    let state = setup_state(code_lines());
    let runtime = register_zoom_plugin(&state);

    // At level 0 (RAW, the default), projection is not active — no directives.
    let directives = collect_directives(&runtime, &state);
    assert!(
        directives.is_empty(),
        "RAW level should produce no directives"
    );
}

#[test]
fn zoom_in_auto_activates_projection() {
    let mut state = setup_state(code_lines());
    let mut runtime = register_zoom_plugin(&state);

    // Before zoom: projection is not active
    assert!(state.config.projection_policy.active_structural().is_none());

    // First Ctrl+Plus transitions RAW→ANNOTATED and activates the projection
    dispatch_key(&mut runtime, &mut state, &ctrl_plus());
    assert_eq!(
        state.config.projection_policy.active_structural(),
        Some(&zoom_projection_id())
    );
}

#[test]
fn zoom_out_to_raw_deactivates_projection() {
    let mut state = setup_state(code_lines());
    let mut runtime = register_zoom_plugin(&state);

    // Zoom in then back out
    dispatch_key(&mut runtime, &mut state, &ctrl_plus());
    assert!(state.config.projection_policy.active_structural().is_some());

    dispatch_key(&mut runtime, &mut state, &ctrl_zero());
    assert!(
        state.config.projection_policy.active_structural().is_none(),
        "returning to RAW should deactivate projection"
    );
}

#[test]
fn zoom_in_produces_directives_and_valid_display_map() {
    let mut state = setup_state(code_lines());
    let mut runtime = register_zoom_plugin(&state);

    // Zoom in to level 2 (Compressed) — Ctrl+Plus twice
    dispatch_key(&mut runtime, &mut state, &ctrl_plus());
    dispatch_key(&mut runtime, &mut state, &ctrl_plus());

    let directives = collect_directives(&runtime, &state);
    assert!(
        !directives.is_empty(),
        "COMPRESSED level should produce directives for indented code"
    );

    // Verify all spatial directives are valid
    let line_count = state.observed.lines.len();
    for d in &directives {
        match d {
            DisplayDirective::Fold { range, .. } => {
                assert!(range.start < range.end, "fold range must be non-empty");
                assert!(range.end <= line_count, "fold range out of bounds");
            }
            DisplayDirective::Hide { range } => {
                assert!(range.start < range.end, "hide range must be non-empty");
                assert!(range.end <= line_count, "hide range out of bounds");
            }
            _ => {}
        }
    }

    // Build DisplayMap — debug_assertions verify INV-1 to INV-7 internally.
    let dm = build_display_map(line_count, &directives);
    assert!(!dm.is_identity(), "COMPRESSED should modify the display");
    assert!(
        dm.display_line_count() < line_count,
        "COMPRESSED should reduce visible lines"
    );
}

#[test]
fn zoom_levels_monotonically_hide_more() {
    let mut state = setup_state(code_lines());
    let mut runtime = register_zoom_plugin(&state);
    let line_count = state.observed.lines.len();

    let mut display_counts = Vec::new();

    // Level 0 (RAW)
    let dm = build_display_map(line_count, &collect_directives(&runtime, &state));
    display_counts.push(dm.display_line_count());

    // Level 1 (ANNOTATED) — no spatial changes expected from indent strategy
    dispatch_key(&mut runtime, &mut state, &ctrl_plus());
    let dm = build_display_map(line_count, &collect_directives(&runtime, &state));
    display_counts.push(dm.display_line_count());

    // Level 2 (COMPRESSED)
    dispatch_key(&mut runtime, &mut state, &ctrl_plus());
    let dm = build_display_map(line_count, &collect_directives(&runtime, &state));
    display_counts.push(dm.display_line_count());

    // Level 3 (OUTLINE)
    dispatch_key(&mut runtime, &mut state, &ctrl_plus());
    let dm = build_display_map(line_count, &collect_directives(&runtime, &state));
    display_counts.push(dm.display_line_count());

    // Level 4 (SKELETON)
    dispatch_key(&mut runtime, &mut state, &ctrl_plus());
    let dm = build_display_map(line_count, &collect_directives(&runtime, &state));
    display_counts.push(dm.display_line_count());

    // SZ-INV-2: monotonicity — each level hides >= previous level
    for i in 1..display_counts.len() {
        assert!(
            display_counts[i] <= display_counts[i - 1],
            "level {i} ({} lines) should show ≤ level {} ({} lines)",
            display_counts[i],
            i - 1,
            display_counts[i - 1],
        );
    }
}

#[test]
fn no_overlapping_spatial_ranges_at_any_level() {
    let mut state = setup_state(code_lines());
    let mut runtime = register_zoom_plugin(&state);

    for level in 0..=4 {
        let directives = collect_directives(&runtime, &state);

        // SZ-INV-4: no overlapping Fold/Hide ranges
        let mut ranges: Vec<(usize, usize)> = Vec::new();
        for d in &directives {
            let r = match d {
                DisplayDirective::Fold { range, .. } => (range.start, range.end),
                DisplayDirective::Hide { range } => (range.start, range.end),
                _ => continue,
            };
            for prev in &ranges {
                assert!(
                    r.1 <= prev.0 || r.0 >= prev.1,
                    "level {level}: overlapping ranges {r:?} and {prev:?}"
                );
            }
            ranges.push(r);
        }

        dispatch_key(&mut runtime, &mut state, &ctrl_plus());
    }
}

#[test]
fn zoom_reset_returns_to_raw() {
    let mut state = setup_state(code_lines());
    let mut runtime = register_zoom_plugin(&state);

    // Zoom in 3 times
    for _ in 0..3 {
        dispatch_key(&mut runtime, &mut state, &ctrl_plus());
    }
    let directives = collect_directives(&runtime, &state);
    assert!(!directives.is_empty(), "should have directives at level 3");

    // Reset
    dispatch_key(&mut runtime, &mut state, &ctrl_zero());
    let directives = collect_directives(&runtime, &state);
    assert!(
        directives.is_empty(),
        "reset should return to RAW (no directives)"
    );
}

#[test]
fn display_map_buffer_roundtrip() {
    let mut state = setup_state(code_lines());
    let mut runtime = register_zoom_plugin(&state);

    // Zoom to COMPRESSED
    dispatch_key(&mut runtime, &mut state, &ctrl_plus());
    dispatch_key(&mut runtime, &mut state, &ctrl_plus());

    let line_count = state.observed.lines.len();
    let directives = collect_directives(&runtime, &state);
    let dm = build_display_map(line_count, &directives);

    // Every display line should map to a valid buffer line.
    for dy in 0..dm.display_line_count() {
        match dm.display_to_buffer(DisplayLine(dy)) {
            InverseResult::Actionable(bl) => {
                assert!(
                    bl.0 < line_count,
                    "display line {dy} maps to out-of-bounds buffer line"
                );
            }
            InverseResult::Informational {
                representative,
                range,
            } => {
                assert!(representative.0 < line_count);
                assert!(range.end <= line_count);
            }
            InverseResult::OutOfRange => {
                panic!("display line {dy} maps to OutOfRange");
            }
            _ => {}
        }
    }
}

#[test]
fn empty_buffer_produces_no_directives() {
    let mut state = setup_state(vec![]);
    let mut runtime = register_zoom_plugin(&state);

    // Zoom to max
    for _ in 0..5 {
        dispatch_key(&mut runtime, &mut state, &ctrl_plus());
    }

    let directives = collect_directives(&runtime, &state);
    assert!(
        directives.is_empty(),
        "empty buffer should produce no directives at any level"
    );
}

#[test]
fn single_line_buffer_no_panic() {
    let mut state = setup_state(vec![make_line("hello world")]);
    let mut runtime = register_zoom_plugin(&state);

    // Cycle through all levels
    for _ in 0..5 {
        dispatch_key(&mut runtime, &mut state, &ctrl_plus());
        let directives = collect_directives(&runtime, &state);
        let dm = build_display_map(state.observed.lines.len(), &directives);
        assert!(dm.display_line_count() >= 1 || state.observed.lines.is_empty());
    }
}

#[test]
fn zoom_key_without_prior_activation_still_works() {
    let mut state = setup_state(code_lines());
    let mut runtime = register_zoom_plugin(&state);

    // No manual set_structural — key dispatch auto-activates
    dispatch_key(&mut runtime, &mut state, &ctrl_plus()); // RAW→ANNOTATED + activate
    dispatch_key(&mut runtime, &mut state, &ctrl_plus()); // ANNOTATED→COMPRESSED

    let directives = collect_directives(&runtime, &state);
    assert!(
        !directives.is_empty(),
        "auto-activated projection should produce directives"
    );
}
