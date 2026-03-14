//! Soundness oracle test for incremental rendering.
//!
//! Verifies the invariant: for each single DirtyFlags value,
//! `render(state, dirty, warm_cache) == render(state, ALL, fresh_cache)`.
//!
//! This catches bugs where ViewCache section deps are too narrow,
//! causing stale cached sections to be served when state changes.

use kasane_core::plugin::PluginRegistry;
use kasane_core::protocol::{Color, Coord, Face, InfoStyle, KakouneRequest, MenuStyle, NamedColor};
use kasane_core::render::pipeline::render_pipeline_cached;
use kasane_core::render::{CellGrid, ViewCache};
use kasane_core::state::{AppState, DirtyFlags};
use kasane_core::test_support::{make_line, test_state_80x24};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a rich AppState with buffer, menu, info, and various options set.
fn rich_state() -> AppState {
    let mut state = test_state_80x24();
    state.lines = vec![
        make_line("fn main() {"),
        make_line("    println!(\"hello\");"),
        make_line("}"),
    ];
    state.status_line = make_line(" main.rs ");
    state.status_mode_line = make_line("normal");
    state.status_default_face = Face {
        fg: Color::Named(NamedColor::Cyan),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    };
    state.cursor_pos = Coord { line: 1, column: 4 };
    state.shadow_enabled = true;
    state.search_dropdown = false;

    // Show inline menu
    state.apply(KakouneRequest::MenuShow {
        items: vec![make_line("foo"), make_line("bar"), make_line("baz")],
        anchor: Coord { line: 1, column: 4 },
        selected_item_face: Face {
            fg: Color::Named(NamedColor::Black),
            bg: Color::Named(NamedColor::Cyan),
            ..Face::default()
        },
        menu_face: Face {
            fg: Color::Named(NamedColor::White),
            bg: Color::Named(NamedColor::Blue),
            ..Face::default()
        },
        style: MenuStyle::Inline,
    });
    // Select first item
    state.apply(KakouneRequest::MenuSelect { selected: 0 });

    // Show info popup
    state.apply(KakouneRequest::InfoShow {
        title: make_line("Help"),
        content: vec![make_line("This is help text"), make_line("second line")],
        anchor: Coord {
            line: 0,
            column: 20,
        },
        face: Face::default(),
        style: InfoStyle::Inline,
    });

    state
}

/// Render to a fresh CellGrid with given dirty flags and cache.
fn render_to_grid(
    state: &AppState,
    registry: &PluginRegistry,
    dirty: DirtyFlags,
    cache: &mut ViewCache,
) -> CellGrid {
    let mut grid = CellGrid::new(state.cols, state.rows);
    grid.clear(&state.default_face);
    render_pipeline_cached(state, registry, &mut grid, dirty, cache);
    grid
}

