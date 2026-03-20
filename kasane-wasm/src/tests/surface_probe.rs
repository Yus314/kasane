use super::*;

#[test]
fn exposes_hosted_surface_descriptor() {
    let mut plugin = load_surface_probe_plugin();
    let surfaces = plugin.surfaces();
    assert_eq!(surfaces.len(), 1);

    let surface = &surfaces[0];
    assert_eq!(surface.surface_key().as_str(), "surface_probe.sidebar");
    assert_eq!(surface.size_hint().min_width, 12);
    assert_eq!(surface.size_hint().preferred_width, Some(24));
    assert_eq!(
        surface.initial_placement(),
        Some(SurfacePlacementRequest::Dock(DockPosition::Left))
    );
    assert_eq!(surface.declared_slots().len(), 1);
    assert_eq!(
        surface.declared_slots()[0].name.as_str(),
        "surface_probe.sidebar.top"
    );
    assert_eq!(surface.declared_slots()[0].kind, SlotKind::AboveBand);
}

#[test]
fn renders_abstract_tree_with_placeholder() {
    let mut plugin = load_surface_probe_plugin();
    let mut surfaces = plugin.surfaces();
    let surface = surfaces.pop().expect("expected hosted surface");
    let state = AppState::default();
    let registry = PluginRegistry::new();
    let ctx = ViewContext {
        state: &state,
        global_state: &state,
        rect: default_surface_rect(),
        focused: true,
        registry: &registry,
        surface_id: surface.id(),
    };

    match surface.view(&ctx) {
        Element::Flex {
            direction: Direction::Column,
            children,
            ..
        } => {
            assert_eq!(children.len(), 2);
            match &children[0].element {
                Element::Text(label, _) => assert_eq!(label, "surface-probe:30x8:focused"),
                other => panic!("expected title text, got {other:?}"),
            }
            match &children[1].element {
                Element::SlotPlaceholder {
                    slot_name,
                    direction,
                    gap,
                } => {
                    assert_eq!(slot_name, "surface_probe.sidebar.top");
                    assert_eq!(*direction, Direction::Column);
                    assert_eq!(*gap, 1);
                }
                other => panic!("expected slot placeholder, got {other:?}"),
            }
        }
        other => panic!("expected column surface root, got {other:?}"),
    }
}

#[test]
fn state_hash_tracks_plugin_state() {
    let mut plugin = load_surface_probe_plugin();
    let surfaces = plugin.surfaces();
    let surface = &surfaces[0];
    assert_eq!(surface.state_hash(), 0);

    let mut state = AppState::default();
    state.cursor_pos.line = 7;
    let effects = plugin.on_state_changed_effects(&state, DirtyFlags::BUFFER_CURSOR);
    assert!(effects.redraw.is_empty());
    assert!(effects.commands.is_empty());
    assert!(effects.scroll_plans.is_empty());

    assert_eq!(plugin.state_hash(), 7);
    assert_eq!(surface.state_hash(), 7);
}

#[test]
fn routes_state_changes_to_guest_and_updates_hash() {
    let mut plugin = load_surface_probe_plugin();
    let mut surfaces = plugin.surfaces();
    let mut surface = surfaces.pop().expect("expected hosted surface");

    let mut state = AppState::default();
    state.cursor_pos.line = 11;

    let commands = surface.on_state_changed(&state, DirtyFlags::BUFFER_CURSOR);
    assert_eq!(commands.len(), 1);
    assert!(matches!(
        commands[0],
        Command::RequestRedraw(flags) if flags == DirtyFlags::BUFFER_CURSOR
    ));
    assert_eq!(surface.state_hash(), 11);
}

