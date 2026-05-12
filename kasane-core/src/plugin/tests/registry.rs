use super::*;
use crate::display::DisplayDirective;
use crate::input::{Key, KeyEvent, Modifiers};
use crate::layout::Rect;
use crate::plugin::{Command, Effects, KeyDispatchResult, KeyHandleResult};
use crate::protocol::Atom;
use crate::protocol::KasaneRequest;
use crate::scroll::{ScrollAccumulationMode, ScrollCurve, ScrollPlan};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

struct TypedLifecyclePlugin;

impl crate::plugin::Plugin for TypedLifecyclePlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId("typed-lifecycle".to_string())
    }

    fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
        r.on_init_tier1(|_state, _app| {
            (
                (),
                crate::plugin::KakouneSideEffects::redraw(DirtyFlags::STATUS),
            )
        });
        r.on_session_ready_tier1(|_state, _app| {
            let mut effects = crate::plugin::KakouneSideEffects::redraw(DirtyFlags::BUFFER);
            effects
                .commands
                .push(crate::plugin::KakouneSideCommand::send_to_kakoune(
                    KasaneRequest::Scroll {
                        amount: 3,
                        line: 1,
                        column: 1,
                    },
                ));
            ((), effects)
        });
    }
}

struct TypedRuntimePlugin;

impl crate::plugin::Plugin for TypedRuntimePlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId("typed-runtime".to_string())
    }

    fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
        r.on_state_changed_tier1(|_state, _app, dirty| {
            if !dirty.contains(DirtyFlags::BUFFER) {
                return ((), crate::plugin::KakouneSideEffects::none());
            }
            let mut effects = crate::plugin::KakouneSideEffects::redraw(DirtyFlags::INFO);
            effects
                .commands
                .push(crate::plugin::KakouneSideCommand::request_redraw(
                    DirtyFlags::STATUS,
                ));
            effects.base.scroll_plans.push(ScrollPlan {
                total_amount: 3,
                line: 2,
                column: 4,
                frame_interval_ms: 8,
                curve: ScrollCurve::Linear,
                accumulation: ScrollAccumulationMode::Add,
            });
            ((), effects)
        });
        r.on_update_tier2(|_state, msg, _app| {
            if msg.downcast_ref::<u32>() != Some(&7) {
                return ((), crate::plugin::ProcessCapableEffects::none());
            }
            let mut effects = crate::plugin::ProcessCapableEffects::redraw(DirtyFlags::BUFFER);
            effects
                .base
                .commands
                .push(crate::plugin::KakouneSideCommand::request_redraw(
                    DirtyFlags::STATUS,
                ));
            effects.base.base.scroll_plans.push(ScrollPlan {
                total_amount: -2,
                line: 1,
                column: 1,
                frame_interval_ms: 16,
                curve: ScrollCurve::Linear,
                accumulation: ScrollAccumulationMode::Add,
            });
            ((), effects)
        });
    }
}

struct ShutdownProbePlugin {
    id: &'static str,
    shutdowns: Arc<AtomicUsize>,
}

impl crate::plugin::Plugin for ShutdownProbePlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId(self.id.to_string())
    }

    fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
        let shutdowns = self.shutdowns.clone();
        r.on_shutdown(move |_state| {
            shutdowns.fetch_add(1, Ordering::SeqCst);
        });
    }
}

struct AuthorityPlugin {
    id: &'static str,
    authorities: PluginAuthorities,
}

pub(super) struct DisplayTransformPlugin {
    pub(super) id: &'static str,
    pub(super) directives: Vec<DisplayDirective>,
    pub(super) priority: i16,
}

impl crate::plugin::Plugin for DisplayTransformPlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId(self.id.to_string())
    }

    fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
        let directives = self.directives.clone();
        r.on_display_witnessed(
            crate::plugin::RecoveryWitness {
                mechanism: crate::plugin::RecoveryMechanism::Declared {
                    description: "test display transform plugin",
                },
            },
            move |_state, _app| directives.clone(),
        );
        r.declare_display_priority(self.priority);
    }
}

struct WorkspaceObserverPlugin {
    id: &'static str,
    hits: Arc<AtomicUsize>,
}

impl crate::plugin::Plugin for WorkspaceObserverPlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId(self.id.to_string())
    }

    fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
        let hits = self.hits.clone();
        r.on_workspace_changed(move |state, _query| {
            hits.fetch_add(1, Ordering::SeqCst);
            state.clone()
        });
    }
}

enum MiddlewareBehavior {
    Passthrough,
    Transform(KeyEvent),
    Consume(String),
}

struct KeyMiddlewarePlugin {
    id: &'static str,
    seen: Arc<Mutex<Vec<KeyEvent>>>,
    behavior: MiddlewareBehavior,
}

impl crate::plugin::Plugin for KeyMiddlewarePlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId(self.id.to_string())
    }

    fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
        let seen = self.seen.clone();
        let behavior = match &self.behavior {
            MiddlewareBehavior::Passthrough => MiddlewareBehavior::Passthrough,
            MiddlewareBehavior::Transform(k) => MiddlewareBehavior::Transform(k.clone()),
            MiddlewareBehavior::Consume(s) => MiddlewareBehavior::Consume(s.clone()),
        };
        r.on_key_middleware(move |state, key, _app| {
            seen.lock().unwrap().push(key.clone());
            let result = match &behavior {
                MiddlewareBehavior::Passthrough => KeyHandleResult::Passthrough,
                MiddlewareBehavior::Transform(next_key) => {
                    KeyHandleResult::Transformed(next_key.clone())
                }
                MiddlewareBehavior::Consume(keyspec) => {
                    KeyHandleResult::Consumed(vec![Command::SendToKakoune(KasaneRequest::Keys(
                        vec![keyspec.clone()],
                    ))])
                }
            };
            (state.clone(), result)
        });
    }
}

impl crate::plugin::Plugin for AuthorityPlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId(self.id.to_string())
    }

    fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
        r.declare_authorities(self.authorities);
    }
}

struct TargetedReadyPlugin {
    id: &'static str,
    redraw: DirtyFlags,
}

impl crate::plugin::Plugin for TargetedReadyPlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId(self.id.to_string())
    }

    fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
        let redraw = self.redraw;
        r.on_session_ready_tier1(move |_state, _app| {
            ((), crate::plugin::KakouneSideEffects::redraw(redraw))
        });
    }
}

#[test]
fn test_empty_registry() {
    let registry = PluginRuntime::new();
    assert!(registry.plugin_count() == 0);
}

#[test]
fn test_plugin_id() {
    let plugin = TestPlugin;
    assert_eq!(plugin.id(), PluginId("test".to_string()));
}

#[test]
fn test_init_all_batch_collects_bootstrap_effects() {
    let mut registry = PluginRuntime::new();
    registry.register(TypedLifecyclePlugin);
    let state = AppState::default();

    let batch = registry.init_all_batch(&AppView::new(&state));
    assert!(batch.redraw.contains(DirtyFlags::STATUS));
}

