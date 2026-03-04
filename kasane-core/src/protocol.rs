use std::collections::HashMap;
use std::fmt;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use simd_json::prelude::*;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Color
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedColor {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Color {
    #[default]
    Default,
    Named(NamedColor),
    Rgb {
        r: u8,
        g: u8,
        b: u8,
    },
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ColorVisitor;

        impl<'de> Visitor<'de> for ColorVisitor {
            type Value = Color;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str(r##""default", a named color, or "#rrggbb""##)
            }

            fn visit_str<E>(self, v: &str) -> Result<Color, E>
            where
                E: de::Error,
            {
                parse_color(v).ok_or_else(|| de::Error::custom(format!("unknown color: {v}")))
            }
        }

        deserializer.deserialize_str(ColorVisitor)
    }
}

impl Serialize for Color {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Color::Default => serializer.serialize_str("default"),
            Color::Named(n) => serializer.serialize_str(named_color_str(*n)),
            Color::Rgb { r, g, b } => {
                serializer.serialize_str(&format!("rgb:{r:02x}{g:02x}{b:02x}"))
            }
        }
    }
}

fn named_color_str(c: NamedColor) -> &'static str {
    match c {
        NamedColor::Black => "black",
        NamedColor::Red => "red",
        NamedColor::Green => "green",
        NamedColor::Yellow => "yellow",
        NamedColor::Blue => "blue",
        NamedColor::Magenta => "magenta",
        NamedColor::Cyan => "cyan",
        NamedColor::White => "white",
        NamedColor::BrightBlack => "bright-black",
        NamedColor::BrightRed => "bright-red",
        NamedColor::BrightGreen => "bright-green",
        NamedColor::BrightYellow => "bright-yellow",
        NamedColor::BrightBlue => "bright-blue",
        NamedColor::BrightMagenta => "bright-magenta",
        NamedColor::BrightCyan => "bright-cyan",
        NamedColor::BrightWhite => "bright-white",
    }
}

fn parse_color(s: &str) -> Option<Color> {
    if s == "default" {
        return Some(Color::Default);
    }
    // Kakoune sends "rgb:RRGGBB", also accept "#RRGGBB" for compatibility
    let hex = s.strip_prefix("rgb:").or_else(|| s.strip_prefix('#'));
    if let Some(hex) = hex {
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(Color::Rgb { r, g, b });
        }
        return None;
    }
    let named = match s {
        "black" => NamedColor::Black,
        "red" => NamedColor::Red,
        "green" => NamedColor::Green,
        "yellow" => NamedColor::Yellow,
        "blue" => NamedColor::Blue,
        "magenta" => NamedColor::Magenta,
        "cyan" => NamedColor::Cyan,
        "white" => NamedColor::White,
        "bright-black" => NamedColor::BrightBlack,
        "bright-red" => NamedColor::BrightRed,
        "bright-green" => NamedColor::BrightGreen,
        "bright-yellow" => NamedColor::BrightYellow,
        "bright-blue" => NamedColor::BrightBlue,
        "bright-magenta" => NamedColor::BrightMagenta,
        "bright-cyan" => NamedColor::BrightCyan,
        "bright-white" => NamedColor::BrightWhite,
        _ => return None,
    };
    Some(Color::Named(named))
}

// ---------------------------------------------------------------------------
// Attribute
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Attribute {
    Underline,
    CurlyUnderline,
    DoubleUnderline,
    Reverse,
    Blink,
    Bold,
    Dim,
    Italic,
    Strikethrough,
    FinalFg,
    FinalBg,
    FinalAttr,
}

// ---------------------------------------------------------------------------
// Face
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Face {
    pub fg: Color,
    pub bg: Color,
    #[serde(default)]
    pub underline: Color,
    #[serde(default)]
    pub attributes: Vec<Attribute>,
}

impl Default for Face {
    fn default() -> Self {
        Face {
            fg: Color::Default,
            bg: Color::Default,
            underline: Color::Default,
            attributes: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Atom / Line / Coord
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Atom {
    pub face: Face,
    pub contents: String,
}

pub type Line = Vec<Atom>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
pub struct Coord {
    pub line: i32,
    pub column: i32,
}

// ---------------------------------------------------------------------------
// CursorMode / MenuStyle / InfoStyle
// ---------------------------------------------------------------------------

/// Cursor display mode sent by Kakoune's `set_cursor` message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CursorMode {
    Buffer,
    Prompt,
}

/// Menu display style sent by Kakoune's `menu_show` message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MenuStyle {
    Prompt,
    Search,
    Inline,
}

impl MenuStyle {
    /// Prompt and search styles are rendered as horizontal multi-column
    /// layouts above the status bar.
    pub fn is_prompt_like(self) -> bool {
        matches!(self, Self::Prompt | Self::Search)
    }
}

/// Info popup display style sent by Kakoune's `info_show` message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InfoStyle {
    Prompt,
    Modal,
    Inline,
    InlineAbove,
    MenuDoc,
}