#[test]
fn integrates_with_surface_registry_and_resolver() {
    let mut registry = PluginRegistry::new();
    registry.register_backend(Box::new(load_surface_probe_plugin()));
    registry.register_backend(Box::new(SurfaceProbeContributor));

    let mut surface_sets = registry.collect_plugin_surfaces();
    assert_eq!(surface_sets.len(), 1);
    let mut surfaces = surface_sets.pop().unwrap().surfaces;
    assert_eq!(surfaces.len(), 1);

    let hosted_surface = surfaces.pop().unwrap();
    let hosted_id = hosted_surface.id();
    let mut surface_registry = SurfaceRegistry::with_workspace(Workspace::new(hosted_id));
    surface_registry
        .try_register(hosted_surface)
        .expect("hosted surface should register");

    let state = AppState::default();
    let root_area = Rect {
        x: 0,
        y: 0,
        w: state.cols,
        h: state.rows,
    };
    let mut sections = surface_registry.compose_view_sections(&state, None, &registry, root_area);

    assert_eq!(sections.surface_reports.len(), 1);
    let report = &sections.surface_reports[0];
    assert!(report.owner_errors.is_empty());
    assert!(report.contributor_issues.is_empty());
    assert!(report.absent_declared_slots.is_empty());
    assert_eq!(report.slot_records.len(), 1);
    assert_eq!(
        report.slot_records[0].slot_name.as_str(),
        "surface_probe.sidebar.top"
    );
    assert_eq!(report.slot_records[0].contribution_count, 1);
    assert_eq!(
        report.slot_records[0].content_kind,
        ResolvedSlotContentKind::Single
    );

    match &sections.base {
        Element::Flex {
            direction: Direction::Column,
            children,
            ..
        } => {
            assert_eq!(children.len(), 2);
            match &children[0].element {
                Element::Text(label, _) => assert!(label.starts_with("surface-probe:")),
                other => panic!("expected surface title, got {other:?}"),
            }
            match &children[1].element {
                Element::ResolvedSlot {
                    slot_name,
                    direction,
                    children,
                    ..
                } => {
                    assert_eq!(slot_name, "surface_probe.sidebar.top");
                    assert_eq!(*direction, Direction::Column);
                    assert_eq!(children.len(), 1);
                    match &children[0].element {
                        Element::Text(label, _) => assert!(label.starts_with("slot-fill:")),
                        other => panic!("expected contributed text, got {other:?}"),
                    }
                }
                other => panic!("expected resolved slot, got {other:?}"),
            }
        }
        other => panic!("expected column base, got {other:?}"),
    }

    let root_area = Rect {
        x: 0,
        y: 0,
        w: state.cols,
        h: state.rows,
    };
    let layout = kasane_core::layout::flex::place(&sections.base, root_area, &state);
    kasane_core::surface::resolve::backfill_surface_report_areas(
        &mut sections.surface_reports,
        &sections.base,
        &layout,
    );
    assert!(sections.surface_reports[0].slot_records[0].area.is_some());
}

#[test]
fn routes_key_events_to_guest() {
    let mut registry = PluginRegistry::new();
    registry.register_backend(Box::new(load_surface_probe_plugin()));

    let mut surface_sets = registry.collect_plugin_surfaces();
    let mut surfaces = surface_sets.pop().unwrap().surfaces;
    let hosted_surface = surfaces.pop().unwrap();
    let hosted_id = hosted_surface.id();
    let mut surface_registry = SurfaceRegistry::with_workspace(Workspace::new(hosted_id));
    surface_registry
        .try_register(hosted_surface)
        .expect("hosted surface should register");

    let mut state = AppState::default();
    state.cols = 80;
    state.rows = 24;
    let commands = surface_registry.route_event(
        SurfaceEvent::Key(KeyEvent {
            key: Key::Char('r'),
            modifiers: Modifiers::empty(),
        }),
        &state,
        Rect {
            x: 0,
            y: 0,
            w: state.cols,
            h: state.rows,
        },
    );
    assert_eq!(commands.len(), 1);
    assert!(matches!(
        commands[0],
        Command::RequestRedraw(flags) if flags == DirtyFlags::BUFFER_CURSOR
    ));
}

#[test]
fn routes_spawn_session_commands_to_host() {
    let mut registry = PluginRegistry::new();
    registry.register_backend(Box::new(load_surface_probe_plugin()));

    let mut surface_sets = registry.collect_plugin_surfaces();
    let surface_set = surface_sets.pop().expect("expected hosted surface set");
    let owner = surface_set.owner.clone();
    let mut surfaces = surface_set.surfaces;
    let hosted_surface = surfaces.pop().expect("expected hosted surface");
    let hosted_id = hosted_surface.id();
    let mut surface_registry = SurfaceRegistry::with_workspace(Workspace::new(hosted_id));
    surface_registry
        .try_register_for_owner(hosted_surface, Some(owner.clone()))
        .expect("hosted surface should register");

    let mut state = AppState::default();
    state.cols = 80;
    state.rows = 24;
    let commands = surface_registry.route_event_with_sources(
        SurfaceEvent::Key(KeyEvent {
            key: Key::Char('n'),
            modifiers: Modifiers::empty(),
        }),
        &state,
        Rect {
            x: 0,
            y: 0,
            w: state.cols,
            h: state.rows,
        },
    );

    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].source_plugin.as_ref(), Some(&owner));
    assert!(matches!(
        commands[0].commands.as_slice(),
        [Command::Session(kasane_core::session::SessionCommand::Spawn {
            key,
            session,
            args,
            activate,
        })]
            if key.as_deref() == Some("surface-probe.spawned")
                && session.as_deref() == Some("surface-probe")
                && args == &vec!["README.md".to_string()]
                && *activate
    ));
}