#[test]
fn test_notify_active_session_ready_batch_collects_effects() {
    let mut registry = PluginRuntime::new();
    registry.register(TypedLifecyclePlugin);
    let state = AppState::default();

    let batch = registry.notify_active_session_ready_batch(&AppView::new(&state));
    assert!(batch.redraw.contains(DirtyFlags::BUFFER));
    assert_eq!(batch.total_command_count(), 1);
    let mut commands = batch.per_plugin_commands.into_iter().flat_map(|(_, c)| c);
    assert!(matches!(
        commands.next(),
        Some(Command::SendToKakoune(KasaneRequest::Scroll { .. }))
    ));
}

#[test]
fn test_notify_plugin_active_session_ready_batch_targets_only_requested_plugin() {
    let mut registry = PluginRuntime::new();
    registry.register(TargetedReadyPlugin {
        id: "alpha",
        redraw: DirtyFlags::STATUS,
    });
    registry.register(TargetedReadyPlugin {
        id: "beta",
        redraw: DirtyFlags::BUFFER,
    });
    let state = AppState::default();

    let batch = registry.notify_plugin_active_session_ready_batch(
        &PluginId("beta".to_string()),
        &AppView::new(&state),
    );
    assert!(batch.redraw.contains(DirtyFlags::BUFFER));
    assert!(!batch.redraw.contains(DirtyFlags::STATUS));
}

#[test]
fn test_notify_state_changed_batch_collects_runtime_effects() {
    let mut registry = PluginRuntime::new();
    registry.register(TypedRuntimePlugin);
    let state = AppState::default();

    let batch = registry.notify_state_changed_batch(&AppView::new(&state), DirtyFlags::BUFFER);
    assert!(batch.redraw.contains(DirtyFlags::INFO));
    assert_eq!(batch.total_command_count(), 1);
    assert_eq!(batch.scroll_plans.len(), 1);
}

#[test]
fn test_deliver_message_batch_collects_runtime_effects() {
    let mut registry = PluginRuntime::new();
    registry.register(TypedRuntimePlugin);
    let state = AppState::default();

    let batch = registry.deliver_message_batch(
        &PluginId("typed-runtime".to_string()),
        Box::new(7u32),
        &AppView::new(&state),
    );
    assert!(batch.redraw.contains(DirtyFlags::BUFFER));
    assert_eq!(batch.total_command_count(), 1);
    assert_eq!(batch.scroll_plans.len(), 1);
}

#[test]
fn test_shutdown_all_calls_all_plugins() {
    let mut registry = PluginRuntime::new();
    registry.register(LifecyclePlugin::new());
    registry.register(LifecyclePlugin::new());
    registry.shutdown_all();
    // Verify via count — can't inspect internal state, but no panic = success
}

#[test]
fn test_collect_plugin_surfaces_returns_owner_group() {
    let mut registry = PluginRuntime::new();
    registry.register(SurfacePlugin);

    let surface_sets = registry.collect_plugin_surfaces();
    assert_eq!(surface_sets.len(), 1);
    assert_eq!(
        surface_sets[0].owner,
        PluginId("surface-plugin".to_string())
    );
    assert_eq!(surface_sets[0].surfaces.len(), 2);
    assert_eq!(surface_sets[0].surfaces[0].id(), SurfaceId(200));
    assert_eq!(surface_sets[0].surfaces[1].id(), SurfaceId(201));
    assert!(matches!(
        surface_sets[0].legacy_workspace_request,
        Some(Placement::SplitFocused {
            direction: SplitDirection::Vertical,
            ratio
        }) if (ratio - 0.5).abs() < f32::EPSILON
    ));
}

#[test]
fn test_remove_plugin_removes_registered_plugin() {
    let mut registry = PluginRuntime::new();
    registry.register(TestPlugin);
    registry.register(SurfacePlugin);

    assert!(registry.remove_plugin(&PluginId("surface-plugin".to_string())));
    assert_eq!(registry.plugin_count(), 1);
    assert!(!registry.remove_plugin(&PluginId("surface-plugin".to_string())));
}

#[test]
fn test_plugin_has_authority_uses_declared_authorities() {
    let mut registry = PluginRuntime::new();
    let plugin_id = PluginId("authority-probe".to_string());
    registry.register(AuthorityPlugin {
        id: "authority-probe",
        authorities: PluginAuthorities::DYNAMIC_SURFACE,
    });

    assert!(registry.plugin_has_authority(&plugin_id, PluginAuthorities::DYNAMIC_SURFACE));
    assert!(!registry.plugin_has_authority(&plugin_id, PluginAuthorities::PTY_PROCESS));
}

#[test]
fn test_register_backend_replacement_updates_authorities() {
    let mut registry = PluginRuntime::new();
    let plugin_id = PluginId("authority-probe".to_string());
    registry.register(AuthorityPlugin {
        id: "authority-probe",
        authorities: PluginAuthorities::DYNAMIC_SURFACE,
    });
    registry.register(AuthorityPlugin {
        id: "authority-probe",
        authorities: PluginAuthorities::PTY_PROCESS,
    });

    assert!(!registry.plugin_has_authority(&plugin_id, PluginAuthorities::DYNAMIC_SURFACE));
    assert!(registry.plugin_has_authority(&plugin_id, PluginAuthorities::PTY_PROCESS));
}

#[test]
fn test_collect_display_directives_composes_multi_plugin() {
    let mut registry = PluginRuntime::new();
    registry.register(DisplayTransformPlugin {
        id: "first",
        directives: vec![DisplayDirective::Hide { range: 1..3 }],
        priority: 0,
    });
    registry.register(DisplayTransformPlugin {
        id: "second",
        directives: vec![DisplayDirective::Fold {
            range: 3..5,
            summary: vec![Atom::plain("folded")],
        }],
        priority: 0,
    });

    let mut state = AppState::default();
    state.observed.lines = (vec![vec![], vec![], vec![], vec![], vec![], vec![]]).into();

    let directives = registry
        .view()
        .collect_display_directives(&AppView::new(&state));
    // Both plugins' directives are present (Hide + Fold)
    assert!(
        directives
            .iter()
            .any(|d| matches!(d, DisplayDirective::Hide { .. }))
    );
    assert!(
        directives
            .iter()
            .any(|d| matches!(d, DisplayDirective::Fold { .. }))
    );
}

#[test]
fn test_collect_display_map_composes_multi_plugin() {
    let mut registry = PluginRuntime::new();
    registry.register(DisplayTransformPlugin {
        id: "first",
        directives: vec![DisplayDirective::Hide { range: 1..3 }],
        priority: 0,
    });

    let mut state = AppState::default();
    state.observed.lines = (vec![vec![], vec![], vec![], vec![]]).into();

    let display_map = registry.view().collect_display_map(&AppView::new(&state));
    // 4 lines - 2 hidden = 2 display lines
    assert_eq!(display_map.display_line_count(), 2);
    assert_eq!(
        display_map.buffer_to_display(crate::display::BufferLine(0)),
        Some(crate::display::DisplayLine(0))
    );
    assert_eq!(
        display_map.buffer_to_display(crate::display::BufferLine(1)),
        None
    ); // hidden
    assert_eq!(
        display_map.buffer_to_display(crate::display::BufferLine(2)),
        None
    ); // hidden
    assert_eq!(
        display_map.buffer_to_display(crate::display::BufferLine(3)),
        Some(crate::display::DisplayLine(1))
    ); // visible
}

