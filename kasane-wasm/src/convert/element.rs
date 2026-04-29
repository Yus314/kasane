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

pub(crate) fn wit_canvas_op_to_canvas_op(
    op: &wit::CanvasDrawOp,
) -> kasane_core::plugin::canvas::CanvasDrawOp {
    use kasane_core::plugin::canvas::CanvasDrawOp as CoreOp;
    match op {
        wit::CanvasDrawOp::FillRect(r) => CoreOp::FillRect {
            x: r.x,
            y: r.y,
            w: r.w,
            h: r.h,
            color: super::wit_brush_to_color(&r.color),
        },
        wit::CanvasDrawOp::RoundedRect(r) => CoreOp::RoundedRect {
            x: r.x,
            y: r.y,
            w: r.w,
            h: r.h,
            corner_radius: r.corner_radius,
            border_width: r.border_width,
            fill_color: super::wit_brush_to_color(&r.fill_color),
            border_color: super::wit_brush_to_color(&r.border_color),
        },
        wit::CanvasDrawOp::Line(l) => CoreOp::Line {
            x1: l.x1,
            y1: l.y1,
            x2: l.x2,
            y2: l.y2,
            color: super::wit_brush_to_color(&l.color),
            width: l.width,
        },
        wit::CanvasDrawOp::Text(t) => CoreOp::Text {
            x: t.x,
            y: t.y,
            text: t.text.clone(),
            color: super::wit_brush_to_color(&t.color),
        },
        wit::CanvasDrawOp::Circle(c) => CoreOp::Circle {
            cx: c.cx,
            cy: c.cy,
            radius: c.radius,
            fill_color: c.fill_color.as_ref().map(super::wit_brush_to_color),
            stroke_color: c.stroke_color.as_ref().map(super::wit_brush_to_color),
            stroke_width: c.stroke_width,
        },
    }
}

// ---------------------------------------------------------------------------
// ElementPatch conversion (WIT list<element-patch-op> → core ElementPatch)
// ---------------------------------------------------------------------------

use kasane_core::plugin::ElementPatch;
use kasane_core::plugin::element_patch::PatchPredicate;
use kasane_core::surface::SurfaceId;

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

    let mut patches = Vec::new();
    let mut i = 0;
    while i < ops.len() {
        let patch = match &ops[i] {
            wit::ElementPatchOp::WrapContainer(style) => ElementPatch::WrapContainer {
                style: std::sync::Arc::new(kasane_core::protocol::UnresolvedStyle::from_face(
                    &super::wit_style_to_face(style),
                )),
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
            wit::ElementPatchOp::ModifyStyle(style) => ElementPatch::ModifyStyle {
                overlay: std::sync::Arc::new(kasane_core::protocol::UnresolvedStyle::from_face(
                    &super::wit_style_to_face(style),
                )),
            },
            wit::ElementPatchOp::When(when) => {
                let predicate = wit_predicate_ops_to_predicate(&when.predicate);
                let then_count = when.then_count as usize;
                let otherwise_count = when.otherwise_count as usize;
                // Consume subsequent ops for then/otherwise branches
                let then_start = i + 1;
                let then_end = then_start + then_count;
                let otherwise_end = then_end + otherwise_count;
                let then_patch = wit_element_patch_ops_to_patch(
                    &ops[then_start..then_end.min(ops.len())],
                    take_element,
                );
                let otherwise_patch = wit_element_patch_ops_to_patch(
                    &ops[then_end..otherwise_end.min(ops.len())],
                    take_element,
                );
                i = otherwise_end;
                patches.push(ElementPatch::When {
                    predicate,
                    then: Box::new(then_patch),
                    otherwise: Box::new(otherwise_patch),
                });
                continue;
            }
        };
        patches.push(patch);
        i += 1;
    }

    if patches.len() == 1 {
        patches.into_iter().next().unwrap()
    } else {
        ElementPatch::Compose(patches)
    }
}

// ---------------------------------------------------------------------------
// PatchPredicate conversion (WIT RPN list<predicate-op> → core PatchPredicate)
// ---------------------------------------------------------------------------

/// Convert RPN-encoded `list<predicate-op>` to a core `PatchPredicate`.
///
/// The RPN stack produces a single predicate value. If the stack is empty
/// or malformed, returns `HasFocus` as a safe fallback.
fn wit_predicate_ops_to_predicate(ops: &[wit::PredicateOp]) -> PatchPredicate {
    let mut stack: Vec<PatchPredicate> = Vec::new();
    for op in ops {
        match op {
            wit::PredicateOp::HasFocus => stack.push(PatchPredicate::HasFocus),
            wit::PredicateOp::SurfaceIs(id) => {
                stack.push(PatchPredicate::SurfaceIs(SurfaceId(*id)));
            }
            wit::PredicateOp::LineRange(range) => {
                stack.push(PatchPredicate::LineRange(
                    range.start as usize..range.end as usize,
                ));
            }
            wit::PredicateOp::NotOp => {
                if let Some(p) = stack.pop() {
                    stack.push(PatchPredicate::Not(Box::new(p)));
                }
            }
            wit::PredicateOp::AndOp => {
                if let (Some(b), Some(a)) = (stack.pop(), stack.pop()) {
                    stack.push(PatchPredicate::And(Box::new(a), Box::new(b)));
                }
            }
            wit::PredicateOp::OrOp => {
                if let (Some(b), Some(a)) = (stack.pop(), stack.pop()) {
                    stack.push(PatchPredicate::Or(Box::new(a), Box::new(b)));
                }
            }
        }
    }
    stack.pop().unwrap_or(PatchPredicate::HasFocus)
}
