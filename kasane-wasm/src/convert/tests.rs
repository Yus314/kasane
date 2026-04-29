use super::*;
use kasane_core::element::{BorderLineStyle, OverlayAnchor};
use kasane_core::input::{Key, KeyEvent, Modifiers, MouseButton, MouseEvent, MouseEventKind};
use kasane_core::layout::{Rect, SplitDirection, flex::Constraints};
use kasane_core::plugin::{
    AppView, Command, ContributeContext, CursorEffect, IoEvent, OrnamentModality, ProcessEvent,
    StdinMode, SurfaceOrnAnchor, SurfaceOrnKind,
};
use kasane_core::protocol::{Atom, Face, KasaneRequest};

/// Test helper: build a `wit::Style` from the legacy face-equivalent
/// fields. The legacy `Face` had four fields (fg, bg, underline,
/// attributes); the new `Style` has 12. This helper mirrors what
/// `Style::from_face` does on the host side, letting test literals
/// stay compact while exercising the WIT-level conversion path.
fn wit_style_from_face_fields(
    fg: wit::Brush,
    bg: wit::Brush,
    underline: wit::Brush,
    attributes: u16,
) -> wit::Style {
    let face = Face {
        fg: super::wit_brush_to_color(&fg),
        bg: super::wit_brush_to_color(&bg),
        underline: super::wit_brush_to_color(&underline),
        attributes: kasane_core::protocol::Attributes::from_bits_truncate(attributes),
    };
    super::face_to_wit(&face)
}
use kasane_core::render::CursorStyle;
use kasane_core::scroll::{
    DefaultScrollCandidate, ResolvedScroll, ScrollAccumulationMode, ScrollCurve, ScrollGranularity,
    ScrollPlan, ScrollPolicyResult,
};
use kasane_core::session::SessionCommand;
use kasane_core::state::{AppState, DirtyFlags};
use kasane_core::surface::{EventContext, SurfaceEvent, SurfaceId};
use kasane_core::workspace::Workspace;

#[test]
fn convert_default_color() {
    let wc = wit::Brush::DefaultColor;
    assert_eq!(wit_brush_to_color(&wc), Color::Default);
}

#[test]
fn convert_rgb_color() {
    let wc = wit::Brush::Rgb(wit::RgbColor {
        r: 40,
        g: 40,
        b: 50,
    });
    assert_eq!(
        wit_brush_to_color(&wc),
        Color::Rgb {
            r: 40,
            g: 40,
            b: 50
        }
    );
}

#[test]
fn convert_named_color() {
    let wc = wit::Brush::Named(wit::NamedColor::BrightCyan);
    assert_eq!(
        wit_brush_to_color(&wc),
        Color::Named(NamedColor::BrightCyan)
    );
}

#[test]
fn convert_face_with_attributes() {
    let ws = wit_style_from_face_fields(
        wit::Brush::Named(wit::NamedColor::Red),
        wit::Brush::Rgb(wit::RgbColor {
            r: 10,
            g: 20,
            b: 30,
        }),
        wit::Brush::DefaultColor,
        0x20, // BOLD
    );
    let f = wit_style_to_face(&ws);
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
    assert!(
        f.attributes
            .contains(kasane_core::protocol::Attributes::BOLD)
    );
}

#[test]
fn convert_atom() {
    let wa = wit::Atom {
        style: wit_style_from_face_fields(
            wit::Brush::Named(wit::NamedColor::Red),
            wit::Brush::DefaultColor,
            wit::Brush::DefaultColor,
            0,
        ),
        contents: "hello".to_string(),
    };
    let a = wit_atom_to_atom(&wa);
    assert_eq!(a.contents.as_str(), "hello");
    assert_eq!(a.face().fg, Color::Named(NamedColor::Red));
}

