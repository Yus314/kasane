//! Display directive oscillation detection.
//!
//! When plugins read the previous frame's `DisplayMap` (via [`FrameworkAccess`])
//! and emit directives conditioned on it, a feedback loop can form:
//! frame N produces directives A, which produce map M₁; frame N+1 reads M₁
//! and produces directives B → map M₂; frame N+2 reads M₂ and produces A again.
//!
//! `DirectiveStabilityMonitor` detects 2-cycles and 3-cycles in the directive
//! stream and emits a tracing warning, allowing diagnostics without silently
//! corrupting display state.
//!
//! [`FrameworkAccess`]: crate::plugin::FrameworkAccess

use super::DisplayDirective;

/// Window size for cycle detection (detects up to (WINDOW-1)-cycles).
const WINDOW: usize = 4;

/// Monitors display directive stability across frames.
///
/// Stores the last few resolved directive sets and detects cycles
/// via `PartialEq` comparison.
#[derive(Debug, Clone)]
pub struct DirectiveStabilityMonitor {
    /// Ring buffer of recent directive sets.
    history: Vec<Vec<DisplayDirective>>,
    /// Number of frames recorded so far (saturates at WINDOW).
    count: usize,
}

impl Default for DirectiveStabilityMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl DirectiveStabilityMonitor {
    /// Create a new monitor with no history.
    pub fn new() -> Self {
        Self {
            history: Vec::with_capacity(WINDOW),
            count: 0,
        }
    }

    /// Record a frame's resolved directives and check for oscillation.
    ///
    /// Returns `true` if a cycle was detected (same directive set appeared
    /// in the recent window), `false` otherwise.
    pub fn record(&mut self, directives: &[DisplayDirective]) -> bool {
        let detected = self.detect_cycle(directives);
        let idx = self.count % WINDOW;
        if self.history.len() <= idx {
            self.history.push(directives.to_vec());
        } else {
            self.history[idx] = directives.to_vec();
        }
        self.count += 1;
        if detected {
            tracing::warn!(
                "display directive oscillation detected (directive set repeated within {WINDOW}-frame window)"
            );
        }
        detected
    }

    /// Reset the monitor (e.g. on buffer change where oscillation is expected).
    pub fn reset(&mut self) {
        self.history.clear();
        self.count = 0;
    }

    fn detect_cycle(&self, directives: &[DisplayDirective]) -> bool {
        // Need at least 2 frames of history to detect a cycle.
        if self.count < 2 {
            return false;
        }
        let filled = self.history.len();
        for i in 0..filled {
            if self.history[i] == directives {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Atom, Face};

    fn fold(start: usize, end: usize) -> DisplayDirective {
        DisplayDirective::Fold {
            range: start..end,
            summary: vec![Atom {
                face: Face::default(),
                contents: "...".into(),
            }],
        }
    }

    fn hide(start: usize, end: usize) -> DisplayDirective {
        DisplayDirective::Hide { range: start..end }
    }

    fn insert_after(after: usize) -> DisplayDirective {
        DisplayDirective::InsertAfter {
            after,
            content: vec![Atom {
                face: Face::default(),
                contents: "virtual".into(),
            }],
        }
    }

    #[test]
    fn no_cycle_on_first_two_frames() {
        let mut mon = DirectiveStabilityMonitor::new();
        let d = vec![fold(1, 3)];
        assert!(!mon.record(&d));
        assert!(!mon.record(&d));
    }

    #[test]
    fn stable_directives_detected_as_repeat() {
        let mut mon = DirectiveStabilityMonitor::new();
        let d = vec![fold(1, 3)];
        mon.record(&d);
        mon.record(&d);
        // Third frame with same directives: history contains two copies, match found
        assert!(mon.record(&d));
    }

    #[test]
    fn detects_2_cycle() {
        let mut mon = DirectiveStabilityMonitor::new();
        let a = vec![fold(1, 3)];
        let b = vec![hide(1, 3)];
        assert!(!mon.record(&a)); // frame 0: A
        assert!(!mon.record(&b)); // frame 1: B (no repeat yet)
        assert!(mon.record(&a)); // frame 2: A again → 2-cycle detected
    }

    #[test]
    fn detects_3_cycle() {
        let mut mon = DirectiveStabilityMonitor::new();
        let a = vec![fold(1, 3)];
        let b = vec![hide(1, 3)];
        let c = vec![insert_after(0)];
        assert!(!mon.record(&a)); // frame 0
        assert!(!mon.record(&b)); // frame 1
        assert!(!mon.record(&c)); // frame 2
        assert!(mon.record(&a)); // frame 3: A repeats ��� 3-cycle
    }

    #[test]
    fn no_false_positive_for_different_directives() {
        let mut mon = DirectiveStabilityMonitor::new();
        assert!(!mon.record(&[fold(1, 3)]));
        assert!(!mon.record(&[fold(2, 4)]));
        assert!(!mon.record(&[fold(3, 5)]));
        assert!(!mon.record(&[fold(4, 6)]));
    }

    #[test]
    fn reset_clears_history() {
        let mut mon = DirectiveStabilityMonitor::new();
        let a = vec![fold(1, 3)];
        let b = vec![hide(1, 3)];
        assert!(!mon.record(&a));
        assert!(!mon.record(&b));
        mon.reset();
        assert!(!mon.record(&a)); // no cycle because history was cleared
    }

    #[test]
    fn empty_directives_do_not_crash() {
        let mut mon = DirectiveStabilityMonitor::new();
        assert!(!mon.record(&[]));
        assert!(!mon.record(&[]));
    }

    #[test]
    fn window_evicts_old_entries() {
        let mut mon = DirectiveStabilityMonitor::new();
        let a = vec![fold(1, 3)];
        // Fill WINDOW slots with distinct directives
        for i in 0..WINDOW {
            mon.record(&[fold(i * 10, i * 10 + 2)]);
        }
        // A was evicted from history, so no cycle detected
        assert!(!mon.record(&a));
    }
}
