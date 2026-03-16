//! Surface abstraction: first-class rectangular screen regions.
//!
//! A Surface owns a rectangular area of the screen and is responsible for
//! building its Element tree and handling events within that region.
//! Both core components (buffer, status bar) and plugins can implement Surface,
//! enabling symmetric extensibility.

pub mod buffer;
pub mod info;
pub mod menu;
mod registry;
pub mod resolve;
pub mod status;
mod traits;
mod types;

pub use registry::*;
pub use traits::*;
pub use types::*;

pub use resolve::{
    ContributorIssue, ContributorIssueKind, OwnerValidationError, OwnerValidationErrorKind,
    ResolvedSlotContentKind, ResolvedSlotRecord, ResolvedTree, SurfaceComposeResult,
    SurfaceRenderOutcome, SurfaceRenderReport,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_surface_id_equality() {
        assert_eq!(SurfaceId(0), SurfaceId(0));
        assert_ne!(SurfaceId(0), SurfaceId(1));
        assert_eq!(SurfaceId::BUFFER, SurfaceId(0));
        assert_eq!(SurfaceId::STATUS, SurfaceId(1));
    }

    #[test]
    fn test_surface_id_hash() {
        use std::collections::HashSet;
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

    use compact_str::CompactString;

    use crate::element::Element;
    use crate::input::{MouseButton, MouseEvent, MouseEventKind};
    use crate::layout::{Rect, SplitDirection};
    use crate::plugin::Command;
    use crate::state::DirtyFlags;
    use crate::surface::buffer::KakouneBufferSurface;
    use crate::surface::status::StatusBarSurface;
    use crate::workspace::{DockPosition, Placement, WorkspaceCommand};

    struct TestSurface {
        id: SurfaceId,
        surface_key: CompactString,
        slots: Vec<SlotDeclaration>,
        initial_placement: Option<SurfacePlacementRequest>,
        size_hint: SizeHint,
    }

    impl TestSurface {
        fn new(
            id: SurfaceId,
            surface_key: impl Into<CompactString>,
            slots: Vec<SlotDeclaration>,
        ) -> Self {
            Self {
                id,
                surface_key: surface_key.into(),
                slots,
                initial_placement: None,
                size_hint: SizeHint::fill(),
            }
        }

        fn with_initial_placement(mut self, initial_placement: SurfacePlacementRequest) -> Self {
            self.initial_placement = Some(initial_placement);
            self
        }

        fn with_size_hint(mut self, size_hint: SizeHint) -> Self {
            self.size_hint = size_hint;
            self
        }
    }

    impl Surface for TestSurface {
        fn id(&self) -> SurfaceId {
            self.id
        }

        fn surface_key(&self) -> CompactString {
            self.surface_key.clone()
        }

        fn size_hint(&self) -> SizeHint {
            self.size_hint
        }

        fn initial_placement(&self) -> Option<SurfacePlacementRequest> {
            self.initial_placement.clone()
        }

        fn view(&self, _ctx: &ViewContext<'_>) -> Element {
            Element::Empty
        }

        fn handle_event(&mut self, _event: SurfaceEvent, _ctx: &EventContext<'_>) -> Vec<Command> {
            vec![]
        }

        fn declared_slots(&self) -> &[SlotDeclaration] {
            &self.slots
        }
    }

    struct EventSurface {
        id: SurfaceId,
        surface_key: CompactString,
        command_flag: DirtyFlags,
    }

    impl EventSurface {
        fn new(
            id: SurfaceId,
            surface_key: impl Into<CompactString>,
            command_flag: DirtyFlags,
        ) -> Self {
            Self {
                id,
                surface_key: surface_key.into(),
                command_flag,
            }
        }
    }

    impl Surface for EventSurface {
        fn id(&self) -> SurfaceId {
            self.id
        }

        fn surface_key(&self) -> CompactString {
            self.surface_key.clone()
        }

        fn size_hint(&self) -> SizeHint {
            SizeHint::fill()
        }

        fn view(&self, _ctx: &ViewContext<'_>) -> Element {
            Element::Empty
        }

        fn handle_event(&mut self, _event: SurfaceEvent, _ctx: &EventContext<'_>) -> Vec<Command> {
            vec![Command::RequestRedraw(self.command_flag)]
        }

        fn on_state_changed(
            &mut self,
            _state: &crate::state::AppState,
            _dirty: DirtyFlags,
        ) -> Vec<Command> {
            vec![Command::RequestRedraw(self.command_flag)]
        }
    }

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
            .try_register(Box::new(TestSurface::new(
                SurfaceId::BUFFER,
                "plugin.buffer-shadow",
                vec![],
            )))
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
            .try_register(Box::new(TestSurface::new(
                SurfaceId(500),
                "kasane.buffer",
                vec![],
            )))
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
            .try_register(Box::new(TestSurface::new(
                SurfaceId(501),
                "plugin.duplicate-slot",
                vec![SlotDeclaration::new(
                    "kasane.buffer.left",
                    SlotKind::LeftRail,
                )],
            )))
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
            .try_register(Box::new(TestSurface::new(
                SurfaceId(502),
                "plugin.bad-slots",
                vec![
                    SlotDeclaration::new("plugin.bad-slots.left", SlotKind::LeftRail),
                    SlotDeclaration::new("plugin.bad-slots.left", SlotKind::RightRail),
                ],
            )))
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
        use crate::plugin::PluginId;
        let mut reg = SurfaceRegistry::new();
        let owner = PluginId("plugin.alpha".into());
        let surface_id = SurfaceId(620);
        reg.try_register_for_owner(
            Box::new(TestSurface::new(surface_id, "plugin.alpha.surface", vec![])),
            Some(owner.clone()),
        )
        .unwrap();

        assert_eq!(reg.surface_owner_plugin(surface_id), Some(&owner));
    }

    #[test]
    fn test_route_event_with_sources_preserves_focused_surface_owner() {
        use crate::plugin::PluginId;
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));

        let owner = PluginId("plugin.focused".into());
        let surface_id = SurfaceId(621);
        reg.try_register_for_owner(
            Box::new(EventSurface::new(
                surface_id,
                "plugin.focused.surface",
                DirtyFlags::STATUS,
            )),
            Some(owner.clone()),
        )
        .unwrap();

        let mut dirty = DirtyFlags::empty();
        crate::workspace::dispatch_workspace_command(
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
            &crate::state::AppState::default(),
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
        use crate::plugin::PluginId;
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));

        let owner_a = PluginId("plugin.alpha".into());
        let owner_b = PluginId("plugin.beta".into());
        let surface_a = SurfaceId(622);
        let surface_b = SurfaceId(623);
        reg.try_register_for_owner(
            Box::new(EventSurface::new(
                surface_a,
                "plugin.alpha.surface",
                DirtyFlags::STATUS,
            )),
            Some(owner_a.clone()),
        )
        .unwrap();
        reg.try_register_for_owner(
            Box::new(EventSurface::new(
                surface_b,
                "plugin.beta.surface",
                DirtyFlags::MENU,
            )),
            Some(owner_b.clone()),
        )
        .unwrap();

        let mut dirty = DirtyFlags::empty();
        crate::workspace::dispatch_workspace_command(
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
        crate::workspace::dispatch_workspace_command(
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
            &crate::state::AppState::default(),
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
        use crate::plugin::PluginId;
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));

        let owner = PluginId("plugin.stateful".into());
        let surface_id = SurfaceId(624);
        reg.try_register_for_owner(
            Box::new(EventSurface::new(
                surface_id,
                "plugin.stateful.surface",
                DirtyFlags::BUFFER,
            )),
            Some(owner.clone()),
        )
        .unwrap();

        let commands = reg
            .on_state_changed_with_sources(&crate::state::AppState::default(), DirtyFlags::BUFFER);

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
        reg.register(Box::new(TestSurface::new(right, "plugin.right", vec![])));

        let mut dirty = DirtyFlags::empty();
        crate::workspace::dispatch_workspace_command(
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
                    modifiers: crate::input::Modifiers::empty(),
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
                    modifiers: crate::input::Modifiers::empty(),
                },
                total,
            ),
            Some(DirtyFlags::ALL)
        );
        match reg.workspace().root() {
            crate::workspace::WorkspaceNode::Split { ratio, .. } => {
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
                    modifiers: crate::input::Modifiers::empty(),
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
                    modifiers: crate::input::Modifiers::empty(),
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

        let state = crate::state::AppState::default();
        let plugin_reg = crate::plugin::PluginRegistry::new();
        let total = Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        };
        let element = reg.compose_view(&state, &plugin_reg, total);
        // KakouneBufferSurface now delegates to the abstract/resolved surface path.
        assert!(!matches!(element, Element::Empty));
    }

    #[test]
    fn test_registry_compose_split_includes_explicit_divider_node() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));
        let right = SurfaceId(626);
        reg.register(Box::new(TestSurface::new(right, "plugin.right", vec![])));

        let mut dirty = DirtyFlags::empty();
        crate::workspace::dispatch_workspace_command(
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

        let state = crate::state::AppState::default();
        let plugin_reg = crate::plugin::PluginRegistry::new();
        let total = Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        };
        let element = reg.compose_view(&state, &plugin_reg, total);
        match element {
            Element::Flex { gap, children, .. } => {
                assert_eq!(gap, 0);
                assert_eq!(children.len(), 3);
                assert_eq!(children[1].min_size, Some(1));
                assert_eq!(children[1].max_size, Some(1));
                match &children[1].element {
                    Element::Container { style, .. } => assert_eq!(
                        style,
                        &crate::element::Style::Token(crate::element::StyleToken::SPLIT_DIVIDER)
                    ),
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
            .split_focused(crate::pane::SplitDirection::Vertical, 0.5);
        assert_eq!(reg.workspace().surface_count(), 2);
        assert!(reg.workspace().root().find(new_id).is_some());
    }

    #[test]
    fn test_apply_initial_placements_uses_descriptor_request_before_legacy() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));

        let anchor_id = SurfaceId(610);
        reg.register(Box::new(TestSurface::new(
            anchor_id,
            "plugin.anchor",
            vec![],
        )));
        let mut dirty = DirtyFlags::empty();
        crate::workspace::dispatch_workspace_command(
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
        reg.register(Box::new(
            TestSurface::new(placed_id, "plugin.placed", vec![]).with_initial_placement(
                SurfacePlacementRequest::SplitFrom {
                    target_surface_key: "kasane.buffer".into(),
                    direction: SplitDirection::Horizontal,
                    ratio: 0.5,
                },
            ),
        ));

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
        reg.register(Box::new(TestSurface::new(
            placed_id,
            "plugin.legacy-placement",
            vec![],
        )));

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
        reg.register(Box::new(
            TestSurface::new(placed_id, "plugin.unresolved-placement", vec![])
                .with_initial_placement(request.clone()),
        ));

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
        reg.register(Box::new(
            TestSurface::new(placed_id, "plugin.tabbed", vec![])
                .with_initial_placement(SurfacePlacementRequest::Tab),
        ));

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
        reg.register(Box::new(
            TestSurface::new(placed_id, "plugin.tabbed-keyed", vec![]).with_initial_placement(
                SurfacePlacementRequest::TabIn {
                    target_surface_key: "kasane.buffer".into(),
                },
            ),
        ));

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
        reg.register(Box::new(
            TestSurface::new(placed_id, "plugin.left-dock", vec![])
                .with_initial_placement(SurfacePlacementRequest::Dock(DockPosition::Left)),
        ));

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
        reg.register(Box::new(
            TestSurface::new(placed_id, "plugin.sized-left-dock", vec![])
                .with_size_hint(SizeHint::fixed(12, 8))
                .with_initial_placement(SurfacePlacementRequest::Dock(DockPosition::Left)),
        ));

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
        reg.register(Box::new(
            TestSurface::new(placed_id, "plugin.float", vec![])
                .with_initial_placement(SurfacePlacementRequest::Float { rect: float_rect }),
        ));

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
        reg.register(Box::new(TestSurface::new(
            placed_id,
            "plugin.float-roundtrip",
            vec![],
        )));

        let mut dirty = DirtyFlags::empty();
        crate::workspace::dispatch_workspace_command(
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
        crate::workspace::dispatch_workspace_command(
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
        crate::workspace::dispatch_workspace_command(
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

        let state = crate::state::AppState::default();
        reg.sync_ephemeral_surfaces(&state);

        // No menu -> no MenuSurface
        assert!(reg.get(SurfaceId::MENU).is_none());
        // No infos -> no InfoSurface
        assert!(reg.get(SurfaceId(SurfaceId::INFO_BASE)).is_none());
    }

    fn make_test_menu() -> crate::state::MenuState {
        use crate::protocol::{Coord, Face, MenuStyle};
        crate::state::MenuState {
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

    fn make_test_info() -> crate::state::InfoState {
        use crate::protocol::{Coord, Face, InfoStyle};
        crate::state::InfoState {
            title: vec![],
            content: vec![],
            anchor: Coord { line: 0, column: 0 },
            face: Face::default(),
            style: InfoStyle::Prompt,
            identity: crate::state::InfoIdentity {
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

        let mut state = crate::state::AppState::default();

        // Menu appears
        state.menu = Some(make_test_menu());
        reg.sync_ephemeral_surfaces(&state);
        assert!(reg.get(SurfaceId::MENU).is_some());

        // Menu disappears
        state.menu = None;
        reg.sync_ephemeral_surfaces(&state);
        assert!(reg.get(SurfaceId::MENU).is_none());
    }

    #[test]
    fn test_sync_ephemeral_info_count_tracks_state() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));

        let mut state = crate::state::AppState::default();

        // Two infos appear
        state.infos.push(make_test_info());
        state.infos.push(make_test_info());
        reg.sync_ephemeral_surfaces(&state);
        assert!(reg.get(SurfaceId(SurfaceId::INFO_BASE)).is_some());
        assert!(reg.get(SurfaceId(SurfaceId::INFO_BASE + 1)).is_some());
        assert!(reg.get(SurfaceId(SurfaceId::INFO_BASE + 2)).is_none());

        // One info removed
        state.infos.pop();
        reg.sync_ephemeral_surfaces(&state);
        assert!(reg.get(SurfaceId(SurfaceId::INFO_BASE)).is_some());
        assert!(reg.get(SurfaceId(SurfaceId::INFO_BASE + 1)).is_none());

        // All infos removed
        state.infos.clear();
        reg.sync_ephemeral_surfaces(&state);
        assert!(reg.get(SurfaceId(SurfaceId::INFO_BASE)).is_none());
    }

    #[test]
    fn test_menu_surface_id() {
        let surface = menu::MenuSurface;
        assert_eq!(surface.id(), SurfaceId::MENU);
    }

    #[test]
    fn test_info_surface_id() {
        let surface = info::InfoSurface::new(0);
        assert_eq!(surface.id(), SurfaceId(SurfaceId::INFO_BASE));
        let surface2 = info::InfoSurface::new(3);
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
}