impl InfoStyle {
    /// Framed styles (prompt, modal) get borders and padding.
    pub fn is_framed(self) -> bool {
        matches!(self, Self::Prompt | Self::Modal)
    }
}

// ---------------------------------------------------------------------------
// Kakoune → Kasane messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum KakouneRequest {
    Draw {
        lines: Vec<Line>,
        default_face: Face,
        padding_face: Face,
    },
    DrawStatus {
        status_line: Line,
        mode_line: Line,
        default_face: Face,
    },
    SetCursor {
        mode: CursorMode,
        coord: Coord,
    },
    MenuShow {
        items: Vec<Line>,
        anchor: Coord,
        selected_item_face: Face,
        menu_face: Face,
        style: MenuStyle,
    },
    MenuSelect {
        selected: i32,
    },
    MenuHide,
    InfoShow {
        title: Line,
        content: Vec<Line>,
        anchor: Coord,
        face: Face,
        style: InfoStyle,
    },
    InfoHide,
    SetUiOptions {
        options: HashMap<String, String>,
    },
    Refresh {
        force: bool,
    },
}

// ---------------------------------------------------------------------------
// Kasane → Kakoune messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum KasaneRequest {
    Keys(Vec<String>),
    Resize {
        rows: u16,
        cols: u16,
    },
    MousePress {
        button: String,
        line: u32,
        column: u32,
    },
    MouseRelease {
        button: String,
        line: u32,
        column: u32,
    },
    MouseMove {
        line: u32,
        column: u32,
    },
    Scroll {
        amount: i32,
        line: u32,
        column: u32,
    },
    MenuSelect(i32),
}

#[derive(Serialize)]
struct JsonRpc<'a, P: Serialize> {
    jsonrpc: &'static str,
    method: &'a str,
    params: P,
}

fn to_json_rpc<P: Serialize>(method: &str, params: P) -> String {
    serde_json::to_string(&JsonRpc {
        jsonrpc: "2.0",
        method,
        params,
    })
    .expect("KasaneRequest serialization should not fail")
}

impl KasaneRequest {
    pub fn to_json(&self) -> String {
        match self {
            Self::Keys(keys) => to_json_rpc("keys", keys),
            Self::Resize { rows, cols } => to_json_rpc("resize", (rows, cols)),
            Self::MousePress {
                button,
                line,
                column,
            } => to_json_rpc("mouse_press", (button, line, column)),
            Self::MouseRelease {
                button,
                line,
                column,
            } => to_json_rpc("mouse_release", (button, line, column)),
            Self::MouseMove { line, column } => to_json_rpc("mouse_move", (line, column)),
            Self::Scroll {
                amount,
                line,
                column,
            } => to_json_rpc("scroll", (amount, line, column)),
            Self::MenuSelect(index) => to_json_rpc("menu_select", (index,)),
        }
    }
}

// ---------------------------------------------------------------------------
// JSON-RPC parsing
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("JSON parse error: {0}")]
    Json(String),
    #[error("unknown method: {0}")]
    UnknownMethod(String),
    #[error("invalid params for {method}: {reason}")]
    InvalidParams { method: String, reason: String },
}

pub fn parse_request(input: &mut [u8]) -> Result<KakouneRequest, ProtocolError> {
    let value: simd_json::OwnedValue =
        simd_json::to_owned_value(input).map_err(|e| ProtocolError::Json(e.to_string()))?;

    let method = value
        .get("method")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ProtocolError::Json("missing method field".into()))?
        .to_string();

    let params = value
        .get("params")
        .ok_or_else(|| ProtocolError::Json("missing params field".into()))?;

    // Convert simd_json::OwnedValue -> serde_json::Value for easier deserialization
    let params_json: serde_json::Value = {
        let s =
            simd_json::serde::to_string(params).map_err(|e| ProtocolError::Json(e.to_string()))?;
        serde_json::from_str(&s).map_err(|e| ProtocolError::Json(e.to_string()))?
    };

    parse_method(&method, &params_json)
}

