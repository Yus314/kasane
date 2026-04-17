//! Projection Mode — named display transformation strategies.
//!
//! Two categories:
//! - **Structural** (Fold/Hide, mutually exclusive — at most one active): e.g. Outline, Focus
//! - **Additive** (InsertAfter/InsertBefore, composable — any number active): e.g. Error Lens, Diff Marks
//!
//! Priority bands:
//! - Structural: -500..0 (Outline: -100, Focus: -200)
//! - Ambient/Legacy: 0..500 (default: 0)
//! - Additive: 500..1000 (Error Lens: 600, Diff Marks: 700)

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::fold_state::FoldToggleState;

/// Unique identifier for a projection mode.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProjectionId(pub Arc<str>);

impl ProjectionId {
    /// Create a new `ProjectionId` from a string.
    pub fn new(name: impl Into<Arc<str>>) -> Self {
        Self(name.into())
    }
}

/// Whether a projection is structural (mutually exclusive) or additive (composable).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectionCategory {
    /// Fold/Hide projections — at most one active at a time.
    Structural,
    /// InsertAfter/InsertBefore projections — any number active simultaneously.
    Additive,
}

/// Metadata describing a projection mode.
#[derive(Clone, Debug)]
pub struct ProjectionDescriptor {
    pub id: ProjectionId,
    pub name: String,
    pub category: ProjectionCategory,
    pub priority: i16,
}

/// Tracks which projections are active and per-projection fold toggle state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProjectionPolicyState {
    active_structural: Option<ProjectionId>,
    active_additive: HashSet<ProjectionId>,
    fold_states: HashMap<ProjectionId, FoldToggleState>,
}

impl ProjectionPolicyState {
    /// Set the active structural projection (at most one).
    ///
    /// Pass `None` to deactivate the current structural projection.
    pub fn set_structural(&mut self, id: Option<ProjectionId>) {
        self.active_structural = id;
    }

    /// Toggle an additive projection on/off.
    pub fn toggle_additive(&mut self, id: ProjectionId) {
        if !self.active_additive.remove(&id) {
            self.active_additive.insert(id);
        }
    }

    /// Deactivate all projections (preserves fold states).
    pub fn clear_all(&mut self) {
        self.active_structural = None;
        self.active_additive.clear();
    }

    /// Whether the given projection is currently active.
    pub fn is_active(&self, id: &ProjectionId) -> bool {
        self.active_structural.as_ref() == Some(id) || self.active_additive.contains(id)
    }

    /// The currently active structural projection, if any.
    pub fn active_structural(&self) -> Option<&ProjectionId> {
        self.active_structural.as_ref()
    }

    /// The set of currently active additive projections.
    pub fn active_additive(&self) -> &HashSet<ProjectionId> {
        &self.active_additive
    }

    /// Get the fold toggle state for a projection (default if absent).
    pub fn fold_state_for(&self, id: &ProjectionId) -> &FoldToggleState {
        static DEFAULT: FoldToggleState = FoldToggleState::empty();
        self.fold_states.get(id).unwrap_or(&DEFAULT)
    }

    /// Get a mutable reference to the fold toggle state for a projection,
    /// creating a default entry if absent.
    pub fn fold_state_for_mut(&mut self, id: &ProjectionId) -> &mut FoldToggleState {
        self.fold_states.entry(id.clone()).or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(name: &str) -> ProjectionId {
        ProjectionId::new(name)
    }

    #[test]
    fn at_most_one_structural() {
        let mut state = ProjectionPolicyState::default();
        state.set_structural(Some(id("outline")));
        assert!(state.is_active(&id("outline")));

        state.set_structural(Some(id("focus")));
        assert!(!state.is_active(&id("outline")));
        assert!(state.is_active(&id("focus")));

        state.set_structural(None);
        assert!(!state.is_active(&id("focus")));
    }

    #[test]
    fn toggle_additive() {
        let mut state = ProjectionPolicyState::default();
        state.toggle_additive(id("error-lens"));
        assert!(state.is_active(&id("error-lens")));

        state.toggle_additive(id("diff-marks"));
        assert!(state.is_active(&id("error-lens")));
        assert!(state.is_active(&id("diff-marks")));

        // Toggle off
        state.toggle_additive(id("error-lens"));
        assert!(!state.is_active(&id("error-lens")));
        assert!(state.is_active(&id("diff-marks")));
    }

    #[test]
    fn clear_all_preserves_fold_states() {
        let mut state = ProjectionPolicyState::default();
        state.set_structural(Some(id("outline")));
        state.toggle_additive(id("error-lens"));
        state.fold_state_for_mut(&id("outline")).toggle(&(5..10));

        state.clear_all();

        assert!(state.active_structural().is_none());
        assert!(state.active_additive().is_empty());
        // Fold state preserved
        assert!(state.fold_state_for(&id("outline")).is_expanded(&(5..10)));
    }

    #[test]
    fn fold_state_for_lazy_init() {
        let state = ProjectionPolicyState::default();
        // Should return default (empty) fold state
        let fold = state.fold_state_for(&id("outline"));
        assert!(!fold.is_expanded(&(0..5)));
    }

    #[test]
    fn fold_state_for_mut_creates_entry() {
        let mut state = ProjectionPolicyState::default();
        state.fold_state_for_mut(&id("outline")).toggle(&(2..5));
        assert!(state.fold_state_for(&id("outline")).is_expanded(&(2..5)));
    }

    #[test]
    fn structural_and_additive_independent() {
        let mut state = ProjectionPolicyState::default();
        state.set_structural(Some(id("outline")));
        state.toggle_additive(id("error-lens"));

        assert!(state.is_active(&id("outline")));
        assert!(state.is_active(&id("error-lens")));

        // Clearing structural doesn't affect additive
        state.set_structural(None);
        assert!(state.is_active(&id("error-lens")));
    }

    #[test]
    fn per_projection_fold_state_preserved_on_switch() {
        let mut state = ProjectionPolicyState::default();
        state.set_structural(Some(id("outline")));
        state.fold_state_for_mut(&id("outline")).toggle(&(5..10));

        // Switch to focus
        state.set_structural(Some(id("focus")));
        state.fold_state_for_mut(&id("focus")).toggle(&(20..30));

        // Switch back
        state.set_structural(Some(id("outline")));
        assert!(state.fold_state_for(&id("outline")).is_expanded(&(5..10)));
        assert!(state.fold_state_for(&id("focus")).is_expanded(&(20..30)));
    }
}
