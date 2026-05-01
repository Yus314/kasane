//! Integration tests for the plugin system:
//!   `#[kasane_plugin]` macro → PluginRuntime → view → layout → paint → CellGrid
//!
//! These tests verify the end-to-end plugin pipeline, covering:
//! Lifecycle, Input, Event/Message, MenuTransform, Transform, and CursorStyle.

use kasane_core::element::Element;
use kasane_core::input::{Key, KeyEvent, Modifiers};
use kasane_core::kasane_plugin;
use kasane_core::plugin::{
    AppView, Command, ContribSizeHint, ContributeContext, Contribution, PluginBackend,
    PluginCapabilities, PluginId, PluginRuntime, SlotId,
};
use kasane_core::protocol::{Color, Coord, Line, MenuStyle, NamedColor, WireFace};
use kasane_core::render::{CursorStyle, cursor_style_default};
use kasane_core::state::{AppState, DirtyFlags, Msg, update_in_place};
use kasane_core::test_support::{make_line, render_with_registry, row_text};

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn setup_state(lines: Vec<Line>) -> AppState {
    let mut state = kasane_core::test_support::test_state_80x24();
    state.observed.lines = (lines).into();
    state.observed.status_default_style = state.observed.default_style.clone();
    state.inference.status_line = make_line(" main.rs ");
    state.observed.status_mode_line = make_line("normal");
    state
}

// ===========================================================================
// Test 1: handle_key first-wins
// ===========================================================================

#[kasane_plugin]
mod key_consumer_plugin {
    use kasane_core::input::KeyEvent;
    use kasane_core::plugin::{AppView, Command};
    use kasane_core::state::DirtyFlags;

    #[state]
    #[derive(Default)]
    pub struct State;

    pub fn handle_key(
        _state: &mut State,
        key: &KeyEvent,
        _core: &AppView<'_>,
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
    let mut state = Box::new(setup_state(vec![make_line("text")]));
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(KeyConsumerPluginPlugin::new()));
    let _ = registry.init_all_batch(&AppView::new(&state));

    // Case 1: Ctrl+S should be consumed by the plugin
    let ctrl_s = KeyEvent {
        key: Key::Char('s'),
        modifiers: Modifiers::CTRL,
    };
    let result = update_in_place(&mut state, Msg::Key(ctrl_s), &mut registry, 3);
    let flags = result.flags;
    let cmds = result.commands;

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
    let cmds = update_in_place(&mut state, Msg::Key(key_a), &mut registry, 3).commands;

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
    use kasane_core::plugin::{AppView, Effects};
    use kasane_core::state::DirtyFlags;

    #[state]
    #[derive(Default)]
    pub struct State {
        pub value: u32,
    }

    #[event]
    pub enum Msg {
        SetValue(u32),
    }

    pub fn update_effects(
        state: &mut State,
        msg: &mut dyn std::any::Any,
        _core: &AppView<'_>,
    ) -> Effects {
        let msg = msg
            .downcast_ref::<Msg>()
            .expect("typed plugin integration test payload must match Msg");
        match msg {
            Msg::SetValue(v) => {
                state.value = *v;
                Effects {
                    redraw: DirtyFlags::STATUS,
                    commands: vec![],
                    scroll_plans: vec![],
                    state_updates: Default::default(),
                }
            }
        }
    }
}

#[test]
fn plugin_message_delivery() {
    let state = setup_state(vec![make_line("text")]);

    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(MsgReceiverPluginPlugin::new()));
    let _ = registry.init_all_batch(&AppView::new(&state));

    let target_id = kasane_core::plugin::PluginId("msg_receiver_plugin".into());
    let payload: Box<dyn std::any::Any> = Box::new(msg_receiver_plugin::Msg::SetValue(42));
    let batch = registry.deliver_message_batch(&target_id, payload, &AppView::new(&state));

    assert!(
        batch.effects.redraw.contains(DirtyFlags::STATUS),
        "deliver_message_batch should return STATUS redraw effect, got: {:?}",
        batch.effects.redraw
    );
    assert!(
        batch.effects.commands.is_empty(),
        "typed update_effects should not emit direct commands"
    );
}

// ===========================================================================
// Test 3: Menu transform adds prefix
// ===========================================================================

#[kasane_plugin]
mod prefix_plugin {
    use kasane_core::plugin::AppView;
    use kasane_core::protocol::Atom;

    #[state]
    #[derive(Default)]
    pub struct State;

    pub fn transform_menu_item(
        _state: &State,
        item: &[Atom],
        _index: usize,
        _selected: bool,
        _core: &AppView<'_>,
    ) -> Option<Vec<Atom>> {
        let mut result = vec![Atom::plain(">> ")];
        result.extend(item.iter().cloned());
        Some(result)
    }
}

#[test]
fn menu_transform_adds_prefix() {
    use kasane_core::protocol::KakouneRequest;

    let mut state = setup_state(vec![make_line("fn main() {}")]);
    state.observed.cursor_pos = Coord { line: 0, column: 3 };

    // Show inline menu with items
    let items = vec![make_line("alpha"), make_line("beta")];
    state.apply(KakouneRequest::MenuShow {
        items,
        anchor: Coord { line: 0, column: 3 },
        selected_item_style: std::sync::Arc::new(
            kasane_core::protocol::UnresolvedStyle::from_face(&WireFace {
                fg: Color::Named(NamedColor::Black),
                bg: Color::Named(NamedColor::Cyan),
                ..WireFace::default()
            }),
        ),
        menu_style: std::sync::Arc::new(kasane_core::protocol::UnresolvedStyle::from_face(
            &WireFace {
                fg: Color::Named(NamedColor::White),
                bg: Color::Named(NamedColor::Blue),
                ..WireFace::default()
            },
        )),
        style: MenuStyle::Inline,
    });

    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(kasane_core::render::view::menu::BuiltinMenuPlugin));
    registry.register_backend(Box::new(PrefixPluginPlugin::new()));
    let _ = registry.init_all_batch(&AppView::new(&state));
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
    let item = vec![kasane_core::protocol::Atom::plain("alpha")];
    let transformed = registry
        .view()
        .transform_menu_item(&item, 0, false, &AppView::new(&state));
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