fn parse_method(method: &str, params: &serde_json::Value) -> Result<KakouneRequest, ProtocolError> {
    let arr = params
        .as_array()
        .ok_or_else(|| ProtocolError::InvalidParams {
            method: method.into(),
            reason: "params must be an array".into(),
        })?;

    match method {
        "draw" => {
            ensure_len(method, arr, 3)?;
            Ok(KakouneRequest::Draw {
                lines: de_param(method, &arr[0], "lines")?,
                default_face: de_param(method, &arr[1], "default_face")?,
                padding_face: de_param(method, &arr[2], "padding_face")?,
            })
        }
        "draw_status" => {
            ensure_len(method, arr, 3)?;
            Ok(KakouneRequest::DrawStatus {
                status_line: de_param(method, &arr[0], "status_line")?,
                mode_line: de_param(method, &arr[1], "mode_line")?,
                default_face: de_param(method, &arr[2], "default_face")?,
            })
        }
        "set_cursor" => {
            ensure_len(method, arr, 2)?;
            Ok(KakouneRequest::SetCursor {
                mode: de_param(method, &arr[0], "mode")?,
                coord: de_param(method, &arr[1], "coord")?,
            })
        }
        "menu_show" => {
            ensure_len(method, arr, 5)?;
            Ok(KakouneRequest::MenuShow {
                items: de_param(method, &arr[0], "items")?,
                anchor: de_param(method, &arr[1], "anchor")?,
                selected_item_face: de_param(method, &arr[2], "selected_item_face")?,
                menu_face: de_param(method, &arr[3], "menu_face")?,
                style: de_param(method, &arr[4], "style")?,
            })
        }
        "menu_select" => {
            ensure_len(method, arr, 1)?;
            Ok(KakouneRequest::MenuSelect {
                selected: de_param(method, &arr[0], "selected")?,
            })
        }
        "menu_hide" => Ok(KakouneRequest::MenuHide),
        "info_show" => {
            ensure_len(method, arr, 5)?;
            Ok(KakouneRequest::InfoShow {
                title: de_param(method, &arr[0], "title")?,
                content: de_param(method, &arr[1], "content")?,
                anchor: de_param(method, &arr[2], "anchor")?,
                face: de_param(method, &arr[3], "face")?,
                style: de_param(method, &arr[4], "style")?,
            })
        }
        "info_hide" => Ok(KakouneRequest::InfoHide),
        "set_ui_options" => {
            ensure_len(method, arr, 1)?;
            Ok(KakouneRequest::SetUiOptions {
                options: de_param(method, &arr[0], "options")?,
            })
        }
        "refresh" => {
            ensure_len(method, arr, 1)?;
            Ok(KakouneRequest::Refresh {
                force: de_param(method, &arr[0], "force")?,
            })
        }
        _ => Err(ProtocolError::UnknownMethod(method.into())),
    }
}

fn ensure_len(
    method: &str,
    arr: &[serde_json::Value],
    expected: usize,
) -> Result<(), ProtocolError> {
    if arr.len() < expected {
        return Err(ProtocolError::InvalidParams {
            method: method.into(),
            reason: format!("expected at least {expected} params, got {}", arr.len()),
        });
    }
    Ok(())
}

