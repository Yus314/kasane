//! Surface abstraction: first-class rectangular screen regions.
//!
//! A Surface owns a rectangular area of the screen and is responsible for
//! building its Element tree and handling events within that region.
//! Both core components (buffer, status bar) and plugins can implement Surface,
//! enabling symmetric extensibility.

pub mod buffer;
pub mod info;
pub mod menu;
pub mod resolve;
pub mod status;

use std::{any::Any, collections::HashMap};

use compact_str::CompactString;

use crate::element::Element;
use crate::input::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use crate::layout::{Rect, SplitDirection};
use crate::plugin::{Command, PluginId, PluginRegistry};
use crate::state::{AppState, DirtyFlags};
use crate::workspace::{
    DockPosition, Placement, Workspace, WorkspaceCommand, WorkspaceDivider, WorkspaceDividerId,
};

pub use resolve::{
    ContributorIssue, ContributorIssueKind, OwnerValidationError, OwnerValidationErrorKind,
    ResolvedSlotContentKind, ResolvedSlotRecord, ResolvedTree, SurfaceComposeResult,
    SurfaceRenderOutcome, SurfaceRenderReport,
};

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
    /// Whether this surface currently has focus.
    pub focused: bool,
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

/// Advisory kind for a surface slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind {
    /// A band above the surface's main content.
    AboveBand,
    /// A band below the surface's main content.
    BelowBand,
    /// A rail on the left side of the surface.
    LeftRail,
    /// A rail on the right side of the surface.
    RightRail,
    /// An overlay slot layered on top of the surface.
    Overlay,
}

/// Static placement request for a surface descriptor.
///
/// Unlike [`Placement`], keyed placements refer to target surfaces by stable
/// `surface_key`, so they can be declared before runtime `SurfaceId`s exist.
#[derive(Debug, Clone, PartialEq)]
pub enum SurfacePlacementRequest {
    SplitFocused {
        direction: SplitDirection,
        ratio: f32,
    },
    SplitFrom {
        target_surface_key: CompactString,
        direction: SplitDirection,
        ratio: f32,
    },
    Tab,
    TabIn {
        target_surface_key: CompactString,
    },
    Dock(DockPosition),
    Float {
        rect: Rect,
    },
}

/// Declaration of an extension point (slot) within a Surface.
#[derive(Debug, Clone)]
pub struct SlotDeclaration {
    /// Fully-qualified slot name (e.g., "kasane.buffer.left").
    pub name: CompactString,
    /// Advisory kind for documentation and discovery.
    pub kind: SlotKind,
}

impl SlotDeclaration {
    pub fn new(name: impl Into<CompactString>, kind: SlotKind) -> Self {
        SlotDeclaration {
            name: name.into(),
            kind,
        }
    }
}

/// Registration-time descriptor for a surface's static contract.
#[derive(Debug, Clone)]
pub struct SurfaceDescriptor {
    pub surface_id: SurfaceId,
    pub surface_key: CompactString,
    pub declared_slots: Vec<SlotDeclaration>,
    pub initial_placement: Option<SurfacePlacementRequest>,
    declared_slot_lookup: HashMap<CompactString, usize>,
}

impl SurfaceDescriptor {
    fn from_surface(surface: &dyn Surface) -> Result<Self, SurfaceRegistrationError> {
        let surface_key = surface.surface_key();
        let declared_slots = surface.declared_slots().to_vec();
        let mut declared_slot_lookup = HashMap::new();
        for (index, slot) in declared_slots.iter().enumerate() {
            if declared_slot_lookup
                .insert(slot.name.clone(), index)
                .is_some()
            {
                return Err(SurfaceRegistrationError::DuplicateDeclaredSlotInSurface {
                    surface_key,
                    slot_name: slot.name.clone(),
                });
            }
        }
        Ok(Self {
            surface_id: surface.id(),
            surface_key,
            declared_slots,
            initial_placement: surface.initial_placement(),
            declared_slot_lookup,
        })
    }

    pub fn declares_slot(&self, slot_name: &str) -> bool {
        self.declared_slot_lookup.contains_key(slot_name)
    }

    pub fn declared_slot(&self, slot_name: &str) -> Option<&SlotDeclaration> {
        self.declared_slot_lookup
            .get(slot_name)
            .and_then(|index| self.declared_slots.get(*index))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SurfaceRegistrationError {
    DuplicateSurfaceId {
        surface_id: SurfaceId,
        existing_surface_key: CompactString,
        new_surface_key: CompactString,
    },
    DuplicateSurfaceKey {
        surface_key: CompactString,
    },
    DuplicateDeclaredSlot {
        slot_name: CompactString,
        existing_surface_key: CompactString,
        new_surface_key: CompactString,
    },
    DuplicateDeclaredSlotInSurface {
        surface_key: CompactString,
        slot_name: CompactString,
    },
}

struct RegisteredSurface {
    surface: Box<dyn Surface>,
    descriptor: SurfaceDescriptor,
    owner_plugin: Option<PluginId>,
}

#[derive(Debug, Clone, Copy)]
struct ActiveDividerDrag {
    divider_id: WorkspaceDividerId,
    direction: SplitDirection,
    start_main: u16,
    start_ratio: f32,
    available_main: u16,
}

pub struct SourcedSurfaceCommands {
    pub source_plugin: Option<PluginId>,
    pub commands: Vec<Command>,
}

impl std::fmt::Debug for SourcedSurfaceCommands {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourcedSurfaceCommands")
            .field("source_plugin", &self.source_plugin)
            .field("commands_len", &self.commands.len())
            .finish()
    }
}

/// A rectangular screen region that can build its own Element tree and handle events.
///
/// Both core components and plugins implement this trait, enabling symmetric
/// extensibility. The core Kakoune buffer view is just one Surface among equals.
pub trait Surface: Any + Send {
    /// Unique identifier for this surface.
    fn id(&self) -> SurfaceId;

    /// Stable semantic key for this surface.
    fn surface_key(&self) -> CompactString;

    /// Size preferences for layout negotiation.
    fn size_hint(&self) -> SizeHint;