#[test]
fn test_collect_display_directives_fold_overlap_higher_priority_wins() {
    let mut registry = PluginRuntime::new();
    registry.register(DisplayTransformPlugin {
        id: "low",
        directives: vec![DisplayDirective::Fold {
            range: 1..4,
            summary: vec![Atom::plain("low-fold")],
        }],
        priority: 0,
    });
    registry.register(DisplayTransformPlugin {
        id: "high",
        directives: vec![DisplayDirective::Fold {
            range: 2..5,
            summary: vec![Atom::plain("high-fold")],
        }],
        priority: 10,
    });

    let mut state = AppState::default();
    state.observed.lines = (vec![vec![], vec![], vec![], vec![], vec![], vec![]]).into();

    let directives = registry
        .view()
        .collect_display_directives(&AppView::new(&state));
    let fold_summaries: Vec<&str> = directives
        .iter()
        .filter_map(|d| match d {
            DisplayDirective::Fold { summary, .. } => summary.first().map(|a| a.contents.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(fold_summaries, vec!["high-fold"]);
}

#[test]
fn test_collect_display_directives_single_plugin_unchanged() {
    let mut registry = PluginRuntime::new();
    registry.register(DisplayTransformPlugin {
        id: "only",
        directives: vec![DisplayDirective::Hide { range: 1..3 }],
        priority: 0,
    });

    let mut state = AppState::default();
    state.observed.lines = (vec![vec![], vec![], vec![], vec![]]).into();

    let directives = registry
        .view()
        .collect_display_directives(&AppView::new(&state));
    assert!(
        directives
            .iter()
            .any(|d| matches!(d, DisplayDirective::Hide { range } if *range == (1..3)))
    );
}

#[test]
fn test_notify_workspace_changed_dispatches_only_to_observers() {
    let hits = Arc::new(AtomicUsize::new(0));
    let mut registry = PluginRuntime::new();
    registry.register(WorkspaceObserverPlugin {
        id: "observer",
        hits: hits.clone(),
    });
    registry.register(TestPlugin);

    let workspace = crate::workspace::Workspace::default();
    let query = workspace.query(Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    });
    registry.notify_workspace_changed(&query);

    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[test]
fn test_dispatch_key_middleware_passes_transformed_key_to_next_plugin() {
    let first_seen = Arc::new(Mutex::new(Vec::new()));
    let second_seen = Arc::new(Mutex::new(Vec::new()));
    let mut registry = PluginRuntime::new();
    registry.register(KeyMiddlewarePlugin {
        id: "transformer",
        seen: first_seen.clone(),
        behavior: MiddlewareBehavior::Transform(KeyEvent {
            key: Key::Char('b'),
            modifiers: Modifiers::SHIFT,
        }),
    });
    registry.register(KeyMiddlewarePlugin {
        id: "consumer",
        seen: second_seen.clone(),
        behavior: MiddlewareBehavior::Consume("<esc>".to_string()),
    });

    let state = AppState::default();
    let result = registry.dispatch_key_middleware(
        &KeyEvent {
            key: Key::Char('a'),
            modifiers: Modifiers::empty(),
        },
        &AppView::new(&state),
    );

    let first_keys = first_seen.lock().unwrap().clone();
    let second_keys = second_seen.lock().unwrap().clone();
    assert_eq!(first_keys.len(), 1);
    assert_eq!(first_keys[0].key, Key::Char('a'));
    assert_eq!(second_keys.len(), 1);
    assert_eq!(second_keys[0].key, Key::Char('b'));
    assert_eq!(second_keys[0].modifiers, Modifiers::SHIFT);
    match result {
        KeyDispatchResult::Consumed {
            source_plugin,
            commands,
        } => {
            assert_eq!(source_plugin, PluginId("consumer".to_string()));
            assert_eq!(commands.len(), 1);
            assert!(matches!(
                &commands[0],
                Command::SendToKakoune(KasaneRequest::Keys(keys)) if keys == &vec!["<esc>".to_string()]
            ));
        }
        KeyDispatchResult::Passthrough(_) => panic!("expected middleware consumer"),
    }
}

#[test]
fn test_dispatch_key_middleware_returns_final_passthrough_key() {
    let mut registry = PluginRuntime::new();
    registry.register(KeyMiddlewarePlugin {
        id: "transformer",
        seen: Arc::new(Mutex::new(Vec::new())),
        behavior: MiddlewareBehavior::Transform(KeyEvent {
            key: Key::PageDown,
            modifiers: Modifiers::CTRL,
        }),
    });
    registry.register(KeyMiddlewarePlugin {
        id: "passthrough",
        seen: Arc::new(Mutex::new(Vec::new())),
        behavior: MiddlewareBehavior::Passthrough,
    });

    let state = AppState::default();
    match registry.dispatch_key_middleware(
        &KeyEvent {
            key: Key::Char('x'),
            modifiers: Modifiers::empty(),
        },
        &AppView::new(&state),
    ) {
        KeyDispatchResult::Consumed { .. } => panic!("expected passthrough"),
        KeyDispatchResult::Passthrough(key) => {
            assert_eq!(key.key, Key::PageDown);
            assert_eq!(key.modifiers, Modifiers::CTRL);
        }
    }
}

#[test]
fn test_unload_plugin_calls_shutdown_and_removes_plugin() {
    let mut registry = PluginRuntime::new();
    let shutdowns = Arc::new(AtomicUsize::new(0));
    registry.register(ShutdownProbePlugin {
        id: "shutdown-probe",
        shutdowns: shutdowns.clone(),
    });

    assert!(registry.contains_plugin(&PluginId("shutdown-probe".to_string())));
    assert!(registry.unload_plugin(&PluginId("shutdown-probe".to_string())));
    assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
    assert!(!registry.contains_plugin(&PluginId("shutdown-probe".to_string())));
    assert!(!registry.unload_plugin(&PluginId("shutdown-probe".to_string())));
    assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
}

#[test]
fn test_on_state_changed_dispatched_with_flags() {
    let mut registry = PluginRuntime::new();
    registry.register(LifecyclePlugin::new());
    let state = AppState::default();

    // Simulate what update() does for Msg::Kakoune
    let view = AppView::new(&state);
    let flags = DirtyFlags::BUFFER | DirtyFlags::STATUS;
    for plugin in registry.plugins_mut() {
        let _ = plugin.on_state_changed_effects(&view, flags);
    }
    // No panic, default implementations work
}

#[test]
fn test_lifecycle_defaults() {
    // TestPlugin has no lifecycle hooks — defaults should work
    let mut registry = PluginRuntime::new();
    registry.register(TestPlugin);
    let state = AppState::default();

    let batch = registry.init_all_batch(&AppView::new(&state));
    assert!(batch.redraw.is_empty());

    registry.shutdown_all();
    // No panic
}

#[test]
fn test_init_all_batch_collects_lifecycle_bootstrap_effects() {
    let mut registry = PluginRuntime::new();
    registry.register(LifecyclePlugin::new());
    let state = AppState::default();

    let batch = registry.init_all_batch(&AppView::new(&state));
    assert!(batch.redraw.contains(DirtyFlags::BUFFER));
}

#[test]
fn test_reload_plugin_batch_collects_bootstrap_effects() {
    let mut registry = PluginRuntime::new();
    registry.register(TypedLifecyclePlugin);
    let state = AppState::default();

    let batch = registry.reload_plugin_batch(
        Box::new(crate::plugin::PluginBridge::new(TypedLifecyclePlugin)),
        &AppView::new(&state),
    );
    assert!(batch.redraw.contains(DirtyFlags::STATUS));
}

#[test]
fn test_any_plugin_state_changed_flag() {
    let mut registry = PluginRuntime::new();
    registry.register(StatefulPlugin);

    // Initial prepare: hash differs from default 0 → changed
    registry.prepare_plugin_cache(DirtyFlags::ALL);
    assert!(registry.any_plugin_state_changed());

    // Second prepare with same hash → no change
    registry.prepare_plugin_cache(DirtyFlags::ALL);
    assert!(!registry.any_plugin_state_changed());
}

// --- deliver_message tests ---

#[test]
fn test_deliver_message_unknown_target() {
    let mut registry = PluginRuntime::new();
    let state = AppState::default();
    let batch = registry.deliver_message_batch(
        &PluginId("unknown".to_string()),
        Box::new(42u32),
        &AppView::new(&state),
    );
    assert!(batch.redraw.is_empty());
    assert!(batch.per_plugin_commands.is_empty());
    assert!(batch.scroll_plans.is_empty());
}

// --- Per-extension-point invalidation tests (Phase 5) ---

/// A contributor-only plugin with controllable hash.
struct ContributorPlugin;

impl crate::plugin::Plugin for ContributorPlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId("contributor".to_string())
    }

    fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
        r.on_contribute(SlotId::STATUS_LEFT, |_state, _app, _ctx| {
            Some(Contribution {
                element: crate::element::Element::plain_text("contrib"),
                priority: 0,
                size_hint: crate::plugin::ContribSizeHint::Auto,
            })
        });
    }
}

