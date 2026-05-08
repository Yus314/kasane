//! Navigation, virtual-edit, buffer-edit intercept, paint-inline-box, and extension-point handlers.

use serde::{Serialize, de::DeserializeOwned};

use crate::display::navigation::{ActionResult, NavigationAction, NavigationPolicy};
use crate::display::unit::DisplayUnit;
use crate::element::Element;

use super::super::channel::ChannelValue;
use super::super::extension_point::{
    CompositionRule, ExtensionContribution, ExtensionDefinition, ExtensionPointId,
};
use super::super::pubsub::{PublishEntry, SubscribeEntry, Topic, TopicId};
use super::super::{AppView, Command, PluginState};

use super::{HandlerRegistry, VirtualEditContext};

impl<S: PluginState + Clone + 'static> HandlerRegistry<S> {
    pub fn on_navigation_policy(
        &mut self,
        handler: impl Fn(&S, &DisplayUnit) -> NavigationPolicy + Send + Sync + 'static,
    ) {
        register_view!(self, navigation_policy_handler, handler, unit);
    }

    /// Register a navigation action handler for display units.
    ///
    /// Called when a `Boundary` unit is activated (click or keyboard).
    /// Returns `(new_state, ActionResult)` following the functional-update model.
    /// FirstWins composition: the first non-Pass result wins.
    pub fn on_navigation_action(
        &mut self,
        handler: impl Fn(&S, &DisplayUnit, NavigationAction) -> (S, ActionResult)
        + Send
        + Sync
        + 'static,
    ) {
        register_state_effect!(self, navigation_action_handler, handler, unit, action);
    }

    // =========================================================================
    // Virtual edit handlers (BDT)
    // =========================================================================

    /// Register a virtual edit handler for editable virtual text.
    ///
    /// Called when a shadow cursor edit is committed on a `PluginDefined`
    /// projection. The handler receives the edit context and returns
    /// `(new_state, Vec<Command>)` with the commands to apply.
    pub fn on_virtual_edit(
        &mut self,
        handler: impl Fn(&S, &VirtualEditContext, &AppView<'_>) -> (S, Vec<Command>)
        + Send
        + Sync
        + 'static,
    ) {
        register_state_effect!(self, virtual_edit_handler, handler, ctx, app);
    }

    /// Register a buffer-edit intercept handler (ADR-035 ShadowCursor
    /// follow-up).
    ///
    /// Invoked by the dispatch loop after the builtin shadow cursor
    /// computes a Mirror-projection commit but before serializing it
    /// to Kakoune `exec -draft` commands. Plugins return a
    /// [`BufferEditVerdict`](crate::state::shadow_cursor::BufferEditVerdict):
    ///
    /// - `PassThrough` — observe without changing the edit (typical
    ///   for logging plugins).
    /// - `Replace(BufferEdit)` — substitute a transformed edit (e.g.
    ///   snap indentation, run an auto-formatter).
    /// - `Veto` — drop the commit entirely (no Kakoune commands
    ///   emitted; the shadow cursor still deactivates).
    ///
    /// Multiple plugins compose: verdicts fold in plugin-priority
    /// order, with `Veto` short-circuiting. Plugins that don't
    /// register an intercept default to PassThrough.
    pub fn on_buffer_edit_intercept(
        &mut self,
        handler: impl Fn(
            &S,
            &crate::state::shadow_cursor::BufferEdit,
            &AppView<'_>,
        ) -> (S, crate::state::shadow_cursor::BufferEditVerdict)
        + Send
        + Sync
        + 'static,
    ) {
        register_state_effect!(self, buffer_edit_intercept_handler, handler, edit, app);
    }

    // =========================================================================
    // Inline-box paint handler (ADR-031 Phase 10 Step 2-native)
    // =========================================================================

    /// Register an inline-box paint handler.
    ///
    /// Called by the renderer when a `DisplayDirective::InlineBox` slot
    /// owned by this plugin needs paint content. The handler receives the
    /// `box_id` declared in the directive and returns either an Element
    /// to paint inside the slot, or `None` to leave the slot empty (the
    /// renderer falls back to the placeholder reservation).
    ///
    /// Auto-registers `PluginCapabilities::INLINE_BOX_PAINTER`. Step 2-host
    /// will wire the renderer to invoke this; until then registration is
    /// inert.
    pub fn on_paint_inline_box(
        &mut self,
        handler: impl Fn(&S, u64, &AppView<'_>) -> Option<Element> + Send + Sync + 'static,
    ) {
        register_view!(self, inline_box_paint_handler, handler, box_id, app);
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
    pub fn publish<T: Serialize + Send + 'static>(
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
                let value = handler(s, app);
                ChannelValue::new(&value).expect("publish serialization failed")
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
    pub fn subscribe<T: DeserializeOwned + 'static>(
        &mut self,
        topic: TopicId,
        handler: impl Fn(&S, &T) -> S + Send + Sync + 'static,
    ) {
        self.table.subscribers.push(SubscribeEntry {
            topic,
            handler: Box::new(
                move |state: &dyn PluginState, value: &ChannelValue| -> Box<dyn PluginState> {
                    let s = state
                        .as_any()
                        .downcast_ref::<S>()
                        .expect("state type mismatch");
                    let v: T = value
                        .deserialize()
                        .expect("subscribe deserialization failed");
                    Box::new(handler(s, &v))
                },
            ),
        });
    }

    /// Publish a typed value on a topic, returning a [`Topic<T>`] handle.
    ///
    /// The returned handle carries the type parameter `T` at compile time,
    /// ensuring that [`subscribe_typed`](Self::subscribe_typed) callers use
    /// the correct type. Untyped [`publish`](Self::publish) / [`subscribe`](Self::subscribe)
    /// remain for WASM cross-boundary interop.
    ///
    /// ```ignore
    /// let topic: Topic<u32> = r.publish_typed("cursor.line", |s, _| s.line);
    /// r.subscribe_typed(&topic, |state, value: &u32| { ... });
    /// ```
    pub fn publish_typed<T: Serialize + Send + 'static>(
        &mut self,
        name: impl Into<compact_str::CompactString>,
        handler: impl Fn(&S, &AppView<'_>) -> T + Send + Sync + 'static,
    ) -> Topic<T> {
        let topic = Topic::<T>::new(name);
        self.table.publishers.push(PublishEntry {
            topic: topic.id().clone(),
            handler: Box::new(move |state: &dyn PluginState, app: &AppView<'_>| {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                let value = handler(s, app);
                ChannelValue::new(&value).expect("publish serialization failed")
            }),
        });
        topic
    }

    /// Subscribe to a [`Topic<T>`] handle with compile-time type safety.
    ///
    /// The `T` parameter is enforced by the `Topic<T>` handle, so no
    /// turbofish is needed and type mismatches are caught at compile time.
    pub fn subscribe_typed<T: DeserializeOwned + 'static>(
        &mut self,
        topic: &Topic<T>,
        handler: impl Fn(&S, &T) -> S + Send + Sync + 'static,
    ) {
        self.subscribe::<T>(topic.id().clone(), handler);
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
    pub fn define_extension_with_handler<
        I: DeserializeOwned + Send + 'static,
        O: Serialize + Send + 'static,
    >(
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
                      input: &ChannelValue,
                      app: &AppView<'_>|
                      -> ChannelValue {
                    let s = state
                        .as_any()
                        .downcast_ref::<S>()
                        .expect("state type mismatch");
                    let i: I = input
                        .deserialize()
                        .expect("extension input deserialization failed");
                    let output = handler(s, &i, app);
                    ChannelValue::new(&output).expect("extension output serialization failed")
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
    pub fn on_extension<I: DeserializeOwned + Send + 'static, O: Serialize + Send + 'static>(
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
                          input: &ChannelValue,
                          app: &AppView<'_>|
                          -> ChannelValue {
                        let s = state
                            .as_any()
                            .downcast_ref::<S>()
                            .expect("state type mismatch");
                        let i: I = input
                            .deserialize()
                            .expect("extension input deserialization failed");
                        let output = handler(s, &i, app);
                        ChannelValue::new(&output).expect("extension output serialization failed")
                    },
                ),
            });
    }
}
