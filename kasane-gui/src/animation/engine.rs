//! Track-based animation engine.

use std::collections::HashMap;
use std::time::Instant;

use super::element_key::ElementKey;
use super::property_track::{PropertyName, PropertyTrack};
use super::track::{AnimationValue, EasingFn, TrackId, TrackState};

/// General-purpose track-based animation engine.
///
/// Supports two keying modes:
/// - `TrackId` (legacy): simple named tracks for cursor, menu, etc.
/// - `(ElementKey, PropertyName)`: per-element property animation
///   for arbitrary UI elements across frames.
pub struct AnimationEngine {
    tracks: HashMap<TrackId, AnimationValue>,
    /// Per-element property tracks, keyed by (element, property).
    properties: HashMap<(ElementKey, PropertyName), PropertyTrack>,
    last_frame: Instant,
}

impl Default for AnimationEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl AnimationEngine {
    pub fn new() -> Self {
        Self {
            tracks: HashMap::new(),
            properties: HashMap::new(),
            last_frame: Instant::now(),
        }
    }

    /// Returns true if the given track is registered.
    pub fn has_track(&self, id: TrackId) -> bool {
        self.tracks.contains_key(&id)
    }

    /// Register a new animation track with initial value, duration, and easing.
    pub fn register(&mut self, id: TrackId, initial: f32, duration: f32, easing: EasingFn) {
        self.tracks
            .insert(id, AnimationValue::new(initial, duration, easing));
    }

    /// Set the target value for a track. Starts animating from current value.
    pub fn set_target(&mut self, id: TrackId, target: f32) {
        if let Some(track) = self.tracks.get_mut(&id) {
            let was_idle = !track.is_active();
            track.set_target(target);
            // If the track just woke up from idle, reset the frame timer
            // so the first tick doesn't see a huge dt from the idle period.
            if was_idle && track.is_active() {
                self.last_frame = Instant::now();
            }
        }
    }

    /// Snap a track to a value immediately (no animation).
    pub fn snap(&mut self, id: TrackId, value: f32) {
        if let Some(track) = self.tracks.get_mut(&id) {
            track.snap(value);
        }
    }

    /// Get the current value of a track.
    pub fn value(&self, id: TrackId) -> f32 {
        self.tracks.get(&id).map(|t| t.current).unwrap_or_default()
    }

    /// Advance all tracks by one frame. Returns true if any track is still active.
    pub fn tick(&mut self) -> bool {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;

        let mut any_active = false;
        for track in self.tracks.values_mut() {
            if track.tick(dt) {
                any_active = true;
            }
        }
        for track in self.properties.values_mut() {
            if track.tick(dt) {
                any_active = true;
            }
        }
        any_active
    }

    /// Returns true if any track is currently animating.
    pub fn is_animating(&self) -> bool {
        self.tracks.values().any(|t| t.is_active())
            || self.properties.values().any(|t| t.is_active())
    }

    /// Compute the next frame deadline, or None if all tracks are idle.
    pub fn next_frame_deadline(&self) -> Option<Instant> {
        if !self.is_animating() {
            return None;
        }
        // 60fps for movement tracks
        Some(self.last_frame + std::time::Duration::from_nanos(16_666_667))
    }

    /// Pause all tracks.
    pub fn pause_all(&mut self) {
        for track in self.tracks.values_mut() {
            if track.state == TrackState::Running {
                track.state = TrackState::Paused;
            }
        }
    }

    /// Resume all paused tracks.
    pub fn resume_all(&mut self) {
        self.last_frame = Instant::now();
        for track in self.tracks.values_mut() {
            if track.state == TrackState::Paused {
                track.state = TrackState::Running;
            }
        }
    }

    /// Update the duration of a track.
    pub fn set_duration(&mut self, id: TrackId, duration: f32) {
        if let Some(track) = self.tracks.get_mut(&id) {
            track.duration = duration;
        }
    }

    /// Update the easing function of a track.
    pub fn set_easing(&mut self, id: TrackId, easing: EasingFn) {
        if let Some(track) = self.tracks.get_mut(&id) {
            track.easing = easing;
        }
    }

    // --- Per-element property animation API ---

    /// Register a property track for an element.
    pub fn register_property(&mut self, key: ElementKey, prop: PropertyName, track: PropertyTrack) {
        self.properties.insert((key, prop), track);
    }

    /// Set the target value for an element's property.
    pub fn set_property_target(&mut self, key: ElementKey, prop: PropertyName, target: f32) {
        if let Some(track) = self.properties.get_mut(&(key, prop)) {
            let was_idle = !track.is_active();
            track.set_target(target);
            if was_idle && track.is_active() {
                self.last_frame = Instant::now();
            }
        }
    }

    /// Snap an element's property to a value immediately.
    pub fn snap_property(&mut self, key: ElementKey, prop: PropertyName, value: f32) {
        if let Some(track) = self.properties.get_mut(&(key, prop)) {
            track.snap(value);
        }
    }

    /// Get the current value of an element's property.
    pub fn property_value(&self, key: ElementKey, prop: PropertyName) -> f32 {
        self.properties
            .get(&(key, prop))
            .map(|t| t.current)
            .unwrap_or_default()
    }

    /// Check if a property track exists.
    pub fn has_property(&self, key: ElementKey, prop: PropertyName) -> bool {
        self.properties.contains_key(&(key, prop))
    }

    /// Remove all property tracks for a given element.
    pub fn remove_element(&mut self, key: ElementKey) {
        self.properties.retain(|(k, _), _| *k != key);
    }
}

#[cfg(test)]
mod tests {
    use super::super::element_key::ElementKind;
    use super::*;

