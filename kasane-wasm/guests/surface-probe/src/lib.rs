kasane_plugin_sdk::generate!();

use std::cell::RefCell;

use exports::kasane::plugin::plugin_api::Guest;
use kasane::plugin::element_builder;
use kasane::plugin::host_state;
use kasane::plugin::types::*;

struct SurfaceProbePlugin;

std::thread_local! {
    static WORKSPACE_SUMMARY: RefCell<Option<String>> = const { RefCell::new(None) };
}

fn workspace_summary() -> Option<String> {
    WORKSPACE_SUMMARY.with(|summary| summary.borrow().clone())
}

impl Guest for SurfaceProbePlugin {
    fn get_id() -> String {
        "surface_probe".to_string()
    }

    fn requested_authorities() -> Vec<PluginAuthority> {
        vec![PluginAuthority::DynamicSurface]
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
        if surface_key == "surface_probe.sidebar" {
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
            let mut children = vec![title, slot];
            if let Some(summary) = workspace_summary() {
                children.push(element_builder::create_text(
                    &summary,
                    host_state::get_default_face(),
                ));
            }
            return Some(element_builder::create_column(&children));
        }

        if surface_key == "surface_probe.dynamic" {
            let label = if ctx.focused {
                format!(
                    "surface-probe.dynamic:{}x{}:focused",
                    ctx.rect.w, ctx.rect.h
                )
            } else {
                format!("surface-probe.dynamic:{}x{}", ctx.rect.w, ctx.rect.h)
            };
            return Some(element_builder::create_text(
                &label,
                host_state::get_default_face(),
            ));
        }

        None
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
                key: KeyCode::Char(c),
                ..
            }) if ctx.focused && c == 'n' as u32 => vec![Command::SpawnSession(SessionConfig {
                key: Some("surface-probe.spawned".to_string()),
                session: Some("surface-probe".to_string()),
                args: vec!["README.md".to_string()],
                activate: true,
            })],
            SurfaceEvent::Key(KeyEvent {
                key: KeyCode::Char(c),
                ..
            }) if ctx.focused && c == 'x' as u32 => vec![Command::CloseSession(None)],
            SurfaceEvent::Key(KeyEvent {
                key: KeyCode::Char(c),
                ..
            }) if ctx.focused && c == 'a' as u32 => vec![Command::RegisterSurface(DynamicSurfaceConfig {
                surface_key: "surface_probe.dynamic".to_string(),
                size_hint: SurfaceSizeHint {
                    min_width: 8,
                    min_height: 3,
                    preferred_width: Some(18),
                    preferred_height: Some(6),
                    flex: 0.0,
                },
                declared_slots: vec![],
                placement: SurfacePlacement::Tab,
            })],
            SurfaceEvent::Key(KeyEvent {
                key: KeyCode::Char(c),
                ..
            }) if ctx.focused && c == 'u' as u32 => {
                vec![Command::UnregisterSurface(
                    "surface_probe.dynamic".to_string(),
                )]
            }
            SurfaceEvent::Key(_) if ctx.focused => {
                vec![Command::RequestRedraw(
                    kasane_plugin_sdk::dirty::BUFFER_CURSOR,
                )]
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
        if !matches!(
            surface_key.as_str(),
            "surface_probe.sidebar" | "surface_probe.dynamic"
        ) || dirty_flags == 0
        {
            return vec![];
        }

        vec![Command::RequestRedraw(dirty_flags)]
    }

    fn display_directives() -> Vec<DisplayDirective> {
        match host_state::get_line_text(0).as_deref() {
            Some("fold") => vec![DisplayDirective::Fold(FoldDirective {
                range_start: 1,
                range_end: 3,
                summary: vec![Atom {
                    face: host_state::get_default_face(),
                    contents: "surface-probe-fold".to_string(),
                }],
            })],
            Some("hide") => vec![DisplayDirective::Hide(HideDirective {
                range_start: 1,
                range_end: 3,
            })],
            Some("insert") => vec![DisplayDirective::InsertAfter(InsertAfterDirective {
                after: 1,
                content: vec![Atom {
                    face: host_state::get_default_face(),
                    contents: "surface-probe-virtual".to_string(),
                }],
            })],
            _ => vec![],
        }
    }

    fn on_workspace_changed(snapshot: WorkspaceSnapshot) {
        let summary = format!(
            "workspace:{}:{}:{}",
            snapshot.focused,
            snapshot.surface_count,
            snapshot.rects.len()
        );
        WORKSPACE_SUMMARY.with(|stored| {
            *stored.borrow_mut() = Some(summary);
        });
    }

    fn state_hash() -> u64 {
        host_state::get_cursor_line() as u64
    }

    kasane_plugin_sdk::default_typed_lifecycle!();
    kasane_plugin_sdk::default_line!();
    fn handle_key(_event: KeyEvent) -> Option<Vec<Command>> {
        None
    }

    fn handle_key_middleware(event: KeyEvent) -> KeyHandleResult {
        match event.key {
            KeyCode::Char(c) if c == 'm' as u32 => KeyHandleResult::Transformed(KeyEvent {
                key: KeyCode::Char('x' as u32),
                modifiers: event.modifiers | kasane_plugin_sdk::modifiers::SHIFT,
            }),
            KeyCode::Char(c) if c == '!' as u32 => KeyHandleResult::Consumed(vec![
                Command::SendKeys(vec!["middleware-consumed".to_string()]),
            ]),
            _ => KeyHandleResult::Passthrough,
        }
    }

    fn handle_mouse(_event: MouseEvent, _id: InteractiveId) -> Option<Vec<Command>> {
        None
    }

    fn handle_default_scroll(_candidate: DefaultScrollCandidate) -> Option<ScrollPolicyResult> {
        None
    }

    fn observe_key(_event: KeyEvent) {}

    fn observe_mouse(_event: MouseEvent) {}

    kasane_plugin_sdk::default_overlay!();
    kasane_plugin_sdk::default_menu_transform!();
    kasane_plugin_sdk::default_replace!();
    kasane_plugin_sdk::default_decorate!();
    kasane_plugin_sdk::default_decorator_priority!();
    kasane_plugin_sdk::default_typed_runtime!();
    kasane_plugin_sdk::default_cursor_style!();
    kasane_plugin_sdk::default_named_slot!();
    kasane_plugin_sdk::default_transform!();
    kasane_plugin_sdk::default_transform_priority!();
    kasane_plugin_sdk::default_annotate!();
    kasane_plugin_sdk::default_overlay_v2!();
    kasane_plugin_sdk::default_contribute!();
    kasane_plugin_sdk::default_contribute_to!();
    kasane_plugin_sdk::default_decorate_cells!();
    kasane_plugin_sdk::default_capabilities!();
    kasane_plugin_sdk::default_view_deps!();
    kasane_plugin_sdk::default_key_map!();

    fn register_capabilities() -> u32 {
        0xFFFFFFFF
    }
}

export!(SurfaceProbePlugin);
