//! Surface abstraction: first-class rectangular screen regions.
//!
//! A Surface owns a rectangular area of the screen and is responsible for
//! building its Element tree and handling events within that region.
//! Both core components (buffer, status bar) and plugins can implement Surface,
//! enabling symmetric extensibility.

pub mod buffer;
pub mod info;
pub mod menu;
pub mod status;

use std::any::Any;

use compact_str::CompactString;

use crate::element::Element;
use crate::input::{KeyEvent, MouseEvent};
use crate::layout::Rect;
use crate::plugin::{Command, PluginRegistry};
use crate::state::{AppState, DirtyFlags};

/// Unique identifier for a surface within a [`Workspace`](crate::workspace::Workspace).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SurfaceId(pub u32);

/// Well-known surface IDs for built-in core surfaces.
impl SurfaceId {
    /// The primary Kakoune buffer surface (always present).
    pub const BUFFER: SurfaceId = SurfaceId(0);
    /// The status bar surface (always present).
    pub const STATUS: SurfaceId = SurfaceId(1);
    /// The menu overlay surface (created/destroyed dynamically).
    pub const MENU: SurfaceId = SurfaceId(2);
    /// Base ID for info overlay surfaces. Info `i` uses `SurfaceId(INFO_BASE + i)`.
    pub const INFO_BASE: u32 = 10;
    /// First ID available for plugin-created surfaces.
    pub const PLUGIN_BASE: u32 = 100;
}

/// Size preferences for layout negotiation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SizeHint {
    pub min_width: u16,
    pub min_height: u16,
    pub preferred_width: Option<u16>,
    pub preferred_height: Option<u16>,
    /// Flex factor for proportional space allocation (0.0 = fixed, >0.0 = flexible).
    pub flex: f32,
}

impl SizeHint {
    /// Fixed-size surface.
    pub fn fixed(w: u16, h: u16) -> Self {
        SizeHint {
            min_width: w,
            min_height: h,
            preferred_width: Some(w),
            preferred_height: Some(h),
            flex: 0.0,
        }
    }

    /// Surface that fills all available space.
    pub fn fill() -> Self {
        SizeHint {
            min_width: 1,
            min_height: 1,
            preferred_width: None,
            preferred_height: None,
            flex: 1.0,
        }
    }

    /// Fixed height, flexible width.
    pub fn fixed_height(h: u16) -> Self {
        SizeHint {
            min_width: 1,
            min_height: h,
            preferred_width: None,
            preferred_height: Some(h),
            flex: 0.0,
        }
    }
}

impl Default for SizeHint {
    fn default() -> Self {
        SizeHint::fill()
    }
}

/// Context provided to a Surface when building its view.
pub struct ViewContext<'a> {
    /// Read-only application state.
    pub state: &'a AppState,
    /// The rectangular area allocated to this surface.
    pub rect: Rect,
    /// Whether this surface currently has focus.
    pub focused: bool,
    /// Plugin registry for collecting slot contributions.
    pub registry: &'a PluginRegistry,
    /// This surface's identifier.
    pub surface_id: SurfaceId,
}

/// Context provided to a Surface when handling events.
pub struct EventContext<'a> {
    /// Read-only application state.
    pub state: &'a AppState,
    /// The rectangular area allocated to this surface.
    pub rect: Rect,
}

/// Events delivered to a Surface.
#[derive(Debug)]
pub enum SurfaceEvent {
    /// A key event (routed to the focused surface).
    Key(KeyEvent),
    /// A mouse event (routed by hit testing).
    Mouse(MouseEvent),
    /// This surface gained focus.
    FocusGained,
    /// This surface lost focus.
    FocusLost,
    /// This surface was resized.
    Resize(Rect),
}

/// Position of a slot relative to the surface's content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotPosition {
    /// Above (column layout) or left of (row layout) the content.
    Before,
    /// Below (column layout) or right of (row layout) the content.
    After,
    /// Left side of the content area.
    Left,
    /// Right side of the content area.
    Right,
    /// Overlay on top of the content.
    Overlay,
}

/// Declaration of an extension point (slot) within a Surface.
#[derive(Debug, Clone)]
pub struct SlotDeclaration {
    /// Fully-qualified slot name (e.g., "kasane.buffer.left").
    pub name: CompactString,
    /// Where contributed elements are placed relative to the surface content.
    pub position: SlotPosition,
}

