//! View collection methods for [`PluginView`].
//!
//! Read-only collection logic split by axis:
//!
//! - [`contributions`] — slot-keyed contributions from `CONTRIBUTOR` plugins.
//! - [`transforms`] — transform chain dispatch (`TRANSFORMER`) plus menu-item transforms.
//! - [`annotations`] — per-line annotations (`ANNOTATOR`) and content annotations.
//! - [`display`] — display directives, display-map building, scroll-offset resolution.
//! - [`overlays`] — overlay collection and menu/info overlay resolution.
//! - [`ornaments`] — render-ornament collection (`RENDER_ORNAMENT`).
//!
//! All axis modules attach inherent `impl<'a> PluginView<'a> { ... }` blocks
//! that share the same `PluginView` defined in `super::registry::mod`.

mod annotations;
mod contributions;
mod display;
mod ornaments;
mod overlays;
mod transforms;

use crate::element::OverlayAnchor;
use crate::plugin::element_patch::ElementPatch;
use crate::plugin::{GutterSide, PluginId, TransformTarget};

use super::PluginSlot;

/// Convert `display::GutterSide` to `plugin::GutterSide`.
pub(super) fn display_gutter_to_plugin(side: crate::display::GutterSide) -> GutterSide {
    match side {
        crate::display::GutterSide::Left => GutterSide::Left,
        crate::display::GutterSide::Right => GutterSide::Right,
    }
}

/// Extract a `Rect` from an `OverlayAnchor` when deterministic without layout.
pub(super) fn overlay_anchor_rect(anchor: &OverlayAnchor) -> Option<crate::layout::Rect> {
    match anchor {
        OverlayAnchor::Absolute { x, y, w, h } => Some(crate::layout::Rect {
            x: *x,
            y: *y,
            w: *w,
            h: *h,
        }),
        // Fill and AnchorPoint need layout to determine the final rect.
        _ => None,
    }
}

/// Debug-only: detect potential transform conflicts from collected patches.
///
/// For native (patch-aware) plugins, scope is derived from `ElementPatch::scope()`.
/// For legacy plugins, scope is derived from `transform_descriptor()`.
///
/// Warns when:
/// - Multiple plugins declare `Replacement` scope for the same target
/// - Non-Identity transforms appear before a Replacement (they'll be absorbed)
#[cfg(debug_assertions)]
pub(super) fn detect_transform_conflicts_from_patches(
    entries: &[(usize, PluginId, Option<ElementPatch>)],
    slots: &[PluginSlot],
    target: &TransformTarget,
) {
    use crate::plugin::context::TransformScope;

    let mut replacement_count = 0;
    let mut replacement_plugin: Option<&PluginId> = None;
    let mut has_non_identity_before_replacement = false;
    let mut seen_non_identity = false;

    for (slot_idx, plugin_id, patch) in entries {
        let scope = if let Some(p) = patch {
            // Native plugin: derive scope from patch
            p.scope()
        } else {
            // Legacy plugin: use declared descriptor
            if let Some(desc) = slots[*slot_idx].backend.transform_descriptor() {
                if !desc.targets.contains(target) {
                    continue;
                }
                desc.scope
            } else {
                continue;
            }
        };

        match scope {
            TransformScope::Replacement => {
                replacement_count += 1;
                if seen_non_identity {
                    has_non_identity_before_replacement = true;
                }
                replacement_plugin = Some(plugin_id);
            }
            TransformScope::Identity => {}
            _ => {
                seen_non_identity = true;
            }
        }
    }

    if replacement_count > 1 {
        tracing::warn!(
            target: "kasane::plugin::transform",
            "Multiple plugins declare Replacement scope for {:?} — \
             only the last in the chain will take effect",
            target,
        );
    }
    if has_non_identity_before_replacement && let Some(pid) = replacement_plugin {
        tracing::warn!(
            target: "kasane::plugin::transform",
            "Non-identity transforms appear before Replacement by {:?} for {:?} — \
             those transforms will be absorbed",
            pid,
            target,
        );
    }
}

/// Check for transform conflicts given a list of (plugin_id, descriptor) pairs.
///
/// Extracted as a free function for unit-testability.
#[cfg(debug_assertions)]
#[allow(dead_code)] // used by tests in tests/compose.rs
pub(crate) fn check_transform_conflicts(
    descriptors: &[(PluginId, Option<crate::plugin::TransformDescriptor>)],
    target: &TransformTarget,
) {
    use crate::plugin::context::TransformScope;

    let mut replacement_count = 0;
    let mut replacement_plugin: Option<&PluginId> = None;
    let mut has_non_identity_before_replacement = false;
    let mut seen_non_identity = false;

    for (plugin_id, desc) in descriptors {
        let Some(desc) = desc else {
            continue;
        };
        // Only consider descriptors that mention this target
        if !desc.targets.contains(target) {
            continue;
        }
        match desc.scope {
            TransformScope::Replacement => {
                replacement_count += 1;
                if seen_non_identity {
                    has_non_identity_before_replacement = true;
                }
                replacement_plugin = Some(plugin_id);
            }
            TransformScope::Identity => {}
            _ => {
                seen_non_identity = true;
            }
        }
    }

    if replacement_count > 1 {
        tracing::warn!(
            target: "kasane::plugin::transform",
            "Multiple plugins declare Replacement scope for {:?} — \
             only the last in the chain will take effect",
            target,
        );
    }
    if has_non_identity_before_replacement && let Some(pid) = replacement_plugin {
        tracing::warn!(
            target: "kasane::plugin::transform",
            "Non-identity transforms appear before Replacement by {:?} for {:?} — \
             those transforms will be absorbed",
            pid,
            target,
        );
    }
}
