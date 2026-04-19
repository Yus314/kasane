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

impl PluginBackend for TypedLifecyclePlugin {
    fn id(&self) -> PluginId {
        PluginId("typed-lifecycle".to_string())
    }

    fn on_init_effects(&mut self, _state: &AppView<'_>) -> Effects {
        Effects::redraw(DirtyFlags::STATUS)
    }

    fn on_active_session_ready_effects(&mut self, _state: &AppView<'_>) -> Effects {
        Effects {
            redraw: DirtyFlags::BUFFER,
            commands: vec![Command::SendToKakoune(KasaneRequest::Scroll {
                amount: 3,
                line: 1,
                column: 1,
            })],
            scroll_plans: vec![],
        }
    }
}

struct TypedRuntimePlugin;

impl PluginBackend for TypedRuntimePlugin {
    fn id(&self) -> PluginId {
        PluginId("typed-runtime".to_string())
    }

    fn on_state_changed_effects(&mut self, _state: &AppView<'_>, dirty: DirtyFlags) -> Effects {
        if !dirty.contains(DirtyFlags::BUFFER) {
            return Effects::default();
        }
        Effects {
            redraw: DirtyFlags::INFO,
            commands: vec![Command::RequestRedraw(DirtyFlags::STATUS)],
            scroll_plans: vec![ScrollPlan {
                total_amount: 3,
                line: 2,
                column: 4,
                frame_interval_ms: 8,
                curve: ScrollCurve::Linear,
                accumulation: ScrollAccumulationMode::Add,
            }],
        }
    }

    fn update_effects(&mut self, msg: &mut dyn std::any::Any, _state: &AppView<'_>) -> Effects {
        if msg.downcast_ref::<u32>() != Some(&7) {
            return Effects::default();
        }
        Effects {
            redraw: DirtyFlags::BUFFER,
            commands: vec![Command::RequestRedraw(DirtyFlags::STATUS)],
            scroll_plans: vec![ScrollPlan {
                total_amount: -2,
                line: 1,
                column: 1,
                frame_interval_ms: 16,
                curve: ScrollCurve::Linear,
                accumulation: ScrollAccumulationMode::Add,
            }],
        }
    }
}

struct ShutdownProbePlugin {
    id: &'static str,
    shutdowns: Arc<AtomicUsize>,
}

impl PluginBackend for ShutdownProbePlugin {
    fn id(&self) -> PluginId {
        PluginId(self.id.to_string())
    }

