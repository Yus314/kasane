use super::*;
use crate::plugin::PluginCapabilities;
use crate::plugin::kakoune_transparent_command::KakouneTransparentCommand;
use crate::plugin::traits::PluginBackend;
use crate::state::DirtyFlags;

#[derive(Clone, Debug, PartialEq, Hash, Default)]
struct TestState {
    counter: u32,
}

#[test]
fn empty_registry_has_no_capabilities() {
    let registry = HandlerRegistry::<TestState>::new();
    let table = registry.into_table();
    assert_eq!(table.capabilities(), PluginCapabilities::empty());
}

#[test]
fn declare_interests() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.declare_interests(DirtyFlags::BUFFER);
    let table = registry.into_table();
    assert_eq!(table.interests(), DirtyFlags::BUFFER);
}

#[test]
fn on_decorate_background_sets_annotator_capability() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_decorate_background(|_state, _line, _app, _ctx| None);
    let table = registry.into_table();
    assert!(table.capabilities().contains(PluginCapabilities::ANNOTATOR));
}

#[test]
fn on_contribute_sets_contributor_capability() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_contribute(SlotId::STATUS_LEFT, |_state, _app, _ctx| None);
    let table = registry.into_table();
    assert!(
        table
            .capabilities()
            .contains(PluginCapabilities::CONTRIBUTOR)
    );
    assert_eq!(table.contribute_handlers.len(), 1);
    assert_eq!(table.contribute_handlers[0].slot, SlotId::STATUS_LEFT);
}

#[test]
fn on_transform_sets_transformer_capability() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_transform(10, |_state, _target, _app, _ctx| ElementPatch::Identity);
    let table = registry.into_table();
    assert!(
        table
            .capabilities()
            .contains(PluginCapabilities::TRANSFORMER)
    );
    assert!(table.transform_handler.is_some());
    assert_eq!(table.transform_handler.as_ref().unwrap().priority, 10);
}

#[test]
fn on_transform_has_empty_targets() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_transform(10, |_state, _target, _app, _ctx| ElementPatch::Identity);
    let table = registry.into_table();
    let desc = table.capability_descriptor();
    assert!(desc.transform_targets.is_empty());
}

#[test]
fn on_transform_for_populates_targets() {
    use crate::plugin::context::TransformTarget;
    let mut registry = HandlerRegistry::<TestState>::new();
    let targets = [TransformTarget::BUFFER, TransformTarget::STATUS_BAR];
    registry.on_transform_for(5, &targets, |_state, _target, _app, _ctx| {
        ElementPatch::Identity
    });
    let table = registry.into_table();
    let desc = table.capability_descriptor();
    assert_eq!(desc.transform_targets.len(), 2);
    assert!(desc.transform_targets.contains(&TransformTarget::BUFFER));
    assert!(
        desc.transform_targets
            .contains(&TransformTarget::STATUS_BAR)
    );
}

#[test]
fn on_transform_for_sets_priority() {
    use crate::plugin::context::TransformTarget;
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_transform_for(
        42,
        &[TransformTarget::MENU],
        |_state, _target, _app, _ctx| ElementPatch::Identity,
    );
    let table = registry.into_table();
    assert_eq!(table.transform_handler.as_ref().unwrap().priority, 42);
}

#[test]
fn may_interfere_detects_transform_target_overlap() {
    use crate::plugin::context::TransformTarget;

    let mut r1 = HandlerRegistry::<TestState>::new();
    r1.on_transform_for(
        0,
        &[TransformTarget::BUFFER, TransformTarget::MENU],
        |_s, _t, _a, _c| ElementPatch::Identity,
    );
    let desc1 = r1.into_table().capability_descriptor();

    let mut r2 = HandlerRegistry::<TestState>::new();
    r2.on_transform_for(
        0,
        &[TransformTarget::MENU, TransformTarget::STATUS_BAR],
        |_s, _t, _a, _c| ElementPatch::Identity,
    );
    let desc2 = r2.into_table().capability_descriptor();

    // MENU overlaps
    assert!(desc1.may_interfere(&desc2));
}

