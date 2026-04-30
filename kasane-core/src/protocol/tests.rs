use super::*;

#[test]
fn test_color_default() {
    let c: Color = serde_json::from_str(r#""default""#).unwrap();
    assert_eq!(c, Color::Default);
}

#[test]
fn test_color_named() {
    let c: Color = serde_json::from_str(r#""red""#).unwrap();
    assert_eq!(c, Color::Named(NamedColor::Red));
}

#[test]
fn test_color_bright_named() {
    let c: Color = serde_json::from_str(r#""bright-cyan""#).unwrap();
    assert_eq!(c, Color::Named(NamedColor::BrightCyan));
}

#[test]
fn test_color_rgb() {
    let c: Color = serde_json::from_str(r#""rgb:ff00ab""#).unwrap();
    assert_eq!(
        c,
        Color::Rgb {
            r: 255,
            g: 0,
            b: 171
        }
    );
}

#[test]
fn test_color_rgb_hash_compat() {
    let c: Color = serde_json::from_str(r##""#ff00ab""##).unwrap();
    assert_eq!(
        c,
        Color::Rgb {
            r: 255,
            g: 0,
            b: 171
        }
    );
}

#[test]
fn test_color_rgb_roundtrip() {
    let original = Color::Rgb {
        r: 0xeb,
        g: 0xdb,
        b: 0xb2,
    };
    let json = serde_json::to_string(&original).unwrap();
    assert_eq!(json, r#""rgb:ebdbb2""#);
    let parsed: Color = serde_json::from_str(&json).unwrap();
    assert_eq!(original, parsed);
}

#[test]
fn test_parse_draw_with_rgb_faces() {
    // Simulates gruvbox-style draw message with RGB default_face
    let json = r#"{"jsonrpc":"2.0","method":"draw","params":[[
        [{"face":{"fg":"rgb:ebdbb2","bg":"rgb:282828","underline":"default","attributes":[]},"contents":"hello"}]
    ],{"line":0,"column":0},{"fg":"rgb:ebdbb2","bg":"rgb:282828","underline":"default","attributes":[]},{"fg":"rgb:504945","bg":"rgb:282828","underline":"default","attributes":[]},0]}"#;
    let mut buf = json.as_bytes().to_vec();
    let req = parse_request(&mut buf).unwrap();
    match req {
        KakouneRequest::Draw {
            lines,
            default_style,
            padding_style,
            ..
        } => {
            let default_face = default_style.to_face();
            let padding_face = padding_style.to_face();
            assert_eq!(lines.len(), 1);
            assert_eq!(lines[0][0].contents, "hello");
            assert_eq!(
                default_face.fg,
                Color::Rgb {
                    r: 0xeb,
                    g: 0xdb,
                    b: 0xb2
                }
            );
            assert_eq!(
                default_face.bg,
                Color::Rgb {
                    r: 0x28,
                    g: 0x28,
                    b: 0x28
                }
            );
            assert_eq!(
                padding_face.fg,
                Color::Rgb {
                    r: 0x50,
                    g: 0x49,
                    b: 0x45
                }
            );
        }
        _ => panic!("expected Draw"),
    }
}

#[test]
fn test_color_rgba_strips_alpha() {
    let c: Color = serde_json::from_str(r#""rgba:46465080""#).unwrap();
    assert_eq!(
        c,
        Color::Rgb {
            r: 0x46,
            g: 0x46,
            b: 0x50
        }
    );
}

#[test]
fn test_color_rgba_fully_opaque() {
    let c: Color = serde_json::from_str(r#""rgba:ff00abff""#).unwrap();
    assert_eq!(
        c,
        Color::Rgb {
            r: 255,
            g: 0,
            b: 171
        }
    );
}

#[test]
fn test_color_rgba_invalid_length() {
    let result: Result<Color, _> = serde_json::from_str(r#""rgba:ff00ab""#);
    assert!(result.is_err());
}

#[test]
fn test_color_invalid() {
    let result: Result<Color, _> = serde_json::from_str(r#""nope""#);
    assert!(result.is_err());
}

#[test]
fn test_attributes_deserialize() {
    let a: Attributes = serde_json::from_str(r#"["curly_underline"]"#).unwrap();
    assert_eq!(a, Attributes::CURLY_UNDERLINE);
}

#[test]
fn test_attributes_roundtrip() {
    let original = Attributes::BOLD | Attributes::ITALIC;
    let json = serde_json::to_string(&original).unwrap();
    let parsed: Attributes = serde_json::from_str(&json).unwrap();
    assert_eq!(original, parsed);
}

#[test]
fn test_face_deserialize() {
    let json =
        r#"{"fg":"red","bg":"default","underline":"default","attributes":["bold","italic"]}"#;
    let f: WireFace = serde_json::from_str(json).unwrap();
    assert_eq!(f.fg, Color::Named(NamedColor::Red));
    assert_eq!(f.bg, Color::Default);
    assert_eq!(f.attributes, Attributes::BOLD | Attributes::ITALIC);
}

#[test]
fn test_face_minimal() {
    let json = r#"{"fg":"default","bg":"default"}"#;
    let f: WireFace = serde_json::from_str(json).unwrap();
    assert_eq!(f, WireFace::default());
}

// `test_atom_deserialize`: removed in ADR-031 Phase A.2 — `Atom` no longer
// implements `Deserialize` directly because its `style_id` is host-side state
// (an interner key), not a wire-format field. Deserialisation goes through
// the `WireAtom` → `intern_line` path inside `parse_request`, which the
// `test_*` tests below exercise end-to-end.

#[test]
fn test_coord_deserialize() {
    let json = r#"{"line":10,"column":5}"#;
    let c: Coord = serde_json::from_str(json).unwrap();
    assert_eq!(
        c,
        Coord {
            line: 10,
            column: 5
        }
    );
}

#[test]
fn test_parse_draw() {
    let json = r#"{"jsonrpc":"2.0","method":"draw","params":[[
        [{"face":{"fg":"default","bg":"default"},"contents":"hello"}]
    ],{"line":0,"column":0},{"fg":"default","bg":"default"},{"fg":"default","bg":"default"},0]}"#;
    let mut buf = json.as_bytes().to_vec();
    let req = parse_request(&mut buf).unwrap();
    match req {
        KakouneRequest::Draw { lines, .. } => {
            assert_eq!(lines.len(), 1);
            assert_eq!(lines[0][0].contents, "hello");
        }
        _ => panic!("expected Draw"),
    }
}

#[test]
fn test_parse_draw_real_kakoune() {
    // Real Kakoune output format
    let json = r#"{ "jsonrpc": "2.0", "method": "draw", "params": [[[{ "face": { "fg": "default", "bg": "default", "underline": "default", "attributes": [] }, "contents": "test\u000a" }]], { "line": 0, "column": 0 }, { "fg": "default", "bg": "default", "underline": "default", "attributes": [] }, { "fg": "blue", "bg": "default", "underline": "default", "attributes": [] }, 0] }"#;
    let mut buf = json.as_bytes().to_vec();
    let req = parse_request(&mut buf).unwrap();
    match req {
        KakouneRequest::Draw {
            lines,
            padding_style,
            ..
        } => {
            let padding_face = padding_style.to_face();
            assert_eq!(lines.len(), 1);
            assert!(lines[0][0].contents.contains("test"));
            assert_eq!(padding_face.fg, Color::Named(NamedColor::Blue));
        }
        _ => panic!("expected Draw"),
    }
}

#[test]
fn test_parse_draw_without_widget_columns() {
    // Kakoune versions before PR #5455 send draw with 4 params (no widget_columns).
    let json = r#"{"jsonrpc":"2.0","method":"draw","params":[[
        [{"face":{"fg":"default","bg":"default","underline":"default","attributes":[]},"contents":"hello"}]
    ],{"line":0,"column":0},{"fg":"default","bg":"default","underline":"default","attributes":[]},{"fg":"default","bg":"default","underline":"default","attributes":[]}]}"#;
    let mut buf = json.as_bytes().to_vec();
    let req = parse_request(&mut buf).unwrap();
    match req {
        KakouneRequest::Draw {
            lines,
            widget_columns,
            ..
        } => {
            assert_eq!(lines.len(), 1);
            assert_eq!(lines[0][0].contents, "hello");
            assert_eq!(widget_columns, 0);
        }
        _ => panic!("expected Draw"),
    }
}

#[test]
fn test_parse_draw_status() {
    let json = r#"{"jsonrpc":"2.0","method":"draw_status","params":[
        [{"face":{"fg":"default","bg":"default"},"contents":":"}],
        [{"face":{"fg":"default","bg":"default"},"contents":"hello"}],
        3,
        [{"face":{"fg":"default","bg":"default"},"contents":"insert"},
         {"face":{"fg":"default","bg":"default"},"contents":" 1 sel"}],
        {"fg":"default","bg":"default"},
        "status"
    ]}"#;
    let mut buf = json.as_bytes().to_vec();
    let req = parse_request(&mut buf).unwrap();
    match req {
        KakouneRequest::DrawStatus {
            prompt,
            content,
            content_cursor_pos,
            mode_line,
            style,
            ..
        } => {
            assert_eq!(prompt[0].contents, ":");
            assert_eq!(content[0].contents, "hello");
            assert_eq!(content_cursor_pos, 3);
            assert_eq!(mode_line[0].contents, "insert");
            assert_eq!(style, StatusStyle::Status);
        }
        _ => panic!("expected DrawStatus"),
    }
}

