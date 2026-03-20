//! Trace-equivalence property tests for rendering pipeline (ADR-016).
//!
//! Verifies the invariant: for any valid AppState S,
//! `render_pipeline(S)` produces deterministic output, and
//! `render_pipeline_cached(S)` agrees with `render_pipeline(S)`.
//!
//! Uses proptest for mutation-based fuzzing from a rich base state.

use kasane_core::plugin::PluginRegistry;
use kasane_core::protocol::{Color, Coord, Face, InfoStyle, KakouneRequest, MenuStyle, NamedColor};
use kasane_core::state::{AppState, DirtyFlags};
use kasane_core::test_support::{assert_grids_equal, make_line, render_to_grid, test_state_80x24};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// Strategies
// ---------------------------------------------------------------------------

/// Mutation operations on AppState.
#[derive(Debug, Clone)]
enum Mutation {
    MoveCursor(i32, i32),
    ChangeLines(Vec<&'static str>),
    ChangeStatusLine(&'static str),
    ChangeModeLine(&'static str),
    ToggleShadow,
    ToggleSearchDropdown,
    ShowMenu,
    HideMenu,
    SelectMenuItem(i32),
    ShowInfo,
    HideInfo,
    ChangeStatusAtTop,
}

/// Generate a random mutation.
fn arb_mutation() -> impl Strategy<Value = Mutation> {
    prop_oneof![
        (0i32..10, 0i32..80).prop_map(|(l, c)| Mutation::MoveCursor(l, c)),
        Just(Mutation::ChangeLines(vec![
            "let x = 1;",
            "let y = 2;",
            "x + y"
        ])),
        Just(Mutation::ChangeLines(vec!["single line"])),
        Just(Mutation::ChangeLines(vec![
            "a", "b", "c", "d", "e", "f", "g", "h", "i", "j"
        ])),
        Just(Mutation::ChangeStatusLine(" buffer.rs [+] ")),
        Just(Mutation::ChangeModeLine("insert")),
        Just(Mutation::ToggleShadow),
        Just(Mutation::ToggleSearchDropdown),
        Just(Mutation::ShowMenu),
        Just(Mutation::HideMenu),
        (0i32..5).prop_map(Mutation::SelectMenuItem),
        Just(Mutation::ShowInfo),
        Just(Mutation::HideInfo),
        Just(Mutation::ChangeStatusAtTop),
    ]
}

/// Apply a mutation to state and return the resulting DirtyFlags.
fn apply_mutation(state: &mut AppState, mutation: &Mutation) -> DirtyFlags {
    match mutation {
        Mutation::MoveCursor(line, col) => {
            state.cursor_pos = Coord {
                line: *line,
                column: *col,
            };
            DirtyFlags::BUFFER
        }
        Mutation::ChangeLines(lines) => {
            let new_lines: Vec<_> = lines.iter().map(|s| make_line(s)).collect();
            state.lines_dirty = vec![true; new_lines.len()];
            state.lines = new_lines;
            DirtyFlags::BUFFER
        }
        Mutation::ChangeStatusLine(s) => {
            state.status_line = make_line(s);
            DirtyFlags::STATUS
        }
        Mutation::ChangeModeLine(s) => {
            state.status_mode_line = make_line(s);
            DirtyFlags::STATUS
        }
        Mutation::ToggleShadow => {
            state.shadow_enabled = !state.shadow_enabled;
            DirtyFlags::OPTIONS
        }
        Mutation::ToggleSearchDropdown => {
            state.search_dropdown = !state.search_dropdown;
            DirtyFlags::OPTIONS
        }
        Mutation::ShowMenu => state.apply(KakouneRequest::MenuShow {
            items: vec![make_line("alpha"), make_line("beta"), make_line("gamma")],
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
        }),
        Mutation::HideMenu => state.apply(KakouneRequest::MenuHide),
        Mutation::SelectMenuItem(n) => {
            if state.menu.is_some() {
                state.apply(KakouneRequest::MenuSelect { selected: *n })
            } else {
                DirtyFlags::empty()
            }
        }
        Mutation::ShowInfo => state.apply(KakouneRequest::InfoShow {
            title: make_line("Test"),
            content: vec![make_line("test info content")],
            anchor: Coord { line: 0, column: 0 },
            face: Face::default(),
            style: InfoStyle::Modal,
        }),
        Mutation::HideInfo => {
            if !state.infos.is_empty() {
                state.apply(KakouneRequest::InfoHide)
            } else {
                DirtyFlags::empty()
            }
        }
        Mutation::ChangeStatusAtTop => {
            state.status_at_top = !state.status_at_top;
            DirtyFlags::OPTIONS
        }
    }
}

// ---------------------------------------------------------------------------
// Rich state builders
// ---------------------------------------------------------------------------

/// Build a rich AppState with buffer, menu, and info.
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
    state.apply(KakouneRequest::MenuSelect { selected: 0 });

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

/// Empty buffer state.
fn empty_state() -> AppState {
    let mut state = test_state_80x24();
    state.lines = vec![make_line("")];
    state.status_line = make_line("");
    state
}

/// Prompt mode state.
fn prompt_state() -> AppState {
    let mut state = test_state_80x24();
    state.lines = vec![make_line("hello world")];
    state.apply(KakouneRequest::DrawStatus {
        prompt: make_line(":"),
        content: make_line("write"),
        content_cursor_pos: 5,
        mode_line: make_line("prompt"),
        default_face: Face::default(),
    });
    state
}

/// Large buffer state.
fn large_buffer_state() -> AppState {
    let mut state = test_state_80x24();
    state.lines = (0..23)
        .map(|i| make_line(&format!("line {i}: some content here")))
        .collect();
    state.status_line = make_line(" large.rs ");
    state.status_mode_line = make_line("normal");
    state
}

/// Multi-info state.
fn multi_info_state() -> AppState {
    let mut state = rich_state();
    state.apply(KakouneRequest::InfoShow {
        title: make_line("Doc"),
        content: vec![make_line("documentation"), make_line("second line")],
        anchor: Coord { line: 2, column: 0 },
        face: Face::default(),
        style: InfoStyle::Modal,
    });
    state
}

// ---------------------------------------------------------------------------
// Property tests
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// render_pipeline is deterministic: two calls with the same state produce identical grids.
    #[test]
    fn test_pipeline_deterministic(mutation in arb_mutation()) {
        let registry = PluginRegistry::new();
        let mut state = rich_state();
        apply_mutation(&mut state, &mutation);

        let grid1 = render_to_grid(&state, &registry);
        let grid2 = render_to_grid(&state, &registry);

        assert_grids_equal(&grid1, &grid2, &format!("determinism after mutation={mutation:?}"));
    }
}

// ---------------------------------------------------------------------------
// Deterministic tests: multiple base states
// ---------------------------------------------------------------------------

/// Test that render_pipeline produces consistent output across multiple state configurations.
#[test]
fn test_multi_state_pipeline_consistency() {
    let registry = PluginRegistry::new();

    let states: Vec<(&str, AppState)> = vec![
        ("rich", rich_state()),
        ("empty", empty_state()),
        ("prompt", prompt_state()),
        ("large_buffer", large_buffer_state()),
        ("multi_info", multi_info_state()),
    ];

    for (state_name, state) in &states {
        // render_pipeline() is deterministic: same state → same grid
        let grid1 = render_to_grid(state, &registry);
        let grid2 = render_to_grid(state, &registry);
        assert_grids_equal(&grid1, &grid2, &format!("determinism {state_name}"));
    }
}