#[test]
fn convert_contribute_context_preserves_unbounded_max() {
    let state = AppState::default();
    let ctx = ContributeContext::from_constraints(
        &AppView::new(&state),
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
    let wc = wit::Command::PasteClipboard;
    assert!(matches!(
        wit_command_to_command(&wc),
        Command::PasteClipboard
    ));
}

#[test]
fn convert_command_unregister_surface() {
    let wc = wit::Command::UnregisterSurface("dynamic.surface".into());
    assert!(matches!(
        wit_command_to_command(&wc),
        Command::UnregisterSurfaceKey { surface_key }
        if surface_key == "dynamic.surface"
    ));
}

#[test]
fn convert_default_scroll_candidate_to_wit() {
    let candidate = DefaultScrollCandidate::new(
        10,
        5,
        Modifiers::CTRL,
        ScrollGranularity::Line,
        3,
        ResolvedScroll::new(3, 10, 5),
    );

    let wit = default_scroll_candidate_to_wit(&candidate);

    assert_eq!(wit.screen_line, 10);
    assert_eq!(wit.screen_column, 5);
    assert_eq!(wit.modifiers, Modifiers::CTRL.bits());
    assert!(matches!(wit.granularity, wit::ScrollGranularity::Line));
    assert_eq!(wit.raw_amount, 3);
    assert_eq!(wit.resolved.amount, 3);
}

#[test]
fn convert_scroll_policy_result_from_wit() {
    let wit_result = wit::ScrollPolicyResult::Plan(wit::ScrollPlan {
        total_amount: 9,
        line: 10,
        column: 5,
        frame_interval_ms: 16,
        curve: wit::ScrollCurve::Linear,
        accumulation: wit::ScrollAccumulationMode::Add,
    });

    let result = wit_scroll_policy_result_to_result(&wit_result);

    assert_eq!(
        result,
        ScrollPolicyResult::Plan(ScrollPlan::new(
            9,
            10,
            5,
            16,
            ScrollCurve::Linear,
            ScrollAccumulationMode::Add,
        ))
    );
}

#[test]
fn convert_bootstrap_effects_from_wit() {
    let effects = wit::BootstrapEffects {
        redraw: (DirtyFlags::BUFFER_CONTENT | DirtyFlags::STATUS).bits(),
    };

    let converted = wit_bootstrap_effects_to_effects(&effects);

    assert!(converted.redraw.contains(DirtyFlags::BUFFER_CONTENT));
    assert!(converted.redraw.contains(DirtyFlags::STATUS));
}

#[test]
fn convert_runtime_effects_from_wit() {
    let effects = wit::RuntimeEffects {
        redraw: DirtyFlags::SESSION.bits(),
        commands: vec![wit::Command::Quit],
        scroll_plans: vec![wit::ScrollPlan {
            total_amount: 3,
            line: 9,
            column: 2,
            frame_interval_ms: 12,
            curve: wit::ScrollCurve::Linear,
            accumulation: wit::ScrollAccumulationMode::Replace,
        }],
    };

    let converted = wit_runtime_effects_to_effects(&effects);

    assert_eq!(converted.redraw, DirtyFlags::SESSION);
    assert!(matches!(converted.commands.as_slice(), [Command::Quit]));
    assert_eq!(
        converted.scroll_plans,
        vec![ScrollPlan::new(
            3,
            9,
            2,
            12,
            ScrollCurve::Linear,
            ScrollAccumulationMode::Replace,
        )]
    );
}

#[test]
fn convert_display_directive_fold_roundtrip() {
    let directive = kasane_core::display::DisplayDirective::Fold {
        range: 2..5,
        summary: vec![Atom::plain("folded")],
    };

    let wit = display_directive_to_wit(&directive);
    let roundtrip = wit_display_directive_to_directive(&wit);

    assert_eq!(roundtrip, directive);
}

#[test]
fn convert_display_directive_hide_roundtrip() {
    let directive = kasane_core::display::DisplayDirective::Hide { range: 4..7 };

    let wit = display_directive_to_wit(&directive);
    let roundtrip = wit_display_directive_to_directive(&wit);

    assert_eq!(roundtrip, directive);
}

#[test]
fn convert_display_directive_list_roundtrip() {
    let directives = vec![kasane_core::display::DisplayDirective::Hide { range: 1..3 }];

    let wit = display_directives_to_wit(&directives);
    let roundtrip = wit_display_directives_to_directives(&wit);

    assert_eq!(roundtrip, directives);
}

#[test]
fn convert_workspace_query_to_snapshot_preserves_surface_layout() {
    let primary = SurfaceId(41);
    let secondary = SurfaceId(42);
    let mut workspace = Workspace::new(primary);
    workspace
        .root_mut()
        .split(primary, SplitDirection::Vertical, 0.5, secondary);
    workspace.focus(secondary);

    let query = workspace.query(Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    });
    let snapshot = workspace_query_to_snapshot(&query);

    assert_eq!(snapshot.surfaces, vec![41, 42]);
    assert_eq!(snapshot.focused, 42);
    assert_eq!(snapshot.surface_count, 2);
    assert_eq!(snapshot.rects.len(), 2);
    assert!(
        snapshot
            .rects
            .iter()
            .any(|rect| { rect.surface_id == 41 && rect.x == 0 && rect.y == 0 && rect.h == 24 })
    );
    assert!(
        snapshot
            .rects
            .iter()
            .any(|rect| { rect.surface_id == 42 && rect.y == 0 && rect.h == 24 })
    );
}

