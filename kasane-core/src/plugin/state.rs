//! Plugin state and trait definitions — externalized state, pure function semantics.
//!
//! `Plugin` is the primary user-facing plugin trait where the framework owns the state.
//! All methods are pure functions: `(&self, &State, ...) → (State, effects)`.
//! This enables deterministic rendering and future Salsa memoization of plugin contributions.

use std::any::Any;

use dyn_clone::DynClone;

use crate::element::{Element, InteractiveId};
use crate::input::{KeyEvent, MouseEvent};
use crate::state::{AppState, DirtyFlags};

use super::{
    AnnotateContext, Command, ContributeContext, Contribution, DisplayDirective, IoEvent,
    LineAnnotation, OverlayContext, OverlayContribution, PluginCapabilities, PluginId, SlotId,
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

    fn display_directives(&self, state: &Self::State, app: &AppState) -> Vec<DisplayDirective> {
        let _ = (state, app);
        vec![]
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
pub(in crate::plugin) mod tests {
    use super::*;
    use crate::plugin::{BackgroundLayer, BlendMode, PluginCapabilities, PluginId};
    use crate::protocol::{Color, Face, NamedColor};
    use crate::state::AppState;

    // ---- CursorLinePure test double ----

    #[derive(Clone, Debug, PartialEq, Default)]
    pub(in crate::plugin) struct CursorLineState {
        pub(in crate::plugin) active_line: i32,
    }

    pub(in crate::plugin) struct CursorLinePure;

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
    }

    // ---- ColorPreviewPure test double (complex state) ----

    #[derive(Clone, Debug, PartialEq)]
    pub(in crate::plugin) struct ColorEntry {
        pub(in crate::plugin) r: u8,
        pub(in crate::plugin) g: u8,
        pub(in crate::plugin) b: u8,
        pub(in crate::plugin) byte_offset: usize,
    }

    #[derive(Clone, Debug, PartialEq, Default)]
    pub(in crate::plugin) struct ColorPreviewState {
        pub(in crate::plugin) color_lines: std::collections::HashMap<usize, Vec<ColorEntry>>,
        pub(in crate::plugin) active_line: i32,
        pub(in crate::plugin) generation: u64,
    }

    pub(in crate::plugin) struct ColorPreviewPure;

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
}
