//! Property-based animation tracks.
//!
//! A `PropertyTrack` animates a single scalar property using either
//! tween interpolation (with easing) or spring physics.

use super::spring::SpringPhysics;
use super::track::EasingFn;

/// Which property of an element is being animated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PropertyName {
    X,
    Y,
    Width,
    Height,
    Opacity,
    Scale,
    Rotation,
    /// A plugin-defined custom property.
    Custom(u32),
}

/// Interpolation mode for a property track.
#[derive(Debug, Clone)]
pub enum InterpolationMode {
    /// Duration-based tween with easing function.
    Tween {
        duration: f32,
        easing: EasingFn,
        /// Progress 0.0 → 1.0
        t: f32,
        prev: f32,
    },
    /// Physics-based spring interpolation.
    Spring(SpringPhysics),
}

/// A single animated property value.
#[derive(Debug, Clone)]
pub struct PropertyTrack {
    pub current: f32,
    pub target: f32,
    mode: InterpolationMode,
}

impl PropertyTrack {
    /// Create a tween-based property track.
    pub fn tween(initial: f32, duration: f32, easing: EasingFn) -> Self {
        Self {
            current: initial,
            target: initial,
            mode: InterpolationMode::Tween {
                duration,
                easing,
                t: 1.0,
                prev: initial,
            },
        }
    }

    /// Create a spring-based property track.
    pub fn spring(initial: f32, stiffness: f64, damping_ratio: f64) -> Self {
        let mut spring = if (damping_ratio - 1.0).abs() < 0.001 {
            SpringPhysics::critically_damped(stiffness)
        } else {
            SpringPhysics::underdamped(stiffness, damping_ratio)
        };
        spring.snap(initial as f64);
        Self {
            current: initial,
            target: initial,
            mode: InterpolationMode::Spring(spring),
        }
    }

    /// Set a new target. Starts animating from the current value.
    pub fn set_target(&mut self, target: f32) {
        if (target - self.target).abs() < 0.001 {
            return;
        }
        self.target = target;
        match &mut self.mode {
            InterpolationMode::Tween { t, prev, .. } => {
                *prev = self.current;
                *t = 0.0;
            }
            InterpolationMode::Spring(spring) => {
                spring.set_target(target as f64);
            }
        }
    }

    /// Snap immediately to a value without animation.
    pub fn snap(&mut self, value: f32) {
        self.current = value;
        self.target = value;
        match &mut self.mode {
            InterpolationMode::Tween { t, prev, .. } => {
                *prev = value;
                *t = 1.0;
            }
            InterpolationMode::Spring(spring) => {
                spring.snap(value as f64);
            }
        }
    }

    /// Advance by `dt` seconds. Returns `true` if still active.
    pub fn tick(&mut self, dt: f32) -> bool {
        match &mut self.mode {
            InterpolationMode::Tween {
                duration,
                easing,
                t,
                prev,
            } => {
                if *t >= 1.0 {
                    return false;
                }
                let dur = (*duration).max(0.001);
                *t += dt / dur;
                if *t >= 1.0 {
                    *t = 1.0;
                    self.current = self.target;
                    return false;
                }
                let eased = easing.apply(*t);
                self.current = *prev + (self.target - *prev) * eased;
                true
            }
            InterpolationMode::Spring(spring) => {
                let active = spring.tick(dt as f64);
                self.current = spring.position as f32;
                active
            }
        }
    }

    /// Returns `true` if this track is currently animating.
    pub fn is_active(&self) -> bool {
        match &self.mode {
            InterpolationMode::Tween { t, .. } => *t < 1.0,
            InterpolationMode::Spring(spring) => !spring.is_at_rest(),
        }
    }

    /// Update the tween duration (no-op for spring tracks).
    pub fn set_duration(&mut self, duration: f32) {
        if let InterpolationMode::Tween { duration: d, .. } = &mut self.mode {
            *d = duration;
        }
    }

    /// Update the easing function (no-op for spring tracks).
    pub fn set_easing(&mut self, easing: EasingFn) {
        if let InterpolationMode::Tween { easing: e, .. } = &mut self.mode {
            *e = easing;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tween_set_target_animates() {
        let mut track = PropertyTrack::tween(0.0, 0.1, EasingFn::Linear);
        track.set_target(10.0);
        assert!(track.is_active());

        track.tick(0.05);
        assert!(track.current > 0.0);
        assert!(track.current < 10.0);

        track.tick(0.1);
        assert!((track.current - 10.0).abs() < 0.01);
        assert!(!track.is_active());
    }

    #[test]
    fn spring_set_target_animates() {
        let mut track = PropertyTrack::spring(0.0, 300.0, 1.0);
        track.set_target(100.0);
        assert!(track.is_active());

        for _ in 0..120 {
            track.tick(1.0 / 60.0);
        }
        assert!(
            (track.current - 100.0).abs() < 1.0,
            "spring should converge, got {}",
            track.current
        );
    }

    #[test]
    fn snap_is_immediate() {
        let mut track = PropertyTrack::tween(0.0, 0.1, EasingFn::EaseOut);
        track.set_target(10.0);
        track.snap(42.0);
        assert!((track.current - 42.0).abs() < 0.001);
        assert!(!track.is_active());
    }

    #[test]
    fn property_name_distinct() {
        assert_ne!(PropertyName::X, PropertyName::Y);
        assert_ne!(PropertyName::Custom(0), PropertyName::Custom(1));
    }
}
