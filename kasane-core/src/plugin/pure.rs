//! Plugin system — externalized state, pure function semantics.
//!
//! `Plugin` is the primary user-facing plugin trait where the framework owns the state.
//! All methods are pure functions: `(&self, &State, ...) → (State, effects)`.
//! This enables deterministic rendering and future Salsa memoization of plugin contributions.
//!
//! Use `PluginBridge` to adapt a `Plugin` into the internal `PluginBackend` trait,
//! or register directly via `PluginRegistry::register()`.

use std::any::Any;

use dyn_clone::DynClone;

use crate::element::{Element, InteractiveId};
use crate::input::{KeyEvent, MouseEvent};
use crate::state::{AppState, DirtyFlags};

use super::{
    AnnotateContext, Command, ContributeContext, Contribution, IoEvent, LineAnnotation,
    OverlayContext, OverlayContribution, PluginBackend, PluginCapabilities, PluginId, SlotId,
    TransformContext, TransformTarget,
};

// =============================================================================
// Phase 0: Foundation Types
// =============================================================================

/// Marker trait for externalized plugin state.
///
/// Framework owns `Box<dyn PluginState>` for each `Plugin`.
/// Implements `Clone`, `PartialEq`, `Debug` on trait objects via blanket impl:
/// any `T: Clone + PartialEq + Debug + Send + 'static` automatically satisfies this trait.
pub trait PluginState: DynClone + std::fmt::Debug + Send + 'static {
    /// Downcast to concrete type (immutable).
    fn as_any(&self) -> &dyn Any;
    /// Downcast to concrete type (mutable).
    fn as_any_mut(&mut self) -> &mut dyn Any;
    /// Dynamic equality comparison across trait objects.
    fn dyn_eq(&self, other: &dyn PluginState) -> bool;
}

// Enable Box<dyn PluginState>.clone()
dyn_clone::clone_trait_object!(PluginState);

// Enable *dyn_state_a == *dyn_state_b
impl PartialEq for dyn PluginState {
    fn eq(&self, other: &Self) -> bool {
        self.dyn_eq(other)
    }
}

/// Blanket implementation: any `Clone + PartialEq + Debug + Send + 'static` type
/// can be used as plugin state with zero boilerplate.
impl<T> PluginState for T
where
    T: Clone + PartialEq + std::fmt::Debug + Send + 'static,
{
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn dyn_eq(&self, other: &dyn PluginState) -> bool {
        other
            .as_any()
            .downcast_ref::<T>()
            .is_some_and(|o| self == o)
    }
}