fn de_param<T: serde::de::DeserializeOwned>(
    method: &str,
    value: &serde_json::Value,
    name: &str,
) -> Result<T, ProtocolError> {
    serde_json::from_value(value.clone()).map_err(|e| ProtocolError::InvalidParams {
        method: method.into(),
        reason: format!("{name}: {e}"),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
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
        ],{"fg":"rgb:ebdbb2","bg":"rgb:282828","underline":"default","attributes":[]},{"fg":"rgb:504945","bg":"rgb:282828","underline":"default","attributes":[]}]}"#;
        let mut buf = json.as_bytes().to_vec();
        let req = parse_request(&mut buf).unwrap();
        match req {
            KakouneRequest::Draw {
                lines,
                default_face,
                padding_face,
            } => {
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
    fn test_color_invalid() {
        let result: Result<Color, _> = serde_json::from_str(r#""nope""#);
        assert!(result.is_err());
    }

    #[test]
    fn test_attribute_deserialize() {
        let a: Attribute = serde_json::from_str(r#""curly_underline""#).unwrap();
        assert_eq!(a, Attribute::CurlyUnderline);
    }

    #[test]
    fn test_face_deserialize() {
        let json =
            r#"{"fg":"red","bg":"default","underline":"default","attributes":["bold","italic"]}"#;
        let f: Face = serde_json::from_str(json).unwrap();
        assert_eq!(f.fg, Color::Named(NamedColor::Red));
        assert_eq!(f.bg, Color::Default);
        assert_eq!(f.attributes, vec![Attribute::Bold, Attribute::Italic]);
    }

    #[test]
    fn test_face_minimal() {
        let json = r#"{"fg":"default","bg":"default"}"#;
        let f: Face = serde_json::from_str(json).unwrap();
        assert_eq!(f, Face::default());
    }

    #[test]
    fn test_atom_deserialize() {
        let json = r#"{"face":{"fg":"default","bg":"default"},"contents":"hello"}"#;
        let a: Atom = serde_json::from_str(json).unwrap();
        assert_eq!(a.contents, "hello");
    }

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
        ],{"fg":"default","bg":"default"},{"fg":"default","bg":"default"}]}"#;
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
        let json = r#"{ "jsonrpc": "2.0", "method": "draw", "params": [[[{ "face": { "fg": "default", "bg": "default", "underline": "default", "attributes": [] }, "contents": "test\u000a" }]], { "fg": "default", "bg": "default", "underline": "default", "attributes": [] }, { "fg": "blue", "bg": "default", "underline": "default", "attributes": [] }] }"#;
        let mut buf = json.as_bytes().to_vec();
        let req = parse_request(&mut buf).unwrap();
        match req {
            KakouneRequest::Draw {
                lines,
                padding_face,
                ..
            } => {
                assert_eq!(lines.len(), 1);
                assert!(lines[0][0].contents.contains("test"));
                assert_eq!(padding_face.fg, Color::Named(NamedColor::Blue));
            }
            _ => panic!("expected Draw"),
        }
    }

    #[test]
    fn test_parse_draw_status() {
        let json = r#"{"jsonrpc":"2.0","method":"draw_status","params":[
            [{"face":{"fg":"default","bg":"default"},"contents":"prompt"}],
            [{"face":{"fg":"default","bg":"default"},"contents":"[normal]"}],
            {"fg":"default","bg":"default"}
        ]}"#;
        let mut buf = json.as_bytes().to_vec();
        let req = parse_request(&mut buf).unwrap();
        match req {
            KakouneRequest::DrawStatus {
                status_line,
                mode_line,
                ..
            } => {
                assert_eq!(status_line[0].contents, "prompt");
                assert_eq!(mode_line[0].contents, "[normal]");
            }
            _ => panic!("expected DrawStatus"),
        }
    }

    #[test]
    fn test_parse_set_cursor() {
        let json =
            r#"{"jsonrpc":"2.0","method":"set_cursor","params":["buffer",{"line":0,"column":1}]}"#;
        let mut buf = json.as_bytes().to_vec();
        let req = parse_request(&mut buf).unwrap();
        match req {
            KakouneRequest::SetCursor { mode, coord } => {
                assert_eq!(mode, CursorMode::Buffer);
                assert_eq!(coord, Coord { line: 0, column: 1 });
            }
            _ => panic!("expected SetCursor"),
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
        // Real messages captured from `kak -ui json`
        let messages = vec![
            r#"{ "jsonrpc": "2.0", "method": "set_ui_options", "params": [{}] }"#,
            r#"{ "jsonrpc": "2.0", "method": "draw", "params": [[[{ "face": { "fg": "default", "bg": "default", "underline": "default", "attributes": [] }, "contents": " " }, { "face": { "fg": "black", "bg": "white", "underline": "default", "attributes": ["final_fg","final_bg"] }, "contents": "t" }, { "face": { "fg": "default", "bg": "default", "underline": "default", "attributes": [] }, "contents": "est\u000a" }]], { "fg": "default", "bg": "default", "underline": "default", "attributes": [] }, { "fg": "blue", "bg": "default", "underline": "default", "attributes": [] }] }"#,
            r#"{ "jsonrpc": "2.0", "method": "draw_status", "params": [[], [{ "face": { "fg": "default", "bg": "default", "underline": "default", "attributes": [] }, "contents": "file.txt 1:1 " }], { "fg": "cyan", "bg": "default", "underline": "default", "attributes": [] }] }"#,
            r#"{ "jsonrpc": "2.0", "method": "set_cursor", "params": ["buffer", { "line": 0, "column": 1 }] }"#,
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
}
