use std::collections::HashSet;

use kasane_core::element::{Element, Style, StyleToken};
use kasane_core::input::{Modifiers, MouseButton, MouseEvent, MouseEventKind};
use kasane_core::layout::{Rect, SplitDirection};
use kasane_core::plugin::{Command, PluginId, PluginRuntime};
use kasane_core::state::{AppState, DirtyFlags};
use kasane_core::surface::buffer::KakouneBufferSurface;
use kasane_core::surface::status::StatusBarSurface;
use kasane_core::surface::{
    SizeHint, SlotDeclaration, SlotKind, Surface, SurfaceEvent, SurfaceId, SurfacePlacementRequest,
    SurfaceRegistrationError, SurfaceRegistry,
};
use kasane_core::test_support::TestSurfaceBuilder;
use kasane_core::workspace::{DockPosition, Placement, WorkspaceCommand};

#[test]
fn test_surface_id_equality() {
    assert_eq!(SurfaceId(0), SurfaceId(0));
    assert_ne!(SurfaceId(0), SurfaceId(1));
    assert_eq!(SurfaceId::BUFFER, SurfaceId(0));
    assert_eq!(SurfaceId::STATUS, SurfaceId(1));
}

#[test]
fn test_surface_id_hash() {
    let mut set = HashSet::new();
    set.insert(SurfaceId(0));
    set.insert(SurfaceId(1));
    set.insert(SurfaceId(0));
    assert_eq!(set.len(), 2);
}

#[test]
fn test_size_hint_fixed() {
    let hint = SizeHint::fixed(80, 24);
    assert_eq!(hint.min_width, 80);
    assert_eq!(hint.min_height, 24);
    assert_eq!(hint.preferred_width, Some(80));
    assert_eq!(hint.preferred_height, Some(24));
    assert_eq!(hint.flex, 0.0);
}

#[test]
fn test_size_hint_fill() {
    let hint = SizeHint::fill();
    assert_eq!(hint.flex, 1.0);
    assert_eq!(hint.preferred_width, None);
    assert_eq!(hint.preferred_height, None);
}

#[test]
fn test_size_hint_fixed_height() {
    let hint = SizeHint::fixed_height(1);
    assert_eq!(hint.min_height, 1);
    assert_eq!(hint.preferred_height, Some(1));
    assert_eq!(hint.flex, 0.0);
}

#[test]
fn test_size_hint_default() {
    let hint = SizeHint::default();
    assert_eq!(hint, SizeHint::fill());
}

#[test]
fn test_slot_declaration() {
    let slot = SlotDeclaration::new("kasane.buffer.left", SlotKind::LeftRail);
    assert_eq!(slot.name.as_str(), "kasane.buffer.left");
    assert_eq!(slot.kind, SlotKind::LeftRail);
}

#[test]
fn test_surface_trait_object_safety() {
    // Verify Surface can be used as a trait object
    fn _accepts_surface(_s: &dyn Surface) {}
    fn _accepts_boxed(_s: Box<dyn Surface>) {}
}

// --- SurfaceRegistry tests ---

#[test]
fn test_registry_register_and_get() {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(KakouneBufferSurface::new()));
    reg.register(Box::new(StatusBarSurface::new()));
    assert_eq!(reg.surface_count(), 2);
    assert!(reg.get(SurfaceId::BUFFER).is_some());
    assert!(reg.get(SurfaceId::STATUS).is_some());
    assert!(reg.get(SurfaceId(99)).is_none());
    assert_eq!(
        reg.surface_id_by_key("kasane.buffer"),
        Some(SurfaceId::BUFFER)
    );
    assert_eq!(
        reg.descriptor(SurfaceId::BUFFER)
            .map(|descriptor| descriptor.surface_key.as_str()),
        Some("kasane.buffer")
    );
}

#[test]
fn test_registry_remove() {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(KakouneBufferSurface::new()));
    assert_eq!(reg.surface_count(), 1);
    let removed = reg.remove(SurfaceId::BUFFER);
    assert!(removed.is_some());
    assert_eq!(reg.surface_count(), 0);
}

#[test]
fn test_registry_reject_duplicate_surface_id() {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(KakouneBufferSurface::new()));
    let err = reg
        .try_register(
            TestSurfaceBuilder::new(SurfaceId::BUFFER)
                .key("plugin.buffer-shadow")
                .build(),
        )
        .unwrap_err();
    assert!(matches!(
        err,
        SurfaceRegistrationError::DuplicateSurfaceId {
            surface_id: SurfaceId::BUFFER,
            ..
        }
    ));
}

