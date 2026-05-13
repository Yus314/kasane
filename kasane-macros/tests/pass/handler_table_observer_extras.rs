// γ-3.2.2d-1: Observer void + Observer per_slot codegen.
//
// Confirms:
// - `void` modifier on Observer drops the state-return — erased alias has
//   no return type, setter accepts `Fn(&S, args...)` (no return), wrapper
//   does not box.
// - `per_slot=K` on Observer follows the same Vec<<Name>Entry> storage
//   pattern as View per_slot but with the state-boxing wrapper from the
//   base Observer setter.

use kasane_macros::handler_table;

handler_table! {
    pub mod spec {
        use kasane_core::plugin::{ChannelValue, PluginState};
        use kasane_core::plugin::pubsub::TopicId;

        // Observer void: shutdown pattern, no state return.
        handler shutdown(): Observer(void);

        // Observer per_slot: subscribe per-topic per-value handler pattern.
        handler subscribe(_value: &ChannelValue): Observer(per_slot = TopicId);
    }
}

fn main() {
    use kasane_core::plugin::pubsub::TopicId;

    let mut registry: spec::HandlerRegistry<u32> = spec::HandlerRegistry::new();

    // void Observer setter takes a closure with no return value.
    registry.on_shutdown(|_state| {
        // observable side effect (e.g. cleanup) would go here
    });

    // per_slot Observer setter takes the topic key as a parameter.
    registry.on_subscribe(TopicId::new("topic.a"), |state, _value| *state);
    registry.on_subscribe(TopicId::new("topic.b"), |state, _value| state.saturating_add(1));

    let table = registry.into_table();
    assert!(table.shutdown_handler.is_some());
    assert_eq!(table.subscribe_handlers.len(), 2);
    assert_eq!(table.subscribe_handlers[0].key, TopicId::new("topic.a"));
    assert_eq!(table.subscribe_handlers[1].key, TopicId::new("topic.b"));
}
