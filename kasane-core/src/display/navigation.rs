//! Navigation policy types for display units.
//!
//! `NavigationPolicy` determines how the input system interacts with display
//! units during navigation. It is orthogonal to `InteractionPolicy` (which
//! governs rendering/cursor suppression): a fold summary has `ReadOnly`
//! interaction but `Boundary { ToggleFold }` navigation.

use crate::element::PluginTag;

use super::unit::SemanticRole;

/// Vertical direction for display-unit navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationDirection {
    Up,
    Down,
}

/// How navigation interacts with a display unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavigationPolicy {
    /// Standard navigation — cursor can be placed here.
    Normal,
    /// Skip during navigation — invisible to nav traversal.
    Skip,
    /// Navigation stops here. Activation triggers the associated action.
    Boundary { action: NavigationAction },
}

/// Action triggered when a Boundary unit is activated (click or keyboard).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavigationAction {
    /// Stop only, no special action.
    None,
    /// Toggle fold expansion/collapse.
    ToggleFold,
    /// Plugin-defined action (DU-4 scope).
    Plugin(PluginTag, u32),
}

/// Result of handling a navigation action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionResult {
    /// Action handled, no further processing.
    Handled,
    /// Emit key sequence to Kakoune.
    SendKeys(String),
    /// Not applicable, continue default processing.
    Pass,
}

impl NavigationPolicy {
    /// Default navigation policy based on `SemanticRole` (design doc §5.3).
    pub fn default_for(role: &SemanticRole) -> Self {
        match role {
            SemanticRole::BufferContent => NavigationPolicy::Normal,
            SemanticRole::FoldSummary => NavigationPolicy::Boundary {
                action: NavigationAction::ToggleFold,
            },
            SemanticRole::VirtualText => NavigationPolicy::Skip,
            SemanticRole::Plugin(_, _) => NavigationPolicy::Skip,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::PluginTag;

    #[test]
    fn default_policy_buffer_content() {
        assert_eq!(
            NavigationPolicy::default_for(&SemanticRole::BufferContent),
            NavigationPolicy::Normal,
        );
    }

    #[test]
    fn default_policy_fold_summary() {
        assert_eq!(
            NavigationPolicy::default_for(&SemanticRole::FoldSummary),
            NavigationPolicy::Boundary {
                action: NavigationAction::ToggleFold,
            },
        );
    }

    #[test]
    fn default_policy_virtual_text() {
        assert_eq!(
            NavigationPolicy::default_for(&SemanticRole::VirtualText),
            NavigationPolicy::Skip,
        );
    }

    #[test]
    fn default_policy_plugin() {
        assert_eq!(
            NavigationPolicy::default_for(&SemanticRole::Plugin(PluginTag(1), 42)),
            NavigationPolicy::Skip,
        );
    }
}
