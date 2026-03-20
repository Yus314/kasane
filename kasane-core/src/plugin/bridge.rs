//! PluginBridge adapter — adapts `Plugin` to the internal `PluginBackend` trait.
//!
//! Use `PluginBridge` to adapt a `Plugin` into the internal `PluginBackend` trait,
//! or register directly via `PluginRegistry::register()`.

use std::any::Any;

use crate::element::{Element, InteractiveId};
use crate::input::{KeyEvent, MouseEvent};
use crate::scroll::{DefaultScrollCandidate, ScrollPolicyResult};
use crate::state::{AppState, DirtyFlags};

use super::state::{Plugin, PluginState};
use super::{
    AnnotateContext, Command, ContributeContext, Contribution, DisplayDirective, IoEvent,
    LineAnnotation, OverlayContext, OverlayContribution, PluginBackend, PluginCapabilities,
    PluginId, SlotId, TransformContext, TransformTarget,
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
    fn allows_process_spawn(&self) -> bool;
    fn transform_priority(&self) -> i16;

    // State transitions
    fn on_init_erased(&self, state: &mut dyn PluginState, app: &AppState) -> Vec<Command>;
    fn on_state_changed_erased(
        &self,
        state: &mut dyn PluginState,
        app: &AppState,
        dirty: DirtyFlags,
    ) -> Vec<Command>;
    fn on_io_event_erased(
        &self,
        state: &mut dyn PluginState,
        event: &IoEvent,
        app: &AppState,
    ) -> Vec<Command>;
    fn observe_key_erased(&self, state: &mut dyn PluginState, key: &KeyEvent, app: &AppState);
    fn observe_mouse_erased(&self, state: &mut dyn PluginState, event: &MouseEvent, app: &AppState);
    fn handle_key_erased(
        &self,
        state: &mut dyn PluginState,
        key: &KeyEvent,
        app: &AppState,
    ) -> Option<Vec<Command>>;
    fn handle_mouse_erased(
        &self,
        state: &mut dyn PluginState,
        event: &MouseEvent,
        id: InteractiveId,
        app: &AppState,
    ) -> Option<Vec<Command>>;
    fn handle_default_scroll_erased(
        &self,
        state: &mut dyn PluginState,
        candidate: DefaultScrollCandidate,
        app: &AppState,
    ) -> Option<ScrollPolicyResult>;
    fn update_erased(
        &self,
        state: &mut dyn PluginState,
        msg: Box<dyn Any>,
        app: &AppState,
    ) -> Vec<Command>;

    // Pure view methods
    fn contribute_to_erased(
        &self,
        state: &dyn PluginState,
        region: &SlotId,
        app: &AppState,
        ctx: &ContributeContext,
    ) -> Option<Contribution>;
    fn transform_erased(
        &self,
        state: &dyn PluginState,
        target: &TransformTarget,
        element: Element,
        app: &AppState,
        ctx: &TransformContext,
    ) -> Element;
    fn annotate_line_erased(
        &self,
        state: &dyn PluginState,
        line: usize,
        app: &AppState,
        ctx: &AnnotateContext,
    ) -> Option<LineAnnotation>;
    fn contribute_overlay_erased(
        &self,
        state: &dyn PluginState,
        app: &AppState,
        ctx: &OverlayContext,
    ) -> Option<OverlayContribution>;
    fn cursor_style_override_erased(
        &self,
        state: &dyn PluginState,
        app: &AppState,
    ) -> Option<crate::render::CursorStyle>;
    fn transform_menu_item_erased(
        &self,
        state: &dyn PluginState,
        item: &[crate::protocol::Atom],
        index: usize,
        selected: bool,
        app: &AppState,
    ) -> Option<Vec<crate::protocol::Atom>>;

    // Display transform
    fn display_directives_erased(
        &self,
        state: &dyn PluginState,
        app: &AppState,
    ) -> Vec<DisplayDirective>;
}

impl<P: Plugin> ErasedPlugin for P {
    fn id(&self) -> PluginId {
        Plugin::id(self)
    }
    fn capabilities(&self) -> PluginCapabilities {
        Plugin::capabilities(self)
    }
    fn allows_process_spawn(&self) -> bool {
        Plugin::allows_process_spawn(self)
    }
    fn transform_priority(&self) -> i16 {
        Plugin::transform_priority(self)
    }

