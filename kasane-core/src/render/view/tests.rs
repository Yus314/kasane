use super::*;
use std::collections::HashSet;

use crate::element::{Direction, OverlayAnchor, ResolvedSlotInstanceId};
use crate::plugin::{
    ContributeContext, Contribution, LineAnnotation, Plugin, PluginCapabilities, PluginId,
    PluginRegistry, SlotId, TransformTarget,
};
use crate::protocol::{Atom, Color, Coord, Face, InfoStyle, MenuStyle, NamedColor};
use crate::state::{AppState, DirtyFlags};
use crate::surface::{
    EventContext, OwnerValidationError, OwnerValidationErrorKind, ResolvedSlotContentKind,
    ResolvedSlotRecord, SizeHint, SlotDeclaration, SlotKind, Surface, SurfaceEvent, SurfaceId,
    SurfaceRegistry, ViewContext, buffer::KakouneBufferSurface, status::StatusBarSurface,
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
        contents: "file.rs".into(),
    }];
    state.status_mode_line = vec![Atom {
        face: Face::default(),
        contents: "normal".into(),
    }];

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
    use crate::plugin::{
        ContribSizeHint, ContributeContext, Contribution, Plugin, PluginCapabilities, PluginId,
    };

    struct StatusLeftPlugin;
    impl Plugin for StatusLeftPlugin {
        fn id(&self) -> PluginId {
            PluginId("status_left".into())
        }
        fn capabilities(&self) -> PluginCapabilities {
            PluginCapabilities::CONTRIBUTOR
        }
        fn contribute_to(
            &self,
            region: &SlotId,
            _state: &AppState,
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
    state.status_line = make_line("status");
    state.status_mode_line = make_line("normal");

    let mut registry = PluginRegistry::new();
    registry.register(Box::new(StatusLeftPlugin));

    let el = view(&state, &registry);

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
    state.status_line = make_line("status");
    state.status_mode_line = make_line("normal");

    let registry = PluginRegistry::new();
    let el = view(&state, &registry);

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
    state.status_line = make_line("status");
    state.status_mode_line = make_line("normal");

    let registry = PluginRegistry::new();
    let element = build_status_surface_abstract(&state, &registry);

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
    state.lines = vec![make_line("buffer")];

    let registry = PluginRegistry::new();
    let element = build_buffer_surface_abstract(&state, &registry);

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

    impl Plugin for GutterPlugin {
        fn id(&self) -> PluginId {
            PluginId("gutter_plugin".into())
        }

        fn annotate_line_with_ctx(
            &self,
            line: usize,
            _state: &AppState,
            _ctx: &crate::plugin::AnnotateContext,
        ) -> Option<LineAnnotation> {
            if line == 0 {
                Some(LineAnnotation {
                    left_gutter: Some(Element::text("L", Face::default())),
                    right_gutter: Some(Element::text("R", Face::default())),
                    background: None,
                    priority: 0,
                })
            } else {
                None
            }
        }
    }

    let mut state = AppState::default();
    state.lines = vec![make_line("buffer")];

    let mut registry = PluginRegistry::new();
    registry.register(Box::new(GutterPlugin));
    let element = build_buffer_surface_abstract(&state, &registry);

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
fn test_surface_view_sections_cached_preserves_surface_reports() {
    let mut state = AppState::default();
    state.lines = vec![make_line("buffer")];
    state.status_line = make_line("status");
    state.status_mode_line = make_line("normal");

    let registry = PluginRegistry::new();
    let mut surface_registry = SurfaceRegistry::new();
    surface_registry.register(Box::new(KakouneBufferSurface::new()));
    surface_registry.register(Box::new(StatusBarSurface::new()));

    let mut cache = crate::render::cache::ViewCache::new();
    let sections = surface_view_sections_cached(&state, &registry, &surface_registry, &mut cache);

    let keys: HashSet<&str> = sections
        .surface_reports
        .iter()
        .map(|report| report.surface_key.as_str())
        .collect();
    assert!(keys.contains("kasane.buffer"));
    assert!(keys.contains("kasane.status"));

    let cached_reports = cache
        .base
        .value
        .as_ref()
        .map(|cached| {
            cached
                .surface_reports
                .iter()
                .map(|report| report.surface_key.as_str())
                .collect::<HashSet<_>>()
        })
        .unwrap();
    assert!(cached_reports.contains("kasane.buffer"));
    assert!(cached_reports.contains("kasane.status"));
    assert_eq!(
        surface_registry.surface_id_by_key("kasane.buffer"),
        Some(SurfaceId::BUFFER)
    );
}

struct SurfaceDepsPlugin;

impl Plugin for SurfaceDepsPlugin {
    fn id(&self) -> PluginId {
        PluginId("surface_deps".into())
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::CONTRIBUTOR
            | PluginCapabilities::TRANSFORMER
            | PluginCapabilities::ANNOTATOR
    }

    fn contribute_to(
        &self,
        _region: &SlotId,
        _state: &AppState,
        _ctx: &ContributeContext,
    ) -> Option<Contribution> {
        None
    }

    fn contribute_deps(&self, region: &SlotId) -> DirtyFlags {
        match region.as_str() {
            "kasane.buffer.left" => DirtyFlags::BUFFER_CURSOR,
            "test.surface.slot" => DirtyFlags::MENU_SELECTION,
            "kasane.buffer.overlay" => DirtyFlags::INFO,
            _ => DirtyFlags::empty(),
        }
    }

    fn transform_deps(&self, target: &TransformTarget) -> DirtyFlags {
        match target {
            TransformTarget::Buffer => DirtyFlags::MENU_STRUCTURE,
            TransformTarget::StatusBar => DirtyFlags::INFO,
            _ => DirtyFlags::empty(),
        }
    }

    fn annotate_deps(&self) -> DirtyFlags {
        DirtyFlags::MENU_SELECTION
    }
}

struct TestSurface {
    id: SurfaceId,
    key: &'static str,
    slots: Vec<SlotDeclaration>,
}

impl TestSurface {
    fn new(id: SurfaceId, key: &'static str, slots: Vec<SlotDeclaration>) -> Self {
        Self { id, key, slots }
    }
}

impl Surface for TestSurface {
    fn id(&self) -> SurfaceId {
        self.id
    }

    fn surface_key(&self) -> compact_str::CompactString {
        self.key.into()
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::fill()
    }

    fn view(&self, _ctx: &ViewContext<'_>) -> Element {
        Element::Empty
    }

    fn handle_event(
        &mut self,
        _event: SurfaceEvent,
        _ctx: &EventContext<'_>,
    ) -> Vec<crate::plugin::Command> {
        vec![]
    }

    fn declared_slots(&self) -> &[SlotDeclaration] {
        &self.slots
    }
}

#[test]
fn test_effective_surface_section_deps_uses_present_slots_and_active_surface_transforms() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(SurfaceDepsPlugin));

    let mut surface_registry = SurfaceRegistry::new();
    surface_registry.register(Box::new(KakouneBufferSurface::new()));
    surface_registry.register(Box::new(StatusBarSurface::new()));

    let cached = crate::surface::SurfaceComposeResult {
        base: Some(Element::Empty),
        surface_reports: vec![
            crate::surface::SurfaceRenderReport {
                surface_key: "kasane.buffer".into(),
                slot_records: vec![ResolvedSlotRecord {
                    surface_key: "kasane.buffer".into(),
                    slot_name: "kasane.buffer.left".into(),
                    instance_id: ResolvedSlotInstanceId(1),
                    direction: Direction::Row,
                    gap: 0,
                    contribution_count: 1,
                    content_kind: ResolvedSlotContentKind::Single,
                    area: None,
                }],
                absent_declared_slots: vec![],
                owner_errors: vec![],
                contributor_issues: vec![],
            },
            crate::surface::SurfaceRenderReport {
                surface_key: "kasane.status".into(),
                slot_records: vec![],
                absent_declared_slots: vec![],
                owner_errors: vec![],
                contributor_issues: vec![],
            },
        ],
    };

    let deps = effective_surface_section_deps(Some(&cached), &registry, &surface_registry);

    assert!(deps.base.contains(BUILD_BASE_DEPS));
    assert!(deps.base.contains(DirtyFlags::BUFFER_CURSOR));
    assert!(deps.base.contains(DirtyFlags::MENU_STRUCTURE));
    assert!(deps.base.contains(DirtyFlags::MENU_SELECTION));
    assert!(deps.base.contains(DirtyFlags::INFO));
    assert_eq!(deps.menu, registry.section_deps().menu);
    assert_eq!(deps.info, registry.section_deps().info);
}

