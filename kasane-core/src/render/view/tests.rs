use super::*;
use std::collections::HashSet;

use crate::element::{Direction, OverlayAnchor};
use crate::plugin::{AppView, LineAnnotation, PluginBackend, PluginId, PluginRuntime, SlotId};
use crate::protocol::{Atom, Color, Coord, Face, InfoStyle, MenuStyle, NamedColor};
use crate::state::AppState;
use crate::surface::{
    SurfaceId, SurfaceRegistry, buffer::KakouneBufferSurface, status::StatusBarSurface,
};
use crate::test_utils::make_line;

fn assert_slot_placeholder(element: &Element, slot_name: &str, direction: Direction) {
    match element {
        Element::SlotPlaceholder {
            slot_name: actual,
            direction: actual_direction,
            ..
        } => {
            assert_eq!(actual.as_str(), slot_name);
            assert_eq!(*actual_direction, direction);
        }
        other => panic!(
            "expected slot placeholder {slot_name}, got {:?}",
            std::mem::discriminant(other)
        ),
    }
}

#[test]
fn test_view_empty_state() {
    let state = AppState::default();
    let registry = PluginRuntime::new();
    let el = view(&state, &registry.view());

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
    state.runtime.cols = 80;
    state.runtime.rows = 24;
    state.observed.lines = vec![make_line("hello")];
    state.apply(crate::protocol::KakouneRequest::MenuShow {
        items: vec![make_line("item1"), make_line("item2")],
        anchor: Coord { line: 1, column: 0 },
        selected_item_style: crate::protocol::default_unresolved_style(),
        menu_style: crate::protocol::default_unresolved_style(),
        style: MenuStyle::Inline,
    });

    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(super::menu::BuiltinMenuPlugin));
    let el = view(&state, &registry.view());

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
    state.runtime.cols = 80;
    state.runtime.rows = 24;
    state.apply(crate::protocol::KakouneRequest::InfoShow {
        title: make_line("Help"),
        content: vec![make_line("some info")],
        anchor: Coord { line: 0, column: 0 },
        info_style: crate::protocol::default_unresolved_style(),
        style: InfoStyle::Modal,
    });

    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(super::info::BuiltinInfoPlugin));
    let el = view(&state, &registry.view());

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
    state.observed.status_default_style = Face {
        fg: Color::Named(NamedColor::Cyan),
        bg: Color::Named(NamedColor::Magenta),
        ..Face::default()
    }
    .into();
    // Atoms with Color::Default — should be resolved to status_default_face colors
    state.inference.status_line = vec![Atom::from_face(Face::default(), "file.rs")];
    state.observed.status_mode_line = vec![Atom::from_face(Face::default(), "normal")];

    let status_bar = build_status_core(&state);

    // Extract StyledLine atoms from the status row children
    let children = match &status_bar {
        Element::Flex { children, .. } => children,
        other => panic!("expected Flex row, got {:?}", std::mem::discriminant(other)),
    };

    // Check status_line atoms
    match &children[0].element {
        Element::StyledLine(atoms) => {
            for atom in atoms {
                assert_eq!(
                    atom.face().fg,
                    Color::Named(NamedColor::Cyan),
                    "status_line fg should be resolved from status_default_face"
                );
                assert_eq!(
                    atom.face().bg,
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
                    atom.face().fg,
                    Color::Named(NamedColor::Cyan),
                    "mode_line fg should be resolved from status_default_face"
                );
                assert_eq!(
                    atom.face().bg,
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
    use crate::plugin::{
        ContribSizeHint, ContributeContext, Contribution, PluginBackend, PluginCapabilities,
        PluginId,
    };

    struct StatusLeftPlugin;
    impl PluginBackend for StatusLeftPlugin {
        fn id(&self) -> PluginId {
            PluginId("status_left".into())
        }
        fn capabilities(&self) -> PluginCapabilities {
            PluginCapabilities::CONTRIBUTOR
        }
        fn contribute_to(
            &self,
            region: &SlotId,
            _state: &AppView<'_>,
            _ctx: &ContributeContext,
        ) -> Option<Contribution> {
            if *region == SlotId::STATUS_LEFT {
                Some(Contribution {
                    element: Element::text("[L]", Face::default()),
                    priority: 0,
                    size_hint: ContribSizeHint::Auto,
                })
            } else {
                None
            }
        }
    }

    let mut state = AppState::default();
    state.inference.status_line = make_line("status");
    state.observed.status_mode_line = make_line("normal");

    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(StatusLeftPlugin));

    let el = view(&state, &registry.view());

    // After Surface-based rendering, the status bar is produced by
    // build_status_surface_abstract → slot resolution. With a STATUS_LEFT
    // plugin, the resolved slot should contain the plugin's [L] element.
    // Structure: Column([ResolvedSlot(above), Container(Row([ResolvedSlot(left, [[L]]), core, ResolvedSlot(right)]))])
    match &el {
        Element::Flex { children, .. } => {
            let status = &children.last().unwrap().element;
            let container = find_status_container(status)
                .expect("should find Container with status row in resolved status bar");
            match container {
                Element::Flex {
                    direction: Direction::Row,
                    children,
                    ..
                } => {
                    // ResolvedSlot(left) + status_core(flex) + ResolvedSlot(right)
                    // The left slot should be resolved with the plugin contribution
                    assert!(
                        children.len() >= 3,
                        "should have resolved_left + status_line + resolved_right (got {})",
                        children.len()
                    );
                    // Check that the left slot contains our plugin's element
                    let has_left_contrib = find_text_content(&children[0].element, "[L]");
                    assert!(
                        has_left_contrib,
                        "STATUS_LEFT slot should contain [L] from plugin"
                    );
                }
                _ => panic!("expected Row inside status container"),
            }
        }
        _ => panic!("expected Column"),
    }
}

/// Recursively check if an element tree contains a Text with the given content.
fn find_text_content(el: &Element, needle: &str) -> bool {
    match el {
        Element::Text(text, _) => text == needle,
        Element::Flex { children, .. } => children
            .iter()
            .any(|c| find_text_content(&c.element, needle)),
        Element::ResolvedSlot { children, .. } => children
            .iter()
            .any(|c| find_text_content(&c.element, needle)),
        Element::Container { child, .. } => find_text_content(child, needle),
        Element::Interactive { child, .. } => find_text_content(child, needle),
        Element::Stack { base, overlays, .. } => {
            find_text_content(base, needle)
                || overlays
                    .iter()
                    .any(|o| find_text_content(&o.element, needle))
        }
        _ => false,
    }
}

#[test]
fn test_info_framed_shadow_disabled() {
    let mut state = AppState::default();
    state.runtime.cols = 80;
    state.runtime.rows = 24;
    state.config.shadow_enabled = false;
    state.apply(crate::protocol::KakouneRequest::InfoShow {
        title: make_line("Help"),
        content: vec![make_line("content")],
        anchor: Coord { line: 0, column: 0 },
        info_style: crate::protocol::default_unresolved_style(),
        style: InfoStyle::Modal,
    });

    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(super::info::BuiltinInfoPlugin));
    let el = view(&state, &registry.view());

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
            Element::Flex { children, .. } => children.iter().find_map(|c| find_shadow(&c.element)),
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
    state.inference.status_line = make_line("status");
    state.observed.status_mode_line = make_line("normal");

    let registry = PluginRuntime::new();
    let el = view(&state, &registry.view());

    // After Surface-based rendering, the status bar is produced by
    // build_status_surface_abstract → slot resolution. The resolved structure
    // is: Column([ResolvedSlot(above), Container(Row([ResolvedSlot(left), core, ResolvedSlot(right)]))])
    // which is then composed into the root Column via compose_base_result.
    match el {
        Element::Flex { children, .. } => {
            // Last child is the status bar surface output (a Column with above + row)
            let status = &children.last().unwrap().element;
            // Find the Container with the status row inside the resolved structure
            let container = find_status_container(status)
                .expect("should find Container with status row in resolved status bar");
            match container {
                Element::Flex {
                    direction: Direction::Row,
                    children,
                    ..
                } => {
                    // ResolvedSlot(left) + status_core(flex) + ResolvedSlot(right)
                    assert!(
                        children.len() >= 2,
                        "status row should have at least status_line + mode_line (got {})",
                        children.len()
                    );
                }
                _ => panic!("expected Row inside status container"),
            }
        }
        _ => panic!("expected Column"),
    }
}

/// Recursively find the Container wrapping the status row.
fn find_status_container(el: &Element) -> Option<&Element> {
    match el {
        Element::Container { child, .. } => Some(child.as_ref()),
        Element::Flex { children, .. } => children
            .iter()
            .find_map(|c| find_status_container(&c.element)),
        Element::ResolvedSlot { children, .. } => children
            .iter()
            .find_map(|c| find_status_container(&c.element)),
        _ => None,
    }
}

#[test]
fn test_status_surface_abstract_shape() {
    let mut state = AppState::default();
    state.inference.status_line = make_line("status");
    state.observed.status_mode_line = make_line("normal");

    let registry = PluginRuntime::new();
    let element = build_status_surface_abstract(&state, &registry.view());

    match element {
        Element::Flex {
            direction: Direction::Column,
            children,
            ..
        } => {
            assert_eq!(children.len(), 2);
            assert_slot_placeholder(
                &children[0].element,
                "kasane.status.above",
                Direction::Column,
            );

            match &children[1].element {
                Element::Container { child, .. } => match child.as_ref() {
                    Element::Flex {
                        direction: Direction::Row,
                        children,
                        ..
                    } => {
                        assert_eq!(children.len(), 3);
                        assert_slot_placeholder(
                            &children[0].element,
                            "kasane.status.left",
                            Direction::Row,
                        );
                        assert_slot_placeholder(
                            &children[2].element,
                            "kasane.status.right",
                            Direction::Row,
                        );
                        assert_eq!(children[1].flex, 1.0);
                    }
                    other => panic!(
                        "expected status row flex, got {:?}",
                        std::mem::discriminant(other)
                    ),
                },
                other => panic!(
                    "expected status container, got {:?}",
                    std::mem::discriminant(other)
                ),
            }
        }
        other => panic!(
            "expected status abstract column, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

#[test]
fn test_buffer_surface_abstract_shape() {
    let mut state = AppState::default();
    state.observed.lines = vec![make_line("buffer")];

    let registry = PluginRuntime::new();
    let element = build_buffer_surface_abstract(&state, &registry.view());

    match element {
        Element::Stack { base, overlays } => {
            assert_eq!(overlays.len(), 1);
            assert!(matches!(overlays[0].anchor, OverlayAnchor::Fill));
            assert_slot_placeholder(
                &overlays[0].element,
                "kasane.buffer.overlay",
                Direction::Column,
            );

            match base.as_ref() {
                Element::Flex {
                    direction: Direction::Column,
                    children,
                    ..
                } => {
                    assert_eq!(children.len(), 3);
                    assert_slot_placeholder(
                        &children[0].element,
                        "kasane.buffer.above",
                        Direction::Column,
                    );
                    assert_slot_placeholder(
                        &children[2].element,
                        "kasane.buffer.below",
                        Direction::Column,
                    );

                    match &children[1].element {
                        Element::Flex {
                            direction: Direction::Row,
                            children,
                            ..
                        } => {
                            assert_eq!(children.len(), 3);
                            assert_slot_placeholder(
                                &children[0].element,
                                "kasane.buffer.left",
                                Direction::Row,
                            );
                            assert_slot_placeholder(
                                &children[2].element,
                                "kasane.buffer.right",
                                Direction::Row,
                            );
                            assert_eq!(children[1].flex, 1.0);
                        }
                        other => panic!(
                            "expected buffer row, got {:?}",
                            std::mem::discriminant(other)
                        ),
                    }
                }
                other => panic!(
                    "expected buffer stack base column, got {:?}",
                    std::mem::discriminant(other)
                ),
            }
        }
        other => panic!(
            "expected buffer abstract stack, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

#[test]
fn test_buffer_surface_abstract_keeps_gutters_outside_side_slots() {
    struct GutterPlugin;

    impl PluginBackend for GutterPlugin {
        fn id(&self) -> PluginId {
            PluginId("gutter_plugin".into())
        }

        fn annotate_line_with_ctx(
            &self,
            line: usize,
            _state: &AppView<'_>,
            _ctx: &crate::plugin::AnnotateContext,
        ) -> Option<LineAnnotation> {
            if line == 0 {
                Some(LineAnnotation {
                    left_gutter: Some(Element::text("L", Face::default())),
                    right_gutter: Some(Element::text("R", Face::default())),
                    background: None,
                    priority: 0,
                    inline: None,
                    virtual_text: vec![],
                })
            } else {
                None
            }
        }
    }

    let mut state = AppState::default();
    state.observed.lines = vec![make_line("buffer")];

    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(GutterPlugin));
    let element = build_buffer_surface_abstract(&state, &registry.view());

    match element {
        Element::Stack { base, .. } => match base.as_ref() {
            Element::Flex { children, .. } => match &children[1].element {
                Element::Flex { children, .. } => {
                    assert_eq!(children.len(), 5);
                    assert!(!matches!(
                        children[0].element,
                        Element::SlotPlaceholder { .. }
                    ));
                    assert_slot_placeholder(
                        &children[1].element,
                        "kasane.buffer.left",
                        Direction::Row,
                    );
                    assert_slot_placeholder(
                        &children[3].element,
                        "kasane.buffer.right",
                        Direction::Row,
                    );
                    assert!(!matches!(
                        children[4].element,
                        Element::SlotPlaceholder { .. }
                    ));
                }
                other => panic!(
                    "expected buffer row, got {:?}",
                    std::mem::discriminant(other)
                ),
            },
            other => panic!(
                "expected buffer stack base column, got {:?}",
                std::mem::discriminant(other)
            ),
        },
        other => panic!(
            "expected buffer abstract stack, got {:?}",
            std::mem::discriminant(&other)
        ),
    }
}

#[test]
fn test_surface_view_sections_preserves_surface_reports() {
    let mut state = AppState::default();
    state.observed.lines = vec![make_line("buffer")];
    state.inference.status_line = make_line("status");
    state.observed.status_mode_line = make_line("normal");

    let registry = PluginRuntime::new();
    let mut surface_registry = SurfaceRegistry::new();
    surface_registry.register(Box::new(KakouneBufferSurface::new()));
    surface_registry.register(Box::new(StatusBarSurface::new()));

    let root_area = crate::layout::Rect {
        x: 0,
        y: 0,
        w: state.runtime.cols,
        h: state.runtime.rows,
    };
    let sections =
        surface_registry.compose_view_sections(&state, None, &registry.view(), root_area);

    let keys: HashSet<&str> = sections
        .surface_reports
        .iter()
        .map(|report| report.surface_key.as_str())
        .collect();
    assert!(keys.contains("kasane.buffer"));
    assert!(keys.contains("kasane.status"));
    assert_eq!(
        surface_registry.surface_id_by_key("kasane.buffer"),
        Some(SurfaceId::BUFFER)
    );
}
