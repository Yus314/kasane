use std::ops::Range;

use crate::display::DisplayMapRef;
use crate::element::{Element, OverlayAnchor};
use crate::layout::Rect;
use crate::layout::flex::Constraints;
use crate::protocol::Face;
use crate::render::InlineDecoration;
use crate::surface::SurfaceId;

use super::{AppView, PluginId};

/// Pane-specific rendering context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneContext {
    pub surface_id: Option<SurfaceId>,
    pub focused: bool,
}

impl PaneContext {
    pub fn new(surface_id: SurfaceId, focused: bool) -> Self {
        Self {
            surface_id: Some(surface_id),
            focused,
        }
    }
}

impl Default for PaneContext {
    fn default() -> Self {
        Self {
            surface_id: None,
            focused: true,
        }
    }
}

/// Layout constraints passed to plugins during contribution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContributeContext {
    pub min_width: u16,
    pub max_width: Option<u16>,
    pub min_height: u16,
    pub max_height: Option<u16>,
    pub visible_lines: Range<usize>,
    pub screen_cols: u16,
    pub screen_rows: u16,
    pub pane_surface_id: Option<SurfaceId>,
    pub pane_focused: bool,
}

impl ContributeContext {
    /// Build from AppView and an optional surface rect.
    pub fn new(app: &AppView<'_>, rect: Option<&Rect>) -> Self {
        Self::new_in_pane(app, rect, PaneContext::default())
    }

    /// Build from AppView and an optional surface rect for a pane.
    pub fn new_in_pane(app: &AppView<'_>, rect: Option<&Rect>, pane: PaneContext) -> Self {
        if let Some(rect) = rect {
            Self::from_constraints_in_pane(app, Constraints::tight(rect.w, rect.h), pane)
        } else {
            Self::from_constraints_in_pane(
                app,
                Constraints::loose(app.cols(), app.available_height()),
                pane,
            )
        }
    }

    /// Build from layout constraints.
    pub fn from_constraints(app: &AppView<'_>, constraints: Constraints) -> Self {
        Self::from_constraints_in_pane(app, constraints, PaneContext::default())
    }

    /// Build from layout constraints for a pane.
    pub fn from_constraints_in_pane(
        app: &AppView<'_>,
        constraints: Constraints,
        pane: PaneContext,
    ) -> Self {
        ContributeContext {
            min_width: constraints.min_width,
            max_width: bounded_constraint(constraints.max_width),
            min_height: constraints.min_height,
            max_height: bounded_constraint(constraints.max_height),
            visible_lines: app.visible_line_range(),
            screen_cols: app.cols(),
            screen_rows: app.rows(),
            pane_surface_id: pane.surface_id,
            pane_focused: pane.focused,
        }
    }
}

fn bounded_constraint(max: u16) -> Option<u16> {
    if max == u16::MAX { None } else { Some(max) }
}

/// Result of a plugin's `contribute_to()` call.
#[derive(Debug, Clone, PartialEq)]
pub struct Contribution {
    pub element: Element,
    pub priority: i16,
    pub size_hint: ContribSizeHint,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourcedContribution {
    pub contributor: PluginId,
    pub contribution: Contribution,
}

/// Size hint for a contribution within a slot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ContribSizeHint {
    Auto,
    Fixed(u16),
    Flex(f32),
}

/// Transform target — unifies Decorator + Replacement targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransformTarget {
    Buffer,
    BufferLine(usize),
    StatusBar,
    Menu,
    MenuPrompt,
    MenuInline,
    MenuSearch,
    Info,
    InfoPrompt,
    InfoModal,
}

impl TransformTarget {
    /// Return the parent target in the refinement hierarchy, if any.
    ///
    /// Style-specific menu/info targets refine their generic parent:
    /// `MenuPrompt → Menu`, `InfoPrompt → Info`, etc.
    pub fn parent(&self) -> Option<TransformTarget> {
        match self {
            Self::MenuPrompt | Self::MenuInline | Self::MenuSearch => Some(Self::Menu),
            Self::InfoPrompt | Self::InfoModal => Some(Self::Info),
            _ => None,
        }
    }

