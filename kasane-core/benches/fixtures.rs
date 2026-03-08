use std::any::Any;

use kasane_core::element::{Element, InteractiveId};
use kasane_core::input::{KeyEvent, MouseEvent};
use kasane_core::plugin::{Command, DecorateTarget, Plugin, PluginId, PluginRegistry, Slot};
use kasane_core::protocol::{
    Atom, Attributes, Color, Coord, Face, KakouneRequest, Line, MenuStyle, NamedColor,
};
use kasane_core::state::AppState;
use serde::Serialize;

// ---------------------------------------------------------------------------
// Dummy plugin for benchmarks
// ---------------------------------------------------------------------------

struct BenchPlugin {
    id: String,
}

impl Plugin for BenchPlugin {
    fn id(&self) -> PluginId {
        PluginId(self.id.clone())
    }

    fn update(&mut self, _msg: Box<dyn Any>, _state: &AppState) -> Vec<Command> {
        vec![]
    }

    fn handle_key(&mut self, _key: &KeyEvent, _state: &AppState) -> Option<Vec<Command>> {
        None
    }

    fn handle_mouse(
        &mut self,
        _event: &MouseEvent,
        _id: InteractiveId,
        _state: &AppState,
    ) -> Option<Vec<Command>> {
        None
    }

    fn contribute(&self, slot: Slot, _state: &AppState) -> Option<Element> {
        match slot {
            Slot::StatusRight => Some(Element::text(format!("[{}]", self.id), Default::default())),
            _ => None,
        }
    }

    fn decorate(&self, _target: DecorateTarget, element: Element, _state: &AppState) -> Element {
        // Wrap in a transparent container (minimal overhead, realistic decoration)
        element
    }

    fn decorator_priority(&self) -> u32 {
        0
    }
}

// ---------------------------------------------------------------------------
// Fixture builders
// ---------------------------------------------------------------------------

/// Build a tree-sitter-style colored line with keyword + identifier + literal atoms.
fn make_colored_line(i: usize) -> Line {
    let keyword_face = Face {
        fg: Color::Rgb {
            r: 255,
            g: 100,
            b: 0,
        },
        bg: Color::Default,
        ..Face::default()
    };
    let ident_face = Face {
        fg: Color::Rgb {
            r: 0,
            g: 200,
            b: 100,
        },
        bg: Color::Default,
        ..Face::default()
    };
    let literal_face = Face {
        fg: Color::Rgb {
            r: 100,
            g: 100,
            b: 255,
        },
        bg: Color::Default,
        ..Face::default()
    };
    let plain_face = Face::default();

    vec![
        Atom {
            face: keyword_face,
            contents: "let".into(),
        },
        Atom {
            face: plain_face,
            contents: " ".into(),
        },
        Atom {
            face: ident_face,
            contents: format!("var_{i}").into(),
        },
        Atom {
            face: plain_face,
            contents: " = ".into(),
        },
        Atom {
            face: literal_face,
            contents: format!("\"{i}_value\"").into(),
        },
        Atom {
            face: plain_face,
            contents: ";".into(),
        },
    ]
}

/// Create an `AppState` representing a typical 80x24 editor with colored buffer lines.
pub fn typical_state(line_count: usize) -> AppState {
    let mut state = AppState::default();
    state.cols = 80;
    state.rows = 24;
    state.default_face = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    };
    state.padding_face = state.default_face;
    state.status_default_face = Face {
        fg: Color::Named(NamedColor::Cyan),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    };
    state.lines = (0..line_count).map(make_colored_line).collect();
    state.status_line = vec![Atom {
        face: Face::default(),
        contents: " NORMAL ".into(),
    }];
    state.status_mode_line = vec![Atom {
        face: Face::default(),
        contents: "normal".into(),
    }];
    state
}

// ---------------------------------------------------------------------------
// Realistic fixture builders (diverse faces, varied line lengths, wide chars)
// ---------------------------------------------------------------------------