#[test]
fn test_parse_draw_status_backward_compat() {
    // Pre-PR#5458 Kakoune sends 5 params (no style).
    let json = r#"{"jsonrpc":"2.0","method":"draw_status","params":[
        [{"face":{"fg":"default","bg":"default"},"contents":":"}],
        [{"face":{"fg":"default","bg":"default"},"contents":"hello"}],
        3,
        [{"face":{"fg":"default","bg":"default"},"contents":"insert"}],
        {"fg":"default","bg":"default"}
    ]}"#;
    let mut buf = json.as_bytes().to_vec();
    let req = parse_request(&mut buf).unwrap();
    match req {
        KakouneRequest::DrawStatus { style, .. } => {
            assert_eq!(style, StatusStyle::default());
        }
        _ => panic!("expected DrawStatus"),
    }
}

#[test]
fn test_parse_draw_status_command_style() {
    let json = r#"{"jsonrpc":"2.0","method":"draw_status","params":[
        [{"face":{"fg":"default","bg":"default"},"contents":":"}],
        [{"face":{"fg":"default","bg":"default"},"contents":"quit"}],
        4,
        [{"face":{"fg":"default","bg":"default"},"contents":"normal"}],
        {"fg":"default","bg":"default"},
        "command"
    ]}"#;
    let mut buf = json.as_bytes().to_vec();
    let req = parse_request(&mut buf).unwrap();
    match req {
        KakouneRequest::DrawStatus { style, .. } => {
            assert_eq!(style, StatusStyle::Command);
        }
        _ => panic!("expected DrawStatus"),
    }
}

