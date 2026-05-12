#![allow(dead_code, unused_imports)]

use std::any::Any;

use kasane_core::element::{Element, InteractiveId};
use kasane_core::input::{KeyEvent, MouseEvent};
use kasane_core::plugin::{
    AppView, Command, ContribSizeHint, ContributeContext, Contribution, PluginCapabilities,
    PluginId, PluginRuntime, SlotId,
};
use kasane_core::protocol::{
    Atom, Attributes, Color, Coord, KakouneRequest, Line, MenuStyle, NamedColor, Style, WireFace,
};
use kasane_core::state::{AppState, DirtyFlags};
use serde::Serialize;

// ---------------------------------------------------------------------------
// Dummy plugin for benchmarks
// ---------------------------------------------------------------------------

struct BenchPlugin {
    id: String,
}

impl kasane_core::plugin::Plugin for BenchPlugin {
    type State = ();

    fn id(&self) -> PluginId {
        PluginId(self.id.clone())
    }

    fn register(&self, r: &mut kasane_core::plugin::HandlerRegistry<()>) {
        let id = self.id.clone();
        r.on_contribute(SlotId::STATUS_RIGHT, move |_state, _app, _ctx| {
            Some(Contribution {
                element: Element::text(format!("[{}]", id), Style::default()),
                priority: 0,
                size_hint: ContribSizeHint::Auto,
            })
        });
    }
}

// ---------------------------------------------------------------------------
// Fixture builders
// ---------------------------------------------------------------------------

/// Build a tree-sitter-style colored line with keyword + identifier + literal atoms.
fn make_colored_line(i: usize) -> Line {
    let keyword_face = WireFace {
        fg: Color::Rgb {
            r: 255,
            g: 100,
            b: 0,
        },
        bg: Color::Default,
        ..WireFace::default()
    };
    let ident_face = WireFace {
        fg: Color::Rgb {
            r: 0,
            g: 200,
            b: 100,
        },
        bg: Color::Default,
        ..WireFace::default()
    };
    let literal_face = WireFace {
        fg: Color::Rgb {
            r: 100,
            g: 100,
            b: 255,
        },
        bg: Color::Default,
        ..WireFace::default()
    };
    let plain_face = WireFace::default();

    vec![
        Atom::with_style(
            "let",
            kasane_core::protocol::Style::from_face(&keyword_face),
        ),
        Atom::with_style(" ", kasane_core::protocol::Style::from_face(&plain_face)),
        Atom::with_style(
            format!("var_{i}"),
            kasane_core::protocol::Style::from_face(&ident_face),
        ),
        Atom::with_style(" = ", kasane_core::protocol::Style::from_face(&plain_face)),
        Atom::with_style(
            format!("\"{i}_value\""),
            kasane_core::protocol::Style::from_face(&literal_face),
        ),
        Atom::with_style(";", kasane_core::protocol::Style::from_face(&plain_face)),
    ]
}

/// Create an `AppState` representing a typical 80x24 editor with colored buffer lines.
pub fn typical_state(line_count: usize) -> AppState {
    let mut state = AppState::default();
    state.runtime.cols = 80;
    state.runtime.rows = 24;
    state.observed.default_style = WireFace {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..WireFace::default()
    }
    .into();
    state.observed.padding_style = state.observed.default_style.clone();
    state.observed.status_default_style = WireFace {
        fg: Color::Named(NamedColor::Cyan),
        bg: Color::Named(NamedColor::Black),
        ..WireFace::default()
    }
    .into();
    state.observed.lines = std::sync::Arc::new((0..line_count).map(make_colored_line).collect());
    state.inference.status_line = vec![Atom {
        style: kasane_core::protocol::default_unresolved_style(),
        contents: " NORMAL ".into(),
    }];
    state.observed.status_mode_line = vec![Atom {
        style: kasane_core::protocol::default_unresolved_style(),
        contents: "normal".into(),
    }];
    state
}

// ---------------------------------------------------------------------------
// Realistic fixture builders (diverse faces, varied line lengths, wide chars)
// ---------------------------------------------------------------------------

fn keyword_face() -> WireFace {
    WireFace {
        fg: Color::Rgb {
            r: 255,
            g: 100,
            b: 0,
        },
        bg: Color::Default,
        ..WireFace::default()
    }
}

fn ident_face() -> WireFace {
    WireFace {
        fg: Color::Rgb {
            r: 0,
            g: 200,
            b: 100,
        },
        bg: Color::Default,
        ..WireFace::default()
    }
}