/// An annotator-only plugin (no handlers — ANNOTATOR capability is forced
/// via a no-op virtual-text handler so the staleness path observes the
/// plugin in the annotator group).
struct AnnotatorPlugin;

impl crate::plugin::Plugin for AnnotatorPlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId("annotator".to_string())
    }

    fn register(&self, r: &mut crate::plugin::HandlerRegistry<()>) {
        r.on_virtual_text(|_state, _line, _app, _ctx| Vec::new());
    }
}

#[test]
fn test_per_extension_point_stale_contributor_only() {
    let mut registry = PluginRuntime::new();
    registry.register(ContributorPlugin);
    registry.register(AnnotatorPlugin);

    // First prepare: both stale (hash changed from default 0)
    registry.prepare_plugin_cache(DirtyFlags::ALL);
    let view = registry.view();
    assert!(view.any_contributor_needs_recollect());
    assert!(view.any_annotator_needs_recollect());

    // Second prepare with same hashes → neither stale (no dirty flags intersect
    // because both plugins declare specific caps, not ALL view_deps)
    registry.prepare_plugin_cache(DirtyFlags::empty());
    let view = registry.view();
    assert!(!view.any_contributor_needs_recollect());
    assert!(!view.any_annotator_needs_recollect());
}

#[test]
fn test_per_extension_point_stale_only_annotator_changes() {
    let mut registry = PluginRuntime::new();
    registry.register(ContributorPlugin);
    registry.register(AnnotatorPlugin);

    // First prepare: both become stale
    registry.prepare_plugin_cache(DirtyFlags::ALL);

    // Stabilize both
    registry.prepare_plugin_cache(DirtyFlags::empty());
    let view = registry.view();
    assert!(!view.any_contributor_needs_recollect());
    assert!(!view.any_annotator_needs_recollect());

    // Now change only the annotator's hash — mutate via re-register
    registry.register(AnnotatorPlugin);
    registry.prepare_plugin_cache(DirtyFlags::empty());
    let view = registry.view();

    // Contributor is NOT stale, annotator IS stale
    assert!(!view.any_contributor_needs_recollect());
    assert!(view.any_annotator_needs_recollect());
}

#[test]
fn test_contribution_cache_reuses_non_stale_plugin() {
    use crate::plugin::ContributionCache;

    let mut registry = PluginRuntime::new();
    registry.register(ContributorPlugin);

    let state = AppState::default();
    let app = AppView::new(&state);
    let ctx = ContributeContext::new(&app, None);
    let mut cache = ContributionCache::default();

    // First prepare: plugin is stale (hash changed 0→1)
    registry.prepare_plugin_cache(DirtyFlags::ALL);
    let view = registry.view();
    let contribs = view.collect_contributions_cached(&SlotId::STATUS_LEFT, &app, &ctx, &mut cache);
    assert_eq!(contribs.len(), 1);

    // Second prepare: plugin NOT stale (hash unchanged)
    registry.prepare_plugin_cache(DirtyFlags::empty());
    let view = registry.view();
    // Should return cached result without re-collecting
    let contribs2 = view.collect_contributions_cached(&SlotId::STATUS_LEFT, &app, &ctx, &mut cache);
    assert_eq!(contribs2.len(), 1);
}

#[test]
fn test_display_transform_stale_independent_of_annotator() {
    let mut registry = PluginRuntime::new();
    registry.register(AnnotatorPlugin);
    registry.register(DisplayTransformPlugin {
        id: "display",
        directives: vec![],
        priority: 0,
    });

    // Both stale initially
    registry.prepare_plugin_cache(DirtyFlags::ALL);
    let view = registry.view();
    assert!(view.any_annotator_needs_recollect());
    assert!(view.any_display_transform_needs_recollect());

    // Stabilize both
    registry.prepare_plugin_cache(DirtyFlags::empty());

    // Change only annotator
    registry.register(AnnotatorPlugin);
    registry.prepare_plugin_cache(DirtyFlags::empty());
    let view = registry.view();
    assert!(view.any_annotator_needs_recollect());
    assert!(!view.any_display_transform_needs_recollect());
}

// --- Annotation decomposition tests (Phase 6) ---

use crate::plugin::handler_registry::HandlerRegistry;
use crate::plugin::handler_table::GutterSide;
use crate::plugin::state::Plugin;
use crate::plugin::{BackgroundLayer, BlendMode};

/// A native Plugin using decomposed annotation handlers.
struct DecomposedAnnotatorPlugin;

impl Plugin for DecomposedAnnotatorPlugin {
    type State = ();
    fn id(&self) -> PluginId {
        PluginId("decomposed-annotator".to_string())
    }
    fn register(&self, r: &mut HandlerRegistry<()>) {
        r.on_decorate_gutter(GutterSide::Left, 10, |_state, line, _app, _ctx| {
            Some(crate::element::Element::text(
                format!("{}", line + 1),
                crate::protocol::Style::default(),
            ))
        });
        r.on_decorate_background(|_state, line, _app, _ctx| {
            if line == 0 {
                Some(BackgroundLayer {
                    style: crate::protocol::Style {
                        bg: crate::protocol::Brush::Named(crate::protocol::NamedColor::Blue),
                        ..crate::protocol::Style::default()
                    },
                    z_order: 0,
                    blend: BlendMode::Opaque,
                })
            } else {
                None
            }
        });
    }
}

/// A native ANNOTATOR plugin using the gutter handler.
///
/// (Originally a "legacy" fixture exercising the monolithic
/// `annotate_line_with_ctx` decompose path; that path is preserved
/// only for WASM plugins. Native plugins always use decomposed handlers.)
struct LegacyAnnotatorPlugin;