#[test]
fn routes_close_session_commands_to_host() {
    let mut registry = PluginRegistry::new();
    registry.register_backend(Box::new(load_surface_probe_plugin()));

    let mut surface_sets = registry.collect_plugin_surfaces();
    let surface_set = surface_sets.pop().expect("expected hosted surface set");
    let owner = surface_set.owner.clone();
    let mut surfaces = surface_set.surfaces;
    let hosted_surface = surfaces.pop().expect("expected hosted surface");
    let hosted_id = hosted_surface.id();
    let mut surface_registry = SurfaceRegistry::with_workspace(Workspace::new(hosted_id));
    surface_registry
        .try_register_for_owner(hosted_surface, Some(owner.clone()))
        .expect("hosted surface should register");

    let mut state = AppState::default();
    state.cols = 80;
    state.rows = 24;
    let commands = surface_registry.route_event_with_sources(
        SurfaceEvent::Key(KeyEvent {
            key: Key::Char('x'),
            modifiers: Modifiers::empty(),
        }),
        &state,
        Rect {
            x: 0,
            y: 0,
            w: state.cols,
            h: state.rows,
        },
    );

    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].source_plugin.as_ref(), Some(&owner));
    assert!(matches!(
        commands[0].commands.as_slice(),
        [Command::Session(kasane_core::session::SessionCommand::Close { key })]
            if key.is_none()
    ));
}

#[test]
fn routes_mouse_and_focus_events_to_guest() {
    let mut registry = PluginRegistry::new();
    registry.register_backend(Box::new(load_surface_probe_plugin()));

    let mut surface_sets = registry.collect_plugin_surfaces();
    let mut surfaces = surface_sets.pop().unwrap().surfaces;
    let hosted_surface = surfaces.pop().unwrap();
    let hosted_id = hosted_surface.id();
    let mut surface_registry = SurfaceRegistry::with_workspace(Workspace::new(hosted_id));
    surface_registry
        .try_register(hosted_surface)
        .expect("hosted surface should register");

    let mut state = AppState::default();
    state.cols = 80;
    state.rows = 24;
    let total = Rect {
        x: 0,
        y: 0,
        w: state.cols,
        h: state.rows,
    };

    let mouse_commands = surface_registry.route_event(
        SurfaceEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Press(MouseButton::Left),
            line: 3,
            column: 4,
            modifiers: Modifiers::empty(),
        }),
        &state,
        total,
    );
    assert_eq!(mouse_commands.len(), 1);
    assert!(matches!(
        mouse_commands[0],
        Command::RequestRedraw(flags) if flags == DirtyFlags::INFO
    ));

    let focus_commands = surface_registry.route_event(SurfaceEvent::FocusGained, &state, total);
    assert_eq!(focus_commands.len(), 1);
    assert!(matches!(
        focus_commands[0],
        Command::RequestRedraw(flags) if flags == DirtyFlags::STATUS
    ));

    let resize_commands = surface_registry.route_event(SurfaceEvent::Resize(total), &state, total);
    assert_eq!(resize_commands.len(), 1);
    assert!(matches!(
        resize_commands[0],
        Command::RequestRedraw(flags) if flags == DirtyFlags::MENU
    ));
}

#[test]
fn state_change_commands_preserve_owner_plugin_source() {
    let mut registry = PluginRegistry::new();
    registry.register_backend(Box::new(load_surface_probe_plugin()));

    let mut surface_sets = registry.collect_plugin_surfaces();
    let surface_set = surface_sets.pop().expect("expected hosted surface set");
    let owner = surface_set.owner.clone();
    let mut surfaces = surface_set.surfaces;
    let hosted_surface = surfaces.pop().expect("expected hosted surface");
    let hosted_id = hosted_surface.id();
    let mut surface_registry = SurfaceRegistry::with_workspace(Workspace::new(hosted_id));
    surface_registry
        .try_register_for_owner(hosted_surface, Some(owner.clone()))
        .expect("hosted surface should register");

    let mut state = AppState::default();
    state.cursor_pos.line = 5;
    let batches = surface_registry.on_state_changed_with_sources(&state, DirtyFlags::BUFFER_CURSOR);
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].source_plugin, Some(owner));
    assert_eq!(batches[0].commands.len(), 1);
    assert!(matches!(
        batches[0].commands[0],
        Command::RequestRedraw(flags) if flags == DirtyFlags::BUFFER_CURSOR
    ));
}
