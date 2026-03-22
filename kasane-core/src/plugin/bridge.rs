//! PluginBridge adapter — adapts `Plugin` to the internal `PluginBackend` trait.
//!
//! Use `PluginBridge` to adapt a `Plugin` into the internal `PluginBackend` trait,
//! or register directly via `PluginRuntime::register()`.

use std::any::Any;

use crate::element::{Element, InteractiveId};
use crate::input::{KeyEvent, MouseEvent};
use crate::scroll::{DefaultScrollCandidate, ScrollPolicyResult};
use crate::state::DirtyFlags;
use crate::workspace::WorkspaceQuery;

use super::state::{Plugin, PluginState};
use super::{
    AnnotateContext, AppView, BootstrapEffects, Command, ContributeContext, Contribution,
    DisplayDirective, IoEvent, KeyHandleResult, LineAnnotation, OverlayContext,
    OverlayContribution, PluginAuthorities, PluginBackend, PluginCapabilities, PluginId,
    RuntimeEffects, SessionReadyEffects, SlotId, TransformContext, TransformTarget,
};

// =============================================================================
// Phase 1: PluginBridge Adapter
// =============================================================================

/// Object-safe version of `Plugin` used by the framework.
/// State is passed as `&dyn PluginState` / `&mut dyn PluginState`.
///
/// Note: we use `&mut dyn PluginState` (not `&mut Box<dyn PluginState>`) to avoid
/// method resolution ambiguity caused by the blanket `PluginState` impl.
pub(crate) trait ErasedPlugin: Send {
    fn id(&self) -> PluginId;
    fn capabilities(&self) -> PluginCapabilities;
    fn authorities(&self) -> PluginAuthorities;
    fn allows_process_spawn(&self) -> bool;
    fn transform_priority(&self) -> i16;
    fn display_directive_priority(&self) -> i16;
    fn view_deps(&self) -> DirtyFlags;