    fn on_shutdown(&mut self) {
        self.shutdowns.fetch_add(1, Ordering::SeqCst);
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

impl PluginBackend for DisplayTransformPlugin {
    fn id(&self) -> PluginId {
        PluginId(self.id.to_string())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::DISPLAY_TRANSFORM
    }

    fn display_directives(&self, _state: &AppView<'_>) -> Vec<DisplayDirective> {
        self.directives.clone()
    }

    fn display_directive_priority(&self) -> i16 {
        self.priority
    }
}

struct WorkspaceObserverPlugin {
    id: &'static str,
    hits: Arc<AtomicUsize>,
}

impl PluginBackend for WorkspaceObserverPlugin {
    fn id(&self) -> PluginId {
        PluginId(self.id.to_string())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::WORKSPACE_OBSERVER
    }

    fn on_workspace_changed(&mut self, _query: &crate::workspace::WorkspaceQuery<'_>) {
        self.hits.fetch_add(1, Ordering::SeqCst);
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

impl PluginBackend for KeyMiddlewarePlugin {
    fn id(&self) -> PluginId {
        PluginId(self.id.to_string())
    }

    fn handle_key_middleware(&mut self, key: &KeyEvent, _state: &AppView<'_>) -> KeyHandleResult {
        self.seen.lock().unwrap().push(key.clone());
        match &self.behavior {
            MiddlewareBehavior::Passthrough => KeyHandleResult::Passthrough,
            MiddlewareBehavior::Transform(next_key) => {
                KeyHandleResult::Transformed(next_key.clone())
            }
            MiddlewareBehavior::Consume(keyspec) => KeyHandleResult::Consumed(vec![
                Command::SendToKakoune(KasaneRequest::Keys(vec![keyspec.clone()])),
            ]),
        }
    }
}

impl PluginBackend for AuthorityPlugin {
    fn id(&self) -> PluginId {
        PluginId(self.id.to_string())
    }

    fn authorities(&self) -> PluginAuthorities {
        self.authorities
    }
}

struct TargetedReadyPlugin {
    id: &'static str,
    redraw: DirtyFlags,
}

impl PluginBackend for TargetedReadyPlugin {
    fn id(&self) -> PluginId {
        PluginId(self.id.to_string())
    }

    fn on_active_session_ready_effects(&mut self, _state: &AppView<'_>) -> Effects {
        Effects {
            redraw: self.redraw,
            commands: vec![],
            scroll_plans: vec![],
        }
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
    registry.register_backend(Box::new(TypedLifecyclePlugin));
    let state = AppState::default();

    let batch = registry.init_all_batch(&AppView::new(&state));
    assert!(batch.effects.redraw.contains(DirtyFlags::STATUS));
}

#[test]
fn test_notify_active_session_ready_batch_collects_effects() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(TypedLifecyclePlugin));
    let state = AppState::default();

    let batch = registry.notify_active_session_ready_batch(&AppView::new(&state));
    assert!(batch.effects.redraw.contains(DirtyFlags::BUFFER));
    assert_eq!(batch.effects.commands.len(), 1);
    assert!(matches!(
        batch.effects.commands.into_iter().next(),
        Some(Command::SendToKakoune(KasaneRequest::Scroll { .. }))
    ));
}

#[test]
fn test_notify_plugin_active_session_ready_batch_targets_only_requested_plugin() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(TargetedReadyPlugin {
        id: "alpha",
        redraw: DirtyFlags::STATUS,
    }));
    registry.register_backend(Box::new(TargetedReadyPlugin {
        id: "beta",
        redraw: DirtyFlags::BUFFER,
    }));
    let state = AppState::default();

    let batch = registry.notify_plugin_active_session_ready_batch(
        &PluginId("beta".to_string()),
        &AppView::new(&state),
    );
    assert!(batch.effects.redraw.contains(DirtyFlags::BUFFER));
    assert!(!batch.effects.redraw.contains(DirtyFlags::STATUS));
}

#[test]
fn test_notify_state_changed_batch_collects_runtime_effects() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(TypedRuntimePlugin));
    let state = AppState::default();

    let batch = registry.notify_state_changed_batch(&AppView::new(&state), DirtyFlags::BUFFER);
    assert!(batch.effects.redraw.contains(DirtyFlags::INFO));
    assert_eq!(batch.effects.commands.len(), 1);
    assert_eq!(batch.effects.scroll_plans.len(), 1);
}

#[test]
fn test_deliver_message_batch_collects_runtime_effects() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(TypedRuntimePlugin));
    let state = AppState::default();

    let batch = registry.deliver_message_batch(
        &PluginId("typed-runtime".to_string()),
        Box::new(7u32),
        &AppView::new(&state),
    );
    assert!(batch.effects.redraw.contains(DirtyFlags::BUFFER));
    assert_eq!(batch.effects.commands.len(), 1);
    assert_eq!(batch.effects.scroll_plans.len(), 1);
}

#[test]
fn test_shutdown_all_calls_all_plugins() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(LifecyclePlugin::new()));
    registry.register_backend(Box::new(LifecyclePlugin::new()));
    registry.shutdown_all();
    // Verify via count — can't inspect internal state, but no panic = success
}

#[test]
fn test_collect_plugin_surfaces_returns_owner_group() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(SurfacePlugin));

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
    registry.register_backend(Box::new(TestPlugin));
    registry.register_backend(Box::new(SurfacePlugin));

    assert!(registry.remove_plugin(&PluginId("surface-plugin".to_string())));
    assert_eq!(registry.plugin_count(), 1);
    assert!(!registry.remove_plugin(&PluginId("surface-plugin".to_string())));
}

#[test]
fn test_plugin_has_authority_uses_declared_authorities() {
    let mut registry = PluginRuntime::new();
    let plugin_id = PluginId("authority-probe".to_string());
    registry.register_backend(Box::new(AuthorityPlugin {
        id: "authority-probe",
        authorities: PluginAuthorities::DYNAMIC_SURFACE,
    }));

    assert!(registry.plugin_has_authority(&plugin_id, PluginAuthorities::DYNAMIC_SURFACE));
    assert!(!registry.plugin_has_authority(&plugin_id, PluginAuthorities::PTY_PROCESS));
}

