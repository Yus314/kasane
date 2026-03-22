use std::ops::Range;

use crate::display::DisplayMapRef;
use crate::element::{Element, Overlay, OverlayAnchor};
use crate::layout::Rect;
use crate::layout::flex::Constraints;
use crate::protocol::Face;
use crate::render::InlineDecoration;
use crate::surface::SurfaceId;

use super::{AppView, PluginId};

/// Sum type for transform chain subjects — either a bare Element or an Overlay
/// (Element + OverlayAnchor).
///
/// Overlay targets (Menu, Info) carry their anchor through the transform chain
/// so plugins can modify positioning. Non-overlay targets use the `Element` variant.
#[derive(Debug, Clone, PartialEq)]
pub enum TransformSubject {
    Element(Element),
    Overlay(Overlay),
}

impl TransformSubject {
    /// Returns `true` if this is an `Overlay` variant.
    pub fn is_overlay(&self) -> bool {
        matches!(self, Self::Overlay(_))
    }

    /// Apply a function to the contained element, preserving the variant.
    pub fn map_element(self, f: impl FnOnce(Element) -> Element) -> Self {
        match self {
            Self::Element(el) => Self::Element(f(el)),
            Self::Overlay(Overlay { element, anchor }) => Self::Overlay(Overlay {
                element: f(element),
                anchor,
            }),
        }
    }

    /// Apply a function to the overlay anchor. No-op for `Element` variant.
    pub fn map_anchor(self, f: impl FnOnce(OverlayAnchor) -> OverlayAnchor) -> Self {
        match self {
            Self::Element(_) => self,
            Self::Overlay(Overlay { element, anchor }) => Self::Overlay(Overlay {
                element,
                anchor: f(anchor),
            }),
        }
    }

    /// Apply a function to the overlay. No-op for `Element` variant.
    pub fn map_overlay(self, f: impl FnOnce(Overlay) -> Overlay) -> Self {
        match self {
            Self::Element(_) => self,
            Self::Overlay(overlay) => Self::Overlay(f(overlay)),
        }
    }

    /// Extract the element, discarding the anchor if present.
    pub fn into_element(self) -> Element {
        match self {
            Self::Element(el) => el,
            Self::Overlay(Overlay { element, .. }) => element,
        }
    }

    /// Extract the overlay if this is an `Overlay` variant.
    pub fn into_overlay(self) -> Option<Overlay> {
        match self {
            Self::Element(_) => None,
            Self::Overlay(overlay) => Some(overlay),
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::{Element, Overlay, OverlayAnchor};
    use crate::protocol::Face;

    fn sample_element() -> Element {
        Element::text("hello", Face::default())
    }

    fn sample_overlay() -> Overlay {
        Overlay {
            element: Element::text("menu", Face::default()),
            anchor: OverlayAnchor::Absolute {
                x: 1,
                y: 2,
                w: 10,
                h: 5,
            },
        }
    }

    #[test]
    fn map_element_preserves_variant() {
        let subj = TransformSubject::Element(sample_element());
        let mapped = subj.map_element(|_| Element::Empty);
        assert!(matches!(mapped, TransformSubject::Element(Element::Empty)));

        let subj = TransformSubject::Overlay(sample_overlay());
        let mapped = subj.map_element(|_| Element::Empty);
        assert!(matches!(
            mapped,
            TransformSubject::Overlay(Overlay {
                element: Element::Empty,
                ..
            })
        ));
    }

    #[test]
    fn map_anchor_noop_for_element() {
        let subj = TransformSubject::Element(sample_element());
        let mapped = subj.clone().map_anchor(|_| OverlayAnchor::Fill);
        assert_eq!(subj, mapped);
    }

    #[test]
    fn map_anchor_modifies_overlay() {
        let subj = TransformSubject::Overlay(sample_overlay());
        let mapped = subj.map_anchor(|_| OverlayAnchor::Fill);
        match mapped {
            TransformSubject::Overlay(o) => assert_eq!(o.anchor, OverlayAnchor::Fill),
            _ => panic!("expected Overlay"),
        }
    }

    #[test]
    fn map_overlay_noop_for_element() {
        let subj = TransformSubject::Element(sample_element());
        let mapped = subj.clone().map_overlay(|mut o| {
            o.anchor = OverlayAnchor::Fill;
            o
        });
        assert_eq!(subj, mapped);
    }

    #[test]
    fn into_element_from_both_variants() {
        let el = sample_element();
        let subj = TransformSubject::Element(el.clone());
        assert_eq!(subj.into_element(), el);

        let overlay = sample_overlay();
        let expected_el = overlay.element.clone();
        let subj = TransformSubject::Overlay(overlay);
        assert_eq!(subj.into_element(), expected_el);
    }

    #[test]
    fn into_overlay_element_returns_none() {
        let subj = TransformSubject::Element(sample_element());
        assert!(subj.into_overlay().is_none());
    }

    #[test]
    fn into_overlay_returns_some() {
        let overlay = sample_overlay();
        let subj = TransformSubject::Overlay(overlay.clone());
        assert_eq!(subj.into_overlay(), Some(overlay));
    }
}
