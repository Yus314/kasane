use super::*;
use kasane_core::element::{BorderLineStyle, OverlayAnchor};
use kasane_core::input::{Key, KeyEvent, Modifiers, MouseButton, MouseEvent, MouseEventKind};
use kasane_core::layout::flex::Constraints;
use kasane_core::plugin::{Command, ContributeContext, IoEvent, ProcessEvent, StdinMode};
use kasane_core::protocol::KasaneRequest;
use kasane_core::session::SessionCommand;
use kasane_core::state::{AppState, DirtyFlags};
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
    use super::input::key_event_to_wit;
    let backspace_ev = key_event_to_wit(&KeyEvent {
        key: Key::Backspace,
        modifiers: Modifiers::empty(),
    });
    assert!(matches!(backspace_ev.key, wit::KeyCode::Backspace));

    let f5_ev = key_event_to_wit(&KeyEvent {
        key: Key::F(5),
        modifiers: Modifiers::empty(),
    });
    assert!(matches!(f5_ev.key, wit::KeyCode::FKey(5)));

    let pgup_ev = key_event_to_wit(&KeyEvent {
        key: Key::PageUp,
        modifiers: Modifiers::empty(),
    });
    assert!(matches!(pgup_ev.key, wit::KeyCode::PageUp));

    let left_ev = key_event_to_wit(&KeyEvent {
        key: Key::Left,
        modifiers: Modifiers::empty(),
    });
    assert!(matches!(left_ev.key, wit::KeyCode::LeftArrow));
}

#[test]
fn convert_mouse_event_kinds() {
    let release_ev = mouse_event_to_wit(&MouseEvent {
        kind: MouseEventKind::Release(MouseButton::Right),
        line: 0,
        column: 0,
        modifiers: Modifiers::empty(),
    });
    assert!(matches!(
        release_ev.kind,
        wit::MouseEventKind::Release(wit::MouseButton::Right)
    ));

    let move_ev = mouse_event_to_wit(&MouseEvent {
        kind: MouseEventKind::Move,
        line: 0,
        column: 0,
        modifiers: Modifiers::empty(),
    });
    assert!(matches!(move_ev.kind, wit::MouseEventKind::MoveEvent));

    let scroll_ev = mouse_event_to_wit(&MouseEvent {
        kind: MouseEventKind::ScrollDown,
        line: 0,
        column: 0,
        modifiers: Modifiers::empty(),
    });
    assert!(matches!(scroll_ev.kind, wit::MouseEventKind::ScrollDown));
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
    use std::time::Duration;
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

#[test]
fn convert_command_switch_session() {
    let wc = wit::Command::SwitchSession("work".to_string());
    match wit_command_to_command(&wc) {
        Command::Session(SessionCommand::Switch { key }) => {
            assert_eq!(key, "work");
        }
        _ => panic!("expected Session::Switch"),
    }
}