#[test]
fn test_registry_reject_duplicate_surface_key() {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(KakouneBufferSurface::new()));
    let err = reg
        .try_register(
            TestSurfaceBuilder::new(SurfaceId(500))
                .key("kasane.buffer")
                .build(),
        )
        .unwrap_err();
    assert_eq!(
        err,
        SurfaceRegistrationError::DuplicateSurfaceKey {
            surface_key: "kasane.buffer".into()
        }
    );
}

#[test]
fn test_registry_reject_duplicate_declared_slot() {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(KakouneBufferSurface::new()));
    let err = reg
        .try_register(
            TestSurfaceBuilder::new(SurfaceId(501))
                .key("plugin.duplicate-slot")
                .slots(vec![SlotDeclaration::new(
                    "kasane.buffer.left",
                    SlotKind::LeftRail,
                )])
                .build(),
        )
        .unwrap_err();
    assert_eq!(
        err,
        SurfaceRegistrationError::DuplicateDeclaredSlot {
            slot_name: "kasane.buffer.left".into(),
            existing_surface_key: "kasane.buffer".into(),
            new_surface_key: "plugin.duplicate-slot".into(),
        }
    );
}

#[test]
fn test_registry_reject_duplicate_declared_slot_in_surface() {
    let mut reg = SurfaceRegistry::new();
    let err = reg
        .try_register(
            TestSurfaceBuilder::new(SurfaceId(502))
                .key("plugin.bad-slots")
                .slots(vec![
                    SlotDeclaration::new("plugin.bad-slots.left", SlotKind::LeftRail),
                    SlotDeclaration::new("plugin.bad-slots.left", SlotKind::RightRail),
                ])
                .build(),
        )
        .unwrap_err();
    assert_eq!(
        err,
        SurfaceRegistrationError::DuplicateDeclaredSlotInSurface {
            surface_key: "plugin.bad-slots".into(),
            slot_name: "plugin.bad-slots.left".into(),
        }
    );
}

#[test]
fn test_try_register_for_owner_tracks_surface_owner_plugin() {
    let mut reg = SurfaceRegistry::new();
    let owner = PluginId("plugin.alpha".into());
    let surface_id = SurfaceId(620);
    reg.try_register_for_owner(
        TestSurfaceBuilder::new(surface_id)
            .key("plugin.alpha.surface")
            .build(),
        Some(owner.clone()),
    )
    .unwrap();

    assert_eq!(reg.surface_owner_plugin(surface_id), Some(&owner));
}

#[test]
fn test_route_event_with_sources_preserves_focused_surface_owner() {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(KakouneBufferSurface::new()));

    let owner = PluginId("plugin.focused".into());
    let surface_id = SurfaceId(621);
    reg.try_register_for_owner(
        TestSurfaceBuilder::new(surface_id)
            .key("plugin.focused.surface")
            .on_event(DirtyFlags::STATUS)
            .on_state_changed(DirtyFlags::STATUS)
            .build(),
        Some(owner.clone()),
    )
    .unwrap();

    let mut dirty = DirtyFlags::empty();
    kasane_core::workspace::dispatch_workspace_command(
        &mut reg,
        WorkspaceCommand::AddSurface {
            surface_id,
            placement: Placement::SplitFocused {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
            },
        },
        &mut dirty,
    );
    reg.workspace_mut().focus(surface_id);

    let commands = reg.route_event_with_sources(
        SurfaceEvent::FocusGained,
        &AppState::default(),
        Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        },
    );

    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].source_plugin.as_ref(), Some(&owner));
    assert!(matches!(
        commands[0].commands.as_slice(),
        [Command::RequestRedraw(DirtyFlags::STATUS)]
    ));
}

