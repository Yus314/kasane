//! Integration tests for Projection Mode (Plan Phases 1–10).
//!
//! Tests the full pipeline: projection descriptors → collection → DisplayMap,
//! including structural/additive activation, cursor safety, per-projection fold
//! state, and legacy fallback.

use super::*;
use crate::display::{
    BufferLine, DisplayDirective, DisplayLine, DisplayMap, FoldToggleState, ProjectionCategory,
    ProjectionDescriptor, ProjectionId, ProjectionPolicyState,
};
use crate::protocol::{Atom, Face};

// ---------------------------------------------------------------------------
// Test plugin: defines one Structural and one Additive projection
// ---------------------------------------------------------------------------

struct ProjectionTestPlugin {
    id: &'static str,
    descriptors: Vec<ProjectionDescriptor>,
    /// (projection_id, directives) pairs
    directive_map: Vec<(String, Vec<DisplayDirective>)>,
}

impl PluginBackend for ProjectionTestPlugin {
    fn id(&self) -> PluginId {
        PluginId(self.id.to_string())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::DISPLAY_TRANSFORM
    }

    fn projection_descriptors(&self) -> &[ProjectionDescriptor] {
        &self.descriptors
    }

    fn projection_directives(
        &self,
        id: &ProjectionId,
        _state: &AppView<'_>,
    ) -> Vec<DisplayDirective> {
        self.directive_map
            .iter()
            .find(|(k, _)| k == &*id.0)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    }
}

fn make_structural_descriptor(name: &str) -> ProjectionDescriptor {
    ProjectionDescriptor {
        id: ProjectionId::new(name),
        name: name.to_string(),
        category: ProjectionCategory::Structural,
        priority: -100,
    }
}

fn make_additive_descriptor(name: &str) -> ProjectionDescriptor {
    ProjectionDescriptor {
        id: ProjectionId::new(name),
        name: name.to_string(),
        category: ProjectionCategory::Additive,
        priority: 600,
    }
}

fn state_with_lines(n: usize) -> AppState {
    let mut state = AppState::default();
    state.observed.lines = (0..n).map(|_| vec![]).collect();
    state
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn no_projection_active_produces_identity_map() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(ProjectionTestPlugin {
        id: "proj-test",
        descriptors: vec![make_structural_descriptor("outline")],
        directive_map: vec![(
            "outline".into(),
            vec![DisplayDirective::Hide { range: 1..3 }],
        )],
    }));

    let state = state_with_lines(5);
    let dm = registry.view().collect_display_map(&AppView::new(&state));
    // No projection active → identity map
    assert_eq!(dm.display_line_count(), 5);
}

#[test]
fn structural_projection_active_applies_directives() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(ProjectionTestPlugin {
        id: "proj-test",
        descriptors: vec![make_structural_descriptor("outline")],
        directive_map: vec![(
            "outline".into(),
            vec![DisplayDirective::Hide { range: 1..3 }],
        )],
    }));

    let mut state = state_with_lines(5);
    state
        .config
        .projection_policy
        .set_structural(Some(ProjectionId::new("outline")));

    let dm = registry.view().collect_display_map(&AppView::new(&state));
    // 5 lines - 2 hidden (1,2) = 3 display lines
    assert_eq!(dm.display_line_count(), 3);
    assert_eq!(
        dm.buffer_to_display(BufferLine(1)),
        None,
        "line 1 should be hidden"
    );
    assert_eq!(
        dm.buffer_to_display(BufferLine(2)),
        None,
        "line 2 should be hidden"
    );
}

#[test]
fn additive_projection_active_applies_directives() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(ProjectionTestPlugin {
        id: "proj-test",
        descriptors: vec![make_additive_descriptor("error-lens")],
        directive_map: vec![(
            "error-lens".into(),
            vec![DisplayDirective::Fold {
                range: 1..3,
                summary: vec![Atom::from_face(Face::default(), "error: unused variable")],
            }],
        )],
    }));

    let mut state = state_with_lines(4);
    state
        .config
        .projection_policy
        .toggle_additive(ProjectionId::new("error-lens"));

    let dm = registry.view().collect_display_map(&AppView::new(&state));
    // 4 lines, fold 1..3 (2 lines → 1 summary) = 3 display lines
    assert_eq!(dm.display_line_count(), 3);
}

#[test]
fn structural_and_additive_compose() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(ProjectionTestPlugin {
        id: "proj-test",
        descriptors: vec![
            make_structural_descriptor("outline"),
            make_additive_descriptor("error-lens"),
        ],
        directive_map: vec![
            (
                "outline".into(),
                vec![DisplayDirective::Hide { range: 2..4 }],
            ),
            (
                "error-lens".into(),
                vec![DisplayDirective::Fold {
                    range: 0..2,
                    summary: vec![Atom::from_face(Face::default(), "folded")],
                }],
            ),
        ],
    }));

    let mut state = state_with_lines(5);
    state
        .config
        .projection_policy
        .set_structural(Some(ProjectionId::new("outline")));
    state
        .config
        .projection_policy
        .toggle_additive(ProjectionId::new("error-lens"));

    let dm = registry.view().collect_display_map(&AppView::new(&state));
    // 5 - 2 hidden = 3, then fold 0..2 (2 lines → 1) = 2 display lines
    assert_eq!(dm.display_line_count(), 2);
}