    /// Static initial placement request for this surface.
    fn initial_placement(&self) -> Option<SurfacePlacementRequest> {
        None
    }

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

/// Manages all Surface instances and the Workspace layout tree.
///
/// Coordinates view composition by calling each Surface's `view()` method
/// with the rectangle allocated by the Workspace, then assembling the
/// results into a single Element tree.
pub struct SurfaceRegistry {
    surfaces: HashMap<SurfaceId, RegisteredSurface>,
    surface_ids_by_key: HashMap<CompactString, SurfaceId>,
    slot_owners_by_name: HashMap<CompactString, SurfaceId>,
    workspace: Workspace,
    active_divider_drag: Option<ActiveDividerDrag>,
}

impl SurfaceRegistry {
    /// Create a new registry with a default workspace rooted at `SurfaceId::BUFFER`.
    pub fn new() -> Self {
        SurfaceRegistry {
            surfaces: HashMap::new(),
            surface_ids_by_key: HashMap::new(),
            slot_owners_by_name: HashMap::new(),
            workspace: Workspace::default(),
            active_divider_drag: None,
        }
    }

    /// Create a registry with a custom initial workspace.
    pub fn with_workspace(workspace: Workspace) -> Self {
        SurfaceRegistry {
            surfaces: HashMap::new(),
            surface_ids_by_key: HashMap::new(),
            slot_owners_by_name: HashMap::new(),
            workspace,
            active_divider_drag: None,
        }
    }

    /// Register a surface after validating its static contract.
    pub fn try_register(
        &mut self,
        surface: Box<dyn Surface>,
    ) -> Result<(), SurfaceRegistrationError> {
        self.try_register_for_owner(surface, None)
    }

    /// Register a surface owned by the given plugin after validating its static contract.
    pub fn try_register_for_owner(
        &mut self,
        surface: Box<dyn Surface>,
        owner_plugin: Option<PluginId>,
    ) -> Result<(), SurfaceRegistrationError> {
        let descriptor = SurfaceDescriptor::from_surface(surface.as_ref())?;

        if let Some(existing) = self.surfaces.get(&descriptor.surface_id) {
            return Err(SurfaceRegistrationError::DuplicateSurfaceId {
                surface_id: descriptor.surface_id,
                existing_surface_key: existing.descriptor.surface_key.clone(),
                new_surface_key: descriptor.surface_key.clone(),
            });
        }
        if self
            .surface_ids_by_key
            .contains_key(descriptor.surface_key.as_str())
        {
            return Err(SurfaceRegistrationError::DuplicateSurfaceKey {
                surface_key: descriptor.surface_key.clone(),
            });
        }
        for slot in &descriptor.declared_slots {
            if let Some(existing_id) = self.slot_owners_by_name.get(slot.name.as_str()) {
                let existing_surface_key = self
                    .surfaces
                    .get(existing_id)
                    .map(|entry| entry.descriptor.surface_key.clone())
                    .unwrap_or_else(|| CompactString::const_new("<unknown>"));
                return Err(SurfaceRegistrationError::DuplicateDeclaredSlot {
                    slot_name: slot.name.clone(),
                    existing_surface_key,
                    new_surface_key: descriptor.surface_key.clone(),
                });
            }
        }

        let surface_id = descriptor.surface_id;
        let surface_key = descriptor.surface_key.clone();
        for slot in &descriptor.declared_slots {
            self.slot_owners_by_name
                .insert(slot.name.clone(), surface_id);
        }
        self.surface_ids_by_key.insert(surface_key, surface_id);
        self.surfaces.insert(
            surface_id,
            RegisteredSurface {
                surface,
                descriptor,
                owner_plugin,
            },
        );
        Ok(())
    }

    /// Register a surface and panic if validation fails.
    #[track_caller]
    pub fn register(&mut self, surface: Box<dyn Surface>) {
        if let Err(err) = self.try_register(surface) {
            panic!("surface registration failed: {err:?}");
        }
    }

    /// Remove a surface by ID.
    pub fn remove(&mut self, id: SurfaceId) -> Option<Box<dyn Surface>> {
        let entry = self.surfaces.remove(&id)?;
        self.surface_ids_by_key
            .remove(entry.descriptor.surface_key.as_str());
        for slot in &entry.descriptor.declared_slots {
            self.slot_owners_by_name.remove(slot.name.as_str());
        }
        Some(entry.surface)
    }

    /// Get a reference to a surface by ID.
    pub fn get(&self, id: SurfaceId) -> Option<&dyn Surface> {
        self.surfaces.get(&id).map(|entry| entry.surface.as_ref())
    }

    /// Get a mutable reference to a surface by ID.
    pub fn get_mut(&mut self, id: SurfaceId) -> Option<&mut dyn Surface> {
        self.surfaces
            .get_mut(&id)
            .map(|entry| entry.surface.as_mut())
    }

    /// Get a registration-time descriptor by surface ID.
    pub fn descriptor(&self, id: SurfaceId) -> Option<&SurfaceDescriptor> {
        self.surfaces.get(&id).map(|entry| &entry.descriptor)
    }

    /// Get the owning plugin for a surface, if it is plugin-provided.
    pub fn surface_owner_plugin(&self, id: SurfaceId) -> Option<&PluginId> {
        self.surfaces
            .get(&id)
            .and_then(|entry| entry.owner_plugin.as_ref())
    }

    /// Resolve a surface key to its surface ID.
    pub fn surface_id_by_key(&self, surface_key: &str) -> Option<SurfaceId> {
        self.surface_ids_by_key.get(surface_key).copied()
    }

    /// Get the workspace.
    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    /// Get a mutable reference to the workspace.
    pub fn workspace_mut(&mut self) -> &mut Workspace {
        &mut self.workspace
    }

    /// Return visible workspace split dividers for the current layout.
    pub fn workspace_dividers(&self, total: Rect) -> Vec<WorkspaceDivider> {
        self.workspace.compute_dividers(total)
    }

