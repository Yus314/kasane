//! Track-based animation engine.

use std::collections::HashMap;
use std::time::Instant;

use super::track::{AnimationValue, EasingFn, TrackId, TrackState};

/// General-purpose track-based animation engine.
///
/// Manages multiple named animation tracks, each interpolating independently.
/// Replaces the cursor-specific `CursorAnimation` with a generic system.
pub struct AnimationEngine {
    tracks: HashMap<TrackId, AnimationValue>,
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
        any_active
    }

    /// Returns true if any track is currently animating.
    pub fn is_animating(&self) -> bool {
        self.tracks.values().any(|t| t.is_active())
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
}

#[cfg(test)]
mod tests {
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
}
