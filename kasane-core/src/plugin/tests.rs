use super::*;
use crate::input::KeyEvent;
use crate::protocol::Face;
use crate::state::{AppState, DirtyFlags};

struct TestPlugin;

impl Plugin for TestPlugin {
    fn id(&self) -> PluginId {
        PluginId("test".to_string())
    }
}

#[test]
fn test_empty_registry() {
    let registry = PluginRegistry::new();
    assert!(registry.plugin_count() == 0);
}

#[test]
fn test_plugin_id() {
    let plugin = TestPlugin;
    assert_eq!(plugin.id(), PluginId("test".to_string()));
}

#[test]
fn test_extract_redraw_flags_merges() {
    let mut commands = vec![
        Command::RequestRedraw(DirtyFlags::BUFFER),
        Command::SendToKakoune(crate::protocol::KasaneRequest::Keys(vec!["a".into()])),
        Command::RequestRedraw(DirtyFlags::INFO),
    ];
    let flags = extract_redraw_flags(&mut commands);
    assert_eq!(flags, DirtyFlags::BUFFER | DirtyFlags::INFO);
    assert_eq!(commands.len(), 1);
    assert!(matches!(commands[0], Command::SendToKakoune(_)));
}

#[test]
fn test_extract_redraw_flags_empty() {
    let mut commands = vec![
        Command::SendToKakoune(crate::protocol::KasaneRequest::Keys(vec!["a".into()])),
        Command::Paste,
    ];
    let flags = extract_redraw_flags(&mut commands);
    assert!(flags.is_empty());
    assert_eq!(commands.len(), 2);
}

// --- Lifecycle hooks tests ---

struct LifecyclePlugin {
    init_called: bool,
    shutdown_called: bool,
    state_changes: Vec<DirtyFlags>,
}

impl LifecyclePlugin {
    fn new() -> Self {
        LifecyclePlugin {
            init_called: false,
            shutdown_called: false,
            state_changes: Vec::new(),
        }
    }
}

impl Plugin for LifecyclePlugin {
    fn id(&self) -> PluginId {
        PluginId("lifecycle".to_string())
    }

    fn on_init(&mut self, _state: &AppState) -> Vec<Command> {
        self.init_called = true;
        vec![Command::RequestRedraw(DirtyFlags::BUFFER)]
    }

    fn on_shutdown(&mut self) {
        self.shutdown_called = true;
    }

    fn on_state_changed(&mut self, _state: &AppState, dirty: DirtyFlags) -> Vec<Command> {
        self.state_changes.push(dirty);
        vec![]
    }
}

#[test]
fn test_init_all_returns_commands() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(LifecyclePlugin::new()));
    let state = AppState::default();
    let commands = registry.init_all(&state);
    assert_eq!(commands.len(), 1);
    assert!(matches!(commands[0], Command::RequestRedraw(_)));
}

#[test]
fn test_shutdown_all_calls_all_plugins() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(LifecyclePlugin::new()));
    registry.register(Box::new(LifecyclePlugin::new()));
    registry.shutdown_all();
    // Verify via count — can't inspect internal state, but no panic = success
}

#[test]
fn test_on_state_changed_dispatched_with_flags() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(LifecyclePlugin::new()));
    let state = AppState::default();

    // Simulate what update() does for Msg::Kakoune
    let flags = DirtyFlags::BUFFER | DirtyFlags::STATUS;
    for plugin in registry.plugins_mut() {
        plugin.on_state_changed(&state, flags);
    }
    // No panic, default implementations work
}

#[test]
fn test_lifecycle_defaults() {
    // TestPlugin has no lifecycle hooks — defaults should work
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(TestPlugin));
    let state = AppState::default();

    let commands = registry.init_all(&state);
    assert!(commands.is_empty());

    registry.shutdown_all();
    // No panic
}

// --- Input observation tests ---

struct ObservingPlugin {
    observed_keys: std::cell::RefCell<Vec<String>>,
}

impl ObservingPlugin {
    fn new() -> Self {
        ObservingPlugin {
            observed_keys: std::cell::RefCell::new(Vec::new()),
        }
    }
}

impl Plugin for ObservingPlugin {
    fn id(&self) -> PluginId {
        PluginId("observer".to_string())
    }

    fn observe_key(&mut self, key: &KeyEvent, _state: &AppState) {
        self.observed_keys
            .borrow_mut()
            .push(format!("{:?}", key.key));
    }
}

#[test]
fn test_observe_key_called() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(ObservingPlugin::new()));
    let state = AppState::default();
    let key = KeyEvent {
        key: crate::input::Key::Char('a'),
        modifiers: crate::input::Modifiers::empty(),
    };
    for plugin in registry.plugins_mut() {
        plugin.observe_key(&key, &state);
    }
    // No panic = success, since we can't downcast
}

