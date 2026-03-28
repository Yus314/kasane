//! Topic-based inter-plugin publish/subscribe system.
//!
//! Plugins can publish typed values on named topics and subscribe to topics
//! published by other plugins. Evaluation is two-phase:
//!
//! 1. **Collect publications** — each publisher produces a value for the current frame.
//! 2. **Deliver to subscribers** — collected values are delivered to subscribers,
//!    which may update their own state.
//!
//! Cycle prevention: publishing during delivery panics in debug mode.

use std::any::Any;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use compact_str::CompactString;

use super::{AppView, PluginId, PluginState};

/// Identifier for a pub/sub topic.
///
/// Topics are namespaced by convention (e.g., `"myplugin.cursor-position"`).
/// Two `TopicId` values are equal if their string representations match.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TopicId(pub CompactString);

impl TopicId {
    pub fn new(name: impl Into<CompactString>) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// =============================================================================
// Type-erased handler types
// =============================================================================

/// Type-erased publisher: `fn(&dyn PluginState, &AppView) -> Box<dyn Any>`.
pub(crate) type ErasedPublisher =
    Box<dyn Fn(&dyn PluginState, &AppView<'_>) -> Box<dyn Any + Send> + Send + Sync>;

/// Type-erased subscriber: `fn(&dyn PluginState, &dyn Any) -> Box<dyn PluginState>`.
pub(crate) type ErasedSubscriber =
    Box<dyn Fn(&dyn PluginState, &dyn Any) -> Box<dyn PluginState> + Send + Sync>;

// =============================================================================
// Registration entries (stored in HandlerTable)
// =============================================================================

/// A publication registration: plugin publishes `T` on a topic.
pub(crate) struct PublishEntry {
    pub(crate) topic: TopicId,
    pub(crate) handler: ErasedPublisher,
}

/// A subscription registration: plugin receives `T` from a topic.
pub(crate) struct SubscribeEntry {
    pub(crate) topic: TopicId,
    pub(crate) handler: ErasedSubscriber,
}

// =============================================================================
// TopicBus — runtime coordination
// =============================================================================

/// Runtime coordinator for topic-based pub/sub evaluation.
///
/// Held externally (e.g. on the event loop) and passed to the plugin runtime
/// during the pub/sub evaluation phase.
pub struct TopicBus {
    /// Published values for the current frame, keyed by topic.
    publications: HashMap<TopicId, Vec<PublishedValue>>,
    /// Guard: true while delivering to subscribers (prevents publish-during-deliver).
    delivering: AtomicBool,
}

pub(crate) struct PublishedValue {
    #[allow(dead_code)]
    pub(crate) publisher: PluginId,
    pub(crate) value: Box<dyn Any + Send>,
}

impl TopicBus {
    pub fn new() -> Self {
        Self {
            publications: HashMap::new(),
            delivering: AtomicBool::new(false),
        }
    }

    /// Record a publication. Panics if called during delivery phase.
    pub(crate) fn publish(
        &mut self,
        topic: TopicId,
        publisher: PluginId,
        value: Box<dyn Any + Send>,
    ) {
        debug_assert!(
            !self.delivering.load(Ordering::Relaxed),
            "cannot publish during delivery phase (cycle detected)"
        );
        self.publications
            .entry(topic)
            .or_default()
            .push(PublishedValue { publisher, value });
    }

    /// Get published values for a topic (for subscriber delivery).
    pub(crate) fn get_publications(&self, topic: &TopicId) -> Option<&Vec<PublishedValue>> {
        self.publications.get(topic)
    }

    /// Enter delivery phase (sets the guard flag).
    pub(crate) fn begin_delivery(&self) {
        self.delivering.store(true, Ordering::Relaxed);
    }

    /// Exit delivery phase (clears the guard flag).
    pub(crate) fn end_delivery(&self) {
        self.delivering.store(false, Ordering::Relaxed);
    }

    /// Clear all publications for the next frame.
    pub fn clear(&mut self) {
        self.publications.clear();
    }
}

impl Default for TopicBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topic_id_equality() {
        let a = TopicId::new("foo.bar");
        let b = TopicId::new("foo.bar");
        let c = TopicId::new("foo.baz");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn publish_and_retrieve() {
        let mut bus = TopicBus::new();
        let topic = TopicId::new("test.counter");
        let plugin = PluginId("test-plugin".to_string());

        bus.publish(topic.clone(), plugin, Box::new(42u32));

        let pubs = bus.get_publications(&topic).unwrap();
        assert_eq!(pubs.len(), 1);
        assert_eq!(*pubs[0].value.downcast_ref::<u32>().unwrap(), 42);
    }

    #[test]
    fn multiple_publishers_same_topic() {
        let mut bus = TopicBus::new();
        let topic = TopicId::new("shared.topic");

        bus.publish(topic.clone(), PluginId("a".to_string()), Box::new(1u32));
        bus.publish(topic.clone(), PluginId("b".to_string()), Box::new(2u32));

        let pubs = bus.get_publications(&topic).unwrap();
        assert_eq!(pubs.len(), 2);
    }

    #[test]
    fn clear_removes_all() {
        let mut bus = TopicBus::new();
        let topic = TopicId::new("test");
        bus.publish(topic.clone(), PluginId("p".to_string()), Box::new(()));
        assert!(bus.get_publications(&topic).is_some());

        bus.clear();
        assert!(bus.get_publications(&topic).is_none());
    }

    #[test]
    #[should_panic(expected = "cannot publish during delivery phase")]
    fn publish_during_delivery_panics_in_debug() {
        let mut bus = TopicBus::new();
        bus.begin_delivery();
        bus.publish(TopicId::new("x"), PluginId("p".to_string()), Box::new(()));
    }

    #[test]
    fn delivery_guard_lifecycle() {
        let bus = TopicBus::new();
        assert!(!bus.delivering.load(Ordering::Relaxed));
        bus.begin_delivery();
        assert!(bus.delivering.load(Ordering::Relaxed));
        bus.end_delivery();
        assert!(!bus.delivering.load(Ordering::Relaxed));
    }

    #[test]
    fn missing_topic_returns_none() {
        let bus = TopicBus::new();
        assert!(bus.get_publications(&TopicId::new("nonexistent")).is_none());
    }
}