fn keyword_face() -> Face {
    Face {
        fg: Color::Rgb {
            r: 255,
            g: 100,
            b: 0,
        },
        bg: Color::Default,
        ..Face::default()
    }
}

fn ident_face() -> Face {
    Face {
        fg: Color::Rgb {
            r: 0,
            g: 200,
            b: 100,
        },
        bg: Color::Default,
        ..Face::default()
    }
}

fn literal_face() -> Face {
    Face {
        fg: Color::Rgb {
            r: 100,
            g: 100,
            b: 255,
        },
        bg: Color::Default,
        ..Face::default()
    }
}

fn comment_face() -> Face {
    Face {
        fg: Color::Rgb {
            r: 128,
            g: 128,
            b: 128,
        },
        bg: Color::Default,
        attributes: Attributes::ITALIC,
        ..Face::default()
    }
}

fn type_face() -> Face {
    Face {
        fg: Color::Named(NamedColor::Cyan),
        bg: Color::Default,
        ..Face::default()
    }
}

fn operator_face() -> Face {
    Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Default,
        ..Face::default()
    }
}

fn string_face() -> Face {
    Face {
        fg: Color::Named(NamedColor::Yellow),
        bg: Color::Default,
        ..Face::default()
    }
}

fn error_face() -> Face {
    Face {
        fg: Color::Named(NamedColor::BrightRed),
        bg: Color::Default,
        attributes: Attributes::BOLD | Attributes::UNDERLINE,
        ..Face::default()
    }
}

fn namespace_face() -> Face {
    Face {
        fg: Color::Named(NamedColor::Magenta),
        bg: Color::Default,
        ..Face::default()
    }
}

fn constant_face() -> Face {
    Face {
        fg: Color::Named(NamedColor::BrightBlue),
        bg: Color::Default,
        ..Face::default()
    }
}

fn short_comment_line(i: usize) -> Line {
    vec![Atom {
        face: comment_face(),
        contents: format!("// comment line {i}").into(),
    }]
}

fn function_def_line(i: usize) -> Line {
    vec![
        Atom {
            face: keyword_face(),
            contents: "fn ".into(),
        },
        Atom {
            face: ident_face(),
            contents: format!("process_{i}").into(),
        },
        Atom {
            face: operator_face(),
            contents: "(".into(),
        },
        Atom {
            face: type_face(),
            contents: "u32".into(),
        },
        Atom {
            face: operator_face(),
            contents: ") {".into(),
        },
    ]
}

fn long_code_line(i: usize) -> Line {
    vec![
        Atom {
            face: keyword_face(),
            contents: "    let ".into(),
        },
        Atom {
            face: ident_face(),
            contents: format!("result_{i}").into(),
        },
        Atom {
            face: operator_face(),
            contents: " = ".into(),
        },
        Atom {
            face: namespace_face(),
            contents: "self".into(),
        },
        Atom {
            face: operator_face(),
            contents: ".".into(),
        },
        Atom {
            face: ident_face(),
            contents: format!("compute_{i}").into(),
        },
        Atom {
            face: operator_face(),
            contents: "(".into(),
        },
        Atom {
            face: literal_face(),
            contents: format!("{}", i * 42).into(),
        },
        Atom {
            face: operator_face(),
            contents: ", ".into(),
        },
        Atom {
            face: string_face(),
            contents: format!("\"value_{i}\"").into(),
        },
        Atom {
            face: operator_face(),
            contents: ");".into(),
        },
    ]
}

fn string_heavy_line(i: usize) -> Line {
    vec![
        Atom {
            face: keyword_face(),
            contents: "    const ".into(),
        },
        Atom {
            face: constant_face(),
            contents: format!("MSG_{i}").into(),
        },
        Atom {
            face: operator_face(),
            contents: ": &str = ".into(),
        },
        Atom {
            face: string_face(),
            contents: format!("\"Hello from module {i}, processing data\"").into(),
        },
        Atom {
            face: operator_face(),
            contents: ";".into(),
        },
    ]
}

