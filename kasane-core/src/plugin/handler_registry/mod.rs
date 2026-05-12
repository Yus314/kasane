//! Type-safe handler registration for the `Plugin` trait architecture.
//!
//! [`HandlerRegistry`] provides typed registration methods that accept closures
//! parameterized over the plugin's concrete state type `S`. Calling
//! [`into_table()`](HandlerRegistry::into_table) performs type erasure and
//! produces a [`HandlerTable`] for framework-internal dispatch.
//!
//! # Example (Phase 2+)
//!
//! ```ignore
//! fn register(&self, r: &mut HandlerRegistry<MyState>) {
//!     r.declare_interests(DirtyFlags::BUFFER);
//!     r.on_state_changed_tier1(|state, app, dirty| {
//!         // ...
//!         (new_state, KakouneSideEffects::default())
//!     });
//!     r.on_decorate_background(|state, line, app, ctx| {
//!         // ...
//!         Some(BackgroundLayer { ... })
//!     });
//! }
//! ```

// Items used by the `tests` submodule (in `tests.rs`) as well as
// KeyMapBuilder. The split-out axis modules each carry their own
// use-statements; this top-level set is intentionally broad so the
// `tests` module (which uses `super::*` and exercises every on_*
// method) compiles without per-test imports.
// `#[allow(unused_imports)]` covers the gap between the lib-only
// build (which uses only KeyMapBuilder + the macros + the Transparency
// impls) and the test build.
#[allow(unused_imports)]
use std::any::Any;
use std::marker::PhantomData;

#[allow(unused_imports)]
use serde::{Serialize, de::DeserializeOwned};

#[allow(unused_imports)]
use crate::display::navigation::{ActionResult, NavigationAction, NavigationPolicy};
#[allow(unused_imports)]
use crate::display::unit::DisplayUnit;
#[allow(unused_imports)]
use crate::element::{Element, InteractiveId, Overlay};
#[allow(unused_imports)]
use crate::input::{
    ChordBinding, CompiledKeyMap, DropEvent, KeyBinding, KeyEvent, KeyGroup, KeyPattern,
    KeyResponse, MouseEvent,
};
#[allow(unused_imports)]
use crate::protocol::Atom;
#[allow(unused_imports)]
use crate::render::InlineDecoration;
#[allow(unused_imports)]
use crate::scroll::{DefaultScrollCandidate, ScrollPolicyResult};
#[allow(unused_imports)]
use crate::state::DirtyFlags;
#[allow(unused_imports)]
use crate::workspace::WorkspaceQuery;

#[allow(unused_imports)]
use super::channel::ChannelValue;
#[allow(unused_imports)]
use super::element_patch::ElementPatch;
#[allow(unused_imports)]
use super::handler_table::{
    ContributeEntry, GutterHandlerEntry, GutterSide, HandlerTable, TransformEntry,
};
use super::kakoune_transparent_effects::KakouneTransparentEffects;
#[allow(unused_imports)]
use super::process_task::{ProcessTaskEntry, ProcessTaskResult, ProcessTaskSpec};
#[allow(unused_imports)]
use super::pubsub::{PublishEntry, SubscribeEntry, Topic, TopicId};
#[allow(unused_imports)]
use super::traits::{
    KeyHandleResult, KeyPreDispatchResult, MousePreDispatchResult, TextInputPreDispatchResult,
};
#[allow(unused_imports)]
use super::{
    AnnotateContext, AppView, BackgroundLayer, Command, ContributeContext, Contribution,
    DisplayDirective, Effects, IoEvent, KakouneTransparentCommand, OrnamentBatch, OverlayContext,
    OverlayContribution, PluginState, RenderOrnamentContext, SlotId, TransformContext,
    TransformTarget, VirtualTextItem,
};

/// Marker trait for handler return types that carry transparency metadata.
///
/// When `IS_TRANSPARENT` is true, the framework records that the handler was
/// registered with a transparent type, enabling compile-time guarantees about
/// the absence of Kakoune writes (ADR-030).
pub trait Transparency {
    /// Whether this type represents a transparent handler return.
    const IS_TRANSPARENT: bool;
}

impl Transparency for Effects {
    const IS_TRANSPARENT: bool = false;
}

impl Transparency for KakouneTransparentEffects {
    const IS_TRANSPARENT: bool = true;
}