#[test]
fn test_route_event_with_sources_preserves_owner_plugin_per_surface_on_resize() {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(KakouneBufferSurface::new()));

    let owner_a = PluginId("plugin.alpha".into());
    let owner_b = PluginId("plugin.beta".into());
    let surface_a = SurfaceId(622);
    let surface_b = SurfaceId(623);
    reg.try_register_for_owner(
        TestSurfaceBuilder::new(surface_a)
            .key("plugin.alpha.surface")
            .on_event(DirtyFlags::STATUS)
            .on_state_changed(DirtyFlags::STATUS)
            .build(),
        Some(owner_a.clone()),
    )
    .unwrap();
    reg.try_register_for_owner(
        TestSurfaceBuilder::new(surface_b)
            .key("plugin.beta.surface")
            .on_event(DirtyFlags::MENU)
            .on_state_changed(DirtyFlags::MENU)
            .build(),
        Some(owner_b.clone()),
    )
    .unwrap();

    let mut dirty = DirtyFlags::empty();
    kasane_core::workspace::dispatch_workspace_command(
        &mut reg,
        WorkspaceCommand::AddSurface {
            surface_id: surface_a,
            placement: Placement::SplitFocused {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
            },
        },
        &mut dirty,
    );
    kasane_core::workspace::dispatch_workspace_command(
        &mut reg,
        WorkspaceCommand::AddSurface {
            surface_id: surface_b,
            placement: Placement::SplitFocused {
                direction: SplitDirection::Horizontal,
                ratio: 0.5,
            },
        },
        &mut dirty,
    );

    let commands = reg.route_event_with_sources(
        SurfaceEvent::Resize(Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        }),
        &AppState::default(),
        Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        },
    );

    assert_eq!(commands.len(), 2);
    assert!(
        commands
            .iter()
            .any(|entry| entry.source_plugin.as_ref() == Some(&owner_a)
                && matches!(
                    entry.commands.as_slice(),
                    [Command::RequestRedraw(DirtyFlags::STATUS)]
                ))
    );
    assert!(
        commands
            .iter()
            .any(|entry| entry.source_plugin.as_ref() == Some(&owner_b)
                && matches!(
                    entry.commands.as_slice(),
                    [Command::RequestRedraw(DirtyFlags::MENU)]
                ))
    );
}

#[test]
fn test_on_state_changed_with_sources_preserves_surface_owner() {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(KakouneBufferSurface::new()));

    let owner = PluginId("plugin.stateful".into());
    let surface_id = SurfaceId(624);
    reg.try_register_for_owner(
        TestSurfaceBuilder::new(surface_id)
            .key("plugin.stateful.surface")
            .on_event(DirtyFlags::BUFFER)
            .on_state_changed(DirtyFlags::BUFFER)
            .build(),
        Some(owner.clone()),
    )
    .unwrap();

    let commands = reg.on_state_changed_with_sources(&AppState::default(), DirtyFlags::BUFFER);

    assert!(commands.iter().any(|entry| {
        entry.source_plugin.as_ref() == Some(&owner)
            && matches!(
                entry.commands.as_slice(),
                [Command::RequestRedraw(DirtyFlags::BUFFER)]
            )
    }));
}

#[test]
fn test_handle_workspace_divider_mouse_resizes_split() {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(KakouneBufferSurface::new()));
    let right = SurfaceId(625);
    reg.register(TestSurfaceBuilder::new(right).key("plugin.right").build());

    let mut dirty = DirtyFlags::empty();
    kasane_core::workspace::dispatch_workspace_command(
        &mut reg,
        WorkspaceCommand::AddSurface {
            surface_id: right,
            placement: Placement::SplitFocused {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
            },
        },
        &mut dirty,
    );

    let total = Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    };
    assert_eq!(
        reg.handle_workspace_divider_mouse(
            &MouseEvent {
                kind: MouseEventKind::Press(MouseButton::Left),
                line: 12,
                column: 40,
                modifiers: Modifiers::empty(),
            },
            total,
        ),
        Some(DirtyFlags::empty())
    );
    assert_eq!(
        reg.handle_workspace_divider_mouse(
            &MouseEvent {
                kind: MouseEventKind::Drag(MouseButton::Left),
                line: 12,
                column: 45,
                modifiers: Modifiers::empty(),
            },
            total,
        ),
        Some(DirtyFlags::ALL)
    );
    match reg.workspace().root() {
        kasane_core::workspace::WorkspaceNode::Split { ratio, .. } => {
            let expected = 0.5 + 5.0 / 79.0;
            assert!((*ratio - expected).abs() < 0.001, "ratio={ratio}");
        }
        other => panic!("expected root split, got {other:?}"),
    }
    assert_eq!(
        reg.handle_workspace_divider_mouse(
            &MouseEvent {
                kind: MouseEventKind::Release(MouseButton::Left),
                line: 12,
                column: 45,
                modifiers: Modifiers::empty(),
            },
            total,
        ),
        Some(DirtyFlags::empty())
    );
}

