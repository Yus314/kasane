//! Animations: cursor movement easing, smooth scroll, blink cycle.

use std::time::Instant;

/// Duration of cursor move animation (ease-out).
const MOVE_DURATION: f32 = 0.1; // 100ms
/// Delay after movement before blinking starts.
const BLINK_DELAY: f32 = 0.5; // 500ms
/// Period of one full blink cycle.
const BLINK_PERIOD: f32 = 1.0; // 1s

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
        if self.move_t < 1.0 {
            self.move_t += dt / MOVE_DURATION;
            if self.move_t >= 1.0 {
                self.move_t = 1.0;
            }
            let t = ease_out_cubic(self.move_t);
            self.current_x = self.prev_x + (self.target_x - self.prev_x) * t;
            self.current_y = self.prev_y + (self.target_y - self.prev_y) * t;
        } else {
            self.current_x = self.target_x;
            self.current_y = self.target_y;
        }

        // Advance blink timer
        self.time_since_move += dt;

        // Compute opacity
        let opacity = if self.time_since_move < BLINK_DELAY {
            1.0 // Always visible right after movement
        } else {
            let blink_t = self.time_since_move - BLINK_DELAY;
            let phase = (blink_t / BLINK_PERIOD * std::f32::consts::TAU).sin();
            // Map sin(-1..1) to opacity(0.3..1.0) for a gentle blink
            0.65 + 0.35 * phase
        };

        // Determine if we need to keep animating
        self.is_animating = self.move_t < 1.0 || self.time_since_move < BLINK_DELAY + BLINK_PERIOD;
        // Actually, we always want blink to continue, so keep animating
        self.is_animating = true;

        CursorRenderState {
            x: self.current_x * cell_width,
            y: self.current_y * cell_height,
            opacity,
        }
    }
}

/// Ease-out cubic: decelerating to zero velocity.
fn ease_out_cubic(t: f32) -> f32 {
    let t = t - 1.0;
    t * t * t + 1.0
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
}
