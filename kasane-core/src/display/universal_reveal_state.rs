//! Host-managed universal reveal state.
//!
//! When enabled, all destructive display directives (`Hide`, `HideInline`)
//! are filtered out before the display algebra normalizes them. This
//! provides §10.2a-faithful recovery for every plugin's destructive
//! directives via a single host-owned toggle, analogous to
//! [`FoldToggleState`](super::fold_state::FoldToggleState) for `Fold`.
//!
//! Filtering happens *pre-algebra* in
//! `kasane-core/src/plugin/registry/collection/display.rs::collect_tagged_display_directives`
//! so that decorations (`StyleInline`, etc.) that would have been
//! displaced by a destructive winner survive on reveal.

/// Tracks whether the user has requested universal reveal of all
/// destructive display directives.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UniversalRevealState {
    reveal_all: bool,
}

impl UniversalRevealState {
    /// Create an empty state (const-compatible).
    pub const fn empty() -> Self {
        Self { reveal_all: false }
    }

    /// Toggle reveal-all on/off.
    pub fn toggle(&mut self) {
        self.reveal_all = !self.reveal_all;
    }

    /// Whether destructive directives are currently being revealed.
    pub fn is_revealed(&self) -> bool {
        self.reveal_all
    }

    /// Explicitly set the reveal flag (used by tests and config import).
    pub fn set(&mut self, revealed: bool) {
        self.reveal_all = revealed;
    }

    /// Clear back to the default (hidden) state.
    pub fn clear(&mut self) {
        self.reveal_all = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_not_revealed() {
        let state = UniversalRevealState::default();
        assert!(!state.is_revealed());
    }

    #[test]
    fn toggle_flips_state() {
        let mut state = UniversalRevealState::default();
        state.toggle();
        assert!(state.is_revealed());
        state.toggle();
        assert!(!state.is_revealed());
    }

    #[test]
    fn toggle_is_idempotent_after_two_calls() {
        let mut state = UniversalRevealState::default();
        let initial = state.is_revealed();
        state.toggle();
        state.toggle();
        assert_eq!(state.is_revealed(), initial);
    }

    #[test]
    fn set_overrides_state() {
        let mut state = UniversalRevealState::default();
        state.set(true);
        assert!(state.is_revealed());
        state.set(false);
        assert!(!state.is_revealed());
    }

    #[test]
    fn clear_resets_to_default() {
        let mut state = UniversalRevealState::default();
        state.toggle();
        state.clear();
        assert!(!state.is_revealed());
    }
}
