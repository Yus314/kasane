//! Pub/sub typed wrappers — the only carve-outs on this axis.
//!
//! γ-3.3c-5b: the redundant manual `on_navigation_policy` /
//! `on_navigation_action` / `on_virtual_edit` / `on_buffer_edit_intercept` /
//! `on_paint_inline_box` setters were retired — plugin code now invokes
//! the macro-generated counterparts via `Deref` from `HandlerRegistry`
//! to `gen::HandlerRegistry`. The `publish` / `subscribe` family stays
//! manual because each setter wraps the underlying generated `on_publish`
//! / `on_subscribe` with `T: Serialize` / `T: DeserializeOwned`
//! conversion through `ChannelValue` — a typed-API ergonomic carve-out.

use serde::{Serialize, de::DeserializeOwned};

use super::super::channel::ChannelValue;
use super::super::pubsub::{PublishEntry, SubscribeEntry, Topic, TopicId};
use super::super::{AppView, PluginState};

use super::HandlerRegistry;

impl<S: PluginState + Clone + 'static> HandlerRegistry<S> {
    // =========================================================================
    // Pub/Sub handlers (typed serialization wrappers)
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
        self.table.publish_handlers.push(PublishEntry {
            key: topic,
            handler: Box::new(move |state: &dyn PluginState, app: &AppView<'_>| {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                let value = handler(s, app);
                Some(ChannelValue::new(&value).expect("publish serialization failed"))
            }),
        });
    }

    /// Publish a pre-formed [`ChannelValue`] on a topic, with `Option`
    /// semantics so the handler can opt out per frame.
    ///
    /// Counterpart to [`Self::publish`] for adapters that produce
    /// already-encoded values — primarily WASM plugins via the
    /// `publish-value(topic) -> option<channel-value>` WIT export.
    pub fn publish_raw(
        &mut self,
        topic: TopicId,
        handler: impl Fn(&S, &AppView<'_>) -> Option<ChannelValue> + Send + Sync + 'static,
    ) {
        self.table.publish_handlers.push(PublishEntry {
            key: topic,
            handler: Box::new(move |state: &dyn PluginState, app: &AppView<'_>| {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                handler(s, app)
            }),
        });
    }

    /// Register interest in a topic without a per-value handler.
    ///
    /// Counterpart to [`Self::subscribe`] for adapters whose contract
    /// dispatches the full value batch in a single call (and so does
    /// not need a per-value mutation hook) — primarily WASM plugins
    /// that pair this with [`Self::on_subscription`] for the per-topic
    /// batch dispatch via the WIT `on-subscription(topic, values)`
    /// export. The per-value handler is a no-op clone so the existing
    /// `deliver_subscriptions` per-value loop stays well-defined.
    pub fn subscribe_raw(&mut self, topic: TopicId) {
        self.table.subscribe_handlers.push(SubscribeEntry {
            key: topic,
            handler: Box::new(
                move |state: &dyn PluginState, _value: &ChannelValue| -> Box<dyn PluginState> {
                    let s = state
                        .as_any()
                        .downcast_ref::<S>()
                        .expect("state type mismatch");
                    Box::new(s.clone()) as Box<dyn PluginState>
                },
            ),
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
        self.table.subscribe_handlers.push(SubscribeEntry {
            key: topic,
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
        self.table.publish_handlers.push(PublishEntry {
            key: topic.id().clone(),
            handler: Box::new(move |state: &dyn PluginState, app: &AppView<'_>| {
                let s = state
                    .as_any()
                    .downcast_ref::<S>()
                    .expect("state type mismatch");
                let value = handler(s, app);
                Some(ChannelValue::new(&value).expect("publish serialization failed"))
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
}