    #[test]
    fn register_and_value() {
        let mut engine = AnimationEngine::new();
        engine.register(TrackId::CURSOR_X, 5.0, 0.1, EasingFn::Linear);
        assert!((engine.value(TrackId::CURSOR_X) - 5.0).abs() < 0.001);
    }

    #[test]
    fn set_target_activates() {
        let mut engine = AnimationEngine::new();
        engine.register(TrackId::CURSOR_X, 0.0, 0.1, EasingFn::Linear);
        assert!(!engine.is_animating());

        engine.set_target(TrackId::CURSOR_X, 10.0);
        assert!(engine.is_animating());
    }

    #[test]
    fn snap_is_immediate() {
        let mut engine = AnimationEngine::new();
        engine.register(TrackId::CURSOR_X, 0.0, 0.1, EasingFn::Linear);
        engine.snap(TrackId::CURSOR_X, 42.0);
        assert!((engine.value(TrackId::CURSOR_X) - 42.0).abs() < 0.001);
        assert!(!engine.is_animating());
    }

    #[test]
    fn tick_progresses_animation() {
        let mut engine = AnimationEngine::new();
        engine.register(TrackId::CURSOR_X, 0.0, 0.1, EasingFn::Linear);
        engine.set_target(TrackId::CURSOR_X, 10.0);

        // Simulate time passing
        std::thread::sleep(std::time::Duration::from_millis(5));
        engine.tick();

        let val = engine.value(TrackId::CURSOR_X);
        assert!(val > 0.0, "should have progressed from 0");
        assert!(val < 10.0, "should not have reached target yet");
    }

    #[test]
    fn tick_completes_animation() {
        let mut engine = AnimationEngine::new();
        engine.register(TrackId::CURSOR_X, 0.0, 0.01, EasingFn::Linear);
        engine.set_target(TrackId::CURSOR_X, 10.0);

        // Wait longer than duration
        std::thread::sleep(std::time::Duration::from_millis(20));
        engine.tick();

        assert!((engine.value(TrackId::CURSOR_X) - 10.0).abs() < 0.001);
        assert!(!engine.is_animating());
    }

    #[test]
    fn pause_resume() {
        let mut engine = AnimationEngine::new();
        engine.register(TrackId::CURSOR_X, 0.0, 1.0, EasingFn::Linear);
        engine.set_target(TrackId::CURSOR_X, 10.0);
        assert!(engine.is_animating());

        engine.pause_all();
        assert!(!engine.is_animating());
        assert!(engine.next_frame_deadline().is_none());

        engine.resume_all();
        assert!(engine.is_animating());
    }

    #[test]
    fn deadline_none_when_idle() {
        let engine = AnimationEngine::new();
        assert!(engine.next_frame_deadline().is_none());
    }

    #[test]
    fn deadline_some_when_active() {
        let mut engine = AnimationEngine::new();
        engine.register(TrackId::CURSOR_X, 0.0, 0.1, EasingFn::Linear);
        engine.set_target(TrackId::CURSOR_X, 10.0);
        assert!(engine.next_frame_deadline().is_some());
    }

    #[test]
    fn multiple_tracks_independent() {
        let mut engine = AnimationEngine::new();
        engine.register(TrackId::CURSOR_X, 0.0, 0.1, EasingFn::Linear);
        engine.register(TrackId::CURSOR_Y, 0.0, 0.1, EasingFn::EaseOut);

        engine.set_target(TrackId::CURSOR_X, 10.0);
        // Y stays idle
        assert!(engine.is_animating());

        engine.snap(TrackId::CURSOR_X, 10.0);
        assert!(!engine.is_animating());
    }

    #[test]
    fn property_track_register_and_value() {
        let mut engine = AnimationEngine::new();
        let key = ElementKey::CURSOR;
        engine.register_property(
            key,
            PropertyName::X,
            PropertyTrack::tween(5.0, 0.1, EasingFn::Linear),
        );
        assert!((engine.property_value(key, PropertyName::X) - 5.0).abs() < 0.001);
    }

    #[test]
    fn property_track_set_target_activates() {
        let mut engine = AnimationEngine::new();
        let key = ElementKey::CURSOR;
        engine.register_property(
            key,
            PropertyName::Opacity,
            PropertyTrack::tween(1.0, 0.1, EasingFn::EaseOut),
        );
        assert!(!engine.is_animating());

        engine.set_property_target(key, PropertyName::Opacity, 0.0);
        assert!(engine.is_animating());
    }

    #[test]
    fn property_track_snap() {
        let mut engine = AnimationEngine::new();
        let key = ElementKey::MENU;
        engine.register_property(key, PropertyName::Y, PropertyTrack::spring(0.0, 300.0, 1.0));
        engine.snap_property(key, PropertyName::Y, 100.0);
        assert!((engine.property_value(key, PropertyName::Y) - 100.0).abs() < 0.001);
        assert!(!engine.is_animating());
    }

    #[test]
    fn remove_element_clears_properties() {
        let mut engine = AnimationEngine::new();
        let key = ElementKey::indexed(ElementKind::Pane, 0);
        engine.register_property(
            key,
            PropertyName::X,
            PropertyTrack::tween(0.0, 0.1, EasingFn::Linear),
        );
        engine.register_property(
            key,
            PropertyName::Y,
            PropertyTrack::tween(0.0, 0.1, EasingFn::Linear),
        );
        assert!(engine.has_property(key, PropertyName::X));

        engine.remove_element(key);
        assert!(!engine.has_property(key, PropertyName::X));
        assert!(!engine.has_property(key, PropertyName::Y));
    }
}