#[test]
fn inactive_projection_not_collected() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(ProjectionTestPlugin {
        id: "proj-test",
        descriptors: vec![
            make_structural_descriptor("outline"),
            make_structural_descriptor("focus"),
        ],
        directive_map: vec![
            (
                "outline".into(),
                vec![DisplayDirective::Hide { range: 1..3 }],
            ),
            ("focus".into(), vec![DisplayDirective::Hide { range: 3..5 }]),
        ],
    }));

    let mut state = state_with_lines(5);
    // Only activate "outline", not "focus"
    state
        .config
        .projection_policy
        .set_structural(Some(ProjectionId::new("outline")));

    let dm = registry.view().collect_display_map(&AppView::new(&state));
    // Only outline's Hide{1..3} applied: 5 - 2 = 3
    assert_eq!(dm.display_line_count(), 3);
    // Line 3 and 4 are NOT hidden (focus is inactive)
    assert!(dm.buffer_to_display(BufferLine(3)).is_some());
    assert!(dm.buffer_to_display(BufferLine(4)).is_some());
}

#[test]
fn cursor_safety_net_prevents_hiding_cursor_line() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(ProjectionTestPlugin {
        id: "proj-test",
        descriptors: vec![make_structural_descriptor("outline")],
        directive_map: vec![(
            "outline".into(),
            vec![DisplayDirective::Hide { range: 0..3 }],
        )],
    }));

    let mut state = state_with_lines(5);
    state.observed.cursor_pos.line = 1; // cursor on line 1
    state
        .config
        .projection_policy
        .set_structural(Some(ProjectionId::new("outline")));

    let dm = registry.view().collect_display_map(&AppView::new(&state));
    // Hide{0..3} covers cursor line 1, so the entire Hide directive is removed
    // by the cursor safety net (retain logic removes Hide if range contains cursor)
    assert_eq!(dm.display_line_count(), 5, "cursor line must not be hidden");
}

#[test]
fn legacy_plugin_without_projections_still_works() {
    use super::registry::DisplayTransformPlugin;

    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(DisplayTransformPlugin {
        id: "legacy",
        directives: vec![DisplayDirective::Hide { range: 1..2 }],
        priority: 0,
    }));

    let mut state = state_with_lines(4);

    let dm = registry.view().collect_display_map(&AppView::new(&state));
    // Legacy plugin (no projections) → directives collected regardless of policy
    assert_eq!(dm.display_line_count(), 3);
}

#[test]
fn collect_projection_descriptors_returns_all() {
    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(ProjectionTestPlugin {
        id: "proj-a",
        descriptors: vec![make_structural_descriptor("outline")],
        directive_map: vec![],
    }));
    registry.register_backend(Box::new(ProjectionTestPlugin {
        id: "proj-b",
        descriptors: vec![make_additive_descriptor("error-lens")],
        directive_map: vec![],
    }));

    let descriptors = registry.view().collect_projection_descriptors();
    assert_eq!(descriptors.len(), 2);
    assert!(descriptors.iter().any(|d| &*d.id.0 == "outline"));
    assert!(descriptors.iter().any(|d| &*d.id.0 == "error-lens"));
}

#[test]
fn per_projection_fold_state_scoping() {
    let mut policy = ProjectionPolicyState::default();
    let outline = ProjectionId::new("outline");
    let focus = ProjectionId::new("focus");

    // Set fold state for outline
    policy.set_structural(Some(outline.clone()));
    policy.fold_state_for_mut(&outline).toggle(&(1..3));

    // Switch to focus — outline fold state preserved
    policy.set_structural(Some(focus.clone()));
    assert!(*policy.fold_state_for(&focus) == FoldToggleState::empty());

    // Switch back to outline — fold state still there
    policy.set_structural(Some(outline.clone()));
    assert!(*policy.fold_state_for(&outline) != FoldToggleState::empty());
}

#[test]
fn projection_off_clears_active_but_preserves_fold() {
    let mut policy = ProjectionPolicyState::default();
    let outline = ProjectionId::new("outline");

    policy.set_structural(Some(outline.clone()));
    policy.fold_state_for_mut(&outline).toggle(&(1..3));
    policy.toggle_additive(ProjectionId::new("error-lens"));

    policy.clear_all();
    assert!(policy.active_structural().is_none());
    assert!(policy.active_additive().is_empty());
    // Fold state is preserved across clear_all
    assert!(*policy.fold_state_for(&outline) != FoldToggleState::empty());
}
