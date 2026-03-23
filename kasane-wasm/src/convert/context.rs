use crate::bindings::kasane::plugin::types as wit;
use kasane_core::element::Direction;
use kasane_core::layout::SplitDirection;
use kasane_core::plugin::{
    AnnotateContext, ContribSizeHint, ContributeContext, OverlayContext, SlotId, TransformContext,
    TransformTarget,
};
use kasane_core::surface::{
    EventContext, SizeHint, SlotKind, SurfaceEvent, SurfacePlacementRequest, ViewContext,
};
use kasane_core::workspace::DockPosition;

use super::{input::key_event_to_wit, input::mouse_event_to_wit, rect_to_wit, wit_rect_to_rect};

pub(crate) fn slot_id_to_wit(slot_id: &SlotId) -> wit::SlotId {
    match slot_id.well_known_index() {
        Some(0) => wit::SlotId::WellKnown(wit::WellKnownSlot::BufferLeft),
        Some(1) => wit::SlotId::WellKnown(wit::WellKnownSlot::BufferRight),
        Some(2) => wit::SlotId::WellKnown(wit::WellKnownSlot::AboveBuffer),
        Some(3) => wit::SlotId::WellKnown(wit::WellKnownSlot::BelowBuffer),
        Some(4) => wit::SlotId::WellKnown(wit::WellKnownSlot::AboveStatus),
        Some(5) => wit::SlotId::WellKnown(wit::WellKnownSlot::StatusLeft),
        Some(6) => wit::SlotId::WellKnown(wit::WellKnownSlot::StatusRight),
        Some(7) => wit::SlotId::WellKnown(wit::WellKnownSlot::Overlay),
        Some(_) => unreachable!("unexpected well-known slot index"),
        None => wit::SlotId::Named(slot_id.as_str().to_string()),
    }
}

pub(crate) fn wit_slot_id_to_slot_id(slot_id: &wit::SlotId) -> SlotId {
    match slot_id {
        wit::SlotId::WellKnown(slot) => match slot {
            wit::WellKnownSlot::BufferLeft => SlotId::BUFFER_LEFT,
            wit::WellKnownSlot::BufferRight => SlotId::BUFFER_RIGHT,
            wit::WellKnownSlot::AboveBuffer => SlotId::ABOVE_BUFFER,
            wit::WellKnownSlot::BelowBuffer => SlotId::BELOW_BUFFER,
            wit::WellKnownSlot::AboveStatus => SlotId::ABOVE_STATUS,
            wit::WellKnownSlot::StatusLeft => SlotId::STATUS_LEFT,
            wit::WellKnownSlot::StatusRight => SlotId::STATUS_RIGHT,
            wit::WellKnownSlot::Overlay => SlotId::OVERLAY,
        },
        wit::SlotId::Named(name) => SlotId::new(name.clone()),
    }
}

enum_convert! {
    pub(crate) wit_layout_direction_to_direction: wit::LayoutDirection => Direction,
    { Row, Column }
}

enum_convert! {
    pub(crate) wit_slot_kind_to_slot_kind: wit::SlotKind => SlotKind,
    { AboveBand, BelowBand, LeftRail, RightRail, Overlay }
}

pub(crate) fn wit_surface_size_hint_to_size_hint(hint: &wit::SurfaceSizeHint) -> SizeHint {
    SizeHint {
        min_width: hint.min_width,
        min_height: hint.min_height,
        preferred_width: hint.preferred_width,
        preferred_height: hint.preferred_height,
        flex: hint.flex,
    }
}

pub(crate) fn wit_surface_placement_to_request(
    placement: &wit::SurfacePlacement,
) -> SurfacePlacementRequest {
    match placement {
        wit::SurfacePlacement::SplitFocused(split) => SurfacePlacementRequest::SplitFocused {
            direction: wit_split_direction_to_split_direction(split.direction),
            ratio: split.ratio,
        },
        wit::SurfacePlacement::SplitFrom(split) => SurfacePlacementRequest::SplitFrom {
            target_surface_key: split.target_surface_key.clone().into(),
            direction: wit_split_direction_to_split_direction(split.direction),
            ratio: split.ratio,
        },
        wit::SurfacePlacement::Tab => SurfacePlacementRequest::Tab,
        wit::SurfacePlacement::TabIn(target_surface_key) => SurfacePlacementRequest::TabIn {
            target_surface_key: target_surface_key.clone().into(),
        },
        wit::SurfacePlacement::Dock(position) => {
            SurfacePlacementRequest::Dock(wit_dock_position_to_dock_position(*position))
        }
        wit::SurfacePlacement::Float(rect) => SurfacePlacementRequest::Float {
            rect: wit_rect_to_rect(rect),
        },
    }
}

/// Convert a WIT surface placement to a core `Placement`.
///
/// Unlike `wit_surface_placement_to_request` (which produces a key-based
/// `SurfacePlacementRequest` for dynamic surfaces), this produces the direct
/// `Placement` type used by workspace commands like `SpawnPaneClient`.
///
/// `SplitFrom` and `TabIn` are not supported and fall back to `SplitFocused`.
pub(crate) fn wit_surface_placement_to_placement(
    placement: &wit::SurfacePlacement,
) -> kasane_core::workspace::Placement {
    use kasane_core::workspace::Placement;

    match placement {
        wit::SurfacePlacement::SplitFocused(split) => Placement::SplitFocused {
            direction: wit_split_direction_to_split_direction(split.direction),
            ratio: split.ratio,
        },
        wit::SurfacePlacement::SplitFrom(split) => {
            // SplitFrom with target_surface_key cannot be resolved here
            // (would need SurfaceRegistry lookup), fall back to SplitFocused.
            Placement::SplitFocused {
                direction: wit_split_direction_to_split_direction(split.direction),
                ratio: split.ratio,
            }
        }
        wit::SurfacePlacement::Tab => Placement::SplitFocused {
            direction: kasane_core::layout::SplitDirection::Vertical,
            ratio: 0.5,
        },
        wit::SurfacePlacement::TabIn(_) => Placement::SplitFocused {
            direction: kasane_core::layout::SplitDirection::Vertical,
            ratio: 0.5,
        },
        wit::SurfacePlacement::Dock(position) => {
            Placement::Dock(wit_dock_position_to_dock_position(*position))
        }
        wit::SurfacePlacement::Float(rect) => Placement::Float {
            rect: wit_rect_to_rect(rect),
        },
    }
}