impl Transparency for Command {
    const IS_TRANSPARENT: bool = false;
}

impl Transparency for KakouneTransparentCommand {
    const IS_TRANSPARENT: bool = true;
}

impl Transparency for KeyHandleResult {
    const IS_TRANSPARENT: bool = false;
}

impl Transparency for super::KakouneTransparentKeyResult {
    const IS_TRANSPARENT: bool = true;
}

/// Context passed to `on_virtual_edit` handlers when a shadow cursor edit is committed.
#[derive(Debug, Clone)]
pub struct VirtualEditContext {
    /// Buffer line anchoring the editable span (0-indexed).
    pub anchor_line: usize,
    /// Index of the span within the editable virtual text.
    pub span_index: usize,
    /// Original text content at activation time.
    pub original_text: String,
    /// Current edited text content.
    pub working_text: String,
    /// Byte range within the anchor buffer line (for Mirror reference).
    pub buffer_byte_range: std::ops::Range<usize>,
}

/// Downcast state, call handler, box the new state and return `(BoxedState, second.into())`.
macro_rules! register_state_effect {
    ($self:ident, $field:ident, $handler:ident $(, $arg:ident)*) => {
        $self.table.$field = Some(Box::new(move |state, $($arg),*| {
            let s = state.as_any().downcast_ref::<S>().expect("state type mismatch");
            let (new_state, effects) = $handler(s, $($arg),*);
            (Box::new(new_state) as Box<dyn PluginState>, effects.into())
        }));
    };
}

/// Downcast state, call handler, forward the return value directly.
macro_rules! register_view {
    ($self:ident, $field:ident, $handler:ident $(, $arg:ident)*) => {
        $self.table.$field = Some(Box::new(move |state, $($arg),*| {
            let s = state.as_any().downcast_ref::<S>().expect("state type mismatch");
            $handler(s, $($arg),*)
        }));
    };
}

/// Downcast state, call handler, box only the returned state.
macro_rules! register_state_only {
    ($self:ident, $field:ident, $handler:ident $(, $arg:ident)*) => {
        $self.table.$field = Some(Box::new(move |state, $($arg),*| {
            let s = state.as_any().downcast_ref::<S>().expect("state type mismatch");
            Box::new($handler(s, $($arg),*)) as Box<dyn PluginState>
        }));
    };
}

/// Downcast state, call handler (no return value).
macro_rules! register_void {
    ($self:ident, $field:ident, $handler:ident) => {
        $self.table.$field = Some(Box::new(move |state| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            $handler(s);
        }));
    };
}

/// Type-safe handler registration builder.
///
/// `S` is the plugin's concrete state type. Registration methods accept closures
/// over `&S` and automatically infer [`PluginCapabilities`] from which handlers
/// are registered.
pub struct HandlerRegistry<S: PluginState> {
    table: HandlerTable,
    _phantom: PhantomData<S>,
}

// Re-export the shared registration macros to all axis submodules.

mod decoration;
mod extension;
mod input;
mod lifecycle;
mod render;
mod transform;

impl<S: PluginState + Clone + 'static> HandlerRegistry<S> {
    /// Create a new empty registry.
    pub(crate) fn new() -> Self {
        Self {
            table: HandlerTable::empty(),
            _phantom: PhantomData,
        }
    }

    /// Consume the registry and produce a type-erased [`HandlerTable`].
    pub(crate) fn into_table(self) -> HandlerTable {
        self.table
    }
}

// =============================================================================
// KeyMapBuilder — fluent API for declaring key maps
// =============================================================================

