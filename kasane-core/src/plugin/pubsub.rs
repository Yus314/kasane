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

use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};

use compact_str::CompactString;

use super::channel::ChannelValue;
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

/// Phantom-typed topic handle for compile-time pub/sub type safety.
///
/// Created by [`HandlerRegistry::publish_typed`] and consumed by
/// [`HandlerRegistry::subscribe_typed`]. The type parameter `T` ensures
/// that publishers and subscribers agree on the value type at compile time.
///
/// Zero runtime cost — the type parameter exists only at compile time.
///
/// ```ignore
/// let topic: Topic<u32> = r.publish_typed("cursor.line", |s, _| s.line);
/// r.subscribe_typed(&topic, |state, value: &u32| { ... });
/// ```
pub struct Topic<T> {
    id: TopicId,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Topic<T> {
    /// Create a new typed topic handle.
    pub fn new(name: impl Into<CompactString>) -> Self {
        Self {
            id: TopicId::new(name),
            _marker: PhantomData,
        }
    }

    /// Get the underlying topic ID.
    pub fn id(&self) -> &TopicId {
        &self.id
    }
}

impl<T> Clone for Topic<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            _marker: PhantomData,
        }
    }
}

impl<T> std::fmt::Debug for Topic<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Topic")
            .field("id", &self.id)
            .field("type", &std::any::type_name::<T>())
            .finish()
    }
}

// =============================================================================
// Type-erased handler types
// =============================================================================

/// Type-erased publisher: `fn(&dyn PluginState, &AppView) -> ChannelValue`.
pub(crate) type ErasedPublisher =
    Box<dyn Fn(&dyn PluginState, &AppView<'_>) -> ChannelValue + Send + Sync>;

/// Type-erased subscriber: `fn(&dyn PluginState, &ChannelValue) -> Box<dyn PluginState>`.
pub(crate) type ErasedSubscriber =
    Box<dyn Fn(&dyn PluginState, &ChannelValue) -> Box<dyn PluginState> + Send + Sync>;

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
/// Window size for oscillation detection history.
const HISTORY_WINDOW: usize = 6;

/// Kind of oscillation pattern detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OscillationKind {
    /// Period-2: ABAB pattern.
    Period2,
    /// Period-3: ABCABC pattern.
    Period3,
}

pub struct TopicBus {
    /// Published values for the current frame, keyed by topic.
    publications: HashMap<TopicId, Vec<PublishedValue>>,
    /// Guard: true while delivering to subscribers (prevents publish-during-deliver).
    delivering: AtomicBool,
    /// Rolling hash history per topic for oscillation detection.
    history: HashMap<TopicId, VecDeque<u64>>,
    /// Frame counter.
    frame_count: u64,
}

pub struct PublishedValue {
    /// The plugin that published this value.
    pub publisher: PluginId,
    /// The published value.
    pub value: ChannelValue,
}

impl TopicBus {
    pub fn new() -> Self {
        Self {
            publications: HashMap::new(),
            delivering: AtomicBool::new(false),
            history: HashMap::new(),
            frame_count: 0,
        }
    }