#[test]
fn may_interfere_no_overlap() {
    use crate::plugin::context::TransformTarget;

    let mut r1 = HandlerRegistry::<TestState>::new();
    r1.on_transform_for(0, &[TransformTarget::BUFFER], |_s, _t, _a, _c| {
        ElementPatch::Identity
    });
    let desc1 = r1.into_table().capability_descriptor();

    let mut r2 = HandlerRegistry::<TestState>::new();
    r2.on_transform_for(0, &[TransformTarget::MENU], |_s, _t, _a, _c| {
        ElementPatch::Identity
    });
    let desc2 = r2.into_table().capability_descriptor();

    assert!(!desc1.may_interfere(&desc2));
}

#[test]
fn on_key_sets_input_handler_capability() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_key(|_state, _key, _app| None::<(TestState, Vec<Command>)>);
    let table = registry.into_table();
    assert!(
        table
            .capabilities()
            .contains(PluginCapabilities::INPUT_HANDLER)
    );
}

#[test]
fn on_text_input_sets_input_handler_capability() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_text_input(|_state, _text, _app| None::<(TestState, Vec<Command>)>);
    let table = registry.into_table();
    assert!(
        table
            .capabilities()
            .contains(PluginCapabilities::INPUT_HANDLER)
    );
}

#[test]
fn on_overlay_sets_overlay_capability() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_overlay(|_state, _app, _ctx| None);
    let table = registry.into_table();
    assert!(table.capabilities().contains(PluginCapabilities::OVERLAY));
}

#[test]
fn on_display_sets_display_transform_capability() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_display(|_state, _app| vec![]);
    let table = registry.into_table();
    assert!(
        table
            .capabilities()
            .contains(PluginCapabilities::DISPLAY_TRANSFORM)
    );
}

#[test]
fn on_render_ornaments_sets_capability() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_render_ornaments(|_state, _app, _ctx| OrnamentBatch::default());
    let table = registry.into_table();
    assert!(
        table
            .capabilities()
            .contains(PluginCapabilities::RENDER_ORNAMENT)
    );
}

#[test]
fn on_paint_inline_box_sets_capability() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_paint_inline_box(|_state, _box_id, _app| None);
    let table = registry.into_table();
    assert!(
        table
            .capabilities()
            .contains(PluginCapabilities::INLINE_BOX_PAINTER)
    );
}

#[test]
fn paint_inline_box_default_is_no_op() {
    // A registry with no inline-box-paint handler must not advertise
    // the capability (gating invariant — host can skip dispatch).
    let registry = HandlerRegistry::<TestState>::new();
    let table = registry.into_table();
    assert!(
        !table
            .capabilities()
            .contains(PluginCapabilities::INLINE_BOX_PAINTER)
    );
}

#[test]
fn multiple_gutter_handlers() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_decorate_gutter(GutterSide::Left, 0, |_s, _l, _a, _c| None);
    registry.on_decorate_gutter(GutterSide::Right, 10, |_s, _l, _a, _c| None);
    let table = registry.into_table();
    assert_eq!(table.gutter_handlers.len(), 2);
    assert_eq!(table.gutter_handlers[0].side, GutterSide::Left);
    assert_eq!(table.gutter_handlers[0].priority, 0);
    assert_eq!(table.gutter_handlers[1].side, GutterSide::Right);
    assert_eq!(table.gutter_handlers[1].priority, 10);
}

#[test]
fn multiple_contribute_handlers() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_contribute(SlotId::STATUS_LEFT, |_s, _a, _c| None);
    registry.on_contribute(SlotId::STATUS_RIGHT, |_s, _a, _c| None);
    let table = registry.into_table();
    assert_eq!(table.contribute_handlers.len(), 2);
}