#[test]
fn test_parse_menu_show() {
    let json = r##"{"jsonrpc":"2.0","method":"menu_show","params":[
        [[{"face":{"fg":"default","bg":"default"},"contents":"item1"}]],
        {"line":1,"column":0},
        {"fg":"default","bg":"#ff0000"},
        {"fg":"default","bg":"default"},
        "inline"
    ]}"##;
    let mut buf = json.as_bytes().to_vec();
    let req = parse_request(&mut buf).unwrap();
    match req {
        KakouneRequest::MenuShow { items, style, .. } => {
            assert_eq!(items.len(), 1);
            assert_eq!(style, MenuStyle::Inline);
        }
        _ => panic!("expected MenuShow"),
    }
}

#[test]
fn test_parse_menu_select() {
    let json = r#"{"jsonrpc":"2.0","method":"menu_select","params":[2]}"#;
    let mut buf = json.as_bytes().to_vec();
    let req = parse_request(&mut buf).unwrap();
    assert_eq!(req, KakouneRequest::MenuSelect { selected: 2 });
}

#[test]
fn test_parse_menu_hide() {
    let json = r#"{"jsonrpc":"2.0","method":"menu_hide","params":[]}"#;
    let mut buf = json.as_bytes().to_vec();
    let req = parse_request(&mut buf).unwrap();
    assert_eq!(req, KakouneRequest::MenuHide);
}