#[test]
fn test_effective_surface_section_deps_falls_back_to_declared_slots_for_owner_failures() {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(SurfaceDepsPlugin));

    let mut surface_registry = SurfaceRegistry::new();
    surface_registry.register(Box::new(TestSurface::new(
        SurfaceId(SurfaceId::PLUGIN_BASE),
        "test.surface",
        vec![SlotDeclaration::new(
            "test.surface.slot",
            SlotKind::LeftRail,
        )],
    )));
    surface_registry.register(Box::new(KakouneBufferSurface::new()));

    let cached = crate::surface::SurfaceComposeResult {
        base: Some(Element::Empty),
        surface_reports: vec![
            crate::surface::SurfaceRenderReport {
                surface_key: "test.surface".into(),
                slot_records: vec![],
                absent_declared_slots: vec!["test.surface.slot".into()],
                owner_errors: vec![OwnerValidationError {
                    surface_key: "test.surface".into(),
                    kind: OwnerValidationErrorKind::UnresolvedSlotPlaceholder,
                    detail: "broken placeholder".into(),
                }],
                contributor_issues: vec![],
            },
            crate::surface::SurfaceRenderReport {
                surface_key: "kasane.buffer".into(),
                slot_records: vec![],
                absent_declared_slots: vec![
                    "kasane.buffer.left".into(),
                    "kasane.buffer.right".into(),
                    "kasane.buffer.above".into(),
                    "kasane.buffer.below".into(),
                    "kasane.buffer.overlay".into(),
                ],
                owner_errors: vec![OwnerValidationError {
                    surface_key: "kasane.buffer".into(),
                    kind: OwnerValidationErrorKind::UnresolvedSlotPlaceholder,
                    detail: "broken buffer".into(),
                }],
                contributor_issues: vec![],
            },
        ],
    };

    let deps = effective_surface_section_deps(Some(&cached), &registry, &surface_registry);

    assert!(deps.base.contains(DirtyFlags::MENU_SELECTION));
    assert!(deps.base.contains(DirtyFlags::INFO));
    assert!(deps.base.contains(DirtyFlags::MENU_STRUCTURE));
}