fn literal_face() -> WireFace {
    WireFace {
        fg: Color::Rgb {
            r: 100,
            g: 100,
            b: 255,
        },
        bg: Color::Default,
        ..WireFace::default()
    }
}

fn comment_face() -> WireFace {
    WireFace {
        fg: Color::Rgb {
            r: 128,
            g: 128,
            b: 128,
        },
        bg: Color::Default,
        attributes: Attributes::ITALIC,
        ..WireFace::default()
    }
}

fn type_face() -> WireFace {
    WireFace {
        fg: Color::Named(NamedColor::Cyan),
        bg: Color::Default,
        ..WireFace::default()
    }
}

fn operator_face() -> WireFace {
    WireFace {
        fg: Color::Named(NamedColor::White),
        bg: Color::Default,
        ..WireFace::default()
    }
}

fn string_face() -> WireFace {
    WireFace {
        fg: Color::Named(NamedColor::Yellow),
        bg: Color::Default,
        ..WireFace::default()
    }
}

fn error_face() -> WireFace {
    WireFace {
        fg: Color::Named(NamedColor::BrightRed),
        bg: Color::Default,
        attributes: Attributes::BOLD | Attributes::UNDERLINE,
        ..WireFace::default()
    }
}

fn namespace_face() -> WireFace {
    WireFace {
        fg: Color::Named(NamedColor::Magenta),
        bg: Color::Default,
        ..WireFace::default()
    }
}

fn constant_face() -> WireFace {
    WireFace {
        fg: Color::Named(NamedColor::BrightBlue),
        bg: Color::Default,
        ..WireFace::default()
    }
}

fn short_comment_line(i: usize) -> Line {
    vec![Atom::with_style(
        format!("// comment line {i}"),
        kasane_core::protocol::Style::from_face(&comment_face()),
    )]
}

fn function_def_line(i: usize) -> Line {
    vec![
        Atom::with_style(
            "fn ",
            kasane_core::protocol::Style::from_face(&keyword_face()),
        ),
        Atom::with_style(
            format!("process_{i}"),
            kasane_core::protocol::Style::from_face(&ident_face()),
        ),
        Atom::with_style(
            "(",
            kasane_core::protocol::Style::from_face(&operator_face()),
        ),
        Atom::with_style("u32", kasane_core::protocol::Style::from_face(&type_face())),
        Atom::with_style(
            ") {",
            kasane_core::protocol::Style::from_face(&operator_face()),
        ),
    ]
}

fn long_code_line(i: usize) -> Line {
    vec![
        Atom::with_style(
            "    let ",
            kasane_core::protocol::Style::from_face(&keyword_face()),
        ),
        Atom::with_style(
            format!("result_{i}"),
            kasane_core::protocol::Style::from_face(&ident_face()),
        ),
        Atom::with_style(
            " = ",
            kasane_core::protocol::Style::from_face(&operator_face()),
        ),
        Atom::with_style(
            "self",
            kasane_core::protocol::Style::from_face(&namespace_face()),
        ),
        Atom::with_style(
            ".",
            kasane_core::protocol::Style::from_face(&operator_face()),
        ),
        Atom::with_style(
            format!("compute_{i}"),
            kasane_core::protocol::Style::from_face(&ident_face()),
        ),
        Atom::with_style(
            "(",
            kasane_core::protocol::Style::from_face(&operator_face()),
        ),
        Atom::with_style(
            format!("{}", i * 42),
            kasane_core::protocol::Style::from_face(&literal_face()),
        ),
        Atom::with_style(
            ", ",
            kasane_core::protocol::Style::from_face(&operator_face()),
        ),
        Atom::with_style(
            format!("\"value_{i}\""),
            kasane_core::protocol::Style::from_face(&string_face()),
        ),
        Atom::with_style(
            ");",
            kasane_core::protocol::Style::from_face(&operator_face()),
        ),
    ]
}

fn string_heavy_line(i: usize) -> Line {
    vec![
        Atom::with_style(
            "    const ",
            kasane_core::protocol::Style::from_face(&keyword_face()),
        ),
        Atom::with_style(
            format!("MSG_{i}"),
            kasane_core::protocol::Style::from_face(&constant_face()),
        ),
        Atom::with_style(
            ": &str = ",
            kasane_core::protocol::Style::from_face(&operator_face()),
        ),
        Atom::with_style(
            format!("\"Hello from module {i}, processing data\""),
            kasane_core::protocol::Style::from_face(&string_face()),
        ),
        Atom::with_style(
            ";",
            kasane_core::protocol::Style::from_face(&operator_face()),
        ),
    ]
}