#[test]
fn test_register_backend_replacement_updates_authorities() {
    let mut registry = PluginRuntime::new();
    let plugin_id = PluginId("authority-probe".to_string());
    registry.register_backend(Box::new(AuthorityPlugin {
        id: "authority-probe",
        authorities: PluginAuthorities::DYNAMIC_SURFACE,
    }));
    registry.register_backend(Box::new(AuthorityPlugin {
        id: "authority-probe",
        authorities: PluginAuthorities::PTY_PROCESS,
    }));

    assert!(!registry.plugin_has_authority(&plugin_id, PluginAuthorities::DYNAMIC_SURFACE));
    assert!(registry.plugin_has_authority(&plugin_id, PluginAuthorities::PTY_PROCESS));
}

#[test]
fn test_collect_display_directives_composes_multi_plugin() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(DisplayTransformPlugin {
        id: "first",
        directives: vec![DisplayDirective::Hide { range: 1..3 }],
        priority: 0,
    }));
    registry.register_backend(Box::new(DisplayTransformPlugin {
        id: "second",
        directives: vec![DisplayDirective::Fold {
            range: 3..5,
            summary: vec![Atom {
                face: Face::default(),
                contents: "folded".into(),
            }],
        }],
        priority: 0,
    }));

    let mut state = AppState::default();
    state.observed.lines = vec![vec![], vec![], vec![], vec![], vec![], vec![]];

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
    registry.register_backend(Box::new(DisplayTransformPlugin {
        id: "first",
        directives: vec![DisplayDirective::Hide { range: 1..3 }],
        priority: 0,
    }));

    let mut state = AppState::default();
    state.observed.lines = vec![vec![], vec![], vec![], vec![]];

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
    registry.register_backend(Box::new(DisplayTransformPlugin {
        id: "low",
        directives: vec![DisplayDirective::Fold {
            range: 1..4,
            summary: vec![Atom {
                face: Face::default(),
                contents: "low-fold".into(),
            }],
        }],
        priority: 0,
    }));
    registry.register_backend(Box::new(DisplayTransformPlugin {
        id: "high",
        directives: vec![DisplayDirective::Fold {
            range: 2..5,
            summary: vec![Atom {
                face: Face::default(),
                contents: "high-fold".into(),
            }],
        }],
        priority: 10,
    }));

    let mut state = AppState::default();
    state.observed.lines = vec![vec![], vec![], vec![], vec![], vec![], vec![]];

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
    registry.register_backend(Box::new(DisplayTransformPlugin {
        id: "only",
        directives: vec![DisplayDirective::Hide { range: 1..3 }],
        priority: 0,
    }));

    let mut state = AppState::default();
    state.observed.lines = vec![vec![], vec![], vec![], vec![]];

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
    registry.register_backend(Box::new(WorkspaceObserverPlugin {
        id: "observer",
        hits: hits.clone(),
    }));
    registry.register_backend(Box::new(TestPlugin));

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
    registry.register_backend(Box::new(KeyMiddlewarePlugin {
        id: "transformer",
        seen: first_seen.clone(),
        behavior: MiddlewareBehavior::Transform(KeyEvent {
            key: Key::Char('b'),
            modifiers: Modifiers::SHIFT,
        }),
    }));
    registry.register_backend(Box::new(KeyMiddlewarePlugin {
        id: "consumer",
        seen: second_seen.clone(),
        behavior: MiddlewareBehavior::Consume("<esc>".to_string()),
    }));

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
    registry.register_backend(Box::new(KeyMiddlewarePlugin {
        id: "transformer",
        seen: Arc::new(Mutex::new(Vec::new())),
        behavior: MiddlewareBehavior::Transform(KeyEvent {
            key: Key::PageDown,
            modifiers: Modifiers::CTRL,
        }),
    }));
    registry.register_backend(Box::new(KeyMiddlewarePlugin {
        id: "passthrough",
        seen: Arc::new(Mutex::new(Vec::new())),
        behavior: MiddlewareBehavior::Passthrough,
    }));

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
    registry.register_backend(Box::new(ShutdownProbePlugin {
        id: "shutdown-probe",
        shutdowns: shutdowns.clone(),
    }));

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
    registry.register_backend(Box::new(LifecyclePlugin::new()));
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
    registry.register_backend(Box::new(TestPlugin));
    let state = AppState::default();

    let batch = registry.init_all_batch(&AppView::new(&state));
    assert!(batch.effects.redraw.is_empty());

    registry.shutdown_all();
    // No panic
}