    /// Consume a mouse event that targets a workspace split divider.
    ///
    /// Returns `Some(dirty)` when the event was consumed by divider drag
    /// handling, otherwise `None`.
    pub fn handle_workspace_divider_mouse(
        &mut self,
        mouse: &MouseEvent,
        total: Rect,
    ) -> Option<DirtyFlags> {
        let main_coord = |direction: SplitDirection, mouse: &MouseEvent| -> u16 {
            match direction {
                SplitDirection::Vertical => mouse.column as u16,
                SplitDirection::Horizontal => mouse.line as u16,
            }
        };

        match mouse.kind {
            MouseEventKind::Press(MouseButton::Left) => {
                let x = mouse.column as u16;
                let y = mouse.line as u16;
                if self.workspace.surface_at(x, y, total).is_some() {
                    return None;
                }
                let divider = self.workspace.divider_at(x, y, total)?;
                self.active_divider_drag = Some(ActiveDividerDrag {
                    divider_id: divider.id,
                    direction: divider.direction,
                    start_main: main_coord(divider.direction, mouse),
                    start_ratio: divider.ratio,
                    available_main: divider.available_main.max(1),
                });
                Some(DirtyFlags::empty())
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                let drag = self.active_divider_drag?;
                let delta_main =
                    i32::from(main_coord(drag.direction, mouse)) - i32::from(drag.start_main);
                let new_ratio = drag.start_ratio + delta_main as f32 / drag.available_main as f32;
                if self.workspace.set_divider_ratio(drag.divider_id, new_ratio) {
                    Some(DirtyFlags::ALL)
                } else {
                    Some(DirtyFlags::empty())
                }
            }
            MouseEventKind::Release(MouseButton::Left) => {
                self.active_divider_drag.take().map(|_| DirtyFlags::empty())
            }
            _ => None,
        }
    }

    /// Resolve a static placement request into a runtime workspace placement.
    pub fn resolve_placement_request(
        &self,
        request: &SurfacePlacementRequest,
    ) -> Option<Placement> {
        match request {
            SurfacePlacementRequest::SplitFocused { direction, ratio } => {
                Some(Placement::SplitFocused {
                    direction: *direction,
                    ratio: *ratio,
                })
            }
            SurfacePlacementRequest::SplitFrom {
                target_surface_key,
                direction,
                ratio,
            } => self
                .surface_id_by_key(target_surface_key.as_str())
                .map(|target| Placement::SplitFrom {
                    target,
                    direction: *direction,
                    ratio: *ratio,
                }),
            SurfacePlacementRequest::Tab => Some(Placement::Tab),
            SurfacePlacementRequest::TabIn { target_surface_key } => self
                .surface_id_by_key(target_surface_key.as_str())
                .map(|target| Placement::TabIn { target }),
            SurfacePlacementRequest::Dock(position) => Some(Placement::Dock(*position)),
            SurfacePlacementRequest::Float { rect } => Some(Placement::Float { rect: *rect }),
        }
    }

    /// Apply initial placements for newly registered surfaces.
    ///
    /// If a surface descriptor carries a keyed initial placement, it takes
    /// precedence over the legacy plugin-wide placement. If the keyed request
    /// cannot be resolved, the surface is left unplaced and the failure is
    /// returned to the caller for diagnostics.
    pub fn apply_initial_placements(
        &mut self,
        surface_ids: &[SurfaceId],
        legacy_placement: Option<&Placement>,
        dirty: &mut DirtyFlags,
    ) -> Vec<(SurfaceId, SurfacePlacementRequest)> {
        self.apply_initial_placements_with_total(surface_ids, legacy_placement, dirty, None)
    }

    /// Apply initial placements for newly registered surfaces with an optional
    /// root layout rect for size-hint-aware placement decisions.
    pub fn apply_initial_placements_with_total(
        &mut self,
        surface_ids: &[SurfaceId],
        legacy_placement: Option<&Placement>,
        dirty: &mut DirtyFlags,
        total: Option<Rect>,
    ) -> Vec<(SurfaceId, SurfacePlacementRequest)> {
        let mut unresolved = Vec::new();

        for surface_id in surface_ids {
            let descriptor = match self.descriptor(*surface_id).cloned() {
                Some(descriptor) => descriptor,
                None => continue,
            };

            let placement = match descriptor.initial_placement.as_ref() {
                Some(request) => match self.resolve_placement_request(request) {
                    Some(placement) => Some(placement),
                    None => {
                        unresolved.push((*surface_id, request.clone()));
                        None
                    }
                },
                None => legacy_placement.cloned(),
            };

            if let Some(placement) = placement {
                crate::workspace::dispatch_workspace_command_with_total(
                    self,
                    WorkspaceCommand::AddSurface {
                        surface_id: *surface_id,
                        placement,
                    },
                    dirty,
                    total,
                );
            }
        }

        unresolved
    }

    /// Number of registered surfaces.
    pub fn surface_count(&self) -> usize {
        self.surfaces.len()
    }

    fn render_surface_outcome(
        &self,
        entry: &RegisteredSurface,
        state: &AppState,
        plugin_registry: &PluginRegistry,
        rect: Rect,
        focused: bool,
    ) -> SurfaceRenderOutcome {
        let ctx = ViewContext {
            state,
            rect,
            focused,
            registry: plugin_registry,
            surface_id: entry.descriptor.surface_id,
        };
        let abstract_root = entry.surface.view(&ctx);
        resolve::resolve_surface_tree(
            &entry.descriptor,
            abstract_root,
            state,
            plugin_registry,
            rect,
        )
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
        self.compose_base_result(state, plugin_registry, total)
            .base
            .unwrap_or(Element::Empty)
    }

    /// Compose the full UI: workspace content + status bar + overlays.
    ///
    /// This is the surface-based base composition path used by `view_cached()`. It:
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
        use crate::render::view;
        let base = self
            .compose_base_result(state, plugin_registry, total)
            .base
            .unwrap_or(Element::Empty);

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

    pub(crate) fn compose_base_result(
        &self,
        state: &AppState,
        plugin_registry: &PluginRegistry,
        total: Rect,
    ) -> SurfaceComposeResult {
        use crate::element::FlexChild;
        let rects = self.workspace.compute_rects(total);
        let (workspace_content, mut surface_reports) =
            self.compose_node_with_reports(self.workspace.root(), state, plugin_registry, &rects);

        let status_bar = self.surfaces.get(&SurfaceId::STATUS).map(|entry| {
            let outcome = self.render_surface_outcome(
                entry,
                state,
                plugin_registry,
                total,
                self.workspace.focused() == SurfaceId::STATUS,
            );
            surface_reports.push(outcome.report);
            outcome.tree.map(|tree| tree.root).unwrap_or(Element::Empty)
        });

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

        SurfaceComposeResult {
            base: Some(base),
            surface_reports,
        }
    }

