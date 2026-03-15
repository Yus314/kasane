use std::path::PathBuf;

use kasane_core::config::PluginsConfig;
use kasane_core::element::{Direction, Element, OverlayAnchor};
use kasane_core::input::{Key, KeyEvent, Modifiers, MouseButton, MouseEvent, MouseEventKind};
use kasane_core::layout::Rect;
use kasane_core::plugin::{
    AnnotateContext, Command, ContribSizeHint, ContributeContext, Contribution, IoEvent,
    OverlayContext, Plugin, PluginId, PluginRegistry, ProcessEvent, SlotId,
};
use kasane_core::protocol::Color;
use kasane_core::render::cache::ViewCache;
use kasane_core::render::view::surface_view_sections_cached;
use kasane_core::state::{AppState, DirtyFlags};
use kasane_core::surface::{
    ResolvedSlotContentKind, SlotKind, SurfaceEvent, SurfacePlacementRequest, SurfaceRegistry,
    ViewContext,
};
use kasane_core::workspace::DockPosition;
use kasane_core::workspace::Workspace;

use crate::WasmPluginLoader;

fn load_cursor_line_plugin() -> crate::WasmPlugin {
    let loader = WasmPluginLoader::new().expect("failed to create loader");
    let bytes = crate::load_wasm_fixture("cursor-line.wasm").expect("failed to load fixture");
    loader
        .load(&bytes, &crate::WasiCapabilityConfig::default())
        .expect("failed to load plugin")
}

fn load_line_numbers_plugin() -> crate::WasmPlugin {
    let loader = WasmPluginLoader::new().expect("failed to create loader");
    let bytes = crate::load_wasm_fixture("line-numbers.wasm").expect("failed to load fixture");
    loader
        .load(&bytes, &crate::WasiCapabilityConfig::default())
        .expect("failed to load plugin")
}

fn load_surface_probe_plugin() -> crate::WasmPlugin {
    let loader = WasmPluginLoader::new().expect("failed to create loader");
    let bytes = crate::load_wasm_fixture("surface-probe.wasm").expect("failed to load fixture");
    loader
        .load(&bytes, &crate::WasiCapabilityConfig::default())
        .expect("failed to load plugin")
}

fn default_annotate_ctx() -> AnnotateContext {
    AnnotateContext {
        line_width: 80,
        gutter_width: 0,
    }
}

fn default_contribute_ctx(state: &AppState) -> ContributeContext {
    ContributeContext::new(state, None)
}

fn default_overlay_ctx() -> OverlayContext {
    OverlayContext {
        screen_cols: 80,
        screen_rows: 24,
        menu_rect: None,
        existing_overlays: vec![],
    }
}

fn default_surface_rect() -> Rect {
    Rect {
        x: 2,
        y: 3,
        w: 30,
        h: 8,
    }
}

struct SurfaceProbeContributor;

impl Plugin for SurfaceProbeContributor {
    fn id(&self) -> PluginId {
        PluginId("surface_probe_contributor".to_string())
    }

    fn contribute_to(
        &self,
        region: &SlotId,
        _state: &AppState,
        ctx: &ContributeContext,
    ) -> Option<Contribution> {
        if region.as_str() != "surface_probe.sidebar.top" {
            return None;
        }

        Some(Contribution {
            element: Element::text(
                format!(
                    "slot-fill:{}x{}",
                    ctx.min_width,
                    ctx.max_width.unwrap_or_default()
                ),
                Default::default(),
            ),
            priority: 0,
            size_hint: ContribSizeHint::Auto,
        })
    }

    fn contribute_deps(&self, region: &SlotId) -> DirtyFlags {
        if region.as_str() == "surface_probe.sidebar.top" {
            DirtyFlags::BUFFER
        } else {
            DirtyFlags::empty()
        }
    }
}

#[test]
fn plugin_id() {
    let plugin = load_cursor_line_plugin();
    assert_eq!(plugin.id().0, "cursor_line");
}

#[test]
fn highlight_active_line() {
    let mut plugin = load_cursor_line_plugin();
    let mut state = AppState::default();
    state.cursor_pos.line = 3;
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);

    let ctx = default_annotate_ctx();
    let ann = plugin.annotate_line_with_ctx(3, &state, &ctx);
    assert!(ann.is_some());
    let ann = ann.unwrap();
    assert!(ann.background.is_some());
    let bg = ann.background.unwrap();
    assert_eq!(
        bg.face.bg,
        Color::Rgb {
            r: 40,
            g: 40,
            b: 50
        }
    );
}

