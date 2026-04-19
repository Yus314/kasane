use kasane_core::display::navigation::{ActionResult, NavigationAction, NavigationPolicy};
use kasane_core::display::projection::{ProjectionCategory, ProjectionDescriptor, ProjectionId};
use kasane_core::display::unit::{DisplayUnit, SemanticRole};
use kasane_core::display::{DisplayDirective, GutterSide, InlineInteraction, VirtualTextPosition};
use kasane_core::element::{Element, InteractiveId, PluginTag};

use crate::bindings::kasane::plugin::types as wit;

/// Convert a WIT `DisplayDirective` to a native `DisplayDirective`.
///
/// Variants with `element-handle` (InsertBefore, InsertAfter, Gutter) call the
/// provided `resolve_element` closure to materialize the handle into an `Element`.
/// The `plugin_tag` is used to construct `InteractiveId` for inline directives.
pub(crate) fn wit_display_directive_to_directive_with_resolver(
    directive: &wit::DisplayDirective,
    plugin_tag: PluginTag,
    resolve_element: &mut dyn FnMut(u32) -> Element,
) -> DisplayDirective {
    match directive {
        // === Spatial ===
        wit::DisplayDirective::Fold(fold) => DisplayDirective::Fold {
            range: fold.range_start as usize..fold.range_end as usize,
            summary: super::wit_atoms_to_atoms(&fold.summary),
        },
        wit::DisplayDirective::Hide(hide) => DisplayDirective::Hide {
            range: hide.range_start as usize..hide.range_end as usize,
        },

        // === InterLine ===
        wit::DisplayDirective::InsertBefore(d) => DisplayDirective::InsertBefore {
            line: d.line as usize,
            content: resolve_element(d.content),
            priority: d.priority,
        },
        wit::DisplayDirective::InsertAfter(d) => DisplayDirective::InsertAfter {
            line: d.line as usize,
            content: resolve_element(d.content),
            priority: d.priority,
        },

        // === Inline ===
        wit::DisplayDirective::InsertInline(d) => DisplayDirective::InsertInline {
            line: d.line as usize,
            byte_offset: d.byte_offset as usize,
            content: super::wit_atoms_to_atoms(&d.content),
            interaction: match d.interactive_id {
                Some(id) => InlineInteraction::Action(InteractiveId::new(id as u32, plugin_tag)),
                None => InlineInteraction::None,
            },
        },
        wit::DisplayDirective::HideInline(d) => DisplayDirective::HideInline {
            line: d.line as usize,
            byte_range: d.byte_start as usize..d.byte_end as usize,
        },
        wit::DisplayDirective::StyleInline(d) => DisplayDirective::StyleInline {
            line: d.line as usize,
            byte_range: d.byte_start as usize..d.byte_end as usize,
            face: super::wit_face_to_face(&d.face),
        },

        // === Decoration ===
        wit::DisplayDirective::StyleLine(d) => DisplayDirective::StyleLine {
            line: d.line as usize,
            face: super::wit_face_to_face(&d.face),
            z_order: d.z_order,
        },
        wit::DisplayDirective::Gutter(d) => DisplayDirective::Gutter {
            line: d.line as usize,
            side: match d.side {
                wit::DisplayGutterSide::Left => GutterSide::Left,
                wit::DisplayGutterSide::Right => GutterSide::Right,
            },
            content: resolve_element(d.content),
            priority: d.priority,
        },
        wit::DisplayDirective::VirtualText(d) => DisplayDirective::VirtualText {
            line: d.line as usize,
            position: match d.position {
                wit::DisplayVtPosition::EndOfLine => VirtualTextPosition::EndOfLine,
                wit::DisplayVtPosition::RightAligned => VirtualTextPosition::RightAligned,
            },
            content: super::wit_atoms_to_atoms(&d.content),
            priority: d.priority,
        },
    }
}