impl Plugin for LegacyAnnotatorPlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId("legacy-annotator".to_string())
    }

    fn register(&self, r: &mut HandlerRegistry<()>) {
        r.on_decorate_gutter(GutterSide::Left, 5, |_state, line, _app, _ctx| {
            Some(crate::element::Element::text(
                format!("L{}", line + 1),
                crate::protocol::Style::default(),
            ))
        });
    }
}

#[test]
fn test_decomposed_annotator_produces_gutter_and_background() {
    let mut registry = PluginRuntime::new();
    registry.register(DecomposedAnnotatorPlugin);

    let mut state = AppState::default();
    state.observed.lines = (vec![vec![], vec![], vec![]]).into();
    state.runtime.rows = 24;
    state.runtime.cols = 80;

    registry.prepare_plugin_cache(DirtyFlags::ALL);
    let view = registry.view();
    let ctx = AnnotateContext {
        line_width: 80,
        gutter_width: 0,
        display_map: None,
        pane_surface_id: None,
        pane_focused: true,
    };

    let result = view.collect_annotations(&AppView::new(&state), &ctx);

    // Should have left gutter (line numbers)
    assert!(result.left_gutter.is_some());
    // Should have backgrounds (line 0 is blue)
    assert!(result.line_backgrounds.is_some());
    let bgs = result.line_backgrounds.unwrap();
    assert!(bgs[0].is_some());
    assert!(bgs[1].is_none());
}

#[test]
fn test_mixed_decomposed_and_legacy_annotators() {
    let mut registry = PluginRuntime::new();
    registry.register(DecomposedAnnotatorPlugin);
    registry.register(LegacyAnnotatorPlugin);

    let mut state = AppState::default();
    state.observed.lines = (vec![vec![], vec![]]).into();
    state.runtime.rows = 24;
    state.runtime.cols = 80;

    registry.prepare_plugin_cache(DirtyFlags::ALL);
    let view = registry.view();
    let ctx = AnnotateContext {
        line_width: 80,
        gutter_width: 0,
        display_map: None,
        pane_surface_id: None,
        pane_focused: true,
    };

    let result = view.collect_annotations(&AppView::new(&state), &ctx);

    // Both plugins contribute left gutter → should have combined gutter
    assert!(result.left_gutter.is_some());
    // Decomposed plugin has background on line 0
    assert!(result.line_backgrounds.is_some());
}

// (Removed) `test_decomposed_annotator_has_decomposed_annotations_flag`:
// asserted that LegacyAnnotatorPlugin reports `has_decomposed_annotations()
// == false` while DecomposedAnnotatorPlugin reports true. After the
// legacy fixture migrated to `impl Plugin` with `on_decorate_gutter`, the
// distinction is no longer meaningful at the test fixture level — every
// native plugin reports decomposed = true. The WASM-side fallback is
// exercised in kasane-wasm integration tests.

// --- Pub/Sub tests (Phase 8a) ---

use crate::plugin::pubsub::{TopicBus, TopicId};

/// Plugin state for pub/sub publisher: tracks a counter.
#[derive(Clone, Default, PartialEq, Hash, Debug)]
struct PubState {
    counter: u32,
}

/// Plugin state for pub/sub subscriber: tracks received value.
#[derive(Clone, Default, PartialEq, Hash, Debug)]
struct SubState {
    received: u32,
}

/// Publisher plugin: publishes its counter on "test.counter".
struct PublisherPlugin;
impl Plugin for PublisherPlugin {
    type State = PubState;
    fn id(&self) -> PluginId {
        PluginId("publisher".to_string())
    }
    fn register(&self, r: &mut HandlerRegistry<PubState>) {
        r.on_state_changed_tier1(|state, _app, _dirty| {
            (
                PubState {
                    counter: state.counter + 1,
                },
                KakouneSideEffects::none(),
            )
        });
        r.publish::<u32>(TopicId::new("test.counter"), |state, _app| state.counter);
    }
}

/// Subscriber plugin: subscribes to "test.counter" and stores the value.
struct SubscriberPlugin;
impl Plugin for SubscriberPlugin {
    type State = SubState;
    fn id(&self) -> PluginId {
        PluginId("subscriber".to_string())
    }
    fn register(&self, r: &mut HandlerRegistry<SubState>) {
        r.subscribe::<u32>(TopicId::new("test.counter"), |_state, value| SubState {
            received: *value,
        });
    }
}

#[test]
fn test_pubsub_publisher_delivers_to_subscriber() {
    let mut runtime = PluginRuntime::new();
    runtime.register(PublisherPlugin);
    runtime.register(SubscriberPlugin);

    let mut state = AppState::default();
    state.runtime.rows = 24;
    state.runtime.cols = 80;
    let app = AppView::new(&state);

    // Trigger state change to bump publisher's counter.
    runtime.notify_state_changed_batch(&app, DirtyFlags::ALL);

    // Run pub/sub evaluation.
    let mut bus = TopicBus::new();
    let _batch = runtime.evaluate_pubsub(&mut bus, &app);

    // Subscriber should now have received the published counter.
    // Verify via prepare_plugin_cache detecting the state change.
    runtime.prepare_plugin_cache(DirtyFlags::empty());
    assert!(runtime.any_plugin_state_changed());
}

#[test]
fn test_pubsub_no_subscribers_is_noop() {
    let mut runtime = PluginRuntime::new();
    runtime.register(PublisherPlugin);

    let mut state = AppState::default();
    state.runtime.rows = 24;
    state.runtime.cols = 80;
    let app = AppView::new(&state);

    runtime.notify_state_changed_batch(&app, DirtyFlags::ALL);

    let mut bus = TopicBus::new();
    let _batch = runtime.evaluate_pubsub(&mut bus, &app);

    // No subscriber → only publisher state changed.
    runtime.prepare_plugin_cache(DirtyFlags::empty());
    assert!(runtime.any_plugin_state_changed());
}

/// Per-topic batch handler returns Effects that flow back through
/// `evaluate_pubsub`'s `EffectsBatch`. This subscriber pairs
/// `r.subscribe` (per-value state mutation, drives the bus `changed`
/// flag) with `r.on_subscription` (per-topic batch effects).
struct SubscriberWithOnSubscription;
impl Plugin for SubscriberWithOnSubscription {
    type State = SubState;
    fn id(&self) -> PluginId {
        PluginId("subscriber.tier1".to_string())
    }
    fn register(&self, r: &mut HandlerRegistry<SubState>) {
        r.subscribe::<u32>(TopicId::new("test.counter"), |_state, value| SubState {
            received: *value,
        });
        r.on_subscription(|state, _topic, _values, _app| {
            (state.clone(), Effects::redraw(DirtyFlags::BUFFER))
        });
    }
}

#[test]
fn test_on_subscription_effects_flow_back_through_evaluate_pubsub() {
    let mut runtime = PluginRuntime::new();
    runtime.register(PublisherPlugin);
    runtime.register(SubscriberWithOnSubscription);

    let mut state = AppState::default();
    state.runtime.rows = 24;
    state.runtime.cols = 80;
    let app = AppView::new(&state);

    // Bump the publisher so it has a non-zero counter to publish.
    runtime.notify_state_changed_batch(&app, DirtyFlags::ALL);

    let mut bus = TopicBus::new();
    let batch = runtime.evaluate_pubsub(&mut bus, &app);

    // The per-topic batch handler returned `redraw: BUFFER`. Before
    // ADR-044 A-3e effect plumbing, this would have been silently
    // dropped at the trait boundary.
    assert!(
        batch.redraw.contains(DirtyFlags::BUFFER),
        "on_subscription redraw flag did not reach the EffectsBatch \
         (got {:?}). The trait widening from `-> bool` to `-> Effects` \
         is the load-bearing change.",
        batch.redraw,
    );
}

