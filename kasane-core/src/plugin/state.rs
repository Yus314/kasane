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
/// Implements `Clone`, `PartialEq`, and `Debug` on trait objects via
/// blanket impl: any `T: Clone + PartialEq + Debug + Send + 'static`
/// automatically satisfies this trait.
///
/// Change detection in [`PluginBridge`](super::plugin_bridge::PluginBridge) uses
/// [`dyn_eq`](Self::dyn_eq) against a clone of the previous state. There
/// is no longer a hash-based fast path, so plugin-state types are free to
/// contain `HashMap` or other non-`Hash` collections without boilerplate.
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

/// Blanket implementation: any `Clone + PartialEq + Debug + Send + 'static`
/// type can be used as plugin state with zero boilerplate.
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
///
/// Plugins that don't need state should impl [`StatelessPlugin`] instead —
/// the blanket impl below auto-derives `Plugin<State = ()>` from any
/// `StatelessPlugin`, eliminating the `type State = ();` boilerplate.
pub trait Plugin: Send + 'static {
    /// Concrete state type. Must be `Clone + PartialEq + Debug + Send + Default`.
    type State: PluginState + PartialEq + Clone + Default;

    /// Unique plugin identifier.
    fn id(&self) -> super::PluginId;

    /// Register handlers on the given registry.
    fn register(&self, registry: &mut super::HandlerRegistry<Self::State>);
}

/// Plugin trait for stateless plugins — those that maintain no per-instance
/// state across handler invocations.
///
/// Implementing `StatelessPlugin` automatically gives you `Plugin<State = ()>`
/// via the blanket impl below, so you don't have to spell out
/// `type State = ();` in every stateless plugin.
///
/// **When to use**: any plugin whose `register()` callbacks don't need to
/// read or mutate plugin-owned state — typical examples are the WASM
/// adapter (state lives guest-side), pure renderers, observers that
/// only emit effects, and most builtin plugins.
///
/// **When NOT to use**: plugins that need to memoize / cache / accumulate
/// state across handler calls. Implement [`Plugin`] directly with a
/// concrete `type State`.
///
/// Coherence note: a single type may impl `StatelessPlugin` *or* `Plugin`,
/// but not both — the blanket below would conflict.
pub trait StatelessPlugin: Send + 'static {
    /// Unique plugin identifier — same contract as [`Plugin::id`].
    fn id(&self) -> super::PluginId;

    /// Register handlers on a `HandlerRegistry<()>` — same contract as
    /// [`Plugin::register`] specialized to `Self::State = ()`.
    fn register(&self, registry: &mut super::HandlerRegistry<()>);
}

impl<P: StatelessPlugin> Plugin for P {
    type State = ();

    fn id(&self) -> super::PluginId {
        StatelessPlugin::id(self)
    }

    fn register(&self, registry: &mut super::HandlerRegistry<Self::State>) {
        StatelessPlugin::register(self, registry)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
pub(in crate::plugin) mod tests {
    use super::*;
    use crate::plugin::{
        BackgroundLayer, BlendMode, HandlerRegistry, KakouneSideEffects, PluginId,
    };
    use crate::protocol::{Color, NamedColor, WireFace};
    use crate::state::DirtyFlags;

    // ---- CursorLinePure test double ----

    #[derive(Clone, Debug, PartialEq, Hash, Default)]
    pub(in crate::plugin) struct CursorLineState {
        pub(in crate::plugin) active_line: i32,
    }

    pub(in crate::plugin) struct CursorLinePure;

    impl Plugin for CursorLinePure {
        type State = CursorLineState;

        fn id(&self) -> PluginId {
            PluginId::from("test.cursor-line-pure")
        }

        fn register(&self, r: &mut HandlerRegistry<CursorLineState>) {
            r.declare_interests(DirtyFlags::BUFFER);
            r.on_state_changed_tier1(|state, app, dirty| {
                if dirty.intersects(DirtyFlags::BUFFER) {
                    let new_state = CursorLineState {
                        active_line: app.cursor_line(),
                    };
                    (new_state, KakouneSideEffects::none())
                } else {
                    (state.clone(), KakouneSideEffects::none())
                }
            });
            r.on_background(|state, line, _app, _ctx| {
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

    #[derive(Clone, Debug, PartialEq, Hash)]
    pub(in crate::plugin) struct ColorEntry {
        pub(in crate::plugin) r: u8,
        pub(in crate::plugin) g: u8,
        pub(in crate::plugin) b: u8,
        pub(in crate::plugin) byte_offset: usize,
    }

    /// `color_lines` uses `BTreeMap` (not `HashMap`) so the derived `Hash` is
    /// deterministic — required by the `PluginState::state_hash` contract.
    #[derive(Clone, Debug, PartialEq, Hash, Default)]
    pub(in crate::plugin) struct ColorPreviewState {
        pub(in crate::plugin) color_lines: std::collections::BTreeMap<usize, Vec<ColorEntry>>,
        pub(in crate::plugin) active_line: i32,
        pub(in crate::plugin) generation: u64,
    }

    pub(in crate::plugin) struct ColorPreviewPure;

    impl Plugin for ColorPreviewPure {
        type State = ColorPreviewState;

        fn id(&self) -> PluginId {
            PluginId::from("test.color-preview-pure")
        }

        fn register(&self, r: &mut HandlerRegistry<ColorPreviewState>) {
            r.declare_interests(DirtyFlags::BUFFER);
            r.on_state_changed_tier1(|state, app, dirty| {
                if dirty.intersects(DirtyFlags::BUFFER) {
                    let mut new_state = state.clone();
                    new_state.active_line = app.cursor_line();
                    new_state.generation += 1;
                    (new_state, KakouneSideEffects::none())
                } else {
                    (state.clone(), KakouneSideEffects::none())
                }
            });
            r.on_background(|state, line, _app, _ctx| {
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
