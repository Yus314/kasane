//! Animation track: individual animated value with easing.

use kasane_core::render::EasingCurve;

/// Identifies an animation track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrackId(u32);

impl TrackId {
    pub const CURSOR_X: Self = Self(0);
    pub const CURSOR_Y: Self = Self(1);
    pub const CURSOR_OPACITY: Self = Self(2);
    pub const MENU_OPACITY: Self = Self(3);
    pub const INFO_OPACITY: Self = Self(4);
}

/// State of an animation track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackState {
    /// Not animating — at rest.
    Idle,
    /// Actively interpolating.
    Running,
    /// Temporarily suspended.
    Paused,
}

/// An individual animated float value with easing.
#[derive(Debug, Clone)]
pub struct AnimationValue {
    pub(crate) current: f32,
    pub(crate) target: f32,
    pub(crate) prev: f32,
    /// Progress 0.0 → 1.0.
    pub(crate) t: f32,
    /// Duration in seconds.
    pub(crate) duration: f32,
    pub(crate) easing: EasingFn,
    pub(crate) state: TrackState,
}

impl AnimationValue {
    pub fn new(initial: f32, duration: f32, easing: EasingFn) -> Self {
        Self {
            current: initial,
            target: initial,
            prev: initial,
            t: 1.0,
            duration,
            easing,
            state: TrackState::Idle,
        }
    }

    /// Set a new target. Starts animating from the current value.
    pub fn set_target(&mut self, target: f32) {
        if (target - self.target).abs() < 0.001 {
            return;
        }
        self.prev = self.current;
        self.target = target;
        self.t = 0.0;
        self.state = TrackState::Running;
    }

    /// Snap immediately to a value without animation.
    pub fn snap(&mut self, value: f32) {
        self.current = value;
        self.target = value;
        self.prev = value;
        self.t = 1.0;
        self.state = TrackState::Idle;
    }

    /// Advance the animation by `dt` seconds. Returns true if still active.
    pub fn tick(&mut self, dt: f32) -> bool {
        if self.state != TrackState::Running {
            return false;
        }
        let duration = self.duration.max(0.001);
        self.t += dt / duration;
        if self.t >= 1.0 {
            self.t = 1.0;
            self.current = self.target;
            self.state = TrackState::Idle;
            return false;
        }
        let eased = self.easing.apply(self.t);
        self.current = self.prev + (self.target - self.prev) * eased;
        true
    }

    pub fn is_active(&self) -> bool {
        self.state == TrackState::Running
    }
}

/// Easing function wrapper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EasingFn {
    Linear,
    EaseOut,
    EaseInOut,
}

impl EasingFn {
    pub fn apply(self, t: f32) -> f32 {
        match self {
            Self::Linear => t,
            Self::EaseOut => ease_out_cubic(t),
            Self::EaseInOut => ease_in_out_cubic(t),
        }
    }
}

impl From<EasingCurve> for EasingFn {
    fn from(curve: EasingCurve) -> Self {
        match curve {
            EasingCurve::Linear => Self::Linear,
            EasingCurve::EaseOut => Self::EaseOut,
            EasingCurve::EaseInOut => Self::EaseInOut,
        }
    }
}

/// Ease-out cubic: decelerating to zero velocity.
pub(crate) fn ease_out_cubic(t: f32) -> f32 {
    let t = t - 1.0;
    t * t * t + 1.0
}

/// Ease-in-out cubic: accelerate then decelerate.
pub(crate) fn ease_in_out_cubic(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        let t = 2.0 * t - 2.0;
        0.5 * t * t * t + 1.0
    }
}