#[test]
fn test_handle_workspace_divider_mouse_ignores_surface_hits() {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(KakouneBufferSurface::new()));
    let total = Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    };
    assert_eq!(
        reg.handle_workspace_divider_mouse(
            &MouseEvent {
                kind: MouseEventKind::Press(MouseButton::Left),
                line: 2,
                column: 2,
                modifiers: Modifiers::empty(),
            },
            total,
        ),
        None
    );
}

#[test]
fn test_registry_compose_single_surface() {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(KakouneBufferSurface::new()));

    let state = AppState::default();
    let plugin_reg = PluginRuntime::new();
    let total = Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    };
    let element = reg.compose_view(&state, &plugin_reg.view(), total);
    // KakouneBufferSurface now delegates to the abstract/resolved surface path.
    assert!(!matches!(element, Element::Empty));
}

#[test]
fn test_registry_compose_split_includes_explicit_divider_node() {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(KakouneBufferSurface::new()));
    let right = SurfaceId(626);
    reg.register(TestSurfaceBuilder::new(right).key("plugin.right").build());

    let mut dirty = DirtyFlags::empty();
    kasane_core::workspace::dispatch_workspace_command(
        &mut reg,
        WorkspaceCommand::AddSurface {
            surface_id: right,
            placement: Placement::SplitFocused {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
            },
        },
        &mut dirty,
    );

    let state = AppState::default();
    let plugin_reg = PluginRuntime::new();
    let total = Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    };
    let element = reg.compose_view(&state, &plugin_reg.view(), total);
    match element {
        Element::Flex { gap, children, .. } => {
            assert_eq!(gap, 0);
            assert_eq!(children.len(), 3);
            assert_eq!(children[1].min_size, Some(1));
            assert_eq!(children[1].max_size, Some(1));
            match &children[1].element {
                Element::Container { style, .. } => {
                    assert_eq!(style, &Style::Token(StyleToken::SPLIT_DIVIDER_FOCUSED))
                }
                other => panic!("expected divider container, got {other:?}"),
            }
        }
        other => panic!("expected split flex root, got {other:?}"),
    }
}

#[test]
fn test_registry_workspace_access() {
    let mut reg = SurfaceRegistry::new();
    assert_eq!(reg.workspace().surface_count(), 1); // default has BUFFER
    let new_id = reg
        .workspace_mut()
        .split_focused(SplitDirection::Vertical, 0.5);
    assert_eq!(reg.workspace().surface_count(), 2);
    assert!(reg.workspace().root().find(new_id).is_some());
}

#[test]
fn test_apply_initial_placements_uses_descriptor_request_before_legacy() {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(KakouneBufferSurface::new()));

    let anchor_id = SurfaceId(610);
    reg.register(
        TestSurfaceBuilder::new(anchor_id)
            .key("plugin.anchor")
            .build(),
    );
    let mut dirty = DirtyFlags::empty();
    kasane_core::workspace::dispatch_workspace_command(
        &mut reg,
        WorkspaceCommand::AddSurface {
            surface_id: anchor_id,
            placement: Placement::SplitFocused {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
            },
        },
        &mut dirty,
    );
    reg.workspace_mut().focus(anchor_id);

    let placed_id = SurfaceId(611);
    reg.register(
        TestSurfaceBuilder::new(placed_id)
            .key("plugin.placed")
            .initial_placement(SurfacePlacementRequest::SplitFrom {
                target_surface_key: "kasane.buffer".into(),
                direction: SplitDirection::Horizontal,
                ratio: 0.5,
            })
            .build(),
    );

    let unresolved = reg.apply_initial_placements(
        &[placed_id],
        Some(&Placement::SplitFocused {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
        }),
        &mut dirty,
    );
    assert!(unresolved.is_empty());

    let rects = reg.workspace().compute_rects(Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    });
    let placed_rect = rects[&placed_id];
    assert_eq!(
        placed_rect,
        Rect {
            x: 0,
            y: 13,
            w: 40,
            h: 11,
        }
    );
}

