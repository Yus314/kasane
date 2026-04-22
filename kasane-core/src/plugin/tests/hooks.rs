use super::*;
use crate::plugin::{
    CursorEffect, CursorEffectOrn, CursorStyleOrn, OrnamentBatch, OrnamentModality,
    RenderOrnamentContext, SurfaceOrn, SurfaceOrnAnchor, SurfaceOrnKind,
};

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
    let result = registry
        .view()
        .transform_menu_item(&item, 0, false, &AppView::new(&state));
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
            .view()
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

struct RenderOrnamentPlugin {
    batch: OrnamentBatch,
}

impl PluginBackend for RenderOrnamentPlugin {
    fn id(&self) -> PluginId {
        PluginId("render-ornament-test".to_string())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::RENDER_ORNAMENT
    }

    fn render_ornaments(
        &self,
        _state: &AppView<'_>,
        _ctx: &RenderOrnamentContext,
    ) -> OrnamentBatch {
        self.batch.clone()
    }
}

#[test]
fn test_collect_ornaments() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(RenderOrnamentPlugin {
        batch: OrnamentBatch {
            emphasis: vec![CellDecoration {
                target: DecorationTarget::Column { column: 3 },
                face: Face::default(),
                merge: FaceMerge::Background,
                priority: 10,
            }],
            cursor_style: Some(CursorStyleOrn {
                hint: crate::render::CursorStyle::Bar.into(),
                priority: 20,
                modality: OrnamentModality::Approximate,
            }),
            cursor_position: None,
            cursor_effects: vec![CursorEffectOrn {
                kind: CursorEffect::Halo,
                face: Face::default(),
                priority: 15,
                modality: OrnamentModality::Approximate,
            }],
            surfaces: vec![SurfaceOrn {
                anchor: SurfaceOrnAnchor::FocusedSurface,
                kind: SurfaceOrnKind::FocusFrame,
                face: Face::default(),
                priority: 30,
                modality: OrnamentModality::Must,
            }],
        },
    }));

    let state = AppState::default();
    let collected = registry
        .view()
        .collect_ornaments(&AppView::new(&state), &RenderOrnamentContext::default());

    assert_eq!(collected.emphasis.len(), 1);
    assert!(collected.cursor_style.is_some());
    assert_eq!(collected.cursor_effects.len(), 1);
    assert_eq!(collected.surfaces.len(), 1);
}

#[test]
fn test_cursor_style_does_not_compete_with_effects() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(RenderOrnamentPlugin {
        batch: OrnamentBatch {
            emphasis: vec![],
            cursor_style: Some(CursorStyleOrn {
                hint: crate::render::CursorStyle::Bar.into(),
                priority: 10,
                modality: OrnamentModality::Must,
            }),
            cursor_position: None,
            cursor_effects: vec![CursorEffectOrn {
                kind: CursorEffect::Halo,
                face: Face::default(),
                priority: 20,
                modality: OrnamentModality::Must,
            }],
            surfaces: vec![],
        },
    }));

    let state = AppState::default();
    let collected = registry
        .view()
        .collect_ornaments(&AppView::new(&state), &RenderOrnamentContext::default());

    // cursor_style and cursor_effects are independent channels
    assert_eq!(
        collected.cursor_style.map(|h| h.shape),
        Some(crate::render::CursorStyle::Bar)
    );
    assert_eq!(collected.cursor_effects.len(), 1);
    assert_eq!(collected.cursor_effects[0].kind, CursorEffect::Halo);
}

#[test]
fn test_cursor_effects_accumulate() {
    struct EffectPlugin {
        id: &'static str,
        effect: CursorEffect,
    }
    impl PluginBackend for EffectPlugin {
        fn id(&self) -> PluginId {
            PluginId(self.id.to_string())
        }
        fn capabilities(&self) -> PluginCapabilities {
            PluginCapabilities::RENDER_ORNAMENT
        }
        fn render_ornaments(
            &self,
            _state: &AppView<'_>,
            _ctx: &RenderOrnamentContext,
        ) -> OrnamentBatch {
            OrnamentBatch {
                cursor_effects: vec![CursorEffectOrn {
                    kind: self.effect,
                    face: Face::default(),
                    priority: 10,
                    modality: OrnamentModality::Approximate,
                }],
                ..OrnamentBatch::default()
            }
        }
    }

    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(EffectPlugin {
        id: "halo",
        effect: CursorEffect::Halo,
    }));
    registry.register_backend(Box::new(EffectPlugin {
        id: "ring",
        effect: CursorEffect::Ring,
    }));

    let state = AppState::default();
    let collected = registry
        .view()
        .collect_ornaments(&AppView::new(&state), &RenderOrnamentContext::default());

    assert_eq!(collected.cursor_effects.len(), 2);
}

#[test]
fn test_cursor_style_modality_wins_over_priority() {
    struct CursorStylePlugin {
        id: &'static str,
        style: crate::render::CursorStyle,
        priority: i16,
        modality: OrnamentModality,
    }
    impl PluginBackend for CursorStylePlugin {
        fn id(&self) -> PluginId {
            PluginId(self.id.to_string())
        }
        fn capabilities(&self) -> PluginCapabilities {
            PluginCapabilities::RENDER_ORNAMENT
        }
        fn render_ornaments(
            &self,
            _state: &AppView<'_>,
            _ctx: &RenderOrnamentContext,
        ) -> OrnamentBatch {
            OrnamentBatch {
                cursor_style: Some(CursorStyleOrn {
                    hint: self.style.into(),
                    priority: self.priority,
                    modality: self.modality,
                }),
                ..OrnamentBatch::default()
            }
        }
    }

    let mut registry = PluginRuntime::new();
    // Plugin A: Must modality but low priority
    registry.register_backend(Box::new(CursorStylePlugin {
        id: "must-low",
        style: crate::render::CursorStyle::Bar,
        priority: 5,
        modality: OrnamentModality::Must,
    }));
    // Plugin B: Approximate modality but high priority
    registry.register_backend(Box::new(CursorStylePlugin {
        id: "approx-high",
        style: crate::render::CursorStyle::Underline,
        priority: 100,
        modality: OrnamentModality::Approximate,
    }));

    let state = AppState::default();
    let collected = registry
        .view()
        .collect_ornaments(&AppView::new(&state), &RenderOrnamentContext::default());

    // Must modality (rank 2) wins over Approximate (rank 1) regardless of priority
    assert_eq!(
        collected.cursor_style.map(|h| h.shape),
        Some(crate::render::CursorStyle::Bar)
    );
}
