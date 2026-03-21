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

impl<P: Plugin> ErasedPlugin for P {
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

    fn on_init_effects_erased(
        &self,
        state: &mut dyn PluginState,
        app: &AppView<'_>,
    ) -> BootstrapEffects {
        let typed = state.as_any_mut().downcast_mut::<P::State>().unwrap();
        let (new_state, effects) = self.on_init_effects(typed, app);
        *typed = new_state;
        effects
    }

    fn on_active_session_ready_effects_erased(
        &self,
        state: &mut dyn PluginState,
        app: &AppView<'_>,
    ) -> SessionReadyEffects {
        let typed = state.as_any_mut().downcast_mut::<P::State>().unwrap();
        let (new_state, effects) = self.on_active_session_ready_effects(typed, app);
        *typed = new_state;
        effects
    }

    fn on_state_changed_effects_erased(
        &self,
        state: &mut dyn PluginState,
        app: &AppView<'_>,
        dirty: DirtyFlags,
    ) -> RuntimeEffects {
        let typed = state.as_any_mut().downcast_mut::<P::State>().unwrap();
        let (new_state, effects) = self.on_state_changed_effects(typed, app, dirty);
        *typed = new_state;
        effects
    }

    fn on_io_event_effects_erased(
        &self,
        state: &mut dyn PluginState,
        event: &IoEvent,
        app: &AppView<'_>,
    ) -> RuntimeEffects {
        let typed = state.as_any_mut().downcast_mut::<P::State>().unwrap();
        let (new_state, effects) = self.on_io_event_effects(typed, event, app);
        *typed = new_state;
        effects
    }

    fn on_workspace_changed_erased(&self, state: &mut dyn PluginState, query: &WorkspaceQuery<'_>) {
        let typed = state.as_any_mut().downcast_mut::<P::State>().unwrap();
        let new_state = self.on_workspace_changed(typed, query);
        *typed = new_state;
    }

    fn observe_key_erased(&self, state: &mut dyn PluginState, key: &KeyEvent, app: &AppView<'_>) {
        let typed = state.as_any_mut().downcast_mut::<P::State>().unwrap();
        let new_state = self.observe_key(typed, key, app);
        *typed = new_state;
    }

    fn observe_mouse_erased(
        &self,
        state: &mut dyn PluginState,
        event: &MouseEvent,
        app: &AppView<'_>,
    ) {
        let typed = state.as_any_mut().downcast_mut::<P::State>().unwrap();
        let new_state = self.observe_mouse(typed, event, app);
        *typed = new_state;
    }

    fn handle_key_erased(
        &self,
        state: &mut dyn PluginState,
        key: &KeyEvent,
        app: &AppView<'_>,
    ) -> Option<Vec<Command>> {
        let typed = state.as_any_mut().downcast_mut::<P::State>().unwrap();
        self.handle_key(typed, key, app).map(|(new_state, cmds)| {
            *typed = new_state;
            cmds
        })
    }

    fn handle_key_middleware_erased(
        &self,
        state: &mut dyn PluginState,
        key: &KeyEvent,
        app: &AppView<'_>,
    ) -> KeyHandleResult {
        let typed = state.as_any_mut().downcast_mut::<P::State>().unwrap();
        let (new_state, result) = self.handle_key_middleware(typed, key, app);
        *typed = new_state;
        result
    }

    fn handle_mouse_erased(
        &self,
        state: &mut dyn PluginState,
        event: &MouseEvent,
        id: InteractiveId,
        app: &AppView<'_>,
    ) -> Option<Vec<Command>> {
        let typed = state.as_any_mut().downcast_mut::<P::State>().unwrap();
        self.handle_mouse(typed, event, id, app)
            .map(|(new_state, cmds)| {
                *typed = new_state;
                cmds
            })
    }