/// Compare two grids cell-by-cell, panicking with a descriptive message on mismatch.
fn assert_grids_equal(actual: &CellGrid, expected: &CellGrid, context: &str) {
    assert_eq!(
        actual.width(),
        expected.width(),
        "{context}: width mismatch"
    );
    assert_eq!(
        actual.height(),
        expected.height(),
        "{context}: height mismatch"
    );
    for y in 0..actual.height() {
        for x in 0..actual.width() {
            let a = actual.get(x, y).unwrap();
            let e = expected.get(x, y).unwrap();
            assert_eq!(
                a, e,
                "{context}: cell mismatch at ({x}, {y})\n  actual:   {:?}\n  expected: {:?}",
                a, e
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Oracle test: warm cache + single flag == fresh cache + ALL
// ---------------------------------------------------------------------------

/// For each atomic DirtyFlags value, verify that rendering with a warm cache
/// and only that flag produces the same output as a full render with ALL.
///
/// This is the core soundness invariant for incremental rendering.
#[test]
fn test_cache_soundness_all_flags_no_mutation() {
    let state = rich_state();
    let registry = PluginRegistry::new();

    let flags_to_test = [
        ("BUFFER", DirtyFlags::BUFFER),
        ("STATUS", DirtyFlags::STATUS),
        ("MENU_STRUCTURE", DirtyFlags::MENU_STRUCTURE),
        ("MENU_SELECTION", DirtyFlags::MENU_SELECTION),
        ("INFO", DirtyFlags::INFO),
        ("OPTIONS", DirtyFlags::OPTIONS),
    ];

    // Reference: fresh cache, ALL flags
    let ref_grid = render_to_grid(&state, &registry, DirtyFlags::ALL, &mut ViewCache::new());

    for (name, flag) in &flags_to_test {
        // Warm cache by rendering with ALL first
        let mut cache = ViewCache::new();
        let _ = render_to_grid(&state, &registry, DirtyFlags::ALL, &mut cache);

        // Now render with only this single flag on the warm cache
        let test_grid = render_to_grid(&state, &registry, *flag, &mut cache);

        assert_grids_equal(&test_grid, &ref_grid, &format!("no-mutation flag={name}"));
    }
}

/// For each atomic DirtyFlags value, verify soundness after a state mutation
/// that triggers that flag.
///
/// Flow:
/// 1. Render initial state with ALL → warms the cache
/// 2. Mutate state in a way that triggers flag F
/// 3. Render mutated state with flag F using warm cache → test grid
/// 4. Render mutated state with ALL using fresh cache → reference grid
/// 5. Assert test grid == reference grid
#[test]
fn test_cache_soundness_after_mutation() {
    let registry = PluginRegistry::new();

    // --- BUFFER ---
    {
        let mut state = rich_state();
        let mut cache = ViewCache::new();
        let _ = render_to_grid(&state, &registry, DirtyFlags::ALL, &mut cache);

        // Mutate: change buffer content
        state.lines = vec![
            make_line("let x = 42;"),
            make_line("let y = 100;"),
            make_line("x + y"),
        ];
        state.lines_dirty = vec![true, true, true];

        let test_grid = render_to_grid(&state, &registry, DirtyFlags::BUFFER, &mut cache);
        let ref_grid = render_to_grid(&state, &registry, DirtyFlags::ALL, &mut ViewCache::new());
        assert_grids_equal(&test_grid, &ref_grid, "mutation BUFFER");
    }

    // --- STATUS ---
    {
        let mut state = rich_state();
        let mut cache = ViewCache::new();
        let _ = render_to_grid(&state, &registry, DirtyFlags::ALL, &mut cache);

        // Mutate: change status line
        state.status_line = make_line(" buffer.rs [+] ");
        state.status_mode_line = make_line("insert");

        let test_grid = render_to_grid(&state, &registry, DirtyFlags::STATUS, &mut cache);
        let ref_grid = render_to_grid(&state, &registry, DirtyFlags::ALL, &mut ViewCache::new());
        assert_grids_equal(&test_grid, &ref_grid, "mutation STATUS");
    }

    // --- MENU_STRUCTURE ---
    {
        let mut state = rich_state();
        let mut cache = ViewCache::new();
        let _ = render_to_grid(&state, &registry, DirtyFlags::ALL, &mut cache);

        // Mutate: replace menu with different items
        state.apply(KakouneRequest::MenuShow {
            items: vec![
                make_line("alpha"),
                make_line("beta"),
                make_line("gamma"),
                make_line("delta"),
            ],
            anchor: Coord { line: 1, column: 4 },
            selected_item_face: Face {
                fg: Color::Named(NamedColor::Black),
                bg: Color::Named(NamedColor::Cyan),
                ..Face::default()
            },
            menu_face: Face {
                fg: Color::Named(NamedColor::White),
                bg: Color::Named(NamedColor::Blue),
                ..Face::default()
            },
            style: MenuStyle::Inline,
        });

        let test_grid = render_to_grid(&state, &registry, DirtyFlags::MENU_STRUCTURE, &mut cache);
        let ref_grid = render_to_grid(&state, &registry, DirtyFlags::ALL, &mut ViewCache::new());
        assert_grids_equal(&test_grid, &ref_grid, "mutation MENU_STRUCTURE");
    }

    // --- MENU_SELECTION ---
    {
        let mut state = rich_state();
        let mut cache = ViewCache::new();
        let _ = render_to_grid(&state, &registry, DirtyFlags::ALL, &mut cache);

        // Mutate: change selected menu item
        state.apply(KakouneRequest::MenuSelect { selected: 2 });

        let test_grid = render_to_grid(&state, &registry, DirtyFlags::MENU_SELECTION, &mut cache);
        let ref_grid = render_to_grid(&state, &registry, DirtyFlags::ALL, &mut ViewCache::new());
        assert_grids_equal(&test_grid, &ref_grid, "mutation MENU_SELECTION");
    }

    // --- INFO ---
    {
        let mut state = rich_state();
        let mut cache = ViewCache::new();
        let _ = render_to_grid(&state, &registry, DirtyFlags::ALL, &mut cache);

        // Mutate: add another info popup
        state.apply(KakouneRequest::InfoShow {
            title: make_line("Doc"),
            content: vec![make_line("fn doc() -> bool"), make_line("Returns true")],
            anchor: Coord { line: 2, column: 0 },
            face: Face::default(),
            style: InfoStyle::Modal,
        });

        let test_grid = render_to_grid(&state, &registry, DirtyFlags::INFO, &mut cache);
        let ref_grid = render_to_grid(&state, &registry, DirtyFlags::ALL, &mut ViewCache::new());
        assert_grids_equal(&test_grid, &ref_grid, "mutation INFO");
    }

    // --- OPTIONS ---
    {
        let mut state = rich_state();
        let mut cache = ViewCache::new();
        let _ = render_to_grid(&state, &registry, DirtyFlags::ALL, &mut cache);

        // Mutate: toggle options that affect rendering
        state.shadow_enabled = false;
        state.search_dropdown = true;

        let test_grid = render_to_grid(&state, &registry, DirtyFlags::OPTIONS, &mut cache);
        let ref_grid = render_to_grid(&state, &registry, DirtyFlags::ALL, &mut ViewCache::new());
        assert_grids_equal(&test_grid, &ref_grid, "mutation OPTIONS");
    }
}

/// Verify that OPTIONS changes correctly invalidate menu and info sections.
///
/// This specifically tests the bug fixed in Phase 1-B: `search_dropdown` and
/// `shadow_enabled` changes (dirty=OPTIONS) must invalidate the menu and info
/// caches respectively.
#[test]
fn test_options_invalidates_menu_and_info_sections() {
    let registry = PluginRegistry::new();

    // Test search_dropdown affecting menu
    {
        let mut state = rich_state();
        let mut cache = ViewCache::new();
        let _ = render_to_grid(&state, &registry, DirtyFlags::ALL, &mut cache);

        // Toggle search_dropdown (triggers OPTIONS flag via SetConfig)
        state.search_dropdown = !state.search_dropdown;

        let test_grid = render_to_grid(&state, &registry, DirtyFlags::OPTIONS, &mut cache);
        let ref_grid = render_to_grid(&state, &registry, DirtyFlags::ALL, &mut ViewCache::new());
        assert_grids_equal(
            &test_grid,
            &ref_grid,
            "OPTIONS→menu (search_dropdown toggle)",
        );
    }

    // Test shadow_enabled affecting info
    {
        let mut state = rich_state();
        let mut cache = ViewCache::new();
        let _ = render_to_grid(&state, &registry, DirtyFlags::ALL, &mut cache);

        // Toggle shadow_enabled (triggers OPTIONS flag via SetConfig)
        state.shadow_enabled = !state.shadow_enabled;

        let test_grid = render_to_grid(&state, &registry, DirtyFlags::OPTIONS, &mut cache);
        let ref_grid = render_to_grid(&state, &registry, DirtyFlags::ALL, &mut ViewCache::new());
        assert_grids_equal(
            &test_grid,
            &ref_grid,
            "OPTIONS→info (shadow_enabled toggle)",
        );
    }
}

/// Verify that combined dirty flags also produce correct output.
#[test]
fn test_cache_soundness_combined_flags() {
    let registry = PluginRegistry::new();
    let state = rich_state();

    let combos = [
        ("BUFFER|STATUS", DirtyFlags::BUFFER | DirtyFlags::STATUS),
        ("MENU", DirtyFlags::MENU),
        (
            "BUFFER|MENU_SELECTION",
            DirtyFlags::BUFFER | DirtyFlags::MENU_SELECTION,
        ),
        (
            "STATUS|INFO|OPTIONS",
            DirtyFlags::STATUS | DirtyFlags::INFO | DirtyFlags::OPTIONS,
        ),
    ];

    for (name, flags) in &combos {
        let mut cache = ViewCache::new();
        let _ = render_to_grid(&state, &registry, DirtyFlags::ALL, &mut cache);

        let test_grid = render_to_grid(&state, &registry, *flags, &mut cache);
        let ref_grid = render_to_grid(&state, &registry, DirtyFlags::ALL, &mut ViewCache::new());
        assert_grids_equal(&test_grid, &ref_grid, &format!("combined {name}"));
    }
}