pub(crate) fn surface_view_context_to_wit(ctx: &ViewContext<'_>) -> wit::SurfaceViewContext {
    wit::SurfaceViewContext {
        rect: rect_to_wit(&ctx.rect),
        focused: ctx.focused,
    }
}

pub(crate) fn surface_event_context_to_wit(ctx: &EventContext<'_>) -> wit::SurfaceEventContext {
    wit::SurfaceEventContext {
        rect: rect_to_wit(&ctx.rect),
        focused: ctx.focused,
    }
}

pub(crate) fn surface_event_to_wit(event: &SurfaceEvent) -> wit::SurfaceEvent {
    match event {
        SurfaceEvent::Key(event) => wit::SurfaceEvent::Key(key_event_to_wit(event)),
        SurfaceEvent::Mouse(event) => wit::SurfaceEvent::Mouse(mouse_event_to_wit(event)),
        SurfaceEvent::FocusGained => wit::SurfaceEvent::FocusGained,
        SurfaceEvent::FocusLost => wit::SurfaceEvent::FocusLost,
        SurfaceEvent::Resize(rect) => wit::SurfaceEvent::Resize(rect_to_wit(rect)),
    }
}

enum_convert! {
    wit_split_direction_to_split_direction: wit::SplitDirection => SplitDirection,
    { Horizontal, Vertical }
}

enum_convert! {
    wit_dock_position_to_dock_position: wit::DockPosition => DockPosition,
    { Left, Right, Bottom, Panel }
}

pub(crate) fn contribute_context_to_wit(ctx: &ContributeContext) -> wit::ContributeContext {
    wit::ContributeContext {
        min_width: ctx.min_width,
        max_width: ctx.max_width,
        min_height: ctx.min_height,
        max_height: ctx.max_height,
        visible_line_start: ctx.visible_lines.start as u32,
        visible_line_end: ctx.visible_lines.end as u32,
        screen_cols: ctx.screen_cols,
        screen_rows: ctx.screen_rows,
    }
}

pub(crate) fn wit_size_hint_to_size_hint(wsh: &wit::ContribSizeHint) -> ContribSizeHint {
    match wsh {
        wit::ContribSizeHint::Auto => ContribSizeHint::Auto,
        wit::ContribSizeHint::FixedSize(n) => ContribSizeHint::Fixed(*n),
        wit::ContribSizeHint::FlexRatio(f) => ContribSizeHint::Flex(*f),
    }
}

pub(crate) fn transform_target_to_wit(target: &TransformTarget) -> wit::TransformTarget {
    match target {
        TransformTarget::Buffer => wit::TransformTarget::Buffer,
        TransformTarget::BufferLine(_) => wit::TransformTarget::BufferLine,
        TransformTarget::StatusBar => wit::TransformTarget::StatusBarT,
        TransformTarget::Menu => wit::TransformTarget::MenuT,
        TransformTarget::MenuPrompt => wit::TransformTarget::MenuPromptT,
        TransformTarget::MenuInline => wit::TransformTarget::MenuInlineT,
        TransformTarget::MenuSearch => wit::TransformTarget::MenuSearchT,
        TransformTarget::Info => wit::TransformTarget::InfoT,
        TransformTarget::InfoPrompt => wit::TransformTarget::InfoPromptT,
        TransformTarget::InfoModal => wit::TransformTarget::InfoModalT,
    }
}

pub(crate) fn transform_context_to_wit(ctx: &TransformContext) -> wit::TransformContext {
    wit::TransformContext {
        is_default: ctx.is_default,
        chain_position: ctx.chain_position as u32,
    }
}

pub(crate) fn annotate_context_to_wit(ctx: &AnnotateContext) -> wit::AnnotateContext {
    wit::AnnotateContext {
        line_width: ctx.line_width,
        gutter_width: ctx.gutter_width,
    }
}

pub(crate) fn overlay_context_to_wit(ctx: &OverlayContext) -> wit::OverlayContext {
    wit::OverlayContext {
        screen_cols: ctx.screen_cols,
        screen_rows: ctx.screen_rows,
        menu_rect: ctx.menu_rect.as_ref().map(rect_to_wit),
        existing_overlays: ctx.existing_overlays.iter().map(rect_to_wit).collect(),
    }
}

pub(crate) fn wit_inline_decoration_to_inline_decoration(
    wit_deco: &wit::InlineDecoration,
) -> kasane_core::render::InlineDecoration {
    let ops = wit_deco
        .ops
        .iter()
        .map(|op| match op {
            wit::InlineOp::StyleRange(s) => kasane_core::render::InlineOp::Style {
                range: s.start as usize..s.end as usize,
                face: super::wit_face_to_face(&s.face),
            },
            wit::InlineOp::HideRange(h) => kasane_core::render::InlineOp::Hide {
                range: h.start as usize..h.end as usize,
            },
        })
        .collect();
    kasane_core::render::InlineDecoration::new(ops)
}