#[test]
fn test_parse_info_show() {
    let json = r#"{"jsonrpc":"2.0","method":"info_show","params":[
        [{"face":{"fg":"default","bg":"default"},"contents":"Title"}],
        [[{"face":{"fg":"default","bg":"default"},"contents":"body line"}]],
        {"line":0,"column":0},
        {"fg":"default","bg":"default"},
        "modal"
    ]}"#;
    let mut buf = json.as_bytes().to_vec();
    let req = parse_request(&mut buf).unwrap();
    match req {
        KakouneRequest::InfoShow { style, content, .. } => {
            assert_eq!(style, InfoStyle::Modal);
            assert_eq!(content.len(), 1);
        }
        _ => panic!("expected InfoShow"),
    }
}

#[test]
fn test_parse_info_hide() {
    let json = r#"{"jsonrpc":"2.0","method":"info_hide","params":[]}"#;
    let mut buf = json.as_bytes().to_vec();
    let req = parse_request(&mut buf).unwrap();
    assert_eq!(req, KakouneRequest::InfoHide);
}

#[test]
fn test_parse_set_ui_options() {
    let json =
        r#"{"jsonrpc":"2.0","method":"set_ui_options","params":[{"ncurses_set_title":"yes"}]}"#;
    let mut buf = json.as_bytes().to_vec();
    let req = parse_request(&mut buf).unwrap();
    match req {
        KakouneRequest::SetUiOptions { options } => {
            assert_eq!(options.get("ncurses_set_title"), Some(&"yes".to_string()));
        }
        _ => panic!("expected SetUiOptions"),
    }
}

#[test]
fn test_parse_refresh() {
    let json = r#"{"jsonrpc":"2.0","method":"refresh","params":[true]}"#;
    let mut buf = json.as_bytes().to_vec();
    let req = parse_request(&mut buf).unwrap();
    assert_eq!(req, KakouneRequest::Refresh { force: true });
}

#[test]
fn test_parse_unknown_method() {
    let json = r#"{"jsonrpc":"2.0","method":"bogus","params":[]}"#;
    let mut buf = json.as_bytes().to_vec();
    let err = parse_request(&mut buf).unwrap_err();
    assert!(matches!(err, ProtocolError::UnknownMethod(_)));
}

#[test]
fn test_kasane_request_keys_json() {
    let req = KasaneRequest::Keys(vec!["a".into(), "<c-x>".into()]);
    let json = req.to_json();
    assert_eq!(
        json,
        r#"{"jsonrpc":"2.0","method":"keys","params":["a","<c-x>"]}"#
    );
    let _: serde_json::Value = serde_json::from_str(&json).unwrap();
}

#[test]
fn test_kasane_request_resize_json() {
    let req = KasaneRequest::Resize { rows: 24, cols: 80 };
    let json = req.to_json();
    assert_eq!(
        json,
        r#"{"jsonrpc":"2.0","method":"resize","params":[24,80]}"#
    );
    let _: serde_json::Value = serde_json::from_str(&json).unwrap();
}

#[test]
fn test_kasane_request_mouse_press_json() {
    let req = KasaneRequest::MousePress {
        button: "left".into(),
        line: 5,
        column: 10,
    };
    let json = req.to_json();
    assert_eq!(
        json,
        r#"{"jsonrpc":"2.0","method":"mouse_press","params":["left",5,10]}"#
    );
    let _: serde_json::Value = serde_json::from_str(&json).unwrap();
}

#[test]
fn test_kasane_request_mouse_release_json() {
    let req = KasaneRequest::MouseRelease {
        button: "left".into(),
        line: 5,
        column: 10,
    };
    let json = req.to_json();
    assert_eq!(
        json,
        r#"{"jsonrpc":"2.0","method":"mouse_release","params":["left",5,10]}"#
    );
    let _: serde_json::Value = serde_json::from_str(&json).unwrap();
}

#[test]
fn test_kasane_request_mouse_move_json() {
    let req = KasaneRequest::MouseMove {
        line: 5,
        column: 10,
    };
    let json = req.to_json();
    assert_eq!(
        json,
        r#"{"jsonrpc":"2.0","method":"mouse_move","params":[5,10]}"#
    );
    let _: serde_json::Value = serde_json::from_str(&json).unwrap();
}

