//! Workspace: generalized layout tree for Surface regions.
//!
//! Replaces and extends the pane system by using [`SurfaceId`]
//! instead of `PaneId` as the leaf identifier. This allows both core
//! components and plugin-owned surfaces to participate in the layout tree.

mod node;
pub use node::*;

use std::collections::HashMap;

use crate::layout::{Rect, SplitDirection};
use crate::state::DirtyFlags;
use crate::surface::{SurfaceId, SurfaceRegistry};

/// Direction for focus navigation between surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusDirection {
    Next,
    Prev,
    Left,
    Right,
    Up,
    Down,
}

/// Position for docking a surface in a well-known area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockPosition {
    Left,
    Right,
    Bottom,
    Panel,
}

/// Describes where a new surface should be placed in the workspace.
#[derive(Debug, Clone)]
pub enum Placement {
    /// Split the focused surface.
    SplitFocused {
        direction: SplitDirection,
        ratio: f32,
    },
    /// Split from a specific surface.
    SplitFrom {
        target: SurfaceId,
        direction: SplitDirection,
        ratio: f32,
    },
    /// Add as a new tab in the focused tab group.
    Tab,
    /// Add as a new tab in a specific surface's tab group.
    TabIn { target: SurfaceId },
    /// Dock in a well-known area.
    Dock(DockPosition),
    /// Float at a specific position.
    Float { rect: Rect },
}

/// Commands that manipulate the workspace layout.
#[derive(Debug)]
pub enum WorkspaceCommand {
    /// Add a surface to the workspace at a specified placement.
    AddSurface {
        surface_id: SurfaceId,
        placement: Placement,
    },
    /// Remove a surface from the workspace.
    RemoveSurface(SurfaceId),
    /// Focus a specific surface.
    Focus(SurfaceId),
    /// Move focus in a direction.
    FocusDirection(FocusDirection),
    /// Resize the focused split divider by delta (-1.0..1.0).
    Resize { delta: f32 },
    /// Resize the focused split divider, but only along the given axis.
    ResizeDirection {
        direction: SplitDirection,
        delta: f32,
    },
    /// Swap two surfaces.
    Swap(SurfaceId, SurfaceId),
    /// Make a tiled surface floating.
    Float { surface_id: SurfaceId, rect: Rect },
    /// Return a floating surface to the tiled layout.
    Unfloat(SurfaceId),
}

/// Dispatch a workspace command to the SurfaceRegistry.
///
/// Shared implementation used by both TUI and GUI event loops.
pub fn dispatch_workspace_command(
    surface_registry: &mut SurfaceRegistry,
    cmd: WorkspaceCommand,
    dirty: &mut DirtyFlags,
) {
    dispatch_workspace_command_with_total(surface_registry, cmd, dirty, None);
}

