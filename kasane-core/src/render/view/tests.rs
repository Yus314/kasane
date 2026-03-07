use super::*;
use crate::element::Direction;
use crate::protocol::{Atom, Color, Coord, Face, InfoStyle, MenuStyle, NamedColor};
use crate::state::AppState;

fn make_line(s: &str) -> Line {
    vec![Atom {
        face: Face::default(),
        contents: s.to_string(),
    }]
}

#[test]
fn test_view_empty_state() {
    let state = AppState::default();
    let registry = PluginRegistry::new();
    let el = view(&state, &registry);

    // Should be a Column with BufferRef + status bar
    match el {
        Element::Flex {
            direction: Direction::Column,
            children,
            ..
        } => {
            assert_eq!(children.len(), 2); // buffer + status
        }
        _ => panic!("expected Column flex"),
    }
}

#[test]
fn test_view_with_menu() {
    let mut state = AppState::default();
    state.cols = 80;
    state.rows = 24;
    state.lines = vec![make_line("hello")];
    state.apply(crate::protocol::KakouneRequest::MenuShow {
        items: vec![make_line("item1"), make_line("item2")],
        anchor: Coord { line: 1, column: 0 },
        selected_item_face: Face::default(),
        menu_face: Face::default(),
        style: MenuStyle::Inline,
    });

    let registry = PluginRegistry::new();
    let el = view(&state, &registry);

    // Should be a Stack (base Column + menu overlay)
    match el {
        Element::Stack { overlays, .. } => {
            assert!(!overlays.is_empty(), "should have menu overlay");
        }
        _ => panic!("expected Stack, got {:?}", std::mem::discriminant(&el)),
    }
}

#[test]
fn test_view_with_info() {
    let mut state = AppState::default();
    state.cols = 80;
    state.rows = 24;
    state.apply(crate::protocol::KakouneRequest::InfoShow {
        title: make_line("Help"),
        content: vec![make_line("some info")],
        anchor: Coord { line: 0, column: 0 },
        face: Face::default(),
        style: InfoStyle::Modal,
    });

    let registry = PluginRegistry::new();
    let el = view(&state, &registry);

    match el {
        Element::Stack { overlays, .. } => {
            assert!(!overlays.is_empty(), "should have info overlay");
        }
        _ => panic!("expected Stack"),
    }
}

#[test]
fn test_status_bar_resolves_default_face() {
    let mut state = AppState::default();
    state.status_default_face = Face {
        fg: Color::Named(NamedColor::Cyan),
        bg: Color::Named(NamedColor::Magenta),
        ..Face::default()
    };
    // Atoms with Color::Default — should be resolved to status_default_face colors
    state.status_line = vec![Atom {
        face: Face::default(),
        contents: "file.rs".to_string(),
    }];
    state.status_mode_line = vec![Atom {
        face: Face::default(),
        contents: "normal".to_string(),
    }];

    let status_bar = build_status_bar(&state, vec![], vec![]);

    // Extract StyledLine atoms from the Container > Row > children
    let row = match &status_bar {
        Element::Container { child, .. } => child.as_ref(),
        other => panic!(
            "expected Container, got {:?}",
            std::mem::discriminant(other)
        ),
    };
    let children = match row {
        Element::Flex { children, .. } => children,
        other => panic!("expected Flex row, got {:?}", std::mem::discriminant(other)),
    };

    // Check status_line atoms
    match &children[0].element {
        Element::StyledLine(atoms) => {
            for atom in atoms {
                assert_eq!(
                    atom.face.fg,
                    Color::Named(NamedColor::Cyan),
                    "status_line fg should be resolved from status_default_face"
                );
                assert_eq!(
                    atom.face.bg,
                    Color::Named(NamedColor::Magenta),
                    "status_line bg should be resolved from status_default_face"
                );
            }
        }
        other => panic!(
            "expected StyledLine, got {:?}",
            std::mem::discriminant(other)
        ),
    }

    // Check mode_line atoms
    match &children[1].element {
        Element::StyledLine(atoms) => {
            for atom in atoms {
                assert_eq!(
                    atom.face.fg,
                    Color::Named(NamedColor::Cyan),
                    "mode_line fg should be resolved from status_default_face"
                );
                assert_eq!(
                    atom.face.bg,
                    Color::Named(NamedColor::Magenta),
                    "mode_line bg should be resolved from status_default_face"
                );
            }
        }
        other => panic!(
            "expected StyledLine, got {:?}",
            std::mem::discriminant(other)
        ),
    }
}

