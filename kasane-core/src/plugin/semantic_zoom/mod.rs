//! Semantic Zoom — progressive code abstraction via display directives.
//!
//! Registers as a single **Structural projection** (`kasane.semantic-zoom`) via
//! the `Plugin` trait's `define_projection` API. Six zoom levels (0–5) generate
//! `DisplayDirective`s (Fold, Hide, StyleInline, VirtualText, Gutter) through
//! the existing display pipeline.

mod indent_strategy;
mod syntax_strategy;

use std::fmt;

use crate::display::{DisplayDirective, ProjectionCategory, ProjectionDescriptor, ProjectionId};
use crate::input::{Key, KeyEvent, KeyPattern, KeyResponse, Modifiers};
use crate::plugin::app_view::AppView;
use crate::plugin::handler_registry::HandlerRegistry;
use crate::plugin::state::Plugin;
use crate::plugin::{Command, PluginId};
use crate::state::DirtyFlags;

// =============================================================================
// ZoomLevel
// =============================================================================

/// Semantic zoom level (0–5).
///
/// - 0 (RAW): Identity — no directives emitted.
/// - 1 (ANNOTATED): Scope/type hints via `StyleInline`.
/// - 2 (COMPRESSED): Fold nested blocks.
/// - 3 (OUTLINE): Hide non-declaration lines.
/// - 4 (SKELETON): Show only signatures.
/// - 5 (MAP): Module-level overview (deferred).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ZoomLevel(pub u8);

impl ZoomLevel {
    pub const RAW: Self = Self(0);
    pub const ANNOTATED: Self = Self(1);
    pub const COMPRESSED: Self = Self(2);
    pub const OUTLINE: Self = Self(3);
    pub const SKELETON: Self = Self(4);
    pub const MAP: Self = Self(5);

    const MIN: u8 = 0;
    const MAX: u8 = 5;

    /// Increase zoom level by 1, saturating at MAX.
    #[must_use]
    pub fn up(self) -> Self {
        Self(self.0.saturating_add(1).min(Self::MAX))
    }

    /// Decrease zoom level by 1, saturating at MIN.
    #[must_use]
    pub fn down(self) -> Self {
        Self(self.0.saturating_sub(1))
    }

    /// Clamp to valid range.
    #[must_use]
    pub fn clamp(self) -> Self {
        Self(self.0.clamp(Self::MIN, Self::MAX))
    }
}

impl fmt::Display for ZoomLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self.0 {
            0 => "Raw",
            1 => "Annotated",
            2 => "Compressed",
            3 => "Outline",
            4 => "Skeleton",
            5 => "Map",
            n => return write!(f, "Level {n}"),
        };
        f.write_str(label)
    }
}

// =============================================================================
// Plugin state
// =============================================================================

/// Externalized state for the Semantic Zoom plugin.
#[derive(Debug, Clone, PartialEq, Hash, Default)]
pub struct SemanticZoomState {
    pub level: ZoomLevel,
}

// =============================================================================
// Projection ID
// =============================================================================

const PROJECTION_ID_STR: &str = "kasane.semantic-zoom";

fn projection_id() -> ProjectionId {
    ProjectionId::new(PROJECTION_ID_STR)
}

// =============================================================================
// Directive generation
// =============================================================================

fn generate_zoom_directives(state: &SemanticZoomState, app: &AppView<'_>) -> Vec<DisplayDirective> {
    if state.level == ZoomLevel::RAW {
        return vec![];
    }

    let has_syntax = app
        .syntax_provider()
        .map(|sp| sp.generation() > 0)
        .unwrap_or(false);

    if has_syntax {
        syntax_strategy::syntax_directives(state, app)
    } else {
        indent_strategy::indent_directives(state.level, app.lines())
    }
}

// =============================================================================
// Plugin
// =============================================================================

/// Semantic Zoom plugin — progressive code abstraction.
pub struct SemanticZoomPlugin;

impl Plugin for SemanticZoomPlugin {
    type State = SemanticZoomState;

    fn id(&self) -> PluginId {
        PluginId(PROJECTION_ID_STR.to_string())
    }