#[test]
fn test_apply_initial_placements_uses_legacy_fallback() {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(KakouneBufferSurface::new()));

    let placed_id = SurfaceId(612);
    reg.register(
        TestSurfaceBuilder::new(placed_id)
            .key("plugin.legacy-placement")
            .build(),
    );

    let mut dirty = DirtyFlags::empty();
    let unresolved = reg.apply_initial_placements(
        &[placed_id],
        Some(&Placement::SplitFocused {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
        }),
        &mut dirty,
    );
    assert!(unresolved.is_empty());

    let rects = reg.workspace().compute_rects(Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    });
    assert_eq!(
        rects[&placed_id],
        Rect {
            x: 41,
            y: 0,
            w: 39,
            h: 24,
        }
    );
}

#[test]
fn test_apply_initial_placements_reports_unresolved_keyed_request() {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(KakouneBufferSurface::new()));

    let placed_id = SurfaceId(613);
    let request = SurfacePlacementRequest::SplitFrom {
        target_surface_key: "missing.surface".into(),
        direction: SplitDirection::Vertical,
        ratio: 0.5,
    };
    reg.register(
        TestSurfaceBuilder::new(placed_id)
            .key("plugin.unresolved-placement")
            .initial_placement(request.clone())
            .build(),
    );

    let mut dirty = DirtyFlags::empty();
    let unresolved = reg.apply_initial_placements(&[placed_id], None, &mut dirty);
    assert_eq!(unresolved, vec![(placed_id, request)]);
    assert!(reg.workspace().root().find(placed_id).is_none());
}

#[test]
fn test_apply_initial_placements_supports_tab_request() {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(KakouneBufferSurface::new()));

    let placed_id = SurfaceId(614);
    reg.register(
        TestSurfaceBuilder::new(placed_id)
            .key("plugin.tabbed")
            .initial_placement(SurfacePlacementRequest::Tab)
            .build(),
    );

    let mut dirty = DirtyFlags::empty();
    let unresolved = reg.apply_initial_placements(&[placed_id], None, &mut dirty);
    assert!(unresolved.is_empty());
    assert_eq!(reg.workspace().focused(), placed_id);
    assert_eq!(reg.workspace().surface_count(), 2);

    let rects = reg.workspace().compute_rects(Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    });
    assert_eq!(rects.len(), 1);
    assert_eq!(
        rects[&placed_id],
        Rect {
            x: 0,
            y: 1,
            w: 80,
            h: 23,
        }
    );
}

#[test]
fn test_apply_initial_placements_supports_tab_in_keyed_request() {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(KakouneBufferSurface::new()));

    let placed_id = SurfaceId(617);
    reg.register(
        TestSurfaceBuilder::new(placed_id)
            .key("plugin.tabbed-keyed")
            .initial_placement(SurfacePlacementRequest::TabIn {
                target_surface_key: "kasane.buffer".into(),
            })
            .build(),
    );

    let mut dirty = DirtyFlags::empty();
    let unresolved = reg.apply_initial_placements(&[placed_id], None, &mut dirty);
    assert!(unresolved.is_empty());
    assert_eq!(reg.workspace().focused(), placed_id);
    assert_eq!(reg.workspace().surface_count(), 2);

    let rects = reg.workspace().compute_rects(Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    });
    assert_eq!(rects.len(), 1);
    assert_eq!(
        rects[&placed_id],
        Rect {
            x: 0,
            y: 1,
            w: 80,
            h: 23,
        }
    );
}

#[test]
fn test_apply_initial_placements_supports_dock_request() {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(KakouneBufferSurface::new()));

    let placed_id = SurfaceId(615);
    reg.register(
        TestSurfaceBuilder::new(placed_id)
            .key("plugin.left-dock")
            .initial_placement(SurfacePlacementRequest::Dock(DockPosition::Left))
            .build(),
    );

    let mut dirty = DirtyFlags::empty();
    let unresolved = reg.apply_initial_placements(&[placed_id], None, &mut dirty);
    assert!(unresolved.is_empty());

    let rects = reg.workspace().compute_rects(Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    });
    assert_eq!(
        rects[&placed_id],
        Rect {
            x: 0,
            y: 0,
            w: 20,
            h: 24,
        }
    );
    assert_eq!(
        rects[&SurfaceId::BUFFER],
        Rect {
            x: 21,
            y: 0,
            w: 59,
            h: 24,
        }
    );
}

