use std::ops::Range;

use crate::element::{Element, OverlayAnchor};
use crate::layout::Rect;
use crate::protocol::Face;
use crate::state::AppState;

/// Layout constraints passed to plugins during contribution.
#[derive(Debug, Clone)]
pub struct ContributeContext {
    pub available_width: u16,
    pub available_height: u16,
    pub visible_lines: Range<usize>,
    pub screen_cols: u16,
    pub screen_rows: u16,
}

impl ContributeContext {
    /// Build from AppState and an optional surface rect.
    pub fn new(state: &AppState, rect: Option<&Rect>) -> Self {
        let (w, h) = if let Some(r) = rect {
            (r.w, r.h)
        } else {
            (state.cols, state.available_height())
        };
        ContributeContext {
            available_width: w,
            available_height: h,
            visible_lines: state.visible_line_range(),
            screen_cols: state.cols,
            screen_rows: state.rows,
        }
    }
}

/// Result of a plugin's `contribute_to()` call.
#[derive(Debug, Clone)]
pub struct Contribution {
    pub element: Element,
    pub priority: i16,
    pub size_hint: ContribSizeHint,
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

/// Context passed to `transform()`.
#[derive(Debug, Clone)]
pub struct TransformContext {
    pub is_default: bool,
    pub chain_position: usize,
}

/// Context for `annotate_line_with_ctx()`.
#[derive(Debug, Clone)]
pub struct AnnotateContext {
    pub line_width: u16,
    pub gutter_width: u16,
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
}

/// Context for overlay contributions with collision avoidance.
#[derive(Debug, Clone)]
pub struct OverlayContext {
    pub screen_cols: u16,
    pub screen_rows: u16,
    pub menu_rect: Option<Rect>,
    pub existing_overlays: Vec<Rect>,
}

/// Overlay contribution with z-index.
#[derive(Debug, Clone)]
pub struct OverlayContribution {
    pub element: Element,
    pub anchor: OverlayAnchor,
    pub z_index: i16,
}

/// Aggregated annotation result from all plugins.
#[derive(Debug, Clone)]
pub struct AnnotationResult {
    pub left_gutter: Option<Element>,
    pub right_gutter: Option<Element>,
    pub line_backgrounds: Option<Vec<Option<Face>>>,
}