#[test]
fn test_status_left_slot_in_status_bar() {
    use crate::plugin::{Plugin, PluginId};

    struct StatusLeftPlugin;
    impl Plugin for StatusLeftPlugin {
        fn id(&self) -> PluginId {
            PluginId("status_left".into())
        }
        fn contribute(&self, slot: Slot, _state: &AppState) -> Option<Element> {
            match slot {
                Slot::StatusLeft => Some(Element::text("[L]", Face::default())),
                _ => None,
            }
        }
    }

    let mut state = AppState::default();
    state.status_line = make_line("status");
    state.status_mode_line = make_line("normal");

    let mut registry = PluginRegistry::new();
    registry.register(Box::new(StatusLeftPlugin));

    let el = view(&state, &registry);

    // The status bar is the last child of the root column.
    // It should now have 3 children: [L], status_line (flex), mode_line.
    match &el {
        Element::Flex { children, .. } => {
            let status = &children.last().unwrap().element;
            match status {
                Element::Container { child, .. } => match child.as_ref() {
                    Element::Flex { children, .. } => {
                        assert_eq!(
                            children.len(),
                            3,
                            "should have status_left + status_line + mode_line"
                        );
                    }
                    _ => panic!("expected Flex row"),
                },
                _ => panic!("expected Container"),
            }
        }
        _ => panic!("expected Column"),
    }
}

#[test]
fn test_info_framed_shadow_disabled() {
    let mut state = AppState::default();
    state.cols = 80;
    state.rows = 24;
    state.shadow_enabled = false;
    state.apply(crate::protocol::KakouneRequest::InfoShow {
        title: make_line("Help"),
        content: vec![make_line("content")],
        anchor: Coord { line: 0, column: 0 },
        face: Face::default(),
        style: InfoStyle::Modal,
    });

    let registry = PluginRegistry::new();
    let el = view(&state, &registry);

    // Find the info overlay's framed Container
    fn find_shadow(el: &Element) -> Option<bool> {
        match el {
            Element::Container {
                shadow,
                border: Some(_),
                ..
            } => Some(*shadow),
            Element::Stack { overlays, .. } => {
                overlays.iter().find_map(|o| find_shadow(&o.element))
            }
            Element::Container { child, .. } => find_shadow(child),
            Element::Interactive { child, .. } => find_shadow(child),
            Element::Flex { children, .. } => {
                children.iter().find_map(|c| find_shadow(&c.element))
            }
            _ => None,
        }
    }

    let shadow = find_shadow(&el);
    assert_eq!(
        shadow,
        Some(false),
        "shadow should be false when shadow_enabled is false"
    );
}

#[test]
fn test_view_status_bar_structure() {
    let mut state = AppState::default();
    state.status_line = make_line("status");
    state.status_mode_line = make_line("normal");

    let registry = PluginRegistry::new();
    let el = view(&state, &registry);

    match el {
        Element::Flex { children, .. } => {
            // Last child should be the status bar (Container with Row)
            let status = &children.last().unwrap().element;
            match status {
                Element::Container { child, .. } => match child.as_ref() {
                    Element::Flex {
                        direction: Direction::Row,
                        children,
                        ..
                    } => {
                        assert_eq!(children.len(), 2); // status_line + mode_line
                    }
                    _ => panic!("expected Row inside status container"),
                },
                _ => panic!("expected Container for status bar"),
            }
        }
        _ => panic!("expected Column"),
    }
}
