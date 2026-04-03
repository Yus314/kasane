use super::*;

use compact_str::CompactString;

use crate::element::Element;
use crate::plugin::PluginRuntime;
use crate::surface::{EventContext, SizeHint, Surface, SurfaceId, SurfaceRegistry, ViewContext};
use crate::workspace::{Placement, WorkspaceCommand, dispatch_workspace_command};

use super::super::surface::{
    rebuild_plugin_surface_registry, reconcile_plugin_surfaces, register_builtin_surfaces,
    setup_plugin_surfaces,
};

#[test]
fn rebuild_plugin_surface_registry_removes_stale_plugin_surfaces() {
    let state = AppState::default();
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(SurfacePlugin));

    let mut surface_registry = SurfaceRegistry::new();
    register_builtin_surfaces(&mut surface_registry);
    setup_plugin_surfaces(&mut registry, &mut surface_registry, &state);

    assert!(surface_registry.get(SurfaceId(200)).is_some());
    assert!(
        surface_registry
            .workspace()
            .root()
            .collect_ids()
            .contains(&SurfaceId(200))
    );

    assert!(registry.unload_plugin(&PluginId("surface-plugin".to_string())));
    rebuild_plugin_surface_registry(&mut registry, &mut surface_registry, &state);

    assert!(surface_registry.get(SurfaceId(200)).is_none());
    assert!(
        !surface_registry
            .workspace()
            .root()
            .collect_ids()
            .contains(&SurfaceId(200))
    );
    assert!(surface_registry.get(SurfaceId::BUFFER).is_some());
    assert!(surface_registry.get(SurfaceId::STATUS).is_some());
}

#[test]
fn reconcile_plugin_surfaces_removes_stale_plugin_surfaces() {
    let state = AppState::default();
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(SurfacePlugin));

    let mut surface_registry = SurfaceRegistry::new();
    register_builtin_surfaces(&mut surface_registry);
    let disabled_plugins = setup_plugin_surfaces(&mut registry, &mut surface_registry, &state);
    assert!(disabled_plugins.is_empty());

    assert!(surface_registry.get(SurfaceId(200)).is_some());
    assert!(
        surface_registry
            .workspace()
            .root()
            .collect_ids()
            .contains(&SurfaceId(200))
    );

    assert!(registry.unload_plugin(&PluginId("surface-plugin".to_string())));
    let disabled_plugins = reconcile_plugin_surfaces(
        &mut registry,
        &mut surface_registry,
        &state,
        &[owner_delta(Some("r0"), None)],
    );

    assert!(disabled_plugins.is_empty());
    assert!(surface_registry.get(SurfaceId(200)).is_none());
    assert!(
        !surface_registry
            .workspace()
            .root()
            .collect_ids()
            .contains(&SurfaceId(200))
    );
}

#[test]
fn reconcile_plugin_surfaces_preserves_same_id_workspace_placement() {
    let state = AppState::default();
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(SurfacePlugin));

    let mut surface_registry = SurfaceRegistry::new();
    register_builtin_surfaces(&mut surface_registry);
    let disabled_plugins = setup_plugin_surfaces(&mut registry, &mut surface_registry, &state);
    assert!(disabled_plugins.is_empty());
    assert_eq!(
        surface_registry
            .workspace()
            .root()
            .collect_ids()
            .into_iter()
            .filter(|surface_id| *surface_id == SurfaceId(200))
            .count(),
        1
    );

    let _ = registry.reload_plugin_batch(Box::new(ReplacementSurfacePlugin), &AppView::new(&state));
    let disabled_plugins = reconcile_plugin_surfaces(
        &mut registry,
        &mut surface_registry,
        &state,
        &[owner_delta(Some("r1"), Some("r2"))],
    );

    assert!(disabled_plugins.is_empty());
    assert!(surface_registry.get(SurfaceId(200)).is_some());
    assert_eq!(
        surface_registry
            .workspace()
            .root()
            .collect_ids()
            .into_iter()
            .filter(|surface_id| *surface_id == SurfaceId(200))
            .count(),
        1
    );
}