    fn on_init_erased(&self, state: &mut dyn PluginState, app: &AppState) -> Vec<Command> {
        let typed = state.as_any_mut().downcast_mut::<P::State>().unwrap();
        let (new_state, cmds) = self.on_init(typed, app);
        *typed = new_state;
        cmds
    }

    fn on_state_changed_erased(
        &self,
        state: &mut dyn PluginState,
        app: &AppState,
        dirty: DirtyFlags,
    ) -> Vec<Command> {
        let typed = state.as_any_mut().downcast_mut::<P::State>().unwrap();
        let (new_state, cmds) = self.on_state_changed(typed, app, dirty);
        *typed = new_state;
        cmds
    }

    fn on_io_event_erased(
        &self,
        state: &mut dyn PluginState,
        event: &IoEvent,
        app: &AppState,
    ) -> Vec<Command> {
        let typed = state.as_any_mut().downcast_mut::<P::State>().unwrap();
        let (new_state, cmds) = self.on_io_event(typed, event, app);
        *typed = new_state;
        cmds
    }

    fn observe_key_erased(&self, state: &mut dyn PluginState, key: &KeyEvent, app: &AppState) {
        let typed = state.as_any_mut().downcast_mut::<P::State>().unwrap();
        let new_state = self.observe_key(typed, key, app);
        *typed = new_state;
    }

    fn observe_mouse_erased(
        &self,
        state: &mut dyn PluginState,
        event: &MouseEvent,
        app: &AppState,
    ) {
        let typed = state.as_any_mut().downcast_mut::<P::State>().unwrap();
        let new_state = self.observe_mouse(typed, event, app);
        *typed = new_state;
    }

    fn handle_key_erased(
        &self,
        state: &mut dyn PluginState,
        key: &KeyEvent,
        app: &AppState,
    ) -> Option<Vec<Command>> {
        let typed = state.as_any_mut().downcast_mut::<P::State>().unwrap();
        self.handle_key(typed, key, app).map(|(new_state, cmds)| {
            *typed = new_state;
            cmds
        })
    }

    fn handle_mouse_erased(
        &self,
        state: &mut dyn PluginState,
        event: &MouseEvent,
        id: InteractiveId,
        app: &AppState,
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
        app: &AppState,
    ) -> Option<ScrollPolicyResult> {
        let typed = state.as_any_mut().downcast_mut::<P::State>().unwrap();
        self.handle_default_scroll(typed, candidate, app)
            .map(|(new_state, result)| {
                *typed = new_state;
                result
            })
    }

    fn update_erased(
        &self,
        state: &mut dyn PluginState,
        msg: Box<dyn Any>,
        app: &AppState,
    ) -> Vec<Command> {
        let typed = state.as_any_mut().downcast_mut::<P::State>().unwrap();
        let (new_state, cmds) = self.update(typed, msg, app);
        *typed = new_state;
        cmds
    }

    fn contribute_to_erased(
        &self,
        state: &dyn PluginState,
        region: &SlotId,
        app: &AppState,
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
        app: &AppState,
        ctx: &TransformContext,
    ) -> Element {
        let typed = state.as_any().downcast_ref::<P::State>().unwrap();
        self.transform(typed, target, element, app, ctx)
    }

    fn annotate_line_erased(
        &self,
        state: &dyn PluginState,
        line: usize,
        app: &AppState,
        ctx: &AnnotateContext,
    ) -> Option<LineAnnotation> {
        let typed = state.as_any().downcast_ref::<P::State>().unwrap();
        self.annotate_line_with_ctx(typed, line, app, ctx)
    }

    fn contribute_overlay_erased(
        &self,
        state: &dyn PluginState,
        app: &AppState,
        ctx: &OverlayContext,
    ) -> Option<OverlayContribution> {
        let typed = state.as_any().downcast_ref::<P::State>().unwrap();
        self.contribute_overlay_with_ctx(typed, app, ctx)
    }

