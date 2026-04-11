//! A8 (Inference Boundedness) property test for `Inference<'a>`.
//!
//! Axiom A8 from `docs/semantics.md` §2.5 states that, for any AppState `s`,
//! the inferred facts `i(s)` depend on `s` only through the pair
//! `(truth(s), policy(s))`. Equivalently, mutating any field that is NOT in
//! the `observed ∪ config` scope — i.e. any `session` or `runtime` field,
//! and any `derived`/`heuristic` field (which is itself downstream of
//! observed+config) — must not cause the inference projection to pick up
//! new information that was not already derivable.
//!
//! A fully dynamical witness (apply protocol messages, verify inference)
//! would require proptest generators over the protocol, which is out of
//! scope for this file. The witness here targets the *projection
//! independence* subset of A8:
//!
//! > Mutating `session` or `runtime` fields on an AppState must not change
//! > the values returned by any accessor on `Inference<'_>`, `Truth<'_>`,
//! > or `Policy<'_>`.
//!
//! This is the strongest property the projection types themselves can
//! witness without invoking `AppState::apply`. A stronger, dynamical form
//! of A8 is tracked for the trace-equivalence harness.

use std::collections::HashMap;

use kasane_core::config::MenuPosition;
use kasane_core::display::FoldToggleState;
use kasane_core::layout::HitMap;
use kasane_core::plugin::PluginId;
use kasane_core::plugin::setting::SettingValue;
use kasane_core::protocol::{Coord, CursorMode, Face, Line, StatusStyle};
use kasane_core::render::color_context::ColorContext;
use kasane_core::render::theme::Theme;
use kasane_core::state::derived::{EditorMode, Selection};
use kasane_core::state::{AppState, DragState};
use proptest::prelude::*;

#[derive(Debug, Clone, PartialEq)]
struct TruthSnapshot {
    lines: Vec<Line>,
    default_face: Face,
    padding_face: Face,
    widget_columns: u16,
    cursor_pos: Coord,
    status_prompt: Line,
    status_content: Line,
    status_content_cursor_pos: i32,
    status_mode_line: Line,
    status_default_face: Face,
    status_style: StatusStyle,
    // MenuState / InfoState do not implement PartialEq, so we compare their
    // Debug formatting instead. This is sufficient for the A8 projection
    // test, which only asserts that repeated reads produce identical output.
    menu_debug: String,
    infos_debug: String,
    ui_options: HashMap<String, String>,
}

fn truth_snapshot(s: &AppState) -> TruthSnapshot {
    let t = s.truth();
    TruthSnapshot {
        lines: t.lines().to_vec(),
        default_face: t.default_face(),
        padding_face: t.padding_face(),
        widget_columns: t.widget_columns(),
        cursor_pos: t.cursor_pos(),
        status_prompt: t.status_prompt().clone(),
        status_content: t.status_content().clone(),
        status_content_cursor_pos: t.status_content_cursor_pos(),
        status_mode_line: t.status_mode_line().clone(),
        status_default_face: t.status_default_face(),
        status_style: t.status_style(),
        menu_debug: format!("{:?}", t.menu()),
        infos_debug: format!("{:?}", t.infos()),
        ui_options: t.ui_options().clone(),
    }
}

#[derive(Debug, Clone, PartialEq)]
struct InferenceSnapshot {
    lines_dirty: Vec<bool>,
    cursor_mode: CursorMode,
    status_line: Line,
    editor_mode: EditorMode,
    color_context: ColorContext,
    cursor_count: usize,
    secondary_cursors: Vec<Coord>,
    selections: Vec<Selection>,
}

fn inference_snapshot(s: &AppState) -> InferenceSnapshot {
    let i = s.inference();
    InferenceSnapshot {
        lines_dirty: i.lines_dirty().to_vec(),
        cursor_mode: i.cursor_mode(),
        status_line: i.status_line().clone(),
        editor_mode: i.editor_mode(),
        color_context: i.color_context().clone(),
        cursor_count: i.cursor_count(),
        secondary_cursors: i.secondary_cursors().to_vec(),
        selections: i.selections().to_vec(),
    }
}

