kasane_plugin_sdk::generate!("../../../kasane-plugin-sdk/wit");

use exports::kasane::plugin::plugin_api::Guest;
use kasane::plugin::element_builder;
use kasane::plugin::host_state;
use kasane::plugin::types::*;

struct SurfaceProbePlugin;

impl Guest for SurfaceProbePlugin {
    fn get_id() -> String {
        "surface_probe".to_string()
    }

    fn surfaces() -> Vec<SurfaceDescriptor> {
        vec![SurfaceDescriptor {
            surface_key: "surface_probe.sidebar".to_string(),
            size_hint: SurfaceSizeHint {
                min_width: 12,
                min_height: 1,
                preferred_width: Some(24),
                preferred_height: None,
                flex: 1.0,
            },
            declared_slots: vec![DeclaredSlot {
                name: "surface_probe.sidebar.top".to_string(),
                kind: SlotKind::AboveBand,
            }],
            initial_placement: Some(SurfacePlacement::Dock(DockPosition::Left)),
        }]
    }

    fn render_surface(surface_key: String, ctx: SurfaceViewContext) -> Option<ElementHandle> {
        if surface_key != "surface_probe.sidebar" {
            return None;
        }

        let label = if ctx.focused {
            format!("surface-probe:{}x{}:focused", ctx.rect.w, ctx.rect.h)
        } else {
            format!("surface-probe:{}x{}", ctx.rect.w, ctx.rect.h)
        };
        let title = element_builder::create_text(&label, host_state::get_default_face());
        let slot = element_builder::create_slot_placeholder(
            &SlotId::Named("surface_probe.sidebar.top".to_string()),
            LayoutDirection::Column,
            1,
        );
        let children = vec![title, slot];
        Some(element_builder::create_column(&children))
    }

    fn handle_surface_event(
        surface_key: String,
        event: SurfaceEvent,
        ctx: SurfaceEventContext,
    ) -> Vec<Command> {
        if surface_key != "surface_probe.sidebar" {
            return vec![];
        }

        match event {
            SurfaceEvent::Key(KeyEvent {
                key: KeyCode::Character(c),
                ..
            }) if ctx.focused && c == "n" => vec![Command::SpawnSession(SessionConfig {
                key: Some("surface-probe.spawned".to_string()),
                session: Some("surface-probe".to_string()),
                args: vec!["README.md".to_string()],
                activate: true,
            })],
            SurfaceEvent::Key(KeyEvent {
                key: KeyCode::Character(c),
                ..
            }) if ctx.focused && c == "x" => vec![Command::CloseSession(None)],
            SurfaceEvent::Key(_) if ctx.focused => {
                vec![Command::RequestRedraw(kasane_plugin_sdk::dirty::BUFFER_CURSOR)]
            }
            SurfaceEvent::Mouse(_) => {
                vec![Command::RequestRedraw(kasane_plugin_sdk::dirty::INFO)]
            }
            SurfaceEvent::FocusGained => {
                vec![Command::RequestRedraw(kasane_plugin_sdk::dirty::STATUS)]
            }
            SurfaceEvent::FocusLost => {
                vec![Command::RequestRedraw(kasane_plugin_sdk::dirty::OPTIONS)]
            }
            SurfaceEvent::Resize(_) => {
                vec![Command::RequestRedraw(kasane_plugin_sdk::dirty::MENU)]
            }
            _ => vec![],
        }
    }

    fn handle_surface_state_changed(surface_key: String, dirty_flags: u16) -> Vec<Command> {
        if surface_key != "surface_probe.sidebar" || dirty_flags == 0 {
            return vec![];
        }

        vec![Command::RequestRedraw(dirty_flags)]
    }

    fn state_hash() -> u64 {
        host_state::get_cursor_line() as u64
    }

    fn slot_deps(_slot: u8) -> u16 {
        kasane_plugin_sdk::dirty::ALL
    }

    kasane_plugin_sdk::default_lifecycle!();
    kasane_plugin_sdk::default_line!();
    kasane_plugin_sdk::default_input!();
    kasane_plugin_sdk::default_overlay!();
    kasane_plugin_sdk::default_menu_transform!();
    kasane_plugin_sdk::default_replace!();
    kasane_plugin_sdk::default_decorate!();
    kasane_plugin_sdk::default_decorator_priority!();
    kasane_plugin_sdk::default_update!();
    kasane_plugin_sdk::default_cursor_style!();
    kasane_plugin_sdk::default_named_slot!();
    kasane_plugin_sdk::default_transform!();
    kasane_plugin_sdk::default_transform_priority!();
    kasane_plugin_sdk::default_annotate!();
    kasane_plugin_sdk::default_overlay_v2!();
    kasane_plugin_sdk::default_contribute!();
    kasane_plugin_sdk::default_contribute_to!();
    kasane_plugin_sdk::default_contribute_deps!();
    kasane_plugin_sdk::default_transform_deps!();
    kasane_plugin_sdk::default_annotate_deps!();
    kasane_plugin_sdk::default_capabilities!();
    kasane_plugin_sdk::default_io_event!();
}

export!(SurfaceProbePlugin);