#[test]
fn setup_plugin_surfaces_returns_diagnostic_for_invalid_surface_contract() {
    let state = AppState::default();
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(InvalidSurfacePlugin));

    let mut surface_registry = SurfaceRegistry::new();
    register_builtin_surfaces(&mut surface_registry);
    let diagnostics = setup_plugin_surfaces(&mut registry, &mut surface_registry, &state);

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].plugin_id(),
        Some(&PluginId("invalid-surface-plugin".to_string()))
    );
    assert!(matches!(
        diagnostics[0].kind,
        crate::plugin::PluginDiagnosticKind::SurfaceRegistrationFailed {
            reason: SurfaceRegistrationError::DuplicateSurfaceId { .. }
        }
    ));
    assert!(!registry.contains_plugin(&PluginId("invalid-surface-plugin".to_string())));
}

#[test]
fn reconcile_plugin_surfaces_returns_diagnostic_for_invalid_replacement() {
    let state = AppState::default();
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(SurfacePlugin));

    let mut surface_registry = SurfaceRegistry::new();
    register_builtin_surfaces(&mut surface_registry);
    let diagnostics = setup_plugin_surfaces(&mut registry, &mut surface_registry, &state);
    assert!(diagnostics.is_empty());

    let _ = registry.reload_plugin_batch(Box::new(InvalidSurfacePlugin), &AppView::new(&state));
    let diagnostics = reconcile_plugin_surfaces(
        &mut registry,
        &mut surface_registry,
        &state,
        &[AppliedWinnerDelta {
            id: PluginId("invalid-surface-plugin".to_string()),
            old: None,
            new: Some(PluginDescriptor {
                id: PluginId("invalid-surface-plugin".to_string()),
                source: PluginSource::Host {
                    provider: "test".to_string(),
                },
                revision: PluginRevision("r1".to_string()),
                rank: PluginRank::HOST,
            }),
        }],
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].plugin_id(),
        Some(&PluginId("invalid-surface-plugin".to_string()))
    );
    assert!(matches!(
        diagnostics[0].kind,
        crate::plugin::PluginDiagnosticKind::SurfaceRegistrationFailed {
            reason: SurfaceRegistrationError::DuplicateSurfaceId { .. }
        }
    ));
    assert!(!registry.contains_plugin(&PluginId("invalid-surface-plugin".to_string())));
}

struct TextInputSurface {
    id: SurfaceId,
}

impl Surface for TextInputSurface {
    fn id(&self) -> SurfaceId {
        self.id
    }

    fn surface_key(&self) -> CompactString {
        "test.text-input-surface".into()
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::fill()
    }

    fn view(&self, _ctx: &ViewContext<'_>) -> Element {
        Element::Empty
    }

    fn handle_event(
        &mut self,
        _event: crate::surface::SurfaceEvent,
        _ctx: &EventContext<'_>,
    ) -> Vec<Command> {
        vec![]
    }

    fn handle_text_input(&mut self, text: &str, _ctx: &EventContext<'_>) -> Option<Vec<Command>> {
        (text == "kana").then_some(vec![Command::RequestRedraw(DirtyFlags::INFO)])
    }
}

struct KeyInputSurface {
    id: SurfaceId,
}

impl Surface for KeyInputSurface {
    fn id(&self) -> SurfaceId {
        self.id
    }

    fn surface_key(&self) -> CompactString {
        "test.key-input-surface".into()
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::fill()
    }

    fn view(&self, _ctx: &ViewContext<'_>) -> Element {
        Element::Empty
    }

    fn handle_event(
        &mut self,
        _event: crate::surface::SurfaceEvent,
        _ctx: &EventContext<'_>,
    ) -> Vec<Command> {
        vec![]
    }

    fn handle_key_input(
        &mut self,
        key: &crate::input::KeyEvent,
        _ctx: &EventContext<'_>,
    ) -> Option<Vec<Command>> {
        (key.key == crate::input::Key::Char('x'))
            .then_some(vec![Command::RequestRedraw(DirtyFlags::STATUS)])
    }
}

#[test]
fn route_surface_text_input_preserves_surface_owner_and_commands() {
    let state = AppState::default();
    let total = crate::layout::Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    };
    let owner = PluginId("surface-owner".to_string());
    let mut surface_registry = SurfaceRegistry::new();
    register_builtin_surfaces(&mut surface_registry);
    surface_registry
        .try_register_for_owner(
            Box::new(TextInputSurface { id: SurfaceId(250) }),
            Some(owner.clone()),
        )
        .unwrap();
    let mut dirty = DirtyFlags::empty();
    dispatch_workspace_command(
        &mut surface_registry,
        WorkspaceCommand::AddSurface {
            surface_id: SurfaceId(250),
            placement: Placement::SplitFocused {
                direction: crate::layout::SplitDirection::Vertical,
                ratio: 0.5,
            },
        },
        &mut dirty,
    );
    surface_registry.workspace_mut().focus(SurfaceId(250));

    let result = route_surface_text_input(
        &crate::input::InputEvent::TextInput("kana".into()),
        &mut surface_registry,
        &state,
        total,
    )
    .expect("focused surface should consume text input");

    assert_eq!(result.source_plugin, Some(owner));
    assert!(matches!(
        result.commands.as_slice(),
        [Command::RequestRedraw(DirtyFlags::INFO)]
    ));
}

