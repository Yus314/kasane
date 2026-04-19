//! Cursor animation adapter: wraps AnimationEngine with the CursorAnimation API.

use std::time::Instant;

use kasane_core::render::{BlinkHint, MovementHint};

use super::engine::AnimationEngine;
use super::track::{EasingFn, TrackId};

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

/// Cursor animation built on top of `AnimationEngine`.
///
/// Manages smooth cursor movement via engine tracks and blink via a
/// time-based sine wave (blink doesn't need track interpolation).
pub struct CursorAnimation {
    engine: AnimationEngine,
    /// Target position in cell coordinates.
    target_x: f32,
    target_y: f32,
    /// Whether the cursor has been initialized with a target.
    initialized: bool,
    /// Whether any animation is currently running.
    pub is_animating: bool,
    // Blink state (managed outside the engine — it's a periodic function, not interpolation)
    time_since_move: f32,
    last_frame: Instant,
    // Plugin-configurable parameters
    blink_enabled: bool,
    blink_delay: f32,
    blink_period: f32,
    min_opacity: f32,
    move_enabled: bool,
}

impl Default for CursorAnimation {
    fn default() -> Self {
        Self::new()
    }
}

impl CursorAnimation {
    pub fn new() -> Self {
        let mut engine = AnimationEngine::new();
        engine.register(
            TrackId::CURSOR_X,
            0.0,
            DEFAULT_MOVE_DURATION,
            EasingFn::EaseOut,
        );
        engine.register(
            TrackId::CURSOR_Y,
            0.0,
            DEFAULT_MOVE_DURATION,
            EasingFn::EaseOut,
        );

        CursorAnimation {
            engine,
            target_x: 0.0,
            target_y: 0.0,
            initialized: false,
            is_animating: true, // Start with blink animation
            time_since_move: 0.0,
            last_frame: Instant::now(),
            blink_enabled: true,
            blink_delay: DEFAULT_BLINK_DELAY,
            blink_period: DEFAULT_BLINK_PERIOD,
            min_opacity: DEFAULT_MIN_OPACITY,
            move_enabled: true,
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
            let dur = m.duration_ms as f32 / 1000.0;
            self.engine.set_duration(TrackId::CURSOR_X, dur);
            self.engine.set_duration(TrackId::CURSOR_Y, dur);
            let easing = EasingFn::from(m.easing);
            self.engine.set_easing(TrackId::CURSOR_X, easing);
            self.engine.set_easing(TrackId::CURSOR_Y, easing);
        }
    }

    /// Update the target cursor position (cell coordinates).
    pub fn update_target(&mut self, x: u16, y: u16) {
        let tx = x as f32;
        let ty = y as f32;
        if !self.initialized {
            // First target: snap immediately
            self.engine.snap(TrackId::CURSOR_X, tx);
            self.engine.snap(TrackId::CURSOR_Y, ty);
            self.target_x = tx;
            self.target_y = ty;
            self.initialized = true;
            return;
        }
        if (tx - self.target_x).abs() < 0.01 && (ty - self.target_y).abs() < 0.01 {
            return; // No change
        }
        self.target_x = tx;
        self.target_y = ty;
        if self.move_enabled {
            self.engine.set_target(TrackId::CURSOR_X, tx);
            self.engine.set_target(TrackId::CURSOR_Y, ty);
        } else {
            self.engine.snap(TrackId::CURSOR_X, tx);
            self.engine.snap(TrackId::CURSOR_Y, ty);
        }
        self.time_since_move = 0.0;
        self.is_animating = true;
    }

    /// Advance animation by one frame. Returns the cursor's pixel position and opacity.
    pub fn tick(&mut self, cell_width: f32, cell_height: f32) -> CursorRenderState {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;

        // Advance movement tracks
        let move_active = if self.move_enabled {
            self.engine.tick()
        } else {
            false
        };

        // Advance blink timer
        self.time_since_move += dt;

        // Compute opacity
        let opacity = if !self.blink_enabled || self.time_since_move < self.blink_delay {
            1.0
        } else {
            let blink_t = self.time_since_move - self.blink_delay;
            let period = self.blink_period.max(0.001);
            let phase = (blink_t / period * std::f32::consts::TAU).sin();
            let half_range = (1.0 - self.min_opacity) / 2.0;
            (self.min_opacity + half_range) + half_range * phase
        };

        // Determine if we need to keep animating
        let blink_active =
            self.blink_enabled && self.time_since_move < self.blink_delay + self.blink_period;
        self.is_animating = move_active || blink_active;

        let cx = self.engine.value(TrackId::CURSOR_X);
        let cy = self.engine.value(TrackId::CURSOR_Y);

        CursorRenderState {
            x: cx * cell_width,
            y: cy * cell_height,
            opacity,
        }
    }

    /// Compute the next instant at which the cursor animation needs a frame.
    pub fn next_frame_deadline(&self) -> Option<Instant> {
        if !self.is_animating {
            return None;
        }
        if self.engine.is_animating() {
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
        self.engine.pause_all();
    }

    /// Resume the animation after a pause, adjusting last_frame to avoid a time jump.
    pub fn resume(&mut self) {
        self.last_frame = Instant::now();
        self.engine.resume_all();
        let move_active = self.engine.is_animating();
        let blink_active =
            self.blink_enabled && self.time_since_move < self.blink_delay + self.blink_period;
        self.is_animating = move_active || blink_active;
    }

    /// Access the underlying animation engine for direct track manipulation.
    pub fn engine(&self) -> &AnimationEngine {
        &self.engine
    }

    /// Access the underlying animation engine mutably.
    pub fn engine_mut(&mut self) -> &mut AnimationEngine {
        &mut self.engine
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
    fn test_move_resets_blink() {
        let mut anim = CursorAnimation::new();
        anim.update_target(0, 0);
        std::thread::sleep(std::time::Duration::from_millis(10));
        let state = anim.tick(10.0, 20.0);
        assert!((state.opacity - 1.0).abs() < 0.01);

        anim.update_target(5, 5);
        let state = anim.tick(10.0, 20.0);
        assert!((state.opacity - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_animation_goes_idle() {
        let mut anim = CursorAnimation::new();
        anim.update_target(0, 0);
        anim.tick(10.0, 20.0);

        // Simulate enough time for blink cycle to complete
        anim.time_since_move = DEFAULT_BLINK_DELAY + DEFAULT_BLINK_PERIOD + 0.1;
        anim.tick(10.0, 20.0);

        assert!(!anim.is_animating);
        assert!(anim.next_frame_deadline().is_none());
    }

    #[test]
    fn test_next_frame_deadline_during_move() {
        let mut anim = CursorAnimation::new();
        anim.update_target(0, 0);
        anim.tick(10.0, 20.0);
        anim.update_target(5, 5);
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
        assert!(anim.is_animating);
    }

    #[test]
    fn test_easing_functions_via_engine() {
        use super::super::track::{ease_in_out_cubic, ease_out_cubic};
        assert!((ease_out_cubic(0.0) - 0.0).abs() < 0.001);
        assert!((ease_out_cubic(1.0) - 1.0).abs() < 0.001);
        assert!(ease_out_cubic(0.5) > 0.5);

        assert!((ease_in_out_cubic(0.0) - 0.0).abs() < 0.001);
        assert!((ease_in_out_cubic(1.0) - 1.0).abs() < 0.001);
    }
}
