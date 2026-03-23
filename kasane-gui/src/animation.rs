//! Animations: cursor movement easing, smooth scroll, blink cycle.

use std::time::Instant;

use kasane_core::render::{BlinkHint, EasingCurve, MovementHint};

/// Default duration of cursor move animation (ease-out).
const DEFAULT_MOVE_DURATION: f32 = 0.1; // 100ms
/// Default delay after movement before blinking starts.
const DEFAULT_BLINK_DELAY: f32 = 0.5; // 500ms
/// Default period of one full blink cycle.
const DEFAULT_BLINK_PERIOD: f32 = 1.0; // 1s
/// Default minimum opacity during blink.
const DEFAULT_MIN_OPACITY: f32 = 0.3;

/// Computed cursor state for the current frame.
pub struct CursorRenderState {
    /// Pixel X coordinate (interpolated).
    pub x: f32,
    /// Pixel Y coordinate (interpolated).
    pub y: f32,
    /// Opacity (0.0 = invisible, 1.0 = fully visible).
    pub opacity: f32,
}

/// Smooth cursor movement and blink animation.
pub struct CursorAnimation {
    /// Current interpolated position (in cell coordinates, fractional).
    current_x: f32,
    current_y: f32,
    /// Previous position at the start of the current animation.
    prev_x: f32,
    prev_y: f32,
    /// Target position (in cell coordinates).
    target_x: f32,
    target_y: f32,
    /// Animation progress 0.0 → 1.0.
    move_t: f32,
    /// Time since last move (for blink delay).
    time_since_move: f32,
    /// Last frame timestamp.
    last_frame: Instant,
    /// Whether any animation is currently running.
    pub is_animating: bool,
    /// Whether the cursor has been initialized with a target.
    initialized: bool,
    // --- Plugin-configurable parameters ---
    blink_enabled: bool,
    blink_delay: f32,
    blink_period: f32,
    min_opacity: f32,
    move_enabled: bool,
    move_duration: f32,
    easing: EasingCurve,
}

impl Default for CursorAnimation {
    fn default() -> Self {
        Self::new()
    }
}

impl CursorAnimation {
    pub fn new() -> Self {
        CursorAnimation {
            current_x: 0.0,
            current_y: 0.0,
            prev_x: 0.0,
            prev_y: 0.0,
            target_x: 0.0,
            target_y: 0.0,
            move_t: 1.0,
            time_since_move: 0.0,
            last_frame: Instant::now(),
            is_animating: true, // Start with blink animation
            initialized: false,
            blink_enabled: true,
            blink_delay: DEFAULT_BLINK_DELAY,
            blink_period: DEFAULT_BLINK_PERIOD,
            min_opacity: DEFAULT_MIN_OPACITY,
            move_enabled: true,
            move_duration: DEFAULT_MOVE_DURATION,
            easing: EasingCurve::EaseOut,
        }
    }

    /// Apply plugin hints to override animation parameters.
    pub fn apply_hints(&mut self, blink: Option<BlinkHint>, movement: Option<MovementHint>) {
        if let Some(b) = blink {
            self.blink_enabled = b.enabled;
            self.blink_delay = b.delay_ms as f32 / 1000.0;
            self.blink_period = b.period_ms as f32 / 1000.0;
            self.min_opacity = b.min_opacity;
        }
        if let Some(m) = movement {
            self.move_enabled = m.enabled;
            self.move_duration = m.duration_ms as f32 / 1000.0;
            self.easing = m.easing;
        }
    }

    /// Update the target cursor position (cell coordinates).
    pub fn update_target(&mut self, x: u16, y: u16) {
        let tx = x as f32;
        let ty = y as f32;
        if !self.initialized {
            // First target: snap immediately
            self.current_x = tx;
            self.current_y = ty;
            self.prev_x = tx;
            self.prev_y = ty;
            self.target_x = tx;
            self.target_y = ty;
            self.move_t = 1.0;
            self.initialized = true;
            return;
        }
        if (tx - self.target_x).abs() < 0.01 && (ty - self.target_y).abs() < 0.01 {
            return; // No change
        }
        // Start new movement animation from current interpolated position
        self.prev_x = self.current_x;
        self.prev_y = self.current_y;
        self.target_x = tx;
        self.target_y = ty;
        self.move_t = 0.0;
        self.time_since_move = 0.0;
        self.is_animating = true;
    }

    /// Advance animation by one frame. Returns the cursor's pixel position and opacity.
    pub fn tick(&mut self, cell_width: f32, cell_height: f32) -> CursorRenderState {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;

        // Advance move animation
        if self.move_enabled && self.move_t < 1.0 {
            let duration = self.move_duration.max(0.001);
            self.move_t += dt / duration;
            if self.move_t >= 1.0 {
                self.move_t = 1.0;
            }
            let t = match self.easing {
                EasingCurve::Linear => self.move_t,
                EasingCurve::EaseOut => ease_out_cubic(self.move_t),
                EasingCurve::EaseInOut => ease_in_out_cubic(self.move_t),
            };
            self.current_x = self.prev_x + (self.target_x - self.prev_x) * t;
            self.current_y = self.prev_y + (self.target_y - self.prev_y) * t;
        } else {
            self.current_x = self.target_x;
            self.current_y = self.target_y;
        }

        // Advance blink timer
        self.time_since_move += dt;

        // Compute opacity
        let opacity = if !self.blink_enabled || self.time_since_move < self.blink_delay {
            1.0 // Fully visible when blink disabled or right after movement
        } else {
            let blink_t = self.time_since_move - self.blink_delay;
            let period = self.blink_period.max(0.001);
            let phase = (blink_t / period * std::f32::consts::TAU).sin();
            // Map sin(-1..1) to opacity(min_opacity..1.0)
            let half_range = (1.0 - self.min_opacity) / 2.0;
            (self.min_opacity + half_range) + half_range * phase
        };

        // Determine if we need to keep animating
        let move_active = self.move_enabled && self.move_t < 1.0;
        let blink_active =
            self.blink_enabled && self.time_since_move < self.blink_delay + self.blink_period;
        self.is_animating = move_active || blink_active;

        CursorRenderState {
            x: self.current_x * cell_width,
            y: self.current_y * cell_height,
            opacity,
        }
    }

