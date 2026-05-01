//! Builtin plugin that displays active projection mode in the status bar.
//!
//! Contributes to `SlotId::STATUS_RIGHT` with a label showing the active
//! structural projection and any active additive projections.
//! Examples: `[Outline]`, `[+ErrorLens]`, `[Outline +ErrorLens +DiffMarks]`.

use crate::element::Element;
use crate::plugin::app_view::AppView;
use crate::plugin::context::{ContribSizeHint, ContributeContext, Contribution};
use crate::plugin::traits::PluginBackend;
use crate::plugin::{PluginCapabilities, PluginId, SlotId};

/// Builtin plugin that displays active projection state in STATUS_RIGHT.
pub struct ProjectionStatusPlugin;

crate::impl_migrated_caps_default!(ProjectionStatusPlugin);

impl PluginBackend for ProjectionStatusPlugin {
    fn id(&self) -> PluginId {
        PluginId("kasane.builtin.projection-status".into())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::CONTRIBUTOR
    }

    fn contribute_to(
        &self,
        region: &SlotId,
        state: &AppView<'_>,
        _ctx: &ContributeContext,
    ) -> Option<Contribution> {
        if region != &SlotId::STATUS_RIGHT {
            return None;
        }

        let policy = state.projection_policy();
        let descriptors = state.available_projections();

        let structural = policy.active_structural();
        let additive = policy.active_additive();

        if structural.is_none() && additive.is_empty() {
            return None;
        }

        let mut parts = Vec::new();

        if let Some(id) = structural {
            let name = descriptors
                .iter()
                .find(|d| &d.id == id)
                .map(|d| d.name.as_str())
                .unwrap_or(&id.0);
            parts.push(name.to_string());
        }

        let mut additive_names: Vec<_> = additive
            .iter()
            .map(|id| {
                let name = descriptors
                    .iter()
                    .find(|d| &d.id == id)
                    .map(|d| d.name.as_str())
                    .unwrap_or(&id.0);
                format!("+{name}")
            })
            .collect();
        additive_names.sort();
        parts.extend(additive_names);

        let label = format!(" [{}] ", parts.join(" "));
        let style = state.status_default_style().clone();

        Some(Contribution {
            element: Element::text_with_style(&label, style),
            priority: 900,
            size_hint: ContribSizeHint::Auto,
        })
    }
}

/// Format the projection status label (for testing).
///
/// Returns `None` if no projections are active.
pub fn format_projection_label(
    structural_name: Option<&str>,
    additive_names: &[&str],
) -> Option<String> {
    if structural_name.is_none() && additive_names.is_empty() {
        return None;
    }

    let mut parts = Vec::new();
    if let Some(name) = structural_name {
        parts.push(name.to_string());
    }
    let mut sorted: Vec<_> = additive_names.iter().map(|n| format!("+{n}")).collect();
    sorted.sort();
    parts.extend(sorted);
    Some(format!(" [{}] ", parts.join(" ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_none_active() {
        assert_eq!(format_projection_label(None, &[]), None);
    }

    #[test]
    fn format_structural_only() {
        assert_eq!(
            format_projection_label(Some("Outline"), &[]),
            Some(" [Outline] ".to_string())
        );
    }

    #[test]
    fn format_additive_only() {
        assert_eq!(
            format_projection_label(None, &["ErrorLens"]),
            Some(" [+ErrorLens] ".to_string())
        );
    }

    #[test]
    fn format_both() {
        assert_eq!(
            format_projection_label(Some("Outline"), &["DiffMarks", "ErrorLens"]),
            Some(" [Outline +DiffMarks +ErrorLens] ".to_string())
        );
    }

    #[test]
    fn contribute_returns_none_when_inactive() {
        let state = crate::state::AppState::default();
        let view = AppView::new(&state);
        let ctx = ContributeContext::new(&view, None);
        let plugin = ProjectionStatusPlugin;
        assert!(
            plugin
                .contribute_to(&SlotId::STATUS_RIGHT, &view, &ctx)
                .is_none()
        );
    }

    #[test]
    fn contribute_shows_structural() {
        use crate::display::{ProjectionCategory, ProjectionDescriptor, ProjectionId};

        let mut state = crate::state::AppState::default();
        let proj_id = ProjectionId::new("outline");
        state.config.projection_policy.set_structural(Some(proj_id));
        state.runtime.available_projections = vec![ProjectionDescriptor {
            id: ProjectionId::new("outline"),
            name: "Outline".to_string(),
            category: ProjectionCategory::Structural,
            priority: -100,
        }];

        let view = AppView::new(&state);
        let ctx = ContributeContext::new(&view, None);
        let plugin = ProjectionStatusPlugin;
        let contrib = plugin
            .contribute_to(&SlotId::STATUS_RIGHT, &view, &ctx)
            .expect("should contribute when structural is active");
        // The element should be Text containing "[Outline]"
        match &contrib.element {
            Element::Text(s, _) => assert!(s.contains("Outline"), "got: {s}"),
            other => panic!("expected Text element, got: {other:?}"),
        }
    }
}