#[test]
fn test_kasane_request_scroll_json() {
    let req = KasaneRequest::Scroll {
        amount: 3,
        line: 5,
        column: 10,
    };
    let json = req.to_json();
    assert_eq!(
        json,
        r#"{"jsonrpc":"2.0","method":"scroll","params":[3,5,10]}"#
    );
    let _: serde_json::Value = serde_json::from_str(&json).unwrap();
}

#[test]
fn test_kasane_request_menu_select_json() {
    let req = KasaneRequest::MenuSelect(2);
    let json = req.to_json();
    assert_eq!(
        json,
        r#"{"jsonrpc":"2.0","method":"menu_select","params":[2]}"#
    );
    let _: serde_json::Value = serde_json::from_str(&json).unwrap();
}

#[test]
fn test_parse_real_kakoune_session() {
    // Real messages captured from `kak -ui json` (new protocol format)
    let messages = [
        r#"{ "jsonrpc": "2.0", "method": "set_ui_options", "params": [{}] }"#,
        r#"{ "jsonrpc": "2.0", "method": "draw", "params": [[[{ "face": { "fg": "default", "bg": "default", "underline": "default", "attributes": [] }, "contents": " " }, { "face": { "fg": "black", "bg": "white", "underline": "default", "attributes": ["final_fg","final_bg"] }, "contents": "t" }, { "face": { "fg": "default", "bg": "default", "underline": "default", "attributes": [] }, "contents": "est\u000a" }]], { "line": 0, "column": 1 }, { "fg": "default", "bg": "default", "underline": "default", "attributes": [] }, { "fg": "blue", "bg": "default", "underline": "default", "attributes": [] }, 0] }"#,
        r#"{ "jsonrpc": "2.0", "method": "draw_status", "params": [[], [], -1, [{ "face": { "fg": "default", "bg": "default", "underline": "default", "attributes": [] }, "contents": "file.txt 1:1 " }, { "face": { "fg": "black", "bg": "yellow", "underline": "default", "attributes": [] }, "contents": "" }, { "face": { "fg": "default", "bg": "default", "underline": "default", "attributes": [] }, "contents": " " }, { "face": { "fg": "blue", "bg": "default", "underline": "default", "attributes": [] }, "contents": "1 sel" }, { "face": { "fg": "default", "bg": "default", "underline": "default", "attributes": [] }, "contents": " - client0@[session]" }], { "fg": "cyan", "bg": "default", "underline": "default", "attributes": [] }] }"#,
        r#"{ "jsonrpc": "2.0", "method": "refresh", "params": [false] }"#,
    ];

    for (i, msg) in messages.iter().enumerate() {
        let mut buf = msg.as_bytes().to_vec();
        let result = parse_request(&mut buf);
        assert!(
            result.is_ok(),
            "message {i} failed to parse: {:?}",
            result.err()
        );
    }
}

#[test]
fn test_color_roundtrip() {
    let colors = vec![
        Color::Default,
        Color::Named(NamedColor::Red),
        Color::Rgb {
            r: 0,
            g: 128,
            b: 255,
        },
    ];
    for c in colors {
        let json = serde_json::to_string(&c).unwrap();
        let parsed: Color = serde_json::from_str(&json).unwrap();
        assert_eq!(c, parsed);
    }
}

#[test]
fn test_parse_draw_with_rgba_selection_face() {
    // Simulates third-party themes (e.g. anhsirk0/kakoune-themes) that use
    // rgba:RRGGBBAA for PrimarySelection/SecondarySelection backgrounds.
    let json = r#"{"jsonrpc":"2.0","method":"draw","params":[[
        [{"face":{"fg":"rgb:abb2bf","bg":"rgba:46465080","underline":"default","attributes":[]},"contents":"hello"}]
    ],{"line":0,"column":0},{"fg":"rgb:abb2bf","bg":"default","underline":"default","attributes":[]},{"fg":"rgb:5c6370","bg":"default","underline":"default","attributes":[]},0]}"#;
    let mut buf = json.as_bytes().to_vec();
    let req = parse_request(&mut buf).unwrap();
    match req {
        KakouneRequest::Draw { lines, .. } => {
            assert_eq!(
                lines[0][0].unresolved_style().to_face().bg,
                Color::Rgb {
                    r: 0x46,
                    g: 0x46,
                    b: 0x50
                }
            );
        }
        _ => panic!("expected Draw"),
    }
}