    /// Compose view decomposed into sections for per-section caching.
    ///
    /// Returns the same structure as `view::ViewSections`:
    /// - `base`: workspace content + status bar
    /// - `menu_overlay`, `info_overlays`, `plugin_overlays`: overlay sections
    #[allow(dead_code)]
    pub(crate) fn compose_view_sections(
        &self,
        state: &AppState,
        plugin_registry: &PluginRegistry,
        total: Rect,
    ) -> crate::render::view::ViewSections {
        use crate::render::view;

        let base_result = self.compose_base_result(state, plugin_registry, total);
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
            base: base_result.base.unwrap_or(Element::Empty),
            menu_overlay,
            info_overlays,
            plugin_overlays,
            surface_reports: base_result.surface_reports,
        }
    }

    fn compose_node_with_reports(
        &self,
        node: &crate::workspace::WorkspaceNode,
        state: &AppState,
        plugin_registry: &PluginRegistry,
        rects: &HashMap<SurfaceId, Rect>,
    ) -> (Element, Vec<SurfaceRenderReport>) {
        use crate::element::FlexChild;
        use crate::workspace::WorkspaceNode;

        match node {
            WorkspaceNode::Leaf { surface_id } => {
                if let Some(entry) = self.surfaces.get(surface_id) {
                    let rect = rects.get(surface_id).copied().unwrap_or(Rect {
                        x: 0,
                        y: 0,
                        w: 0,
                        h: 0,
                    });
                    let outcome = self.render_surface_outcome(
                        entry,
                        state,
                        plugin_registry,
                        rect,
                        self.workspace.focused() == *surface_id,
                    );
                    (
                        outcome.tree.map(|tree| tree.root).unwrap_or(Element::Empty),
                        vec![outcome.report],
                    )
                } else {
                    (Element::Empty, vec![])
                }
            }
            WorkspaceNode::Split {
                direction,
                first,
                second,
                ..
            } => {
                let divider = Element::container(
                    Element::Empty,
                    crate::element::Style::Token(crate::element::StyleToken::SPLIT_DIVIDER),
                );
                let elem_direction = match direction {
                    crate::pane::SplitDirection::Vertical => crate::element::Direction::Row,
                    crate::pane::SplitDirection::Horizontal => crate::element::Direction::Column,
                };
                let (first_elem, mut first_reports) =
                    self.compose_node_with_reports(first, state, plugin_registry, rects);
                let (second_elem, second_reports) =
                    self.compose_node_with_reports(second, state, plugin_registry, rects);
                first_reports.extend(second_reports);
                (
                    Element::Flex {
                        direction: elem_direction,
                        children: vec![
                            FlexChild::flexible(first_elem, 1.0),
                            FlexChild {
                                element: divider,
                                flex: 0.0,
                                min_size: Some(1),
                                max_size: Some(1),
                            },
                            FlexChild::flexible(second_elem, 1.0),
                        ],
                        gap: 0,
                        align: crate::element::Align::Start,
                        cross_align: crate::element::Align::Start,
                    },
                    first_reports,
                )
            }
            WorkspaceNode::Tabs { tabs, active, .. } => {
                if let Some(active_tab) = tabs.get(*active) {
                    self.compose_node_with_reports(active_tab, state, plugin_registry, rects)
                } else {
                    (Element::Empty, vec![])
                }
            }
            WorkspaceNode::Float { base, floating } => {
                let (base_elem, mut surface_reports) =
                    self.compose_node_with_reports(base, state, plugin_registry, rects);
                let mut overlays = Vec::new();
                for entry in floating {
                    let (overlay_elem, overlay_reports) =
                        self.compose_node_with_reports(&entry.node, state, plugin_registry, rects);
                    surface_reports.extend(overlay_reports);
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
                let composed = if overlays.is_empty() {
                    base_elem
                } else {
                    Element::stack(base_elem, overlays)
                };
                (composed, surface_reports)
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
            if !self.surfaces.contains_key(&SurfaceId::MENU) {
                self.register(Box::new(menu::MenuSurface));
            }
        } else {
            self.remove(SurfaceId::MENU);
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
            self.remove(id);
        }
        // Add missing info surfaces
        for i in 0..info_count {
            let id = SurfaceId(SurfaceId::INFO_BASE + i as u32);
            if !self.surfaces.contains_key(&id) {
                self.register(Box::new(info::InfoSurface::new(i)));
            }
        }
    }

    /// Collect all declared slots across all registered surfaces.
    pub fn all_declared_slots(&self) -> Vec<(SurfaceId, &SlotDeclaration)> {
        let mut result = Vec::new();
        for (id, entry) in &self.surfaces {
            for slot in &entry.descriptor.declared_slots {
                result.push((*id, slot));
            }
        }
        result
    }

    /// Find the surface that declares a given slot name.
    pub fn slot_owner(&self, slot_name: &str) -> Option<SurfaceId> {
        self.slot_owners_by_name.get(slot_name).copied()
    }

    /// Notify all surfaces of a state change.
    pub fn on_state_changed(&mut self, state: &AppState, dirty: DirtyFlags) -> Vec<Command> {
        self.on_state_changed_with_sources(state, dirty)
            .into_iter()
            .flat_map(|entry| entry.commands)
            .collect()
    }

    /// Notify all surfaces of a state change and preserve source plugins.
    pub fn on_state_changed_with_sources(
        &mut self,
        state: &AppState,
        dirty: DirtyFlags,
    ) -> Vec<SourcedSurfaceCommands> {
        let mut results = Vec::new();
        for entry in self.surfaces.values_mut() {
            let commands = entry.surface.on_state_changed(state, dirty);
            if !commands.is_empty() {
                results.push(SourcedSurfaceCommands {
                    source_plugin: entry.owner_plugin.clone(),
                    commands,
                });
            }
        }
        results
    }

    /// Route an event to the appropriate surface.
    pub fn route_event(
        &mut self,
        event: SurfaceEvent,
        state: &AppState,
        total: Rect,
    ) -> Vec<Command> {
        self.route_event_with_sources(event, state, total)
            .into_iter()
            .flat_map(|entry| entry.commands)
            .collect()
    }

    /// Route an event and preserve the source plugin for each surface-local command batch.
    pub fn route_event_with_sources(
        &mut self,
        event: SurfaceEvent,
        state: &AppState,
        total: Rect,
    ) -> Vec<SourcedSurfaceCommands> {
        match &event {
            SurfaceEvent::Key(_) | SurfaceEvent::FocusGained | SurfaceEvent::FocusLost => {
                // Route to focused surface
                let focused = self.workspace.focused();
                if let Some(entry) = self.surfaces.get_mut(&focused) {
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
                    let ctx = EventContext {
                        state,
                        rect,
                        focused: !matches!(event, SurfaceEvent::FocusLost),
                    };
                    let commands = entry.surface.handle_event(event, &ctx);
                    if commands.is_empty() {
                        vec![]
                    } else {
                        vec![SourcedSurfaceCommands {
                            source_plugin: entry.owner_plugin.clone(),
                            commands,
                        }]
                    }
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
                    if let Some(entry) = self.surfaces.get_mut(&surface_id) {
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
                        let ctx = EventContext {
                            state,
                            rect,
                            focused: surface_id == self.workspace.focused(),
                        };
                        let commands = entry.surface.handle_event(event, &ctx);
                        if commands.is_empty() {
                            vec![]
                        } else {
                            vec![SourcedSurfaceCommands {
                                source_plugin: entry.owner_plugin.clone(),
                                commands,
                            }]
                        }
                    } else {
                        vec![]
                    }
                } else {
                    vec![]
                }
            }
            SurfaceEvent::Resize(_) => {
                let rects = self.workspace.compute_rects(total);
                let focused = self.workspace.focused();
                let mut results = Vec::new();
                for (surface_id, rect) in rects {
                    if let Some(entry) = self.surfaces.get_mut(&surface_id) {
                        let ctx = EventContext {
                            state,
                            rect,
                            focused: surface_id == focused,
                        };
                        let commands = entry.surface.handle_event(SurfaceEvent::Resize(rect), &ctx);
                        if !commands.is_empty() {
                            results.push(SourcedSurfaceCommands {
                                source_plugin: entry.owner_plugin.clone(),
                                commands,
                            });
                        }
                    }
                }
                results
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
        let slot = SlotDeclaration::new("kasane.buffer.left", SlotKind::LeftRail);
        assert_eq!(slot.name.as_str(), "kasane.buffer.left");
        assert_eq!(slot.kind, SlotKind::LeftRail);
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

    struct TestSurface {
        id: SurfaceId,
        surface_key: CompactString,
        slots: Vec<SlotDeclaration>,
        initial_placement: Option<SurfacePlacementRequest>,
        size_hint: SizeHint,
    }

    impl TestSurface {
        fn new(
            id: SurfaceId,
            surface_key: impl Into<CompactString>,
            slots: Vec<SlotDeclaration>,
        ) -> Self {
            Self {
                id,
                surface_key: surface_key.into(),
                slots,
                initial_placement: None,
                size_hint: SizeHint::fill(),
            }
        }

        fn with_initial_placement(mut self, initial_placement: SurfacePlacementRequest) -> Self {
            self.initial_placement = Some(initial_placement);
            self
        }

        fn with_size_hint(mut self, size_hint: SizeHint) -> Self {
            self.size_hint = size_hint;
            self
        }
    }

    impl Surface for TestSurface {
        fn id(&self) -> SurfaceId {
            self.id
        }

        fn surface_key(&self) -> CompactString {
            self.surface_key.clone()
        }

        fn size_hint(&self) -> SizeHint {
            self.size_hint
        }

        fn initial_placement(&self) -> Option<SurfacePlacementRequest> {
            self.initial_placement.clone()
        }

        fn view(&self, _ctx: &ViewContext<'_>) -> Element {
            Element::Empty
        }

        fn handle_event(&mut self, _event: SurfaceEvent, _ctx: &EventContext<'_>) -> Vec<Command> {
            vec![]
        }

        fn declared_slots(&self) -> &[SlotDeclaration] {
            &self.slots
        }
    }

    struct EventSurface {
        id: SurfaceId,
        surface_key: CompactString,
        command_flag: DirtyFlags,
    }

    impl EventSurface {
        fn new(
            id: SurfaceId,
            surface_key: impl Into<CompactString>,
            command_flag: DirtyFlags,
        ) -> Self {
            Self {
                id,
                surface_key: surface_key.into(),
                command_flag,
            }
        }
    }

    impl Surface for EventSurface {
        fn id(&self) -> SurfaceId {
            self.id
        }

        fn surface_key(&self) -> CompactString {
            self.surface_key.clone()
        }

        fn size_hint(&self) -> SizeHint {
            SizeHint::fill()
        }

        fn view(&self, _ctx: &ViewContext<'_>) -> Element {
            Element::Empty
        }

        fn handle_event(&mut self, _event: SurfaceEvent, _ctx: &EventContext<'_>) -> Vec<Command> {
            vec![Command::RequestRedraw(self.command_flag)]
        }

        fn on_state_changed(&mut self, _state: &AppState, _dirty: DirtyFlags) -> Vec<Command> {
            vec![Command::RequestRedraw(self.command_flag)]
        }
    }

    #[test]
    fn test_registry_register_and_get() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));
        reg.register(Box::new(StatusBarSurface::new()));
        assert_eq!(reg.surface_count(), 2);
        assert!(reg.get(SurfaceId::BUFFER).is_some());
        assert!(reg.get(SurfaceId::STATUS).is_some());
        assert!(reg.get(SurfaceId(99)).is_none());
        assert_eq!(
            reg.surface_id_by_key("kasane.buffer"),
            Some(SurfaceId::BUFFER)
        );
        assert_eq!(
            reg.descriptor(SurfaceId::BUFFER)
                .map(|descriptor| descriptor.surface_key.as_str()),
            Some("kasane.buffer")
        );
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
    fn test_registry_reject_duplicate_surface_id() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));
        let err = reg
            .try_register(Box::new(TestSurface::new(
                SurfaceId::BUFFER,
                "plugin.buffer-shadow",
                vec![],
            )))
            .unwrap_err();
        assert!(matches!(
            err,
            SurfaceRegistrationError::DuplicateSurfaceId {
                surface_id: SurfaceId::BUFFER,
                ..
            }
        ));
    }

