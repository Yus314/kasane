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
//!         (new_state, RuntimeEffects::default())
//!     });
//!     r.on_annotate_background(|state, line, app, ctx| {
//!         // ...
//!         Some(BackgroundLayer { ... })
//!     });
//! }
//! ```

use std::any::Any;
use std::marker::PhantomData;

use crate::element::{Element, InteractiveId};
use crate::input::{KeyEvent, MouseEvent};
use crate::protocol::Atom;
use crate::render::{CursorStyleHint, InlineDecoration};
use crate::scroll::{DefaultScrollCandidate, ScrollPolicyResult};
use crate::state::DirtyFlags;
use crate::workspace::WorkspaceQuery;

use super::element_patch::ElementPatch;
use super::extension_point::{
    CompositionRule, ExtensionContribution, ExtensionDefinition, ExtensionPointId,
};
use super::handler_table::{
    ContributeEntry, GutterHandlerEntry, GutterSide, HandlerTable, TransformEntry,
};
use super::pubsub::{PublishEntry, SubscribeEntry, TopicId};
use super::traits::KeyHandleResult;
use super::{
    AnnotateContext, AppView, BackgroundLayer, BootstrapEffects, CellDecoration, Command,
    ContributeContext, Contribution, DisplayDirective, IoEvent, OverlayContext,
    OverlayContribution, PluginState, RuntimeEffects, SessionReadyEffects, SlotId,
    TransformContext, TransformTarget, VirtualTextItem,
};

