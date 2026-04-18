use kasane_core::display::DisplayDirective;
use kasane_core::display::navigation::{ActionResult, NavigationAction, NavigationPolicy};
use kasane_core::display::projection::{ProjectionCategory, ProjectionDescriptor, ProjectionId};
use kasane_core::display::unit::{DisplayUnit, SemanticRole};

use crate::bindings::kasane::plugin::types as wit;

pub(crate) fn wit_display_directive_to_directive(
    directive: &wit::DisplayDirective,
) -> DisplayDirective {
    match directive {
        wit::DisplayDirective::Fold(fold) => DisplayDirective::Fold {
            range: fold.range_start as usize..fold.range_end as usize,
            summary: super::wit_atoms_to_atoms(&fold.summary),
        },
        wit::DisplayDirective::Hide(hide) => DisplayDirective::Hide {
            range: hide.range_start as usize..hide.range_end as usize,
        },
    }
}

pub(crate) fn wit_display_directives_to_directives(
    directives: &[wit::DisplayDirective],
) -> Vec<DisplayDirective> {
    directives
        .iter()
        .map(wit_display_directive_to_directive)
        .collect()
}

#[cfg(test)]
pub(crate) fn display_directive_to_wit(directive: &DisplayDirective) -> wit::DisplayDirective {
    match directive {
        DisplayDirective::Fold { range, summary } => {
            wit::DisplayDirective::Fold(wit::FoldDirective {
                range_start: range.start as u32,
                range_end: range.end as u32,
                summary: super::atoms_to_wit(summary),
            })
        }
        DisplayDirective::Hide { range } => wit::DisplayDirective::Hide(wit::HideDirective {
            range_start: range.start as u32,
            range_end: range.end as u32,
        }),
    }
}

#[cfg(test)]
pub(crate) fn display_directives_to_wit(
    directives: &[DisplayDirective],
) -> Vec<wit::DisplayDirective> {
    directives.iter().map(display_directive_to_wit).collect()
}

// === DU-4: Display unit converters ===

/// Convert a native `DisplayUnit` to a WIT `DisplayUnitInfo`.
pub(crate) fn display_unit_to_wit(unit: &DisplayUnit) -> wit::DisplayUnitInfo {
    let (role, plugin_tag, role_id) = match &unit.role {
        SemanticRole::BufferContent => (wit::SemanticRole::BufferContent, None, 0),
        SemanticRole::FoldSummary => (wit::SemanticRole::FoldSummary, None, 0),
        SemanticRole::Plugin(tag, id) => {
            (wit::SemanticRole::PluginDefined, Some(tag.0 as u32), *id)
        }
    };
    wit::DisplayUnitInfo {
        display_line: unit.display_line as u32,
        role,
        plugin_tag,
        role_id,
    }
}

/// Convert a WIT `NavigationPolicyKind` to a native `NavigationPolicy`.
pub(crate) fn wit_navigation_policy_to_policy(p: wit::NavigationPolicyKind) -> NavigationPolicy {
    match p {
        wit::NavigationPolicyKind::Normal => NavigationPolicy::Normal,
        wit::NavigationPolicyKind::Skip => NavigationPolicy::Skip,
        wit::NavigationPolicyKind::Boundary => NavigationPolicy::Boundary {
            action: NavigationAction::None,
        },
    }
}

/// Convert a WIT `NavigationActionResult` to a native `ActionResult`.
pub(crate) fn wit_action_result_to_action_result(r: wit::NavigationActionResult) -> ActionResult {
    if r.handled {
        if let Some(keys) = r.keys {
            ActionResult::SendKeys(keys)
        } else {
            ActionResult::Handled
        }
    } else {
        ActionResult::Pass
    }
}

/// Encode a `NavigationAction` to a u32 for the WIT `action-kind` parameter.
pub(crate) fn navigation_action_to_wit_kind(action: &NavigationAction) -> u32 {
    match action {
        NavigationAction::None => 0,
        NavigationAction::ToggleFold => 1,
        NavigationAction::Plugin(_tag, id) => 2 + id,
    }
}

// === Projection Mode converters ===

fn wit_projection_category_to_category(c: &wit::ProjectionCategory) -> ProjectionCategory {
    match c {
        wit::ProjectionCategory::Structural => ProjectionCategory::Structural,
        wit::ProjectionCategory::Additive => ProjectionCategory::Additive,
    }
}

pub(crate) fn wit_projection_descriptor_to_descriptor(
    w: &wit::ProjectionDescriptor,
) -> ProjectionDescriptor {
    ProjectionDescriptor {
        id: ProjectionId::new(w.id.as_str()),
        name: w.name.clone(),
        category: wit_projection_category_to_category(&w.category),
        priority: w.priority,
    }
}

pub(crate) fn wit_projection_descriptors_to_descriptors(
    descs: &[wit::ProjectionDescriptor],
) -> Vec<ProjectionDescriptor> {
    descs
        .iter()
        .map(wit_projection_descriptor_to_descriptor)
        .collect()
}