// ===========================================================================
// Test 4: Buffer transform adds banner
// ===========================================================================

#[kasane_plugin]
mod buffer_banner {
    use kasane_core::element::{Element, FlexChild};
    #[allow(unused_imports)]
    use kasane_core::plugin::TransformTarget;
    use kasane_core::plugin::{AppView, TransformSubject};

    #[state]
    #[derive(Default)]
    pub struct State;

    #[transform(TransformTarget::BUFFER)]
    pub fn wrap_buffer(
        _state: &State,
        subject: TransformSubject,
        _core: &AppView<'_>,
    ) -> TransformSubject {
        subject.map_element(|element| {
            Element::column(vec![
                FlexChild::fixed(Element::plain_text("[buffer transformed]")),
                FlexChild::flexible(element, 1.0),
            ])
        })
    }
}

#[test]
fn buffer_transform_adds_banner() {
    let state = setup_state(vec![make_line("line 0"), make_line("line 1")]);

    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(BufferBannerPlugin::new()));
    let _ = registry.init_all_batch(&AppView::new(&state));

    let transformed = registry
        .view()
        .apply_transform_chain(
            kasane_core::plugin::TransformTarget::BUFFER,
            kasane_core::plugin::TransformSubject::Element(Element::buffer_ref(0..2)),
            &AppView::new(&state),
        )
        .into_element();
    match transformed {
        Element::Flex { children, .. } => {
            assert_eq!(
                children.len(),
                2,
                "transform should wrap the buffer in a column"
            );
        }
        other => panic!("expected transformed buffer wrapper, got {other:?}"),
    }

    let grid = render_with_registry(&state, &registry);
    assert_eq!(row_text(&grid, 0), "[buffer transformed]");
    assert_eq!(row_text(&grid, 1), "line 0");
    assert_eq!(row_text(&grid, 2), "line 1");
}

// ===========================================================================
// Test 5: ABOVE_BUFFER / BELOW_BUFFER contribute_to proof
// ===========================================================================

struct VerticalBandsPlugin;

impl PluginBackend for VerticalBandsPlugin {
    fn id(&self) -> PluginId {
        PluginId("vertical_bands".into())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::CONTRIBUTOR
    }

    fn contribute_to(
        &self,
        region: &SlotId,
        _state: &AppView<'_>,
        _ctx: &ContributeContext,
    ) -> Option<Contribution> {
        let label = if region == &SlotId::ABOVE_BUFFER {
            "ABOVE-BUFFER"
        } else if region == &SlotId::BELOW_BUFFER {
            "BELOW-BUFFER"
        } else {
            return None;
        };

        Some(Contribution {
            element: Element::plain_text(label),
            priority: 0,
            size_hint: ContribSizeHint::Auto,
        })
    }
}

#[test]
fn above_and_below_buffer_slots_render() {
    let state = setup_state(vec![make_line("line 0"), make_line("line 1")]);

    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(VerticalBandsPlugin));
    let _ = registry.init_all_batch(&AppView::new(&state));

    let grid = render_with_registry(&state, &registry);
    let rows: Vec<String> = (0..state.runtime.rows)
        .map(|y| row_text(&grid, y))
        .collect();

    assert!(
        rows.iter().any(|row| row.contains("ABOVE-BUFFER")),
        "expected ABOVE_BUFFER contribution in rendered output"
    );
    assert!(
        rows.iter().any(|row| row.contains("BELOW-BUFFER")),
        "expected BELOW_BUFFER contribution in rendered output"
    );
}

// ===========================================================================
// Test 6: Render ornament cursor style wins over default logic
// ===========================================================================

struct UnderlineCursorPlugin;

impl PluginBackend for UnderlineCursorPlugin {
    fn id(&self) -> PluginId {
        PluginId("underline_cursor".into())
    }

    fn capabilities(&self) -> kasane_core::plugin::PluginCapabilities {
        kasane_core::plugin::PluginCapabilities::RENDER_ORNAMENT
    }

    fn render_ornaments(
        &self,
        _state: &AppView<'_>,
        _ctx: &kasane_core::plugin::RenderOrnamentContext,
    ) -> kasane_core::plugin::OrnamentBatch {
        kasane_core::plugin::OrnamentBatch {
            cursor_style: Some(kasane_core::plugin::CursorStyleOrn {
                hint: CursorStyle::Underline.into(),
                priority: 10,
                modality: kasane_core::plugin::OrnamentModality::Must,
            }),
            ..kasane_core::plugin::OrnamentBatch::default()
        }
    }
}

#[test]
fn render_ornament_cursor_style_wins_over_default_logic() {
    let mut state = setup_state(vec![make_line("text")]);
    state.runtime.focused = false;

    assert_eq!(cursor_style_default(&state), CursorStyle::Outline);

    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(UnderlineCursorPlugin));
    let _ = registry.init_all_batch(&AppView::new(&state));

    let ctx = kasane_core::plugin::RenderOrnamentContext::default();
    let collected = registry
        .view()
        .collect_ornaments(&AppView::new(&state), &ctx);
    assert_eq!(
        collected.cursor_style.map(|h| h.shape),
        Some(CursorStyle::Underline)
    );
}