fn indented_block_line(i: usize) -> Line {
    vec![
        Atom {
            style: kasane_core::protocol::default_unresolved_style(),
            contents: "    ".into(),
        },
        Atom::with_style(
            "if ",
            kasane_core::protocol::Style::from_face(&keyword_face()),
        ),
        Atom::with_style(
            format!("count_{i}"),
            kasane_core::protocol::Style::from_face(&ident_face()),
        ),
        Atom::with_style(
            " > ",
            kasane_core::protocol::Style::from_face(&operator_face()),
        ),
        Atom::with_style(
            format!("{}", i * 10),
            kasane_core::protocol::Style::from_face(&literal_face()),
        ),
        Atom::with_style(
            " {",
            kasane_core::protocol::Style::from_face(&operator_face()),
        ),
    ]
}

fn cjk_comment_line(i: usize) -> Line {
    vec![Atom::with_style(
        format!("// 処理{i}: データ変換と検証"),
        kasane_core::protocol::Style::from_face(&comment_face()),
    )]
}

fn attribute_heavy_line(i: usize) -> Line {
    vec![
        Atom::with_style(
            "ERROR",
            kasane_core::protocol::Style::from_face(&WireFace {
                attributes: Attributes::BOLD,
                ..error_face()
            }),
        ),
        Atom::with_style(
            ": ",
            kasane_core::protocol::Style::from_face(&operator_face()),
        ),
        Atom::with_style(
            format!("\"unexpected token at line {i}\""),
            kasane_core::protocol::Style::from_face(&WireFace {
                attributes: Attributes::ITALIC | Attributes::UNDERLINE,
                ..string_face()
            }),
        ),
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
    state.runtime.cols = 80;
    state.runtime.rows = 24;
    state.observed.default_style = WireFace {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..WireFace::default()
    }
    .into();
    state.observed.padding_style = state.observed.default_style.clone();
    state.observed.status_default_style = WireFace {
        fg: Color::Named(NamedColor::Cyan),
        bg: Color::Named(NamedColor::Black),
        ..WireFace::default()
    }
    .into();
    state.observed.lines = std::sync::Arc::new((0..line_count).map(make_realistic_line).collect());
    state.inference.status_line = vec![Atom {
        style: kasane_core::protocol::default_unresolved_style(),
        contents: " NORMAL ".into(),
    }];
    state.observed.status_mode_line = vec![Atom {
        style: kasane_core::protocol::default_unresolved_style(),
        contents: "normal".into(),
    }];
    state
}

/// JSON-RPC "draw" message as raw bytes using realistic line data.
#[allow(dead_code)]
pub fn draw_realistic_json(line_count: usize) -> Vec<u8> {
    let lines: Vec<Line> = (0..line_count).map(make_realistic_line).collect();
    let wire_lines = lines_to_wire(&lines);
    let default_face = WireFace {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..WireFace::default()
    };
    let padding_face = default_face;
    to_json_bytes(
        "draw",
        (
            &wire_lines,
            &Coord::default(),
            &default_face,
            &padding_face,
            0u16,
        ),
    )
}

/// Create a state with `n` lines modified starting at `start_line` (simulating an edit).
pub fn state_with_edit(base: &AppState, start_line: usize, n: usize) -> AppState {
    let mut state = base.clone();
    for i in start_line..(start_line + n).min(state.observed.lines.len()) {
        std::sync::Arc::make_mut(&mut state.observed.lines)[i] = vec![
            Atom::with_style(
                format!("edited_line_{i}"),
                kasane_core::protocol::Style::from_face(&WireFace {
                    fg: Color::Rgb { r: 255, g: 0, b: 0 },
                    bg: Color::Default,
                    ..WireFace::default()
                }),
            ),
            Atom::plain(" // modified"),
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
                style: kasane_core::protocol::default_unresolved_style(),
                contents: format!("completion_{i}").into(),
            }]
        })
        .collect();
    let menu_face = WireFace {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Blue),
        ..WireFace::default()
    };
    let selected_face = WireFace {
        fg: Color::Named(NamedColor::Black),
        bg: Color::Named(NamedColor::Cyan),
        ..WireFace::default()
    };
    let screen_h = state.available_height();
    state.observed.menu = Some(kasane_core::state::MenuState::new(
        items,
        kasane_core::state::MenuParams {
            anchor: Coord {
                line: 5,
                column: 10,
            },
            selected_item_face: selected_face.into(),
            menu_face: menu_face.into(),
            style: MenuStyle::Inline,
            screen_w: state.runtime.cols,
            screen_h,
            max_height: state.config.menu_max_height,
        },
    ));
    state
}