    /// Compute the next instant at which the cursor animation needs a frame,
    /// or `None` if the cursor is fully idle (no movement, blink cycle complete).
    pub fn next_frame_deadline(&self) -> Option<Instant> {
        if !self.is_animating {
            return None;
        }
        if self.move_enabled && self.move_t < 1.0 {
            // Smooth 60fps move animation
            Some(self.last_frame + std::time::Duration::from_nanos(16_666_667))
        } else if self.blink_enabled && self.time_since_move < self.blink_delay {
            // Waiting for blink to start — wake at blink start
            let remaining = self.blink_delay - self.time_since_move;
            Some(self.last_frame + std::time::Duration::from_secs_f32(remaining))
        } else if self.blink_enabled {
            // Blinking — 30fps is sufficient for sin wave
            Some(self.last_frame + std::time::Duration::from_nanos(33_333_333))
        } else {
            None
        }
    }

    /// Pause the animation (e.g. on window focus loss).
    pub fn pause(&mut self) {
        self.is_animating = false;
    }

    /// Resume the animation after a pause, adjusting last_frame to avoid a time jump.
    pub fn resume(&mut self) {
        self.last_frame = Instant::now();
        // Re-evaluate whether we should be animating
        let move_active = self.move_enabled && self.move_t < 1.0;
        let blink_active =
            self.blink_enabled && self.time_since_move < self.blink_delay + self.blink_period;
        self.is_animating = move_active || blink_active;
    }
}

/// Ease-out cubic: decelerating to zero velocity.
fn ease_out_cubic(t: f32) -> f32 {
    let t = t - 1.0;
    t * t * t + 1.0
}

/// Ease-in-out cubic: accelerate then decelerate.
fn ease_in_out_cubic(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        let t = 2.0 * t - 2.0;
        0.5 * t * t * t + 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_snap() {
        let mut anim = CursorAnimation::new();
        anim.update_target(5, 10);
        let state = anim.tick(10.0, 20.0);
        assert!((state.x - 50.0).abs() < 0.1);
        assert!((state.y - 200.0).abs() < 0.1);
        assert!((state.opacity - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_ease_out_cubic() {
        assert!((ease_out_cubic(0.0) - 0.0).abs() < 0.001);
        assert!((ease_out_cubic(1.0) - 1.0).abs() < 0.001);
        // Midpoint should be > 0.5 (ease-out is front-loaded)
        assert!(ease_out_cubic(0.5) > 0.5);
    }

    #[test]
    fn test_move_resets_blink() {
        let mut anim = CursorAnimation::new();
        anim.update_target(0, 0);
        // Simulate some time passing
        std::thread::sleep(std::time::Duration::from_millis(10));
        let state = anim.tick(10.0, 20.0);
        assert!((state.opacity - 1.0).abs() < 0.01);

        // Move to new position
        anim.update_target(5, 5);
        let state = anim.tick(10.0, 20.0);
        // Should be fully visible (just moved)
        assert!((state.opacity - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_animation_goes_idle() {
        let mut anim = CursorAnimation::new();
        anim.update_target(0, 0);
        anim.tick(10.0, 20.0);

        // After tick with initial snap, move_t = 1.0 and time_since_move ≈ 0.
        // Simulate enough time for blink cycle to complete.
        anim.time_since_move = DEFAULT_BLINK_DELAY + DEFAULT_BLINK_PERIOD + 0.1;
        anim.move_t = 1.0;
        anim.tick(10.0, 20.0);

        // Should no longer be animating
        assert!(!anim.is_animating);
        assert!(anim.next_frame_deadline().is_none());
    }

    #[test]
    fn test_next_frame_deadline_during_move() {
        let mut anim = CursorAnimation::new();
        anim.update_target(0, 0);
        anim.tick(10.0, 20.0);
        anim.update_target(5, 5);
        // move_t is now 0, so we're animating movement
        assert!(anim.is_animating);
        let deadline = anim.next_frame_deadline();
        assert!(deadline.is_some());
    }

    #[test]
    fn test_pause_resume() {
        let mut anim = CursorAnimation::new();
        anim.update_target(0, 0);
        anim.tick(10.0, 20.0);
        assert!(anim.is_animating);

        anim.pause();
        assert!(!anim.is_animating);
        assert!(anim.next_frame_deadline().is_none());

        anim.resume();
        // Should resume animating (time_since_move ≈ 0 < BLINK_DELAY + BLINK_PERIOD)
        assert!(anim.is_animating);
    }
}