#[test]
fn route_surface_paste_payload_preserves_surface_owner_and_commands() {
    let state = AppState::default();
    let total = crate::layout::Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    };
    let owner = PluginId("surface-owner".to_string());
    let mut surface_registry = SurfaceRegistry::new();
    register_builtin_surfaces(&mut surface_registry);
    surface_registry
        .try_register_for_owner(
            Box::new(TextInputSurface { id: SurfaceId(251) }),
            Some(owner.clone()),
        )
        .unwrap();
    let mut dirty = DirtyFlags::empty();
    dispatch_workspace_command(
        &mut surface_registry,
        WorkspaceCommand::AddSurface {
            surface_id: SurfaceId(251),
            placement: Placement::SplitFocused {
                direction: crate::layout::SplitDirection::Vertical,
                ratio: 0.5,
            },
        },
        &mut dirty,
    );
    surface_registry.workspace_mut().focus(SurfaceId(251));

    let routed = route_surface_text_input(
        &InputEvent::Paste("kana".into()),
        &mut surface_registry,
        &state,
        total,
    )
    .expect("focused surface should consume pasted text");

    assert_eq!(routed.source_plugin, Some(owner));
    assert!(matches!(
        routed.commands.as_slice(),
        [Command::RequestRedraw(DirtyFlags::INFO)]
    ));
}

#[test]
fn route_surface_text_input_returns_none_when_unhandled() {
    let state = AppState::default();
    let total = crate::layout::Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    };
    let mut surface_registry = SurfaceRegistry::new();
    register_builtin_surfaces(&mut surface_registry);

    let result = route_surface_text_input(
        &crate::input::InputEvent::TextInput("kana".into()),
        &mut surface_registry,
        &state,
        total,
    );

    assert!(result.is_none());
}

#[test]
fn route_surface_key_input_preserves_surface_owner_and_commands() {
    let state = AppState::default();
    let total = crate::layout::Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    };
    let owner = PluginId("surface-owner".to_string());
    let mut surface_registry = SurfaceRegistry::new();
    register_builtin_surfaces(&mut surface_registry);
    surface_registry
        .try_register_for_owner(
            Box::new(KeyInputSurface { id: SurfaceId(251) }),
            Some(owner.clone()),
        )
        .unwrap();
    let mut dirty = DirtyFlags::empty();
    dispatch_workspace_command(
        &mut surface_registry,
        WorkspaceCommand::AddSurface {
            surface_id: SurfaceId(251),
            placement: Placement::SplitFocused {
                direction: crate::layout::SplitDirection::Vertical,
                ratio: 0.5,
            },
        },
        &mut dirty,
    );
    surface_registry.workspace_mut().focus(SurfaceId(251));

    let result = route_surface_key_input(
        &crate::input::InputEvent::Key(crate::input::KeyEvent {
            key: crate::input::Key::Char('x'),
            modifiers: crate::input::Modifiers::empty(),
        }),
        &mut surface_registry,
        &state,
        total,
    )
    .expect("focused surface should consume key input");

    assert_eq!(result.source_plugin, Some(owner));
    assert!(matches!(
        result.commands.as_slice(),
        [Command::RequestRedraw(DirtyFlags::STATUS)]
    ));
}

#[test]
fn route_surface_key_input_returns_none_when_unhandled() {
    let state = AppState::default();
    let total = crate::layout::Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    };
    let mut surface_registry = SurfaceRegistry::new();
    register_builtin_surfaces(&mut surface_registry);

    let result = route_surface_key_input(
        &crate::input::InputEvent::Key(crate::input::KeyEvent {
            key: crate::input::Key::Char('x'),
            modifiers: crate::input::Modifiers::empty(),
        }),
        &mut surface_registry,
        &state,
        total,
    );

    assert!(result.is_none());
}