/// Dispatch a workspace command with an optional workspace layout rect.
///
/// Commands such as directional focus use `total` to resolve visible surface
/// geometry. Callers that do not have a current layout rect may pass `None`,
/// in which case geometry-dependent commands are a no-op.
pub fn dispatch_workspace_command_with_total(
    surface_registry: &mut SurfaceRegistry,
    cmd: WorkspaceCommand,
    dirty: &mut DirtyFlags,
    total: Option<Rect>,
) {
    fn dock_ratio_for_surface(
        surface_registry: &SurfaceRegistry,
        surface_id: SurfaceId,
        position: DockPosition,
        total: Option<Rect>,
    ) -> f32 {
        let default = match position {
            DockPosition::Left | DockPosition::Right => 0.25,
            DockPosition::Bottom | DockPosition::Panel => 0.20,
        };

        let Some(total) = total else {
            return default;
        };
        let Some(surface) = surface_registry.get(surface_id) else {
            return default;
        };
        let hint = surface.size_hint();
        let (preferred, min, available) = match position {
            DockPosition::Left | DockPosition::Right => (
                hint.preferred_width,
                hint.min_width,
                total.w.saturating_sub(1),
            ),
            DockPosition::Bottom | DockPosition::Panel => (
                hint.preferred_height,
                hint.min_height,
                total.h.saturating_sub(1),
            ),
        };

        if available == 0 {
            return default;
        }

        let desired = preferred.or((hint.flex == 0.0).then_some(min));
        let Some(desired) = desired else {
            return default;
        };

        (desired.min(available) as f32 / available as f32).clamp(0.05, 0.95)
    }

    fn surface_label(surface_registry: &SurfaceRegistry, surface_id: SurfaceId) -> String {
        surface_registry
            .descriptor(surface_id)
            .map(|descriptor| descriptor.surface_key.to_string())
            .unwrap_or_else(|| format!("surface-{}", surface_id.0))
    }

    match cmd {
        WorkspaceCommand::AddSurface {
            surface_id,
            placement,
        } => {
            match placement {
                Placement::SplitFocused { direction, ratio } => {
                    let ws = surface_registry.workspace_mut();
                    let focused = ws.focused();
                    ws.root_mut().split(focused, direction, ratio, surface_id);
                }
                Placement::SplitFrom {
                    target,
                    direction,
                    ratio,
                } => {
                    let ws = surface_registry.workspace_mut();
                    ws.root_mut().split(target, direction, ratio, surface_id);
                }
                Placement::Tab => {
                    let target = surface_registry.workspace().focused();
                    let target_label = surface_label(surface_registry, target);
                    let new_label = surface_label(surface_registry, surface_id);
                    let ws = surface_registry.workspace_mut();
                    ws.add_tab(target, surface_id, &target_label, &new_label);
                }
                Placement::TabIn { target } => {
                    let target_label = surface_label(surface_registry, target);
                    let new_label = surface_label(surface_registry, surface_id);
                    let ws = surface_registry.workspace_mut();
                    ws.add_tab(target, surface_id, &target_label, &new_label);
                }
                Placement::Dock(position) => {
                    let ratio =
                        dock_ratio_for_surface(surface_registry, surface_id, position, total);
                    let ws = surface_registry.workspace_mut();
                    ws.dock_surface(surface_id, position, ratio);
                }
                Placement::Float { rect } => {
                    let ws = surface_registry.workspace_mut();
                    ws.add_floating(surface_id, rect);
                }
            }
            *dirty |= DirtyFlags::ALL;
        }
        WorkspaceCommand::RemoveSurface(id) => {
            surface_registry.workspace_mut().close(id);
            *dirty |= DirtyFlags::ALL;
        }
        WorkspaceCommand::Focus(id) => {
            surface_registry.workspace_mut().focus(id);
            *dirty |= DirtyFlags::ALL;
        }
        WorkspaceCommand::FocusDirection(direction) => {
            if let Some(total) = total
                && surface_registry
                    .workspace_mut()
                    .focus_direction(direction, total)
                    .is_some()
            {
                *dirty |= DirtyFlags::ALL;
            }
        }
        WorkspaceCommand::Swap(a, b) => {
            if surface_registry.workspace_mut().swap_surfaces(a, b) {
                *dirty |= DirtyFlags::ALL;
            }
        }
        WorkspaceCommand::Float { surface_id, rect } => {
            if surface_registry
                .workspace_mut()
                .float_surface(surface_id, rect)
            {
                *dirty |= DirtyFlags::ALL;
            }
        }
        WorkspaceCommand::Unfloat(surface_id) => {
            if surface_registry.workspace_mut().unfloat_surface(
                surface_id,
                SplitDirection::Vertical,
                0.5,
            ) {
                *dirty |= DirtyFlags::ALL;
            }
        }
        WorkspaceCommand::Resize { delta } => {
            if surface_registry.workspace_mut().resize_focused(delta) {
                *dirty |= DirtyFlags::ALL;
            }
        }
        WorkspaceCommand::ResizeDirection { direction, delta } => {
            if surface_registry
                .workspace_mut()
                .resize_direction(direction, delta)
            {
                *dirty |= DirtyFlags::ALL;
            }
        }
    }
}

/// Manages the workspace layout tree and focus state.
pub struct Workspace {
    root: WorkspaceNode,
    focused: SurfaceId,
    focus_history: Vec<SurfaceId>,
    next_id: u32,
}

impl Workspace {
    /// Create a new Workspace with a single root surface.
    pub fn new(root_surface: SurfaceId) -> Self {
        Workspace {
            root: WorkspaceNode::leaf(root_surface),
            focused: root_surface,
            focus_history: Vec::new(),
            next_id: SurfaceId::PLUGIN_BASE,
        }
    }

    /// Get the root node of the workspace tree.
    pub fn root(&self) -> &WorkspaceNode {
        &self.root
    }