#[test]
fn convert_session_ready_effects_from_wit() {
    let effects = wit::SessionReadyEffects {
        redraw: DirtyFlags::STATUS.bits(),
        commands: vec![
            wit::SessionReadyCommand::SendKeys(vec!["g".into(), "g".into()]),
            wit::SessionReadyCommand::PluginMessage(wit::MessageConfig {
                target_plugin: "peer".into(),
                payload: vec![1, 2, 3],
            }),
        ],
        scroll_plans: vec![wit::ScrollPlan {
            total_amount: 5,
            line: 4,
            column: 1,
            frame_interval_ms: 16,
            curve: wit::ScrollCurve::Linear,
            accumulation: wit::ScrollAccumulationMode::Add,
        }],
    };

    let converted = wit_session_ready_effects_to_effects(&effects);

    assert_eq!(converted.redraw, DirtyFlags::STATUS);
    assert!(matches!(
        converted.commands.first(),
        Some(Command::SendToKakoune(
            KasaneRequest::Keys(keys)
        )) if keys == &vec!["g".to_string(), "g".to_string()]
    ));
    assert!(matches!(
        converted.commands.get(1),
        Some(Command::PluginMessage { target, .. })
        if target.0 == "peer"
    ));
    assert_eq!(
        converted.scroll_plans,
        vec![ScrollPlan::new(
            5,
            4,
            1,
            16,
            ScrollCurve::Linear,
            ScrollAccumulationMode::Add,
        )]
    );
}

#[test]
fn convert_render_ornament_context_to_wit() {
    let ctx = RenderOrnamentContext {
        screen_cols: 120,
        screen_rows: 40,
        visible_line_start: 10,
        visible_line_end: 20,
        ..Default::default()
    };

    let wit_ctx = render_ornament_context_to_wit(&ctx);

    assert_eq!(wit_ctx.screen_cols, 120);
    assert_eq!(wit_ctx.screen_rows, 40);
    assert_eq!(wit_ctx.visible_line_start, 10);
    assert_eq!(wit_ctx.visible_line_end, 20);
}