#[test]
fn no_highlight_on_other_lines() {
    let mut plugin = load_cursor_line_plugin();
    let mut state = AppState::default();
    state.cursor_pos.line = 3;
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);

    let ctx = default_annotate_ctx();
    assert!(plugin.annotate_line_with_ctx(0, &state, &ctx).is_none());
    assert!(plugin.annotate_line_with_ctx(2, &state, &ctx).is_none());
    assert!(plugin.annotate_line_with_ctx(4, &state, &ctx).is_none());
}

#[test]
fn tracks_cursor_movement() {
    let mut plugin = load_cursor_line_plugin();
    let mut state = AppState::default();
    let ctx = default_annotate_ctx();

    state.cursor_pos.line = 0;
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);
    assert!(plugin.annotate_line_with_ctx(0, &state, &ctx).is_some());
    assert!(plugin.annotate_line_with_ctx(5, &state, &ctx).is_none());

    state.cursor_pos.line = 5;
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);
    assert!(plugin.annotate_line_with_ctx(0, &state, &ctx).is_none());
    assert!(plugin.annotate_line_with_ctx(5, &state, &ctx).is_some());
}

#[test]
fn state_hash_changes_on_line_change() {
    let mut plugin = load_cursor_line_plugin();
    let h1 = plugin.state_hash();

    let mut state = AppState::default();
    state.cursor_pos.line = 10;
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);
    let h2 = plugin.state_hash();

    assert_ne!(h1, h2);
}

#[test]
fn on_init_and_shutdown_do_not_panic() {
    let mut plugin = load_cursor_line_plugin();
    let state = AppState::default();
    let cmds = plugin.on_init(&state);
    assert!(cmds.is_empty());
    plugin.on_shutdown();
}

// --- cursor-line contribute tests ---

#[test]
fn cursor_line_contribute_returns_none() {
    let mut plugin = load_cursor_line_plugin();
    let state = AppState::default();
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);
    let ctx = default_contribute_ctx(&state);
    // cursor-line plugin has no slot contributions
    assert!(
        plugin
            .contribute_to(&SlotId::BUFFER_LEFT, &state, &ctx)
            .is_none()
    );
}

// --- line-numbers plugin tests ---

#[test]
fn line_numbers_plugin_id() {
    let plugin = load_line_numbers_plugin();
    assert_eq!(plugin.id().0, "wasm_line_numbers");
}

#[test]
fn line_numbers_contribute_buffer_left() {
    let mut plugin = load_line_numbers_plugin();
    let mut state = AppState::default();
    state.lines = vec![vec![], vec![], vec![]]; // 3 lines
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);

    let ctx = default_contribute_ctx(&state);
    let contrib = plugin.contribute_to(&SlotId::BUFFER_LEFT, &state, &ctx);
    assert!(contrib.is_some());

    // Should be a column with 3 children
    match contrib.unwrap().element {
        Element::Flex {
            direction: Direction::Column,
            children,
            ..
        } => {
            assert_eq!(children.len(), 3);
            // Check first child is " 1 "
            match &children[0].element {
                Element::Text(s, _) => assert_eq!(s, " 1 "),
                other => panic!("expected Text, got {other:?}"),
            }
            // Check last child is " 3 "
            match &children[2].element {
                Element::Text(s, _) => assert_eq!(s, " 3 "),
                other => panic!("expected Text, got {other:?}"),
            }
        }
        other => panic!("expected Column Flex, got {other:?}"),
    }
}

#[test]
fn line_numbers_no_contribution_for_other_slots() {
    let mut plugin = load_line_numbers_plugin();
    let mut state = AppState::default();
    state.lines = vec![vec![]];
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);

    let ctx = default_contribute_ctx(&state);
    assert!(
        plugin
            .contribute_to(&SlotId::BUFFER_RIGHT, &state, &ctx)
            .is_none()
    );
    assert!(
        plugin
            .contribute_to(&SlotId::STATUS_LEFT, &state, &ctx)
            .is_none()
    );
}

