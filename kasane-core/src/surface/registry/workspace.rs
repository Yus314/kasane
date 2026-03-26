//! Workspace delegation and placement.

use super::*;

impl SurfaceRegistry {
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
}