#[test]
fn test_apply_initial_placements_uses_size_hint_for_dock_ratio_when_total_known() {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(KakouneBufferSurface::new()));

    let placed_id = SurfaceId(618);
    reg.register(
        TestSurfaceBuilder::new(placed_id)
            .key("plugin.sized-left-dock")
            .size_hint(SizeHint::fixed(12, 8))
            .initial_placement(SurfacePlacementRequest::Dock(DockPosition::Left))
            .build(),
    );

    let mut dirty = DirtyFlags::empty();
    let unresolved = reg.apply_initial_placements_with_total(
        &[placed_id],
        None,
        &mut dirty,
        Some(Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        }),
    );
    assert!(unresolved.is_empty());

    let rects = reg.workspace().compute_rects(Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    });
    assert_eq!(
        rects[&placed_id],
        Rect {
            x: 0,
            y: 0,
            w: 12,
            h: 24,
        }
    );
    assert_eq!(
        rects[&SurfaceId::BUFFER],
        Rect {
            x: 13,
            y: 0,
            w: 67,
            h: 24,
        }
    );
}

#[test]
fn test_apply_initial_placements_supports_float_request() {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(KakouneBufferSurface::new()));

    let placed_id = SurfaceId(616);
    let float_rect = Rect {
        x: 8,
        y: 4,
        w: 30,
        h: 10,
    };
    reg.register(
        TestSurfaceBuilder::new(placed_id)
            .key("plugin.float")
            .initial_placement(SurfacePlacementRequest::Float { rect: float_rect })
            .build(),
    );

    let mut dirty = DirtyFlags::empty();
    let unresolved = reg.apply_initial_placements(&[placed_id], None, &mut dirty);
    assert!(unresolved.is_empty());

    let rects = reg.workspace().compute_rects(Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    });
    assert_eq!(
        rects[&SurfaceId::BUFFER],
        Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        }
    );
    assert_eq!(rects[&placed_id], float_rect);
}

#[test]
fn test_workspace_command_unfloat_retiles_surface() {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(KakouneBufferSurface::new()));

    let placed_id = SurfaceId(618);
    reg.register(
        TestSurfaceBuilder::new(placed_id)
            .key("plugin.float-roundtrip")
            .build(),
    );

    let mut dirty = DirtyFlags::empty();
    kasane_core::workspace::dispatch_workspace_command(
        &mut reg,
        WorkspaceCommand::AddSurface {
            surface_id: placed_id,
            placement: Placement::SplitFocused {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
            },
        },
        &mut dirty,
    );
    kasane_core::workspace::dispatch_workspace_command(
        &mut reg,
        WorkspaceCommand::Float {
            surface_id: placed_id,
            rect: Rect {
                x: 8,
                y: 4,
                w: 30,
                h: 10,
            },
        },
        &mut dirty,
    );
    kasane_core::workspace::dispatch_workspace_command(
        &mut reg,
        WorkspaceCommand::Unfloat(placed_id),
        &mut dirty,
    );

    let rects = reg.workspace().compute_rects(Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    });
    assert_eq!(
        rects[&SurfaceId::BUFFER],
        Rect {
            x: 0,
            y: 0,
            w: 40,
            h: 24,
        }
    );
    assert_eq!(
        rects[&placed_id],
        Rect {
            x: 41,
            y: 0,
            w: 39,
            h: 24,
        }
    );
}

// --- S6: Ephemeral surface lifecycle ---

#[test]
fn test_sync_ephemeral_no_menu_no_info() {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(KakouneBufferSurface::new()));

    let state = AppState::default();
    reg.sync_ephemeral_surfaces(&state);

    // No menu -> no MenuSurface
    assert!(reg.get(SurfaceId::MENU).is_none());
    // No infos -> no InfoSurface
    assert!(reg.get(SurfaceId(SurfaceId::INFO_BASE)).is_none());
}

fn make_test_menu() -> kasane_core::state::MenuState {
    use kasane_core::protocol::{Coord, Face, MenuStyle};
    kasane_core::state::MenuState {
        items: vec![],
        anchor: Coord { line: 0, column: 0 },
        selected_item_face: Face::default(),
        menu_face: Face::default(),
        style: MenuStyle::Inline,
        selected: None,
        first_item: 0,
        columns: 1,
        win_height: 0,
        menu_lines: 0,
        max_item_width: 0,
        screen_w: 80,
        columns_split: None,
    }
}

fn make_test_info() -> kasane_core::state::InfoState {
    use kasane_core::protocol::{Coord, Face, InfoStyle};
    kasane_core::state::InfoState {
        title: vec![],
        content: vec![],
        anchor: Coord { line: 0, column: 0 },
        face: Face::default(),
        style: InfoStyle::Prompt,
        identity: kasane_core::state::InfoIdentity {
            style: InfoStyle::Prompt,
            anchor_line: 0,
        },
        scroll_offset: 0,
    }
}

