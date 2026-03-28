use crate::bindings::kasane::plugin::types as wit;
use kasane_core::config::MenuPosition;
use kasane_core::element::{
    BorderConfig, BorderLineStyle, Edges, GridColumn, ImageFit, OverlayAnchor,
};
use kasane_core::protocol::{InfoStyle, MenuStyle, StatusStyle};

pub(crate) fn wit_overlay_anchor_to_overlay_anchor(wa: &wit::OverlayAnchor) -> OverlayAnchor {
    match wa {
        wit::OverlayAnchor::Absolute(a) => OverlayAnchor::Absolute {
            x: a.x,
            y: a.y,
            w: a.w,
            h: a.h,
        },
        wit::OverlayAnchor::AnchorPoint(ap) => OverlayAnchor::AnchorPoint {
            coord: ap.coord.into(),
            prefer_above: ap.prefer_above,
            avoid: ap.avoid.iter().map(|r| (*r).into()).collect(),
        },
    }
}

pub(crate) fn overlay_anchor_to_wit(anchor: &OverlayAnchor) -> wit::OverlayAnchor {
    match anchor {
        OverlayAnchor::Absolute { x, y, w, h } => {
            wit::OverlayAnchor::Absolute(wit::AbsoluteAnchor {
                x: *x,
                y: *y,
                w: *w,
                h: *h,
            })
        }
        OverlayAnchor::AnchorPoint {
            coord,
            prefer_above,
            avoid,
        } => wit::OverlayAnchor::AnchorPoint(wit::AnchorPointConfig {
            coord: wit::Coord {
                line: coord.line,
                column: coord.column,
            },
            prefer_above: *prefer_above,
            avoid: avoid.iter().map(super::rect_to_wit).collect(),
        }),
        OverlayAnchor::Fill => {
            // WIT OverlayAnchor doesn't have a Fill variant; use Absolute(0,0,0,0)
            // as a sentinel. This path is unlikely for transform subjects.
            wit::OverlayAnchor::Absolute(wit::AbsoluteAnchor {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            })
        }
    }
}

pub(crate) fn wit_border_to_border_config(b: &wit::BorderLineStyle) -> BorderConfig {
    let style = match b {
        wit::BorderLineStyle::Single => BorderLineStyle::Single,
        wit::BorderLineStyle::Rounded => BorderLineStyle::Rounded,
        wit::BorderLineStyle::Double => BorderLineStyle::Double,
        wit::BorderLineStyle::Heavy => BorderLineStyle::Heavy,
        wit::BorderLineStyle::Ascii => BorderLineStyle::Ascii,
    };
    BorderConfig::new(style)
}

pub(crate) fn wit_edges_to_edges(we: &wit::Edges) -> Edges {
    Edges {
        top: we.top,
        right: we.right,
        bottom: we.bottom,
        left: we.left,
    }
}

pub(crate) fn wit_grid_width_to_grid_column(gw: &wit::GridWidth) -> GridColumn {
    match gw {
        wit::GridWidth::Fixed(w) => GridColumn::fixed(*w),
        wit::GridWidth::FlexWidth(f) => GridColumn::flex(*f),
        wit::GridWidth::AutoWidth => GridColumn::auto(),
    }
}

pub(crate) fn info_style_to_string(style: &InfoStyle) -> String {
    match style {
        InfoStyle::Prompt => "prompt".into(),
        InfoStyle::Modal => "modal".into(),
        InfoStyle::Inline => "inline".into(),
        InfoStyle::InlineAbove => "inlineAbove".into(),
        InfoStyle::MenuDoc => "menuDoc".into(),
    }
}

pub(crate) fn menu_style_to_string(style: &MenuStyle) -> String {
    match style {
        MenuStyle::Prompt => "prompt".into(),
        MenuStyle::Search => "search".into(),
        MenuStyle::Inline => "inline".into(),
    }
}

pub(crate) fn status_style_to_string(style: &StatusStyle) -> String {
    match style {
        StatusStyle::Status => "status".into(),
        StatusStyle::Command => "command".into(),
        StatusStyle::Search => "search".into(),
        StatusStyle::Prompt => "prompt".into(),
    }
}

pub(crate) fn menu_position_to_string(pos: &MenuPosition) -> String {
    match pos {
        MenuPosition::Auto => "auto".into(),
        MenuPosition::Above => "above".into(),
        MenuPosition::Below => "below".into(),
    }
}

pub(crate) fn wit_image_fit_to_image_fit(wf: &wit::ImageFit) -> ImageFit {
    match wf {
        wit::ImageFit::Contain => ImageFit::Contain,
        wit::ImageFit::Cover => ImageFit::Cover,
        wit::ImageFit::Fill => ImageFit::Fill,
    }
}

// ---------------------------------------------------------------------------
// ElementPatch conversion (WIT list<element-patch-op> → core ElementPatch)
// ---------------------------------------------------------------------------

use kasane_core::plugin::ElementPatch;

/// Convert a WIT `list<element-patch-op>` into a core `ElementPatch`.
///
/// Each `element-handle` in the ops is materialized into an `Element` via
/// the provided `take_element` closure (typically `store.data_mut().take_root_element()`).
///
/// - Empty list → `Identity`
/// - Single op → that patch variant
/// - Multiple ops → `Compose`
pub(crate) fn wit_element_patch_ops_to_patch(
    ops: &[wit::ElementPatchOp],
    take_element: &mut dyn FnMut(u32) -> kasane_core::element::Element,
) -> ElementPatch {
    if ops.is_empty() {
        return ElementPatch::Identity;
    }

    let patches: Vec<ElementPatch> = ops
        .iter()
        .map(|op| match op {
            wit::ElementPatchOp::WrapContainer(face) => ElementPatch::WrapContainer {
                face: super::wit_face_to_face(face),
            },
            wit::ElementPatchOp::Prepend(handle) => ElementPatch::Prepend {
                element: take_element(*handle),
            },
            wit::ElementPatchOp::Append(handle) => ElementPatch::Append {
                element: take_element(*handle),
            },
            wit::ElementPatchOp::Replace(handle) => ElementPatch::Replace {
                element: take_element(*handle),
            },
            wit::ElementPatchOp::ModifyFace(face) => ElementPatch::ModifyFace {
                overlay: super::wit_face_to_face(face),
            },
        })
        .collect();

    if patches.len() == 1 {
        patches.into_iter().next().unwrap()
    } else {
        ElementPatch::Compose(patches)
    }
}