    fn cursor_style_override_erased(
        &self,
        state: &dyn PluginState,
        app: &AppState,
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
        app: &AppState,
    ) -> Option<Vec<crate::protocol::Atom>> {
        let typed = state.as_any().downcast_ref::<P::State>().unwrap();
        self.transform_menu_item(typed, item, index, selected, app)
    }

    fn display_directives_erased(
        &self,
        state: &dyn PluginState,
        app: &AppState,
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
/// in `PluginRegistry::prepare_plugin_cache()`.
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

    fn allows_process_spawn(&self) -> bool {
        self.inner.allows_process_spawn()
    }

    fn state_hash(&self) -> u64 {
        self.generation
    }

    fn transform_priority(&self) -> i16 {
        self.inner.transform_priority()
    }

    // --- Lifecycle ---

    fn on_init(&mut self, state: &AppState) -> Vec<Command> {
        let cmds = self.inner.on_init_erased(&mut *self.state, state);
        self.check_state_change();
        cmds
    }

    fn on_shutdown(&mut self) {
        // Plugin has no shutdown hook (pure functions don't need cleanup).
    }

    fn on_state_changed(&mut self, state: &AppState, dirty: DirtyFlags) -> Vec<Command> {
        let cmds = self
            .inner
            .on_state_changed_erased(&mut *self.state, state, dirty);
        self.check_state_change();
        cmds
    }

    fn on_io_event(&mut self, event: &IoEvent, state: &AppState) -> Vec<Command> {
        let cmds = self
            .inner
            .on_io_event_erased(&mut *self.state, event, state);
        self.check_state_change();
        cmds
    }

    // --- Input ---

    fn observe_key(&mut self, key: &KeyEvent, state: &AppState) {
        self.inner.observe_key_erased(&mut *self.state, key, state);
        self.check_state_change();
    }

    fn observe_mouse(&mut self, event: &MouseEvent, state: &AppState) {
        self.inner
            .observe_mouse_erased(&mut *self.state, event, state);
        self.check_state_change();
    }

    fn handle_key(&mut self, key: &KeyEvent, state: &AppState) -> Option<Vec<Command>> {
        let result = self.inner.handle_key_erased(&mut *self.state, key, state);
        self.check_state_change();
        result
    }

    fn handle_mouse(
        &mut self,
        event: &MouseEvent,
        id: InteractiveId,
        state: &AppState,
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
        state: &AppState,
    ) -> Option<ScrollPolicyResult> {
        let result = self
            .inner
            .handle_default_scroll_erased(&mut *self.state, candidate, state);
        self.check_state_change();
        result
    }

    fn update(&mut self, msg: Box<dyn Any>, state: &AppState) -> Vec<Command> {
        let cmds = self.inner.update_erased(&mut *self.state, msg, state);
        self.check_state_change();
        cmds
    }

    // --- View contributions ---

    fn contribute_to(
        &self,
        region: &SlotId,
        state: &AppState,
        ctx: &ContributeContext,
    ) -> Option<Contribution> {
        self.inner
            .contribute_to_erased(&*self.state, region, state, ctx)
    }

    fn transform(
        &self,
        target: &TransformTarget,
        element: Element,
        state: &AppState,
        ctx: &TransformContext,
    ) -> Element {
        self.inner
            .transform_erased(&*self.state, target, element, state, ctx)
    }

    fn annotate_line_with_ctx(
        &self,
        line: usize,
        state: &AppState,
        ctx: &AnnotateContext,
    ) -> Option<LineAnnotation> {
        self.inner
            .annotate_line_erased(&*self.state, line, state, ctx)
    }

    fn display_directives(&self, state: &AppState) -> Vec<DisplayDirective> {
        self.inner.display_directives_erased(&*self.state, state)
    }

    fn contribute_overlay_with_ctx(
        &self,
        state: &AppState,
        ctx: &OverlayContext,
    ) -> Option<OverlayContribution> {
        self.inner
            .contribute_overlay_erased(&*self.state, state, ctx)
    }

    fn cursor_style_override(&self, state: &AppState) -> Option<crate::render::CursorStyle> {
        self.inner.cursor_style_override_erased(&*self.state, state)
    }

    fn transform_menu_item(
        &self,
        item: &[crate::protocol::Atom],
        index: usize,
        selected: bool,
        state: &AppState,
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
    use crate::plugin::{AnnotateContext, PluginCapabilities, PluginId, PluginRegistry};
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
        bridge.on_state_changed(&app, DirtyFlags::BUFFER);
        assert_eq!(bridge.state_hash(), 1);

        // Same input → same state → no generation bump
        bridge.on_state_changed(&app, DirtyFlags::BUFFER);
        assert_eq!(bridge.state_hash(), 1);

        // Different input → different state → generation bumps
        app.cursor_pos.line = 10;
        bridge.on_state_changed(&app, DirtyFlags::BUFFER);
        assert_eq!(bridge.state_hash(), 2);
    }

    #[test]
    fn bridge_no_change_on_irrelevant_dirty() {
        let mut bridge = PluginBridge::new(CursorLinePure);
        let app = AppState::default();

        // STATUS dirty doesn't trigger CursorLinePure's on_state_changed logic
        bridge.on_state_changed(&app, DirtyFlags::STATUS);
        assert_eq!(bridge.state_hash(), 0);
    }

    #[test]
    fn bridge_annotates_cursor_line() {
        let mut bridge = PluginBridge::new(CursorLinePure);
        let mut app = AppState::default();
        app.cursor_pos.line = 3;

        bridge.on_state_changed(&app, DirtyFlags::BUFFER);

        let ctx = AnnotateContext {
            line_width: 80,
            gutter_width: 0,
            display_map: None,
        };

        assert!(bridge.annotate_line_with_ctx(3, &app, &ctx).is_some());
        assert!(bridge.annotate_line_with_ctx(0, &app, &ctx).is_none());
        assert!(bridge.annotate_line_with_ctx(5, &app, &ctx).is_none());
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
                _app: &AppState,
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

        let result = bridge.handle_default_scroll(candidate, &state);

        assert_eq!(
            result,
            Some(ScrollPolicyResult::Immediate(ResolvedScroll::new(3, 10, 5)))
        );
        assert_eq!(bridge.state_hash(), 1);
    }

    // ---- Registry integration tests ----

    #[test]
    fn register_integrates_with_registry() {
        let mut registry = PluginRegistry::new();
        registry.register(CursorLinePure);
        assert_eq!(registry.plugin_count(), 1);
    }

    #[test]
    fn registry_init_and_state_change() {
        let mut registry = PluginRegistry::new();
        registry.register(CursorLinePure);

        let mut app = AppState::default();
        app.cursor_pos.line = 2;
        app.lines = vec![vec![], vec![], vec![], vec![], vec![]];
        app.cols = 80;
        app.rows = 24;

        let cmds = registry.init_all(&app);
        assert!(cmds.is_empty());

        // Notify plugins of state change
        for plugin in registry.plugins_mut() {
            plugin.on_state_changed(&app, DirtyFlags::BUFFER);
        }

        // Prepare cache — should detect state change
        registry.prepare_plugin_cache(DirtyFlags::BUFFER);
        assert!(registry.any_plugin_state_changed());

        // Second prepare with no further changes
        registry.prepare_plugin_cache(DirtyFlags::empty());
        assert!(!registry.any_plugin_state_changed());
    }

    #[test]
    fn registry_collect_annotations_from_pure_plugin() {
        let mut registry = PluginRegistry::new();
        registry.register(CursorLinePure);

        let mut app = AppState::default();
        app.cursor_pos.line = 1;
        app.lines = vec![vec![], vec![], vec![]];
        app.cols = 80;
        app.rows = 24;

        // Init and state change
        registry.init_all(&app);
        for plugin in registry.plugins_mut() {
            plugin.on_state_changed(&app, DirtyFlags::BUFFER);
        }

        let ctx = AnnotateContext {
            line_width: 80,
            gutter_width: 0,
            display_map: None,
        };
        let result = registry.collect_annotations(&app, &ctx);
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

        bridge.on_state_changed(&app, DirtyFlags::BUFFER);
        assert_eq!(bridge.state_hash(), 1); // generation bumped

        // Same cursor → state still changes (generation increments)
        bridge.on_state_changed(&app, DirtyFlags::BUFFER);
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