#[test]
fn convert_ornament_batch_from_wit() {
    let batch = wit::OrnamentBatch {
        emphasis: vec![wit::CellDecoration {
            target: wit::DecorationTarget::Column(3),
            style: wit_style_from_face_fields(
                wit::Brush::DefaultColor,
                wit::Brush::Named(wit::NamedColor::Blue),
                wit::Brush::DefaultColor,
                0,
            ),
            merge: 2,
            priority: 5,
        }],
        cursor_style: Some(wit::CursorStyleOrn {
            shape: 2,
            priority: 7,
            modality: wit::OrnamentModality::Approximate,
        }),
        cursor_effects: vec![wit::CursorEffectOrn {
            kind: wit::CursorEffect::Halo,
            style: wit_style_from_face_fields(
                wit::Brush::Named(wit::NamedColor::Yellow),
                wit::Brush::DefaultColor,
                wit::Brush::DefaultColor,
                0,
            ),
            priority: 3,
            modality: wit::OrnamentModality::Must,
        }],
        surfaces: vec![wit::SurfaceOrn {
            anchor: wit::SurfaceOrnAnchor::SurfaceKey("sidebar".into()),
            kind: wit::SurfaceOrnKind::InactiveTint,
            style: wit_style_from_face_fields(
                wit::Brush::DefaultColor,
                wit::Brush::Named(wit::NamedColor::BrightBlack),
                wit::Brush::DefaultColor,
                0,
            ),
            priority: 9,
            modality: wit::OrnamentModality::May,
        }],
    };

    let converted = wit_ornament_batch_to_ornament_batch(&batch);

    assert_eq!(converted.emphasis.len(), 1);
    assert!(matches!(
        converted.emphasis[0].target,
        kasane_core::plugin::DecorationTarget::Column { column: 3 }
    ));
    assert_eq!(converted.emphasis[0].priority, 5);

    let cursor_style = converted.cursor_style.expect("missing cursor style");
    assert_eq!(cursor_style.priority, 7);
    assert_eq!(cursor_style.modality, OrnamentModality::Approximate);
    assert_eq!(cursor_style.hint.shape, CursorStyle::Underline);

    assert_eq!(converted.cursor_effects.len(), 1);
    assert_eq!(converted.cursor_effects[0].kind, CursorEffect::Halo);
    assert_eq!(converted.cursor_effects[0].priority, 3);
    assert_eq!(converted.cursor_effects[0].modality, OrnamentModality::Must);

    assert_eq!(converted.surfaces.len(), 1);
    let surface = &converted.surfaces[0];
    assert_eq!(surface.priority, 9);
    assert_eq!(surface.modality, OrnamentModality::May);
    assert_eq!(surface.kind, SurfaceOrnKind::InactiveTint);
    assert!(matches!(
        &surface.anchor,
        SurfaceOrnAnchor::SurfaceKey(key) if key == "sidebar"
    ));
}

#[test]
fn convert_ornament_batch_invalid_cursor_shape_drops_style() {
    let batch = wit::OrnamentBatch {
        emphasis: vec![],
        cursor_style: Some(wit::CursorStyleOrn {
            shape: 99,
            priority: 10,
            modality: wit::OrnamentModality::Must,
        }),
        cursor_effects: vec![],
        surfaces: vec![],
    };

    let converted = wit_ornament_batch_to_ornament_batch(&batch);
    assert!(
        converted.cursor_style.is_none(),
        "invalid shape code should produce None, not a default Block"
    );
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
    assert!(matches!(wit_ev.key, wit::KeyCode::Char(cp) if cp == 'x' as u32));
    assert_eq!(wit_ev.modifiers, Modifiers::ALT.bits());
    let roundtrip = wit_key_event_to_key_event(&wit_ev).expect("valid key event");
    assert_eq!(roundtrip, native);
}

#[test]
fn convert_key_event_rejects_invalid_codepoint() {
    let err = wit_key_event_to_key_event(&wit::KeyEvent {
        key: wit::KeyCode::Char(0xD800), // surrogate, not a valid char
        modifiers: 0,
    })
    .expect_err("invalid codepoint should be rejected");

    assert!(err.contains("invalid Unicode codepoint"));
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
            assert!(matches!(key.key, wit::KeyCode::Char(cp) if cp == 'r' as u32));
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
        wit::Brush::DefaultColor
    ));
}

#[test]
fn convert_color_to_wit_rgb() {
    match color_to_wit(&Color::Rgb {
        r: 10,
        g: 20,
        b: 30,
    }) {
        wit::Brush::Rgb(rgb) => {
            assert_eq!((rgb.r, rgb.g, rgb.b), (10, 20, 30));
        }
        other => panic!("expected Rgb, got {other:?}"),
    }
}