/// Build a `KakouneRequest::Draw` message with the given number of lines.
pub fn draw_request(line_count: usize) -> KakouneRequest {
    let lines: Vec<Line> = (0..line_count).map(make_colored_line).collect();
    KakouneRequest::Draw {
        lines,
        cursor_pos: Coord::default(),
        default_style: std::sync::Arc::new(kasane_core::protocol::UnresolvedStyle::from_face(
            &WireFace {
                fg: Color::Named(NamedColor::White),
                bg: Color::Named(NamedColor::Black),
                ..WireFace::default()
            },
        )),
        padding_style: std::sync::Arc::new(kasane_core::protocol::UnresolvedStyle::from_face(
            &WireFace {
                fg: Color::Named(NamedColor::White),
                bg: Color::Named(NamedColor::Black),
                ..WireFace::default()
            },
        )),
        widget_columns: 0,
    }
}

/// Create a `PluginRuntime` with N dummy plugins.
/// Each plugin contributes to `StatusRight` and acts as a no-op decorator on `Buffer`.
pub fn registry_with_plugins(n: usize) -> PluginRuntime {
    let mut registry = PluginRuntime::new();
    for i in 0..n {
        registry.register(BenchPlugin {
            id: format!("bench_plugin_{i}"),
        });
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

/// Wire-shaped atom for benchmark JSON fixtures. Mirrors `WireAtom` in
/// `kasane_core::protocol::parse` (which is private). `Atom` itself
/// holds an `Arc<UnresolvedStyle>` and is opaque to the wire format,
/// so we project to the legacy `{ face, contents }` shape here.
#[derive(Serialize)]
struct WireAtomBench<'a> {
    face: WireFace,
    contents: &'a str,
}

fn atoms_to_wire(line: &[Atom]) -> Vec<WireAtomBench<'_>> {
    line.iter()
        .map(|a| WireAtomBench {
            face: a.unresolved_style().to_face(),
            contents: a.contents.as_str(),
        })
        .collect()
}

fn lines_to_wire(lines: &[Vec<Atom>]) -> Vec<Vec<WireAtomBench<'_>>> {
    lines.iter().map(|l| atoms_to_wire(l)).collect()
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
    let wire_lines = lines_to_wire(&lines);
    let default_face = WireFace {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Black),
        ..WireFace::default()
    };
    let padding_face = default_face;
    to_json_bytes(
        "draw",
        (
            &wire_lines,
            &Coord::default(),
            &default_face,
            &padding_face,
            0u16,
        ),
    )
}

/// JSON-RPC "draw_status" message as raw bytes.
pub fn draw_status_json() -> Vec<u8> {
    let prompt: Line = vec![Atom::plain(" NORMAL ")];
    let content: Line = Vec::new();
    let mode_line: Line = vec![Atom::plain("normal")];
    let wire_prompt = atoms_to_wire(&prompt);
    let wire_content = atoms_to_wire(&content);
    let wire_mode_line = atoms_to_wire(&mode_line);
    let default_face = WireFace {
        fg: Color::Named(NamedColor::Cyan),
        bg: Color::Named(NamedColor::Black),
        ..WireFace::default()
    };
    to_json_bytes(
        "draw_status",
        (
            &wire_prompt,
            &wire_content,
            -1i32,
            &wire_mode_line,
            &default_face,
            "status",
        ),
    )
}

/// JSON-RPC "menu_show" message as raw bytes with the given item count.
pub fn menu_show_json(item_count: usize) -> Vec<u8> {
    let items: Vec<Line> = (0..item_count)
        .map(|i| vec![Atom::plain(format!("completion_{i}"))])
        .collect();
    let wire_items = lines_to_wire(&items);
    let anchor = Coord {
        line: 5,
        column: 10,
    };
    let selected_face = WireFace {
        fg: Color::Named(NamedColor::Black),
        bg: Color::Named(NamedColor::Cyan),
        ..WireFace::default()
    };
    let menu_face = WireFace {
        fg: Color::Named(NamedColor::White),
        bg: Color::Named(NamedColor::Blue),
        ..WireFace::default()
    };
    to_json_bytes(
        "menu_show",
        (&wire_items, &anchor, &selected_face, &menu_face, "inline"),
    )
}