#[test]
fn line_numbers_empty_buffer_returns_none() {
    let plugin = load_line_numbers_plugin();
    let state = AppState::default();
    let ctx = default_contribute_ctx(&state);
    // default lines is empty
    assert!(
        plugin
            .contribute_to(&SlotId::BUFFER_LEFT, &state, &ctx)
            .is_none()
    );
}

#[test]
fn line_numbers_state_hash_changes_with_line_count() {
    let mut plugin = load_line_numbers_plugin();
    let h1 = plugin.state_hash();

    let mut state = AppState::default();
    state.lines = vec![vec![], vec![]];
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);
    let h2 = plugin.state_hash();

    assert_ne!(h1, h2);
}

#[test]
fn line_numbers_contribute_deps() {
    let plugin = load_line_numbers_plugin();
    // BufferLeft depends on BUFFER
    let deps = plugin.contribute_deps(&SlotId::BUFFER_LEFT);
    assert!(deps.intersects(DirtyFlags::BUFFER));
}

#[test]
fn line_numbers_width_adapts_to_line_count() {
    let mut plugin = load_line_numbers_plugin();
    let mut state = AppState::default();
    // 100 lines → 3-digit width
    state.lines = (0..100).map(|_| vec![]).collect();
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);

    let ctx = default_contribute_ctx(&state);
    let contrib = plugin
        .contribute_to(&SlotId::BUFFER_LEFT, &state, &ctx)
        .unwrap();
    match contrib.element {
        Element::Flex {
            direction: Direction::Column,
            children,
            ..
        } => {
            assert_eq!(children.len(), 100);
            // First line: "  1 " (3 digits padded)
            match &children[0].element {
                Element::Text(s, _) => assert_eq!(s, "  1 "),
                other => panic!("expected Text, got {other:?}"),
            }
            // Line 100: "100 "
            match &children[99].element {
                Element::Text(s, _) => assert_eq!(s, "100 "),
                other => panic!("expected Text, got {other:?}"),
            }
        }
        other => panic!("expected Column Flex, got {other:?}"),
    }
}

// --- hosted surface tests ---

#[test]
fn surface_probe_exposes_hosted_surface_descriptor() {
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
fn surface_probe_renders_abstract_tree_with_placeholder() {
    let mut plugin = load_surface_probe_plugin();
    let mut surfaces = plugin.surfaces();
    let surface = surfaces.pop().expect("expected hosted surface");
    let state = AppState::default();
    let registry = PluginRegistry::new();
    let ctx = ViewContext {
        state: &state,
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
fn hosted_surface_state_hash_tracks_plugin_state() {
    let mut plugin = load_surface_probe_plugin();
    let surfaces = plugin.surfaces();
    let surface = &surfaces[0];
    assert_eq!(surface.state_hash(), 0);

    let mut state = AppState::default();
    state.cursor_pos.line = 7;
    plugin.on_state_changed(&state, DirtyFlags::BUFFER_CURSOR);

    assert_eq!(plugin.state_hash(), 7);
    assert_eq!(surface.state_hash(), 7);
}

#[test]
fn hosted_surface_routes_state_changes_to_guest_and_updates_hash() {
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
fn hosted_surface_integrates_with_surface_registry_and_resolver() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(load_surface_probe_plugin()));
    registry.register(Box::new(SurfaceProbeContributor));

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
    let mut cache = ViewCache::new();
    let mut sections =
        surface_view_sections_cached(&state, &registry, &surface_registry, &mut cache);

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
fn hosted_surface_routes_key_events_to_guest() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(load_surface_probe_plugin()));

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
fn hosted_surface_routes_spawn_session_commands_to_host() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(load_surface_probe_plugin()));

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
fn hosted_surface_routes_close_session_commands_to_host() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(load_surface_probe_plugin()));

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
fn hosted_surface_routes_mouse_and_focus_events_to_guest() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(load_surface_probe_plugin()));

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
fn hosted_surface_state_change_commands_preserve_owner_plugin_source() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(load_surface_probe_plugin()));

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

// --- discover_and_register tests ---

#[test]
fn discover_loads_fixtures_directory() {
    let config = PluginsConfig {
        auto_discover: true,
        path: Some(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures")
                .to_string_lossy()
                .into_owned(),
        ),
        disabled: vec![],
        ..Default::default()
    };
    let mut registry = PluginRegistry::new();
    crate::discover_and_register(&config, &mut registry);

    // Should have loaded both cursor-line.wasm and line-numbers.wasm
    assert!(registry.plugin_count() >= 2, "expected at least 2 plugins");
}