    fn register(&self, r: &mut HandlerRegistry<Self::State>) {
        r.declare_interests(DirtyFlags::BUFFER_CONTENT | DirtyFlags::OPTIONS);

        r.define_projection(
            ProjectionDescriptor {
                id: projection_id(),
                name: "Semantic Zoom".to_string(),
                category: ProjectionCategory::Structural,
                priority: -50,
            },
            generate_zoom_directives,
        );

        r.on_key_map(|km| {
            km.group(
                "semantic-zoom",
                |_state: &SemanticZoomState| true,
                |g| {
                    g.bind(
                        KeyPattern::Exact(KeyEvent {
                            key: Key::Char('+'),
                            modifiers: Modifiers::CTRL,
                        }),
                        "zoom_in",
                    );
                    g.bind(
                        KeyPattern::Exact(KeyEvent {
                            key: Key::Char('-'),
                            modifiers: Modifiers::CTRL,
                        }),
                        "zoom_out",
                    );
                    g.bind(
                        KeyPattern::Exact(KeyEvent {
                            key: Key::Char('0'),
                            modifiers: Modifiers::CTRL,
                        }),
                        "zoom_reset",
                    );
                },
            );

            km.action("zoom_in", |state, _key, _app| {
                let new_level = state.level.up();
                if new_level == state.level {
                    return (state.clone(), KeyResponse::Pass);
                }
                // Activate the projection when transitioning from RAW.
                let response = if state.level == ZoomLevel::RAW {
                    KeyResponse::ConsumeWith(vec![Command::SetStructuralProjection(Some(
                        projection_id(),
                    ))])
                } else {
                    KeyResponse::Consume
                };
                (SemanticZoomState { level: new_level }, response)
            });

            km.action("zoom_out", |state, _key, _app| {
                let new_level = state.level.down();
                if new_level == state.level {
                    return (state.clone(), KeyResponse::Pass);
                }
                // Deactivate when returning to RAW.
                let response = if new_level == ZoomLevel::RAW {
                    KeyResponse::ConsumeWith(vec![Command::SetStructuralProjection(None)])
                } else {
                    KeyResponse::Consume
                };
                (SemanticZoomState { level: new_level }, response)
            });

            km.action("zoom_reset", |state, _key, _app| {
                // Deactivate projection if currently active.
                let response = if state.level != ZoomLevel::RAW {
                    KeyResponse::ConsumeWith(vec![Command::SetStructuralProjection(None)])
                } else {
                    KeyResponse::Consume
                };
                (
                    SemanticZoomState {
                        level: ZoomLevel::RAW,
                    },
                    response,
                )
            });
        });
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_level_up_saturates_at_max() {
        let level = ZoomLevel::MAP;
        assert_eq!(level.up(), ZoomLevel::MAP);
    }

    #[test]
    fn zoom_level_down_saturates_at_min() {
        let level = ZoomLevel::RAW;
        assert_eq!(level.down(), ZoomLevel::RAW);
    }

    #[test]
    fn zoom_level_up_increments() {
        assert_eq!(ZoomLevel::RAW.up(), ZoomLevel::ANNOTATED);
        assert_eq!(ZoomLevel::ANNOTATED.up(), ZoomLevel::COMPRESSED);
        assert_eq!(ZoomLevel::COMPRESSED.up(), ZoomLevel::OUTLINE);
        assert_eq!(ZoomLevel::OUTLINE.up(), ZoomLevel::SKELETON);
        assert_eq!(ZoomLevel::SKELETON.up(), ZoomLevel::MAP);
    }

    #[test]
    fn zoom_level_down_decrements() {
        assert_eq!(ZoomLevel::MAP.down(), ZoomLevel::SKELETON);
        assert_eq!(ZoomLevel::SKELETON.down(), ZoomLevel::OUTLINE);
        assert_eq!(ZoomLevel::OUTLINE.down(), ZoomLevel::COMPRESSED);
        assert_eq!(ZoomLevel::COMPRESSED.down(), ZoomLevel::ANNOTATED);
        assert_eq!(ZoomLevel::ANNOTATED.down(), ZoomLevel::RAW);
    }

    #[test]
    fn zoom_level_clamp() {
        assert_eq!(ZoomLevel(10).clamp(), ZoomLevel::MAP);
        assert_eq!(ZoomLevel(3).clamp(), ZoomLevel::OUTLINE);
    }

    #[test]
    fn default_state_is_raw() {
        let state = SemanticZoomState::default();
        assert_eq!(state.level, ZoomLevel::RAW);
    }

    #[test]
    fn raw_level_returns_empty_directives() {
        let state = SemanticZoomState::default();
        let app_state = crate::state::AppState::default();
        let view = AppView::new(&app_state);
        let directives = generate_zoom_directives(&state, &view);
        assert!(directives.is_empty());
    }

    #[test]
    fn plugin_registers_structural_projection() {
        let plugin = SemanticZoomPlugin;
        let mut registry = HandlerRegistry::<SemanticZoomState>::new();
        plugin.register(&mut registry);
        let table = registry.into_table();
        assert_eq!(table.projection_entries.len(), 1);
        assert_eq!(table.projection_entries[0].descriptor.id, projection_id());
        assert_eq!(
            table.projection_entries[0].descriptor.category,
            ProjectionCategory::Structural,
        );
    }

    #[test]
    fn zoom_level_display() {
        assert_eq!(format!("{}", ZoomLevel::RAW), "Raw");
        assert_eq!(format!("{}", ZoomLevel::ANNOTATED), "Annotated");
        assert_eq!(format!("{}", ZoomLevel::COMPRESSED), "Compressed");
        assert_eq!(format!("{}", ZoomLevel::OUTLINE), "Outline");
        assert_eq!(format!("{}", ZoomLevel::SKELETON), "Skeleton");
        assert_eq!(format!("{}", ZoomLevel::MAP), "Map");
    }
}
