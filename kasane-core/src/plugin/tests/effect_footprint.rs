//! Structural witness tests for effect category classification and
//! transparency flags (ADR-030 Level 5).

use crate::plugin::command::EffectCategory;
use crate::plugin::handler_registry::HandlerRegistry;
use crate::plugin::process_task::{ProcessTaskResult, ProcessTaskSpec};
use crate::plugin::transparent_command::TransparentCommand;
use crate::plugin::transparent_effects::TransparentEffects;
use crate::plugin::{AppView, Effects, IoEvent};
use crate::state::DirtyFlags;

use super::command_classification::make_all_command_instances;

// =============================================================================
// EffectCategory exhaustive coverage
// =============================================================================

#[test]
fn effect_category_covers_all_variants() {
    // Every Command variant must return a non-empty EffectCategory.
    for cmd in make_all_command_instances() {
        let cat = cmd.effect_category();
        assert!(
            !cat.is_empty(),
            "effect_category() returned empty for {}",
            cmd.variant_name()
        );
    }
}

#[test]
fn effect_category_is_single_bit_per_variant() {
    // Each variant maps to exactly one category (single bit set).
    for cmd in make_all_command_instances() {
        let cat = cmd.effect_category();
        assert_eq!(
            cat.bits().count_ones(),
            1,
            "effect_category() for {} should be a single bit, got {:?}",
            cmd.variant_name(),
            cat
        );
    }
}

#[test]
fn kakoune_writing_category_matches_is_kakoune_writing() {
    for cmd in make_all_command_instances() {
        let is_writing = cmd.is_kakoune_writing();
        let cat_writing = cmd
            .effect_category()
            .contains(EffectCategory::KAKOUNE_WRITING);
        assert_eq!(
            is_writing,
            cat_writing,
            "is_kakoune_writing() and effect_category() disagree for {}",
            cmd.variant_name()
        );
    }
}

#[test]
fn all_categories_are_covered() {
    // The union of all variant categories should cover all defined categories.
    let all_cats: EffectCategory = make_all_command_instances()
        .iter()
        .fold(EffectCategory::empty(), |acc, cmd| {
            acc | cmd.effect_category()
        });
    // CASCADE_TRIGGERS is a composite constant, not a unique category.
    // Every individual category bit should be covered by at least one variant.
    let individual_cats = EffectCategory::all().difference(EffectCategory::CASCADE_TRIGGERS)
        | EffectCategory::PLUGIN_MESSAGE
        | EffectCategory::TIMER
        | EffectCategory::INPUT_INJECTION;
    assert_eq!(
        all_cats, individual_cats,
        "some EffectCategory bits are not covered by any Command variant"
    );
}

#[test]
fn cascade_triggers_is_union_of_message_timer_injection() {
    assert_eq!(
        EffectCategory::CASCADE_TRIGGERS,
        EffectCategory::PLUGIN_MESSAGE | EffectCategory::TIMER | EffectCategory::INPUT_INJECTION,
    );
}

// =============================================================================
// TransparentEffects structural witnesses
// =============================================================================

#[test]
fn transparent_effects_default_is_empty() {
    let te = TransparentEffects::none();
    let effects: Effects = te.into();
    assert!(effects.commands.is_empty());
    assert!(effects.scroll_plans.is_empty());
    assert_eq!(effects.redraw, DirtyFlags::empty());
}

#[test]
fn transparent_effects_redraw_preserves_flags() {
    let te = TransparentEffects::redraw(DirtyFlags::BUFFER | DirtyFlags::STATUS);
    let effects: Effects = te.into();
    assert!(effects.redraw.contains(DirtyFlags::BUFFER));
    assert!(effects.redraw.contains(DirtyFlags::STATUS));
}

#[test]
fn transparent_effects_commands_convert_correctly() {
    let cmds = vec![
        TransparentCommand::request_redraw(DirtyFlags::BUFFER),
        TransparentCommand::quit(),
    ];
    let te = TransparentEffects::with(cmds);
    let effects: Effects = te.into();
    assert_eq!(effects.commands.len(), 2);
    assert_eq!(effects.commands[0].variant_name(), "RequestRedraw");
    assert_eq!(effects.commands[1].variant_name(), "Quit");
}

