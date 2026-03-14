//! Integration tests for the plugin system:
//!   `#[kasane_plugin]` macro → PluginRegistry → view → layout → paint → CellGrid
//!
//! These tests verify the end-to-end plugin pipeline, covering:
//! Lifecycle, Input, Event/Message, and MenuTransform.

use kasane_core::input::{Key, KeyEvent, Modifiers};
use kasane_core::kasane_plugin;
use kasane_core::plugin::{Command, PluginRegistry};
use kasane_core::protocol::{Color, Coord, Face, Line, MenuStyle, NamedColor};
use kasane_core::state::{AppState, DirtyFlags, Msg, update};
use kasane_core::test_support::{make_line, render_with_registry, row_text};

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn setup_state(lines: Vec<Line>) -> AppState {
    let mut state = kasane_core::test_support::test_state_80x24();
    state.lines = lines;
    state.status_default_face = state.default_face;
    state.status_line = make_line(" main.rs ");
    state.status_mode_line = make_line("normal");
    state
}

// ===========================================================================
// Test 1: handle_key first-wins
// ===========================================================================

#[kasane_plugin]
mod key_consumer_plugin {
    use kasane_core::input::KeyEvent;
    use kasane_core::plugin::Command;
    use kasane_core::state::{AppState, DirtyFlags};

    #[state]
    #[derive(Default)]
    pub struct State;

    pub fn handle_key(
        _state: &mut State,
        key: &KeyEvent,
        _core: &AppState,
    ) -> Option<Vec<Command>> {
        // Consume Ctrl+S
        if key.key == kasane_core::input::Key::Char('s')
            && key.modifiers.contains(kasane_core::input::Modifiers::CTRL)
        {
            Some(vec![Command::RequestRedraw(DirtyFlags::ALL)])
        } else {
            None
        }
    }
}

#[test]
fn handle_key_first_wins() {
    let mut state = setup_state(vec![make_line("text")]);
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(KeyConsumerPluginPlugin::new()));
    registry.init_all(&state);

    // Case 1: Ctrl+S should be consumed by the plugin
    let ctrl_s = KeyEvent {
        key: Key::Char('s'),
        modifiers: Modifiers::CTRL,
    };
    let (flags, cmds) = update(&mut state, Msg::Key(ctrl_s), &mut registry, 3);

    // Plugin returns RequestRedraw(ALL) → extracted into flags
    assert!(
        flags.contains(DirtyFlags::ALL),
        "Ctrl+S should produce ALL dirty flags from plugin"
    );
    // No SendToKakoune command (plugin consumed the key)
    let has_send = cmds.iter().any(|c| matches!(c, Command::SendToKakoune(_)));
    assert!(
        !has_send,
        "Ctrl+S should NOT produce SendToKakoune (plugin consumed it)"
    );

    // Case 2: regular key 'a' should pass through to Kakoune
    let key_a = KeyEvent {
        key: Key::Char('a'),
        modifiers: Modifiers::empty(),
    };
    let (_flags, cmds) = update(&mut state, Msg::Key(key_a), &mut registry, 3);

    let has_send = cmds.iter().any(|c| matches!(c, Command::SendToKakoune(_)));
    assert!(
        has_send,
        "regular key 'a' should produce SendToKakoune (plugin did not consume it)"
    );
}

// ===========================================================================
// Test 2: Plugin message delivery
// ===========================================================================

#[kasane_plugin]
mod msg_receiver_plugin {
    use kasane_core::plugin::Command;
    use kasane_core::state::{AppState, DirtyFlags};

    #[state]
    #[derive(Default)]
    pub struct State {
        pub value: u32,
    }

    #[event]
    pub enum Msg {
        SetValue(u32),
    }

    pub fn update(state: &mut State, msg: Msg, _core: &AppState) -> Vec<Command> {
        match msg {
            Msg::SetValue(v) => {
                state.value = v;
                vec![Command::RequestRedraw(DirtyFlags::STATUS)]
            }
        }
    }
}

#[test]
fn plugin_message_delivery() {
    let state = setup_state(vec![make_line("text")]);

    let mut registry = PluginRegistry::new();
    registry.register(Box::new(MsgReceiverPluginPlugin::new()));
    registry.init_all(&state);

    let target_id = kasane_core::plugin::PluginId("msg_receiver_plugin".into());
    let payload: Box<dyn std::any::Any> = Box::new(msg_receiver_plugin::Msg::SetValue(42));
    let (flags, cmds) = registry.deliver_message(&target_id, payload, &state);

    // Assertion 1: RequestRedraw(STATUS) is extracted into flags
    assert!(
        flags.contains(DirtyFlags::STATUS),
        "deliver_message should return STATUS flag, got: {flags:?}"
    );

    // Assertion 2: commands are empty (RequestRedraw was extracted)
    assert!(
        cmds.is_empty(),
        "commands should be empty after extracting RequestRedraw, got {} commands",
        cmds.len()
    );
}

// ===========================================================================
// Test 3: Menu transform adds prefix
// ===========================================================================

#[kasane_plugin]
mod prefix_plugin {
    use kasane_core::protocol::{Atom, Face};
    use kasane_core::state::AppState;

    #[state]
    #[derive(Default)]
    pub struct State;

    pub fn transform_menu_item(
        _state: &State,
        item: &[Atom],
        _index: usize,
        _selected: bool,
        _core: &AppState,
    ) -> Option<Vec<Atom>> {
        let mut result = vec![Atom {
            face: Face::default(),
            contents: ">> ".into(),
        }];
        result.extend(item.iter().cloned());
        Some(result)
    }
}

#[test]
fn menu_transform_adds_prefix() {
    use kasane_core::protocol::KakouneRequest;

    let mut state = setup_state(vec![make_line("fn main() {}")]);
    state.cursor_pos = Coord { line: 0, column: 3 };

    // Show inline menu with items
    let items = vec![make_line("alpha"), make_line("beta")];
    state.apply(KakouneRequest::MenuShow {
        items,
        anchor: Coord { line: 0, column: 3 },
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

    let mut registry = PluginRegistry::new();
    registry.register(Box::new(PrefixPluginPlugin::new()));
    registry.init_all(&state);
    registry.prepare_plugin_cache(DirtyFlags::ALL);

    let grid = render_with_registry(&state, &registry);

    // The menu window may truncate items, so check for the prefix ">> " rather than full text.
    let mut found_prefix = false;
    for y in 0..grid.height() {
        let text = row_text(&grid, y);
        if text.contains(">> ") {
            found_prefix = true;
            break;
        }
    }
    assert!(found_prefix, "menu should show items with '>> ' prefix");

    // Also verify via the registry API directly that the transform is applied
    let item = vec![kasane_core::protocol::Atom {
        face: Face::default(),
        contents: "alpha".into(),
    }];
    let transformed = registry.transform_menu_item(&item, 0, false, &state);
    assert!(transformed.is_some(), "transform should return Some");
    let transformed = transformed.unwrap();
    assert_eq!(
        transformed[0].contents.as_str(),
        ">> ",
        "first atom should be the prefix"
    );
    assert_eq!(
        transformed[1].contents.as_str(),
        "alpha",
        "second atom should be the original item"
    );
}