    /// Get a mutable reference to the root node.
    pub fn root_mut(&mut self) -> &mut WorkspaceNode {
        &mut self.root
    }

    /// Get the currently focused surface id.
    pub fn focused(&self) -> SurfaceId {
        self.focused
    }

    /// Get the focus history stack.
    pub fn focus_history(&self) -> &[SurfaceId] {
        &self.focus_history
    }

    /// Total number of leaf surfaces.
    pub fn surface_count(&self) -> usize {
        self.root.leaf_count()
    }

    /// Allocate the next available SurfaceId.
    pub fn next_surface_id(&mut self) -> SurfaceId {
        let id = SurfaceId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Split the focused surface, returning the new surface's id.
    pub fn split_focused(&mut self, direction: SplitDirection, ratio: f32) -> SurfaceId {
        let new_id = self.next_surface_id();
        self.root.split(self.focused, direction, ratio, new_id);
        new_id
    }

    /// Add a new surface as a tab in the tab group containing `target`.
    pub fn add_tab(
        &mut self,
        target: SurfaceId,
        new_id: SurfaceId,
        target_label: &str,
        new_label: &str,
    ) -> bool {
        let added = self.root.add_tab(target, new_id, target_label, new_label);
        if added {
            self.focus(new_id);
        }
        added
    }

    /// Wrap the current root with a docked surface.
    pub fn dock_surface(&mut self, surface_id: SurfaceId, placement: DockPosition, ratio: f32) {
        let clamped_ratio = ratio.clamp(0.05, 0.95);
        let old_root = std::mem::replace(&mut self.root, WorkspaceNode::leaf(surface_id));
        let dock_leaf = WorkspaceNode::leaf(surface_id);
        self.root = match placement {
            DockPosition::Left => WorkspaceNode::Split {
                direction: SplitDirection::Vertical,
                ratio: clamped_ratio,
                first: Box::new(dock_leaf),
                second: Box::new(old_root),
            },
            DockPosition::Right => WorkspaceNode::Split {
                direction: SplitDirection::Vertical,
                ratio: 1.0 - clamped_ratio,
                first: Box::new(old_root),
                second: Box::new(dock_leaf),
            },
            DockPosition::Bottom | DockPosition::Panel => WorkspaceNode::Split {
                direction: SplitDirection::Horizontal,
                ratio: 1.0 - clamped_ratio,
                first: Box::new(old_root),
                second: Box::new(dock_leaf),
            },
        };
    }

    /// Add a floating surface to the workspace root.
    pub fn add_floating(&mut self, surface_id: SurfaceId, rect: Rect) {
        let floating_entry = FloatingEntry {
            node: WorkspaceNode::leaf(surface_id),
            rect,
            z_order: match &self.root {
                WorkspaceNode::Float { floating, .. } => {
                    floating
                        .iter()
                        .map(|entry| entry.z_order)
                        .max()
                        .unwrap_or(0)
                        + 1
                }
                _ => 0,
            },
            restore: None,
        };

        match &mut self.root {
            WorkspaceNode::Float { floating, .. } => floating.push(floating_entry),
            _ => {
                let old_root = std::mem::replace(&mut self.root, WorkspaceNode::leaf(surface_id));
                self.root = WorkspaceNode::Float {
                    base: Box::new(old_root),
                    floating: vec![floating_entry],
                };
            }
        }
    }

    /// Move an existing tiled surface into the floating layer.
    pub fn float_surface(&mut self, surface_id: SurfaceId, rect: Rect) -> bool {
        match &mut self.root {
            WorkspaceNode::Float { base, floating } => {
                if let Some(entry) = floating.iter_mut().find(|entry| {
                    matches!(&entry.node, WorkspaceNode::Leaf { surface_id: id } if *id == surface_id)
                }) {
                    entry.rect = rect;
                    return true;
                }
                if base.leaf_count() <= 1 {
                    return false;
                }
                let restore = base.capture_restore_placement(surface_id);
                if !base.remove(surface_id) {
                    return false;
                }
                let z_order = floating
                    .iter()
                    .map(|entry| entry.z_order)
                    .max()
                    .unwrap_or(0)
                    + 1;
                floating.push(FloatingEntry {
                    node: WorkspaceNode::leaf(surface_id),
                    rect,
                    z_order,
                    restore,
                });
                true
            }
            _ => {
                if self.root.leaf_count() <= 1 {
                    return false;
                }
                let restore = self.root.capture_restore_placement(surface_id);
                let mut old_root =
                    std::mem::replace(&mut self.root, WorkspaceNode::leaf(surface_id));
                if !old_root.remove(surface_id) {
                    self.root = old_root;
                    return false;
                }
                self.root = WorkspaceNode::Float {
                    base: Box::new(old_root),
                    floating: vec![FloatingEntry {
                        node: WorkspaceNode::leaf(surface_id),
                        rect,
                        z_order: 0,
                        restore,
                    }],
                };
                true
            }
        }
    }

    /// Move a floating surface back into the tiled layout by splitting the
    /// current tiled anchor.
    pub fn unfloat_surface(
        &mut self,
        surface_id: SurfaceId,
        direction: SplitDirection,
        ratio: f32,
    ) -> bool {
        let WorkspaceNode::Float { base, floating } = &mut self.root else {
            return false;
        };

        let Some(pos) = floating.iter().position(|entry| {
            matches!(&entry.node, WorkspaceNode::Leaf { surface_id: id } if *id == surface_id)
        }) else {
            return false;
        };
        let entry = floating.remove(pos);

        let restored = entry
            .restore
            .filter(|restore| base.find(restore.anchor).is_some())
            .map(|restore| {
                base.split_with_side(
                    restore.anchor,
                    restore.direction,
                    restore.ratio,
                    surface_id,
                    restore.side,
                )
            })
            .unwrap_or(false);

        if !restored {
            let anchor = if base.find(self.focused).is_some() {
                self.focused
            } else if let Some(anchor) = base.collect_ids().first().copied() {
                anchor
            } else {
                return false;
            };

            if !base.split(anchor, direction, ratio, surface_id) {
                return false;
            }
        }

        if self.focused != surface_id {
            self.focus_history.push(self.focused);
            self.focused = surface_id;
        }
        true
    }

    /// Focus a specific surface. Pushes the previous focus onto the history stack.
    pub fn focus(&mut self, target: SurfaceId) {
        if target == self.focused {
            return;
        }
        if self.root.find(target).is_some() {
            self.focus_history.push(self.focused);
            self.focused = target;
        }
    }

    /// Resize the nearest split containing the focused surface.
    ///
    /// Positive `delta` grows the focused surface's subtree.
    pub fn resize_focused(&mut self, delta: f32) -> bool {
        if delta == 0.0 {
            return false;
        }
        self.root.resize_target(self.focused, delta)
    }

    /// Resize the nearest split of the given direction containing the focused surface.
    ///
    /// Positive `delta` grows the focused surface's subtree.
    /// Returns `false` if no matching split was found.
    pub fn resize_direction(&mut self, direction: SplitDirection, delta: f32) -> bool {
        if delta == 0.0 {
            return false;
        }
        self.root
            .resize_direction_target(self.focused, direction, delta)
    }

    /// Swap two surfaces in the workspace layout.
    pub fn swap_surfaces(&mut self, a: SurfaceId, b: SurfaceId) -> bool {
        if a == b {
            return false;
        }
        if self.root.find(a).is_none() || self.root.find(b).is_none() {
            return false;
        }
        self.root.swap_leaf_ids(a, b);
        true
    }

    /// Move focus based on visible workspace geometry.
    pub fn focus_direction(&mut self, direction: FocusDirection, total: Rect) -> Option<SurfaceId> {
        let visible = self.visible_surfaces(total);
        if visible.is_empty() {
            return None;
        }

        match direction {
            FocusDirection::Next | FocusDirection::Prev => {
                let len = visible.len();
                let current_index = visible
                    .iter()
                    .position(|(surface_id, _)| *surface_id == self.focused)
                    .unwrap_or(0);
                let next_index = match direction {
                    FocusDirection::Next => (current_index + 1) % len,
                    FocusDirection::Prev => {
                        if current_index == 0 {
                            len - 1
                        } else {
                            current_index - 1
                        }
                    }
                    _ => unreachable!(),
                };
                let target = visible[next_index].0;
                if target == self.focused {
                    None
                } else {
                    let previous = self.focused;
                    self.focus(target);
                    if self.focused != previous {
                        Some(target)
                    } else {
                        None
                    }
                }
            }
            FocusDirection::Left
            | FocusDirection::Right
            | FocusDirection::Up
            | FocusDirection::Down => {
                let current_rect = visible.iter().find_map(|(surface_id, rect)| {
                    (*surface_id == self.focused).then_some(*rect)
                })?;

                let target = visible
                    .iter()
                    .filter(|(surface_id, _)| *surface_id != self.focused)
                    .filter_map(|(surface_id, rect)| {
                        focus_direction_score(direction, current_rect, *rect)
                            .map(|score| (score, *surface_id))
                    })
                    .min_by_key(|(score, surface_id)| (*score, surface_id.0))
                    .map(|(_, surface_id)| surface_id)?;

                let previous = self.focused;
                self.focus(target);
                if self.focused != previous {
                    Some(target)
                } else {
                    None
                }
            }
        }
    }

    /// Return to the previously focused surface.
    pub fn focus_previous(&mut self) -> Option<SurfaceId> {
        while let Some(prev) = self.focus_history.pop() {
            if self.root.find(prev).is_some() {
                let old = self.focused;
                self.focused = prev;
                return Some(old);
            }
        }
        None
    }

    /// Close a surface by removing it from the tree.
    /// Returns `true` if removal succeeded (cannot remove the last surface).
    pub fn close(&mut self, target: SurfaceId) -> bool {
        if self.root.leaf_count() <= 1 {
            return false;
        }
        if !self.root.remove(target) {
            return false;
        }
        if self.focused == target && self.focus_previous().is_none() {
            let ids = self.root.collect_ids();
            if let Some(&first) = ids.first() {
                self.focused = first;
            }
        }
        self.focus_history.retain(|id| *id != target);
        true
    }

    /// Compute screen rectangles for all surfaces.
    pub fn compute_rects(&self, total: Rect) -> HashMap<SurfaceId, Rect> {
        self.root.compute_rects(total)
    }

    /// Compute visible split divider geometry for the current workspace.
    pub fn compute_dividers(&self, total: Rect) -> Vec<WorkspaceDivider> {
        let mut dividers = Vec::new();
        let mut next_id = 0;
        self.root
            .compute_dividers_inner(total, &mut next_id, &mut dividers);
        dividers
    }

    /// Find which surface contains the given screen coordinates.
    pub fn surface_at(&self, x: u16, y: u16, total: Rect) -> Option<SurfaceId> {
        let rects = self.compute_rects(total);
        rects.into_iter().find_map(|(id, rect)| {
            if x >= rect.x && x < rect.x + rect.w && y >= rect.y && y < rect.y + rect.h {
                Some(id)
            } else {
                None
            }
        })
    }

    /// Find the visible split divider under the given screen coordinates.
    pub fn divider_at(&self, x: u16, y: u16, total: Rect) -> Option<WorkspaceDivider> {
        self.compute_dividers(total).into_iter().find(|divider| {
            x >= divider.rect.x
                && x < divider.rect.x + divider.rect.w
                && y >= divider.rect.y
                && y < divider.rect.y + divider.rect.h
        })
    }

    /// Set the ratio of a specific visible divider.
    pub fn set_divider_ratio(&mut self, divider_id: WorkspaceDividerId, ratio: f32) -> bool {
        let mut next_id = 0;
        self.root.set_divider_ratio(divider_id, ratio, &mut next_id)
    }

    /// Build a read-only query handle for inspecting the workspace.
    pub fn query(&self, total: Rect) -> WorkspaceQuery<'_> {
        WorkspaceQuery {
            workspace: self,
            rects: self.compute_rects(total),
            surface_keys: HashMap::new(),
        }
    }