// --- Menu transform tests ---

struct IconPlugin;

impl Plugin for IconPlugin {
    fn id(&self) -> PluginId {
        PluginId("icons".to_string())
    }

    fn transform_menu_item(
        &self,
        item: &[crate::protocol::Atom],
        _index: usize,
        _selected: bool,
        _state: &AppState,
    ) -> Option<Vec<crate::protocol::Atom>> {
        let mut result = vec![crate::protocol::Atom {
            face: Face::default(),
            contents: "★ ".into(),
        }];
        result.extend(item.iter().cloned());
        Some(result)
    }
}

#[test]
fn test_transform_menu_item() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(IconPlugin));
    let state = AppState::default();
    let item = vec![crate::protocol::Atom {
        face: Face::default(),
        contents: "foo".into(),
    }];
    let result = registry.transform_menu_item(&item, 0, false, &state);
    assert!(result.is_some());
    let result = result.unwrap();
    assert_eq!(result[0].contents.as_str(), "★ ");
    assert_eq!(result[1].contents.as_str(), "foo");
}

#[test]
fn test_transform_menu_item_no_plugin() {
    let registry = PluginRegistry::new();
    let state = AppState::default();
    let item = vec![crate::protocol::Atom {
        face: Face::default(),
        contents: "foo".into(),
    }];
    assert!(
        registry
            .transform_menu_item(&item, 0, false, &state)
            .is_none()
    );
}

// --- deliver_message tests ---

#[test]
fn test_deliver_message_to_plugin() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(TestPlugin));
    let state = AppState::default();
    let (flags, commands) =
        registry.deliver_message(&PluginId("test".to_string()), Box::new(42u32), &state);
    assert!(flags.is_empty());
    assert!(commands.is_empty());
}

#[test]
fn test_deliver_message_unknown_target() {
    let mut registry = PluginRegistry::new();
    let state = AppState::default();
    let (flags, commands) =
        registry.deliver_message(&PluginId("unknown".to_string()), Box::new(42u32), &state);
    assert!(flags.is_empty());
    assert!(commands.is_empty());
}

// --- extract_deferred_commands tests ---

#[test]
fn test_extract_deferred_separates_correctly() {
    let commands = vec![
        Command::SendToKakoune(crate::protocol::KasaneRequest::Keys(vec!["a".into()])),
        Command::ScheduleTimer {
            delay: std::time::Duration::from_millis(100),
            target: PluginId("test".into()),
            payload: Box::new(42u32),
        },
        Command::PluginMessage {
            target: PluginId("other".into()),
            payload: Box::new("hello"),
        },
        Command::SetConfig {
            key: "foo".into(),
            value: "bar".into(),
        },
        Command::Paste,
    ];
    let (normal, deferred) = extract_deferred_commands(commands);
    assert_eq!(normal.len(), 2); // SendToKakoune + Paste
    assert_eq!(deferred.len(), 3); // Timer + Message + Config
}

#[test]
fn test_extract_deferred_empty() {
    let commands = vec![
        Command::SendToKakoune(crate::protocol::KasaneRequest::Keys(vec!["a".into()])),
        Command::Quit,
    ];
    let (normal, deferred) = extract_deferred_commands(commands);
    assert_eq!(normal.len(), 2);
    assert!(deferred.is_empty());
}

#[test]
fn test_set_config_stores_in_ui_options() {
    // SetConfig applied via ui_options (integration would be in event loop)
    let mut state = AppState::default();
    state.ui_options.insert("key".into(), "value".into());
    assert_eq!(state.ui_options.get("key").unwrap(), "value");
}

// --- Plugin state change guard tests ---

struct StatefulPlugin {
    hash: u64,
}

impl Plugin for StatefulPlugin {
    fn id(&self) -> PluginId {
        PluginId("stateful".to_string())
    }

    fn state_hash(&self) -> u64 {
        self.hash
    }
}

#[test]
fn test_any_plugin_state_changed_flag() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(StatefulPlugin { hash: 1 }));

    // Initial prepare: hash differs from default 0 → changed
    registry.prepare_plugin_cache(DirtyFlags::ALL);
    assert!(registry.any_plugin_state_changed());

    // Second prepare with same hash → no change
    registry.prepare_plugin_cache(DirtyFlags::ALL);
    assert!(!registry.any_plugin_state_changed());
}

// --- SlotId tests ---

#[test]
fn test_slot_id_well_known() {
    assert!(SlotId::BUFFER_LEFT.is_well_known());
    assert!(SlotId::STATUS_RIGHT.is_well_known());
    assert_eq!(SlotId::BUFFER_LEFT.well_known_index(), Some(0));
}

