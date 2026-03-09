//! Integration tests for the plugin system:
//!   `#[kasane_plugin]` macro → PluginRegistry → view → layout → paint → CellGrid
//!
//! These tests verify the end-to-end plugin pipeline, covering all extension points:
//! Slot, Decorator, Replacement, Overlay, LineDecoration, Lifecycle, Input, Event/Message,
//! and MenuTransform.

use kasane_core::input::{Key, KeyEvent, Modifiers};
use kasane_core::kasane_plugin;
use kasane_core::layout::Rect;
use kasane_core::layout::flex::place;
use kasane_core::plugin::{Command, PluginRegistry, Slot};
use kasane_core::protocol::{Color, Coord, Face, Line, MenuStyle, NamedColor};
use kasane_core::render::CellGrid;
use kasane_core::render::paint;
use kasane_core::render::view;
use kasane_core::state::{AppState, DirtyFlags, Msg, update};
use kasane_core::test_support::make_line;

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

fn render_with_registry(state: &AppState, registry: &PluginRegistry) -> CellGrid {
    let element = view::view(state, registry);
    let root = Rect {
        x: 0,
        y: 0,
        w: state.cols,
        h: state.rows,
    };
    let layout = place(&element, root, state);
    let mut grid = CellGrid::new(state.cols, state.rows);
    grid.clear(&state.default_face);
    paint::paint(&element, &layout, &mut grid, state);
    grid
}

fn row_text(grid: &CellGrid, y: u16) -> String {
    let mut s = String::new();
    for x in 0..grid.width() {
        if let Some(cell) = grid.get(x, y)
            && cell.width > 0
        {
            s.push_str(&cell.grapheme);
        }
    }
    s.trim_end().to_string()
}

// ===========================================================================
// Test 1: Multi-Extension-Point E2E
// ===========================================================================

#[kasane_plugin]
mod multi_ext_plugin {
    use kasane_core::element::Element;
    use kasane_core::plugin::{Command, LineDecoration};
    use kasane_core::protocol::{Color, Face};
    use kasane_core::state::{AppState, DirtyFlags};

    #[state]
    #[derive(Default)]
    pub struct State {
        pub active_line: i32,
    }

    pub fn on_state_changed(state: &mut State, core: &AppState, dirty: DirtyFlags) -> Vec<Command> {
        if dirty.intersects(DirtyFlags::BUFFER) {
            state.active_line = core.cursor_pos.line;
        }
        vec![]
    }

    #[slot(Slot::BufferLeft)]
    pub fn view(_state: &State, core: &AppState) -> Option<Element> {
        let count = core.lines.len();
        Some(Element::text(format!("{count}L"), Face::default()))
    }

    pub fn contribute_line(state: &State, line: usize, _core: &AppState) -> Option<LineDecoration> {
        if line == state.active_line as usize {
            Some(LineDecoration {
                left_gutter: None,
                right_gutter: None,
                background: Some(Face {
                    bg: Color::Rgb {
                        r: 50,
                        g: 50,
                        b: 60,
                    },
                    ..Face::default()
                }),
            })
        } else {
            None
        }
    }
}

#[test]
fn multi_extension_plugin_e2e() {
    let mut state = setup_state(vec![
        make_line("first line"),
        make_line("second line"),
        make_line("third line"),
    ]);
    state.cursor_pos = Coord { line: 1, column: 0 };

    let mut registry = PluginRegistry::new();
    registry.register(Box::new(MultiExtPluginPlugin::new()));
    registry.init_all(&state);

    // Simulate state change notification (as update() would do)
    for plugin in registry.plugins_mut() {
        plugin.on_state_changed(&state, DirtyFlags::BUFFER);
    }
    registry.prepare_plugin_cache(DirtyFlags::BUFFER);

    // Assertion 1: Slot contribute produces the expected element via the macro-generated path
    let slot_elements = registry.collect_slot(Slot::BufferLeft, &state);
    assert_eq!(
        slot_elements.len(),
        1,
        "BufferLeft slot should have 1 contributed element"
    );
    // Verify the element reaches the view pipeline (gutter column is allocated)
    let grid = render_with_registry(&state, &registry);
    let r0 = row_text(&grid, 0);
    assert!(
        r0.contains("first line"),
        "row 0 should contain buffer text, got: {r0:?}"
    );

    // Assertion 2: active_line (row 1) has Rgb(50,50,60) background from contribute_line
    let target_bg = Color::Rgb {
        r: 50,
        g: 50,
        b: 60,
    };
    let mut found_active_bg = false;
    for x in 0..grid.width() {
        if let Some(cell) = grid.get(x, 1)
            && cell.face.bg == target_bg
        {
            found_active_bg = true;
            break;
        }
    }
    assert!(
        found_active_bg,
        "active line (row 1) should have Rgb(50,50,60) background"
    );

    // Assertion 3: non-active line (row 0) does NOT have that background
    let mut found_non_active_bg = false;
    for x in 0..grid.width() {
        if let Some(cell) = grid.get(x, 0)
            && cell.face.bg == target_bg
        {
            found_non_active_bg = true;
            break;
        }
    }
    assert!(
        !found_non_active_bg,
        "non-active line (row 0) should NOT have Rgb(50,50,60) background"
    );
}