#[test]
fn discover_skips_disabled_plugins() {
    let config = PluginsConfig {
        auto_discover: true,
        path: Some(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures")
                .to_string_lossy()
                .into_owned(),
        ),
        disabled: vec!["cursor_line".to_string()],
        ..Default::default()
    };
    let mut registry = PluginRegistry::new();
    crate::discover_and_register(&config, &mut registry);

    // cursor-line skipped; the remaining fixtures still load.
    assert_eq!(registry.plugin_count(), 5);
}

#[test]
fn discover_does_nothing_when_disabled() {
    let config = PluginsConfig {
        auto_discover: false,
        path: Some(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures")
                .to_string_lossy()
                .into_owned(),
        ),
        disabled: vec![],
        ..Default::default()
    };
    let mut registry = PluginRegistry::new();
    crate::discover_and_register(&config, &mut registry);

    assert_eq!(registry.plugin_count(), 0);
}

#[test]
fn discover_handles_missing_directory() {
    let config = PluginsConfig {
        auto_discover: true,
        path: Some("/nonexistent/path/to/plugins".to_string()),
        disabled: vec![],
        ..Default::default()
    };
    let mut registry = PluginRegistry::new();
    // Should not panic, just silently skip
    crate::discover_and_register(&config, &mut registry);
    assert_eq!(registry.plugin_count(), 0);
}

// --- color-preview plugin tests ---

fn load_color_preview_plugin() -> crate::WasmPlugin {
    let loader = WasmPluginLoader::new().expect("failed to create loader");
    let bytes = crate::load_wasm_fixture("color-preview.wasm").expect("failed to load fixture");
    loader
        .load(&bytes, &crate::WasiCapabilityConfig::default())
        .expect("failed to load plugin")
}

fn make_state_with_lines(lines: &[&str]) -> AppState {
    use kasane_core::protocol::{Atom, Face};
    let mut state = AppState::default();
    state.lines = lines
        .iter()
        .map(|s| {
            vec![Atom {
                face: Face::default(),
                contents: (*s).into(),
            }]
        })
        .collect();
    state.lines_dirty = vec![true; lines.len()];
    state
}

#[test]
fn color_preview_plugin_id() {
    let plugin = load_color_preview_plugin();
    assert_eq!(plugin.id().0, "color_preview");
}

#[test]
fn color_preview_detects_colors_in_line() {
    let mut plugin = load_color_preview_plugin();
    let state = make_state_with_lines(&["#ff0000"]);
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);

    let ctx = default_annotate_ctx();
    let ann = plugin.annotate_line_with_ctx(0, &state, &ctx);
    assert!(ann.is_some());
    let ann = ann.unwrap();
    assert!(ann.left_gutter.is_some());
    assert!(ann.background.is_none());
}

#[test]
fn color_preview_no_decoration_without_colors() {
    let mut plugin = load_color_preview_plugin();
    let state = make_state_with_lines(&["no colors here"]);
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);

    let ctx = default_annotate_ctx();
    assert!(plugin.annotate_line_with_ctx(0, &state, &ctx).is_none());
}

#[test]
fn color_preview_overlay_on_color_line() {
    let mut plugin = load_color_preview_plugin();
    let mut state = make_state_with_lines(&["#3498db"]);
    state.cursor_pos = kasane_core::protocol::Coord { line: 0, column: 0 };
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);

    let ctx = default_overlay_ctx();
    let overlay = plugin.contribute_overlay_with_ctx(&state, &ctx);
    assert!(overlay.is_some());
}

#[test]
fn color_preview_no_overlay_on_plain_line() {
    let mut plugin = load_color_preview_plugin();
    let mut state = make_state_with_lines(&["no colors here", "#ff0000"]);
    state.cursor_pos = kasane_core::protocol::Coord { line: 0, column: 0 };
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);

    let ctx = default_overlay_ctx();
    assert!(plugin.contribute_overlay_with_ctx(&state, &ctx).is_none());
}