#[test]
fn test_sync_ephemeral_menu_appears_and_disappears() {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(KakouneBufferSurface::new()));

    let mut state = AppState::default();

    // Menu appears
    state.observed.menu = Some(make_test_menu());
    reg.sync_ephemeral_surfaces(&state);
    assert!(reg.get(SurfaceId::MENU).is_some());

    // Menu disappears
    state.observed.menu = None;
    reg.sync_ephemeral_surfaces(&state);
    assert!(reg.get(SurfaceId::MENU).is_none());
}

#[test]
fn test_sync_ephemeral_info_count_tracks_state() {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(KakouneBufferSurface::new()));

    let mut state = AppState::default();

    // Two infos appear
    state.observed.infos.push(make_test_info());
    state.observed.infos.push(make_test_info());
    reg.sync_ephemeral_surfaces(&state);
    assert!(reg.get(SurfaceId(SurfaceId::INFO_BASE)).is_some());
    assert!(reg.get(SurfaceId(SurfaceId::INFO_BASE + 1)).is_some());
    assert!(reg.get(SurfaceId(SurfaceId::INFO_BASE + 2)).is_none());

    // One info removed
    state.observed.infos.pop();
    reg.sync_ephemeral_surfaces(&state);
    assert!(reg.get(SurfaceId(SurfaceId::INFO_BASE)).is_some());
    assert!(reg.get(SurfaceId(SurfaceId::INFO_BASE + 1)).is_none());

    // All infos removed
    state.observed.infos.clear();
    reg.sync_ephemeral_surfaces(&state);
    assert!(reg.get(SurfaceId(SurfaceId::INFO_BASE)).is_none());
}

#[test]
fn test_menu_surface_id() {
    use kasane_core::surface::menu::MenuSurface;
    let surface = MenuSurface;
    assert_eq!(surface.id(), SurfaceId::MENU);
}

#[test]
fn test_info_surface_id() {
    use kasane_core::surface::info::InfoSurface;
    let surface = InfoSurface::new(0);
    assert_eq!(surface.id(), SurfaceId(SurfaceId::INFO_BASE));
    let surface2 = InfoSurface::new(3);
    assert_eq!(surface2.id(), SurfaceId(SurfaceId::INFO_BASE + 3));
}

// --- S7: Surface-local named slots ---

#[test]
fn test_all_declared_slots() {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(KakouneBufferSurface::new()));
    reg.register(Box::new(StatusBarSurface::new()));

    let slots = reg.all_declared_slots();
    // KakouneBufferSurface declares 5 slots, StatusBarSurface declares 3
    assert_eq!(slots.len(), 8);

    let slot_names: Vec<&str> = slots.iter().map(|(_, s)| s.name.as_str()).collect();
    assert!(slot_names.contains(&"kasane.buffer.left"));
    assert!(slot_names.contains(&"kasane.buffer.right"));
    assert!(slot_names.contains(&"kasane.buffer.above"));
    assert!(slot_names.contains(&"kasane.buffer.below"));
    assert!(slot_names.contains(&"kasane.buffer.overlay"));
    assert!(slot_names.contains(&"kasane.status.above"));
    assert!(slot_names.contains(&"kasane.status.left"));
    assert!(slot_names.contains(&"kasane.status.right"));
}

#[test]
fn test_slot_owner() {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(KakouneBufferSurface::new()));
    reg.register(Box::new(StatusBarSurface::new()));

    assert_eq!(
        reg.slot_owner("kasane.buffer.left"),
        Some(SurfaceId::BUFFER)
    );
    assert_eq!(
        reg.slot_owner("kasane.status.right"),
        Some(SurfaceId::STATUS)
    );
    assert_eq!(reg.slot_owner("nonexistent.slot"), None);
}

#[test]
fn test_slot_owner_after_surface_removal() {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(KakouneBufferSurface::new()));
    assert!(reg.slot_owner("kasane.buffer.left").is_some());

    reg.remove(SurfaceId::BUFFER);
    assert!(reg.slot_owner("kasane.buffer.left").is_none());
}