    fn handle_default_scroll_erased(
        &self,
        state: &mut dyn PluginState,
        candidate: DefaultScrollCandidate,
        app: &AppView<'_>,
    ) -> Option<ScrollPolicyResult> {
        let typed = state.as_any_mut().downcast_mut::<P::State>().unwrap();
        self.handle_default_scroll(typed, candidate, app)
            .map(|(new_state, result)| {
                *typed = new_state;
                result
            })
    }

    fn update_effects_erased(
        &self,
        state: &mut dyn PluginState,
        msg: &mut dyn Any,
        app: &AppView<'_>,
    ) -> RuntimeEffects {
        let typed = state.as_any_mut().downcast_mut::<P::State>().unwrap();
        let (new_state, effects) = self.update_effects(typed, msg, app);
        *typed = new_state;
        effects
    }

    fn contribute_to_erased(
        &self,
        state: &dyn PluginState,
        region: &SlotId,
        app: &AppView<'_>,
        ctx: &ContributeContext,
    ) -> Option<Contribution> {
        let typed = state.as_any().downcast_ref::<P::State>().unwrap();
        self.contribute_to(typed, region, app, ctx)
    }

    fn transform_erased(
        &self,
        state: &dyn PluginState,
        target: &TransformTarget,
        element: Element,
        app: &AppView<'_>,
        ctx: &TransformContext,
    ) -> Element {
        let typed = state.as_any().downcast_ref::<P::State>().unwrap();
        self.transform(typed, target, element, app, ctx)
    }

    fn annotate_line_erased(
        &self,
        state: &dyn PluginState,
        line: usize,
        app: &AppView<'_>,
        ctx: &AnnotateContext,
    ) -> Option<LineAnnotation> {
        let typed = state.as_any().downcast_ref::<P::State>().unwrap();
        self.annotate_line_with_ctx(typed, line, app, ctx)
    }

    fn contribute_overlay_erased(
        &self,
        state: &dyn PluginState,
        app: &AppView<'_>,
        ctx: &OverlayContext,
    ) -> Option<OverlayContribution> {
        let typed = state.as_any().downcast_ref::<P::State>().unwrap();
        self.contribute_overlay_with_ctx(typed, app, ctx)
    }

    fn cursor_style_override_erased(
        &self,
        state: &dyn PluginState,
        app: &AppView<'_>,
    ) -> Option<crate::render::CursorStyle> {
        let typed = state.as_any().downcast_ref::<P::State>().unwrap();
        self.cursor_style_override(typed, app)
    }

    fn transform_menu_item_erased(
        &self,
        state: &dyn PluginState,
        item: &[crate::protocol::Atom],
        index: usize,
        selected: bool,
        app: &AppView<'_>,
    ) -> Option<Vec<crate::protocol::Atom>> {
        let typed = state.as_any().downcast_ref::<P::State>().unwrap();
        self.transform_menu_item(typed, item, index, selected, app)
    }

    fn display_directives_erased(
        &self,
        state: &dyn PluginState,
        app: &AppView<'_>,
    ) -> Vec<DisplayDirective> {
        let typed = state.as_any().downcast_ref::<P::State>().unwrap();
        self.display_directives(typed, app)
    }
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

impl PluginBackend for PluginBridge {
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

    // --- Lifecycle ---

    fn on_init_effects(&mut self, state: &AppView<'_>) -> BootstrapEffects {
        let effects = self.inner.on_init_effects_erased(&mut *self.state, state);
        self.check_state_change();
        effects
    }

    fn on_active_session_ready_effects(&mut self, state: &AppView<'_>) -> SessionReadyEffects {
        let effects = self
            .inner
            .on_active_session_ready_effects_erased(&mut *self.state, state);
        self.check_state_change();
        effects
    }

    fn on_state_changed_effects(
        &mut self,
        state: &AppView<'_>,
        dirty: DirtyFlags,
    ) -> RuntimeEffects {
        let effects = self
            .inner
            .on_state_changed_effects_erased(&mut *self.state, state, dirty);
        self.check_state_change();
        effects
    }

    fn on_shutdown(&mut self) {
        // Plugin has no shutdown hook (pure functions don't need cleanup).
    }

