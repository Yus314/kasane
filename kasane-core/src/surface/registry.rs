use std::collections::HashMap;

use compact_str::CompactString;

use crate::element::Element;
use crate::input::{MouseButton, MouseEventKind};
use crate::layout::{Rect, SplitDirection};
use crate::plugin::{Command, PluginId, PluginView};
use crate::state::{AppState, DirtyFlags};
use crate::workspace::{
    Placement, Workspace, WorkspaceCommand, WorkspaceDivider, WorkspaceDividerId,
};

use super::pane_map::PaneStates;
use super::resolve::{self, SurfaceComposeResult, SurfaceRenderOutcome, SurfaceRenderReport};
use super::{
    EventContext, SlotDeclaration, SourcedSurfaceCommands, Surface, SurfaceDescriptor,
    SurfaceEvent, SurfaceId, SurfacePlacementRequest, SurfaceRegistrationError, ViewContext,
};

pub(crate) struct RegisteredSurface {
    pub(crate) surface: Box<dyn Surface>,
    pub(crate) descriptor: SurfaceDescriptor,
    pub(crate) owner_plugin: Option<PluginId>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ActiveDividerDrag {
    divider_id: WorkspaceDividerId,
    direction: SplitDirection,
    start_main: u16,
    start_ratio: f32,
    available_main: u16,
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

    /// Remove every surface owned by a plugin from the registry, preserving workspace nodes.
    pub fn remove_owned_surfaces(&mut self, owner: &PluginId) -> Vec<SurfaceId> {
        let mut surface_ids: Vec<_> = self
            .surfaces
            .iter()
            .filter(|(_, entry)| entry.owner_plugin.as_ref() == Some(owner))
            .map(|(surface_id, _)| *surface_id)
            .collect();
        surface_ids.sort_by_key(|surface_id| surface_id.0);
        for surface_id in &surface_ids {
            let _ = self.remove(*surface_id);
        }
        surface_ids
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

    /// Returns whether the workspace tree currently contains the given surface id.
    pub fn workspace_contains(&self, surface_id: SurfaceId) -> bool {
        self.workspace.root().find(surface_id).is_some()
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
        mouse: &crate::input::MouseEvent,
        total: Rect,
    ) -> Option<DirtyFlags> {
        let main_coord = |direction: SplitDirection, mouse: &crate::input::MouseEvent| -> u16 {
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
        pane_states: Option<&PaneStates<'_>>,
        plugin_registry: &PluginView<'_>,
        rect: Rect,
        focused: bool,
    ) -> SurfaceRenderOutcome {
        let pane_state = pane_states
            .and_then(|ps| ps.state_for_surface(entry.descriptor.surface_id))
            .unwrap_or(state);
        let ctx = ViewContext {
            state: pane_state,
            global_state: state,
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
    /// - Split -> Flex (Row or Column)
    /// - Tabs -> active tab only (tab bar rendered separately)
    /// - Float -> Stack with overlays
    /// - Leaf -> direct Surface::view() output
    pub fn compose_view(
        &self,
        state: &AppState,
        plugin_registry: &PluginView<'_>,
        total: Rect,
    ) -> Element {
        self.compose_base_result(state, None, plugin_registry, total)
            .base
            .unwrap_or(Element::Empty)
    }

    /// Compose the full UI: workspace content + status bar + overlays.
    ///
    /// This is the surface-based base composition path used by `view()`. It:
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
        plugin_registry: &PluginView<'_>,
        total: Rect,
    ) -> Element {
        use crate::render::view;
        let base = self
            .compose_base_result(state, None, plugin_registry, total)
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
        pane_states: Option<&PaneStates<'_>>,
        plugin_registry: &PluginView<'_>,
        total: Rect,
    ) -> SurfaceComposeResult {
        use crate::element::FlexChild;
        let rects = self.workspace.compute_rects(total);
        let (workspace_content, mut surface_reports) = self.compose_node_with_reports(
            self.workspace.root(),
            state,
            pane_states,
            plugin_registry,
            &rects,
        );

        // In multi-pane mode, the status bar should reflect the focused pane's
        // content (prompt, mode line, etc.), not the primary session's.
        let focused = self.workspace.focused();
        let status_state = pane_states
            .and_then(|ps| ps.state_for_surface_or_focused(SurfaceId::STATUS, focused))
            .unwrap_or(state);
        let status_bar = self.surfaces.get(&SurfaceId::STATUS).map(|entry| {
            let ctx = ViewContext {
                state: status_state,
                global_state: state,
                rect: total,
                focused: focused == SurfaceId::STATUS,
                registry: plugin_registry,
                surface_id: entry.descriptor.surface_id,
            };
            let abstract_root = entry.surface.view(&ctx);
            let outcome = resolve::resolve_surface_tree(
                &entry.descriptor,
                abstract_root,
                state,
                plugin_registry,
                total,
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
    pub fn compose_view_sections(
        &self,
        state: &AppState,
        pane_states: Option<&PaneStates<'_>>,
        plugin_registry: &PluginView<'_>,
        total: Rect,
    ) -> crate::render::view::ViewSections {
        use crate::render::view;

        let base_result = self.compose_base_result(state, pane_states, plugin_registry, total);
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

        let display_map = plugin_registry.collect_display_map(state);
        let focused = self.workspace.focused();
        let focused_pane_rect = self.workspace.compute_rects(total).get(&focused).copied();
        let focused_pane_state = pane_states
            .and_then(|ps| ps.state_for_surface(focused))
            .map(|s| Box::new(s.clone()));
        view::ViewSections {
            base: base_result.base.unwrap_or(Element::Empty),
            menu_overlay,
            info_overlays,
            plugin_overlays,
            surface_reports: base_result.surface_reports,
            display_map,
            focused_pane_rect,
            focused_pane_state,
        }
    }

    fn compose_node_with_reports(
        &self,
        node: &crate::workspace::WorkspaceNode,
        state: &AppState,
        pane_states: Option<&PaneStates<'_>>,
        plugin_registry: &PluginView<'_>,
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
                        pane_states,
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
                    crate::layout::SplitDirection::Vertical => crate::element::Direction::Row,
                    crate::layout::SplitDirection::Horizontal => crate::element::Direction::Column,
                };
                let (first_elem, mut first_reports) = self.compose_node_with_reports(
                    first,
                    state,
                    pane_states,
                    plugin_registry,
                    rects,
                );
                let (second_elem, second_reports) = self.compose_node_with_reports(
                    second,
                    state,
                    pane_states,
                    plugin_registry,
                    rects,
                );
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
                    self.compose_node_with_reports(
                        active_tab,
                        state,
                        pane_states,
                        plugin_registry,
                        rects,
                    )
                } else {
                    (Element::Empty, vec![])
                }
            }
            WorkspaceNode::Float { base, floating } => {
                let (base_elem, mut surface_reports) = self.compose_node_with_reports(
                    base,
                    state,
                    pane_states,
                    plugin_registry,
                    rects,
                );
                let mut overlays = Vec::new();
                for entry in floating {
                    let (overlay_elem, overlay_reports) = self.compose_node_with_reports(
                        &entry.node,
                        state,
                        pane_states,
                        plugin_registry,
                        rects,
                    );
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
                self.register(Box::new(super::menu::MenuSurface));
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
                self.register(Box::new(super::info::InfoSurface::new(i)));
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