/// Physical adjacency test for Split(A, Split(B, C)) layout: |A│B│C|
///
/// The divider between `first` and `second` is physically adjacent to
/// the trailing-edge leaf of `first` and the leading-edge leaf of `second`.
///
/// Expectations per focus state:
///   A focused → outer=FOCUSED (A on trailing edge of first), inner=NORMAL
///   B focused → outer=FOCUSED (B on leading edge of second), inner=FOCUSED (B on trailing edge)
///   C focused → outer=NORMAL  (C not on any edge of outer), inner=FOCUSED (C on leading edge)
#[test]
fn test_registry_compose_split_divider_physical_adjacency() {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(KakouneBufferSurface::new()));

    let b = SurfaceId(700);
    reg.register(TestSurfaceBuilder::new(b).key("plugin.b").build());
    let mut dirty = DirtyFlags::empty();
    kasane_core::workspace::dispatch_workspace_command(
        &mut reg,
        WorkspaceCommand::AddSurface {
            surface_id: b,
            placement: Placement::SplitFocused {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
            },
        },
        &mut dirty,
    );

    // Focus back to A (BUFFER)
    kasane_core::workspace::dispatch_workspace_command(
        &mut reg,
        WorkspaceCommand::FocusDirection(kasane_core::workspace::FocusDirection::Prev),
        &mut dirty,
    );
    assert_eq!(reg.workspace().focused(), SurfaceId::BUFFER);

    let c = SurfaceId(701);
    reg.register(TestSurfaceBuilder::new(c).key("plugin.c").build());
    kasane_core::workspace::dispatch_workspace_command(
        &mut reg,
        WorkspaceCommand::AddSurface {
            surface_id: c,
            placement: Placement::SplitFrom {
                target: b,
                direction: SplitDirection::Vertical,
                ratio: 0.5,
            },
        },
        &mut dirty,
    );

    // Layout: Split(A*, Split(B, C))  →  |A*│B│C|
    let state = AppState::default();
    let plugin_reg = PluginRuntime::new();
    let total = Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    };

    // Helper to extract divider tokens from compose output
    fn divider_tokens(element: &Element) -> (StyleToken, StyleToken) {
        match element {
            Element::Flex { children, .. } => {
                assert_eq!(children.len(), 3);
                let outer = match &children[1].element {
                    Element::Container {
                        style: Style::Token(t),
                        ..
                    } => t.clone(),
                    other => panic!("expected outer divider, got {other:?}"),
                };
                let inner = match &children[2].element {
                    Element::Flex {
                        children: inner, ..
                    } => {
                        assert_eq!(inner.len(), 3);
                        match &inner[1].element {
                            Element::Container {
                                style: Style::Token(t),
                                ..
                            } => t.clone(),
                            other => panic!("expected inner divider, got {other:?}"),
                        }
                    }
                    other => panic!("expected inner flex, got {other:?}"),
                };
                (outer, inner)
            }
            other => panic!("expected flex, got {other:?}"),
        }
    }

    // A focused: outer=FOCUSED, inner=NORMAL
    let element = reg.compose_view(&state, &plugin_reg.view(), total);
    let (outer, inner) = divider_tokens(&element);
    assert_eq!(outer, StyleToken::SPLIT_DIVIDER_FOCUSED, "A focused: outer");
    assert_eq!(inner, StyleToken::SPLIT_DIVIDER, "A focused: inner");

    // B focused: outer=FOCUSED (B on leading edge), inner=FOCUSED (B on trailing edge)
    kasane_core::workspace::dispatch_workspace_command(
        &mut reg,
        WorkspaceCommand::Focus(b),
        &mut dirty,
    );
    let element = reg.compose_view(&state, &plugin_reg.view(), total);
    let (outer, inner) = divider_tokens(&element);
    assert_eq!(outer, StyleToken::SPLIT_DIVIDER_FOCUSED, "B focused: outer");
    assert_eq!(inner, StyleToken::SPLIT_DIVIDER_FOCUSED, "B focused: inner");

    // C focused: outer=NORMAL (C not adjacent), inner=FOCUSED
    kasane_core::workspace::dispatch_workspace_command(
        &mut reg,
        WorkspaceCommand::Focus(c),
        &mut dirty,
    );
    let element = reg.compose_view(&state, &plugin_reg.view(), total);
    let (outer, inner) = divider_tokens(&element);
    assert_eq!(outer, StyleToken::SPLIT_DIVIDER, "C focused: outer");
    assert_eq!(inner, StyleToken::SPLIT_DIVIDER_FOCUSED, "C focused: inner");
}