#[derive(Debug, Clone, PartialEq)]
struct PolicySnapshot {
    shadow_enabled: bool,
    padding_char: String,
    secondary_blend_ratio: f32,
    theme: Theme,
    status_at_top: bool,
    menu_max_height: u16,
    menu_position: MenuPosition,
    search_dropdown: bool,
    scrollbar_thumb: String,
    scrollbar_track: String,
    assistant_art: Option<Vec<String>>,
    plugin_config: HashMap<String, String>,
    plugin_settings: HashMap<PluginId, HashMap<String, SettingValue>>,
    fold_toggle_state: FoldToggleState,
}

fn policy_snapshot(s: &AppState) -> PolicySnapshot {
    let p = s.policy();
    PolicySnapshot {
        shadow_enabled: p.shadow_enabled(),
        padding_char: p.padding_char().to_string(),
        secondary_blend_ratio: p.secondary_blend_ratio(),
        theme: p.theme().clone(),
        status_at_top: p.status_at_top(),
        menu_max_height: p.menu_max_height(),
        menu_position: p.menu_position(),
        search_dropdown: p.search_dropdown(),
        scrollbar_thumb: p.scrollbar_thumb().to_string(),
        scrollbar_track: p.scrollbar_track().to_string(),
        assistant_art: p.assistant_art().map(|v| v.to_vec()),
        plugin_config: p.plugin_config().clone(),
        plugin_settings: p.plugin_settings().clone(),
        fold_toggle_state: p.fold_toggle_state().clone(),
    }
}

/// Strategy: arbitrary values for every session + runtime field on
/// `AppState`. These are the fields A8 says inference cannot depend on.
fn arb_session_and_runtime_mutation() -> impl Strategy<
    Value = (
        bool,           // focused
        bool,           // drag Active vs None
        u16,            // cols
        u16,            // rows
        usize,          // display_scroll_offset
        Option<String>, // active_session_key
    ),
> {
    (
        any::<bool>(),
        any::<bool>(),
        any::<u16>(),
        any::<u16>(),
        any::<usize>(),
        proptest::option::of("[a-z]{1,8}".prop_map(|s| s.to_string())),
    )
}

fn apply_mutation(
    state: &mut AppState,
    (focused, drag_active, cols, rows, scroll, session_key): (
        bool,
        bool,
        u16,
        u16,
        usize,
        Option<String>,
    ),
) {
    state.focused = focused;
    state.drag = if drag_active {
        DragState::Active {
            button: kasane_core::input::MouseButton::Left,
            start_line: 0,
            start_column: 0,
        }
    } else {
        DragState::None
    };
    state.cols = cols;
    state.rows = rows;
    state.hit_map = HitMap::new();
    state.display_scroll_offset = scroll;
    state.active_session_key = session_key;
}

/// Baseline state with non-trivial protocol, inference, and policy
/// contents, so the mutation test isn't operating on default zeros.
fn baseline_state() -> AppState {
    let mut s = AppState::default();
    s.cursor_pos = Coord { line: 2, column: 5 };
    s.widget_columns = 3;
    s.padding_char = "·".to_string();
    s.menu_max_height = 17;
    s.shadow_enabled = false;
    s.cursor_count = 2;
    s.secondary_cursors = vec![Coord { line: 4, column: 1 }];
    s
}

proptest! {
    #[test]
    fn mutating_session_or_runtime_preserves_truth_inference_policy(
        mutation in arb_session_and_runtime_mutation(),
    ) {
        let baseline = baseline_state();
        let before_truth = truth_snapshot(&baseline);
        let before_inference = inference_snapshot(&baseline);
        let before_policy = policy_snapshot(&baseline);

        let mut mutated = baseline.clone();
        apply_mutation(&mut mutated, mutation);

        prop_assert_eq!(truth_snapshot(&mutated), before_truth);
        prop_assert_eq!(inference_snapshot(&mutated), before_inference);
        prop_assert_eq!(policy_snapshot(&mutated), before_policy);
    }
}
