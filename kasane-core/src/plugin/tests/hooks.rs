use super::*;
use crate::plugin::{
    CursorEffect, CursorEffectOrn, CursorStyleOrn, OrnamentBatch, OrnamentModality,
    RenderOrnamentContext, SurfaceOrn, SurfaceOrnAnchor, SurfaceOrnKind,
};

// --- Input observation tests ---

struct ObservingPlugin;

impl crate::plugin::Plugin for ObservingPlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId::from("observer")
    }

    fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
        r.on_observe_key(|_state, _key, _app| ());
    }
}

#[test]
fn test_observe_key_called() {
    let mut registry = PluginRuntime::new();
    registry.register(ObservingPlugin);
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

impl crate::plugin::Plugin for IconPlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId::from("icons")
    }

    fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
        r.on_menu_transform(|_state, item, _index, _selected, _app| {
            let mut result = vec![crate::protocol::Atom::plain("★ ")];
            result.extend(item.iter().cloned());
            Some(result)
        });
    }
}

#[test]
fn test_transform_menu_item() {
    let mut registry = PluginRuntime::new();
    registry.register(IconPlugin);
    let state = AppState::default();
    let item = vec![crate::protocol::Atom::plain("foo")];
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
    let item = vec![crate::protocol::Atom::plain("foo")];
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

impl crate::plugin::Plugin for RenderOrnamentPlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId::from("render-ornament-test")
    }

    fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
        let batch = self.batch.clone();
        r.on_render_ornament(move |_state, _app, _ctx| batch.clone());
    }
}

#[test]
fn test_collect_ornaments() {
    let mut registry = PluginRuntime::new();
    registry.register(RenderOrnamentPlugin {
        batch: OrnamentBatch {
            emphasis: vec![CellDecoration {
                target: DecorationTarget::Column { column: 3 },
                style: crate::protocol::Style::default(),
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
                style: crate::protocol::Style::default(),
                priority: 15,
                modality: OrnamentModality::Approximate,
            }],
            surfaces: vec![SurfaceOrn {
                anchor: SurfaceOrnAnchor::FocusedSurface,
                kind: SurfaceOrnKind::FocusFrame,
                style: crate::protocol::Style::default(),
                priority: 30,
                modality: OrnamentModality::Must,
            }],
        },
    });

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
    registry.register(RenderOrnamentPlugin {
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
                style: crate::protocol::Style::default(),
                priority: 20,
                modality: OrnamentModality::Must,
            }],
            surfaces: vec![],
        },
    });

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
    impl crate::plugin::Plugin for EffectPlugin {
        type State = ();
        fn id(&self) -> PluginId {
            PluginId::from(&*self.id)
        }
        fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
            let effect = self.effect;
            r.on_render_ornament(move |_state, _app, _ctx| OrnamentBatch {
                cursor_effects: vec![CursorEffectOrn {
                    kind: effect,
                    style: crate::protocol::Style::default(),
                    priority: 10,
                    modality: OrnamentModality::Approximate,
                }],
                ..OrnamentBatch::default()
            });
        }
    }

    let mut registry = PluginRuntime::new();
    registry.register(EffectPlugin {
        id: "halo",
        effect: CursorEffect::Halo,
    });
    registry.register(EffectPlugin {
        id: "ring",
        effect: CursorEffect::Ring,
    });

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
    impl crate::plugin::Plugin for CursorStylePlugin {
        type State = ();
        fn id(&self) -> PluginId {
            PluginId::from(&*self.id)
        }
        fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
            let style = self.style;
            let priority = self.priority;
            let modality = self.modality;
            r.on_render_ornament(move |_state, _app, _ctx| OrnamentBatch {
                cursor_style: Some(CursorStyleOrn {
                    hint: style.into(),
                    priority,
                    modality,
                }),
                ..OrnamentBatch::default()
            });
        }
    }

    let mut registry = PluginRuntime::new();
    // Plugin A: Must modality but low priority
    registry.register(CursorStylePlugin {
        id: "must-low",
        style: crate::render::CursorStyle::Bar,
        priority: 5,
        modality: OrnamentModality::Must,
    });
    // Plugin B: Approximate modality but high priority
    registry.register(CursorStylePlugin {
        id: "approx-high",
        style: crate::render::CursorStyle::Underline,
        priority: 100,
        modality: OrnamentModality::Approximate,
    });

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