    /// Record a publication. Panics if called during delivery phase.
    pub fn publish(&mut self, topic: TopicId, publisher: PluginId, value: ChannelValue) {
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
    pub fn get_publications(&self, topic: &TopicId) -> Option<&Vec<PublishedValue>> {
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

    /// Record frame hashes for oscillation detection.
    ///
    /// Call after each pub/sub evaluation round. Hashes the serialized
    /// data bytes of each publication for accurate content-based detection.
    pub fn record_frame_hashes(&mut self) {
        self.frame_count += 1;
        for (topic, pubs) in &self.publications {
            let mut hasher = std::hash::DefaultHasher::new();
            for pv in pubs {
                pv.publisher.0.hash(&mut hasher);
                pv.value.data().hash(&mut hasher);
            }
            let hash = hasher.finish();
            let history = self.history.entry(topic.clone()).or_default();
            history.push_back(hash);
            if history.len() > HISTORY_WINDOW {
                history.pop_front();
            }
        }
    }

    /// Detect oscillation patterns in topic publication history.
    ///
    /// Returns oscillation kind for any topic exhibiting period-2 (ABAB)
    /// or period-3 (ABCABC) patterns across recent frames.
    pub fn detect_oscillation(&self) -> Vec<(TopicId, OscillationKind)> {
        let mut oscillations = Vec::new();
        for (topic, history) in &self.history {
            if let Some(kind) = detect_pattern(history) {
                oscillations.push((topic.clone(), kind));
            }
        }
        oscillations
    }

    /// Clear all publications for the next frame.
    pub fn clear(&mut self) {
        self.publications.clear();
    }
}

/// Detect period-2 or period-3 oscillation in a hash history.
fn detect_pattern(history: &VecDeque<u64>) -> Option<OscillationKind> {
    let len = history.len();
    // Period-2: need at least 4 entries (ABAB)
    if len >= 4 {
        let a = history[len - 4];
        let b = history[len - 3];
        let c = history[len - 2];
        let d = history[len - 1];
        if a != b && a == c && b == d {
            return Some(OscillationKind::Period2);
        }
    }
    // Period-3: need at least 6 entries (ABCABC)
    if len >= 6 {
        let a = history[len - 6];
        let b = history[len - 5];
        let c = history[len - 4];
        let d = history[len - 3];
        let e = history[len - 2];
        let f = history[len - 1];
        if a != b && b != c && a != c && a == d && b == e && c == f {
            return Some(OscillationKind::Period3);
        }
    }
    None
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

        bus.publish(topic.clone(), plugin, ChannelValue::new(&42u32).unwrap());

        let pubs = bus.get_publications(&topic).unwrap();
        assert_eq!(pubs.len(), 1);
        assert_eq!(pubs[0].value.deserialize::<u32>().unwrap(), 42);
    }

    #[test]
    fn multiple_publishers_same_topic() {
        let mut bus = TopicBus::new();
        let topic = TopicId::new("shared.topic");

        bus.publish(
            topic.clone(),
            PluginId("a".to_string()),
            ChannelValue::new(&1u32).unwrap(),
        );
        bus.publish(
            topic.clone(),
            PluginId("b".to_string()),
            ChannelValue::new(&2u32).unwrap(),
        );

        let pubs = bus.get_publications(&topic).unwrap();
        assert_eq!(pubs.len(), 2);
    }

    #[test]
    fn clear_removes_all() {
        let mut bus = TopicBus::new();
        let topic = TopicId::new("test");
        bus.publish(
            topic.clone(),
            PluginId("p".to_string()),
            ChannelValue::new(&()).unwrap(),
        );
        assert!(bus.get_publications(&topic).is_some());

        bus.clear();
        assert!(bus.get_publications(&topic).is_none());
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "cannot publish during delivery phase")]
    fn publish_during_delivery_panics_in_debug() {
        let mut bus = TopicBus::new();
        bus.begin_delivery();
        bus.publish(
            TopicId::new("x"),
            PluginId("p".to_string()),
            ChannelValue::new(&()).unwrap(),
        );
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

    // --- Oscillation detection ---

    #[test]
    fn no_oscillation_without_history() {
        let bus = TopicBus::new();
        assert!(bus.detect_oscillation().is_empty());
    }

    #[test]
    fn detect_period_2_oscillation() {
        let mut history = VecDeque::new();
        history.push_back(100);
        history.push_back(200);
        history.push_back(100);
        history.push_back(200);
        assert_eq!(detect_pattern(&history), Some(OscillationKind::Period2));
    }

    #[test]
    fn detect_period_3_oscillation() {
        let mut history = VecDeque::new();
        history.push_back(100);
        history.push_back(200);
        history.push_back(300);
        history.push_back(100);
        history.push_back(200);
        history.push_back(300);
        assert_eq!(detect_pattern(&history), Some(OscillationKind::Period3));
    }

    #[test]
    fn no_oscillation_stable_values() {
        let mut history = VecDeque::new();
        history.push_back(100);
        history.push_back(100);
        history.push_back(100);
        history.push_back(100);
        assert_eq!(detect_pattern(&history), None);
    }

    #[test]
    fn record_frame_hashes_builds_history() {
        let mut bus = TopicBus::new();
        let topic = TopicId::new("test.topic");
        let plugin = PluginId("p".to_string());

        // Frame 1
        bus.publish(
            topic.clone(),
            plugin.clone(),
            ChannelValue::new(&1u32).unwrap(),
        );
        bus.record_frame_hashes();
        bus.clear();

        // Frame 2
        bus.publish(
            topic.clone(),
            plugin.clone(),
            ChannelValue::new(&2u32).unwrap(),
        );
        bus.record_frame_hashes();
        bus.clear();

        assert_eq!(bus.history.get(&topic).unwrap().len(), 2);
        assert!(bus.detect_oscillation().is_empty());
    }

    #[test]
    fn alternating_publishers_trigger_period_2() {
        let mut bus = TopicBus::new();
        let topic = TopicId::new("test.osc");

        for i in 0..4 {
            let plugin_name = if i % 2 == 0 { "a" } else { "b" };
            bus.publish(
                topic.clone(),
                PluginId(plugin_name.to_string()),
                ChannelValue::new(&()).unwrap(),
            );
            bus.record_frame_hashes();
            bus.clear();
        }

        let oscillations = bus.detect_oscillation();
        assert_eq!(oscillations.len(), 1);
        assert_eq!(oscillations[0].0, topic);
        assert_eq!(oscillations[0].1, OscillationKind::Period2);
    }
}