#[test]
fn combined_capabilities() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_decorate_background(|_s, _l, _a, _c| None);
    registry.on_overlay(|_s, _a, _c| None);
    registry.on_key(|_s, _k, _a| None::<(TestState, Vec<Command>)>);
    let table = registry.into_table();
    let caps = table.capabilities();
    assert!(caps.contains(PluginCapabilities::ANNOTATOR));
    assert!(caps.contains(PluginCapabilities::OVERLAY));
    assert!(caps.contains(PluginCapabilities::INPUT_HANDLER));
    assert!(!caps.contains(PluginCapabilities::TRANSFORMER));
}

#[test]
fn has_annotation_handlers_with_background() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_decorate_background(|_s, _l, _a, _c| None);
    let table = registry.into_table();
    assert!(table.has_annotation_handlers());
}

#[test]
fn has_annotation_handlers_with_gutter() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_decorate_gutter(GutterSide::Left, 0, |_s, _l, _a, _c| None);
    let table = registry.into_table();
    assert!(table.has_annotation_handlers());
}

#[test]
#[allow(deprecated)] // ADR-044 A-3g: test exercises the legacy setter
fn handler_type_erasure_invocation() {
    // Verify that erased handlers can be invoked with the correct state type.
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_state_changed(|state, _app, _dirty| {
        let new_state = TestState {
            counter: state.counter + 1,
        };
        (new_state, Effects::default())
    });
    let table = registry.into_table();

    // Create a boxed state
    let _state: Box<dyn PluginState> = Box::new(TestState { counter: 5 });

    // We can't easily create an AppView in tests, but we can verify
    // the handler is stored and the type alias is correct.
    assert!(table.state_changed_handler.is_some());
}

/// The tier-1 state-changed setter (issue #102, ADR-044) accepts closures
/// returning `KakouneSideEffects` and stores them through the same
/// state-changed handler slot as the legacy setter. The compile-time
/// rejection of `Effects` returns is witnessed by the `compile_fail`
/// doctest at the setter site — see `lifecycle.rs`.
#[test]
fn tier1_setter_stores_handler_at_state_changed_slot() {
    use super::super::KakouneSideEffects;

    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_state_changed_tier1(|state, _app, _dirty| {
        let new_state = TestState {
            counter: state.counter + 1,
        };
        (new_state, KakouneSideEffects::none())
    });
    let table = registry.into_table();
    assert!(
        table.state_changed_handler.is_some(),
        "tier1 setter must store into state_changed_handler"
    );
}

/// Tier-1 lifecycle setters route into the standard handler slots so
/// the dispatcher sees them indistinguishably from legacy
/// `Effects`-typed handlers — the tier check is purely a registration
/// constraint.
#[test]
fn tier1_init_and_session_ready_setters_store_handlers() {
    use super::super::KakouneSideEffects;

    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_init_tier1(|state, _app| (state.clone(), KakouneSideEffects::none()));
    registry.on_session_ready_tier1(|state, _app| (state.clone(), KakouneSideEffects::none()));
    let table = registry.into_table();
    assert!(table.init_handler.is_some(), "on_init_tier1 stores handler");
    assert!(
        table.session_ready_handler.is_some(),
        "on_session_ready_tier1 stores handler"
    );
}

/// Tier-2 setters accept `ProcessCapableEffects` (and narrower tiers
/// via the `From` lifts) but reject raw `Effects`.
#[test]
fn tier2_io_and_update_setters_store_handlers() {
    use super::super::{KakouneSideEffects, ProcessCapableEffects};

    let mut registry = HandlerRegistry::<TestState>::new();
    registry
        .on_io_event_tier2(|state, _event, _app| (state.clone(), ProcessCapableEffects::none()));
    // Narrower tier (KakouneSideEffects) lifts into ProcessCapableEffects
    // via `From`.
    registry.on_update_tier2(|state, _msg, _app| (state.clone(), KakouneSideEffects::none()));
    let table = registry.into_table();
    assert!(
        table.io_event_handler.is_some(),
        "on_io_event_tier2 stores handler"
    );
    assert!(
        table.update_handler.is_some(),
        "on_update_tier2 stores handler"
    );
}