    #[test]
    fn test_registry_reject_duplicate_surface_key() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));
        let err = reg
            .try_register(Box::new(TestSurface::new(
                SurfaceId(500),
                "kasane.buffer",
                vec![],
            )))
            .unwrap_err();
        assert_eq!(
            err,
            SurfaceRegistrationError::DuplicateSurfaceKey {
                surface_key: "kasane.buffer".into()
            }
        );
    }

    #[test]
    fn test_registry_reject_duplicate_declared_slot() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));
        let err = reg
            .try_register(Box::new(TestSurface::new(
                SurfaceId(501),
                "plugin.duplicate-slot",
                vec![SlotDeclaration::new(
                    "kasane.buffer.left",
                    SlotKind::LeftRail,
                )],
            )))
            .unwrap_err();
        assert_eq!(
            err,
            SurfaceRegistrationError::DuplicateDeclaredSlot {
                slot_name: "kasane.buffer.left".into(),
                existing_surface_key: "kasane.buffer".into(),
                new_surface_key: "plugin.duplicate-slot".into(),
            }
        );
    }

    #[test]
    fn test_registry_reject_duplicate_declared_slot_in_surface() {
        let mut reg = SurfaceRegistry::new();
        let err = reg
            .try_register(Box::new(TestSurface::new(
                SurfaceId(502),
                "plugin.bad-slots",
                vec![
                    SlotDeclaration::new("plugin.bad-slots.left", SlotKind::LeftRail),
                    SlotDeclaration::new("plugin.bad-slots.left", SlotKind::RightRail),
                ],
            )))
            .unwrap_err();
        assert_eq!(
            err,
            SurfaceRegistrationError::DuplicateDeclaredSlotInSurface {
                surface_key: "plugin.bad-slots".into(),
                slot_name: "plugin.bad-slots.left".into(),
            }
        );
    }

    #[test]
    fn test_try_register_for_owner_tracks_surface_owner_plugin() {
        let mut reg = SurfaceRegistry::new();
        let owner = PluginId("plugin.alpha".into());
        let surface_id = SurfaceId(620);
        reg.try_register_for_owner(
            Box::new(TestSurface::new(surface_id, "plugin.alpha.surface", vec![])),
            Some(owner.clone()),
        )
        .unwrap();

        assert_eq!(reg.surface_owner_plugin(surface_id), Some(&owner));
    }

    #[test]
    fn test_route_event_with_sources_preserves_focused_surface_owner() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));

        let owner = PluginId("plugin.focused".into());
        let surface_id = SurfaceId(621);
        reg.try_register_for_owner(
            Box::new(EventSurface::new(
                surface_id,
                "plugin.focused.surface",
                DirtyFlags::STATUS,
            )),
            Some(owner.clone()),
        )
        .unwrap();

        let mut dirty = DirtyFlags::empty();
        crate::workspace::dispatch_workspace_command(
            &mut reg,
            WorkspaceCommand::AddSurface {
                surface_id,
                placement: Placement::SplitFocused {
                    direction: SplitDirection::Vertical,
                    ratio: 0.5,
                },
            },
            &mut dirty,
        );
        reg.workspace_mut().focus(surface_id);

        let commands = reg.route_event_with_sources(
            SurfaceEvent::FocusGained,
            &crate::state::AppState::default(),
            Rect {
                x: 0,
                y: 0,
                w: 80,
                h: 24,
            },
        );

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].source_plugin.as_ref(), Some(&owner));
        assert!(matches!(
            commands[0].commands.as_slice(),
            [Command::RequestRedraw(DirtyFlags::STATUS)]
        ));
    }

    #[test]
    fn test_route_event_with_sources_preserves_owner_plugin_per_surface_on_resize() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));

        let owner_a = PluginId("plugin.alpha".into());
        let owner_b = PluginId("plugin.beta".into());
        let surface_a = SurfaceId(622);
        let surface_b = SurfaceId(623);
        reg.try_register_for_owner(
            Box::new(EventSurface::new(
                surface_a,
                "plugin.alpha.surface",
                DirtyFlags::STATUS,
            )),
            Some(owner_a.clone()),
        )
        .unwrap();
        reg.try_register_for_owner(
            Box::new(EventSurface::new(
                surface_b,
                "plugin.beta.surface",
                DirtyFlags::MENU,
            )),
            Some(owner_b.clone()),
        )
        .unwrap();

        let mut dirty = DirtyFlags::empty();
        crate::workspace::dispatch_workspace_command(
            &mut reg,
            WorkspaceCommand::AddSurface {
                surface_id: surface_a,
                placement: Placement::SplitFocused {
                    direction: SplitDirection::Vertical,
                    ratio: 0.5,
                },
            },
            &mut dirty,
        );
        crate::workspace::dispatch_workspace_command(
            &mut reg,
            WorkspaceCommand::AddSurface {
                surface_id: surface_b,
                placement: Placement::SplitFocused {
                    direction: SplitDirection::Horizontal,
                    ratio: 0.5,
                },
            },
            &mut dirty,
        );

        let commands = reg.route_event_with_sources(
            SurfaceEvent::Resize(Rect {
                x: 0,
                y: 0,
                w: 80,
                h: 24,
            }),
            &crate::state::AppState::default(),
            Rect {
                x: 0,
                y: 0,
                w: 80,
                h: 24,
            },
        );

        assert_eq!(commands.len(), 2);
        assert!(
            commands
                .iter()
                .any(|entry| entry.source_plugin.as_ref() == Some(&owner_a)
                    && matches!(
                        entry.commands.as_slice(),
                        [Command::RequestRedraw(DirtyFlags::STATUS)]
                    ))
        );
        assert!(
            commands
                .iter()
                .any(|entry| entry.source_plugin.as_ref() == Some(&owner_b)
                    && matches!(
                        entry.commands.as_slice(),
                        [Command::RequestRedraw(DirtyFlags::MENU)]
                    ))
        );
    }

    #[test]
    fn test_on_state_changed_with_sources_preserves_surface_owner() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));

        let owner = PluginId("plugin.stateful".into());
        let surface_id = SurfaceId(624);
        reg.try_register_for_owner(
            Box::new(EventSurface::new(
                surface_id,
                "plugin.stateful.surface",
                DirtyFlags::BUFFER,
            )),
            Some(owner.clone()),
        )
        .unwrap();

        let commands = reg
            .on_state_changed_with_sources(&crate::state::AppState::default(), DirtyFlags::BUFFER);

        assert!(commands.iter().any(|entry| {
            entry.source_plugin.as_ref() == Some(&owner)
                && matches!(
                    entry.commands.as_slice(),
                    [Command::RequestRedraw(DirtyFlags::BUFFER)]
                )
        }));
    }

    #[test]
    fn test_handle_workspace_divider_mouse_resizes_split() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));
        let right = SurfaceId(625);
        reg.register(Box::new(TestSurface::new(right, "plugin.right", vec![])));

        let mut dirty = DirtyFlags::empty();
        crate::workspace::dispatch_workspace_command(
            &mut reg,
            WorkspaceCommand::AddSurface {
                surface_id: right,
                placement: Placement::SplitFocused {
                    direction: SplitDirection::Vertical,
                    ratio: 0.5,
                },
            },
            &mut dirty,
        );

        let total = Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        };
        assert_eq!(
            reg.handle_workspace_divider_mouse(
                &MouseEvent {
                    kind: MouseEventKind::Press(MouseButton::Left),
                    line: 12,
                    column: 40,
                    modifiers: crate::input::Modifiers::empty(),
                },
                total,
            ),
            Some(DirtyFlags::empty())
        );
        assert_eq!(
            reg.handle_workspace_divider_mouse(
                &MouseEvent {
                    kind: MouseEventKind::Drag(MouseButton::Left),
                    line: 12,
                    column: 45,
                    modifiers: crate::input::Modifiers::empty(),
                },
                total,
            ),
            Some(DirtyFlags::ALL)
        );
        match reg.workspace().root() {
            crate::workspace::WorkspaceNode::Split { ratio, .. } => {
                let expected = 0.5 + 5.0 / 79.0;
                assert!((*ratio - expected).abs() < 0.001, "ratio={ratio}");
            }
            other => panic!("expected root split, got {other:?}"),
        }
        assert_eq!(
            reg.handle_workspace_divider_mouse(
                &MouseEvent {
                    kind: MouseEventKind::Release(MouseButton::Left),
                    line: 12,
                    column: 45,
                    modifiers: crate::input::Modifiers::empty(),
                },
                total,
            ),
            Some(DirtyFlags::empty())
        );
    }

    #[test]
    fn test_handle_workspace_divider_mouse_ignores_surface_hits() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));
        let total = Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        };
        assert_eq!(
            reg.handle_workspace_divider_mouse(
                &MouseEvent {
                    kind: MouseEventKind::Press(MouseButton::Left),
                    line: 2,
                    column: 2,
                    modifiers: crate::input::Modifiers::empty(),
                },
                total,
            ),
            None
        );
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
        // KakouneBufferSurface now delegates to the abstract/resolved surface path.
        assert!(!matches!(element, Element::Empty));
    }

    #[test]
    fn test_registry_compose_split_includes_explicit_divider_node() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));
        let right = SurfaceId(626);
        reg.register(Box::new(TestSurface::new(right, "plugin.right", vec![])));

        let mut dirty = DirtyFlags::empty();
        crate::workspace::dispatch_workspace_command(
            &mut reg,
            WorkspaceCommand::AddSurface {
                surface_id: right,
                placement: Placement::SplitFocused {
                    direction: SplitDirection::Vertical,
                    ratio: 0.5,
                },
            },
            &mut dirty,
        );

        let state = crate::state::AppState::default();
        let plugin_reg = crate::plugin::PluginRegistry::new();
        let total = Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        };
        let element = reg.compose_view(&state, &plugin_reg, total);
        match element {
            Element::Flex { gap, children, .. } => {
                assert_eq!(gap, 0);
                assert_eq!(children.len(), 3);
                assert_eq!(children[1].min_size, Some(1));
                assert_eq!(children[1].max_size, Some(1));
                match &children[1].element {
                    Element::Container { style, .. } => assert_eq!(
                        style,
                        &crate::element::Style::Token(crate::element::StyleToken::SPLIT_DIVIDER)
                    ),
                    other => panic!("expected divider container, got {other:?}"),
                }
            }
            other => panic!("expected split flex root, got {other:?}"),
        }
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

    #[test]
    fn test_apply_initial_placements_uses_descriptor_request_before_legacy() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));

        let anchor_id = SurfaceId(610);
        reg.register(Box::new(TestSurface::new(
            anchor_id,
            "plugin.anchor",
            vec![],
        )));
        let mut dirty = DirtyFlags::empty();
        crate::workspace::dispatch_workspace_command(
            &mut reg,
            WorkspaceCommand::AddSurface {
                surface_id: anchor_id,
                placement: Placement::SplitFocused {
                    direction: SplitDirection::Vertical,
                    ratio: 0.5,
                },
            },
            &mut dirty,
        );
        reg.workspace_mut().focus(anchor_id);

        let placed_id = SurfaceId(611);
        reg.register(Box::new(
            TestSurface::new(placed_id, "plugin.placed", vec![]).with_initial_placement(
                SurfacePlacementRequest::SplitFrom {
                    target_surface_key: "kasane.buffer".into(),
                    direction: SplitDirection::Horizontal,
                    ratio: 0.5,
                },
            ),
        ));

        let unresolved = reg.apply_initial_placements(
            &[placed_id],
            Some(&Placement::SplitFocused {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
            }),
            &mut dirty,
        );
        assert!(unresolved.is_empty());

        let rects = reg.workspace().compute_rects(Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        });
        let placed_rect = rects[&placed_id];
        assert_eq!(
            placed_rect,
            Rect {
                x: 0,
                y: 13,
                w: 40,
                h: 11,
            }
        );
    }

    #[test]
    fn test_apply_initial_placements_uses_legacy_fallback() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));

        let placed_id = SurfaceId(612);
        reg.register(Box::new(TestSurface::new(
            placed_id,
            "plugin.legacy-placement",
            vec![],
        )));

        let mut dirty = DirtyFlags::empty();
        let unresolved = reg.apply_initial_placements(
            &[placed_id],
            Some(&Placement::SplitFocused {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
            }),
            &mut dirty,
        );
        assert!(unresolved.is_empty());

        let rects = reg.workspace().compute_rects(Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        });
        assert_eq!(
            rects[&placed_id],
            Rect {
                x: 41,
                y: 0,
                w: 39,
                h: 24,
            }
        );
    }

    #[test]
    fn test_apply_initial_placements_reports_unresolved_keyed_request() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));

        let placed_id = SurfaceId(613);
        let request = SurfacePlacementRequest::SplitFrom {
            target_surface_key: "missing.surface".into(),
            direction: SplitDirection::Vertical,
            ratio: 0.5,
        };
        reg.register(Box::new(
            TestSurface::new(placed_id, "plugin.unresolved-placement", vec![])
                .with_initial_placement(request.clone()),
        ));

        let mut dirty = DirtyFlags::empty();
        let unresolved = reg.apply_initial_placements(&[placed_id], None, &mut dirty);
        assert_eq!(unresolved, vec![(placed_id, request)]);
        assert!(reg.workspace().root().find(placed_id).is_none());
    }

    #[test]
    fn test_apply_initial_placements_supports_tab_request() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));

        let placed_id = SurfaceId(614);
        reg.register(Box::new(
            TestSurface::new(placed_id, "plugin.tabbed", vec![])
                .with_initial_placement(SurfacePlacementRequest::Tab),
        ));

        let mut dirty = DirtyFlags::empty();
        let unresolved = reg.apply_initial_placements(&[placed_id], None, &mut dirty);
        assert!(unresolved.is_empty());
        assert_eq!(reg.workspace().focused(), placed_id);
        assert_eq!(reg.workspace().surface_count(), 2);

        let rects = reg.workspace().compute_rects(Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        });
        assert_eq!(rects.len(), 1);
        assert_eq!(
            rects[&placed_id],
            Rect {
                x: 0,
                y: 1,
                w: 80,
                h: 23,
            }
        );
    }

    #[test]
    fn test_apply_initial_placements_supports_tab_in_keyed_request() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));

        let placed_id = SurfaceId(617);
        reg.register(Box::new(
            TestSurface::new(placed_id, "plugin.tabbed-keyed", vec![]).with_initial_placement(
                SurfacePlacementRequest::TabIn {
                    target_surface_key: "kasane.buffer".into(),
                },
            ),
        ));

        let mut dirty = DirtyFlags::empty();
        let unresolved = reg.apply_initial_placements(&[placed_id], None, &mut dirty);
        assert!(unresolved.is_empty());
        assert_eq!(reg.workspace().focused(), placed_id);
        assert_eq!(reg.workspace().surface_count(), 2);

        let rects = reg.workspace().compute_rects(Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        });
        assert_eq!(rects.len(), 1);
        assert_eq!(
            rects[&placed_id],
            Rect {
                x: 0,
                y: 1,
                w: 80,
                h: 23,
            }
        );
    }

    #[test]
    fn test_apply_initial_placements_supports_dock_request() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));

        let placed_id = SurfaceId(615);
        reg.register(Box::new(
            TestSurface::new(placed_id, "plugin.left-dock", vec![])
                .with_initial_placement(SurfacePlacementRequest::Dock(DockPosition::Left)),
        ));

        let mut dirty = DirtyFlags::empty();
        let unresolved = reg.apply_initial_placements(&[placed_id], None, &mut dirty);
        assert!(unresolved.is_empty());

        let rects = reg.workspace().compute_rects(Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        });
        assert_eq!(
            rects[&placed_id],
            Rect {
                x: 0,
                y: 0,
                w: 20,
                h: 24,
            }
        );
        assert_eq!(
            rects[&SurfaceId::BUFFER],
            Rect {
                x: 21,
                y: 0,
                w: 59,
                h: 24,
            }
        );
    }

    #[test]
    fn test_apply_initial_placements_uses_size_hint_for_dock_ratio_when_total_known() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));

        let placed_id = SurfaceId(618);
        reg.register(Box::new(
            TestSurface::new(placed_id, "plugin.sized-left-dock", vec![])
                .with_size_hint(SizeHint::fixed(12, 8))
                .with_initial_placement(SurfacePlacementRequest::Dock(DockPosition::Left)),
        ));

        let mut dirty = DirtyFlags::empty();
        let unresolved = reg.apply_initial_placements_with_total(
            &[placed_id],
            None,
            &mut dirty,
            Some(Rect {
                x: 0,
                y: 0,
                w: 80,
                h: 24,
            }),
        );
        assert!(unresolved.is_empty());

        let rects = reg.workspace().compute_rects(Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        });
        assert_eq!(
            rects[&placed_id],
            Rect {
                x: 0,
                y: 0,
                w: 12,
                h: 24,
            }
        );
        assert_eq!(
            rects[&SurfaceId::BUFFER],
            Rect {
                x: 13,
                y: 0,
                w: 67,
                h: 24,
            }
        );
    }

    #[test]
    fn test_apply_initial_placements_supports_float_request() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));

        let placed_id = SurfaceId(616);
        let float_rect = Rect {
            x: 8,
            y: 4,
            w: 30,
            h: 10,
        };
        reg.register(Box::new(
            TestSurface::new(placed_id, "plugin.float", vec![])
                .with_initial_placement(SurfacePlacementRequest::Float { rect: float_rect }),
        ));

        let mut dirty = DirtyFlags::empty();
        let unresolved = reg.apply_initial_placements(&[placed_id], None, &mut dirty);
        assert!(unresolved.is_empty());

        let rects = reg.workspace().compute_rects(Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        });
        assert_eq!(
            rects[&SurfaceId::BUFFER],
            Rect {
                x: 0,
                y: 0,
                w: 80,
                h: 24,
            }
        );
        assert_eq!(rects[&placed_id], float_rect);
    }

    #[test]
    fn test_workspace_command_unfloat_retiles_surface() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(KakouneBufferSurface::new()));

        let placed_id = SurfaceId(618);
        reg.register(Box::new(TestSurface::new(
            placed_id,
            "plugin.float-roundtrip",
            vec![],
        )));

        let mut dirty = DirtyFlags::empty();
        crate::workspace::dispatch_workspace_command(
            &mut reg,
            WorkspaceCommand::AddSurface {
                surface_id: placed_id,
                placement: Placement::SplitFocused {
                    direction: SplitDirection::Vertical,
                    ratio: 0.5,
                },
            },
            &mut dirty,
        );
        crate::workspace::dispatch_workspace_command(
            &mut reg,
            WorkspaceCommand::Float {
                surface_id: placed_id,
                rect: Rect {
                    x: 8,
                    y: 4,
                    w: 30,
                    h: 10,
                },
            },
            &mut dirty,
        );
        crate::workspace::dispatch_workspace_command(
            &mut reg,
            WorkspaceCommand::Unfloat(placed_id),
            &mut dirty,
        );

        let rects = reg.workspace().compute_rects(Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        });
        assert_eq!(
            rects[&SurfaceId::BUFFER],
            Rect {
                x: 0,
                y: 0,
                w: 40,
                h: 24,
            }
        );
        assert_eq!(
            rects[&placed_id],
            Rect {
                x: 41,
                y: 0,
                w: 39,
                h: 24,
            }
        );
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