impl SlotDeclaration {
    pub fn new(name: impl Into<CompactString>, position: SlotPosition) -> Self {
        SlotDeclaration {
            name: name.into(),
            position,
        }
    }
}

/// A rectangular screen region that can build its own Element tree and handle events.
///
/// Both core components and plugins implement this trait, enabling symmetric
/// extensibility. The core Kakoune buffer view is just one Surface among equals.
pub trait Surface: Any + Send {
    /// Unique identifier for this surface.
    fn id(&self) -> SurfaceId;

    /// Size preferences for layout negotiation.
    fn size_hint(&self) -> SizeHint;

    /// Build the Element tree for the allocated rectangle.
    fn view(&self, ctx: &ViewContext<'_>) -> Element;

    /// Handle an event within this surface's region.
    fn handle_event(&mut self, event: SurfaceEvent, ctx: &EventContext<'_>) -> Vec<Command>;

    /// Notification that shared application state has changed.
    fn on_state_changed(&mut self, _state: &AppState, _dirty: DirtyFlags) -> Vec<Command> {
        vec![]
    }

    /// Hash of surface-internal state for view caching.
    /// A change in this value invalidates the cached view output.
    fn state_hash(&self) -> u64 {
        0
    }

    /// Extension points (slots) that this surface exposes to plugins.
    fn declared_slots(&self) -> &[SlotDeclaration] {
        &[]
    }
}

// ---------------------------------------------------------------------------
// SurfaceRegistry
// ---------------------------------------------------------------------------

use crate::workspace::Workspace;
use std::collections::HashMap;

/// Manages all Surface instances and the Workspace layout tree.
///
/// Coordinates view composition by calling each Surface's `view()` method
/// with the rectangle allocated by the Workspace, then assembling the
/// results into a single Element tree.
pub struct SurfaceRegistry {
    surfaces: HashMap<SurfaceId, Box<dyn Surface>>,
    workspace: Workspace,
}

impl SurfaceRegistry {
    /// Create a new registry with a default workspace rooted at `SurfaceId::BUFFER`.
    pub fn new() -> Self {
        SurfaceRegistry {
            surfaces: HashMap::new(),
            workspace: Workspace::default(),
        }
    }

    /// Create a registry with a custom initial workspace.
    pub fn with_workspace(workspace: Workspace) -> Self {
        SurfaceRegistry {
            surfaces: HashMap::new(),
            workspace,
        }
    }

    /// Register a surface. If a surface with the same ID already exists, it is replaced.
    pub fn register(&mut self, surface: Box<dyn Surface>) {
        self.surfaces.insert(surface.id(), surface);
    }

    /// Remove a surface by ID.
    pub fn remove(&mut self, id: SurfaceId) -> Option<Box<dyn Surface>> {
        self.surfaces.remove(&id)
    }

    /// Get a reference to a surface by ID.
    pub fn get(&self, id: SurfaceId) -> Option<&dyn Surface> {
        self.surfaces.get(&id).map(|s| s.as_ref())
    }

    /// Get a mutable reference to a surface by ID.
    pub fn get_mut(&mut self, id: SurfaceId) -> Option<&mut dyn Surface> {
        self.surfaces.get_mut(&id).map(|s| s.as_mut())
    }

    /// Get the workspace.
    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    /// Get a mutable reference to the workspace.
    pub fn workspace_mut(&mut self) -> &mut Workspace {
        &mut self.workspace
    }

    /// Number of registered surfaces.
    pub fn surface_count(&self) -> usize {
        self.surfaces.len()
    }

    /// Compose the full Element tree from all surfaces according to workspace layout.
    ///
    /// For each surface in the workspace tree, calls `surface.view()` with the
    /// allocated rectangle, then assembles results following the tree structure:
    /// - Split → Flex (Row or Column)
    /// - Tabs → active tab only (tab bar rendered separately)
    /// - Float → Stack with overlays
    /// - Leaf → direct Surface::view() output
    pub fn compose_view(
        &self,
        state: &AppState,
        plugin_registry: &PluginRegistry,
        total: Rect,
    ) -> Element {
        let rects = self.workspace.compute_rects(total);
        self.compose_node(self.workspace.root(), state, plugin_registry, &rects, total)
    }

    /// Compose the full UI: workspace content + status bar + overlays.
    ///
    /// This is the Surface-based equivalent of `view_cached()`. It:
    /// 1. Renders the workspace tree content (buffer panes) via Surface::view()
    /// 2. Adds the StatusBarSurface output (top or bottom based on `status_at_top`)
    /// 3. Uses the existing view layer for overlay positioning (menu, info, plugin)
    ///
    /// Overlay surfaces (menu, info) are managed via `sync_ephemeral_surfaces()`
    /// for lifecycle, but their view output uses the existing positioning functions
    /// from `render::view` which correctly compute anchor positions.
    pub fn compose_full_view(
        &self,
        state: &AppState,
        plugin_registry: &PluginRegistry,
        total: Rect,
    ) -> Element {
        use crate::element::FlexChild;
        use crate::render::view;

        let dummy_ctx = |surface_id: SurfaceId| ViewContext {
            state,
            rect: total,
            focused: self.workspace.focused() == surface_id,
            registry: plugin_registry,
            surface_id,
        };

        // 1. Build workspace content (buffer panes)
        let workspace_content = self.compose_view(state, plugin_registry, total);

        // 2. Build status bar (if StatusBarSurface is registered)
        let status_bar = self
            .surfaces
            .get(&SurfaceId::STATUS)
            .map(|s| s.view(&dummy_ctx(SurfaceId::STATUS)));

        // 3. Compose base: Column [status(top?), workspace(flex), status(bottom?)]
        let base = match status_bar {
            Some(status) => {
                let mut children = Vec::new();
                if state.status_at_top {
                    children.push(FlexChild::fixed(status));
                    children.push(FlexChild::flexible(workspace_content, 1.0));
                } else {
                    children.push(FlexChild::flexible(workspace_content, 1.0));
                    children.push(FlexChild::fixed(status));
                }
                Element::column(children)
            }
            None => workspace_content,
        };

        // 4. Collect overlays using the view layer's positioning functions.
        // These correctly compute anchor positions for menus and info popups.
        let mut overlays = Vec::new();
        if let Some(overlay) = view::build_menu_section_standalone(state, plugin_registry) {
            overlays.push(overlay);
        }
        overlays.extend(view::build_info_section_standalone(state, plugin_registry));
        {
            let overlay_ctx = crate::plugin::OverlayContext {
                screen_cols: state.cols,
                screen_rows: state.rows,
                menu_rect: None,
                existing_overlays: vec![],
            };
            overlays.extend(
                plugin_registry
                    .collect_overlays_with_ctx(state, &overlay_ctx)
                    .into_iter()
                    .map(|oc| crate::element::Overlay {
                        element: oc.element,
                        anchor: oc.anchor,
                    }),
            );
        }

        // 5. Assemble into final tree
        if overlays.is_empty() {
            base
        } else {
            Element::stack(base, overlays)
        }
    }

    /// Compose view decomposed into sections for per-section caching.
    ///
    /// Returns the same structure as `view::ViewSections`:
    /// - `base`: workspace content + status bar
    /// - `menu_overlay`, `info_overlays`, `plugin_overlays`: overlay sections
    pub(crate) fn compose_view_sections(
        &self,
        state: &AppState,
        plugin_registry: &PluginRegistry,
        total: Rect,
    ) -> crate::render::view::ViewSections {
        use crate::element::FlexChild;
        use crate::render::view;

        let dummy_ctx = |surface_id: SurfaceId| ViewContext {
            state,
            rect: total,
            focused: self.workspace.focused() == surface_id,
            registry: plugin_registry,
            surface_id,
        };

        // 1. Build workspace content (buffer panes)
        let workspace_content = self.compose_view(state, plugin_registry, total);

        // 2. Build status bar (if StatusBarSurface is registered)
        let status_bar = self
            .surfaces
            .get(&SurfaceId::STATUS)
            .map(|s| s.view(&dummy_ctx(SurfaceId::STATUS)));

        // 3. Compose base: Column [status(top?), workspace(flex), status(bottom?)]
        let base = match status_bar {
            Some(status) => {
                let mut children = Vec::new();
                if state.status_at_top {
                    children.push(FlexChild::fixed(status));
                    children.push(FlexChild::flexible(workspace_content, 1.0));
                } else {
                    children.push(FlexChild::flexible(workspace_content, 1.0));
                    children.push(FlexChild::fixed(status));
                }
                Element::column(children)
            }
            None => workspace_content,
        };

        // 4. Decomposed overlay sections
        let menu_overlay = view::build_menu_section_standalone(state, plugin_registry);
        let info_overlays = view::build_info_section_standalone(state, plugin_registry);
        let overlay_ctx = crate::plugin::OverlayContext {
            screen_cols: state.cols,
            screen_rows: state.rows,
            menu_rect: None,
            existing_overlays: vec![],
        };
        let plugin_overlays: Vec<crate::element::Overlay> = plugin_registry
            .collect_overlays_with_ctx(state, &overlay_ctx)
            .into_iter()
            .map(|oc| crate::element::Overlay {
                element: oc.element,
                anchor: oc.anchor,
            })
            .collect();

        view::ViewSections {
            base,
            menu_overlay,
            info_overlays,
            plugin_overlays,
        }
    }

    fn compose_node(
        &self,
        node: &crate::workspace::WorkspaceNode,
        state: &AppState,
        plugin_registry: &PluginRegistry,
        rects: &HashMap<SurfaceId, Rect>,
        _area: Rect,
    ) -> Element {
        use crate::element::FlexChild;
        use crate::workspace::WorkspaceNode;

        match node {
            WorkspaceNode::Leaf { surface_id } => {
                if let Some(surface) = self.surfaces.get(surface_id) {
                    let rect = rects.get(surface_id).copied().unwrap_or(Rect {
                        x: 0,
                        y: 0,
                        w: 0,
                        h: 0,
                    });
                    let ctx = ViewContext {
                        state,
                        rect,
                        focused: self.workspace.focused() == *surface_id,
                        registry: plugin_registry,
                        surface_id: *surface_id,
                    };
                    surface.view(&ctx)
                } else {
                    Element::Empty
                }
            }
            WorkspaceNode::Split {
                direction,
                first,
                second,
                ..
            } => {
                let elem_direction = match direction {
                    crate::pane::SplitDirection::Vertical => crate::element::Direction::Row,
                    crate::pane::SplitDirection::Horizontal => crate::element::Direction::Column,
                };
                let first_elem = self.compose_node(first, state, plugin_registry, rects, _area);
                let second_elem = self.compose_node(second, state, plugin_registry, rects, _area);
                Element::Flex {
                    direction: elem_direction,
                    children: vec![
                        FlexChild::flexible(first_elem, 1.0),
                        FlexChild::flexible(second_elem, 1.0),
                    ],
                    gap: 1, // 1-cell divider
                    align: crate::element::Align::Start,
                    cross_align: crate::element::Align::Start,
                }
            }
            WorkspaceNode::Tabs { tabs, active, .. } => {
                if let Some(active_tab) = tabs.get(*active) {
                    self.compose_node(active_tab, state, plugin_registry, rects, _area)
                } else {
                    Element::Empty
                }
            }
            WorkspaceNode::Float { base, floating } => {
                let base_elem = self.compose_node(base, state, plugin_registry, rects, _area);
                let mut overlays = Vec::new();
                for entry in floating {
                    let overlay_elem =
                        self.compose_node(&entry.node, state, plugin_registry, rects, _area);
                    overlays.push(crate::element::Overlay {
                        element: overlay_elem,
                        anchor: crate::element::OverlayAnchor::Absolute {
                            x: entry.rect.x,
                            y: entry.rect.y,
                            w: entry.rect.w,
                            h: entry.rect.h,
                        },
                    });
                }
                if overlays.is_empty() {
                    base_elem
                } else {
                    Element::stack(base_elem, overlays)
                }
            }
        }
    }

    /// Synchronize ephemeral surfaces (menu, infos) with the current AppState.
    ///
    /// Registers/removes MenuSurface and InfoSurface instances to match
    /// whether `state.menu` and `state.infos` are present.
    pub fn sync_ephemeral_surfaces(&mut self, state: &AppState) {
        // Menu surface
        if state.menu.is_some() {
            self.surfaces
                .entry(SurfaceId::MENU)
                .or_insert_with(|| Box::new(menu::MenuSurface));
        } else {
            self.surfaces.remove(&SurfaceId::MENU);
        }

        // Info surfaces: one per info popup
        // Remove stale info surfaces
        let info_count = state.infos.len();
        let stale_ids: Vec<SurfaceId> = self
            .surfaces
            .keys()
            .filter(|id| {
                id.0 >= SurfaceId::INFO_BASE
                    && id.0 < SurfaceId::PLUGIN_BASE
                    && (id.0 - SurfaceId::INFO_BASE) as usize >= info_count
            })
            .copied()
            .collect();
        for id in stale_ids {
            self.surfaces.remove(&id);
        }
        // Add missing info surfaces
        for i in 0..info_count {
            let id = SurfaceId(SurfaceId::INFO_BASE + i as u32);
            self.surfaces
                .entry(id)
                .or_insert_with(|| Box::new(info::InfoSurface::new(i)));
        }
    }

    /// Collect all declared slots across all registered surfaces.
    pub fn all_declared_slots(&self) -> Vec<(SurfaceId, &SlotDeclaration)> {
        let mut result = Vec::new();
        for (id, surface) in &self.surfaces {
            for slot in surface.declared_slots() {
                result.push((*id, slot));
            }
        }
        result
    }

    /// Find the surface that declares a given slot name.
    pub fn slot_owner(&self, slot_name: &str) -> Option<SurfaceId> {
        for (id, surface) in &self.surfaces {
            for slot in surface.declared_slots() {
                if slot.name.as_str() == slot_name {
                    return Some(*id);
                }
            }
        }
        None
    }

    /// Notify all surfaces of a state change.
    pub fn on_state_changed(&mut self, state: &AppState, dirty: DirtyFlags) -> Vec<Command> {
        let mut commands = Vec::new();
        for surface in self.surfaces.values_mut() {
            commands.extend(surface.on_state_changed(state, dirty));
        }
        commands
    }

    /// Route an event to the appropriate surface.
    pub fn route_event(
        &mut self,
        event: SurfaceEvent,
        state: &AppState,
        total: Rect,
    ) -> Vec<Command> {
        match &event {
            SurfaceEvent::Key(_) | SurfaceEvent::FocusGained | SurfaceEvent::FocusLost => {
                // Route to focused surface
                let focused = self.workspace.focused();
                if let Some(surface) = self.surfaces.get_mut(&focused) {
                    let rect = self
                        .workspace
                        .compute_rects(total)
                        .get(&focused)
                        .copied()
                        .unwrap_or(Rect {
                            x: 0,
                            y: 0,
                            w: 0,
                            h: 0,
                        });
                    let ctx = EventContext { state, rect };
                    surface.handle_event(event, &ctx)
                } else {
                    vec![]
                }
            }
            SurfaceEvent::Mouse(mouse_event) => {
                // Route to surface under cursor
                let target = self.workspace.surface_at(
                    mouse_event.column as u16,
                    mouse_event.line as u16,
                    total,
                );
                if let Some(surface_id) = target {
                    if let Some(surface) = self.surfaces.get_mut(&surface_id) {
                        let rect = self
                            .workspace
                            .compute_rects(total)
                            .get(&surface_id)
                            .copied()
                            .unwrap_or(Rect {
                                x: 0,
                                y: 0,
                                w: 0,
                                h: 0,
                            });
                        let ctx = EventContext { state, rect };
                        surface.handle_event(event, &ctx)
                    } else {
                        vec![]
                    }
                } else {
                    vec![]
                }
            }
            SurfaceEvent::Resize(_) => {
                // Resize is handled at workspace level, not individual surfaces
                vec![]
            }
        }
    }
}

impl Default for SurfaceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_surface_id_equality() {
        assert_eq!(SurfaceId(0), SurfaceId(0));
        assert_ne!(SurfaceId(0), SurfaceId(1));
        assert_eq!(SurfaceId::BUFFER, SurfaceId(0));
        assert_eq!(SurfaceId::STATUS, SurfaceId(1));
    }

    #[test]
    fn test_surface_id_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(SurfaceId(0));
        set.insert(SurfaceId(1));
        set.insert(SurfaceId(0));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_size_hint_fixed() {
        let hint = SizeHint::fixed(80, 24);
        assert_eq!(hint.min_width, 80);
        assert_eq!(hint.min_height, 24);
        assert_eq!(hint.preferred_width, Some(80));
        assert_eq!(hint.preferred_height, Some(24));
        assert_eq!(hint.flex, 0.0);
    }

    #[test]
    fn test_size_hint_fill() {
        let hint = SizeHint::fill();
        assert_eq!(hint.flex, 1.0);
        assert_eq!(hint.preferred_width, None);
        assert_eq!(hint.preferred_height, None);
    }

    #[test]
    fn test_size_hint_fixed_height() {
        let hint = SizeHint::fixed_height(1);
        assert_eq!(hint.min_height, 1);
        assert_eq!(hint.preferred_height, Some(1));
        assert_eq!(hint.flex, 0.0);
    }

    #[test]
    fn test_size_hint_default() {
        let hint = SizeHint::default();
        assert_eq!(hint, SizeHint::fill());
    }

    #[test]
    fn test_slot_declaration() {
        let slot = SlotDeclaration::new("kasane.buffer.left", SlotPosition::Left);
        assert_eq!(slot.name.as_str(), "kasane.buffer.left");
        assert_eq!(slot.position, SlotPosition::Left);
    }

    #[test]
    fn test_surface_trait_object_safety() {
        // Verify Surface can be used as a trait object
        fn _accepts_surface(_s: &dyn Surface) {}
        fn _accepts_boxed(_s: Box<dyn Surface>) {}
    }

    // --- SurfaceRegistry tests ---

    use crate::surface::buffer::KakouneBufferSurface;
    use crate::surface::status::StatusBarSurface;

    #[test]
    fn test_registry_register_and_get() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));
        reg.register(Box::new(StatusBarSurface::new()));
        assert_eq!(reg.surface_count(), 2);
        assert!(reg.get(SurfaceId::BUFFER).is_some());
        assert!(reg.get(SurfaceId::STATUS).is_some());
        assert!(reg.get(SurfaceId(99)).is_none());
    }

    #[test]
    fn test_registry_remove() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));
        assert_eq!(reg.surface_count(), 1);
        let removed = reg.remove(SurfaceId::BUFFER);
        assert!(removed.is_some());
        assert_eq!(reg.surface_count(), 0);
    }

    #[test]
    fn test_registry_replace_existing() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));
        reg.register(Box::new(KakouneBufferSurface::new())); // same ID
        assert_eq!(reg.surface_count(), 1);
    }

    #[test]
    fn test_registry_compose_single_surface() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));

        let state = crate::state::AppState::default();
        let plugin_reg = crate::plugin::PluginRegistry::new();
        let total = Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        };
        let element = reg.compose_view(&state, &plugin_reg, total);
        // KakouneBufferSurface delegates to view_cached(), which produces a real Element tree
        assert!(!matches!(element, Element::Empty));
    }

    #[test]
    fn test_registry_workspace_access() {
        let mut reg = SurfaceRegistry::new();
        assert_eq!(reg.workspace().surface_count(), 1); // default has BUFFER
        let new_id = reg
            .workspace_mut()
            .split_focused(crate::pane::SplitDirection::Vertical, 0.5);
        assert_eq!(reg.workspace().surface_count(), 2);
        assert!(reg.workspace().root().find(new_id).is_some());
    }

    // --- S6: Ephemeral surface lifecycle ---

    #[test]
    fn test_sync_ephemeral_no_menu_no_info() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));

        let state = crate::state::AppState::default();
        reg.sync_ephemeral_surfaces(&state);

        // No menu → no MenuSurface
        assert!(reg.get(SurfaceId::MENU).is_none());
        // No infos → no InfoSurface
        assert!(reg.get(SurfaceId(SurfaceId::INFO_BASE)).is_none());
    }

    fn make_test_menu() -> crate::state::MenuState {
        use crate::protocol::{Coord, Face, MenuStyle};
        crate::state::MenuState {
            items: vec![],
            anchor: Coord { line: 0, column: 0 },
            selected_item_face: Face::default(),
            menu_face: Face::default(),
            style: MenuStyle::Inline,
            selected: None,
            first_item: 0,
            columns: 1,
            win_height: 0,
            menu_lines: 0,
            max_item_width: 0,
            screen_w: 80,
            columns_split: None,
        }
    }

    fn make_test_info() -> crate::state::InfoState {
        use crate::protocol::{Coord, Face, InfoStyle};
        crate::state::InfoState {
            title: vec![],
            content: vec![],
            anchor: Coord { line: 0, column: 0 },
            face: Face::default(),
            style: InfoStyle::Prompt,
            identity: crate::state::InfoIdentity {
                style: InfoStyle::Prompt,
                anchor_line: 0,
            },
            scroll_offset: 0,
        }
    }

    #[test]
    fn test_sync_ephemeral_menu_appears_and_disappears() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));

        let mut state = crate::state::AppState::default();

        // Menu appears
        state.menu = Some(make_test_menu());
        reg.sync_ephemeral_surfaces(&state);
        assert!(reg.get(SurfaceId::MENU).is_some());

        // Menu disappears
        state.menu = None;
        reg.sync_ephemeral_surfaces(&state);
        assert!(reg.get(SurfaceId::MENU).is_none());
    }

    #[test]
    fn test_sync_ephemeral_info_count_tracks_state() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));

        let mut state = crate::state::AppState::default();

        // Two infos appear
        state.infos.push(make_test_info());
        state.infos.push(make_test_info());
        reg.sync_ephemeral_surfaces(&state);
        assert!(reg.get(SurfaceId(SurfaceId::INFO_BASE)).is_some());
        assert!(reg.get(SurfaceId(SurfaceId::INFO_BASE + 1)).is_some());
        assert!(reg.get(SurfaceId(SurfaceId::INFO_BASE + 2)).is_none());

        // One info removed
        state.infos.pop();
        reg.sync_ephemeral_surfaces(&state);
        assert!(reg.get(SurfaceId(SurfaceId::INFO_BASE)).is_some());
        assert!(reg.get(SurfaceId(SurfaceId::INFO_BASE + 1)).is_none());

        // All infos removed
        state.infos.clear();
        reg.sync_ephemeral_surfaces(&state);
        assert!(reg.get(SurfaceId(SurfaceId::INFO_BASE)).is_none());
    }

    #[test]
    fn test_menu_surface_id() {
        let surface = menu::MenuSurface;
        assert_eq!(surface.id(), SurfaceId::MENU);
    }

    #[test]
    fn test_info_surface_id() {
        let surface = info::InfoSurface::new(0);
        assert_eq!(surface.id(), SurfaceId(SurfaceId::INFO_BASE));
        let surface2 = info::InfoSurface::new(3);
        assert_eq!(surface2.id(), SurfaceId(SurfaceId::INFO_BASE + 3));
    }

    // --- S7: Surface-local named slots ---

    #[test]
    fn test_all_declared_slots() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));
        reg.register(Box::new(StatusBarSurface::new()));

        let slots = reg.all_declared_slots();
        // KakouneBufferSurface declares 5 slots, StatusBarSurface declares 3
        assert_eq!(slots.len(), 8);

        let slot_names: Vec<&str> = slots.iter().map(|(_, s)| s.name.as_str()).collect();
        assert!(slot_names.contains(&"kasane.buffer.left"));
        assert!(slot_names.contains(&"kasane.buffer.right"));
        assert!(slot_names.contains(&"kasane.buffer.above"));
        assert!(slot_names.contains(&"kasane.buffer.below"));
        assert!(slot_names.contains(&"kasane.buffer.overlay"));
        assert!(slot_names.contains(&"kasane.status.above"));
        assert!(slot_names.contains(&"kasane.status.left"));
        assert!(slot_names.contains(&"kasane.status.right"));
    }

    #[test]
    fn test_slot_owner() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));
        reg.register(Box::new(StatusBarSurface::new()));

        assert_eq!(
            reg.slot_owner("kasane.buffer.left"),
            Some(SurfaceId::BUFFER)
        );
        assert_eq!(
            reg.slot_owner("kasane.status.right"),
            Some(SurfaceId::STATUS)
        );
        assert_eq!(reg.slot_owner("nonexistent.slot"), None);
    }

    #[test]
    fn test_slot_owner_after_surface_removal() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));
        assert!(reg.slot_owner("kasane.buffer.left").is_some());

        reg.remove(SurfaceId::BUFFER);
        assert!(reg.slot_owner("kasane.buffer.left").is_none());
    }
}
