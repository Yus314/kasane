use crate::bindings::kasane::plugin::types as wit;
use kasane_core::config::MenuPosition;
use kasane_core::element::{BorderConfig, BorderLineStyle, Edges, GridColumn, OverlayAnchor};
use kasane_core::layout::Rect;
use kasane_core::protocol::{Coord, InfoStyle, MenuStyle};

pub(crate) fn wit_overlay_anchor_to_overlay_anchor(wa: &wit::OverlayAnchor) -> OverlayAnchor {
    match wa {
        wit::OverlayAnchor::Absolute(a) => OverlayAnchor::Absolute {
            x: a.x,
            y: a.y,
            w: a.w,
            h: a.h,
        },
        wit::OverlayAnchor::AnchorPoint(ap) => OverlayAnchor::AnchorPoint {
            coord: Coord {
                line: ap.coord.line,
                column: ap.coord.column,
            },
            prefer_above: ap.prefer_above,
            avoid: ap
                .avoid
                .iter()
                .map(|r| Rect {
                    x: r.x,
                    y: r.y,
                    w: r.w,
                    h: r.h,
                })
                .collect(),
        },
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

pub(crate) fn menu_position_to_string(pos: &MenuPosition) -> String {
    match pos {
        MenuPosition::Auto => "auto".into(),
        MenuPosition::Above => "above".into(),
        MenuPosition::Below => "below".into(),
    }
}