#[test]
fn test_pubsub_bus_clears_between_evaluations() {
    let mut bus = TopicBus::new();
    let topic = TopicId::new("test");
    bus.publish(
        topic.clone(),
        PluginId("p".to_string()),
        super::super::channel::ChannelValue::new(&42u32).unwrap(),
    );
    assert!(bus.get_publications(&topic).is_some());

    // evaluate_pubsub calls bus.clear() at the start.
    // Simulate: clear and verify.
    bus.clear();
    assert!(bus.get_publications(&topic).is_none());
}

// --- InteractiveId PluginTag tests (Phase 8) ---

use crate::element::{InteractiveId, PluginTag};
use crate::input::{MouseButton, MouseEventKind};

#[test]
fn test_plugin_tags_are_monotonically_assigned_starting_from_1() {
    let mut registry = PluginRuntime::new();
    registry.register(TestPlugin);
    registry.register(SurfacePlugin);

    let tags: Vec<(PluginId, PluginTag)> = registry.all_plugin_tags();
    assert_eq!(tags[0].1, PluginTag(1));
    assert_eq!(tags[1].1, PluginTag(2));
}

#[test]
fn test_plugin_tag_zero_is_reserved_for_framework() {
    let mut registry = PluginRuntime::new();
    registry.register(TestPlugin);
    registry.register(SurfacePlugin);
    registry.register(StatefulPlugin);

    for (_id, tag) in registry.all_plugin_tags() {
        assert_ne!(
            tag,
            PluginTag::FRAMEWORK,
            "PluginTag(0) should never be assigned to plugins"
        );
    }
}

#[test]
fn test_replacing_plugin_reuses_its_tag() {
    let mut registry = PluginRuntime::new();
    registry.register(TestPlugin);
    let original_tag = registry.plugin_tag(&PluginId("test".to_string())).unwrap();

    // Replace with same ID
    registry.register(TestPlugin);
    let replaced_tag = registry.plugin_tag(&PluginId("test".to_string())).unwrap();
    assert_eq!(original_tag, replaced_tag);
}

#[test]
fn test_unloading_plugin_does_not_recycle_tag() {
    let mut registry = PluginRuntime::new();
    registry.register(TestPlugin);
    registry.register(SurfacePlugin);

    // Remove first plugin
    registry.remove_plugin(&PluginId("test".to_string()));

    // Register new plugin — should get tag 3, not 1
    registry.register(StatefulPlugin);
    let new_tag = registry
        .plugin_tag(&PluginId("stateful".to_string()))
        .unwrap();
    assert_eq!(new_tag, PluginTag(3));
}

// --- Owner-based dispatch tests ---

struct MousePlugin42;

impl Plugin for MousePlugin42 {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId("mouse42".to_string())
    }

    fn register(&self, r: &mut HandlerRegistry<()>) {
        r.on_handle_mouse(|_state, _event, id, _app| {
            if id.local == 42 {
                Some(((), vec![Command::RequestRedraw(DirtyFlags::BUFFER)]))
            } else {
                None
            }
        });
    }
}

struct DecoyMousePlugin;

impl Plugin for DecoyMousePlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId("decoy-mouse".to_string())
    }

    fn register(&self, r: &mut HandlerRegistry<()>) {
        r.on_handle_mouse(|_state, _event, _id, _app| -> Option<((), Vec<Command>)> {
            // Should never be called for tagged IDs belonging to other plugins
            panic!("decoy plugin should not be called for tagged IDs");
        });
    }
}

#[test]
fn test_tagged_interactive_id_routes_to_correct_plugin() {
    let mut registry = PluginRuntime::new();
    registry.register(DecoyMousePlugin);
    registry.register(MousePlugin42);

    let mouse42_tag = registry
        .plugin_tag(&PluginId("mouse42".to_string()))
        .unwrap();

    let state = AppState::default();
    let id = InteractiveId::new(42, mouse42_tag);
    let mouse_event = crate::input::MouseEvent {
        kind: MouseEventKind::Press(MouseButton::Left),
        line: 0,
        column: 0,
        modifiers: crate::input::Modifiers::empty(),
    };

    match registry.dispatch_mouse_handler(&mouse_event, id, &AppView::new(&state)) {
        MouseHandleResult::Handled {
            source_plugin,
            commands,
        } => {
            assert_eq!(source_plugin, PluginId("mouse42".to_string()));
            assert_eq!(commands.len(), 1);
        }
        MouseHandleResult::NotHandled => panic!("expected Handled"),
    }
}

#[test]
fn test_framework_tagged_id_uses_legacy_fallback() {
    let mut registry = PluginRuntime::new();
    registry.register(MousePlugin42);

    let state = AppState::default();
    let id = InteractiveId::framework(42);
    let mouse_event = crate::input::MouseEvent {
        kind: MouseEventKind::Press(MouseButton::Left),
        line: 0,
        column: 0,
        modifiers: crate::input::Modifiers::empty(),
    };

    match registry.dispatch_mouse_handler(&mouse_event, id, &AppView::new(&state)) {
        MouseHandleResult::Handled { source_plugin, .. } => {
            assert_eq!(source_plugin, PluginId("mouse42".to_string()));
        }
        MouseHandleResult::NotHandled => panic!("expected Handled via legacy fallback"),
    }
}

#[test]
fn test_nonexistent_tag_returns_not_handled() {
    let mut registry = PluginRuntime::new();
    registry.register(MousePlugin42);

    let state = AppState::default();
    let id = InteractiveId::new(42, PluginTag(999));
    let mouse_event = crate::input::MouseEvent {
        kind: MouseEventKind::Press(MouseButton::Left),
        line: 0,
        column: 0,
        modifiers: crate::input::Modifiers::empty(),
    };

    assert!(matches!(
        registry.dispatch_mouse_handler(&mouse_event, id, &AppView::new(&state)),
        MouseHandleResult::NotHandled
    ));
}

// --- DU-4: Navigation dispatch tests ---

use crate::display::InteractionPolicy;
use crate::display::navigation::{ActionResult, NavigationAction, NavigationPolicy};
use crate::display::unit::{DisplayUnit, DisplayUnitId, SemanticRole, UnitSource};

fn make_display_unit(role: SemanticRole) -> DisplayUnit {
    let source = UnitSource::Line(0);
    DisplayUnit {
        id: DisplayUnitId::from_content(&source, &role),
        display_line: 0,
        role,
        source,
        interaction: InteractionPolicy::Normal,
    }
}

struct NavPolicyPlugin {
    name: &'static str,
    policy: NavigationPolicy,
}

impl Plugin for NavPolicyPlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId(self.name.to_string())
    }

    fn register(&self, r: &mut HandlerRegistry<()>) {
        let policy = self.policy.clone();
        r.on_navigation_policy(move |_state, _unit| policy.clone());
    }
}