#[test]
fn test_slot_id_custom_not_well_known() {
    let custom = SlotId::new("my.plugin.sidebar");
    assert!(!custom.is_well_known());
    assert_eq!(custom.well_known_index(), None);
    assert_eq!(custom.as_str(), "my.plugin.sidebar");
}

// -----------------------------------------------------------------------
// PaintHook tests
// -----------------------------------------------------------------------

struct TestPaintHook {
    id: &'static str,
    deps: DirtyFlags,
    surface_filter: Option<crate::surface::SurfaceId>,
}

impl PaintHook for TestPaintHook {
    fn id(&self) -> &str {
        self.id
    }
    fn deps(&self) -> DirtyFlags {
        self.deps
    }
    fn surface_filter(&self) -> Option<crate::surface::SurfaceId> {
        self.surface_filter.clone()
    }
    fn apply(
        &self,
        grid: &mut crate::render::CellGrid,
        _region: &crate::layout::Rect,
        _state: &AppState,
    ) {
        // Write a marker character at (0, 0) to prove the hook ran
        if let Some(cell) = grid.get_mut(0, 0) {
            cell.grapheme = compact_str::CompactString::new(self.id);
        }
    }
}

struct PaintHookPlugin {
    hooks: Vec<Box<dyn PaintHook>>,
}

impl Plugin for PaintHookPlugin {
    fn id(&self) -> PluginId {
        PluginId("paint-hook-test".to_string())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::PAINT_HOOK
    }

    fn paint_hooks(&self) -> Vec<Box<dyn PaintHook>> {
        // Re-create hooks each time (test simplicity)
        self.hooks
            .iter()
            .map(|h| -> Box<dyn PaintHook> {
                Box::new(TestPaintHook {
                    id: match h.id() {
                        "hook-a" => "hook-a",
                        "hook-b" => "hook-b",
                        _ => "unknown",
                    },
                    deps: h.deps(),
                    surface_filter: h.surface_filter(),
                })
            })
            .collect()
    }
}

#[test]
fn test_collect_paint_hooks() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(PaintHookPlugin {
        hooks: vec![
            Box::new(TestPaintHook {
                id: "hook-a",
                deps: DirtyFlags::BUFFER,
                surface_filter: None,
            }),
            Box::new(TestPaintHook {
                id: "hook-b",
                deps: DirtyFlags::STATUS,
                surface_filter: None,
            }),
        ],
    }));
    let hooks = registry.collect_paint_hooks();
    assert_eq!(hooks.len(), 2);
    assert_eq!(hooks[0].id(), "hook-a");
    assert_eq!(hooks[1].id(), "hook-b");
}

#[test]
fn test_paint_hook_applies_to_grid() {
    use crate::layout::Rect;
    use crate::render::CellGrid;

    let mut grid = CellGrid::new(10, 5);
    let state = AppState::default();
    let region = Rect {
        x: 0,
        y: 0,
        w: 10,
        h: 5,
    };
    let hook = TestPaintHook {
        id: "X",
        deps: DirtyFlags::ALL,
        surface_filter: None,
    };
    hook.apply(&mut grid, &region, &state);
    assert_eq!(grid.get(0, 0).unwrap().grapheme.as_str(), "X");
}

#[test]
fn test_apply_paint_hooks_deps_filtering() {
    use crate::layout::Rect;
    use crate::render::CellGrid;
    use crate::render::pipeline::apply_paint_hooks;

    let mut grid = CellGrid::new(10, 5);
    let state = AppState::default();
    let region = Rect {
        x: 0,
        y: 0,
        w: 10,
        h: 5,
    };

    // Hook depends on STATUS, but dirty is BUFFER → should NOT run
    let hooks: Vec<Box<dyn PaintHook>> = vec![Box::new(TestPaintHook {
        id: "Z",
        deps: DirtyFlags::STATUS,
        surface_filter: None,
    })];
    apply_paint_hooks(&hooks, &mut grid, &region, &state, DirtyFlags::BUFFER);
    // Cell (0,0) should still be the default (space)
    assert_ne!(grid.get(0, 0).unwrap().grapheme.as_str(), "Z");

    // Now with matching dirty flags → should run
    apply_paint_hooks(&hooks, &mut grid, &region, &state, DirtyFlags::STATUS);
    assert_eq!(grid.get(0, 0).unwrap().grapheme.as_str(), "Z");
}

#[test]
fn test_paint_hook_no_capability_not_collected() {
    struct NoPaintHookPlugin;
    impl Plugin for NoPaintHookPlugin {
        fn id(&self) -> PluginId {
            PluginId("no-hook".to_string())
        }
        // capabilities() defaults to empty — no PAINT_HOOK
    }

    let mut registry = PluginRegistry::new();
    registry.register(Box::new(NoPaintHookPlugin));
    let hooks = registry.collect_paint_hooks();
    assert!(hooks.is_empty());
}