#[test]
fn convert_color_to_wit_named() {
    match color_to_wit(&Color::Named(NamedColor::BrightCyan)) {
        wit::Brush::Named(n) => assert!(matches!(n, wit::NamedColor::BrightCyan)),
        other => panic!("expected Named, got {other:?}"),
    }
}

#[test]
fn convert_face_roundtrip() {
    use kasane_core::protocol::Attributes;
    let native = Face {
        fg: Color::Named(NamedColor::Red),
        bg: Color::Rgb { r: 1, g: 2, b: 3 },
        underline: Color::Default,
        attributes: Attributes::BOLD | Attributes::ITALIC,
    };
    let wit_f = face_to_wit(&native);
    let back = wit_style_to_face(&wit_f);
    assert_eq!(native.fg, back.fg);
    assert_eq!(native.bg, back.bg);
    assert_eq!(native.underline, back.underline);
    assert_eq!(native.attributes, back.attributes);
}

#[test]
fn convert_atom_roundtrip() {
    let native = Atom::plain("hello");
    let wit_a = atom_to_wit(&native);
    let back = wit_atom_to_atom(&wit_a);
    assert_eq!(native.contents.as_str(), back.contents.as_str());
}

#[test]
fn convert_command_schedule_timer() {
    use std::time::Duration;
    let wc = wit::Command::ScheduleTimer(wit::TimerConfig {
        timer_id: 42,
        delay_ms: 500,
        target_plugin: "my_plugin".into(),
        payload: vec![1, 2, 3],
    });
    match wit_command_to_command(&wc) {
        Command::ScheduleTimer {
            timer_id,
            delay,
            target,
            payload,
        } => {
            assert_eq!(timer_id, 42);
            assert_eq!(delay, Duration::from_millis(500));
            assert_eq!(target.0, "my_plugin");
            let bytes = payload.downcast::<Vec<u8>>().unwrap();
            assert_eq!(*bytes, vec![1, 2, 3]);
        }
        _ => panic!("unexpected command variant"),
    }
}

