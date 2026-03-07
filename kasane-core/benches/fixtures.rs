use std::any::Any;

use kasane_core::element::{Element, InteractiveId};
use kasane_core::input::{KeyEvent, MouseEvent};
use kasane_core::plugin::{Command, DecorateTarget, Plugin, PluginId, PluginRegistry, Slot};
use kasane_core::protocol::{
    Atom, Color, Coord, Face, KakouneRequest, Line, MenuStyle, NamedColor,
};
use kasane_core::state::AppState;

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
            Slot::StatusRight => Some(Element::text(
                format!("[{}]", self.id),
                Default::default(),
            )),
            _ => None,
        }
    }

    fn decorate(
        &self,
        _target: DecorateTarget,
        element: Element,
        _state: &AppState,
    ) -> Element {
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
            contents: "let".to_string(),
        },
        Atom {
            face: plain_face,
            contents: " ".to_string(),
        },
        Atom {
            face: ident_face,
            contents: format!("var_{i}"),
        },
        Atom {
            face: plain_face,
            contents: " = ".to_string(),
        },
        Atom {
            face: literal_face,
            contents: format!("\"{i}_value\""),
        },
        Atom {
            face: plain_face,
            contents: ";".to_string(),
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
        contents: " NORMAL ".to_string(),
    }];
    state.status_mode_line = vec![Atom {
        face: Face::default(),
        contents: "normal".to_string(),
    }];
    state
}

/// Create an `AppState` with a menu visible (inline style at anchor).
pub fn state_with_menu(item_count: usize) -> AppState {
    let mut state = typical_state(23);

    let items: Vec<Line> = (0..item_count)
        .map(|i| {
            vec![Atom {
                face: Face::default(),
                contents: format!("completion_{i}"),
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
    let screen_h = state.rows.saturating_sub(1);
    state.menu = Some(kasane_core::state::MenuState::new(
        items,
        Coord { line: 5, column: 10 },
        selected_face,
        menu_face,
        MenuStyle::Inline,
        state.cols,
        screen_h,
        state.menu_max_height,
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
