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
//!     r.on_state_changed(|state, app, dirty| {
//!         // ...
//!         (new_state, Effects::default())
//!     });
//!     r.on_decorate_background(|state, line, app, ctx| {
//!         // ...
//!         Some(BackgroundLayer { ... })
//!     });
//! }
//! ```

// Items used by `mod tests` below as well as KeyMapBuilder. The split-out
// axis modules each carry their own use-statements; this top-level set is
// intentionally broad so the `#[cfg(test)] mod tests` block (which uses
// `super::*` and exercises every on_* method) compiles without per-test
// imports. `#[allow(unused_imports)]` covers the gap between the lib-only
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
use super::extension_point::{
    CompositionRule, ExtensionContribution, ExtensionDefinition, ExtensionPointId,
};
#[allow(unused_imports)]
use super::handler_table::{
    ContributeEntry, GutterHandlerEntry, GutterSide, HandlerTable, TransformEntry,
};
use super::kakoune_safe_effects::KakouneSafeEffects;
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
    DisplayDirective, Effects, IoEvent, KakouneSafeCommand, OrnamentBatch, OverlayContext,
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

impl Transparency for KakouneSafeEffects {
    const IS_TRANSPARENT: bool = true;
}

impl Transparency for Command {
    const IS_TRANSPARENT: bool = false;
}

impl Transparency for KakouneSafeCommand {
    const IS_TRANSPARENT: bool = true;
}

impl Transparency for KeyHandleResult {
    const IS_TRANSPARENT: bool = false;
}

impl Transparency for super::KakouneSafeKeyResult {
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
mod tests {
    use super::*;
    use crate::plugin::PluginCapabilities;
    use crate::plugin::kakoune_safe_command::KakouneSafeCommand;
    use crate::plugin::traits::PluginBackend;
    use crate::state::DirtyFlags;

    #[derive(Clone, Debug, PartialEq, Hash, Default)]
    struct TestState {
        counter: u32,
    }

    #[test]
    fn empty_registry_has_no_capabilities() {
        let registry = HandlerRegistry::<TestState>::new();
        let table = registry.into_table();
        assert_eq!(table.capabilities(), PluginCapabilities::empty());
    }

    #[test]
    fn declare_interests() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.declare_interests(DirtyFlags::BUFFER);
        let table = registry.into_table();
        assert_eq!(table.interests(), DirtyFlags::BUFFER);
    }