type GroupPredicate<S> = Box<dyn Fn(&S) -> bool + Send + Sync>;
type ActionHandler<S> = Box<dyn Fn(&S, &KeyEvent, &AppView<'_>) -> (S, KeyResponse) + Send + Sync>;

/// Builder for constructing a [`CompiledKeyMap`] with type-safe state access.
pub struct KeyMapBuilder<S: PluginState> {
    groups: Vec<KeyGroupDef<S>>,
    chord_groups: Vec<ChordGroupDef>,
    pub(crate) group_predicates: Vec<GroupPredicate<S>>,
    pub(crate) actions: Vec<(&'static str, ActionHandler<S>)>,
}

struct KeyGroupDef<S> {
    name: &'static str,
    predicate: GroupPredicate<S>,
    bindings: Vec<KeyBinding>,
    chords: Vec<ChordBinding>,
}

struct ChordGroupDef {
    bindings: Vec<ChordBinding>,
}

impl<S: PluginState + Clone + 'static> KeyMapBuilder<S> {
    fn new() -> Self {
        Self {
            groups: Vec::new(),
            chord_groups: Vec::new(),
            group_predicates: Vec::new(),
            actions: Vec::new(),
        }
    }

    /// Define a key group that is active when the predicate returns true.
    ///
    /// Groups are evaluated in declaration order — first matching binding wins.
    pub fn group(
        &mut self,
        name: &'static str,
        when: impl Fn(&S) -> bool + Send + Sync + 'static,
        build: impl FnOnce(&mut KeyGroupConfig),
    ) {
        let mut cfg = KeyGroupConfig {
            bindings: Vec::new(),
            chords: Vec::new(),
        };
        build(&mut cfg);
        self.groups.push(KeyGroupDef {
            name,
            predicate: Box::new(when),
            bindings: cfg.bindings,
            chords: cfg.chords,
        });
    }

    /// Define chord bindings under a leader key.
    ///
    /// The chord group is always active (create it inside a `group()` for
    /// conditional activation).
    pub fn chord(&mut self, leader: KeyEvent, build: impl FnOnce(&mut ChordConfig)) {
        let mut cfg = ChordConfig {
            leader: leader.clone(),
            bindings: Vec::new(),
        };
        build(&mut cfg);
        self.chord_groups.push(ChordGroupDef {
            bindings: cfg.bindings,
        });
    }

    /// Register an action handler by ID.
    ///
    /// Action handlers receive the current state and the triggering key event,
    /// and return the updated state plus a [`KeyResponse`].
    pub fn action(
        &mut self,
        id: &'static str,
        handler: impl Fn(&S, &KeyEvent, &AppView<'_>) -> (S, KeyResponse) + Send + Sync + 'static,
    ) {
        self.actions.push((id, Box::new(handler)));
    }

    /// Build the initial [`CompiledKeyMap`] from the declared groups.
    fn build_compiled_map(&mut self) -> CompiledKeyMap {
        let mut groups = Vec::new();

        for def in &self.groups {
            let active = true; // will be refreshed on first frame
            groups.push(KeyGroup {
                name: def.name,
                active,
                bindings: def.bindings.clone(),
                chords: def.chords.clone(),
            });
        }

        // Merge standalone chord groups into their own always-active group.
        for chord_def in &self.chord_groups {
            groups.push(KeyGroup {
                name: "__chord__",
                active: true,
                bindings: Vec::new(),
                chords: chord_def.bindings.clone(),
            });
        }

        // Move predicates out for the refresh handler.
        self.group_predicates = self
            .groups
            .iter_mut()
            .map(|def| {
                // Replace with a dummy predicate; the real one is captured by the closure.
                std::mem::replace(&mut def.predicate, Box::new(|_| true))
            })
            .collect();
        // Always-active chord groups get constant `true` predicates.
        for _ in &self.chord_groups {
            self.group_predicates.push(Box::new(|_| true));
        }

        CompiledKeyMap {
            groups,
            ..Default::default()
        }
    }
}

/// Configuration for bindings within a key group.
pub struct KeyGroupConfig {
    bindings: Vec<KeyBinding>,
    chords: Vec<ChordBinding>,
}

impl KeyGroupConfig {
    /// Add a single-key binding.
    pub fn bind(&mut self, pattern: KeyPattern, action_id: &'static str) {
        self.bindings.push(KeyBinding { pattern, action_id });
    }

    /// Add a chord binding within this group.
    pub fn chord_bind(&mut self, leader: KeyEvent, follower: KeyPattern, action_id: &'static str) {
        self.chords.push(ChordBinding {
            leader,
            follower,
            action_id,
        });
    }
}

/// Configuration for chord bindings under a leader key.
pub struct ChordConfig {
    leader: KeyEvent,
    bindings: Vec<ChordBinding>,
}

impl ChordConfig {
    /// Add a follower binding under this chord's leader.
    pub fn bind(&mut self, follower: KeyPattern, action_id: &'static str) {
        self.bindings.push(ChordBinding {
            leader: self.leader.clone(),
            follower,
            action_id,
        });
    }
}

#[cfg(test)]
mod tests;