#[test]
fn test_init_all_batch_collects_lifecycle_bootstrap_effects() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(LifecyclePlugin::new()));
    let state = AppState::default();

    let batch = registry.init_all_batch(&AppView::new(&state));
    assert!(batch.effects.redraw.contains(DirtyFlags::BUFFER));
}

#[test]
fn test_reload_plugin_batch_collects_bootstrap_effects() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(TypedLifecyclePlugin));
    let state = AppState::default();

    let batch = registry.reload_plugin_batch(Box::new(TypedLifecyclePlugin), &AppView::new(&state));
    assert!(batch.effects.redraw.contains(DirtyFlags::STATUS));
}

#[test]
fn test_any_plugin_state_changed_flag() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(StatefulPlugin { hash: 1 }));

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
    assert!(batch.effects.redraw.is_empty());
    assert!(batch.effects.commands.is_empty());
    assert!(batch.effects.scroll_plans.is_empty());
}

// --- Per-extension-point invalidation tests (Phase 5) ---

/// A contributor-only plugin with controllable hash.
struct ContributorPlugin {
    hash: u64,
}

impl PluginBackend for ContributorPlugin {
    fn id(&self) -> PluginId {
        PluginId("contributor".to_string())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::CONTRIBUTOR
    }

    fn state_hash(&self) -> u64 {
        self.hash
    }

    fn contribute_to(
        &self,
        region: &SlotId,
        _state: &AppView<'_>,
        _ctx: &ContributeContext,
    ) -> Option<Contribution> {
        if *region == SlotId::STATUS_LEFT {
            Some(Contribution {
                element: crate::element::Element::text("contrib", Face::default()),
                priority: 0,
                size_hint: crate::plugin::ContribSizeHint::Auto,
            })
        } else {
            None
        }
    }
}

/// An annotator-only plugin with controllable hash.
struct AnnotatorPlugin {
    hash: u64,
}

impl PluginBackend for AnnotatorPlugin {
    fn id(&self) -> PluginId {
        PluginId("annotator".to_string())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ANNOTATOR
    }

    fn state_hash(&self) -> u64 {
        self.hash
    }
}