    #[test]
    fn on_decorate_background_sets_annotator_capability() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_decorate_background(|_state, _line, _app, _ctx| None);
        let table = registry.into_table();
        assert!(table.capabilities().contains(PluginCapabilities::ANNOTATOR));
    }

    #[test]
    fn on_contribute_sets_contributor_capability() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_contribute(SlotId::STATUS_LEFT, |_state, _app, _ctx| None);
        let table = registry.into_table();
        assert!(
            table
                .capabilities()
                .contains(PluginCapabilities::CONTRIBUTOR)
        );
        assert_eq!(table.contribute_handlers.len(), 1);
        assert_eq!(table.contribute_handlers[0].slot, SlotId::STATUS_LEFT);
    }

    #[test]
    fn on_transform_sets_transformer_capability() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_transform(10, |_state, _target, _app, _ctx| ElementPatch::Identity);
        let table = registry.into_table();
        assert!(
            table
                .capabilities()
                .contains(PluginCapabilities::TRANSFORMER)
        );
        assert!(table.transform_handler.is_some());
        assert_eq!(table.transform_handler.as_ref().unwrap().priority, 10);
    }

    #[test]
    fn on_transform_has_empty_targets() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_transform(10, |_state, _target, _app, _ctx| ElementPatch::Identity);
        let table = registry.into_table();
        let desc = table.capability_descriptor();
        assert!(desc.transform_targets.is_empty());
    }

    #[test]
    fn on_transform_for_populates_targets() {
        use crate::plugin::context::TransformTarget;
        let mut registry = HandlerRegistry::<TestState>::new();
        let targets = [TransformTarget::BUFFER, TransformTarget::STATUS_BAR];
        registry.on_transform_for(5, &targets, |_state, _target, _app, _ctx| {
            ElementPatch::Identity
        });
        let table = registry.into_table();
        let desc = table.capability_descriptor();
        assert_eq!(desc.transform_targets.len(), 2);
        assert!(desc.transform_targets.contains(&TransformTarget::BUFFER));
        assert!(
            desc.transform_targets
                .contains(&TransformTarget::STATUS_BAR)
        );
    }

    #[test]
    fn on_transform_for_sets_priority() {
        use crate::plugin::context::TransformTarget;
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_transform_for(
            42,
            &[TransformTarget::MENU],
            |_state, _target, _app, _ctx| ElementPatch::Identity,
        );
        let table = registry.into_table();
        assert_eq!(table.transform_handler.as_ref().unwrap().priority, 42);
    }

    #[test]
    fn may_interfere_detects_transform_target_overlap() {
        use crate::plugin::context::TransformTarget;

        let mut r1 = HandlerRegistry::<TestState>::new();
        r1.on_transform_for(
            0,
            &[TransformTarget::BUFFER, TransformTarget::MENU],
            |_s, _t, _a, _c| ElementPatch::Identity,
        );
        let desc1 = r1.into_table().capability_descriptor();

        let mut r2 = HandlerRegistry::<TestState>::new();
        r2.on_transform_for(
            0,
            &[TransformTarget::MENU, TransformTarget::STATUS_BAR],
            |_s, _t, _a, _c| ElementPatch::Identity,
        );
        let desc2 = r2.into_table().capability_descriptor();

        // MENU overlaps
        assert!(desc1.may_interfere(&desc2));
    }

    #[test]
    fn may_interfere_no_overlap() {
        use crate::plugin::context::TransformTarget;

        let mut r1 = HandlerRegistry::<TestState>::new();
        r1.on_transform_for(0, &[TransformTarget::BUFFER], |_s, _t, _a, _c| {
            ElementPatch::Identity
        });
        let desc1 = r1.into_table().capability_descriptor();

        let mut r2 = HandlerRegistry::<TestState>::new();
        r2.on_transform_for(0, &[TransformTarget::MENU], |_s, _t, _a, _c| {
            ElementPatch::Identity
        });
        let desc2 = r2.into_table().capability_descriptor();

        assert!(!desc1.may_interfere(&desc2));
    }

    #[test]
    fn on_key_sets_input_handler_capability() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_key(|_state, _key, _app| None::<(TestState, Vec<Command>)>);
        let table = registry.into_table();
        assert!(
            table
                .capabilities()
                .contains(PluginCapabilities::INPUT_HANDLER)
        );
    }

    #[test]
    fn on_text_input_sets_input_handler_capability() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_text_input(|_state, _text, _app| None::<(TestState, Vec<Command>)>);
        let table = registry.into_table();
        assert!(
            table
                .capabilities()
                .contains(PluginCapabilities::INPUT_HANDLER)
        );
    }

    #[test]
    fn on_overlay_sets_overlay_capability() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_overlay(|_state, _app, _ctx| None);
        let table = registry.into_table();
        assert!(table.capabilities().contains(PluginCapabilities::OVERLAY));
    }

    #[test]
    fn on_display_sets_display_transform_capability() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_display(|_state, _app| vec![]);
        let table = registry.into_table();
        assert!(
            table
                .capabilities()
                .contains(PluginCapabilities::DISPLAY_TRANSFORM)
        );
    }

    #[test]
    fn on_render_ornaments_sets_capability() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_render_ornaments(|_state, _app, _ctx| OrnamentBatch::default());
        let table = registry.into_table();
        assert!(
            table
                .capabilities()
                .contains(PluginCapabilities::RENDER_ORNAMENT)
        );
    }

    #[test]
    fn on_paint_inline_box_sets_capability() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_paint_inline_box(|_state, _box_id, _app| None);
        let table = registry.into_table();
        assert!(
            table
                .capabilities()
                .contains(PluginCapabilities::INLINE_BOX_PAINTER)
        );
    }

    #[test]
    fn paint_inline_box_default_is_no_op() {
        // A registry with no inline-box-paint handler must not advertise
        // the capability (gating invariant — host can skip dispatch).
        let registry = HandlerRegistry::<TestState>::new();
        let table = registry.into_table();
        assert!(
            !table
                .capabilities()
                .contains(PluginCapabilities::INLINE_BOX_PAINTER)
        );
    }

    #[test]
    fn multiple_gutter_handlers() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_decorate_gutter(GutterSide::Left, 0, |_s, _l, _a, _c| None);
        registry.on_decorate_gutter(GutterSide::Right, 10, |_s, _l, _a, _c| None);
        let table = registry.into_table();
        assert_eq!(table.gutter_handlers.len(), 2);
        assert_eq!(table.gutter_handlers[0].side, GutterSide::Left);
        assert_eq!(table.gutter_handlers[0].priority, 0);
        assert_eq!(table.gutter_handlers[1].side, GutterSide::Right);
        assert_eq!(table.gutter_handlers[1].priority, 10);
    }

    #[test]
    fn multiple_contribute_handlers() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_contribute(SlotId::STATUS_LEFT, |_s, _a, _c| None);
        registry.on_contribute(SlotId::STATUS_RIGHT, |_s, _a, _c| None);
        let table = registry.into_table();
        assert_eq!(table.contribute_handlers.len(), 2);
    }

    #[test]
    fn combined_capabilities() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_decorate_background(|_s, _l, _a, _c| None);
        registry.on_overlay(|_s, _a, _c| None);
        registry.on_key(|_s, _k, _a| None::<(TestState, Vec<Command>)>);
        let table = registry.into_table();
        let caps = table.capabilities();
        assert!(caps.contains(PluginCapabilities::ANNOTATOR));
        assert!(caps.contains(PluginCapabilities::OVERLAY));
        assert!(caps.contains(PluginCapabilities::INPUT_HANDLER));
        assert!(!caps.contains(PluginCapabilities::TRANSFORMER));
    }

    #[test]
    fn has_annotation_handlers_with_background() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_decorate_background(|_s, _l, _a, _c| None);
        let table = registry.into_table();
        assert!(table.has_annotation_handlers());
    }

    #[test]
    fn has_annotation_handlers_with_gutter() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_decorate_gutter(GutterSide::Left, 0, |_s, _l, _a, _c| None);
        let table = registry.into_table();
        assert!(table.has_annotation_handlers());
    }

    #[test]
    fn handler_type_erasure_invocation() {
        // Verify that erased handlers can be invoked with the correct state type.
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_state_changed(|state, _app, _dirty| {
            let new_state = TestState {
                counter: state.counter + 1,
            };
            (new_state, Effects::default())
        });
        let table = registry.into_table();

        // Create a boxed state
        let _state: Box<dyn PluginState> = Box::new(TestState { counter: 5 });

        // We can't easily create an AppView in tests, but we can verify
        // the handler is stored and the type alias is correct.
        assert!(table.state_changed_handler.is_some());
    }

    #[test]
    fn on_navigation_policy_sets_capability() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_navigation_policy(|_state, _unit| NavigationPolicy::Normal);
        let table = registry.into_table();
        assert!(
            table
                .capabilities()
                .contains(PluginCapabilities::NAVIGATION_POLICY)
        );
    }

    #[test]
    fn on_navigation_action_sets_capability_and_updates_state() {
        use crate::display;
        use crate::plugin::PluginBridge;
        use crate::plugin::state::Plugin;

        #[derive(Clone, Debug, PartialEq, Hash, Default)]
        struct NavTestState {
            counter: u32,
        }
        struct NavTestPlugin;
        impl Plugin for NavTestPlugin {
            type State = NavTestState;
            fn id(&self) -> crate::plugin::PluginId {
                crate::plugin::PluginId("nav-test".into())
            }
            fn register(&self, r: &mut HandlerRegistry<NavTestState>) {
                r.on_navigation_action(|state, _unit, _action| {
                    (
                        NavTestState {
                            counter: state.counter + 1,
                        },
                        ActionResult::Handled,
                    )
                });
            }
        }

        let mut bridge = PluginBridge::new(NavTestPlugin);
        assert!(
            bridge
                .capabilities()
                .contains(PluginCapabilities::NAVIGATION_ACTION)
        );

        let unit = display::unit::DisplayUnit {
            id: display::unit::DisplayUnitId::from_content(
                &display::unit::UnitSource::Line(0),
                &display::unit::SemanticRole::BufferContent,
            ),
            display_line: 0,
            role: display::unit::SemanticRole::BufferContent,
            source: display::unit::UnitSource::Line(0),
            interaction: display::InteractionPolicy::Normal,
        };
        let result = bridge.navigation_action(&unit, NavigationAction::None);
        assert_eq!(result, Some(ActionResult::Handled));
    }

    // =========================================================================
    // Transparent handler registration (ADR-030 Level 3)
    // =========================================================================

    #[test]
    fn on_key_transparent_sets_input_handler_and_transparency() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_key(
            |_state: &TestState, _key, _app| -> Option<(TestState, Vec<KakouneSafeCommand>)> {
                None
            },
        );
        assert!(registry.is_input_transparent());
        let table = registry.into_table();
        assert!(
            table
                .capabilities()
                .contains(PluginCapabilities::INPUT_HANDLER)
        );
        assert!(table.transparency.key_handler);
    }

    #[test]
    fn on_key_non_transparent_means_not_input_transparent() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_key(|_state, _key, _app| None::<(TestState, Vec<Command>)>);
        assert!(!registry.is_input_transparent());
    }

    #[test]
    fn mixed_transparent_and_non_transparent_is_not_transparent() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_key(
            |_state: &TestState, _key, _app| -> Option<(TestState, Vec<KakouneSafeCommand>)> {
                None
            },
        );
        registry.on_text_input(|_state, _text, _app| None::<(TestState, Vec<Command>)>);
        assert!(!registry.is_input_transparent());
    }

    #[test]
    fn all_transparent_handlers_means_input_transparent() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_key(
            |_state: &TestState, _key, _app| -> Option<(TestState, Vec<KakouneSafeCommand>)> {
                None
            },
        );
        registry.on_text_input(
            |_state: &TestState, _text, _app| -> Option<(TestState, Vec<KakouneSafeCommand>)> {
                None
            },
        );
        assert!(registry.is_input_transparent());
    }

    #[test]
    fn no_handlers_is_input_transparent() {
        let registry = HandlerRegistry::<TestState>::new();
        assert!(registry.is_input_transparent());
    }

    // =========================================================================
    // Unified display handler tests (Phase 1B.2)
    // =========================================================================

    #[test]
    fn on_display_unified_sets_display_transform_capability() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_display_unified(|_state, _app| vec![]);
        let table = registry.into_table();
        assert!(
            table
                .capabilities()
                .contains(PluginCapabilities::DISPLAY_TRANSFORM)
        );
    }

    #[test]
    fn on_display_unified_sets_annotator_capability() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_display_unified(|_state, _app| vec![]);
        let table = registry.into_table();
        assert!(table.capabilities().contains(PluginCapabilities::ANNOTATOR));
    }

    #[test]
    fn on_display_unified_sets_content_annotator_capability() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_display_unified(|_state, _app| vec![]);
        let table = registry.into_table();
        assert!(
            table
                .capabilities()
                .contains(PluginCapabilities::CONTENT_ANNOTATOR)
        );
    }

    #[test]
    fn on_display_unified_safe_is_recoverable() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_display_unified_safe(|_state, _app| vec![]);
        assert!(registry.is_display_recoverable());
    }

    #[test]
    fn on_display_unified_is_not_recoverable() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_display_unified(|_state, _app| vec![]);
        assert!(!registry.is_display_recoverable());
    }
}