// ===========================================================================
// Test 2: Decorator wraps buffer
// ===========================================================================

#[kasane_plugin]
mod border_deco_plugin {
    use kasane_core::element::{BorderConfig, BorderLineStyle, Element};
    use kasane_core::state::AppState;

    #[state]
    #[derive(Default)]
    pub struct State;

    #[decorate(DecorateTarget::Buffer, priority = 10)]
    pub fn decorate(_state: &State, element: Element, _core: &AppState) -> Element {
        Element::Container {
            child: Box::new(element),
            border: Some(BorderConfig::new(BorderLineStyle::Rounded)),
            shadow: false,
            padding: kasane_core::element::Edges::ZERO,
            style: kasane_core::element::Style::Direct(kasane_core::protocol::Face::default()),
            title: None,
        }
    }
}

#[test]
fn decorator_wraps_buffer() {
    let state = setup_state(vec![make_line("hello")]);

    let mut registry = PluginRegistry::new();
    registry.register(Box::new(BorderDecoPluginPlugin::new()));
    registry.init_all(&state);
    registry.prepare_plugin_cache(DirtyFlags::ALL);

    let grid = render_with_registry(&state, &registry);

    // Assertion 1: row 0 contains rounded border character "╭"
    let r0 = row_text(&grid, 0);
    assert!(
        r0.contains('╭'),
        "row 0 should contain rounded border '╭', got: {r0:?}"
    );

    // Assertion 2: "hello" still appears somewhere in the grid
    let mut found_hello = false;
    for y in 0..grid.height() {
        if row_text(&grid, y).contains("hello") {
            found_hello = true;
            break;
        }
    }
    assert!(found_hello, "buffer text 'hello' should still be visible");
}

// ===========================================================================
// Test 3: Replacement replaces status bar
// ===========================================================================

#[kasane_plugin]
mod custom_status_plugin {
    use kasane_core::element::Element;
    use kasane_core::protocol::Face;
    use kasane_core::state::AppState;

    #[state]
    #[derive(Default)]
    pub struct State;

    #[replace(ReplaceTarget::StatusBar)]
    pub fn replace(_state: &State, _core: &AppState) -> Option<Element> {
        Some(Element::text("CUSTOM-STATUS", Face::default()))
    }
}

#[test]
fn replacement_replaces_status_bar() {
    let state = setup_state(vec![make_line("buffer content")]);

    let mut registry = PluginRegistry::new();
    registry.register(Box::new(CustomStatusPluginPlugin::new()));
    registry.init_all(&state);
    registry.prepare_plugin_cache(DirtyFlags::ALL);

    let grid = render_with_registry(&state, &registry);

    // Status bar is at row 23 (last row of 24-row terminal)
    let status = row_text(&grid, 23);

    // Assertion 1: custom status text appears
    assert!(
        status.contains("CUSTOM-STATUS"),
        "status bar should contain 'CUSTOM-STATUS', got: {status:?}"
    );

    // Assertion 2: built-in status text is gone
    assert!(
        !status.contains("main.rs"),
        "status bar should NOT contain 'main.rs' (replaced), got: {status:?}"
    );
}

// ===========================================================================
// Test 4: Overlay slot renders
// ===========================================================================

#[kasane_plugin]
mod overlay_slot_plugin {
    use kasane_core::element::Element;
    use kasane_core::protocol::Face;
    use kasane_core::state::AppState;

    #[state]
    #[derive(Default)]
    pub struct State;

    #[slot(Slot::Overlay)]
    pub fn view(_state: &State, _core: &AppState) -> Option<Element> {
        Some(Element::text("OVERLAY-TEXT", Face::default()))
    }
}

#[test]
fn overlay_slot_renders() {
    let state = setup_state(vec![make_line("buffer line")]);

    let mut registry = PluginRegistry::new();
    registry.register(Box::new(OverlaySlotPluginPlugin::new()));
    registry.init_all(&state);
    registry.prepare_plugin_cache(DirtyFlags::ALL);

    let grid = render_with_registry(&state, &registry);

    // Assertion: overlay text appears somewhere in the grid
    let mut found = false;
    for y in 0..grid.height() {
        if row_text(&grid, y).contains("OVERLAY-TEXT") {
            found = true;
            break;
        }
    }
    assert!(
        found,
        "overlay text 'OVERLAY-TEXT' should be visible in the grid"
    );
}

// ===========================================================================
// Test 5: handle_key first-wins
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
    let mut grid = CellGrid::new(state.cols, state.rows);
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(KeyConsumerPluginPlugin::new()));
    registry.init_all(&state);

    // Case 1: Ctrl+S should be consumed by the plugin
    let ctrl_s = KeyEvent {
        key: Key::Char('s'),
        modifiers: Modifiers::CTRL,
    };
    let (flags, cmds) = update(&mut state, Msg::Key(ctrl_s), &mut registry, &mut grid, 3);

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
    let (_flags, cmds) = update(&mut state, Msg::Key(key_a), &mut registry, &mut grid, 3);

    let has_send = cmds.iter().any(|c| matches!(c, Command::SendToKakoune(_)));
    assert!(
        has_send,
        "regular key 'a' should produce SendToKakoune (plugin did not consume it)"
    );
}

// ===========================================================================
// Test 6: Plugin message delivery
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
// Test 7: Menu transform adds prefix
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