/// Convert a list of WIT directives, resolving element handles via the closure.
pub(crate) fn wit_display_directives_to_directives_with_resolver(
    directives: &[wit::DisplayDirective],
    plugin_tag: PluginTag,
    resolve_element: &mut dyn FnMut(u32) -> Element,
) -> Vec<DisplayDirective> {
    directives
        .iter()
        .map(|d| wit_display_directive_to_directive_with_resolver(d, plugin_tag, resolve_element))
        .collect()
}

/// Convenience wrapper for spatial-only directives (no element handles).
///
/// Panics if the directive contains an `element-handle` — use
/// [`wit_display_directive_to_directive_with_resolver`] instead for full conversion.
#[cfg(test)]
pub(crate) fn wit_display_directive_to_directive(
    directive: &wit::DisplayDirective,
) -> DisplayDirective {
    wit_display_directive_to_directive_with_resolver(
        directive,
        PluginTag::FRAMEWORK,
        &mut |handle| {
            panic!(
                "wit_display_directive_to_directive: encountered element-handle {handle} \
                 but no resolver was provided; use the _with_resolver variant"
            )
        },
    )
}

#[cfg(test)]
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
        // === Spatial ===
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

        // === InterLine ===
        DisplayDirective::InsertBefore {
            line,
            content: _,
            priority,
        } => wit::DisplayDirective::InsertBefore(wit::InterlineDirective {
            line: *line as u32,
            // Element cannot be round-tripped through WIT in tests — use placeholder handle 0.
            content: 0,
            priority: *priority,
        }),
        DisplayDirective::InsertAfter {
            line,
            content: _,
            priority,
        } => wit::DisplayDirective::InsertAfter(wit::InterlineDirective {
            line: *line as u32,
            content: 0,
            priority: *priority,
        }),

        // === Inline ===
        DisplayDirective::InsertInline {
            line,
            byte_offset,
            content,
            interaction,
        } => wit::DisplayDirective::InsertInline(wit::InsertInlineDirective {
            line: *line as u32,
            byte_offset: *byte_offset as u32,
            content: super::atoms_to_wit(content),
            interactive_id: match interaction {
                InlineInteraction::Action(id) => Some(id.local as u64),
                InlineInteraction::None => None,
            },
        }),
        DisplayDirective::HideInline { line, byte_range } => {
            wit::DisplayDirective::HideInline(wit::HideInlineDirective {
                line: *line as u32,
                byte_start: byte_range.start as u32,
                byte_end: byte_range.end as u32,
            })
        }
        DisplayDirective::StyleInline {
            line,
            byte_range,
            face,
        } => wit::DisplayDirective::StyleInline(wit::StyleInlineDirective {
            line: *line as u32,
            byte_start: byte_range.start as u32,
            byte_end: byte_range.end as u32,
            face: super::face_to_wit(face),
        }),

        // === Decoration ===
        DisplayDirective::StyleLine {
            line,
            face,
            z_order,
        } => wit::DisplayDirective::StyleLine(wit::StyleLineDirective {
            line: *line as u32,
            face: super::face_to_wit(face),
            z_order: *z_order,
        }),
        DisplayDirective::Gutter {
            line,
            side,
            content: _,
            priority,
        } => wit::DisplayDirective::Gutter(wit::GutterDirective {
            line: *line as u32,
            side: match side {
                GutterSide::Left => wit::DisplayGutterSide::Left,
                GutterSide::Right => wit::DisplayGutterSide::Right,
            },
            content: 0,
            priority: *priority,
        }),
        DisplayDirective::VirtualText {
            line,
            position,
            content,
            priority,
        } => wit::DisplayDirective::VirtualText(wit::VirtualTextDirective {
            line: *line as u32,
            position: match position {
                VirtualTextPosition::EndOfLine => wit::DisplayVtPosition::EndOfLine,
                VirtualTextPosition::RightAligned => wit::DisplayVtPosition::RightAligned,
            },
            content: super::atoms_to_wit(content),
            priority: *priority,
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