/// Process-task tier-2 setters store entries with `transparent: false`
/// (the ADR-030 transparency marker is independent of ADR-044 tier;
/// tier-typed handlers do not claim transparency).
#[test]
fn tier2_process_task_setters_store_entries() {
    use super::super::ProcessCapableEffects;
    use crate::plugin::process_task::ProcessTaskSpec;

    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_process_task_tier2(
        "task-a",
        ProcessTaskSpec::new("fd", &["--type", "f"]),
        |state, _r, _app| (state.clone(), ProcessCapableEffects::none()),
    );
    registry.on_process_task_streaming_tier2(
        "task-b",
        ProcessTaskSpec::new("rg", &["pattern"]),
        |state, _r, _app| (state.clone(), ProcessCapableEffects::none()),
    );
    let table = registry.into_table();
    assert_eq!(table.process_tasks.len(), 2);
    assert!(!table.process_tasks[0].streaming);
    assert!(table.process_tasks[1].streaming);
    assert!(!table.process_tasks[0].transparent);
    assert!(!table.process_tasks[1].transparent);
}

/// Tier-1 input setters store handlers and route through the same
/// erased dispatch slots as the legacy setters. The asymmetric command
/// projection (`KakouneSideCommand → Command`, no reverse) prevents
/// `SpawnProcess`-bearing closures from compiling against
/// `on_key_tier1` / `on_text_input_tier1` / `on_drop_tier1`.
#[test]
fn tier1_input_setters_store_handlers() {
    use super::super::KakouneSideCommand;

    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_key_tier1(
        |state, _key, _app| -> Option<(TestState, Vec<KakouneSideCommand>)> {
            Some((state.clone(), vec![]))
        },
    );
    registry.on_text_input_tier1(
        |state, _text, _app| -> Option<(TestState, Vec<KakouneSideCommand>)> {
            Some((state.clone(), vec![]))
        },
    );
    registry.on_drop_tier1(
        |state, _event, _id, _app| -> Option<(TestState, Vec<KakouneSideCommand>)> {
            Some((state.clone(), vec![]))
        },
    );
    let table = registry.into_table();
    assert!(table.key_handler.is_some());
    assert!(table.text_input_handler.is_some());
    assert!(table.handle_drop_handler.is_some());
}

/// Tier-1 mouse fallback setter accepts `Option<Vec<KakouneSideCommand>>`
/// and stores into the same fallback slot as the legacy setter.
#[test]
fn tier1_mouse_fallback_setter_stores_handler() {
    use super::super::KakouneSideCommand;

    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_mouse_fallback_tier1(
        |state, _event, _scroll, _app| -> (TestState, Option<Vec<KakouneSideCommand>>) {
            (state.clone(), None)
        },
    );
    let table = registry.into_table();
    assert!(table.mouse_fallback_handler.is_some());
}

#[test]
fn on_navigation_policy_sets_capability() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_navigation_policy(|_state, _unit| NavigationPolicy::Normal);
    let table = registry.into_table();
    assert!(
        table
            .capabilities()
            .contains(PluginCapabilities::NAVIGATION_POLICY)
    );
}

