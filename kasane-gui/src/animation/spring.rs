//! Spring physics for smooth inertial scrolling.
//!
//! Uses a semi-implicit Euler integrator for stability. The spring converges
//! to its target when both position error and velocity are below `rest_threshold`.

/// Spring physics simulation with damped harmonic oscillator.
#[derive(Debug, Clone)]
pub struct SpringPhysics {
    pub position: f64,
    pub velocity: f64,
    pub target: f64,
    pub stiffness: f64,
    pub damping: f64,
    pub mass: f64,
    pub rest_threshold: f64,
}

impl SpringPhysics {
    /// Create a critically-damped spring (no oscillation, fastest convergence).
    pub fn critically_damped(stiffness: f64) -> Self {
        let damping = 2.0 * (stiffness).sqrt();
        Self {
            position: 0.0,
            velocity: 0.0,
            target: 0.0,
            stiffness,
            damping,
            mass: 1.0,
            rest_threshold: 0.5,
        }
    }

    /// Create an underdamped spring (slight bounce).
    pub fn underdamped(stiffness: f64, damping_ratio: f64) -> Self {
        let damping = 2.0 * damping_ratio * (stiffness).sqrt();
        Self {
            position: 0.0,
            velocity: 0.0,
            target: 0.0,
            stiffness,
            damping,
            mass: 1.0,
            rest_threshold: 0.5,
        }
    }

    /// Returns `true` if the spring is at rest (converged to target).
    pub fn is_at_rest(&self) -> bool {
        let error = (self.position - self.target).abs();
        error < self.rest_threshold && self.velocity.abs() < self.rest_threshold
    }

    /// Advance the spring by `dt` seconds (semi-implicit Euler).
    /// Returns `true` if still active (not at rest).
    pub fn tick(&mut self, dt: f64) -> bool {
        if self.is_at_rest() {
            self.position = self.target;
            self.velocity = 0.0;
            return false;
        }

        // Clamp dt to avoid explosion from large time steps
        let dt = dt.min(1.0 / 30.0);

        let displacement = self.position - self.target;
        let spring_force = -self.stiffness * displacement;
        let damping_force = -self.damping * self.velocity;
        let acceleration = (spring_force + damping_force) / self.mass;

        // Semi-implicit Euler: update velocity first, then position
        self.velocity += acceleration * dt;
        self.position += self.velocity * dt;

        !self.is_at_rest()
    }

    /// Set a new target position.
    pub fn set_target(&mut self, target: f64) {
        self.target = target;
    }

    /// Add to the current target (for incremental scroll input).
    pub fn add_target(&mut self, delta: f64) {
        self.target += delta;
    }

    /// Snap immediately to the target (no animation).
    pub fn snap(&mut self, value: f64) {
        self.position = value;
        self.velocity = 0.0;
        self.target = value;
    }

    /// Reset position and target to zero.
    pub fn reset(&mut self) {
        self.position = 0.0;
        self.velocity = 0.0;
        self.target = 0.0;
    }
}

impl Default for SpringPhysics {
    fn default() -> Self {
        Self::critically_damped(300.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn critically_damped_converges() {
        let mut spring = SpringPhysics::critically_damped(300.0);
        spring.set_target(100.0);

        let dt = 1.0 / 60.0;
        let mut frames = 0;
        while !spring.is_at_rest() && frames < 600 {
            spring.tick(dt);
            frames += 1;
        }

        assert!(
            spring.is_at_rest(),
            "should converge within 10 seconds ({frames} frames), pos={}, vel={}",
            spring.position,
            spring.velocity
        );
        assert!(
            (spring.position - 100.0).abs() < 1.0,
            "should be near target, got {}",
            spring.position
        );
    }

    #[test]
    fn underdamped_oscillates_then_converges() {
        let mut spring = SpringPhysics::underdamped(300.0, 0.5);
        spring.set_target(100.0);

        let dt = 1.0 / 60.0;
        let mut overshot = false;
        let mut frames = 0;
        while !spring.is_at_rest() && frames < 600 {
            spring.tick(dt);
            if spring.position > 100.0 {
                overshot = true;
            }
            frames += 1;
        }

        assert!(overshot, "underdamped spring should overshoot");
        assert!(spring.is_at_rest(), "should eventually converge");
    }

    #[test]
    fn snap_is_immediate() {
        let mut spring = SpringPhysics::default();
        spring.set_target(50.0);
        spring.tick(1.0 / 60.0);

        spring.snap(200.0);
        assert!((spring.position - 200.0).abs() < 0.001);
        assert!(spring.is_at_rest());
    }

    #[test]
    fn add_target_accumulates() {
        let mut spring = SpringPhysics::default();
        spring.add_target(10.0);
        spring.add_target(20.0);
        assert!((spring.target - 30.0).abs() < 0.001);
    }

    #[test]
    fn at_rest_initially() {
        let spring = SpringPhysics::default();
        assert!(spring.is_at_rest());
    }

    #[test]
    fn large_dt_clamped() {
        let mut spring = SpringPhysics::critically_damped(300.0);
        spring.set_target(100.0);
        // Even with a huge dt, shouldn't explode
        spring.tick(10.0);
        assert!(
            spring.position.abs() < 1000.0,
            "position should be bounded, got {}",
            spring.position
        );
    }

    #[test]
    fn reset_zeroes() {
        let mut spring = SpringPhysics::default();
        spring.set_target(100.0);
        spring.tick(1.0 / 60.0);
        spring.reset();
        assert!(spring.is_at_rest());
        assert!((spring.position).abs() < 0.001);
    }
}
