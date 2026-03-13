//! Type conversions between WIT-generated types and kasane-core types.

use std::time::Duration;

use crate::bindings::kasane::plugin::types as wit;
use kasane_core::element::{BorderConfig, BorderLineStyle, Edges, GridColumn, OverlayAnchor};
use kasane_core::input::{Key, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use kasane_core::layout::Rect;
use kasane_core::plugin::{Command, DecorateTarget, PluginId, ReplaceTarget};
use kasane_core::protocol::{Atom, Attributes, Color, Coord, Face, KasaneRequest, NamedColor};
use kasane_core::state::DirtyFlags;

// ---------------------------------------------------------------------------
// Face / Color conversions (WIT → native)
// ---------------------------------------------------------------------------

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

fn wit_named_to_named(wn: wit::NamedColor) -> NamedColor {
    match wn {
        wit::NamedColor::Black => NamedColor::Black,
        wit::NamedColor::Red => NamedColor::Red,
        wit::NamedColor::Green => NamedColor::Green,
        wit::NamedColor::Yellow => NamedColor::Yellow,
        wit::NamedColor::Blue => NamedColor::Blue,
        wit::NamedColor::Magenta => NamedColor::Magenta,
        wit::NamedColor::Cyan => NamedColor::Cyan,
        wit::NamedColor::White => NamedColor::White,
        wit::NamedColor::BrightBlack => NamedColor::BrightBlack,
        wit::NamedColor::BrightRed => NamedColor::BrightRed,
        wit::NamedColor::BrightGreen => NamedColor::BrightGreen,
        wit::NamedColor::BrightYellow => NamedColor::BrightYellow,
        wit::NamedColor::BrightBlue => NamedColor::BrightBlue,
        wit::NamedColor::BrightMagenta => NamedColor::BrightMagenta,
        wit::NamedColor::BrightCyan => NamedColor::BrightCyan,
        wit::NamedColor::BrightWhite => NamedColor::BrightWhite,
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

fn named_to_wit(n: NamedColor) -> wit::NamedColor {
    match n {
        NamedColor::Black => wit::NamedColor::Black,
        NamedColor::Red => wit::NamedColor::Red,
        NamedColor::Green => wit::NamedColor::Green,
        NamedColor::Yellow => wit::NamedColor::Yellow,
        NamedColor::Blue => wit::NamedColor::Blue,
        NamedColor::Magenta => wit::NamedColor::Magenta,
        NamedColor::Cyan => wit::NamedColor::Cyan,
        NamedColor::White => wit::NamedColor::White,
        NamedColor::BrightBlack => wit::NamedColor::BrightBlack,
        NamedColor::BrightRed => wit::NamedColor::BrightRed,
        NamedColor::BrightGreen => wit::NamedColor::BrightGreen,
        NamedColor::BrightYellow => wit::NamedColor::BrightYellow,
        NamedColor::BrightBlue => wit::NamedColor::BrightBlue,
        NamedColor::BrightMagenta => wit::NamedColor::BrightMagenta,
        NamedColor::BrightCyan => wit::NamedColor::BrightCyan,
        NamedColor::BrightWhite => wit::NamedColor::BrightWhite,
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
// Decorate / Replace target conversions (native → WIT)
// ---------------------------------------------------------------------------

pub(crate) fn decorate_target_to_wit(target: &DecorateTarget) -> wit::DecorateTarget {
    match target {
        DecorateTarget::Buffer => wit::DecorateTarget::Buffer,
        DecorateTarget::StatusBar => wit::DecorateTarget::StatusBar,
        DecorateTarget::Menu => wit::DecorateTarget::Menu,
        DecorateTarget::Info => wit::DecorateTarget::Info,
        DecorateTarget::BufferLine(line) => wit::DecorateTarget::BufferLine(*line as u32),
    }
}

pub(crate) fn replace_target_to_wit(target: &ReplaceTarget) -> wit::ReplaceTarget {
    match target {
        ReplaceTarget::MenuPrompt => wit::ReplaceTarget::MenuPrompt,
        ReplaceTarget::MenuInline => wit::ReplaceTarget::MenuInline,
        ReplaceTarget::MenuSearch => wit::ReplaceTarget::MenuSearch,
        ReplaceTarget::InfoPrompt => wit::ReplaceTarget::InfoPrompt,
        ReplaceTarget::InfoModal => wit::ReplaceTarget::InfoModal,
        ReplaceTarget::StatusBar => wit::ReplaceTarget::StatusBar,
    }
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
    }
}

pub(crate) fn wit_commands_to_commands(wcs: &[wit::Command]) -> Vec<Command> {
    wcs.iter().map(wit_command_to_command).collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use kasane_core::input::Modifiers;

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
                assert!(flags.contains(DirtyFlags::BUFFER));
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

    #[test]
    fn convert_decorate_target() {
        use kasane_core::plugin::DecorateTarget;
        assert!(matches!(
            decorate_target_to_wit(&DecorateTarget::Buffer),
            wit::DecorateTarget::Buffer
        ));
        assert!(matches!(
            decorate_target_to_wit(&DecorateTarget::StatusBar),
            wit::DecorateTarget::StatusBar
        ));
        match decorate_target_to_wit(&DecorateTarget::BufferLine(42)) {
            wit::DecorateTarget::BufferLine(n) => assert_eq!(n, 42),
            other => panic!("expected BufferLine, got {other:?}"),
        }
    }

    #[test]
    fn convert_replace_target() {
        use kasane_core::plugin::ReplaceTarget;
        assert!(matches!(
            replace_target_to_wit(&ReplaceTarget::MenuPrompt),
            wit::ReplaceTarget::MenuPrompt
        ));
        assert!(matches!(
            replace_target_to_wit(&ReplaceTarget::StatusBar),
            wit::ReplaceTarget::StatusBar
        ));
    }
}