fn indented_block_line(i: usize) -> Line {
    vec![
        Atom {
            face: Face::default(),
            contents: "    ".into(),
        },
        Atom {
            face: keyword_face(),
            contents: "if ".into(),
        },
        Atom {
            face: ident_face(),
            contents: format!("count_{i}").into(),
        },
        Atom {
            face: operator_face(),
            contents: " > ".into(),
        },
        Atom {
            face: literal_face(),
            contents: format!("{}", i * 10).into(),
        },
        Atom {
            face: operator_face(),
            contents: " {".into(),
        },
    ]
}

fn cjk_comment_line(i: usize) -> Line {
    vec![Atom {
        face: comment_face(),
        contents: format!("// 処理{i}: データ変換と検証").into(),
    }]
}

fn attribute_heavy_line(i: usize) -> Line {
    vec![
        Atom {
            face: Face {
                attributes: Attributes::BOLD,
                ..error_face()
            },
            contents: "ERROR".into(),
        },
        Atom {
            face: operator_face(),
            contents: ": ".into(),
        },
        Atom {
            face: Face {
                attributes: Attributes::ITALIC | Attributes::UNDERLINE,
                ..string_face()
            },
            contents: format!("\"unexpected token at line {i}\"").into(),
        },
    ]
}

fn make_realistic_line(i: usize) -> Line {
    match i % 8 {
        0 => vec![], // empty line
        1 => short_comment_line(i),
        2 => function_def_line(i),
        3 => long_code_line(i),
        4 => string_heavy_line(i),
        5 => indented_block_line(i),
        6 => cjk_comment_line(i),
        7 => attribute_heavy_line(i),
        _ => unreachable!(),
    }
}

/// Realistic state with varied line lengths, diverse faces, and wide chars.
pub fn realistic_state(line_count: usize) -> AppState {
    let mut state = AppState::default();
    state.cols = 80;
    state.rows = 24;
    state.default_face = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    };
    state.padding_face = state.default_face;
    state.status_default_face = Face {
        fg: Color::Named(NamedColor::Cyan),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    };
    state.lines = (0..line_count).map(make_realistic_line).collect();
    state.status_line = vec![Atom {
        face: Face::default(),
        contents: " NORMAL ".into(),
    }];
    state.status_mode_line = vec![Atom {
        face: Face::default(),
        contents: "normal".into(),
    }];
    state
}

/// JSON-RPC "draw" message as raw bytes using realistic line data.
#[allow(dead_code)]
pub fn draw_realistic_json(line_count: usize) -> Vec<u8> {
    let lines: Vec<Line> = (0..line_count).map(make_realistic_line).collect();
    let default_face = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    };
    let padding_face = default_face;
    to_json_bytes("draw", (&lines, &default_face, &padding_face))
}

/// Create a state with `n` lines modified starting at `start_line` (simulating an edit).
pub fn state_with_edit(base: &AppState, start_line: usize, n: usize) -> AppState {
    let mut state = base.clone();
    for i in start_line..(start_line + n).min(state.lines.len()) {
        state.lines[i] = vec![
            Atom {
                face: Face {
                    fg: Color::Rgb { r: 255, g: 0, b: 0 },
                    bg: Color::Default,
                    ..Face::default()
                },
                contents: format!("edited_line_{i}").into(),
            },
            Atom {
                face: Face::default(),
                contents: " // modified".into(),
            },
        ];
    }
    state
}

/// Create an `AppState` with a menu visible (inline style at anchor).
pub fn state_with_menu(item_count: usize) -> AppState {
    let mut state = typical_state(23);

    let items: Vec<Line> = (0..item_count)
        .map(|i| {
            vec![Atom {
                face: Face::default(),
                contents: format!("completion_{i}").into(),
            }]
        })
        .collect();
    let menu_face = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Blue),
        ..Face::default()
    };
    let selected_face = Face {
        fg: Color::Named(NamedColor::Black),
        bg: Color::Named(NamedColor::Cyan),
        ..Face::default()
    };
    let screen_h = state.available_height();
    state.menu = Some(kasane_core::state::MenuState::new(
        items,
        kasane_core::state::MenuParams {
            anchor: Coord {
                line: 5,
                column: 10,
            },
            selected_item_face: selected_face,
            menu_face,
            style: MenuStyle::Inline,
            screen_w: state.cols,
            screen_h,
            max_height: state.menu_max_height,
        },
    ));
    state
}