    /// Return the refinement chain: `[parent, self]` if a parent exists, otherwise `[self]`.
    ///
    /// Used by `apply_transform_chain_hierarchical` to apply transforms from
    /// generic to specific.
    pub fn refinement_chain(&self) -> Vec<TransformTarget> {
        match self.parent() {
            Some(parent) => vec![parent, *self],
            None => vec![*self],
        }
    }

    /// Returns true if this target is a style-specific refinement of a generic target.
    pub fn is_refinement(&self) -> bool {
        self.parent().is_some()
    }
}

/// Scope of a transform's effect on the element tree.
///
/// Used by `TransformDescriptor` for declarative conflict detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransformScope {
    /// Pass-through (no-op transform).
    Identity,
    /// Wraps the element in a container/decorator.
    Wrapper,
    /// Prepends content before the element.
    Prepend,
    /// Appends content after the element.
    Append,
    /// Modifies element attributes (face, style) without changing structure.
    Attribute,
    /// Replaces the element entirely. Absorbs all prior transforms.
    Replacement,
    /// Changes the element structure (e.g., reorders children).
    Structural,
}

/// Declarative description of a plugin's transform behavior.
///
/// Plugins may optionally declare their transform descriptor for debug-time
/// conflict detection. When two plugins both declare `Replacement` scope for
/// the same target, a warning is emitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransformDescriptor {
    pub targets: Vec<TransformTarget>,
    pub scope: TransformScope,
}

/// Context passed to `transform()`.
#[derive(Debug, Clone)]
pub struct TransformContext {
    pub is_default: bool,
    pub chain_position: usize,
    pub pane_surface_id: Option<SurfaceId>,
    pub pane_focused: bool,
}

/// Context for `annotate_line_with_ctx()`.
#[derive(Debug, Clone)]
pub struct AnnotateContext {
    pub line_width: u16,
    pub gutter_width: u16,
    /// The active DisplayMap, if any display transformations are in effect.
    pub display_map: Option<DisplayMapRef>,
    pub pane_surface_id: Option<SurfaceId>,
    pub pane_focused: bool,
}

/// A background layer with z-ordering and blend mode.
#[derive(Debug, Clone)]
pub struct BackgroundLayer {
    pub face: Face,
    pub z_order: i16,
    pub blend: BlendMode,
}

/// How a background layer is composited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    Opaque,
}

/// New line annotation with `BackgroundLayer` support.
///
/// Annotations are collected from all annotating plugins per visible line.
/// When multiple plugins contribute gutter elements, they are sorted by
/// `priority` (ascending: lower values appear first / leftmost).
#[derive(Debug, Clone)]
pub struct LineAnnotation {
    pub left_gutter: Option<Element>,
    pub right_gutter: Option<Element>,
    pub background: Option<BackgroundLayer>,
    /// Sort priority for gutter element ordering (default: 0).
    /// Lower values sort first (leftmost in left gutter, leftmost in right gutter).
    /// Mirrors `Contribution::priority` and `BackgroundLayer::z_order` conventions.
    pub priority: i16,
    /// Inline decoration (byte-range Style/Hide) for this line.
    pub inline: Option<InlineDecoration>,
}

/// Context for overlay contributions with collision avoidance.
#[derive(Debug, Clone)]
pub struct OverlayContext {
    pub screen_cols: u16,
    pub screen_rows: u16,
    pub menu_rect: Option<Rect>,
    pub existing_overlays: Vec<Rect>,
    pub focused_surface_id: Option<SurfaceId>,
}

/// Overlay contribution with z-index.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlayContribution {
    pub element: Element,
    pub anchor: OverlayAnchor,
    pub z_index: i16,
    /// Plugin that contributed this overlay (for deterministic tie-breaking).
    pub plugin_id: PluginId,
}

/// Aggregated annotation result from all plugins.
#[derive(Debug, Clone)]
pub struct AnnotationResult {
    pub left_gutter: Option<Element>,
    pub right_gutter: Option<Element>,
    pub line_backgrounds: Option<Vec<Option<Face>>>,
    /// Per-line inline decorations (indexed by visible line).
    pub inline_decorations: Option<Vec<Option<InlineDecoration>>>,
}