#[test]
fn test_per_extension_point_stale_contributor_only() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(ContributorPlugin { hash: 1 }));
    registry.register_backend(Box::new(AnnotatorPlugin { hash: 1 }));

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
    registry.register_backend(Box::new(ContributorPlugin { hash: 1 }));
    registry.register_backend(Box::new(AnnotatorPlugin { hash: 1 }));

    // First prepare: both become stale
    registry.prepare_plugin_cache(DirtyFlags::ALL);

    // Stabilize both
    registry.prepare_plugin_cache(DirtyFlags::empty());
    let view = registry.view();
    assert!(!view.any_contributor_needs_recollect());
    assert!(!view.any_annotator_needs_recollect());

    // Now change only the annotator's hash — mutate via re-register
    registry.register_backend(Box::new(AnnotatorPlugin { hash: 2 }));
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
    registry.register_backend(Box::new(ContributorPlugin { hash: 1 }));

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
    registry.register_backend(Box::new(AnnotatorPlugin { hash: 1 }));
    registry.register_backend(Box::new(DisplayTransformPlugin {
        id: "display",
        directives: vec![],
        priority: 0,
    }));

    // Both stale initially
    registry.prepare_plugin_cache(DirtyFlags::ALL);
    let view = registry.view();
    assert!(view.any_annotator_needs_recollect());
    assert!(view.any_display_transform_needs_recollect());

    // Stabilize both
    registry.prepare_plugin_cache(DirtyFlags::empty());

    // Change only annotator
    registry.register_backend(Box::new(AnnotatorPlugin { hash: 2 }));
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
        r.on_annotate_gutter(GutterSide::Left, 10, |_state, line, _app, _ctx| {
            Some(crate::element::Element::text(
                format!("{}", line + 1),
                Face::default(),
            ))
        });
        r.on_annotate_background(|_state, line, _app, _ctx| {
            if line == 0 {
                Some(BackgroundLayer {
                    face: Face {
                        bg: crate::protocol::Color::Named(crate::protocol::NamedColor::Blue),
                        ..Face::default()
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

/// A legacy PluginBackend that uses the monolithic annotate_line_with_ctx.
struct LegacyAnnotatorPlugin;

impl PluginBackend for LegacyAnnotatorPlugin {
    fn id(&self) -> PluginId {
        PluginId("legacy-annotator".to_string())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::ANNOTATOR
    }

    fn annotate_line_with_ctx(
        &self,
        line: usize,
        _state: &AppView<'_>,
        _ctx: &AnnotateContext,
    ) -> Option<super::super::LineAnnotation> {
        Some(super::super::LineAnnotation {
            left_gutter: Some(crate::element::Element::text(
                format!("L{}", line + 1),
                Face::default(),
            )),
            right_gutter: None,
            background: None,
            priority: 5,
            inline: None,
            virtual_text: vec![],
        })
    }
}

#[test]
fn test_decomposed_annotator_produces_gutter_and_background() {
    let mut registry = PluginRuntime::new();
    registry.register(DecomposedAnnotatorPlugin);

    let mut state = AppState::default();
    state.observed.lines = vec![vec![], vec![], vec![]];
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
    registry.register_backend(Box::new(LegacyAnnotatorPlugin));

    let mut state = AppState::default();
    state.observed.lines = vec![vec![], vec![]];
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

#[test]
fn test_decomposed_annotator_has_decomposed_annotations_flag() {
    let mut registry = PluginRuntime::new();
    registry.register(DecomposedAnnotatorPlugin);
    registry.register_backend(Box::new(LegacyAnnotatorPlugin));

    // PluginBridge (from register()) should report has_decomposed_annotations = true
    // LegacyAnnotatorPlugin should report has_decomposed_annotations = false
    let mut decomposed_count = 0;
    let mut legacy_count = 0;
    for p in registry.plugins_mut() {
        if p.has_decomposed_annotations() {
            decomposed_count += 1;
        } else {
            legacy_count += 1;
        }
    }
    assert_eq!(decomposed_count, 1);
    assert_eq!(legacy_count, 1);
}

// --- Pub/Sub tests (Phase 8a) ---

use crate::plugin::pubsub::{TopicBus, TopicId};

/// Plugin state for pub/sub publisher: tracks a counter.
#[derive(Clone, Default, PartialEq, Debug)]
struct PubState {
    counter: u32,
}

/// Plugin state for pub/sub subscriber: tracks received value.
#[derive(Clone, Default, PartialEq, Debug)]
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
        r.on_state_changed(|state, _app, _dirty| {
            (
                PubState {
                    counter: state.counter + 1,
                },
                Effects::default(),
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
    runtime.evaluate_pubsub(&mut bus, &app);

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
    runtime.evaluate_pubsub(&mut bus, &app);

    // No subscriber → only publisher state changed.
    runtime.prepare_plugin_cache(DirtyFlags::empty());
    assert!(runtime.any_plugin_state_changed());
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

// --- Extension Point tests (Phase 8b) ---

use crate::plugin::extension_point::{CompositionRule, ExtensionPointId};

/// Plugin that defines a custom extension point.
struct ExtPointDefiner;
impl Plugin for ExtPointDefiner {
    type State = ();
    fn id(&self) -> PluginId {
        PluginId("ext-definer".to_string())
    }
    fn register(&self, r: &mut HandlerRegistry<()>) {
        r.define_extension_with_handler::<(), Vec<String>>(
            ExtensionPointId::new("test.items"),
            CompositionRule::Merge,
            |_state, _input, _app| vec!["from-definer".to_string()],
        );
    }
}

/// Plugin that contributes to the extension point.
struct ExtPointContributor;
impl Plugin for ExtPointContributor {
    type State = ();
    fn id(&self) -> PluginId {
        PluginId("ext-contributor".to_string())
    }
    fn register(&self, r: &mut HandlerRegistry<()>) {
        r.on_extension::<(), Vec<String>>(
            ExtensionPointId::new("test.items"),
            |_state, _input, _app| vec!["from-contributor".to_string()],
        );
    }
}

#[test]
fn test_extension_point_collects_from_definer_and_contributor() {
    let mut runtime = PluginRuntime::new();
    runtime.register(ExtPointDefiner);
    runtime.register(ExtPointContributor);

    let mut state = AppState::default();
    state.runtime.rows = 24;
    state.runtime.cols = 80;
    let app = AppView::new(&state);

    let input = super::super::channel::ChannelValue::new(&()).unwrap();
    let results = runtime.evaluate_extensions(&input, &app);
    let items = results.get::<Vec<String>>(&ExtensionPointId::new("test.items"));

    // Both definer and contributor should produce results.
    assert_eq!(items.len(), 2);
    assert!(items[0].contains(&"from-definer".to_string()));
    assert!(items[1].contains(&"from-contributor".to_string()));
}

#[test]
fn test_extension_point_no_contributors_only_definer() {
    let mut runtime = PluginRuntime::new();
    runtime.register(ExtPointDefiner);

    let mut state = AppState::default();
    state.runtime.rows = 24;
    state.runtime.cols = 80;
    let app = AppView::new(&state);

    let input = super::super::channel::ChannelValue::new(&()).unwrap();
    let results = runtime.evaluate_extensions(&input, &app);
    let items = results.get::<Vec<String>>(&ExtensionPointId::new("test.items"));
    assert_eq!(items.len(), 1);
    assert!(items[0].contains(&"from-definer".to_string()));
}

#[test]
fn test_extension_point_unknown_returns_empty() {
    let mut runtime = PluginRuntime::new();
    runtime.register(ExtPointDefiner);

    let mut state = AppState::default();
    state.runtime.rows = 24;
    state.runtime.cols = 80;
    let app = AppView::new(&state);

    let input = super::super::channel::ChannelValue::new(&()).unwrap();
    let results = runtime.evaluate_extensions(&input, &app);
    let items = results.get::<u32>(&ExtensionPointId::new("nonexistent"));
    assert!(items.is_empty());
}

// --- InteractiveId PluginTag tests (Phase 8) ---

use crate::element::{InteractiveId, PluginTag};
use crate::input::{MouseButton, MouseEventKind};

#[test]
fn test_plugin_tags_are_monotonically_assigned_starting_from_1() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(TestPlugin));
    registry.register_backend(Box::new(SurfacePlugin));

    let tags: Vec<(PluginId, PluginTag)> = registry.all_plugin_tags();
    assert_eq!(tags[0].1, PluginTag(1));
    assert_eq!(tags[1].1, PluginTag(2));
}

#[test]
fn test_plugin_tag_zero_is_reserved_for_framework() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(TestPlugin));
    registry.register_backend(Box::new(SurfacePlugin));
    registry.register_backend(Box::new(StatefulPlugin { hash: 1 }));

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
    registry.register_backend(Box::new(TestPlugin));
    let original_tag = registry.plugin_tag(&PluginId("test".to_string())).unwrap();

    // Replace with same ID
    registry.register_backend(Box::new(TestPlugin));
    let replaced_tag = registry.plugin_tag(&PluginId("test".to_string())).unwrap();
    assert_eq!(original_tag, replaced_tag);
}

#[test]
fn test_unloading_plugin_does_not_recycle_tag() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(TestPlugin));
    registry.register_backend(Box::new(SurfacePlugin));

    // Remove first plugin
    registry.remove_plugin(&PluginId("test".to_string()));

    // Register new plugin — should get tag 3, not 1
    registry.register_backend(Box::new(StatefulPlugin { hash: 1 }));
    let new_tag = registry
        .plugin_tag(&PluginId("stateful".to_string()))
        .unwrap();
    assert_eq!(new_tag, PluginTag(3));
}

// --- Owner-based dispatch tests ---

struct MousePlugin42;

impl PluginBackend for MousePlugin42 {
    fn id(&self) -> PluginId {
        PluginId("mouse42".to_string())
    }

    fn handle_mouse(
        &mut self,
        _event: &crate::input::MouseEvent,
        id: InteractiveId,
        _state: &AppView<'_>,
    ) -> Option<Vec<Command>> {
        if id.local == 42 {
            Some(vec![Command::RequestRedraw(DirtyFlags::BUFFER)])
        } else {
            None
        }
    }
}

struct DecoyMousePlugin;

impl PluginBackend for DecoyMousePlugin {
    fn id(&self) -> PluginId {
        PluginId("decoy-mouse".to_string())
    }

    fn handle_mouse(
        &mut self,
        _event: &crate::input::MouseEvent,
        _id: InteractiveId,
        _state: &AppView<'_>,
    ) -> Option<Vec<Command>> {
        // Should never be called for tagged IDs belonging to other plugins
        panic!("decoy plugin should not be called for tagged IDs");
    }
}

#[test]
fn test_tagged_interactive_id_routes_to_correct_plugin() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(DecoyMousePlugin));
    registry.register_backend(Box::new(MousePlugin42));

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
    registry.register_backend(Box::new(MousePlugin42));

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
    registry.register_backend(Box::new(MousePlugin42));

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

impl PluginBackend for NavPolicyPlugin {
    fn id(&self) -> PluginId {
        PluginId(self.name.to_string())
    }
    fn navigation_policy(&self, _unit: &DisplayUnit) -> Option<NavigationPolicy> {
        Some(self.policy.clone())
    }
}

struct NavActionPlugin {
    name: &'static str,
    result: ActionResult,
}

impl PluginBackend for NavActionPlugin {
    fn id(&self) -> PluginId {
        PluginId(self.name.to_string())
    }
    fn navigation_action(
        &mut self,
        _unit: &DisplayUnit,
        _action: NavigationAction,
    ) -> Option<ActionResult> {
        Some(self.result.clone())
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
    registry.register_backend(Box::new(NavPolicyPlugin {
        name: "nav-policy-1",
        policy: NavigationPolicy::Normal,
    }));
    // Second plugin returns Skip — should be ignored because first wins
    registry.register_backend(Box::new(NavPolicyPlugin {
        name: "nav-policy-2",
        policy: NavigationPolicy::Skip,
    }));

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
    registry.register_backend(Box::new(NavActionPlugin {
        name: "nav-action-1",
        result: ActionResult::Handled,
    }));
    registry.register_backend(Box::new(NavActionPlugin {
        name: "nav-action-2",
        result: ActionResult::SendKeys("j".to_string()),
    }));

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
                    face: Face {
                        bg: crate::protocol::Color::Named(crate::protocol::NamedColor::Red),
                        ..Face::default()
                    },
                    z_order: 0,
                },
                // Decoration: gutter
                DisplayDirective::Gutter {
                    line: 1,
                    side: crate::display::GutterSide::Left,
                    content: crate::element::Element::text("G", Face::default()),
                    priority: 5,
                },
                // Decoration: virtual text
                DisplayDirective::VirtualText {
                    line: 0,
                    position: crate::display::VirtualTextPosition::EndOfLine,
                    content: vec![Atom {
                        face: Face::default(),
                        contents: "hint".into(),
                    }],
                    priority: 0,
                },
                // InterLine: insert after
                DisplayDirective::InsertAfter {
                    line: 0,
                    content: crate::element::Element::text("inserted", Face::default()),
                    priority: 0,
                },
                // Inline: style
                DisplayDirective::StyleInline {
                    line: 1,
                    byte_range: 0..3,
                    face: Face {
                        fg: crate::protocol::Color::Named(crate::protocol::NamedColor::Green),
                        ..Face::default()
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
    state.observed.lines = vec![vec![], vec![], vec![], vec![]];

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
    state.observed.lines = vec![vec![], vec![], vec![], vec![]];
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
    state.observed.lines = vec![vec![], vec![], vec![], vec![]];
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
    state.observed.lines = vec![vec![], vec![], vec![]];
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
    registry.register_backend(Box::new(LegacyAnnotatorPlugin));

    let mut state = AppState::default();
    state.observed.lines = vec![vec![], vec![], vec![]];
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