/// Type-safe handler registration builder.
///
/// `S` is the plugin's concrete state type. Registration methods accept closures
/// over `&S` and automatically infer [`PluginCapabilities`] from which handlers
/// are registered.
pub struct HandlerRegistry<S: PluginState> {
    table: HandlerTable,
    _phantom: PhantomData<S>,
}

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

    // =========================================================================
    // Configuration
    // =========================================================================

    /// Declare which [`DirtyFlags`] this plugin's view methods depend on.
    ///
    /// When no declared flags are dirty and the plugin's state hasn't changed,
    /// the framework can skip re-collecting this plugin's contributions.
    /// Default: `DirtyFlags::ALL` (always re-collect).
    pub fn declare_interests(&mut self, flags: DirtyFlags) {
        self.table.interests = flags;
    }

    // =========================================================================
    // Lifecycle handlers
    // =========================================================================

    /// Register an initialization handler.
    pub fn on_init(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>) -> (S, BootstrapEffects) + Send + Sync + 'static,
    ) {
        self.table.init_handler = Some(Box::new(move |state, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            let (new_state, effects) = handler(s, app);
            (Box::new(new_state) as Box<dyn PluginState>, effects)
        }));
    }

    /// Register a session-ready handler.
    pub fn on_session_ready(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>) -> (S, SessionReadyEffects) + Send + Sync + 'static,
    ) {
        self.table.session_ready_handler = Some(Box::new(move |state, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            let (new_state, effects) = handler(s, app);
            (Box::new(new_state) as Box<dyn PluginState>, effects)
        }));
    }

    /// Register a state-changed handler.
    pub fn on_state_changed(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>, DirtyFlags) -> (S, RuntimeEffects) + Send + Sync + 'static,
    ) {
        self.table.state_changed_handler = Some(Box::new(move |state, app, dirty| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            let (new_state, effects) = handler(s, app, dirty);
            (Box::new(new_state) as Box<dyn PluginState>, effects)
        }));
    }

    /// Register an I/O event handler.
    pub fn on_io_event(
        &mut self,
        handler: impl Fn(&S, &IoEvent, &AppView<'_>) -> (S, RuntimeEffects) + Send + Sync + 'static,
    ) {
        self.table.io_event_handler = Some(Box::new(move |state, event, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            let (new_state, effects) = handler(s, event, app);
            (Box::new(new_state) as Box<dyn PluginState>, effects)
        }));
    }

    /// Register a workspace-changed handler.
    pub fn on_workspace_changed(
        &mut self,
        handler: impl Fn(&S, &WorkspaceQuery<'_>) -> S + Send + Sync + 'static,
    ) {
        self.table.workspace_changed_handler = Some(Box::new(move |state, query| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            Box::new(handler(s, query)) as Box<dyn PluginState>
        }));
    }

    /// Register a shutdown handler.
    pub fn on_shutdown(&mut self, handler: impl Fn(&S) + Send + Sync + 'static) {
        self.table.shutdown_handler = Some(Box::new(move |state| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            handler(s);
        }));
    }

    /// Register an update (message) handler.
    pub fn on_update(
        &mut self,
        handler: impl Fn(&S, &mut dyn Any, &AppView<'_>) -> (S, RuntimeEffects) + Send + Sync + 'static,
    ) {
        self.table.update_handler = Some(Box::new(move |state, msg, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            let (new_state, effects) = handler(s, msg, app);
            (Box::new(new_state) as Box<dyn PluginState>, effects)
        }));
    }

    // =========================================================================
    // Input handlers
    // =========================================================================

    /// Register a key handler (consumes keys, returns commands).
    pub fn on_key(
        &mut self,
        handler: impl Fn(&S, &KeyEvent, &AppView<'_>) -> Option<(S, Vec<Command>)>
        + Send
        + Sync
        + 'static,
    ) {
        self.table.key_handler = Some(Box::new(move |state, key, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            handler(s, key, app)
                .map(|(new_state, cmds)| (Box::new(new_state) as Box<dyn PluginState>, cmds))
        }));
    }

    /// Register a key middleware handler.
    pub fn on_key_middleware(
        &mut self,
        handler: impl Fn(&S, &KeyEvent, &AppView<'_>) -> (S, KeyHandleResult) + Send + Sync + 'static,
    ) {
        self.table.key_middleware_handler = Some(Box::new(move |state, key, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            let (new_state, result) = handler(s, key, app);
            (Box::new(new_state) as Box<dyn PluginState>, result)
        }));
    }

    /// Register a key observer (notification only, cannot consume).
    pub fn on_observe_key(
        &mut self,
        handler: impl Fn(&S, &KeyEvent, &AppView<'_>) -> S + Send + Sync + 'static,
    ) {
        self.table.observe_key_handler = Some(Box::new(move |state, key, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            Box::new(handler(s, key, app)) as Box<dyn PluginState>
        }));
    }

    /// Register a mouse observer (notification only, cannot consume).
    pub fn on_observe_mouse(
        &mut self,
        handler: impl Fn(&S, &MouseEvent, &AppView<'_>) -> S + Send + Sync + 'static,
    ) {
        self.table.observe_mouse_handler = Some(Box::new(move |state, event, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            Box::new(handler(s, event, app)) as Box<dyn PluginState>
        }));
    }

    /// Register a mouse handler (interactive element click).
    pub fn on_handle_mouse(
        &mut self,
        handler: impl Fn(&S, &MouseEvent, InteractiveId, &AppView<'_>) -> Option<(S, Vec<Command>)>
        + Send
        + Sync
        + 'static,
    ) {
        self.table.handle_mouse_handler = Some(Box::new(move |state, event, id, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            handler(s, event, id, app)
                .map(|(new_state, cmds)| (Box::new(new_state) as Box<dyn PluginState>, cmds))
        }));
    }

    /// Register a default scroll policy handler.
    pub fn on_default_scroll(
        &mut self,
        handler: impl Fn(&S, DefaultScrollCandidate, &AppView<'_>) -> Option<(S, ScrollPolicyResult)>
        + Send
        + Sync
        + 'static,
    ) {
        self.table.default_scroll_handler = Some(Box::new(move |state, candidate, app| {
            let s = state
                .as_any()
                .downcast_ref::<S>()
                .expect("state type mismatch");
            handler(s, candidate, app)
                .map(|(new_state, result)| (Box::new(new_state) as Box<dyn PluginState>, result))
        }));
    }

    // =========================================================================
    // View handlers
    // =========================================================================

    /// Register a contribution handler for a specific slot.
    pub fn on_contribute(
        &mut self,
        slot: SlotId,
        handler: impl Fn(&S, &AppView<'_>, &ContributeContext) -> Option<Contribution>
        + Send
        + Sync
        + 'static,
    ) {
        let erased = Box::new(
            move |state: &dyn PluginState,
                  app: &AppView<'_>,
                  ctx: &ContributeContext|
                  -> Option<Contribution> {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                handler(s, app, ctx)
            },
        );
        self.table.contribute_handlers.push(ContributeEntry {
            slot,
            handler: erased,
        });
    }

    /// Register a transform handler with priority.
    ///
    /// The handler returns an [`ElementPatch`] describing the declarative transform.
    /// Higher priority = applied earlier (inner position in the chain).
    pub fn on_transform(
        &mut self,
        priority: i16,
        handler: impl Fn(&S, &TransformTarget, &AppView<'_>, &TransformContext) -> ElementPatch
        + Send
        + Sync
        + 'static,
    ) {
        let erased = Box::new(
            move |state: &dyn PluginState,
                  target: &TransformTarget,
                  app: &AppView<'_>,
                  ctx: &TransformContext|
                  -> ElementPatch {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                handler(s, target, app, ctx)
            },
        );
        self.table.transform_handler = Some(TransformEntry {
            priority,
            handler: erased,
        });
    }

    /// Register a gutter annotation handler.
    ///
    /// `side` determines left or right gutter placement. `priority` controls
    /// sort ordering (lower = further left within the same side).
    pub fn on_annotate_gutter(
        &mut self,
        side: GutterSide,
        priority: i16,
        handler: impl Fn(&S, usize, &AppView<'_>, &AnnotateContext) -> Option<Element>
        + Send
        + Sync
        + 'static,
    ) {
        let erased = Box::new(
            move |state: &dyn PluginState,
                  line: usize,
                  app: &AppView<'_>,
                  ctx: &AnnotateContext|
                  -> Option<Element> {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                handler(s, line, app, ctx)
            },
        );
        self.table.gutter_handlers.push(GutterHandlerEntry {
            side,
            priority,
            handler: erased,
        });
    }

    /// Register a background annotation handler.
    pub fn on_annotate_background(
        &mut self,
        handler: impl Fn(&S, usize, &AppView<'_>, &AnnotateContext) -> Option<BackgroundLayer>
        + Send
        + Sync
        + 'static,
    ) {
        self.table.background_handler = Some(Box::new(
            move |state: &dyn PluginState,
                  line: usize,
                  app: &AppView<'_>,
                  ctx: &AnnotateContext|
                  -> Option<BackgroundLayer> {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                handler(s, line, app, ctx)
            },
        ));
    }

    /// Register an inline decoration handler.
    pub fn on_annotate_inline(
        &mut self,
        handler: impl Fn(&S, usize, &AppView<'_>, &AnnotateContext) -> Option<InlineDecoration>
        + Send
        + Sync
        + 'static,
    ) {
        self.table.inline_handler = Some(Box::new(
            move |state: &dyn PluginState,
                  line: usize,
                  app: &AppView<'_>,
                  ctx: &AnnotateContext|
                  -> Option<InlineDecoration> {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                handler(s, line, app, ctx)
            },
        ));
    }

    /// Register a virtual text handler.
    pub fn on_virtual_text(
        &mut self,
        handler: impl Fn(&S, usize, &AppView<'_>, &AnnotateContext) -> Vec<VirtualTextItem>
        + Send
        + Sync
        + 'static,
    ) {
        self.table.virtual_text_handler = Some(Box::new(
            move |state: &dyn PluginState,
                  line: usize,
                  app: &AppView<'_>,
                  ctx: &AnnotateContext|
                  -> Vec<VirtualTextItem> {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                handler(s, line, app, ctx)
            },
        ));
    }

    /// Register an overlay contribution handler.
    pub fn on_overlay(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>, &OverlayContext) -> Option<OverlayContribution>
        + Send
        + Sync
        + 'static,
    ) {
        self.table.overlay_handler = Some(Box::new(
            move |state: &dyn PluginState,
                  app: &AppView<'_>,
                  ctx: &OverlayContext|
                  -> Option<OverlayContribution> {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                handler(s, app, ctx)
            },
        ));
    }

    /// Register a display directive handler.
    pub fn on_display(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>) -> Vec<DisplayDirective> + Send + Sync + 'static,
    ) {
        self.table.display_handler = Some(Box::new(
            move |state: &dyn PluginState, app: &AppView<'_>| -> Vec<DisplayDirective> {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                handler(s, app)
            },
        ));
    }

    /// Register a cell decoration handler.
    pub fn on_cell_decoration(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>) -> Vec<CellDecoration> + Send + Sync + 'static,
    ) {
        self.table.cell_decoration_handler = Some(Box::new(
            move |state: &dyn PluginState, app: &AppView<'_>| -> Vec<CellDecoration> {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                handler(s, app)
            },
        ));
    }

    /// Register a cursor style override handler.
    pub fn on_cursor_style(
        &mut self,
        handler: impl Fn(&S, &AppView<'_>) -> Option<CursorStyleHint> + Send + Sync + 'static,
    ) {
        self.table.cursor_style_handler = Some(Box::new(
            move |state: &dyn PluginState, app: &AppView<'_>| -> Option<CursorStyleHint> {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                handler(s, app)
            },
        ));
    }

    /// Register a menu item transform handler.
    pub fn on_menu_transform(
        &mut self,
        handler: impl Fn(&S, &[Atom], usize, bool, &AppView<'_>) -> Option<Vec<Atom>>
        + Send
        + Sync
        + 'static,
    ) {
        self.table.menu_transform_handler = Some(Box::new(
            move |state: &dyn PluginState,
                  item: &[Atom],
                  index: usize,
                  selected: bool,
                  app: &AppView<'_>|
                  -> Option<Vec<Atom>> {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                handler(s, item, index, selected, app)
            },
        ));
    }

    // =========================================================================
    // Pub/Sub handlers
    // =========================================================================

    /// Publish a typed value on a topic each frame.
    ///
    /// The handler is called during the publication collection phase. Its return
    /// value is delivered to all subscribers of the same topic.
    ///
    /// ```ignore
    /// r.publish::<u32>(TopicId::new("cursor.line"), |state, _app| state.cursor_line);
    /// ```
    pub fn publish<T: Send + 'static>(
        &mut self,
        topic: TopicId,
        handler: impl Fn(&S, &AppView<'_>) -> T + Send + Sync + 'static,
    ) {
        self.table.publishers.push(PublishEntry {
            topic,
            handler: Box::new(move |state: &dyn PluginState, app: &AppView<'_>| {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                Box::new(handler(s, app)) as Box<dyn std::any::Any + Send>
            }),
        });
    }

    /// Subscribe to a typed topic published by another plugin.
    ///
    /// The handler receives each published value and returns the updated state.
    /// Called during the delivery phase, after all publications are collected.
    ///
    /// ```ignore
    /// r.subscribe::<u32>(TopicId::new("cursor.line"), |state, value| {
    ///     MyState { tracked_line: *value, ..state.clone() }
    /// });
    /// ```
    pub fn subscribe<T: 'static>(
        &mut self,
        topic: TopicId,
        handler: impl Fn(&S, &T) -> S + Send + Sync + 'static,
    ) {
        self.table.subscribers.push(SubscribeEntry {
            topic,
            handler: Box::new(
                move |state: &dyn PluginState, value: &dyn std::any::Any| -> Box<dyn PluginState> {
                    let s = state
                        .as_any()
                        .downcast_ref::<S>()
                        .expect("state type mismatch");
                    let v = value.downcast_ref::<T>().expect("topic type mismatch");
                    Box::new(handler(s, v))
                },
            ),
        });
    }

    // =========================================================================
    // Extension Point handlers
    // =========================================================================

    /// Define a custom extension point that other plugins can contribute to.
    ///
    /// The `rule` determines how multiple contributions are composed.
    ///
    /// ```ignore
    /// r.define_extension::<(), Vec<StatusItem>>(
    ///     ExtensionPointId::new("myplugin.status-items"),
    ///     CompositionRule::Merge,
    /// );
    /// ```
    pub fn define_extension<I: Send + 'static, O: Send + 'static>(
        &mut self,
        id: ExtensionPointId,
        rule: CompositionRule,
    ) {
        self.table.extension_definitions.push(ExtensionDefinition {
            id,
            rule,
            handler: None,
        });
    }

    /// Define a custom extension point and also contribute a handler for it.
    pub fn define_extension_with_handler<I: Send + 'static, O: Send + 'static>(
        &mut self,
        id: ExtensionPointId,
        rule: CompositionRule,
        handler: impl Fn(&S, &I, &AppView<'_>) -> O + Send + Sync + 'static,
    ) {
        self.table.extension_definitions.push(ExtensionDefinition {
            id,
            rule,
            handler: Some(Box::new(
                move |state: &dyn PluginState,
                      input: &dyn std::any::Any,
                      app: &AppView<'_>|
                      -> Box<dyn std::any::Any + Send> {
                    let s = state
                        .as_any()
                        .downcast_ref::<S>()
                        .expect("state type mismatch");
                    let i = input
                        .downcast_ref::<I>()
                        .expect("extension input type mismatch");
                    Box::new(handler(s, i, app))
                },
            )),
        });
    }

    /// Contribute to an extension point defined by another plugin.
    ///
    /// ```ignore
    /// r.on_extension::<(), Vec<StatusItem>>(
    ///     ExtensionPointId::new("myplugin.status-items"),
    ///     |_state, _input, _app| vec![StatusItem { text: "hello" }],
    /// );
    /// ```
    pub fn on_extension<I: Send + 'static, O: Send + 'static>(
        &mut self,
        id: ExtensionPointId,
        handler: impl Fn(&S, &I, &AppView<'_>) -> O + Send + Sync + 'static,
    ) {
        self.table
            .extension_contributions
            .push(ExtensionContribution {
                id,
                handler: Box::new(
                    move |state: &dyn PluginState,
                          input: &dyn std::any::Any,
                          app: &AppView<'_>|
                          -> Box<dyn std::any::Any + Send> {
                        let s = state
                            .as_any()
                            .downcast_ref::<S>()
                            .expect("state type mismatch");
                        let i = input
                            .downcast_ref::<I>()
                            .expect("extension input type mismatch");
                        Box::new(handler(s, i, app))
                    },
                ),
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::PluginCapabilities;
    use crate::state::DirtyFlags;

    #[derive(Clone, Debug, PartialEq, Default)]
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
    fn on_annotate_background_sets_annotator_capability() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_annotate_background(|_state, _line, _app, _ctx| None);
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
    fn on_key_sets_input_handler_capability() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_key(|_state, _key, _app| None);
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
    fn on_cursor_style_sets_capability() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_cursor_style(|_state, _app| None);
        let table = registry.into_table();
        assert!(
            table
                .capabilities()
                .contains(PluginCapabilities::CURSOR_STYLE)
        );
    }

    #[test]
    fn multiple_gutter_handlers() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_annotate_gutter(GutterSide::Left, 0, |_s, _l, _a, _c| None);
        registry.on_annotate_gutter(GutterSide::Right, 10, |_s, _l, _a, _c| None);
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
        registry.on_annotate_background(|_s, _l, _a, _c| None);
        registry.on_overlay(|_s, _a, _c| None);
        registry.on_key(|_s, _k, _a| None);
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
        registry.on_annotate_background(|_s, _l, _a, _c| None);
        let table = registry.into_table();
        assert!(table.has_annotation_handlers());
    }

    #[test]
    fn has_annotation_handlers_with_gutter() {
        let mut registry = HandlerRegistry::<TestState>::new();
        registry.on_annotate_gutter(GutterSide::Left, 0, |_s, _l, _a, _c| None);
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
            (new_state, RuntimeEffects::default())
        });
        let table = registry.into_table();

        // Create a boxed state
        let _state: Box<dyn PluginState> = Box::new(TestState { counter: 5 });

        // We can't easily create an AppView in tests, but we can verify
        // the handler is stored and the type alias is correct.
        assert!(table.state_changed_handler.is_some());
    }
}
