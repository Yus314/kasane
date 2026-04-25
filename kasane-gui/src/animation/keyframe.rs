//! Multi-segment keyframe animation tracks.
//!
//! A `KeyframeTrack` chains multiple timed segments, each with its own
//! target value and easing. Useful for complex multi-step transitions.

use super::track::EasingFn;

/// A single keyframe: target value + duration + easing for the segment
/// leading _to_ this keyframe.
#[derive(Debug, Clone)]
pub struct Keyframe {
    pub value: f32,
    pub duration: f32,
    pub easing: EasingFn,
}

/// A multi-segment keyframe track.
///
/// Segments are processed in order. When one completes, the next begins.
/// After the last segment, the track goes idle at the final value.
#[derive(Debug, Clone)]
pub struct KeyframeTrack {
    keyframes: Vec<Keyframe>,
    current_segment: usize,
    current_value: f32,
    segment_prev: f32,
    segment_t: f32,
    looping: bool,
}

impl KeyframeTrack {
    /// Create a new keyframe track starting at `initial`.
    pub fn new(initial: f32, keyframes: Vec<Keyframe>) -> Self {
        Self {
            keyframes,
            current_segment: 0,
            current_value: initial,
            segment_prev: initial,
            segment_t: 0.0,
            looping: false,
        }
    }

    /// Enable looping: after the last keyframe, restart from the beginning.
    pub fn with_looping(mut self, looping: bool) -> Self {
        self.looping = looping;
        self
    }

    /// Current interpolated value.
    pub fn value(&self) -> f32 {
        self.current_value
    }

    /// Returns `true` if still animating.
    pub fn is_active(&self) -> bool {
        self.current_segment < self.keyframes.len()
    }

    /// Advance by `dt` seconds. Returns `true` if still active.
    pub fn tick(&mut self, dt: f32) -> bool {
        if !self.is_active() {
            return false;
        }

        let kf = &self.keyframes[self.current_segment];
        let dur = kf.duration.max(0.001);
        self.segment_t += dt / dur;

        if self.segment_t >= 1.0 {
            // Segment complete
            self.current_value = kf.value;
            self.segment_prev = kf.value;
            self.segment_t = 0.0;
            self.current_segment += 1;

            if self.current_segment >= self.keyframes.len() && self.looping {
                self.current_segment = 0;
            }
            return self.is_active();
        }

        let eased = kf.easing.apply(self.segment_t);
        self.current_value = self.segment_prev + (kf.value - self.segment_prev) * eased;
        true
    }

    /// Reset to initial state.
    pub fn reset(&mut self, initial: f32) {
        self.current_segment = 0;
        self.current_value = initial;
        self.segment_prev = initial;
        self.segment_t = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_keyframe_completes() {
        let mut track = KeyframeTrack::new(
            0.0,
            vec![Keyframe {
                value: 10.0,
                duration: 0.1,
                easing: EasingFn::Linear,
            }],
        );
        assert!(track.is_active());

        // Advance past duration
        track.tick(0.15);
        assert!(!track.is_active());
        assert!((track.value() - 10.0).abs() < 0.01);
    }

    #[test]
    fn multi_segment_sequence() {
        let mut track = KeyframeTrack::new(
            0.0,
            vec![
                Keyframe {
                    value: 50.0,
                    duration: 0.1,
                    easing: EasingFn::Linear,
                },
                Keyframe {
                    value: 100.0,
                    duration: 0.1,
                    easing: EasingFn::Linear,
                },
            ],
        );

        // Complete first segment
        track.tick(0.15);
        assert!(track.is_active());
        assert!((track.value() - 50.0).abs() < 0.01);

        // Complete second segment
        track.tick(0.15);
        assert!(!track.is_active());
        assert!((track.value() - 100.0).abs() < 0.01);
    }

    #[test]
    fn looping_restarts() {
        let mut track = KeyframeTrack::new(
            0.0,
            vec![Keyframe {
                value: 10.0,
                duration: 0.1,
                easing: EasingFn::Linear,
            }],
        )
        .with_looping(true);

        track.tick(0.15);
        // Should still be active due to looping
        assert!(track.is_active());
    }

    #[test]
    fn reset_restarts() {
        let mut track = KeyframeTrack::new(
            0.0,
            vec![Keyframe {
                value: 10.0,
                duration: 0.1,
                easing: EasingFn::Linear,
            }],
        );

        track.tick(0.15);
        assert!(!track.is_active());

        track.reset(5.0);
        assert!(track.is_active());
        assert!((track.value() - 5.0).abs() < 0.01);
    }
}
