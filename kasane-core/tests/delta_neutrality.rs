#![allow(clippy::field_reassign_with_default)]
//! Axiom A9 (Delta Neutrality) property test.
//!
//! From `docs/semantics.md` §2.7:
//!
//! > **A9 (Delta Neutrality)**: For any message `m` that is not a Kakoune
//! > protocol message, the projection `p : AppState → KakouneProtocolFacts`
//! > commutes with `update`, i.e. `p(update(s, m)) = p(s)`.
//!
//! In other words: **non-protocol messages must not mutate observed state.**
//! Only `Msg::Kakoune(...)` messages are allowed to change the projection
//! [`Truth`]; everything else (keys, text, mouse, resize, focus) can only
//! schedule side effects.
//!
//! This test witnesses A9 at runtime via mutation fuzzing. See ADR-030 for
//! the enforcement plan and `kasane-core/src/state/truth.rs` for the
//! projection type.

use kasane_core::input::{Key, KeyEvent, Modifiers, MouseEvent, MouseEventKind};
use kasane_core::plugin::PluginRuntime;
use kasane_core::protocol::{Coord, Face, Line, StatusStyle};
use kasane_core::state::{AppState, Msg, Truth, update_in_place};
use kasane_core::test_support::{make_line, test_state_80x24};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// ObservedSnapshot — owned clone of every field exposed by Truth.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
struct ObservedSnapshot {
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
    /// MenuState does not implement PartialEq, so we compare on its `Debug`
    /// representation — sufficient as a structural witness.
    menu_debug: String,
    /// InfoState does not implement PartialEq; same treatment as `menu_debug`.
    infos_debug: String,
    ui_options: std::collections::HashMap<String, String>,
}

impl ObservedSnapshot {
    fn capture(truth: Truth<'_>) -> Self {
        Self {
            lines: truth.lines().to_vec(),
            default_face: truth.default_face(),
            padding_face: truth.padding_face(),
            widget_columns: truth.widget_columns(),
            cursor_pos: truth.cursor_pos(),
            status_prompt: truth.status_prompt().clone(),
            status_content: truth.status_content().clone(),
            status_content_cursor_pos: truth.status_content_cursor_pos(),
            status_mode_line: truth.status_mode_line().clone(),
            status_default_face: truth.status_default_face(),
            status_style: truth.status_style(),
            menu_debug: format!("{:?}", truth.menu()),
            infos_debug: format!("{:?}", truth.infos()),
            ui_options: truth.ui_options().clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// NonProtocolMsgSpec — a Debug-able description of a non-protocol message.
//
// `Msg` itself is not `Debug` (it carries non-Debug payloads), so proptest
// cannot display shrunken counter-examples from a `Strategy<Value = Msg>`.
// We generate this spec instead and lower it to `Msg` inside the test body.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum NonProtocolMsgSpec {
    Key(Key),
    TextInput(String),
    Mouse { line: u32, column: u32 },
    FocusGained,
    FocusLost,
    Resize { cols: u16, rows: u16 },
    ClipboardPaste,
}

impl NonProtocolMsgSpec {
    fn into_msg(self) -> Msg {
        match self {
            NonProtocolMsgSpec::Key(key) => Msg::Key(KeyEvent {
                key,
                modifiers: Modifiers::empty(),
            }),
            NonProtocolMsgSpec::TextInput(text) => Msg::TextInput(text),
            NonProtocolMsgSpec::Mouse { line, column } => Msg::Mouse(MouseEvent {
                kind: MouseEventKind::Move,
                line,
                column,
                modifiers: Modifiers::empty(),
            }),
            NonProtocolMsgSpec::FocusGained => Msg::FocusGained,
            NonProtocolMsgSpec::FocusLost => Msg::FocusLost,
            NonProtocolMsgSpec::Resize { cols, rows } => Msg::Resize { cols, rows },
            NonProtocolMsgSpec::ClipboardPaste => Msg::ClipboardPaste,
        }
    }
}

fn arb_non_protocol_msg_spec() -> impl Strategy<Value = NonProtocolMsgSpec> {
    prop_oneof![
        Just(NonProtocolMsgSpec::Key(Key::Char('a'))),
        Just(NonProtocolMsgSpec::Key(Key::Char('Z'))),
        Just(NonProtocolMsgSpec::Key(Key::Enter)),
        Just(NonProtocolMsgSpec::Key(Key::Escape)),
        Just(NonProtocolMsgSpec::Key(Key::Backspace)),
        Just(NonProtocolMsgSpec::Key(Key::Up)),
        Just(NonProtocolMsgSpec::Key(Key::Down)),
        Just(NonProtocolMsgSpec::Key(Key::Left)),
        Just(NonProtocolMsgSpec::Key(Key::Right)),
        Just(NonProtocolMsgSpec::TextInput(String::new())),
        Just(NonProtocolMsgSpec::TextInput("hello".to_string())),
        Just(NonProtocolMsgSpec::TextInput("a".to_string())),
        (0u32..24, 0u32..80).prop_map(|(line, column)| NonProtocolMsgSpec::Mouse { line, column }),
        Just(NonProtocolMsgSpec::FocusGained),
        Just(NonProtocolMsgSpec::FocusLost),
        (40u16..200, 10u16..60).prop_map(|(cols, rows)| NonProtocolMsgSpec::Resize { cols, rows }),
        Just(NonProtocolMsgSpec::ClipboardPaste),
    ]
}

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

fn rich_state() -> Box<AppState> {
    let mut state = Box::new(test_state_80x24());
    state.observed.lines = vec![make_line("hello"), make_line("world"), make_line("!")];
    state.inference.lines_dirty = vec![true; 3];
    state.observed.cursor_pos = Coord { line: 1, column: 2 };
    state.observed.status_prompt = make_line(":");
    state.observed.status_content = make_line("edit");
    state.observed.status_content_cursor_pos = 4;
    state.observed.status_mode_line = make_line("insert");
    state.observed.widget_columns = 5;
    state
}

// ---------------------------------------------------------------------------
// Property: A9 Delta Neutrality.
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 256,
        .. ProptestConfig::default()
    })]

    /// For every non-protocol message `m`,
    /// `Truth(update(s, m)) == Truth(s)` structurally.
    #[test]
    fn non_protocol_messages_preserve_truth(
        spec in arb_non_protocol_msg_spec(),
    ) {
        let mut state = rich_state();
        let mut registry = PluginRuntime::new();

        let before = ObservedSnapshot::capture(state.truth());
        let _ = update_in_place(&mut state, spec.into_msg(), &mut registry, 3);
        let after = ObservedSnapshot::capture(state.truth());

        prop_assert_eq!(
            before,
            after,
            "A9 Delta Neutrality violated: non-protocol message mutated observed state",
        );
    }
}
