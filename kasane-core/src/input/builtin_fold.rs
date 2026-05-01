//! Built-in fold toggle plugin.
//!
//! Handles `NavigationAction::ToggleFold` by returning `ActionResult::ToggleFold`,
//! which the `update` layer applies to the appropriate fold state.
//!
//! Registered as the lowest-priority plugin so that any user plugin can
//! override fold toggle behavior via `navigation_action()`.

use crate::display::navigation::{ActionResult, NavigationAction};
use crate::display::unit::{DisplayUnit, UnitSource};
use crate::plugin::{PluginBackend, PluginCapabilities, PluginId};

/// Built-in plugin for fold toggle handling.
///
/// Moves the fold toggle fallback from `update.rs` into a proper plugin,
/// making it overridable by user plugins registered at higher priority.
pub struct BuiltinFoldPlugin;

crate::impl_migrated_caps_default!(BuiltinFoldPlugin);

impl PluginBackend for BuiltinFoldPlugin {
    fn id(&self) -> PluginId {
        PluginId("kasane.builtin.fold".into())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::NAVIGATION_ACTION
    }

    fn navigation_action(
        &mut self,
        unit: &DisplayUnit,
        action: NavigationAction,
    ) -> Option<ActionResult> {
        if let NavigationAction::ToggleFold = action
            && let UnitSource::LineRange(ref range) = unit.source
        {
            return Some(ActionResult::ToggleFold(range.clone()));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::InteractionPolicy;
    use crate::display::unit::{DisplayUnit, DisplayUnitId, SemanticRole, UnitSource};

    fn make_fold_unit(range: std::ops::Range<usize>) -> DisplayUnit {
        let source = UnitSource::LineRange(range);
        let role = SemanticRole::FoldSummary;
        DisplayUnit {
            id: DisplayUnitId::from_content(&source, &role),
            display_line: 0,
            role,
            source,
            interaction: InteractionPolicy::ReadOnly,
        }
    }

    #[test]
    fn toggle_fold_returns_range() {
        let mut plugin = BuiltinFoldPlugin;
        let unit = make_fold_unit(2..5);
        let result = plugin.navigation_action(&unit, NavigationAction::ToggleFold);
        assert_eq!(result, Some(ActionResult::ToggleFold(2..5)));
    }

    #[test]
    fn non_fold_action_passes() {
        let mut plugin = BuiltinFoldPlugin;
        let unit = make_fold_unit(2..5);
        let result = plugin.navigation_action(&unit, NavigationAction::None);
        assert!(result.is_none());
    }

    #[test]
    fn non_range_source_passes() {
        let mut plugin = BuiltinFoldPlugin;
        let source = UnitSource::Line(3);
        let role = SemanticRole::BufferContent;
        let unit = DisplayUnit {
            id: DisplayUnitId::from_content(&source, &role),
            display_line: 3,
            role,
            source,
            interaction: InteractionPolicy::Normal,
        };
        let result = plugin.navigation_action(&unit, NavigationAction::ToggleFold);
        assert!(result.is_none());
    }
}