#[test]
fn on_navigation_action_sets_capability_and_updates_state() {
    use crate::display;
    use crate::plugin::PluginBridge;
    use crate::plugin::state::Plugin;

    #[derive(Clone, Debug, PartialEq, Hash, Default)]
    struct NavTestState {
        counter: u32,
    }
    struct NavTestPlugin;
    impl Plugin for NavTestPlugin {
        type State = NavTestState;
        fn id(&self) -> crate::plugin::PluginId {
            crate::plugin::PluginId("nav-test".into())
        }
        fn register(&self, r: &mut HandlerRegistry<NavTestState>) {
            r.on_navigation_action(|state, _unit, _action| {
                (
                    NavTestState {
                        counter: state.counter + 1,
                    },
                    ActionResult::Handled,
                )
            });
        }
    }

    let mut bridge = PluginBridge::new(NavTestPlugin);
    assert!(
        bridge
            .capabilities()
            .contains(PluginCapabilities::NAVIGATION_ACTION)
    );

    let unit = display::unit::DisplayUnit {
        id: display::unit::DisplayUnitId::from_content(
            &display::unit::UnitSource::Line(0),
            &display::unit::SemanticRole::BufferContent,
        ),
        display_line: 0,
        role: display::unit::SemanticRole::BufferContent,
        source: display::unit::UnitSource::Line(0),
        interaction: display::InteractionPolicy::Normal,
    };
    let result = bridge.navigation_action(&unit, NavigationAction::None);
    assert_eq!(result, Some(ActionResult::Handled));
}

// =========================================================================
// Transparent handler registration (ADR-030 Level 3)
// =========================================================================

#[test]
fn on_key_transparent_sets_input_handler_and_transparency() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_key(
        |_state: &TestState, _key, _app| -> Option<(TestState, Vec<KakouneTransparentCommand>)> {
            None
        },
    );
    assert!(registry.is_input_transparent());
    let table = registry.into_table();
    assert!(
        table
            .capabilities()
            .contains(PluginCapabilities::INPUT_HANDLER)
    );
    assert!(table.transparency.key_handler);
}

#[test]
fn on_key_non_transparent_means_not_input_transparent() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_key(|_state, _key, _app| None::<(TestState, Vec<Command>)>);
    assert!(!registry.is_input_transparent());
}

#[test]
fn mixed_transparent_and_non_transparent_is_not_transparent() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_key(
        |_state: &TestState, _key, _app| -> Option<(TestState, Vec<KakouneTransparentCommand>)> {
            None
        },
    );
    registry.on_text_input(|_state, _text, _app| None::<(TestState, Vec<Command>)>);
    assert!(!registry.is_input_transparent());
}

#[test]
fn all_transparent_handlers_means_input_transparent() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_key(
        |_state: &TestState, _key, _app| -> Option<(TestState, Vec<KakouneTransparentCommand>)> {
            None
        },
    );
    registry.on_text_input(
        |_state: &TestState, _text, _app| -> Option<(TestState, Vec<KakouneTransparentCommand>)> {
            None
        },
    );
    assert!(registry.is_input_transparent());
}

#[test]
fn no_handlers_is_input_transparent() {
    let registry = HandlerRegistry::<TestState>::new();
    assert!(registry.is_input_transparent());
}

// =========================================================================
// Unified display handler tests (Phase 1B.2)
// =========================================================================

#[test]
fn on_display_unified_sets_display_transform_capability() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_display_unified(|_state, _app| vec![]);
    let table = registry.into_table();
    assert!(
        table
            .capabilities()
            .contains(PluginCapabilities::DISPLAY_TRANSFORM)
    );
}

#[test]
fn on_display_unified_sets_annotator_capability() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_display_unified(|_state, _app| vec![]);
    let table = registry.into_table();
    assert!(table.capabilities().contains(PluginCapabilities::ANNOTATOR));
}

#[test]
fn on_display_unified_sets_content_annotator_capability() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_display_unified(|_state, _app| vec![]);
    let table = registry.into_table();
    assert!(
        table
            .capabilities()
            .contains(PluginCapabilities::CONTENT_ANNOTATOR)
    );
}

#[test]
fn on_display_unified_safe_is_recoverable() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_display_unified_safe(|_state, _app| vec![]);
    assert!(registry.is_display_recoverable());
}

#[test]
fn on_display_unified_is_not_recoverable() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_display_unified(|_state, _app| vec![]);
    assert!(!registry.is_display_recoverable());
}