    fn on_io_event_effects(&mut self, event: &IoEvent, state: &AppView<'_>) -> RuntimeEffects {
        let effects = self
            .inner
            .on_io_event_effects_erased(&mut *self.state, event, state);
        self.check_state_change();
        effects
    }

    fn on_workspace_changed(&mut self, query: &WorkspaceQuery<'_>) {
        self.inner
            .on_workspace_changed_erased(&mut *self.state, query);
        self.check_state_change();
    }

    // --- Input ---

    fn observe_key(&mut self, key: &KeyEvent, state: &AppView<'_>) {
        self.inner.observe_key_erased(&mut *self.state, key, state);
        self.check_state_change();
    }

    fn observe_mouse(&mut self, event: &MouseEvent, state: &AppView<'_>) {
        self.inner
            .observe_mouse_erased(&mut *self.state, event, state);
        self.check_state_change();
    }

    fn handle_key(&mut self, key: &KeyEvent, state: &AppView<'_>) -> Option<Vec<Command>> {
        let result = self.inner.handle_key_erased(&mut *self.state, key, state);
        self.check_state_change();
        result
    }

    fn handle_key_middleware(&mut self, key: &KeyEvent, state: &AppView<'_>) -> KeyHandleResult {
        let result = self
            .inner
            .handle_key_middleware_erased(&mut *self.state, key, state);
        self.check_state_change();
        result
    }

    fn handle_mouse(
        &mut self,
        event: &MouseEvent,
        id: InteractiveId,
        state: &AppView<'_>,
    ) -> Option<Vec<Command>> {
        let result = self
            .inner
            .handle_mouse_erased(&mut *self.state, event, id, state);
        self.check_state_change();
        result
    }

    fn handle_default_scroll(
        &mut self,
        candidate: DefaultScrollCandidate,
        state: &AppView<'_>,
    ) -> Option<ScrollPolicyResult> {
        let result = self
            .inner
            .handle_default_scroll_erased(&mut *self.state, candidate, state);
        self.check_state_change();
        result
    }

    fn update_effects(&mut self, msg: &mut dyn Any, state: &AppView<'_>) -> RuntimeEffects {
        let effects = self
            .inner
            .update_effects_erased(&mut *self.state, msg, state);
        self.check_state_change();
        effects
    }

    // --- View contributions ---

    fn contribute_to(
        &self,
        region: &SlotId,
        state: &AppView<'_>,
        ctx: &ContributeContext,
    ) -> Option<Contribution> {
        self.inner
            .contribute_to_erased(&*self.state, region, state, ctx)
    }

    fn transform(
        &self,
        target: &TransformTarget,
        element: Element,
        state: &AppView<'_>,
        ctx: &TransformContext,
    ) -> Element {
        self.inner
            .transform_erased(&*self.state, target, element, state, ctx)
    }

    fn annotate_line_with_ctx(
        &self,
        line: usize,
        state: &AppView<'_>,
        ctx: &AnnotateContext,
    ) -> Option<LineAnnotation> {
        self.inner
            .annotate_line_erased(&*self.state, line, state, ctx)
    }

    fn display_directives(&self, state: &AppView<'_>) -> Vec<DisplayDirective> {
        self.inner.display_directives_erased(&*self.state, state)
    }

    fn contribute_overlay_with_ctx(
        &self,
        state: &AppView<'_>,
        ctx: &OverlayContext,
    ) -> Option<OverlayContribution> {
        self.inner
            .contribute_overlay_erased(&*self.state, state, ctx)
    }

    fn cursor_style_override(&self, state: &AppView<'_>) -> Option<crate::render::CursorStyle> {
        self.inner.cursor_style_override_erased(&*self.state, state)
    }

    fn transform_menu_item(
        &self,
        item: &[crate::protocol::Atom],
        index: usize,
        selected: bool,
        state: &AppView<'_>,
    ) -> Option<Vec<crate::protocol::Atom>> {
        self.inner
            .transform_menu_item_erased(&*self.state, item, index, selected, state)
    }
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