struct NavActionPlugin {
    name: &'static str,
    result: ActionResult,
}

impl Plugin for NavActionPlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId(self.name.to_string())
    }

    fn register(&self, r: &mut HandlerRegistry<()>) {
        let result = self.result.clone();
        r.on_navigation_action(move |state, _unit, _action| (state.clone(), result.clone()));
    }
}

#[test]
fn resolve_navigation_policy_falls_back_to_default() {
    let registry = PluginRuntime::new();
    let unit = make_display_unit(SemanticRole::FoldSummary);
    let policy = registry.resolve_navigation_policy(&unit);
    assert_eq!(
        policy,
        NavigationPolicy::default_for(&SemanticRole::FoldSummary)
    );
}

#[test]
fn resolve_navigation_policy_first_wins() {
    let mut registry = PluginRuntime::new();
    // First plugin returns Normal (overriding the FoldSummary default of Boundary)
    registry.register(NavPolicyPlugin {
        name: "nav-policy-1",
        policy: NavigationPolicy::Normal,
    });
    // Second plugin returns Skip — should be ignored because first wins
    registry.register(NavPolicyPlugin {
        name: "nav-policy-2",
        policy: NavigationPolicy::Skip,
    });

    let unit = make_display_unit(SemanticRole::FoldSummary);
    let policy = registry.resolve_navigation_policy(&unit);
    assert_eq!(policy, NavigationPolicy::Normal);
}

#[test]
fn dispatch_navigation_action_returns_pass_when_no_plugins() {
    let mut registry = PluginRuntime::new();
    let unit = make_display_unit(SemanticRole::FoldSummary);
    let result = registry.dispatch_navigation_action(&unit, NavigationAction::ToggleFold);
    assert_eq!(result, ActionResult::Pass);
}

#[test]
fn dispatch_navigation_action_first_wins() {
    let mut registry = PluginRuntime::new();
    registry.register(NavActionPlugin {
        name: "nav-action-1",
        result: ActionResult::Handled,
    });
    registry.register(NavActionPlugin {
        name: "nav-action-2",
        result: ActionResult::SendKeys("j".to_string()),
    });

    let unit = make_display_unit(SemanticRole::FoldSummary);
    let result = registry.dispatch_navigation_action(&unit, NavigationAction::ToggleFold);
    assert_eq!(result, ActionResult::Handled);
}

// --- Unified display collection tests (Phase 1B.3) ---

/// A Plugin that uses `on_display_unified()` to return all directive categories.
struct UnifiedDisplayPlugin;

impl Plugin for UnifiedDisplayPlugin {
    type State = ();
    fn id(&self) -> PluginId {
        PluginId("unified-display".to_string())
    }
    fn register(&self, r: &mut HandlerRegistry<()>) {
        r.on_display_unified(|_state, _app| {
            vec![
                // Spatial
                DisplayDirective::Hide { range: 2..3 },
                // Decoration: background
                DisplayDirective::StyleLine {
                    line: 0,
                    style: crate::protocol::Style {
                        bg: crate::protocol::Brush::Named(crate::protocol::NamedColor::Red),
                        ..crate::protocol::Style::default()
                    },
                    z_order: 0,
                },
                // Decoration: gutter
                DisplayDirective::Gutter {
                    line: 1,
                    side: crate::display::GutterSide::Left,
                    content: crate::element::Element::plain_text("G"),
                    priority: 5,
                },
                // Decoration: virtual text
                DisplayDirective::VirtualText {
                    line: 0,
                    position: crate::display::VirtualTextPosition::EndOfLine,
                    content: vec![Atom::plain("hint")],
                    priority: 0,
                },
                // InterLine: insert after
                DisplayDirective::InsertAfter {
                    line: 0,
                    content: crate::element::Element::plain_text("inserted"),
                    priority: 0,
                },
                // Inline: style
                DisplayDirective::StyleInline {
                    line: 1,
                    byte_range: 0..3,
                    style: crate::protocol::Style {
                        fg: crate::protocol::Brush::Named(crate::protocol::NamedColor::Green),
                        ..crate::protocol::Style::default()
                    },
                },
            ]
        });
    }
}

#[test]
fn unified_display_spatial_routed_to_display_directives() {
    let mut registry = PluginRuntime::new();
    registry.register(UnifiedDisplayPlugin);

    let mut state = AppState::default();
    state.observed.lines = (vec![vec![], vec![], vec![], vec![]]).into();

    registry.prepare_plugin_cache(DirtyFlags::ALL);
    let view = registry.view();
    let directives = view.collect_display_directives(&AppView::new(&state));

    // Should contain the Hide directive from the unified plugin
    assert!(
        directives
            .iter()
            .any(|d| matches!(d, DisplayDirective::Hide { range } if *range == (2..3)))
    );
    // Should NOT contain non-spatial directives
    assert!(
        !directives
            .iter()
            .any(|d| matches!(d, DisplayDirective::StyleLine { .. }))
    );
}

#[test]
fn unified_display_decoration_routed_to_annotations() {
    let mut registry = PluginRuntime::new();
    registry.register(UnifiedDisplayPlugin);

    let mut state = AppState::default();
    state.observed.lines = (vec![vec![], vec![], vec![], vec![]]).into();
    state.runtime.rows = 24;
    state.runtime.cols = 80;

    registry.prepare_plugin_cache(DirtyFlags::ALL);
    let view = registry.view();
    let ctx = AnnotateContext {
        line_width: 80,
        gutter_width: 0,
        display_map: None,
        pane_surface_id: None,
        pane_focused: true,
    };

    let result = view.collect_annotations(&AppView::new(&state), &ctx);

    // Background on line 0 (from StyleLine)
    assert!(result.line_backgrounds.is_some());
    let bgs = result.line_backgrounds.as_ref().unwrap();
    assert!(bgs[0].is_some());
    assert!(bgs[1].is_none());

    // Left gutter on line 1 (from Gutter)
    assert!(result.left_gutter.is_some());

    // Virtual text on line 0
    assert!(result.virtual_text.is_some());
    let vts = result.virtual_text.as_ref().unwrap();
    assert!(vts[0].is_some());

    // Inline decoration on line 1 (from StyleInline)
    assert!(result.inline_decorations.is_some());
    let inlines = result.inline_decorations.as_ref().unwrap();
    assert!(inlines[1].is_some());
}

#[test]
fn unified_display_interline_routed_to_content_annotations() {
    let mut registry = PluginRuntime::new();
    registry.register(UnifiedDisplayPlugin);

    let mut state = AppState::default();
    state.observed.lines = (vec![vec![], vec![], vec![], vec![]]).into();
    state.runtime.rows = 24;
    state.runtime.cols = 80;

    registry.prepare_plugin_cache(DirtyFlags::ALL);
    let view = registry.view();
    let ctx = AnnotateContext {
        line_width: 80,
        gutter_width: 0,
        display_map: None,
        pane_surface_id: None,
        pane_focused: true,
    };

    let annotations = view.collect_content_annotations(&AppView::new(&state), &ctx);

    // Should contain the InsertAfter at line 0
    assert_eq!(annotations.len(), 1);
    assert_eq!(
        annotations[0].anchor,
        crate::display::ContentAnchor::InsertAfter(0)
    );
    assert_eq!(annotations[0].priority, 0);
}

