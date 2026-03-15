//! Type conversions between WIT-generated types and kasane-core types.

use std::time::Duration;

use crate::bindings::kasane::plugin::types as wit;
use kasane_core::config::MenuPosition;
use kasane_core::element::{
    BorderConfig, BorderLineStyle, Direction, Edges, GridColumn, OverlayAnchor,
};
use kasane_core::input::{Key, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use kasane_core::layout::{Rect, SplitDirection};
use kasane_core::plugin::{
    AnnotateContext, Command, ContribSizeHint, ContributeContext, IoEvent, OverlayContext,
    PluginId, ProcessEvent, SlotId, StdinMode, TransformContext, TransformTarget,
};
use kasane_core::protocol::{
    Atom, Attributes, Color, Coord, Face, InfoStyle, KasaneRequest, MenuStyle, NamedColor,
};
use kasane_core::session::SessionCommand;
use kasane_core::state::DirtyFlags;
use kasane_core::surface::{
    EventContext, SizeHint, SlotKind, SurfaceEvent, SurfacePlacementRequest, ViewContext,
};
use kasane_core::workspace::DockPosition;

// ---------------------------------------------------------------------------
// Bidirectional enum conversion macro
// ---------------------------------------------------------------------------

/// Generate two functions that map 1:1 between a WIT enum and a native enum.
macro_rules! bidirectional_enum {
    ($to_native:ident: $wit_ty:ty => $native_ty:ty,
     $to_wit:ident: $native_ty2:ty => $wit_ty2:ty,
     { $($variant:ident),* $(,)? }) => {
        fn $to_native(w: $wit_ty) -> $native_ty {
            match w { $( <$wit_ty>::$variant => <$native_ty>::$variant, )* }
        }
        fn $to_wit(n: $native_ty) -> $wit_ty {
            match n { $( <$native_ty>::$variant => <$wit_ty>::$variant, )* }
        }
    };
}

// ---------------------------------------------------------------------------
// Face / Color conversions (WIT ↔ native)
// ---------------------------------------------------------------------------

bidirectional_enum! {
    wit_named_to_named: wit::NamedColor => NamedColor,
    named_to_wit: NamedColor => wit::NamedColor,
    {
        Black, Red, Green, Yellow, Blue, Magenta, Cyan, White,
        BrightBlack, BrightRed, BrightGreen, BrightYellow,
        BrightBlue, BrightMagenta, BrightCyan, BrightWhite,
    }
}

pub(crate) fn wit_face_to_face(wf: &wit::Face) -> Face {
    Face {
        fg: wit_color_to_color(&wf.fg),
        bg: wit_color_to_color(&wf.bg),
        underline: wit_color_to_color(&wf.underline),
        attributes: Attributes::from_bits_truncate(wf.attributes),
    }
}

fn wit_color_to_color(wc: &wit::Color) -> Color {
    match wc {
        wit::Color::DefaultColor => Color::Default,
        wit::Color::Named(n) => Color::Named(wit_named_to_named(*n)),
        wit::Color::Rgb(rgb) => Color::Rgb {
            r: rgb.r,
            g: rgb.g,
            b: rgb.b,
        },
    }
}

// ---------------------------------------------------------------------------
// Atom conversion (WIT → native)
// ---------------------------------------------------------------------------

pub(crate) fn wit_atom_to_atom(wa: &wit::Atom) -> Atom {
    Atom {
        face: wit_face_to_face(&wa.face),
        contents: wa.contents.as_str().into(),
    }
}

// ---------------------------------------------------------------------------
// Face / Color / Atom conversions (native → WIT)
// ---------------------------------------------------------------------------

pub(crate) fn color_to_wit(c: &Color) -> wit::Color {
    match c {
        Color::Default => wit::Color::DefaultColor,
        Color::Named(n) => wit::Color::Named(named_to_wit(*n)),
        Color::Rgb { r, g, b } => wit::Color::Rgb(wit::RgbColor {
            r: *r,
            g: *g,
            b: *b,
        }),
    }
}

pub(crate) fn face_to_wit(f: &Face) -> wit::Face {
    wit::Face {
        fg: color_to_wit(&f.fg),
        bg: color_to_wit(&f.bg),
        underline: color_to_wit(&f.underline),
        attributes: f.attributes.bits(),
    }
}

pub(crate) fn atom_to_wit(a: &Atom) -> wit::Atom {
    wit::Atom {
        face: face_to_wit(&a.face),
        contents: a.contents.to_string(),
    }
}

pub(crate) fn atoms_to_wit(atoms: &[Atom]) -> Vec<wit::Atom> {
    atoms.iter().map(atom_to_wit).collect()
}

pub(crate) fn wit_atoms_to_atoms(atoms: &[wit::Atom]) -> Vec<Atom> {
    atoms.iter().map(wit_atom_to_atom).collect()
}

// ---------------------------------------------------------------------------
// Command conversion (WIT → native)
// ---------------------------------------------------------------------------

pub(crate) fn wit_command_to_command(wc: &wit::Command) -> Command {
    match wc {
        wit::Command::SendKeys(keys) => Command::SendToKakoune(KasaneRequest::Keys(keys.clone())),
        wit::Command::Paste => Command::Paste,
        wit::Command::Quit => Command::Quit,
        wit::Command::RequestRedraw(bits) => {
            Command::RequestRedraw(DirtyFlags::from_bits_truncate(*bits))
        }
        wit::Command::SetConfig(entry) => Command::SetConfig {
            key: entry.key.clone(),
            value: entry.value.clone(),
        },
        wit::Command::ScheduleTimer(tc) => Command::ScheduleTimer {
            delay: Duration::from_millis(tc.delay_ms),
            target: PluginId(tc.target_plugin.clone()),
            payload: Box::new(tc.payload.clone()),
        },
        wit::Command::PluginMessage(mc) => Command::PluginMessage {
            target: PluginId(mc.target_plugin.clone()),
            payload: Box::new(mc.payload.clone()),
        },
        wit::Command::SpawnProcess(cfg) => Command::SpawnProcess {
            job_id: cfg.job_id,
            program: cfg.program.clone(),
            args: cfg.args.clone(),
            stdin_mode: match cfg.stdin_mode {
                wit::StdinMode::NullStdin => StdinMode::Null,
                wit::StdinMode::Piped => StdinMode::Piped,
            },
        },
        wit::Command::SpawnSession(cfg) => Command::Session(SessionCommand::Spawn {
            key: cfg.key.clone(),
            session: cfg.session.clone(),
            args: cfg.args.clone(),
            activate: cfg.activate,
        }),
        wit::Command::CloseSession(key) => {
            Command::Session(SessionCommand::Close { key: key.clone() })
        }
        wit::Command::WriteToProcess(cfg) => Command::WriteToProcess {
            job_id: cfg.job_id,
            data: cfg.data.clone(),
        },
        wit::Command::CloseProcessStdin(job_id) => Command::CloseProcessStdin { job_id: *job_id },
        wit::Command::KillProcess(job_id) => Command::KillProcess { job_id: *job_id },
    }
}

pub(crate) fn wit_commands_to_commands(wcs: &[wit::Command]) -> Vec<Command> {
    wcs.iter().map(wit_command_to_command).collect()
}

// ---------------------------------------------------------------------------
// I/O event conversion (native → WIT, for calling guest exports)
// ---------------------------------------------------------------------------

pub(crate) fn io_event_to_wit(event: &IoEvent) -> wit::IoEvent {
    match event {
        IoEvent::Process(pe) => wit::IoEvent::Process(process_event_to_wit(pe)),
    }
}

fn process_event_to_wit(pe: &ProcessEvent) -> wit::ProcessEvent {
    wit::ProcessEvent {
        job_id: match pe {
            ProcessEvent::Stdout { job_id, .. }
            | ProcessEvent::Stderr { job_id, .. }
            | ProcessEvent::Exited { job_id, .. }
            | ProcessEvent::SpawnFailed { job_id, .. } => *job_id,
        },
        kind: match pe {
            ProcessEvent::Stdout { data, .. } => wit::ProcessEventKind::Stdout(data.clone()),
            ProcessEvent::Stderr { data, .. } => wit::ProcessEventKind::Stderr(data.clone()),
            ProcessEvent::Exited { exit_code, .. } => wit::ProcessEventKind::Exited(*exit_code),
            ProcessEvent::SpawnFailed { error, .. } => {
                wit::ProcessEventKind::SpawnFailed(error.clone())
            }
        },
    }
}

// ---------------------------------------------------------------------------
// Input conversions (native → WIT, for calling guest exports)
// ---------------------------------------------------------------------------

pub(crate) fn mouse_event_to_wit(event: &MouseEvent) -> wit::MouseEvent {
    wit::MouseEvent {
        kind: mouse_event_kind_to_wit(&event.kind),
        line: event.line,
        column: event.column,
        modifiers: event.modifiers.bits(),
    }
}

fn mouse_event_kind_to_wit(kind: &MouseEventKind) -> wit::MouseEventKind {
    match kind {
        MouseEventKind::Press(b) => wit::MouseEventKind::Press(mouse_button_to_wit(*b)),
        MouseEventKind::Release(b) => wit::MouseEventKind::Release(mouse_button_to_wit(*b)),
        MouseEventKind::Move => wit::MouseEventKind::MoveEvent,
        MouseEventKind::Drag(b) => wit::MouseEventKind::Drag(mouse_button_to_wit(*b)),
        MouseEventKind::ScrollUp => wit::MouseEventKind::ScrollUp,
        MouseEventKind::ScrollDown => wit::MouseEventKind::ScrollDown,
    }
}

fn mouse_button_to_wit(b: MouseButton) -> wit::MouseButton {
    match b {
        MouseButton::Left => wit::MouseButton::Left,
        MouseButton::Middle => wit::MouseButton::Middle,
        MouseButton::Right => wit::MouseButton::Right,
    }
}

pub(crate) fn key_event_to_wit(event: &KeyEvent) -> wit::KeyEvent {
    wit::KeyEvent {
        key: key_to_wit(&event.key),
        modifiers: event.modifiers.bits(),
    }
}

fn key_to_wit(key: &Key) -> wit::KeyCode {
    match key {
        Key::Char(c) => wit::KeyCode::Character(c.to_string()),
        Key::Backspace => wit::KeyCode::Backspace,
        Key::Delete => wit::KeyCode::Delete,
        Key::Enter => wit::KeyCode::Enter,
        Key::Tab => wit::KeyCode::Tab,
        Key::Escape => wit::KeyCode::Escape,
        Key::Up => wit::KeyCode::Up,
        Key::Down => wit::KeyCode::Down,
        Key::Left => wit::KeyCode::LeftArrow,
        Key::Right => wit::KeyCode::RightArrow,
        Key::Home => wit::KeyCode::Home,
        Key::End => wit::KeyCode::End,
        Key::PageUp => wit::KeyCode::PageUp,
        Key::PageDown => wit::KeyCode::PageDown,
        Key::F(n) => wit::KeyCode::FKey(*n),
    }
}

// ---------------------------------------------------------------------------
// Overlay / anchor conversions (WIT → native)
// ---------------------------------------------------------------------------

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

pub(crate) fn wit_rect_to_rect(rect: &wit::Rect) -> Rect {
    Rect {
        x: rect.x,
        y: rect.y,
        w: rect.w,
        h: rect.h,
    }
}

// ---------------------------------------------------------------------------
// Element builder type conversions (WIT → native)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Style / config string conversions (native → string)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// v0.5.0: Contribute / Transform / Annotate conversions
// ---------------------------------------------------------------------------

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

pub(crate) fn wit_layout_direction_to_direction(direction: wit::LayoutDirection) -> Direction {
    match direction {
        wit::LayoutDirection::Row => Direction::Row,
        wit::LayoutDirection::Column => Direction::Column,
    }
}

pub(crate) fn wit_slot_kind_to_slot_kind(kind: wit::SlotKind) -> SlotKind {
    match kind {
        wit::SlotKind::AboveBand => SlotKind::AboveBand,
        wit::SlotKind::BelowBand => SlotKind::BelowBand,
        wit::SlotKind::LeftRail => SlotKind::LeftRail,
        wit::SlotKind::RightRail => SlotKind::RightRail,
        wit::SlotKind::Overlay => SlotKind::Overlay,
    }
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

pub(crate) fn surface_view_context_to_wit(ctx: &ViewContext<'_>) -> wit::SurfaceViewContext {
    wit::SurfaceViewContext {
        rect: wit::Rect {
            x: ctx.rect.x,
            y: ctx.rect.y,
            w: ctx.rect.w,
            h: ctx.rect.h,
        },
        focused: ctx.focused,
    }
}

pub(crate) fn surface_event_context_to_wit(ctx: &EventContext<'_>) -> wit::SurfaceEventContext {
    wit::SurfaceEventContext {
        rect: wit::Rect {
            x: ctx.rect.x,
            y: ctx.rect.y,
            w: ctx.rect.w,
            h: ctx.rect.h,
        },
        focused: ctx.focused,
    }
}

pub(crate) fn surface_event_to_wit(event: &SurfaceEvent) -> wit::SurfaceEvent {
    match event {
        SurfaceEvent::Key(event) => wit::SurfaceEvent::Key(key_event_to_wit(event)),
        SurfaceEvent::Mouse(event) => wit::SurfaceEvent::Mouse(mouse_event_to_wit(event)),
        SurfaceEvent::FocusGained => wit::SurfaceEvent::FocusGained,
        SurfaceEvent::FocusLost => wit::SurfaceEvent::FocusLost,
        SurfaceEvent::Resize(rect) => wit::SurfaceEvent::Resize(wit::Rect {
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: rect.h,
        }),
    }
}

fn wit_split_direction_to_split_direction(direction: wit::SplitDirection) -> SplitDirection {
    match direction {
        wit::SplitDirection::Horizontal => SplitDirection::Horizontal,
        wit::SplitDirection::Vertical => SplitDirection::Vertical,
    }
}

fn wit_dock_position_to_dock_position(position: wit::DockPosition) -> DockPosition {
    match position {
        wit::DockPosition::Left => DockPosition::Left,
        wit::DockPosition::Right => DockPosition::Right,
        wit::DockPosition::Bottom => DockPosition::Bottom,
        wit::DockPosition::Panel => DockPosition::Panel,
    }
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
        menu_rect: ctx.menu_rect.map(|r| wit::Rect {
            x: r.x,
            y: r.y,
            w: r.w,
            h: r.h,
        }),
        existing_overlays: ctx
            .existing_overlays
            .iter()
            .map(|r| wit::Rect {
                x: r.x,
                y: r.y,
                w: r.w,
                h: r.h,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kasane_core::input::Modifiers;
    use kasane_core::layout::flex::Constraints;
    use kasane_core::state::AppState;
    use kasane_core::surface::{EventContext, SurfaceEvent};

    #[test]
    fn convert_default_color() {
        let wc = wit::Color::DefaultColor;
        assert_eq!(wit_color_to_color(&wc), Color::Default);
    }

    #[test]
    fn convert_rgb_color() {
        let wc = wit::Color::Rgb(wit::RgbColor {
            r: 40,
            g: 40,
            b: 50,
        });
        assert_eq!(
            wit_color_to_color(&wc),
            Color::Rgb {
                r: 40,
                g: 40,
                b: 50
            }
        );
    }

    #[test]
    fn convert_named_color() {
        let wc = wit::Color::Named(wit::NamedColor::BrightCyan);
        assert_eq!(
            wit_color_to_color(&wc),
            Color::Named(NamedColor::BrightCyan)
        );
    }

    #[test]
    fn convert_face_with_attributes() {
        let wf = wit::Face {
            fg: wit::Color::Named(wit::NamedColor::Red),
            bg: wit::Color::Rgb(wit::RgbColor {
                r: 10,
                g: 20,
                b: 30,
            }),
            underline: wit::Color::DefaultColor,
            attributes: 0x20, // BOLD
        };
        let f = wit_face_to_face(&wf);
        assert_eq!(f.fg, Color::Named(NamedColor::Red));
        assert_eq!(
            f.bg,
            Color::Rgb {
                r: 10,
                g: 20,
                b: 30
            }
        );
        assert_eq!(f.underline, Color::Default);
        assert!(f.attributes.contains(Attributes::BOLD));
    }

    #[test]
    fn convert_atom() {
        let wa = wit::Atom {
            face: wit::Face {
                fg: wit::Color::Named(wit::NamedColor::Red),
                bg: wit::Color::DefaultColor,
                underline: wit::Color::DefaultColor,
                attributes: 0,
            },
            contents: "hello".to_string(),
        };
        let a = wit_atom_to_atom(&wa);
        assert_eq!(a.contents.as_str(), "hello");
        assert_eq!(a.face.fg, Color::Named(NamedColor::Red));
    }

    #[test]
    fn convert_contribute_context_preserves_unbounded_max() {
        let state = AppState::default();
        let ctx = ContributeContext::from_constraints(
            &state,
            Constraints {
                min_width: 2,
                max_width: u16::MAX,
                min_height: 1,
                max_height: 9,
            },
        );

        let wit_ctx = contribute_context_to_wit(&ctx);
        assert_eq!(wit_ctx.min_width, 2);
        assert_eq!(wit_ctx.max_width, None);
        assert_eq!(wit_ctx.min_height, 1);
        assert_eq!(wit_ctx.max_height, Some(9));
    }

    #[test]
    fn convert_command_send_keys() {
        let wc = wit::Command::SendKeys(vec!["a".into(), "b".into()]);
        match wit_command_to_command(&wc) {
            Command::SendToKakoune(KasaneRequest::Keys(keys)) => {
                assert_eq!(keys, vec!["a", "b"]);
            }
            _ => panic!("unexpected command variant"),
        }
    }

    #[test]
    fn convert_command_paste() {
        let wc = wit::Command::Paste;
        assert!(matches!(wit_command_to_command(&wc), Command::Paste));
    }

    #[test]
    fn convert_command_quit() {
        let wc = wit::Command::Quit;
        assert!(matches!(wit_command_to_command(&wc), Command::Quit));
    }

    #[test]
    fn convert_command_request_redraw() {
        let wc = wit::Command::RequestRedraw(0x03);
        match wit_command_to_command(&wc) {
            Command::RequestRedraw(flags) => {
                assert!(flags.contains(DirtyFlags::BUFFER_CONTENT));
                assert!(flags.contains(DirtyFlags::STATUS));
            }
            _ => panic!("unexpected command variant"),
        }
    }

    #[test]
    fn convert_command_set_config() {
        let wc = wit::Command::SetConfig(wit::ConfigEntry {
            key: "theme".into(),
            value: "dark".into(),
        });
        match wit_command_to_command(&wc) {
            Command::SetConfig { key, value } => {
                assert_eq!(key, "theme");
                assert_eq!(value, "dark");
            }
            _ => panic!("unexpected command variant"),
        }
    }

    #[test]
    fn convert_mouse_event_roundtrip() {
        let native = MouseEvent {
            kind: MouseEventKind::Press(MouseButton::Left),
            line: 5,
            column: 10,
            modifiers: Modifiers::CTRL | Modifiers::SHIFT,
        };
        let wit_ev = mouse_event_to_wit(&native);
        assert_eq!(wit_ev.line, 5);
        assert_eq!(wit_ev.column, 10);
        assert_eq!(
            wit_ev.modifiers,
            (Modifiers::CTRL | Modifiers::SHIFT).bits()
        );
        assert!(matches!(
            wit_ev.kind,
            wit::MouseEventKind::Press(wit::MouseButton::Left)
        ));
    }

    #[test]
    fn convert_key_event_roundtrip() {
        let native = KeyEvent {
            key: Key::Char('x'),
            modifiers: Modifiers::ALT,
        };
        let wit_ev = key_event_to_wit(&native);
        assert!(matches!(wit_ev.key, wit::KeyCode::Character(ref s) if s == "x"));
        assert_eq!(wit_ev.modifiers, Modifiers::ALT.bits());
    }

    #[test]
    fn convert_surface_event_key_roundtrip() {
        let native = SurfaceEvent::Key(KeyEvent {
            key: Key::Char('r'),
            modifiers: Modifiers::CTRL,
        });
        let wit_ev = surface_event_to_wit(&native);
        match wit_ev {
            wit::SurfaceEvent::Key(key) => {
                assert!(matches!(key.key, wit::KeyCode::Character(ref s) if s == "r"));
                assert_eq!(key.modifiers, Modifiers::CTRL.bits());
            }
            other => panic!("expected key surface event, got {other:?}"),
        }
    }

    #[test]
    fn convert_surface_event_context_preserves_focus() {
        let state = AppState::default();
        let ctx = EventContext {
            state: &state,
            rect: Rect {
                x: 4,
                y: 5,
                w: 12,
                h: 3,
            },
            focused: false,
        };
        let wit_ctx = surface_event_context_to_wit(&ctx);
        assert_eq!(wit_ctx.rect.x, 4);
        assert_eq!(wit_ctx.rect.y, 5);
        assert_eq!(wit_ctx.rect.w, 12);
        assert_eq!(wit_ctx.rect.h, 3);
        assert!(!wit_ctx.focused);
    }

    #[test]
    fn convert_overlay_anchor_absolute() {
        let wa = wit::OverlayAnchor::Absolute(wit::AbsoluteAnchor {
            x: 10,
            y: 20,
            w: 30,
            h: 40,
        });
        match wit_overlay_anchor_to_overlay_anchor(&wa) {
            OverlayAnchor::Absolute { x, y, w, h } => {
                assert_eq!((x, y, w, h), (10, 20, 30, 40));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn convert_overlay_anchor_point() {
        let wa = wit::OverlayAnchor::AnchorPoint(wit::AnchorPointConfig {
            coord: wit::Coord { line: 1, column: 2 },
            prefer_above: true,
            avoid: vec![wit::Rect {
                x: 0,
                y: 0,
                w: 10,
                h: 5,
            }],
        });
        match wit_overlay_anchor_to_overlay_anchor(&wa) {
            OverlayAnchor::AnchorPoint {
                coord,
                prefer_above,
                avoid,
            } => {
                assert_eq!(coord.line, 1);
                assert_eq!(coord.column, 2);
                assert!(prefer_above);
                assert_eq!(avoid.len(), 1);
                assert_eq!(avoid[0].w, 10);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn convert_border_styles() {
        assert_eq!(
            wit_border_to_border_config(&wit::BorderLineStyle::Rounded).line_style,
            BorderLineStyle::Rounded
        );
        assert_eq!(
            wit_border_to_border_config(&wit::BorderLineStyle::Heavy).line_style,
            BorderLineStyle::Heavy
        );
    }

    #[test]
    fn convert_edges() {
        let we = wit::Edges {
            top: 1,
            right: 2,
            bottom: 3,
            left: 4,
        };
        let e = wit_edges_to_edges(&we);
        assert_eq!((e.top, e.right, e.bottom, e.left), (1, 2, 3, 4));
    }

    #[test]
    fn convert_grid_widths() {
        assert_eq!(
            wit_grid_width_to_grid_column(&wit::GridWidth::Fixed(10)).width,
            kasane_core::element::GridWidth::Fixed(10)
        );
        assert_eq!(
            wit_grid_width_to_grid_column(&wit::GridWidth::FlexWidth(2.0)).width,
            kasane_core::element::GridWidth::Flex(2.0)
        );
        assert_eq!(
            wit_grid_width_to_grid_column(&wit::GridWidth::AutoWidth).width,
            kasane_core::element::GridWidth::Auto
        );
    }

    #[test]
    fn convert_key_special_keys() {
        assert!(matches!(
            key_to_wit(&Key::Backspace),
            wit::KeyCode::Backspace
        ));
        assert!(matches!(key_to_wit(&Key::F(5)), wit::KeyCode::FKey(5)));
        assert!(matches!(key_to_wit(&Key::PageUp), wit::KeyCode::PageUp));
        assert!(matches!(key_to_wit(&Key::Left), wit::KeyCode::LeftArrow));
    }

    #[test]
    fn convert_mouse_event_kinds() {
        assert!(matches!(
            mouse_event_kind_to_wit(&MouseEventKind::Release(MouseButton::Right)),
            wit::MouseEventKind::Release(wit::MouseButton::Right)
        ));
        assert!(matches!(
            mouse_event_kind_to_wit(&MouseEventKind::Move),
            wit::MouseEventKind::MoveEvent
        ));
        assert!(matches!(
            mouse_event_kind_to_wit(&MouseEventKind::ScrollDown),
            wit::MouseEventKind::ScrollDown
        ));
    }

    // --- native → WIT conversion tests ---

    #[test]
    fn convert_color_to_wit_default() {
        assert!(matches!(
            color_to_wit(&Color::Default),
            wit::Color::DefaultColor
        ));
    }

    #[test]
    fn convert_color_to_wit_rgb() {
        match color_to_wit(&Color::Rgb {
            r: 10,
            g: 20,
            b: 30,
        }) {
            wit::Color::Rgb(rgb) => {
                assert_eq!((rgb.r, rgb.g, rgb.b), (10, 20, 30));
            }
            other => panic!("expected Rgb, got {other:?}"),
        }
    }

    #[test]
    fn convert_color_to_wit_named() {
        match color_to_wit(&Color::Named(NamedColor::BrightCyan)) {
            wit::Color::Named(n) => assert!(matches!(n, wit::NamedColor::BrightCyan)),
            other => panic!("expected Named, got {other:?}"),
        }
    }

    #[test]
    fn convert_face_roundtrip() {
        let native = Face {
            fg: Color::Named(NamedColor::Red),
            bg: Color::Rgb { r: 1, g: 2, b: 3 },
            underline: Color::Default,
            attributes: Attributes::BOLD | Attributes::ITALIC,
        };
        let wit_f = face_to_wit(&native);
        let back = wit_face_to_face(&wit_f);
        assert_eq!(native.fg, back.fg);
        assert_eq!(native.bg, back.bg);
        assert_eq!(native.underline, back.underline);
        assert_eq!(native.attributes, back.attributes);
    }

    #[test]
    fn convert_atom_roundtrip() {
        let native = Atom {
            face: Face::default(),
            contents: "hello".into(),
        };
        let wit_a = atom_to_wit(&native);
        let back = wit_atom_to_atom(&wit_a);
        assert_eq!(native.contents.as_str(), back.contents.as_str());
    }

    #[test]
    fn convert_command_schedule_timer() {
        let wc = wit::Command::ScheduleTimer(wit::TimerConfig {
            delay_ms: 500,
            target_plugin: "my_plugin".into(),
            payload: vec![1, 2, 3],
        });
        match wit_command_to_command(&wc) {
            Command::ScheduleTimer {
                delay,
                target,
                payload,
            } => {
                assert_eq!(delay, Duration::from_millis(500));
                assert_eq!(target.0, "my_plugin");
                let bytes = payload.downcast::<Vec<u8>>().unwrap();
                assert_eq!(*bytes, vec![1, 2, 3]);
            }
            _ => panic!("unexpected command variant"),
        }
    }

    #[test]
    fn convert_command_plugin_message() {
        let wc = wit::Command::PluginMessage(wit::MessageConfig {
            target_plugin: "other".into(),
            payload: vec![42],
        });
        match wit_command_to_command(&wc) {
            Command::PluginMessage { target, payload } => {
                assert_eq!(target.0, "other");
                let bytes = payload.downcast::<Vec<u8>>().unwrap();
                assert_eq!(*bytes, vec![42]);
            }
            _ => panic!("unexpected command variant"),
        }
    }

    // --- Phase P-2: IoEvent conversion tests ---

    #[test]
    fn convert_io_event_process_stdout() {
        let native = IoEvent::Process(ProcessEvent::Stdout {
            job_id: 42,
            data: b"output data".to_vec(),
        });
        let wit_ev = io_event_to_wit(&native);
        match wit_ev {
            wit::IoEvent::Process(pe) => {
                assert_eq!(pe.job_id, 42);
                match pe.kind {
                    wit::ProcessEventKind::Stdout(data) => {
                        assert_eq!(data, b"output data");
                    }
                    _ => panic!("expected Stdout kind"),
                }
            }
        }
    }

    #[test]
    fn convert_io_event_process_stderr() {
        let native = IoEvent::Process(ProcessEvent::Stderr {
            job_id: 7,
            data: b"err".to_vec(),
        });
        let wit_ev = io_event_to_wit(&native);
        match wit_ev {
            wit::IoEvent::Process(pe) => {
                assert_eq!(pe.job_id, 7);
                assert!(matches!(pe.kind, wit::ProcessEventKind::Stderr(ref d) if d == b"err"));
            }
        }
    }

    #[test]
    fn convert_io_event_process_exited() {
        let native = IoEvent::Process(ProcessEvent::Exited {
            job_id: 1,
            exit_code: 127,
        });
        let wit_ev = io_event_to_wit(&native);
        match wit_ev {
            wit::IoEvent::Process(pe) => {
                assert_eq!(pe.job_id, 1);
                assert!(matches!(pe.kind, wit::ProcessEventKind::Exited(127)));
            }
        }
    }

    #[test]
    fn convert_io_event_process_spawn_failed() {
        let native = IoEvent::Process(ProcessEvent::SpawnFailed {
            job_id: 99,
            error: "not found".to_string(),
        });
        let wit_ev = io_event_to_wit(&native);
        match wit_ev {
            wit::IoEvent::Process(pe) => {
                assert_eq!(pe.job_id, 99);
                match pe.kind {
                    wit::ProcessEventKind::SpawnFailed(msg) => {
                        assert_eq!(msg, "not found");
                    }
                    _ => panic!("expected SpawnFailed kind"),
                }
            }
        }
    }

    #[test]
    fn convert_io_event_roundtrip_preserves_job_id() {
        // Test all ProcessEvent variants preserve job_id through conversion
        for job_id in [0u64, 1, u64::MAX] {
            let events = vec![
                IoEvent::Process(ProcessEvent::Stdout {
                    job_id,
                    data: vec![],
                }),
                IoEvent::Process(ProcessEvent::Stderr {
                    job_id,
                    data: vec![],
                }),
                IoEvent::Process(ProcessEvent::Exited {
                    job_id,
                    exit_code: 0,
                }),
                IoEvent::Process(ProcessEvent::SpawnFailed {
                    job_id,
                    error: String::new(),
                }),
            ];
            for event in &events {
                let wit_ev = io_event_to_wit(event);
                match wit_ev {
                    wit::IoEvent::Process(pe) => assert_eq!(pe.job_id, job_id),
                }
            }
        }
    }

    // --- Phase P-2: Process command conversion tests ---

    #[test]
    fn convert_command_spawn_process() {
        let wc = wit::Command::SpawnProcess(wit::SpawnProcessConfig {
            job_id: 10,
            program: "grep".into(),
            args: vec!["-r".into(), "foo".into()],
            stdin_mode: wit::StdinMode::Piped,
        });
        match wit_command_to_command(&wc) {
            Command::SpawnProcess {
                job_id,
                program,
                args,
                stdin_mode,
            } => {
                assert_eq!(job_id, 10);
                assert_eq!(program, "grep");
                assert_eq!(args, vec!["-r".to_string(), "foo".to_string()]);
                assert_eq!(stdin_mode, StdinMode::Piped);
            }
            _ => panic!("expected SpawnProcess"),
        }
    }

    #[test]
    fn convert_command_spawn_process_null_stdin() {
        let wc = wit::Command::SpawnProcess(wit::SpawnProcessConfig {
            job_id: 1,
            program: "ls".into(),
            args: vec![],
            stdin_mode: wit::StdinMode::NullStdin,
        });
        match wit_command_to_command(&wc) {
            Command::SpawnProcess { stdin_mode, .. } => {
                assert_eq!(stdin_mode, StdinMode::Null);
            }
            _ => panic!("expected SpawnProcess"),
        }
    }

    #[test]
    fn convert_command_write_to_process() {
        let wc = wit::Command::WriteToProcess(wit::WriteProcessConfig {
            job_id: 5,
            data: vec![1, 2, 3, 4],
        });
        match wit_command_to_command(&wc) {
            Command::WriteToProcess { job_id, data } => {
                assert_eq!(job_id, 5);
                assert_eq!(data, vec![1, 2, 3, 4]);
            }
            _ => panic!("expected WriteToProcess"),
        }
    }

    #[test]
    fn convert_command_close_process_stdin() {
        let wc = wit::Command::CloseProcessStdin(42);
        match wit_command_to_command(&wc) {
            Command::CloseProcessStdin { job_id } => {
                assert_eq!(job_id, 42);
            }
            _ => panic!("expected CloseProcessStdin"),
        }
    }

    #[test]
    fn convert_command_kill_process() {
        let wc = wit::Command::KillProcess(99);
        match wit_command_to_command(&wc) {
            Command::KillProcess { job_id } => {
                assert_eq!(job_id, 99);
            }
            _ => panic!("expected KillProcess"),
        }
    }

    #[test]
    fn convert_command_spawn_session() {
        let wc = wit::Command::SpawnSession(wit::SessionConfig {
            key: Some("work".to_string()),
            session: Some("project".to_string()),
            args: vec!["file.txt".to_string()],
            activate: true,
        });
        match wit_command_to_command(&wc) {
            Command::Session(SessionCommand::Spawn {
                key,
                session,
                args,
                activate,
            }) => {
                assert_eq!(key.as_deref(), Some("work"));
                assert_eq!(session.as_deref(), Some("project"));
                assert_eq!(args, vec!["file.txt".to_string()]);
                assert!(activate);
            }
            _ => panic!("expected Session::Spawn"),
        }
    }

    #[test]
    fn convert_command_close_session() {
        let wc = wit::Command::CloseSession(Some("work".to_string()));
        match wit_command_to_command(&wc) {
            Command::Session(SessionCommand::Close { key }) => {
                assert_eq!(key.as_deref(), Some("work"));
            }
            _ => panic!("expected Session::Close"),
        }
    }
}