/// Primary plugin trait — state is externalized, all methods are pure functions.
///
/// All `&mut self` methods from `PluginBackend` become `(&self, &State) → (State, effects)`.
/// All `&self` view methods gain `state: &Self::State` parameter.
///
/// Register via `PluginRegistry::register()` or wrap manually with `PluginBridge`.
pub trait Plugin: Send + 'static {
    /// Concrete state type. Must be `Clone + PartialEq + Debug + Send + Default`.
    type State: PluginState + PartialEq + Clone + Default;

    fn id(&self) -> PluginId;

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::empty()
    }

    fn allows_process_spawn(&self) -> bool {
        true
    }

    // --- State transitions (replace &mut self methods) ---

    fn on_init(&self, state: &Self::State, app: &AppState) -> (Self::State, Vec<Command>) {
        let _ = app;
        (state.clone(), vec![])
    }

    fn on_state_changed(
        &self,
        state: &Self::State,
        app: &AppState,
        dirty: DirtyFlags,
    ) -> (Self::State, Vec<Command>) {
        let _ = (app, dirty);
        (state.clone(), vec![])
    }

    fn on_io_event(
        &self,
        state: &Self::State,
        event: &IoEvent,
        app: &AppState,
    ) -> (Self::State, Vec<Command>) {
        let _ = (event, app);
        (state.clone(), vec![])
    }

    fn observe_key(&self, state: &Self::State, key: &KeyEvent, app: &AppState) -> Self::State {
        let _ = (key, app);
        state.clone()
    }

    fn observe_mouse(
        &self,
        state: &Self::State,
        event: &MouseEvent,
        app: &AppState,
    ) -> Self::State {
        let _ = (event, app);
        state.clone()
    }

    fn handle_key(
        &self,
        state: &Self::State,
        key: &KeyEvent,
        app: &AppState,
    ) -> Option<(Self::State, Vec<Command>)> {
        let _ = (state, key, app);
        None
    }

    fn handle_mouse(
        &self,
        state: &Self::State,
        event: &MouseEvent,
        id: InteractiveId,
        app: &AppState,
    ) -> Option<(Self::State, Vec<Command>)> {
        let _ = (state, event, id, app);
        None
    }

    fn update(
        &self,
        state: &Self::State,
        msg: Box<dyn Any>,
        app: &AppState,
    ) -> (Self::State, Vec<Command>) {
        let _ = (msg, app);
        (state.clone(), vec![])
    }

    // --- Pure view methods (state passed as parameter) ---

    fn contribute_to(
        &self,
        state: &Self::State,
        region: &SlotId,
        app: &AppState,
        ctx: &ContributeContext,
    ) -> Option<Contribution> {
        let _ = (state, region, app, ctx);
        None
    }

    fn transform(
        &self,
        state: &Self::State,
        target: &TransformTarget,
        element: Element,
        app: &AppState,
        ctx: &TransformContext,
    ) -> Element {
        let _ = (state, target, app, ctx);
        element
    }

    fn annotate_line_with_ctx(
        &self,
        state: &Self::State,
        line: usize,
        app: &AppState,
        ctx: &AnnotateContext,
    ) -> Option<LineAnnotation> {
        let _ = (state, line, app, ctx);
        None
    }

    fn contribute_overlay_with_ctx(
        &self,
        state: &Self::State,
        app: &AppState,
        ctx: &OverlayContext,
    ) -> Option<OverlayContribution> {
        let _ = (state, app, ctx);
        None
    }

    fn cursor_style_override(
        &self,
        state: &Self::State,
        app: &AppState,
    ) -> Option<crate::render::CursorStyle> {
        let _ = (state, app);
        None
    }

    fn transform_menu_item(
        &self,
        state: &Self::State,
        item: &[crate::protocol::Atom],
        index: usize,
        selected: bool,
        app: &AppState,
    ) -> Option<Vec<crate::protocol::Atom>> {
        let _ = (state, item, index, selected, app);
        None
    }

    fn transform_priority(&self) -> i16 {
        0
    }

    // --- Dependency declarations ---

    fn contribute_deps(&self, _region: &SlotId) -> DirtyFlags {
        DirtyFlags::ALL
    }

    fn transform_deps(&self, _target: &TransformTarget) -> DirtyFlags {
        DirtyFlags::ALL
    }

    fn annotate_deps(&self) -> DirtyFlags {
        DirtyFlags::ALL
    }

    fn overlay_deps(&self) -> DirtyFlags {
        DirtyFlags::ALL
    }
}

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

    // Dependency declarations
    fn contribute_deps_erased(&self, region: &SlotId) -> DirtyFlags;
    fn transform_deps_erased(&self, target: &TransformTarget) -> DirtyFlags;
    fn annotate_deps_erased(&self) -> DirtyFlags;
    fn overlay_deps_erased(&self) -> DirtyFlags;
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

    fn contribute_deps_erased(&self, region: &SlotId) -> DirtyFlags {
        self.contribute_deps(region)
    }
    fn transform_deps_erased(&self, target: &TransformTarget) -> DirtyFlags {
        self.transform_deps(target)
    }
    fn annotate_deps_erased(&self) -> DirtyFlags {
        self.annotate_deps()
    }
    fn overlay_deps_erased(&self) -> DirtyFlags {
        self.overlay_deps()
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

    fn contribute_deps(&self, region: &SlotId) -> DirtyFlags {
        self.inner.contribute_deps_erased(region)
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

    fn transform_deps(&self, target: &TransformTarget) -> DirtyFlags {
        self.inner.transform_deps_erased(target)
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

    fn annotate_deps(&self) -> DirtyFlags {
        self.inner.annotate_deps_erased()
    }

    fn contribute_overlay_with_ctx(
        &self,
        state: &AppState,
        ctx: &OverlayContext,
    ) -> Option<OverlayContribution> {
        self.inner
            .contribute_overlay_erased(&*self.state, state, ctx)
    }

    fn overlay_deps(&self) -> DirtyFlags {
        self.inner.overlay_deps_erased()
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
    use super::*;
    use crate::plugin::{
        AnnotateContext, BackgroundLayer, BlendMode, PluginCapabilities, PluginId, PluginRegistry,
    };
    use crate::protocol::{Color, Face, NamedColor};
    use crate::state::AppState;

    // ---- Phase 2: CursorLinePure test double ----

    #[derive(Clone, Debug, PartialEq, Default)]
    struct CursorLineState {
        active_line: i32,
    }

    struct CursorLinePure;

    impl Plugin for CursorLinePure {
        type State = CursorLineState;

        fn id(&self) -> PluginId {
            PluginId("test.cursor-line-pure".into())
        }

        fn capabilities(&self) -> PluginCapabilities {
            PluginCapabilities::ANNOTATOR
        }

        fn on_state_changed(
            &self,
            state: &Self::State,
            app: &AppState,
            dirty: DirtyFlags,
        ) -> (Self::State, Vec<Command>) {
            if dirty.intersects(DirtyFlags::BUFFER) {
                let new_state = CursorLineState {
                    active_line: app.cursor_pos.line,
                };
                (new_state, vec![])
            } else {
                (state.clone(), vec![])
            }
        }

        fn annotate_line_with_ctx(
            &self,
            state: &Self::State,
            line: usize,
            _app: &AppState,
            _ctx: &AnnotateContext,
        ) -> Option<LineAnnotation> {
            if line as i32 == state.active_line {
                Some(LineAnnotation {
                    left_gutter: None,
                    right_gutter: None,
                    background: Some(BackgroundLayer {
                        face: Face {
                            bg: Color::Named(NamedColor::Blue),
                            ..Face::default()
                        },
                        z_order: 0,
                        blend: BlendMode::Opaque,
                    }),
                    priority: 0,
                })
            } else {
                None
            }
        }

        fn annotate_deps(&self) -> DirtyFlags {
            DirtyFlags::BUFFER
        }
    }

    // ---- Phase 4: ColorPreviewPure test double (complex state) ----

    #[derive(Clone, Debug, PartialEq)]
    struct ColorEntry {
        r: u8,
        g: u8,
        b: u8,
        byte_offset: usize,
    }

    #[derive(Clone, Debug, PartialEq, Default)]
    struct ColorPreviewState {
        color_lines: std::collections::HashMap<usize, Vec<ColorEntry>>,
        active_line: i32,
        generation: u64,
    }

    struct ColorPreviewPure;

    impl Plugin for ColorPreviewPure {
        type State = ColorPreviewState;

        fn id(&self) -> PluginId {
            PluginId("test.color-preview-pure".into())
        }

        fn capabilities(&self) -> PluginCapabilities {
            PluginCapabilities::ANNOTATOR | PluginCapabilities::OVERLAY
        }

        fn on_state_changed(
            &self,
            state: &Self::State,
            app: &AppState,
            dirty: DirtyFlags,
        ) -> (Self::State, Vec<Command>) {
            if dirty.intersects(DirtyFlags::BUFFER) {
                let mut new_state = state.clone();
                new_state.active_line = app.cursor_pos.line;
                new_state.generation += 1;
                (new_state, vec![])
            } else {
                (state.clone(), vec![])
            }
        }

        fn annotate_line_with_ctx(
            &self,
            state: &Self::State,
            line: usize,
            _app: &AppState,
            _ctx: &AnnotateContext,
        ) -> Option<LineAnnotation> {
            if state.color_lines.contains_key(&line) {
                Some(LineAnnotation {
                    left_gutter: None,
                    right_gutter: None,
                    background: Some(BackgroundLayer {
                        face: Face {
                            bg: Color::Named(NamedColor::Green),
                            ..Face::default()
                        },
                        z_order: 0,
                        blend: BlendMode::Opaque,
                    }),
                    priority: 0,
                })
            } else {
                None
            }
        }

        fn annotate_deps(&self) -> DirtyFlags {
            DirtyFlags::BUFFER
        }
    }

    // ---- PluginState trait object tests ----

    #[test]
    fn plugin_state_equality() {
        let s1: Box<dyn PluginState> = Box::new(CursorLineState { active_line: 5 });
        let s2: Box<dyn PluginState> = Box::new(CursorLineState { active_line: 5 });
        let s3: Box<dyn PluginState> = Box::new(CursorLineState { active_line: 10 });

        assert_eq!(*s1, *s2);
        assert_ne!(*s1, *s3);
    }

    #[test]
    fn plugin_state_cross_type_inequality() {
        let s1: Box<dyn PluginState> = Box::new(CursorLineState { active_line: 0 });
        let s2: Box<dyn PluginState> = Box::new(ColorPreviewState::default());
        assert_ne!(*s1, *s2);
    }

    #[test]
    fn plugin_state_clone() {
        let s1: Box<dyn PluginState> = Box::new(CursorLineState { active_line: 5 });
        let s2 = s1.clone();
        assert_eq!(*s1, *s2);
    }

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
        };

        assert!(bridge.annotate_line_with_ctx(3, &app, &ctx).is_some());
        assert!(bridge.annotate_line_with_ctx(0, &app, &ctx).is_none());
        assert!(bridge.annotate_line_with_ctx(5, &app, &ctx).is_none());
    }

    #[test]
    fn bridge_deps_delegated() {
        let bridge = PluginBridge::new(CursorLinePure);
        assert_eq!(bridge.annotate_deps(), DirtyFlags::BUFFER);
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