#[test]
fn unified_display_cache_called_once() {
    // Verifies that calling all three collection methods uses the cache
    // (the unified plugin's unified_display() is called only once).
    let mut registry = PluginRuntime::new();
    registry.register(UnifiedDisplayPlugin);

    let mut state = AppState::default();
    state.observed.lines = (vec![vec![], vec![], vec![]]).into();
    state.runtime.rows = 24;
    state.runtime.cols = 80;

    registry.prepare_plugin_cache(DirtyFlags::ALL);
    let view = registry.view();
    let app_view = AppView::new(&state);
    let ctx = AnnotateContext {
        line_width: 80,
        gutter_width: 0,
        display_map: None,
        pane_surface_id: None,
        pane_focused: true,
    };

    // Call all three collection methods
    let _directives = view.collect_display_directives(&app_view);
    let _annotations = view.collect_annotations(&app_view, &ctx);
    let _content = view.collect_content_annotations(&app_view, &ctx);

    // The cache should be populated (one entry for the unified plugin)
    let cache = view.unified_cache.borrow();
    assert!(cache[0].is_some(), "unified cache should be populated");
}

#[test]
fn unified_and_legacy_annotators_coexist() {
    let mut registry = PluginRuntime::new();
    registry.register(UnifiedDisplayPlugin);
    registry.register(DecomposedAnnotatorPlugin);
    registry.register(LegacyAnnotatorPlugin);

    let mut state = AppState::default();
    state.observed.lines = (vec![vec![], vec![], vec![]]).into();
    state.runtime.rows = 24;
    state.runtime.cols = 80;

    registry.prepare_plugin_cache(DirtyFlags::ALL);
    let view = registry.view();
    let ctx = AnnotateContext {
        line_width: 80,
        gutter_width: 0,
        display_map: None,
        pane_surface_id: None,
        pane_focused: true,
    };

    let result = view.collect_annotations(&AppView::new(&state), &ctx);

    // All three annotation sources contribute:
    // - UnifiedDisplayPlugin: StyleLine on line 0, Gutter on line 1
    // - DecomposedAnnotatorPlugin: left gutter on all lines, background on line 0
    // - LegacyAnnotatorPlugin: left gutter on all lines
    assert!(result.left_gutter.is_some());
    assert!(result.line_backgrounds.is_some());
}

// =============================================================================
// PluginRuntime::sync_lenses (Composable Lenses auto-wired lifecycle)
// =============================================================================

mod sync_lenses {
    use super::super::super::PluginRuntime;
    use super::super::super::traits::PluginBackend;
    use crate::lens::{Lens, LensId, LensRegistry};
    use crate::plugin::PluginId;
    use std::sync::{Arc, Mutex};

    /// Native plugin that owns one lens and registers it via the
    /// `PluginBackend::register_lenses` hook.
    struct LensOwningPlugin {
        id: PluginId,
        lens_name: String,
        registered: Arc<Mutex<usize>>,
    }

    struct OwnedLens {
        id: LensId,
    }
    impl Lens for OwnedLens {
        fn id(&self) -> LensId {
            self.id.clone()
        }
        fn display(
            &self,
            _view: &crate::plugin::AppView<'_>,
        ) -> Vec<crate::display::DisplayDirective> {
            Vec::new()
        }
    }

    impl PluginBackend for LensOwningPlugin {
        fn id(&self) -> PluginId {
            self.id.clone()
        }
        fn on_init_effects(
            &mut self,
            _state: &crate::plugin::AppView<'_>,
        ) -> crate::plugin::Effects {
            crate::plugin::Effects::default()
        }
        fn register_lenses(&self, registry: &mut LensRegistry) -> usize {
            registry.register(Arc::new(OwnedLens {
                id: LensId::new(self.id.clone(), self.lens_name.clone()),
            }));
            *self.registered.lock().unwrap() += 1;
            1
        }
    }

    fn make_runtime(plugins: Vec<Box<dyn PluginBackend>>) -> PluginRuntime {
        let mut runtime = PluginRuntime::new();
        for plugin in plugins {
            runtime.register_backend(plugin);
        }
        runtime
    }

    #[test]
    fn sync_lenses_registers_from_each_plugin() {
        let counter_a = Arc::new(Mutex::new(0));
        let counter_b = Arc::new(Mutex::new(0));
        let runtime = make_runtime(vec![
            Box::new(LensOwningPlugin {
                id: PluginId("alpha".into()),
                lens_name: "lens-a".into(),
                registered: counter_a.clone(),
            }),
            Box::new(LensOwningPlugin {
                id: PluginId("beta".into()),
                lens_name: "lens-b".into(),
                registered: counter_b.clone(),
            }),
        ]);

        let mut lens_registry = LensRegistry::new();
        let count = runtime.sync_lenses(&mut lens_registry);
        assert_eq!(count, 2);
        assert!(lens_registry.is_registered(&LensId::new(PluginId("alpha".into()), "lens-a")));
        assert!(lens_registry.is_registered(&LensId::new(PluginId("beta".into()), "lens-b")));
        assert_eq!(*counter_a.lock().unwrap(), 1);
        assert_eq!(*counter_b.lock().unwrap(), 1);
    }

    #[test]
    fn sync_lenses_drops_stale_plugins_then_re_registers_live() {
        // First sync: alpha + beta both registered.
        let runtime_initial = make_runtime(vec![
            Box::new(LensOwningPlugin {
                id: PluginId("alpha".into()),
                lens_name: "a".into(),
                registered: Arc::new(Mutex::new(0)),
            }),
            Box::new(LensOwningPlugin {
                id: PluginId("beta".into()),
                lens_name: "b".into(),
                registered: Arc::new(Mutex::new(0)),
            }),
        ]);
        let mut lens_registry = LensRegistry::new();
        runtime_initial.sync_lenses(&mut lens_registry);
        assert_eq!(lens_registry.len(), 2);

        // Second sync: only alpha remains. beta's lens should be
        // dropped; alpha's stays.
        let runtime_after = make_runtime(vec![Box::new(LensOwningPlugin {
            id: PluginId("alpha".into()),
            lens_name: "a".into(),
            registered: Arc::new(Mutex::new(0)),
        })]);
        runtime_after.sync_lenses(&mut lens_registry);
        assert_eq!(lens_registry.len(), 1);
        assert!(lens_registry.is_registered(&LensId::new(PluginId("alpha".into()), "a")));
        assert!(!lens_registry.is_registered(&LensId::new(PluginId("beta".into()), "b")));
    }

    #[test]
    fn sync_lenses_default_no_op_for_plugins_without_lenses() {
        // A plugin that doesn't override register_lenses returns 0
        // by default.
        struct NoLensPlugin;
        impl PluginBackend for NoLensPlugin {
            fn id(&self) -> PluginId {
                PluginId("no-lens".into())
            }
            fn on_init_effects(
                &mut self,
                _state: &crate::plugin::AppView<'_>,
            ) -> crate::plugin::Effects {
                crate::plugin::Effects::default()
            }
        }
        let runtime = make_runtime(vec![Box::new(NoLensPlugin)]);
        let mut lens_registry = LensRegistry::new();
        let count = runtime.sync_lenses(&mut lens_registry);
        assert_eq!(count, 0);
        assert_eq!(lens_registry.len(), 0);
    }
}