#[test]
fn color_preview_state_hash_changes() {
    let mut plugin = load_color_preview_plugin();
    let h1 = plugin.state_hash();

    let state = make_state_with_lines(&["#aabbcc"]);
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);
    let h2 = plugin.state_hash();

    assert_ne!(h1, h2);
}

#[test]
fn color_preview_skips_non_buffer_dirty() {
    let mut plugin = load_color_preview_plugin();
    let h1 = plugin.state_hash();

    let state = make_state_with_lines(&["#aabbcc"]);
    plugin.on_state_changed(&state, DirtyFlags::STATUS);
    let h2 = plugin.state_hash();

    assert_eq!(h1, h2);
}

#[test]
fn color_preview_handle_mouse_increments() {
    use kasane_core::element::InteractiveId;
    use kasane_core::input::{Modifiers, MouseButton, MouseEvent, MouseEventKind};

    let mut plugin = load_color_preview_plugin();
    let mut state = make_state_with_lines(&["#100000"]);
    state.cursor_pos = kasane_core::protocol::Coord { line: 0, column: 0 };
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);

    // R up button: id = 2000 + 0*6 + 0 = 2000
    let event = MouseEvent {
        kind: MouseEventKind::Press(MouseButton::Left),
        line: 0,
        column: 0,
        modifiers: Modifiers::empty(),
    };
    let result = plugin.handle_mouse(&event, InteractiveId(2000), &state);
    assert!(result.is_some());
    let cmds = result.unwrap();
    assert_eq!(cmds.len(), 1);
    // Should be a SendToKakoune command
    match &cmds[0] {
        kasane_core::plugin::Command::SendToKakoune(
            kasane_core::protocol::KasaneRequest::Keys(keys),
        ) => {
            let joined: String = keys.join("");
            assert!(joined.contains("#110000"), "Expected #110000 in: {joined}");
        }
        _ => panic!("Expected SendToKakoune Keys"),
    }
}

#[test]
fn color_preview_handle_mouse_consumes_release() {
    use kasane_core::element::InteractiveId;
    use kasane_core::input::{Modifiers, MouseButton, MouseEvent, MouseEventKind};

    let mut plugin = load_color_preview_plugin();
    let mut state = make_state_with_lines(&["#ff0000"]);
    state.cursor_pos = kasane_core::protocol::Coord { line: 0, column: 0 };
    plugin.on_state_changed(&state, DirtyFlags::BUFFER);

    let event = MouseEvent {
        kind: MouseEventKind::Release(MouseButton::Left),
        line: 0,
        column: 0,
        modifiers: Modifiers::empty(),
    };
    let result = plugin.handle_mouse(&event, InteractiveId(2000), &state);
    assert!(result.is_some());
    assert!(result.unwrap().is_empty());
}

// --- bundled plugin tests ---

#[test]
fn register_bundled_plugins_loads_four() {
    let config = PluginsConfig {
        auto_discover: false,
        path: None,
        disabled: vec![],
        ..Default::default()
    };
    let mut registry = PluginRegistry::new();
    crate::register_bundled_plugins(&config, &mut registry);

    assert_eq!(registry.plugin_count(), 4);
}

#[test]
fn register_bundled_plugins_respects_disabled() {
    let config = PluginsConfig {
        auto_discover: false,
        path: None,
        disabled: vec!["color_preview".to_string()],
        ..Default::default()
    };
    let mut registry = PluginRegistry::new();
    crate::register_bundled_plugins(&config, &mut registry);

    assert_eq!(registry.plugin_count(), 3);
}

#[test]
fn filesystem_plugin_overrides_bundled() {
    let config = PluginsConfig {
        auto_discover: false,
        path: None,
        disabled: vec![],
        ..Default::default()
    };
    let mut registry = PluginRegistry::new();
    crate::register_bundled_plugins(&config, &mut registry);
    assert_eq!(registry.plugin_count(), 4);

    // Register another plugin with the same ID
    let loader = WasmPluginLoader::new().unwrap();
    let bytes = crate::load_wasm_fixture("cursor-line.wasm").unwrap();
    let plugin = loader
        .load(&bytes, &crate::WasiCapabilityConfig::default())
        .unwrap();
    assert_eq!(plugin.id().0, "cursor_line");
    registry.register(Box::new(plugin));

    // Should still be 4, not 5 (replaced, not added)
    assert_eq!(registry.plugin_count(), 4);
}