#[test]
fn test_parse_old_protocol_set_cursor() {
    let json =
        r#"{"jsonrpc":"2.0","method":"set_cursor","params":["buffer",{"line":0,"column":0}]}"#;
    let mut buf = json.as_bytes().to_vec();
    let err = parse_request(&mut buf).unwrap_err();
    assert!(matches!(err, ProtocolError::OldProtocol(_)));
    let msg = err.to_string();
    assert!(msg.contains("Kakoune v2026.04.12"));
    assert!(msg.contains("older protocol"));
}

// ---------------------------------------------------------------------------
// ADR-031: Style projection from wire-format WireFace
// ---------------------------------------------------------------------------
//
// Atom no longer derives Deserialize (its style_id is an interner key, not
// a wire field). The tests below feed WireFace JSON to Style::from_face — the
// same path the protocol parser takes when it builds an `Atom` via
// `Atom::from_wire`.

fn parse_face_json(json: &str) -> WireFace {
    serde_json::from_str(json).unwrap()
}

#[test]
fn test_atom_style_projection_from_wire_format() {
    let face = parse_face_json(
        r#"{"fg":"red","bg":"black","underline":"default","attributes":["bold","italic"]}"#,
    );
    let style = Style::from_face(&face);
    assert_eq!(style.fg, Brush::Named(NamedColor::Red));
    assert_eq!(style.bg, Brush::Named(NamedColor::Black));
    assert_eq!(style.font_weight, FontWeight::BOLD);
    assert_eq!(style.font_slant, FontSlant::Italic);
    assert!(style.underline.is_none());
    assert!(!style.blink);
    assert!(!style.reverse);
}

#[test]
fn test_atom_style_projection_decorations() {
    let face = parse_face_json(
        r#"{"fg":"default","bg":"default","underline":"red","attributes":["curly_underline","strikethrough"]}"#,
    );
    let style = Style::from_face(&face);
    let underline = style.underline.expect("underline expected");
    assert_eq!(underline.style, DecorationStyle::Curly);
    assert_eq!(underline.color, Brush::Named(NamedColor::Red));
    assert!(style.strikethrough.is_some());
}

#[test]
fn test_atom_style_projection_final_flags() {
    let face = parse_face_json(
        r#"{"fg":"black","bg":"white","underline":"default","attributes":["final_fg","final_bg"]}"#,
    );
    let unresolved = crate::protocol::style::UnresolvedStyle::from_face(&face);
    assert!(unresolved.final_fg);
    assert!(unresolved.final_bg);
    assert!(!unresolved.final_style);
    assert_eq!(unresolved.style.fg, Brush::Named(NamedColor::Black));
}

#[test]
fn test_atom_style_projection_rgb_brushes() {
    let face = parse_face_json(
        r#"{"fg":"rgb:ff0080","bg":"rgb:204060","underline":"default","attributes":[]}"#,
    );
    let style = Style::from_face(&face);
    assert_eq!(style.fg, Brush::rgb(0xff, 0x00, 0x80));
    assert_eq!(style.bg, Brush::rgb(0x20, 0x40, 0x60));
}

#[test]
fn test_atom_style_round_trip_face() {
    // UnresolvedStyle::from_face followed by ::to_face must round-trip any
    // face whose attributes lie within the legacy bitset, including the
    // Kakoune `final_*` resolution flags.
    let face = parse_face_json(
        r#"{"fg":"red","bg":"blue","underline":"green","attributes":["bold","italic","dim","curly_underline","final_fg"]}"#,
    );
    let unresolved = crate::protocol::style::UnresolvedStyle::from_face(&face);
    assert_eq!(unresolved.to_face(), face);

    // The post-resolve Style form drops `final_*`; round-trip works only on
    // faces without those flags.
    let mut face_no_final = face;
    face_no_final
        .attributes
        .remove(crate::protocol::Attributes::FINAL_FG);
    let style = Style::from_face(&face_no_final);
    assert_eq!(style.to_face(), face_no_final);
}