    // State transitions
    fn on_init_effects_erased(
        &self,
        state: &mut dyn PluginState,
        app: &AppView<'_>,
    ) -> BootstrapEffects;
    fn on_active_session_ready_effects_erased(
        &self,
        state: &mut dyn PluginState,
        app: &AppView<'_>,
    ) -> SessionReadyEffects;
    fn on_state_changed_effects_erased(
        &self,
        state: &mut dyn PluginState,
        app: &AppView<'_>,
        dirty: DirtyFlags,
    ) -> RuntimeEffects;
    fn on_io_event_effects_erased(
        &self,
        state: &mut dyn PluginState,
        event: &IoEvent,
        app: &AppView<'_>,
    ) -> RuntimeEffects;
    fn on_workspace_changed_erased(&self, state: &mut dyn PluginState, query: &WorkspaceQuery<'_>);
    fn observe_key_erased(&self, state: &mut dyn PluginState, key: &KeyEvent, app: &AppView<'_>);
    fn observe_mouse_erased(
        &self,
        state: &mut dyn PluginState,
        event: &MouseEvent,
        app: &AppView<'_>,
    );
    fn handle_key_erased(
        &self,
        state: &mut dyn PluginState,
        key: &KeyEvent,
        app: &AppView<'_>,
    ) -> Option<Vec<Command>>;
    fn handle_key_middleware_erased(
        &self,
        state: &mut dyn PluginState,
        key: &KeyEvent,
        app: &AppView<'_>,
    ) -> KeyHandleResult;
    fn handle_mouse_erased(
        &self,
        state: &mut dyn PluginState,
        event: &MouseEvent,
        id: InteractiveId,
        app: &AppView<'_>,
    ) -> Option<Vec<Command>>;
    fn handle_default_scroll_erased(
        &self,
        state: &mut dyn PluginState,
        candidate: DefaultScrollCandidate,
        app: &AppView<'_>,
    ) -> Option<ScrollPolicyResult>;
    fn update_effects_erased(
        &self,
        state: &mut dyn PluginState,
        msg: &mut dyn Any,
        app: &AppView<'_>,
    ) -> RuntimeEffects;

    // Pure view methods
    fn contribute_to_erased(
        &self,
        state: &dyn PluginState,
        region: &SlotId,
        app: &AppView<'_>,
        ctx: &ContributeContext,
    ) -> Option<Contribution>;
    fn transform_erased(
        &self,
        state: &dyn PluginState,
        target: &TransformTarget,
        element: Element,
        app: &AppView<'_>,
        ctx: &TransformContext,
    ) -> Element;
    fn annotate_line_erased(
        &self,
        state: &dyn PluginState,
        line: usize,
        app: &AppView<'_>,
        ctx: &AnnotateContext,
    ) -> Option<LineAnnotation>;
    fn contribute_overlay_erased(
        &self,
        state: &dyn PluginState,
        app: &AppView<'_>,
        ctx: &OverlayContext,
    ) -> Option<OverlayContribution>;
    fn cursor_style_override_erased(
        &self,
        state: &dyn PluginState,
        app: &AppView<'_>,
    ) -> Option<crate::render::CursorStyle>;
    fn transform_menu_item_erased(
        &self,
        state: &dyn PluginState,
        item: &[crate::protocol::Atom],
        index: usize,
        selected: bool,
        app: &AppView<'_>,
    ) -> Option<Vec<crate::protocol::Atom>>;

    // Display transform
    fn display_directives_erased(
        &self,
        state: &dyn PluginState,
        app: &AppView<'_>,
    ) -> Vec<DisplayDirective>;
}

// ---------------------------------------------------------------------------
// Type-erasure macros — reduce boilerplate in `impl<P: Plugin> ErasedPlugin for P`.
//
// Each macro captures a recurring pattern (A–D) from the erased→typed bridge:
//
//   A  erased_mut_effects  — downcast_mut, call returning (new_state, T), store, return T
//   B  erased_mut_void     — downcast_mut, call returning new_state, store
//   C  erased_mut_option   — downcast_mut, call returning Option<(new_state, T)>, conditional store
//   D  erased_ref          — downcast_ref, call, return value
//
// Pattern E (trivial 1-line delegation) is kept inline.
// ---------------------------------------------------------------------------

/// Pattern A: downcast_mut → call returning (new_state, effects) → store → return effects
macro_rules! erased_mut_effects {
    ($erased:ident => $typed:ident ($($p:ident : $pt:ty),*) -> $ret:ty) => {
        fn $erased(&self, state: &mut dyn PluginState, $($p: $pt),*) -> $ret {
            let typed = state.as_any_mut().downcast_mut::<P::State>().unwrap();
            let (new_state, effects) = self.$typed(typed, $($p),*);
            *typed = new_state;
            effects
        }
    };
}

/// Pattern B: downcast_mut → call returning new_state → store
macro_rules! erased_mut_void {
    ($erased:ident => $typed:ident ($($p:ident : $pt:ty),*)) => {
        fn $erased(&self, state: &mut dyn PluginState, $($p: $pt),*) {
            let typed = state.as_any_mut().downcast_mut::<P::State>().unwrap();
            let new_state = self.$typed(typed, $($p),*);
            *typed = new_state;
        }
    };
}

/// Pattern C: downcast_mut → call returning Option<(new_state, T)> → conditional store
macro_rules! erased_mut_option {
    ($erased:ident => $typed:ident ($($p:ident : $pt:ty),*) -> Option<$ret:ty>) => {
        fn $erased(&self, state: &mut dyn PluginState, $($p: $pt),*) -> Option<$ret> {
            let typed = state.as_any_mut().downcast_mut::<P::State>().unwrap();
            self.$typed(typed, $($p),*).map(|(new_state, val)| {
                *typed = new_state;
                val
            })
        }
    };
}

/// Pattern D: downcast_ref → call → return value
macro_rules! erased_ref {
    ($erased:ident => $typed:ident ($($p:ident : $pt:ty),*) -> $ret:ty) => {
        fn $erased(&self, state: &dyn PluginState, $($p: $pt),*) -> $ret {
            let typed = state.as_any().downcast_ref::<P::State>().unwrap();
            self.$typed(typed, $($p),*)
        }
    };
}

impl<P: Plugin> ErasedPlugin for P {
    // Pattern E — trivial 1-line delegations
    fn id(&self) -> PluginId {
        Plugin::id(self)
    }
    fn capabilities(&self) -> PluginCapabilities {
        Plugin::capabilities(self)
    }
    fn authorities(&self) -> PluginAuthorities {
        Plugin::authorities(self)
    }
    fn allows_process_spawn(&self) -> bool {
        Plugin::allows_process_spawn(self)
    }
    fn transform_priority(&self) -> i16 {
        Plugin::transform_priority(self)
    }
    fn display_directive_priority(&self) -> i16 {
        Plugin::display_directive_priority(self)
    }
    fn view_deps(&self) -> DirtyFlags {
        Plugin::view_deps(self)
    }

    // Pattern A — mut + effects return
    erased_mut_effects!(on_init_effects_erased => on_init_effects(app: &AppView<'_>) -> BootstrapEffects);
    erased_mut_effects!(on_active_session_ready_effects_erased => on_active_session_ready_effects(app: &AppView<'_>) -> SessionReadyEffects);
    erased_mut_effects!(on_state_changed_effects_erased => on_state_changed_effects(app: &AppView<'_>, dirty: DirtyFlags) -> RuntimeEffects);
    erased_mut_effects!(on_io_event_effects_erased => on_io_event_effects(event: &IoEvent, app: &AppView<'_>) -> RuntimeEffects);
    erased_mut_effects!(handle_key_middleware_erased => handle_key_middleware(key: &KeyEvent, app: &AppView<'_>) -> KeyHandleResult);
    erased_mut_effects!(update_effects_erased => update_effects(msg: &mut dyn Any, app: &AppView<'_>) -> RuntimeEffects);

    // Pattern B — mut + void
    erased_mut_void!(on_workspace_changed_erased => on_workspace_changed(query: &WorkspaceQuery<'_>));
    erased_mut_void!(observe_key_erased => observe_key(key: &KeyEvent, app: &AppView<'_>));
    erased_mut_void!(observe_mouse_erased => observe_mouse(event: &MouseEvent, app: &AppView<'_>));

    // Pattern C — mut + Option
    erased_mut_option!(handle_key_erased => handle_key(key: &KeyEvent, app: &AppView<'_>) -> Option<Vec<Command>>);
    erased_mut_option!(handle_mouse_erased => handle_mouse(event: &MouseEvent, id: InteractiveId, app: &AppView<'_>) -> Option<Vec<Command>>);
    erased_mut_option!(handle_default_scroll_erased => handle_default_scroll(candidate: DefaultScrollCandidate, app: &AppView<'_>) -> Option<ScrollPolicyResult>);

    // Pattern D — ref + return
    erased_ref!(contribute_to_erased => contribute_to(region: &SlotId, app: &AppView<'_>, ctx: &ContributeContext) -> Option<Contribution>);
    erased_ref!(transform_erased => transform(target: &TransformTarget, element: Element, app: &AppView<'_>, ctx: &TransformContext) -> Element);
    erased_ref!(annotate_line_erased => annotate_line_with_ctx(line: usize, app: &AppView<'_>, ctx: &AnnotateContext) -> Option<LineAnnotation>);
    erased_ref!(contribute_overlay_erased => contribute_overlay_with_ctx(app: &AppView<'_>, ctx: &OverlayContext) -> Option<OverlayContribution>);
    erased_ref!(cursor_style_override_erased => cursor_style_override(app: &AppView<'_>) -> Option<crate::render::CursorStyle>);
    erased_ref!(transform_menu_item_erased => transform_menu_item(item: &[crate::protocol::Atom], index: usize, selected: bool, app: &AppView<'_>) -> Option<Vec<crate::protocol::Atom>>);
    erased_ref!(display_directives_erased => display_directives(app: &AppView<'_>) -> Vec<DisplayDirective>);
}

/// Adapts a `Plugin` to the internal `PluginBackend` trait.
///
/// Holds the plugin logic + its externalized state. State changes are tracked
/// via a generation counter (incremented on every state mutation detected
/// by `PartialEq` comparison), which powers the existing L1 cache invalidation
/// in `PluginRuntime::prepare_plugin_cache()`.
pub struct PluginBridge {
    inner: Box<dyn ErasedPlugin>,
    state: Box<dyn PluginState>,
    /// Monotonic generation counter for `state_hash()`.
    generation: u64,
    /// Snapshot of state after last mutation, for change detection.
    prev_state: Box<dyn PluginState>,
}

impl PluginBridge {
    /// Create a new bridge from a `Plugin`, initialized with `Default::default()` state.
    pub fn new<P: Plugin>(plugin: P) -> Self {
        let state: Box<dyn PluginState> = Box::new(P::State::default());
        let prev_state = state.clone();
        PluginBridge {
            inner: Box::new(plugin),
            state,
            generation: 0,
            prev_state,
        }
    }

    /// Compare current state with previous snapshot; bump generation if changed.
    fn check_state_change(&mut self) {
        if *self.state != *self.prev_state {
            self.generation += 1;
            self.prev_state = self.state.clone();
        }
    }
}

// ---------------------------------------------------------------------------
// Bridge delegation macros — reduce boilerplate in `impl PluginBackend for PluginBridge`.
//
//   bridge_mut  — call erased method on mutable state, check_state_change, return
//   bridge_ref  — call erased method on shared state, return (no state check)
//
// Hand-written methods (property delegates without erased state):
//   id, capabilities, authorities, allows_process_spawn, state_hash,
//   transform_priority, display_directive_priority, view_deps, on_shutdown
// ---------------------------------------------------------------------------

/// Mutating delegation: call erased → check_state_change → return
macro_rules! bridge_mut {
    ($method:ident => $erased:ident ($($p:ident : $pt:ty),*) -> $ret:ty) => {
        fn $method(&mut self, $($p: $pt),*) -> $ret {
            let result = self.inner.$erased(&mut *self.state, $($p),*);
            self.check_state_change();
            result
        }
    };
    // void variant (no return value)
    ($method:ident => $erased:ident ($($p:ident : $pt:ty),*)) => {
        fn $method(&mut self, $($p: $pt),*) {
            self.inner.$erased(&mut *self.state, $($p),*);
            self.check_state_change();
        }
    };
}

/// Read-only delegation: call erased → return (no state check)
macro_rules! bridge_ref {
    ($method:ident => $erased:ident ($($p:ident : $pt:ty),*) -> $ret:ty) => {
        fn $method(&self, $($p: $pt),*) -> $ret {
            self.inner.$erased(&*self.state, $($p),*)
        }
    };
}

impl PluginBackend for PluginBridge {
    // --- Property delegates (hand-written, no erased state) ---

    fn id(&self) -> PluginId {
        self.inner.id()
    }

    fn capabilities(&self) -> PluginCapabilities {
        self.inner.capabilities()
    }

    fn authorities(&self) -> PluginAuthorities {
        self.inner.authorities()
    }

    fn allows_process_spawn(&self) -> bool {
        self.inner.allows_process_spawn()
    }

    fn state_hash(&self) -> u64 {
        self.generation
    }

    fn transform_priority(&self) -> i16 {
        self.inner.transform_priority()
    }

    fn display_directive_priority(&self) -> i16 {
        self.inner.display_directive_priority()
    }

    fn view_deps(&self) -> DirtyFlags {
        self.inner.view_deps()
    }

    fn on_shutdown(&mut self) {
        // Plugin has no shutdown hook (pure functions don't need cleanup).
    }

    // --- Lifecycle (bridge_mut) ---

    bridge_mut!(on_init_effects => on_init_effects_erased(state: &AppView<'_>) -> BootstrapEffects);
    bridge_mut!(on_active_session_ready_effects => on_active_session_ready_effects_erased(state: &AppView<'_>) -> SessionReadyEffects);
    bridge_mut!(on_state_changed_effects => on_state_changed_effects_erased(state: &AppView<'_>, dirty: DirtyFlags) -> RuntimeEffects);
    bridge_mut!(on_io_event_effects => on_io_event_effects_erased(event: &IoEvent, state: &AppView<'_>) -> RuntimeEffects);
    bridge_mut!(on_workspace_changed => on_workspace_changed_erased(query: &WorkspaceQuery<'_>));

    // --- Input (bridge_mut) ---

    bridge_mut!(observe_key => observe_key_erased(key: &KeyEvent, state: &AppView<'_>));
    bridge_mut!(observe_mouse => observe_mouse_erased(event: &MouseEvent, state: &AppView<'_>));
    bridge_mut!(handle_key => handle_key_erased(key: &KeyEvent, state: &AppView<'_>) -> Option<Vec<Command>>);
    bridge_mut!(handle_key_middleware => handle_key_middleware_erased(key: &KeyEvent, state: &AppView<'_>) -> KeyHandleResult);
    bridge_mut!(handle_mouse => handle_mouse_erased(event: &MouseEvent, id: InteractiveId, state: &AppView<'_>) -> Option<Vec<Command>>);
    bridge_mut!(handle_default_scroll => handle_default_scroll_erased(candidate: DefaultScrollCandidate, state: &AppView<'_>) -> Option<ScrollPolicyResult>);
    bridge_mut!(update_effects => update_effects_erased(msg: &mut dyn Any, state: &AppView<'_>) -> RuntimeEffects);

    // --- View contributions (bridge_ref) ---

    bridge_ref!(contribute_to => contribute_to_erased(region: &SlotId, state: &AppView<'_>, ctx: &ContributeContext) -> Option<Contribution>);
    bridge_ref!(transform => transform_erased(target: &TransformTarget, element: Element, state: &AppView<'_>, ctx: &TransformContext) -> Element);
    bridge_ref!(annotate_line_with_ctx => annotate_line_erased(line: usize, state: &AppView<'_>, ctx: &AnnotateContext) -> Option<LineAnnotation>);
    bridge_ref!(display_directives => display_directives_erased(state: &AppView<'_>) -> Vec<DisplayDirective>);
    bridge_ref!(contribute_overlay_with_ctx => contribute_overlay_erased(state: &AppView<'_>, ctx: &OverlayContext) -> Option<OverlayContribution>);
    bridge_ref!(cursor_style_override => cursor_style_override_erased(state: &AppView<'_>) -> Option<crate::render::CursorStyle>);
    bridge_ref!(transform_menu_item => transform_menu_item_erased(item: &[crate::protocol::Atom], index: usize, selected: bool, state: &AppView<'_>) -> Option<Vec<crate::protocol::Atom>>);
}

/// Marker trait for runtime detection of `Plugin`-backed plugins.
///
/// Enables the framework to access externalized state directly on `dyn PluginBackend`
/// objects that are backed by `PluginBridge`.
pub trait IsBridgedPlugin {
    fn plugin_state(&self) -> &dyn PluginState;
    fn plugin_state_mut(&mut self) -> &mut dyn PluginState;
}

impl IsBridgedPlugin for PluginBridge {
    fn plugin_state(&self) -> &dyn PluginState {
        &*self.state
    }
    fn plugin_state_mut(&mut self) -> &mut dyn PluginState {
        &mut *self.state
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::super::state::tests::{ColorPreviewPure, CursorLinePure, CursorLineState};
    use super::*;
    use crate::layout::Rect;
    use crate::plugin::{AnnotateContext, PluginCapabilities, PluginId, PluginRuntime};
    use crate::scroll::{ResolvedScroll, ScrollPolicyResult};
    use crate::state::AppState;

    // ---- PluginBridge tests ----

    #[test]
    fn bridge_delegates_id_and_capabilities() {
        let bridge = PluginBridge::new(CursorLinePure);
        assert_eq!(bridge.id(), PluginId("test.cursor-line-pure".into()));
        assert_eq!(bridge.capabilities(), PluginCapabilities::ANNOTATOR);
        assert_eq!(bridge.state_hash(), 0);
    }

    #[test]
    fn bridge_tracks_state_changes() {
        let mut bridge = PluginBridge::new(CursorLinePure);
        let mut app = AppState::default();
        app.cursor_pos.line = 5;

        assert_eq!(bridge.state_hash(), 0);

        // State changes: active_line 0 → 5
        bridge.on_state_changed_effects(&AppView::new(&app), DirtyFlags::BUFFER);
        assert_eq!(bridge.state_hash(), 1);

        // Same input → same state → no generation bump
        bridge.on_state_changed_effects(&AppView::new(&app), DirtyFlags::BUFFER);
        assert_eq!(bridge.state_hash(), 1);

        // Different input → different state → generation bumps
        app.cursor_pos.line = 10;
        bridge.on_state_changed_effects(&AppView::new(&app), DirtyFlags::BUFFER);
        assert_eq!(bridge.state_hash(), 2);
    }

    #[test]
    fn bridge_no_change_on_irrelevant_dirty() {
        let mut bridge = PluginBridge::new(CursorLinePure);
        let app = AppState::default();

        // STATUS dirty doesn't trigger CursorLinePure's on_state_changed logic
        bridge.on_state_changed_effects(&AppView::new(&app), DirtyFlags::STATUS);
        assert_eq!(bridge.state_hash(), 0);
    }

    #[test]
    fn bridge_annotates_cursor_line() {
        let mut bridge = PluginBridge::new(CursorLinePure);
        let mut app = AppState::default();
        app.cursor_pos.line = 3;

        bridge.on_state_changed_effects(&AppView::new(&app), DirtyFlags::BUFFER);

        let view = AppView::new(&app);
        let ctx = AnnotateContext {
            line_width: 80,
            gutter_width: 0,
            display_map: None,
            pane_surface_id: None,
            pane_focused: true,
        };

        assert!(bridge.annotate_line_with_ctx(3, &view, &ctx).is_some());
        assert!(bridge.annotate_line_with_ctx(0, &view, &ctx).is_none());
        assert!(bridge.annotate_line_with_ctx(5, &view, &ctx).is_none());
    }

    #[test]
    fn bridge_handles_default_scroll_and_tracks_state_changes() {
        #[derive(Default)]
        struct ScrollPure;

        impl Plugin for ScrollPure {
            type State = CursorLineState;

            fn id(&self) -> PluginId {
                PluginId("test.scroll-pure".into())
            }

            fn capabilities(&self) -> PluginCapabilities {
                PluginCapabilities::SCROLL_POLICY
            }

            fn handle_default_scroll(
                &self,
                state: &Self::State,
                candidate: DefaultScrollCandidate,
                _app: &AppView<'_>,
            ) -> Option<(Self::State, ScrollPolicyResult)> {
                let mut next = state.clone();
                next.active_line = candidate.screen_line as i32;
                Some((
                    next,
                    ScrollPolicyResult::Immediate(ResolvedScroll::new(
                        candidate.resolved.amount,
                        candidate.resolved.line,
                        candidate.resolved.column,
                    )),
                ))
            }
        }

        let mut bridge = PluginBridge::new(ScrollPure);
        let state = AppState::default();
        let candidate = DefaultScrollCandidate::new(
            10,
            5,
            crate::input::Modifiers::empty(),
            crate::scroll::ScrollGranularity::Line,
            3,
            ResolvedScroll::new(3, 10, 5),
        );

        let result = bridge.handle_default_scroll(candidate, &AppView::new(&state));

        assert_eq!(
            result,
            Some(ScrollPolicyResult::Immediate(ResolvedScroll::new(3, 10, 5)))
        );
        assert_eq!(bridge.state_hash(), 1);
    }

    #[test]
    fn bridge_tracks_workspace_changed_state_updates() {
        #[derive(Default)]
        struct WorkspaceObserverPure;

        impl Plugin for WorkspaceObserverPure {
            type State = u32;

            fn id(&self) -> PluginId {
                PluginId("test.workspace-observer-pure".into())
            }

            fn capabilities(&self) -> PluginCapabilities {
                PluginCapabilities::WORKSPACE_OBSERVER
            }

            fn on_workspace_changed(
                &self,
                state: &Self::State,
                _query: &crate::workspace::WorkspaceQuery<'_>,
            ) -> Self::State {
                state + 1
            }
        }

        let mut bridge = PluginBridge::new(WorkspaceObserverPure);
        let workspace = crate::workspace::Workspace::default();
        let query = workspace.query(Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        });

        bridge.on_workspace_changed(&query);

        assert_eq!(bridge.state_hash(), 1);
    }

    // ---- Registry integration tests ----

    #[test]
    fn register_integrates_with_registry() {
        let mut registry = PluginRuntime::new();
        registry.register(CursorLinePure);
        assert_eq!(registry.plugin_count(), 1);
    }

    #[test]
    fn registry_init_and_state_change() {
        let mut registry = PluginRuntime::new();
        registry.register(CursorLinePure);

        let mut app = AppState::default();
        app.cursor_pos.line = 2;
        app.lines = vec![vec![], vec![], vec![], vec![], vec![]];
        app.cols = 80;
        app.rows = 24;

        let _batch = registry.init_all_batch(&AppView::new(&app));

        // Notify plugins of state change
        let batch = registry.notify_state_changed_batch(&AppView::new(&app), DirtyFlags::BUFFER);
        assert!(batch.effects.commands.is_empty());

        // Prepare cache — should detect state change
        registry.prepare_plugin_cache(DirtyFlags::BUFFER);
        assert!(registry.any_plugin_state_changed());

        // Second prepare with no further changes
        registry.prepare_plugin_cache(DirtyFlags::empty());
        assert!(!registry.any_plugin_state_changed());
    }

    #[test]
    fn registry_collect_annotations_from_pure_plugin() {
        let mut registry = PluginRuntime::new();
        registry.register(CursorLinePure);

        let mut app = AppState::default();
        app.cursor_pos.line = 1;
        app.lines = vec![vec![], vec![], vec![]];
        app.cols = 80;
        app.rows = 24;

        // Init and state change
        let _ = registry.init_all_batch(&AppView::new(&app));
        let batch = registry.notify_state_changed_batch(&AppView::new(&app), DirtyFlags::BUFFER);
        assert!(batch.effects.commands.is_empty());

        let ctx = AnnotateContext {
            line_width: 80,
            gutter_width: 0,
            display_map: None,
            pane_surface_id: None,
            pane_focused: true,
        };
        let result = registry.collect_annotations(&AppView::new(&app), &ctx);
        assert!(result.line_backgrounds.is_some());
        let bgs = result.line_backgrounds.unwrap();
        assert!(bgs[0].is_none()); // line 0: no highlight
        assert!(bgs[1].is_some()); // line 1: cursor line highlighted
        assert!(bgs[2].is_none()); // line 2: no highlight
    }

    // ---- Complex state (ColorPreviewPure) tests ----

    #[test]
    fn complex_state_tracks_changes() {
        let mut bridge = PluginBridge::new(ColorPreviewPure);
        let mut app = AppState::default();
        app.cursor_pos.line = 0;

        bridge.on_state_changed_effects(&AppView::new(&app), DirtyFlags::BUFFER);
        assert_eq!(bridge.state_hash(), 1); // generation bumped

        // Same cursor → state still changes (generation increments)
        bridge.on_state_changed_effects(&AppView::new(&app), DirtyFlags::BUFFER);
        assert_eq!(bridge.state_hash(), 2);
    }

    #[test]
    fn needs_recollect_false_when_dirty_disjoint_from_view_deps() {
        // Plugin that only depends on BUFFER
        struct BufferOnlyPlugin;
        impl Plugin for BufferOnlyPlugin {
            type State = ();
            fn id(&self) -> PluginId {
                PluginId("test.buffer-only".into())
            }
            fn capabilities(&self) -> PluginCapabilities {
                PluginCapabilities::CONTRIBUTOR
            }
            fn view_deps(&self) -> DirtyFlags {
                DirtyFlags::BUFFER
            }
        }

        let mut registry = PluginRuntime::new();
        registry.register(BufferOnlyPlugin);

        // First prepare: always needs recollect (first frame)
        registry.prepare_plugin_cache(DirtyFlags::empty());
        assert!(registry.any_needs_recollect());

        // After first frame, no dirty flags and no state change → skip
        registry.prepare_plugin_cache(DirtyFlags::empty());
        assert!(!registry.any_needs_recollect());

        // STATUS dirty is disjoint from BUFFER view_deps → still skip
        registry.prepare_plugin_cache(DirtyFlags::STATUS);
        assert!(!registry.any_needs_recollect());

        // BUFFER dirty intersects view_deps → needs recollect
        registry.prepare_plugin_cache(DirtyFlags::BUFFER);
        assert!(registry.any_needs_recollect());
    }

    #[test]
    fn needs_recollect_true_when_state_hash_changes() {
        let mut registry = PluginRuntime::new();
        registry.register(CursorLinePure);

        let mut app = AppState::default();
        app.cursor_pos.line = 5;

        // First frame
        registry.prepare_plugin_cache(DirtyFlags::empty());
        assert!(registry.any_needs_recollect());

        // Mutate plugin state
        registry.notify_state_changed_batch(&AppView::new(&app), DirtyFlags::BUFFER);

        // State hash changed → needs recollect even without matching dirty
        registry.prepare_plugin_cache(DirtyFlags::empty());
        assert!(registry.any_needs_recollect());

        // No further state change, no matching dirty → skip
        registry.prepare_plugin_cache(DirtyFlags::empty());
        assert!(!registry.any_needs_recollect());
    }

    #[test]
    fn view_deps_exposed_through_plugin_view() {
        // Plugin that only depends on BUFFER
        struct BufferOnlyPlugin;
        impl Plugin for BufferOnlyPlugin {
            type State = ();
            fn id(&self) -> PluginId {
                PluginId("test.buffer-only-view".into())
            }
            fn view_deps(&self) -> DirtyFlags {
                DirtyFlags::BUFFER
            }
        }

        let mut registry = PluginRuntime::new();
        registry.register(BufferOnlyPlugin);

        // First frame
        registry.prepare_plugin_cache(DirtyFlags::empty());
        assert!(registry.view().any_needs_recollect());

        // Second frame, STATUS only → skip
        registry.prepare_plugin_cache(DirtyFlags::STATUS);
        assert!(!registry.view().any_needs_recollect());

        // BUFFER dirty → recollect
        registry.prepare_plugin_cache(DirtyFlags::BUFFER);
        assert!(registry.view().any_needs_recollect());
    }

    #[test]
    fn is_pure_plugin_marker() {
        let mut bridge = PluginBridge::new(CursorLinePure);
        let state = bridge.plugin_state();
        assert_eq!(format!("{:?}", state), "CursorLineState { active_line: 0 }");

        // Mutate through IsBridgedPlugin (returns &mut dyn PluginState)
        {
            let state_mut = bridge.plugin_state_mut();
            let typed = state_mut
                .as_any_mut()
                .downcast_mut::<CursorLineState>()
                .unwrap();
            typed.active_line = 42;
        }

        let state = bridge.plugin_state();
        let typed = state.as_any().downcast_ref::<CursorLineState>().unwrap();
        assert_eq!(typed.active_line, 42);
    }
}