#[test]
fn sdk_wit_matches_host_wit() {
    let host_wit = include_str!("../wit/plugin.wit");
    let sdk_wit = include_str!("../../kasane-plugin-sdk/wit/plugin.wit");
    assert_eq!(
        host_wit, sdk_wit,
        "SDK WIT and host WIT are out of sync — update kasane-plugin-sdk/wit/plugin.wit"
    );
}

// --- fuzzy-finder plugin tests (Phase P-3) ---

fn fuzzy_finder_wasi_config() -> crate::WasiCapabilityConfig {
    crate::WasiCapabilityConfig {
        data_base_dir: std::env::temp_dir().join("kasane_test_fuzzy_finder"),
        ..Default::default()
    }
}

fn load_fuzzy_finder_plugin() -> crate::WasmPlugin {
    let loader = WasmPluginLoader::new().expect("failed to create loader");
    let bytes = crate::load_wasm_fixture("fuzzy-finder.wasm").expect("failed to load fixture");
    let config = fuzzy_finder_wasi_config();
    loader.load(&bytes, &config).expect("failed to load plugin")
}

fn load_fuzzy_finder_plugin_with_config(config: &crate::WasiCapabilityConfig) -> crate::WasmPlugin {
    let loader = WasmPluginLoader::new().expect("failed to create loader");
    let bytes = crate::load_wasm_fixture("fuzzy-finder.wasm").expect("failed to load fixture");
    loader.load(&bytes, config).expect("failed to load plugin")
}

fn ctrl_p_event() -> KeyEvent {
    KeyEvent {
        key: Key::Char('p'),
        modifiers: Modifiers::CTRL,
    }
}

fn char_event(c: char) -> KeyEvent {
    KeyEvent {
        key: Key::Char(c),
        modifiers: Modifiers::empty(),
    }
}

fn key_event(key: Key) -> KeyEvent {
    KeyEvent {
        key,
        modifiers: Modifiers::empty(),
    }
}

#[test]
fn fuzzy_finder_plugin_id() {
    let plugin = load_fuzzy_finder_plugin();
    assert_eq!(plugin.id().0, "fuzzy_finder");
}

#[test]
fn fuzzy_finder_requests_process_capability() {
    let plugin = load_fuzzy_finder_plugin();
    assert!(plugin.allows_process_spawn());
}

#[test]
fn fuzzy_finder_process_denied_by_config() {
    use std::collections::HashMap;
    let mut config = fuzzy_finder_wasi_config();
    config.deny_capabilities =
        HashMap::from([("fuzzy_finder".to_string(), vec!["process".to_string()])]);
    let plugin = load_fuzzy_finder_plugin_with_config(&config);
    assert!(!plugin.allows_process_spawn());
}

#[test]
fn fuzzy_finder_inactive_passes_keys() {
    let mut plugin = load_fuzzy_finder_plugin();
    let state = AppState::default();

    // Regular keys pass through when inactive
    let result = plugin.handle_key(&char_event('a'), &state);
    assert!(result.is_none());

    let result = plugin.handle_key(&key_event(Key::Enter), &state);
    assert!(result.is_none());
}

#[test]
fn fuzzy_finder_ctrl_p_returns_spawn_command() {
    let mut plugin = load_fuzzy_finder_plugin();
    let state = AppState::default();

    let result = plugin.handle_key(&ctrl_p_event(), &state);
    assert!(result.is_some());
    let cmds = result.unwrap();

    // Should contain a SpawnProcess command (for fd)
    let has_spawn = cmds
        .iter()
        .any(|c| matches!(c, Command::SpawnProcess { .. }));
    assert!(has_spawn, "expected SpawnProcess in commands");
}

#[test]
fn fuzzy_finder_consumes_keys_when_active() {
    let mut plugin = load_fuzzy_finder_plugin();
    let state = AppState::default();

    // Activate
    plugin.handle_key(&ctrl_p_event(), &state);

    // All keys should be consumed (Some, not None)
    let result = plugin.handle_key(&char_event('a'), &state);
    assert!(result.is_some());

    let result = plugin.handle_key(&key_event(Key::Escape), &state);
    assert!(result.is_some());
}