    /// Build a read-only query handle with surface key mappings.
    pub fn query_with_keys(
        &self,
        total: Rect,
        surface_keys: HashMap<SurfaceId, compact_str::CompactString>,
    ) -> WorkspaceQuery<'_> {
        WorkspaceQuery {
            workspace: self,
            rects: self.compute_rects(total),
            surface_keys,
        }
    }

    fn visible_surfaces(&self, total: Rect) -> Vec<(SurfaceId, Rect)> {
        let mut visible: Vec<_> = self.compute_rects(total).into_iter().collect();
        visible.sort_by_key(|(surface_id, rect)| (rect.y, rect.x, rect.h, rect.w, surface_id.0));
        visible
    }
}

fn focus_direction_score(
    direction: FocusDirection,
    current: Rect,
    candidate: Rect,
) -> Option<(u16, u8, u16, u16)> {
    let current_left = current.x;
    let current_right = current.x.saturating_add(current.w);
    let current_top = current.y;
    let current_bottom = current.y.saturating_add(current.h);
    let candidate_left = candidate.x;
    let candidate_right = candidate.x.saturating_add(candidate.w);
    let candidate_top = candidate.y;
    let candidate_bottom = candidate.y.saturating_add(candidate.h);

    let current_center_x = current.x as i32 * 2 + current.w as i32;
    let current_center_y = current.y as i32 * 2 + current.h as i32;
    let candidate_center_x = candidate.x as i32 * 2 + candidate.w as i32;
    let candidate_center_y = candidate.y as i32 * 2 + candidate.h as i32;

    let horizontal_gap = range_gap(current_left, current_right, candidate_left, candidate_right);
    let vertical_gap = range_gap(current_top, current_bottom, candidate_top, candidate_bottom);

    match direction {
        FocusDirection::Left => {
            if candidate_center_x >= current_center_x {
                return None;
            }
            Some((
                current_center_x
                    .saturating_sub(candidate_center_x)
                    .try_into()
                    .unwrap_or(u16::MAX),
                u8::from(vertical_gap != 0),
                vertical_gap,
                horizontal_gap,
            ))
        }
        FocusDirection::Right => {
            if candidate_center_x <= current_center_x {
                return None;
            }
            Some((
                candidate_center_x
                    .saturating_sub(current_center_x)
                    .try_into()
                    .unwrap_or(u16::MAX),
                u8::from(vertical_gap != 0),
                vertical_gap,
                horizontal_gap,
            ))
        }
        FocusDirection::Up => {
            if candidate_center_y >= current_center_y {
                return None;
            }
            Some((
                current_center_y
                    .saturating_sub(candidate_center_y)
                    .try_into()
                    .unwrap_or(u16::MAX),
                u8::from(horizontal_gap != 0),
                horizontal_gap,
                vertical_gap,
            ))
        }
        FocusDirection::Down => {
            if candidate_center_y <= current_center_y {
                return None;
            }
            Some((
                candidate_center_y
                    .saturating_sub(current_center_y)
                    .try_into()
                    .unwrap_or(u16::MAX),
                u8::from(horizontal_gap != 0),
                horizontal_gap,
                vertical_gap,
            ))
        }
        FocusDirection::Next | FocusDirection::Prev => None,
    }
}