/// Build a `KakouneRequest::Draw` message with the given number of lines.
pub fn draw_request(line_count: usize) -> KakouneRequest {
    let lines: Vec<Line> = (0..line_count).map(make_colored_line).collect();
    KakouneRequest::Draw {
        lines,
        default_face: Face {
            fg: Color::Named(NamedColor::White),
            bg: Color::Named(NamedColor::Black),
            ..Face::default()
        },
        padding_face: Face {
            fg: Color::Named(NamedColor::White),
            bg: Color::Named(NamedColor::Black),
            ..Face::default()
        },
    }
}

/// Create a `PluginRegistry` with N dummy plugins.
/// Each plugin contributes to `StatusRight` and acts as a no-op decorator on `Buffer`.
pub fn registry_with_plugins(n: usize) -> PluginRegistry {
    let mut registry = PluginRegistry::new();
    for i in 0..n {
        registry.register(Box::new(BenchPlugin {
            id: format!("bench_plugin_{i}"),
        }));
    }
    registry
}

// ---------------------------------------------------------------------------
// JSON fixture builders (for parse benchmarks)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct JsonRpcMsg<P: Serialize> {
    jsonrpc: &'static str,
    method: &'static str,
    params: P,
}

fn to_json_bytes<P: Serialize>(method: &'static str, params: P) -> Vec<u8> {
    serde_json::to_vec(&JsonRpcMsg {
        jsonrpc: "2.0",
        method,
        params,
    })
    .expect("fixture serialization should not fail")
}

/// JSON-RPC "draw" message as raw bytes (for simd_json parse benchmarks).
pub fn draw_json(line_count: usize) -> Vec<u8> {
    let lines: Vec<Line> = (0..line_count).map(make_colored_line).collect();
    let default_face = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    };
    let padding_face = default_face;
    to_json_bytes("draw", (&lines, &default_face, &padding_face))
}

/// JSON-RPC "draw_status" message as raw bytes.
pub fn draw_status_json() -> Vec<u8> {
    let status_line: Line = vec![Atom {
        face: Face::default(),
        contents: " NORMAL ".into(),
    }];
    let mode_line: Line = vec![Atom {
        face: Face::default(),
        contents: "normal".into(),
    }];
    let default_face = Face {
        fg: Color::Named(NamedColor::Cyan),
        bg: Color::Named(NamedColor::Black),
        ..Face::default()
    };
    to_json_bytes("draw_status", (&status_line, &mode_line, &default_face))
}

/// JSON-RPC "set_cursor" message as raw bytes.
pub fn set_cursor_json() -> Vec<u8> {
    to_json_bytes(
        "set_cursor",
        (
            "buffer",
            Coord {
                line: 5,
                column: 10,
            },
        ),
    )
}

/// JSON-RPC "menu_show" message as raw bytes with the given item count.
pub fn menu_show_json(item_count: usize) -> Vec<u8> {
    let items: Vec<Line> = (0..item_count)
        .map(|i| {
            vec![Atom {
                face: Face::default(),
                contents: format!("completion_{i}").into(),
            }]
        })
        .collect();
    let anchor = Coord {
        line: 5,
        column: 10,
    };
    let selected_face = Face {
        fg: Color::Named(NamedColor::Black),
        bg: Color::Named(NamedColor::Cyan),
        ..Face::default()
    };
    let menu_face = Face {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Blue),
        ..Face::default()
    };
    to_json_bytes(
        "menu_show",
        (&items, &anchor, &selected_face, &menu_face, "inline"),
    )
}