#[test]
fn fuzzy_finder_escape_deactivates() {
    let mut plugin = load_fuzzy_finder_plugin();
    let state = AppState::default();

    // Activate
    plugin.handle_key(&ctrl_p_event(), &state);

    // Escape
    plugin.handle_key(&key_event(Key::Escape), &state);

    // Should be inactive again — keys pass through
    let result = plugin.handle_key(&char_event('a'), &state);
    assert!(result.is_none());
}

#[test]
fn fuzzy_finder_io_event_stdout_accumulation() {
    let mut plugin = load_fuzzy_finder_plugin();
    let state = AppState::default();

    // Activate (spawns fd with job_id=1)
    plugin.handle_key(&ctrl_p_event(), &state);
    let h1 = plugin.state_hash();

    // Simulate fd stdout in chunks
    plugin.on_io_event(
        &IoEvent::Process(ProcessEvent::Stdout {
            job_id: 1,
            data: b"file1.rs\nfile2.rs\n".to_vec(),
        }),
        &state,
    );
    plugin.on_io_event(
        &IoEvent::Process(ProcessEvent::Stdout {
            job_id: 1,
            data: b"file3.rs\n".to_vec(),
        }),
        &state,
    );

    // Simulate fd exit
    let cmds = plugin.on_io_event(
        &IoEvent::Process(ProcessEvent::Exited {
            job_id: 1,
            exit_code: 0,
        }),
        &state,
    );

    let h2 = plugin.state_hash();
    assert_ne!(h1, h2, "state_hash should change after receiving file list");

    // Should request redraw
    let has_redraw = cmds.iter().any(|c| matches!(c, Command::RequestRedraw(_)));
    assert!(has_redraw, "expected RequestRedraw after fd exit");

    // Overlay should now show results
    let ctx = default_overlay_ctx();
    let overlay = plugin.contribute_overlay_with_ctx(&state, &ctx);
    assert!(
        overlay.is_some(),
        "overlay should be visible after file list received"
    );
}

#[test]
fn fuzzy_finder_overlay_uses_absolute_anchor() {
    let mut plugin = load_fuzzy_finder_plugin();
    let state = AppState::default();

    plugin.handle_key(&ctrl_p_event(), &state);
    plugin.on_io_event(
        &IoEvent::Process(ProcessEvent::Stdout {
            job_id: 1,
            data: b"src/main.rs\nsrc/lib.rs\n".to_vec(),
        }),
        &state,
    );
    plugin.on_io_event(
        &IoEvent::Process(ProcessEvent::Exited {
            job_id: 1,
            exit_code: 0,
        }),
        &state,
    );

    let overlay = plugin
        .contribute_overlay_with_ctx(&state, &default_overlay_ctx())
        .expect("expected overlay");
    match overlay.anchor {
        OverlayAnchor::Absolute { w, h, .. } => {
            assert!(w > 0);
            assert!(h > 0);
        }
        other => panic!("expected absolute overlay anchor, got {other:?}"),
    }
}

#[test]
fn fuzzy_finder_spawn_failed_no_panic() {
    let mut plugin = load_fuzzy_finder_plugin();
    let state = AppState::default();

    // Activate
    plugin.handle_key(&ctrl_p_event(), &state);

    // fd fails → should try find fallback
    let cmds = plugin.on_io_event(
        &IoEvent::Process(ProcessEvent::SpawnFailed {
            job_id: 1,
            error: "not found".to_string(),
        }),
        &state,
    );

    // Should have spawned find as fallback
    let has_spawn = cmds
        .iter()
        .any(|c| matches!(c, Command::SpawnProcess { .. }));
    assert!(has_spawn, "expected find fallback SpawnProcess");

    // find also fails
    let cmds = plugin.on_io_event(
        &IoEvent::Process(ProcessEvent::SpawnFailed {
            job_id: 2,
            error: "not found".to_string(),
        }),
        &state,
    );

    // Should show error overlay without panicking
    let has_redraw = cmds.iter().any(|c| matches!(c, Command::RequestRedraw(_)));
    assert!(has_redraw);

    let ctx = default_overlay_ctx();
    let overlay = plugin.contribute_overlay_with_ctx(&state, &ctx);
    assert!(overlay.is_some(), "overlay should show error");
}