fn range_gap(start_a: u16, end_a: u16, start_b: u16, end_b: u16) -> u16 {
    if end_a <= start_b {
        start_b - end_a
    } else {
        start_a.saturating_sub(end_b)
    }
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new(SurfaceId::BUFFER)
    }
}

/// Read-only view of the workspace layout, available to plugins.
pub struct WorkspaceQuery<'a> {
    workspace: &'a Workspace,
    rects: HashMap<SurfaceId, Rect>,
    surface_keys: HashMap<SurfaceId, compact_str::CompactString>,
}

impl WorkspaceQuery<'_> {
    /// All surface IDs in the workspace.
    pub fn surfaces(&self) -> Vec<SurfaceId> {
        self.workspace.root.collect_ids()
    }

    /// Get the rectangle for a specific surface.
    pub fn rect_of(&self, id: SurfaceId) -> Option<Rect> {
        self.rects.get(&id).copied()
    }

    /// Get the currently focused surface.
    pub fn focused(&self) -> SurfaceId {
        self.workspace.focused
    }

    /// Get the total number of surfaces.
    pub fn surface_count(&self) -> usize {
        self.workspace.surface_count()
    }

    /// Get the surface key for a specific surface ID.
    pub fn surface_key_of(&self, id: SurfaceId) -> Option<&str> {
        self.surface_keys.get(&id).map(|s| s.as_str())
    }

    /// Find a surface ID by its surface key string.
    pub fn surface_key_of_str(&self, key: &str) -> Option<SurfaceId> {
        self.surface_keys
            .iter()
            .find(|(_, v)| v.as_str() == key)
            .map(|(id, _)| *id)
    }
}

#[cfg(test)]
mod tests;