#[test]
fn transparent_effects_push_accumulates() {
    let mut te = TransparentEffects::none();
    te.push(TransparentCommand::quit());
    te.set_redraw(DirtyFlags::BUFFER);
    te.push(TransparentCommand::paste_clipboard());
    let effects: Effects = te.into();
    assert_eq!(effects.commands.len(), 2);
    assert!(effects.redraw.contains(DirtyFlags::BUFFER));
}

// =============================================================================
// Lifecycle transparency flags
// =============================================================================

#[derive(Clone, Debug, Default, PartialEq)]
struct TestState {
    counter: u32,
}

#[test]
fn no_handlers_is_fully_transparent() {
    let registry = HandlerRegistry::<TestState>::new();
    assert!(registry.is_input_transparent());
    assert!(registry.is_lifecycle_transparent());
    assert!(registry.is_fully_transparent());
}

#[test]
fn non_transparent_lifecycle_handler_means_not_fully_transparent() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_state_changed(
        |state: &TestState, _app: &AppView<'_>, _dirty: DirtyFlags| {
            (state.clone(), Effects::none())
        },
    );
    assert!(registry.is_input_transparent()); // no input handlers
    assert!(!registry.is_lifecycle_transparent());
    assert!(!registry.is_fully_transparent());
}

#[test]
fn transparent_lifecycle_handler_is_lifecycle_transparent() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_state_changed_transparent(
        |state: &TestState, _app: &AppView<'_>, _dirty: DirtyFlags| {
            (state.clone(), TransparentEffects::none())
        },
    );
    assert!(registry.is_lifecycle_transparent());
    assert!(registry.is_fully_transparent());
}

#[test]
fn mixed_transparent_and_non_transparent_lifecycle_is_not_transparent() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_state_changed_transparent(
        |state: &TestState, _app: &AppView<'_>, _dirty: DirtyFlags| {
            (state.clone(), TransparentEffects::none())
        },
    );
    registry.on_init(|state: &TestState, _app: &AppView<'_>| (state.clone(), Effects::none()));
    assert!(!registry.is_lifecycle_transparent());
}

#[test]
fn all_transparent_lifecycle_handlers() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_init_transparent(|state: &TestState, _app: &AppView<'_>| {
        (state.clone(), TransparentEffects::none())
    });
    registry.on_session_ready_transparent(|state: &TestState, _app: &AppView<'_>| {
        (state.clone(), TransparentEffects::none())
    });
    registry.on_state_changed_transparent(
        |state: &TestState, _app: &AppView<'_>, _dirty: DirtyFlags| {
            (state.clone(), TransparentEffects::none())
        },
    );
    registry.on_io_event_transparent(|state: &TestState, _event: &IoEvent, _app: &AppView<'_>| {
        (state.clone(), TransparentEffects::none())
    });
    registry.on_update_transparent(
        |state: &TestState, _msg: &mut dyn std::any::Any, _app: &AppView<'_>| {
            (state.clone(), TransparentEffects::none())
        },
    );
    assert!(registry.is_lifecycle_transparent());
    assert!(registry.is_fully_transparent());
}

#[test]
fn transparent_input_but_non_transparent_lifecycle_is_not_fully_transparent() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_key_transparent(|_state, _key, _app| None);
    registry.on_state_changed(
        |state: &TestState, _app: &AppView<'_>, _dirty: DirtyFlags| {
            (state.clone(), Effects::none())
        },
    );
    assert!(registry.is_input_transparent());
    assert!(!registry.is_lifecycle_transparent());
    assert!(!registry.is_fully_transparent());
}

#[test]
fn transparent_process_task_is_lifecycle_transparent() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_process_task_transparent(
        "test_task",
        ProcessTaskSpec::new("echo", &["hello"]),
        |state: &TestState, _result: &ProcessTaskResult, _app: &AppView<'_>| {
            (state.clone(), TransparentEffects::none())
        },
    );
    assert!(registry.is_lifecycle_transparent());
}

#[test]
fn non_transparent_process_task_is_not_lifecycle_transparent() {
    let mut registry = HandlerRegistry::<TestState>::new();
    registry.on_process_task(
        "test_task",
        ProcessTaskSpec::new("echo", &["hello"]),
        |state: &TestState, _result: &ProcessTaskResult, _app: &AppView<'_>| {
            (state.clone(), Effects::none())
        },
    );
    assert!(!registry.is_lifecycle_transparent());
}