#[test]
fn fuzzy_finder_fzf_spawn_failed_shows_error() {
    let mut plugin = load_fuzzy_finder_plugin();
    let state = AppState::default();

    // Activate and provide file list
    plugin.handle_key(&ctrl_p_event(), &state);
    plugin.on_io_event(
        &IoEvent::Process(ProcessEvent::Stdout {
            job_id: 1,
            data: b"file1.rs\n".to_vec(),
        }),
        &state,
    );
    plugin.on_io_event(
        &IoEvent::Process(ProcessEvent::Exited {
            job_id: 1,
            exit_code: 0,
        }),
        &state,
    );

    // Type a character to trigger fzf
    plugin.handle_key(&char_event('f'), &state);

    // fzf spawn fails (job_id = 100 + 1 = 101)
    plugin.on_io_event(
        &IoEvent::Process(ProcessEvent::SpawnFailed {
            job_id: 101,
            error: "fzf not installed".to_string(),
        }),
        &state,
    );

    // Should still show overlay (with error)
    let ctx = default_overlay_ctx();
    let overlay = plugin.contribute_overlay_with_ctx(&state, &ctx);
    assert!(overlay.is_some());
}

#[test]
fn fuzzy_finder_enter_selects_file() {
    let mut plugin = load_fuzzy_finder_plugin();
    let state = AppState::default();

    // Activate and provide file list
    plugin.handle_key(&ctrl_p_event(), &state);
    plugin.on_io_event(
        &IoEvent::Process(ProcessEvent::Stdout {
            job_id: 1,
            data: b"src/main.rs\nsrc/lib.rs\n".to_vec(),
        }),
        &state,
    );
    plugin.on_io_event(
        &IoEvent::Process(ProcessEvent::Exited {
            job_id: 1,
            exit_code: 0,
        }),
        &state,
    );

    // Press Enter to select first file
    let cmds = plugin.handle_key(&key_event(Key::Enter), &state);
    assert!(cmds.is_some());
    let cmds = cmds.unwrap();

    // Should contain SendKeys with edit command
    let has_send_keys = cmds.iter().any(|c| {
        matches!(c, Command::SendToKakoune(kasane_core::protocol::KasaneRequest::Keys(keys)) if keys.iter().any(|k| k.contains("e") || k.contains("d") || k.contains("i") || k.contains("t")))
    });
    assert!(has_send_keys, "expected SendToKakoune with edit keys");

    // Should be inactive after Enter
    let result = plugin.handle_key(&char_event('a'), &state);
    assert!(result.is_none(), "should be inactive after Enter");
}

#[test]
fn fuzzy_finder_up_down_navigation() {
    let mut plugin = load_fuzzy_finder_plugin();
    let state = AppState::default();

    // Activate and provide file list
    plugin.handle_key(&ctrl_p_event(), &state);
    plugin.on_io_event(
        &IoEvent::Process(ProcessEvent::Stdout {
            job_id: 1,
            data: b"a.rs\nb.rs\nc.rs\n".to_vec(),
        }),
        &state,
    );
    plugin.on_io_event(
        &IoEvent::Process(ProcessEvent::Exited {
            job_id: 1,
            exit_code: 0,
        }),
        &state,
    );

    let h1 = plugin.state_hash();

    // Down
    plugin.handle_key(&key_event(Key::Down), &state);
    let h2 = plugin.state_hash();
    assert_ne!(h1, h2, "state_hash should change on Down");

    // Down again
    plugin.handle_key(&key_event(Key::Down), &state);
    let h3 = plugin.state_hash();
    assert_ne!(h2, h3);

    // Up
    plugin.handle_key(&key_event(Key::Up), &state);
    let h4 = plugin.state_hash();
    assert_ne!(h3, h4);
}

#[test]
fn fuzzy_finder_discover_loads_with_fixtures() {
    // When discover scans fixtures/, fuzzy-finder.wasm should load
    let config = PluginsConfig {
        auto_discover: true,
        path: Some(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures")
                .to_string_lossy()
                .into_owned(),
        ),
        disabled: vec![],
        ..Default::default()
    };
    let mut registry = PluginRegistry::new();
    crate::discover_and_register(&config, &mut registry);

    // Should now include fuzzy_finder among loaded plugins
    assert!(
        registry.plugin_count() >= 5,
        "expected at least 5 plugins (including fuzzy_finder)"
    );
}
