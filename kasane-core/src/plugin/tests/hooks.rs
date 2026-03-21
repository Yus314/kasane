use super::*;

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

impl PluginBackend for ObservingPlugin {
    fn id(&self) -> PluginId {
        PluginId("observer".to_string())
    }

    fn observe_key(&mut self, key: &KeyEvent, _state: &AppView<'_>) {
        self.observed_keys
            .borrow_mut()
            .push(format!("{:?}", key.key));
    }
}

#[test]
fn test_observe_key_called() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(ObservingPlugin::new()));
    let state = AppState::default();
    let key = KeyEvent {
        key: crate::input::Key::Char('a'),
        modifiers: crate::input::Modifiers::empty(),
    };
    let view = AppView::new(&state);
    for plugin in registry.plugins_mut() {
        plugin.observe_key(&key, &view);
    }
    // No panic = success, since we can't downcast
}

// --- Menu transform tests ---

struct IconPlugin;

impl PluginBackend for IconPlugin {
    fn id(&self) -> PluginId {
        PluginId("icons".to_string())
    }

    fn transform_menu_item(
        &self,
        item: &[crate::protocol::Atom],
        _index: usize,
        _selected: bool,
        _state: &AppView<'_>,
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
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(IconPlugin));
    let state = AppState::default();
    let item = vec![crate::protocol::Atom {
        face: Face::default(),
        contents: "foo".into(),
    }];
    let result = registry.transform_menu_item(&item, 0, false, &AppView::new(&state));
    assert!(result.is_some());
    let result = result.unwrap();
    assert_eq!(result[0].contents.as_str(), "★ ");
    assert_eq!(result[1].contents.as_str(), "foo");
}

#[test]
fn test_transform_menu_item_no_plugin() {
    let registry = PluginRuntime::new();
    let state = AppState::default();
    let item = vec![crate::protocol::Atom {
        face: Face::default(),
        contents: "foo".into(),
    }];
    assert!(
        registry
            .transform_menu_item(&item, 0, false, &AppView::new(&state))
            .is_none()
    );
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

// --- PaintHook tests ---

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

impl PluginBackend for PaintHookPlugin {
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
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(PaintHookPlugin {
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
fn test_collect_paint_hooks_for_owner() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(PaintHookPlugin {
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

    let hooks = registry.collect_paint_hooks_for_owner(&PluginId("paint-hook-test".to_string()));
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
    impl PluginBackend for NoPaintHookPlugin {
        fn id(&self) -> PluginId {
            PluginId("no-hook".to_string())
        }
        // capabilities() defaults to empty — no PAINT_HOOK
    }

    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(NoPaintHookPlugin));
    let hooks = registry.collect_paint_hooks();
    assert!(hooks.is_empty());
}