#[test]
fn convert_command_cancel_timer() {
    let wc = wit::Command::CancelTimer(99);
    match wit_command_to_command(&wc) {
        Command::CancelTimer { timer_id } => {
            assert_eq!(timer_id, 99);
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

// --- From impl roundtrip tests ---

#[test]
fn coord_native_to_wit_roundtrip() {
    let native = kasane_core::protocol::Coord {
        line: 42,
        column: 7,
    };
    let wit_c: wit::Coord = native.into();
    let back: kasane_core::protocol::Coord = wit_c.into();
    assert_eq!(native, back);
}

#[test]
fn coord_wit_to_native_roundtrip() {
    let wit_c = wit::Coord {
        line: -1,
        column: 99,
    };
    let native: kasane_core::protocol::Coord = wit_c.into();
    let back: wit::Coord = native.into();
    assert_eq!(wit_c.line, back.line);
    assert_eq!(wit_c.column, back.column);
}

#[test]
fn rect_native_to_wit_roundtrip() {
    let native = Rect {
        x: 5,
        y: 10,
        w: 80,
        h: 24,
    };
    let wit_r: wit::Rect = native.into();
    let back: Rect = wit_r.into();
    assert_eq!(native, back);
}

#[test]
fn rect_wit_to_native_roundtrip() {
    let wit_r = wit::Rect {
        x: 0,
        y: 0,
        w: 120,
        h: 40,
    };
    let native: Rect = wit_r.into();
    let back: wit::Rect = native.into();
    assert_eq!(wit_r.x, back.x);
    assert_eq!(wit_r.y, back.y);
    assert_eq!(wit_r.w, back.w);
    assert_eq!(wit_r.h, back.h);
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
        _ => panic!("expected Process variant"),
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
        _ => panic!("expected Process variant"),
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
        _ => panic!("expected Process variant"),
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
        _ => panic!("expected Process variant"),
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
                _ => panic!("expected Process variant"),
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

#[test]
fn convert_command_edit_buffer() {
    use kasane_core::plugin::{BufferEdit, BufferPosition};

    let wc = wit::Command::EditBuffer(vec![
        wit::BufferEdit {
            start_line: 3,
            start_column: 5,
            end_line: 3,
            end_column: 10,
            replacement: "hello".into(),
        },
        wit::BufferEdit {
            start_line: 1,
            start_column: 1,
            end_line: 1,
            end_column: 1,
            replacement: String::new(),
        },
    ]);
    match wit_command_to_command(&wc) {
        Command::EditBuffer { edits } => {
            assert_eq!(edits.len(), 2);
            assert_eq!(
                edits[0],
                BufferEdit {
                    start: BufferPosition { line: 3, column: 5 },
                    end: BufferPosition {
                        line: 3,
                        column: 10
                    },
                    replacement: "hello".into(),
                }
            );
            assert_eq!(
                edits[1],
                BufferEdit {
                    start: BufferPosition { line: 1, column: 1 },
                    end: BufferPosition { line: 1, column: 1 },
                    replacement: String::new(),
                }
            );
        }
        _ => panic!("expected EditBuffer"),
    }
}

#[test]
fn convert_command_inject_key() {
    use kasane_core::input::{InputEvent, Key, Modifiers};

    let wc = wit::Command::InjectKey(wit::KeyEvent {
        key: wit::KeyCode::Char('a' as u32),
        modifiers: Modifiers::CTRL.bits(),
    });
    match wit_command_to_command(&wc) {
        Command::InjectInput(InputEvent::Key(key_event)) => {
            assert_eq!(key_event.key, Key::Char('a'));
            assert_eq!(key_event.modifiers, Modifiers::CTRL);
        }
        _ => panic!("expected InjectInput(Key(...))"),
    }
}

#[test]
fn convert_workspace_resize_command() {
    let wc = wit::Command::WorkspaceCommand(wit::WorkspaceCmd::Resize(0.05));
    match wit_command_to_command(&wc) {
        Command::Workspace(kasane_core::workspace::WorkspaceCommand::Resize { delta }) => {
            assert!((delta - 0.05).abs() < f32::EPSILON);
        }
        _ => panic!("expected Workspace(Resize)"),
    }
}

// --- Phase D: ChannelValue WIT conversion tests ---

use kasane_core::plugin::channel::ChannelValue;

#[test]
fn channel_value_wit_round_trip_u32() {
    let original = ChannelValue::new(&42u32).unwrap();
    let wit_val = channel_value_to_wit(&original);
    let restored = wit_channel_value_to_core(&wit_val);
    assert_eq!(restored.deserialize::<u32>().unwrap(), 42);
    assert!(restored.type_hint().contains("u32"));
}

#[test]
fn channel_value_wit_round_trip_string() {
    let original = ChannelValue::new(&"hello world".to_string()).unwrap();
    let wit_val = channel_value_to_wit(&original);
    let restored = wit_channel_value_to_core(&wit_val);
    assert_eq!(restored.deserialize::<String>().unwrap(), "hello world");
}

#[test]
fn channel_value_wit_round_trip_vec() {
    let original = ChannelValue::new(&vec![1u32, 2, 3]).unwrap();
    let wit_val = channel_value_to_wit(&original);
    let restored = wit_channel_value_to_core(&wit_val);
    assert_eq!(restored.deserialize::<Vec<u32>>().unwrap(), vec![1, 2, 3]);
}

#[test]
fn channel_value_wit_preserves_raw_data() {
    let original = ChannelValue::new(&99u32).unwrap();
    let wit_val = channel_value_to_wit(&original);
    assert_eq!(wit_val.data, original.data());
    assert_eq!(wit_val.type_hint, original.type_hint());
    let restored = wit_channel_value_to_core(&wit_val);
    assert_eq!(original, restored);
}
