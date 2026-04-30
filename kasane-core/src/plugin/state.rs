//! Plugin state and trait definitions — externalized state, pure function semantics.
//!
//! `Plugin` is the primary user-facing plugin trait where the framework owns the state.
//! All methods are pure functions: `(&self, &State, ...) → (State, effects)`.
//! This enables deterministic rendering and future Salsa memoization of plugin contributions.

use std::any::Any;

use dyn_clone::DynClone;

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

/// Primary plugin trait — register extension points via [`HandlerRegistry`].
///
/// All extension points are declared in `register()` by calling registration
/// methods on the provided [`HandlerRegistry`]. Capabilities are auto-inferred
/// from which handlers are registered.
///
/// Register via `PluginRuntime::register()` or wrap manually with `PluginBridge`.
pub trait Plugin: Send + 'static {
    /// Concrete state type. Must be `Clone + PartialEq + Debug + Send + Default`.
    type State: PluginState + PartialEq + Clone + Default;

    /// Unique plugin identifier.
    fn id(&self) -> super::PluginId;

    /// Register handlers on the given registry.
    fn register(&self, registry: &mut super::HandlerRegistry<Self::State>);
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
pub(in crate::plugin) mod tests {
    use super::*;
    use crate::plugin::{BackgroundLayer, BlendMode, Effects, HandlerRegistry, PluginId};
    use crate::protocol::{Color, NamedColor, WireFace};
    use crate::state::DirtyFlags;

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

        fn register(&self, r: &mut HandlerRegistry<CursorLineState>) {
            r.declare_interests(DirtyFlags::BUFFER);
            r.on_state_changed(|state, app, dirty| {
                if dirty.intersects(DirtyFlags::BUFFER) {
                    let new_state = CursorLineState {
                        active_line: app.cursor_line(),
                    };
                    (new_state, Effects::default())
                } else {
                    (state.clone(), Effects::default())
                }
            });
            r.on_annotate_background(|state, line, _app, _ctx| {
                if line as i32 == state.active_line {
                    Some(BackgroundLayer {
                        style: crate::protocol::Style::from_face(&WireFace {
                            bg: Color::Named(NamedColor::Blue),
                            ..WireFace::default()
                        }),
                        z_order: 0,
                        blend: BlendMode::Opaque,
                    })
                } else {
                    None
                }
            });
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

        fn register(&self, r: &mut HandlerRegistry<ColorPreviewState>) {
            r.declare_interests(DirtyFlags::BUFFER);
            r.on_state_changed(|state, app, dirty| {
                if dirty.intersects(DirtyFlags::BUFFER) {
                    let mut new_state = state.clone();
                    new_state.active_line = app.cursor_line();
                    new_state.generation += 1;
                    (new_state, Effects::default())
                } else {
                    (state.clone(), Effects::default())
                }
            });
            r.on_annotate_background(|state, line, _app, _ctx| {
                if state.color_lines.contains_key(&line) {
                    Some(BackgroundLayer {
                        style: crate::protocol::Style::from_face(&WireFace {
                            bg: Color::Named(NamedColor::Green),
                            ..WireFace::default()
                        }),
                        z_order: 0,
                        blend: BlendMode::Opaque,
                    })
                } else {
                    None
                }
            });
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
