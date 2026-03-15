//! Workspace: generalized layout tree for Surface regions.
//!
//! Replaces and extends the pane system (`pane.rs`) by using [`SurfaceId`]
//! instead of `PaneId` as the leaf identifier. This allows both core
//! components and plugin-owned surfaces to participate in the layout tree.
//!
//! The pane module remains for backward compatibility (type aliases).

use std::collections::HashMap;

use crate::layout::{Rect, SplitDirection};
use crate::state::DirtyFlags;
use crate::surface::{SurfaceId, SurfaceRegistry};

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

/// A floating surface entry in the workspace.
#[derive(Debug, Clone)]
pub struct FloatingEntry {
    pub node: WorkspaceNode,
    pub rect: Rect,
    pub z_order: u16,
    pub restore: Option<RestorePlacement>,
}

/// Best-effort placement to restore a tiled surface after it was floated.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RestorePlacement {
    pub anchor: SurfaceId,
    pub direction: SplitDirection,
    pub ratio: f32,
    pub side: SplitSide,
}

/// Which side of a split a surface occupies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitSide {
    First,
    Second,
}

/// Frame-local identifier for a visible workspace split divider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorkspaceDividerId(pub u32);

/// Geometry and metadata for a visible workspace split divider.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorkspaceDivider {
    pub id: WorkspaceDividerId,
    pub rect: Rect,
    pub direction: SplitDirection,
    /// Current ratio assigned to the first subtree of the split.
    pub ratio: f32,
    /// Available main-axis span of the split, excluding the 1-cell divider.
    pub available_main: u16,
}

/// A node in the workspace layout tree.
#[derive(Debug, Clone)]
pub enum WorkspaceNode {
    /// A single surface.
    Leaf { surface_id: SurfaceId },
    /// A split into two sub-trees.
    Split {
        direction: SplitDirection,
        /// Ratio allocated to the first child (0.0..1.0).
        ratio: f32,
        first: Box<WorkspaceNode>,
        second: Box<WorkspaceNode>,
    },
    /// Tab group: multiple surface trees sharing the same screen area.
    Tabs {
        tabs: Vec<WorkspaceNode>,
        active: usize,
        labels: Vec<String>,
    },
    /// A base node with floating overlays.
    Float {
        base: Box<WorkspaceNode>,
        floating: Vec<FloatingEntry>,
    },
}

impl WorkspaceNode {
    /// Create a leaf node.
    pub fn leaf(surface_id: SurfaceId) -> Self {
        WorkspaceNode::Leaf { surface_id }
    }

    /// Find a node containing the given SurfaceId.
    pub fn find(&self, target: SurfaceId) -> Option<&WorkspaceNode> {
        match self {
            WorkspaceNode::Leaf { surface_id } if *surface_id == target => Some(self),
            WorkspaceNode::Leaf { .. } => None,
            WorkspaceNode::Split { first, second, .. } => {
                first.find(target).or_else(|| second.find(target))
            }
            WorkspaceNode::Tabs { tabs, .. } => tabs.iter().find_map(|tab| tab.find(target)),
            WorkspaceNode::Float { base, floating } => base
                .find(target)
                .or_else(|| floating.iter().find_map(|entry| entry.node.find(target))),
        }
    }

    /// Collect all leaf SurfaceIds in this subtree (depth-first order).
    pub fn collect_ids(&self) -> Vec<SurfaceId> {
        let mut ids = Vec::new();
        self.collect_ids_inner(&mut ids);
        ids
    }

    fn collect_ids_inner(&self, ids: &mut Vec<SurfaceId>) {
        match self {
            WorkspaceNode::Leaf { surface_id } => ids.push(*surface_id),
            WorkspaceNode::Split { first, second, .. } => {
                first.collect_ids_inner(ids);
                second.collect_ids_inner(ids);
            }
            WorkspaceNode::Tabs { tabs, .. } => {
                for tab in tabs {
                    tab.collect_ids_inner(ids);
                }
            }
            WorkspaceNode::Float { base, floating } => {
                base.collect_ids_inner(ids);
                for entry in floating {
                    entry.node.collect_ids_inner(ids);
                }
            }
        }
    }

    /// Split the leaf node with the given `target` id.
    /// Returns `true` if the split was performed.
    pub fn split(
        &mut self,
        target: SurfaceId,
        direction: SplitDirection,
        ratio: f32,
        new_id: SurfaceId,
    ) -> bool {
        self.split_with_side(target, direction, ratio, new_id, SplitSide::Second)
    }

    fn split_with_side(
        &mut self,
        target: SurfaceId,
        direction: SplitDirection,
        ratio: f32,
        new_id: SurfaceId,
        new_side: SplitSide,
    ) -> bool {
        match self {
            WorkspaceNode::Leaf { surface_id } if *surface_id == target => {
                let old = WorkspaceNode::Leaf {
                    surface_id: *surface_id,
                };
                let new = WorkspaceNode::Leaf { surface_id: new_id };
                *self = match new_side {
                    SplitSide::First => WorkspaceNode::Split {
                        direction,
                        ratio,
                        first: Box::new(new),
                        second: Box::new(old),
                    },
                    SplitSide::Second => WorkspaceNode::Split {
                        direction,
                        ratio,
                        first: Box::new(old),
                        second: Box::new(new),
                    },
                };
                true
            }
            WorkspaceNode::Leaf { .. } => false,
            WorkspaceNode::Split { first, second, .. } => {
                first.split_with_side(target, direction, ratio, new_id, new_side)
                    || second.split_with_side(target, direction, ratio, new_id, new_side)
            }
            WorkspaceNode::Tabs { tabs, .. } => tabs
                .iter_mut()
                .any(|tab| tab.split_with_side(target, direction, ratio, new_id, new_side)),
            WorkspaceNode::Float { base, floating } => {
                base.split_with_side(target, direction, ratio, new_id, new_side)
                    || floating.iter_mut().any(|entry| {
                        entry
                            .node
                            .split_with_side(target, direction, ratio, new_id, new_side)
                    })
            }
        }
    }

    /// Add a new tab to the tab group containing `target`, or wrap the target
    /// leaf into a new tab group if needed.
    pub fn add_tab(
        &mut self,
        target: SurfaceId,
        new_id: SurfaceId,
        target_label: &str,
        new_label: &str,
    ) -> bool {
        match self {
            WorkspaceNode::Leaf { surface_id } if *surface_id == target => {
                let old = WorkspaceNode::Leaf {
                    surface_id: *surface_id,
                };
                let new = WorkspaceNode::Leaf { surface_id: new_id };
                *self = WorkspaceNode::Tabs {
                    tabs: vec![old, new],
                    active: 1,
                    labels: vec![target_label.to_string(), new_label.to_string()],
                };
                true
            }
            WorkspaceNode::Leaf { .. } => false,
            WorkspaceNode::Split { first, second, .. } => {
                first.add_tab(target, new_id, target_label, new_label)
                    || second.add_tab(target, new_id, target_label, new_label)
            }
            WorkspaceNode::Tabs {
                tabs,
                active,
                labels,
            } => {
                if tabs.iter().any(|tab| tab.find(target).is_some()) {
                    tabs.push(WorkspaceNode::Leaf { surface_id: new_id });
                    labels.push(new_label.to_string());
                    *active = tabs.len() - 1;
                    true
                } else {
                    false
                }
            }
            WorkspaceNode::Float { base, floating } => {
                base.add_tab(target, new_id, target_label, new_label)
                    || floating
                        .iter_mut()
                        .any(|entry| entry.node.add_tab(target, new_id, target_label, new_label))
            }
        }
    }

    /// Remove the leaf with the given `target` id, collapsing the parent split.
    /// Returns `true` if the removal was performed.
    pub fn remove(&mut self, target: SurfaceId) -> bool {
        match self {
            WorkspaceNode::Leaf { .. } => false,
            WorkspaceNode::Split { first, second, .. } => {
                if matches!(first.as_ref(), WorkspaceNode::Leaf { surface_id } if *surface_id == target)
                {
                    *self = *second.clone();
                    return true;
                }
                if matches!(second.as_ref(), WorkspaceNode::Leaf { surface_id } if *surface_id == target)
                {
                    *self = *first.clone();
                    return true;
                }
                first.remove(target) || second.remove(target)
            }
            WorkspaceNode::Tabs {
                tabs,
                active,
                labels,
            } => {
                if let Some(pos) = tabs.iter().position(
                    |tab| matches!(tab, WorkspaceNode::Leaf { surface_id } if *surface_id == target),
                ) {
                    tabs.remove(pos);
                    if pos < labels.len() {
                        labels.remove(pos);
                    }
                    if *active >= tabs.len() && !tabs.is_empty() {
                        *active = tabs.len() - 1;
                    }
                    if tabs.len() == 1 {
                        *self = tabs.remove(0);
                    }
                    return true;
                }
                tabs.iter_mut().any(|tab| tab.remove(target))
            }
            WorkspaceNode::Float { base, floating } => {
                // Try removing from floating entries first
                if let Some(pos) = floating.iter().position(|entry| {
                    matches!(&entry.node, WorkspaceNode::Leaf { surface_id } if *surface_id == target)
                }) {
                    floating.remove(pos);
                    return true;
                }
                // Try floating entry subtrees
                for entry in floating.iter_mut() {
                    if entry.node.remove(target) {
                        return true;
                    }
                }
                // Try base
                base.remove(target)
            }
        }
    }

    /// Count the number of leaf nodes.
    pub fn leaf_count(&self) -> usize {
        match self {
            WorkspaceNode::Leaf { .. } => 1,
            WorkspaceNode::Split { first, second, .. } => first.leaf_count() + second.leaf_count(),
            WorkspaceNode::Tabs { tabs, .. } => tabs.iter().map(|tab| tab.leaf_count()).sum(),
            WorkspaceNode::Float { base, floating } => {
                base.leaf_count()
                    + floating
                        .iter()
                        .map(|entry| entry.node.leaf_count())
                        .sum::<usize>()
            }
        }
    }

    /// Compute screen rectangles for all leaf surfaces given the total available area.
    /// Floating entries are NOT included in the tiled layout; their rects come from
    /// the `FloatingEntry.rect` field.
    pub fn compute_rects(&self, area: Rect) -> HashMap<SurfaceId, Rect> {
        let mut rects = HashMap::new();
        self.compute_rects_inner(area, &mut rects);
        rects
    }

    fn compute_rects_inner(&self, area: Rect, rects: &mut HashMap<SurfaceId, Rect>) {
        match self {
            WorkspaceNode::Leaf { surface_id } => {
                rects.insert(*surface_id, area);
            }
            WorkspaceNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let (first_area, second_area) = area.split(*direction, *ratio);
                first.compute_rects_inner(first_area, rects);
                second.compute_rects_inner(second_area, rects);
            }
            WorkspaceNode::Tabs { tabs, active, .. } => {
                if area.h <= 1 {
                    return;
                }
                let content_area = Rect {
                    x: area.x,
                    y: area.y + 1,
                    w: area.w,
                    h: area.h - 1,
                };
                if let Some(active_tab) = tabs.get(*active) {
                    active_tab.compute_rects_inner(content_area, rects);
                }
            }
            WorkspaceNode::Float { base, floating } => {
                // Base gets the full tiled area
                base.compute_rects_inner(area, rects);
                // Floating entries use their own rects
                for entry in floating {
                    entry.node.compute_rects_inner(entry.rect, rects);
                }
            }
        }
    }

    fn compute_dividers_inner(
        &self,
        area: Rect,
        next_id: &mut u32,
        dividers: &mut Vec<WorkspaceDivider>,
    ) {
        match self {
            WorkspaceNode::Leaf { .. } => {}
            WorkspaceNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let current_id = WorkspaceDividerId(*next_id);
                *next_id += 1;
                let (first_area, second_area) = area.split(*direction, *ratio);
                let (rect, available_main) = match direction {
                    SplitDirection::Vertical => (
                        Rect {
                            x: first_area.x + first_area.w,
                            y: area.y,
                            w: 1,
                            h: area.h,
                        },
                        area.w.saturating_sub(1),
                    ),
                    SplitDirection::Horizontal => (
                        Rect {
                            x: area.x,
                            y: first_area.y + first_area.h,
                            w: area.w,
                            h: 1,
                        },
                        area.h.saturating_sub(1),
                    ),
                };
                dividers.push(WorkspaceDivider {
                    id: current_id,
                    rect,
                    direction: *direction,
                    ratio: *ratio,
                    available_main,
                });
                first.compute_dividers_inner(first_area, next_id, dividers);
                second.compute_dividers_inner(second_area, next_id, dividers);
            }
            WorkspaceNode::Tabs { tabs, active, .. } => {
                if area.h <= 1 {
                    return;
                }
                let content_area = Rect {
                    x: area.x,
                    y: area.y + 1,
                    w: area.w,
                    h: area.h - 1,
                };
                if let Some(active_tab) = tabs.get(*active) {
                    active_tab.compute_dividers_inner(content_area, next_id, dividers);
                }
            }
            WorkspaceNode::Float { base, floating } => {
                base.compute_dividers_inner(area, next_id, dividers);
                for entry in floating {
                    entry
                        .node
                        .compute_dividers_inner(entry.rect, next_id, dividers);
                }
            }
        }
    }

    fn set_divider_ratio(
        &mut self,
        target: WorkspaceDividerId,
        ratio: f32,
        next_id: &mut u32,
    ) -> bool {
        match self {
            WorkspaceNode::Leaf { .. } => false,
            WorkspaceNode::Split {
                ratio: current,
                first,
                second,
                ..
            } => {
                let current_id = WorkspaceDividerId(*next_id);
                *next_id += 1;
                if current_id == target {
                    let new_ratio = ratio.clamp(0.05, 0.95);
                    if (new_ratio - *current).abs() < f32::EPSILON {
                        return false;
                    }
                    *current = new_ratio;
                    return true;
                }
                first.set_divider_ratio(target, ratio, next_id)
                    || second.set_divider_ratio(target, ratio, next_id)
            }
            WorkspaceNode::Tabs { tabs, active, .. } => {
                if let Some(active_tab) = tabs.get_mut(*active) {
                    active_tab.set_divider_ratio(target, ratio, next_id)
                } else {
                    false
                }
            }
            WorkspaceNode::Float { base, floating } => {
                if base.set_divider_ratio(target, ratio, next_id) {
                    return true;
                }
                for entry in floating {
                    if entry.node.set_divider_ratio(target, ratio, next_id) {
                        return true;
                    }
                }
                false
            }
        }
    }

    /// Adjust the nearest ancestor split ratio for `target`.
    ///
    /// Positive `delta` grows the subtree containing `target`.
    fn resize_target(&mut self, target: SurfaceId, delta: f32) -> bool {
        match self {
            WorkspaceNode::Leaf { .. } => false,
            WorkspaceNode::Split {
                ratio,
                first,
                second,
                ..
            } => {
                let in_first = first.find(target).is_some();
                let in_second = second.find(target).is_some();
                if !in_first && !in_second {
                    return false;
                }

                if in_first && first.resize_target(target, delta) {
                    return true;
                }
                if in_second && second.resize_target(target, delta) {
                    return true;
                }

                let signed_delta = if in_first { delta } else { -delta };
                let new_ratio = (*ratio + signed_delta).clamp(0.05, 0.95);
                if (new_ratio - *ratio).abs() < f32::EPSILON {
                    return false;
                }
                *ratio = new_ratio;
                true
            }
            WorkspaceNode::Tabs { tabs, active, .. } => {
                if let Some(active_tab) = tabs.get_mut(*active)
                    && active_tab.find(target).is_some()
                {
                    return active_tab.resize_target(target, delta);
                }
                false
            }
            WorkspaceNode::Float { base, .. } => {
                if base.find(target).is_some() {
                    base.resize_target(target, delta)
                } else {
                    false
                }
            }
        }
    }

    /// Swap the positions of two leaf surfaces.
    fn swap_leaf_ids(&mut self, a: SurfaceId, b: SurfaceId) {
        match self {
            WorkspaceNode::Leaf { surface_id } => {
                if *surface_id == a {
                    *surface_id = b;
                } else if *surface_id == b {
                    *surface_id = a;
                }
            }
            WorkspaceNode::Split { first, second, .. } => {
                first.swap_leaf_ids(a, b);
                second.swap_leaf_ids(a, b);
            }
            WorkspaceNode::Tabs { tabs, .. } => {
                for tab in tabs {
                    tab.swap_leaf_ids(a, b);
                }
            }
            WorkspaceNode::Float { base, floating } => {
                base.swap_leaf_ids(a, b);
                for entry in floating {
                    entry.node.swap_leaf_ids(a, b);
                }
            }
        }
    }

    fn first_leaf_id(&self) -> Option<SurfaceId> {
        match self {
            WorkspaceNode::Leaf { surface_id } => Some(*surface_id),
            WorkspaceNode::Split { first, second, .. } => {
                first.first_leaf_id().or_else(|| second.first_leaf_id())
            }
            WorkspaceNode::Tabs { tabs, active, .. } => tabs
                .get(*active)
                .and_then(WorkspaceNode::first_leaf_id)
                .or_else(|| tabs.iter().find_map(WorkspaceNode::first_leaf_id)),
            WorkspaceNode::Float { base, .. } => base.first_leaf_id(),
        }
    }

    fn capture_restore_placement(&self, target: SurfaceId) -> Option<RestorePlacement> {
        match self {
            WorkspaceNode::Leaf { .. } => None,
            WorkspaceNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                if first.find(target).is_some() {
                    if let Some(inner) = first.capture_restore_placement(target) {
                        return Some(inner);
                    }
                    return second.first_leaf_id().map(|anchor| RestorePlacement {
                        anchor,
                        direction: *direction,
                        ratio: *ratio,
                        side: SplitSide::First,
                    });
                }
                if second.find(target).is_some() {
                    if let Some(inner) = second.capture_restore_placement(target) {
                        return Some(inner);
                    }
                    return first.first_leaf_id().map(|anchor| RestorePlacement {
                        anchor,
                        direction: *direction,
                        ratio: *ratio,
                        side: SplitSide::Second,
                    });
                }
                None
            }
            WorkspaceNode::Tabs { tabs, active, .. } => tabs
                .get(*active)
                .and_then(|tab| {
                    tab.find(target)
                        .and_then(|_| tab.capture_restore_placement(target))
                })
                .or_else(|| {
                    tabs.iter().find_map(|tab| {
                        tab.find(target)
                            .and_then(|_| tab.capture_restore_placement(target))
                    })
                }),
            WorkspaceNode::Float { base, .. } => base.capture_restore_placement(target),
        }
    }
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
    FocusDirection(crate::pane::FocusDirection),
    /// Resize the focused split divider by delta (-1.0..1.0).
    Resize { delta: f32 },
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
    pub fn focus_direction(
        &mut self,
        direction: crate::pane::FocusDirection,
        total: Rect,
    ) -> Option<SurfaceId> {
        let visible = self.visible_surfaces(total);
        if visible.is_empty() {
            return None;
        }

        match direction {
            crate::pane::FocusDirection::Next | crate::pane::FocusDirection::Prev => {
                let len = visible.len();
                let current_index = visible
                    .iter()
                    .position(|(surface_id, _)| *surface_id == self.focused)
                    .unwrap_or(0);
                let next_index = match direction {
                    crate::pane::FocusDirection::Next => (current_index + 1) % len,
                    crate::pane::FocusDirection::Prev => {
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
            crate::pane::FocusDirection::Left
            | crate::pane::FocusDirection::Right
            | crate::pane::FocusDirection::Up
            | crate::pane::FocusDirection::Down => {
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
        }
    }

    fn visible_surfaces(&self, total: Rect) -> Vec<(SurfaceId, Rect)> {
        let mut visible: Vec<_> = self.compute_rects(total).into_iter().collect();
        visible.sort_by_key(|(surface_id, rect)| (rect.y, rect.x, rect.h, rect.w, surface_id.0));
        visible
    }
}

fn focus_direction_score(
    direction: crate::pane::FocusDirection,
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
        crate::pane::FocusDirection::Left => {
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
        crate::pane::FocusDirection::Right => {
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
        crate::pane::FocusDirection::Up => {
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
        crate::pane::FocusDirection::Down => {
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
        crate::pane::FocusDirection::Next | crate::pane::FocusDirection::Prev => None,
    }
}

fn range_gap(start_a: u16, end_a: u16, start_b: u16, end_b: u16) -> u16 {
    if end_a <= start_b {
        start_b - end_a
    } else if end_b <= start_a {
        start_a - end_b
    } else {
        0
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use compact_str::CompactString;

    use crate::element::Element;
    use crate::plugin::Command;
    use crate::surface::{EventContext, SizeHint, Surface, SurfaceEvent, ViewContext};

    struct WorkspaceTestSurface {
        id: SurfaceId,
        key: &'static str,
    }

    impl WorkspaceTestSurface {
        fn new(id: SurfaceId, key: &'static str) -> Self {
            Self { id, key }
        }
    }

    impl Surface for WorkspaceTestSurface {
        fn id(&self) -> SurfaceId {
            self.id
        }

        fn surface_key(&self) -> CompactString {
            self.key.into()
        }

        fn size_hint(&self) -> SizeHint {
            SizeHint::fill()
        }

        fn view(&self, _ctx: &ViewContext<'_>) -> Element {
            Element::Empty
        }

        fn handle_event(&mut self, _event: SurfaceEvent, _ctx: &EventContext<'_>) -> Vec<Command> {
            vec![]
        }
    }

    #[test]
    fn test_new_workspace() {
        let ws = Workspace::new(SurfaceId::BUFFER);
        assert_eq!(ws.focused(), SurfaceId::BUFFER);
        assert_eq!(ws.surface_count(), 1);
        assert!(ws.focus_history().is_empty());
    }

    #[test]
    fn test_split_creates_two_surfaces() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        let new_id = ws.split_focused(SplitDirection::Vertical, 0.5);
        assert_eq!(ws.surface_count(), 2);
        let ids = ws.root().collect_ids();
        assert!(ids.contains(&SurfaceId::BUFFER));
        assert!(ids.contains(&new_id));
    }

    #[test]
    fn test_split_tree_structure() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        let new_id = ws.split_focused(SplitDirection::Vertical, 0.5);
        match ws.root() {
            WorkspaceNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                assert_eq!(*direction, SplitDirection::Vertical);
                assert_eq!(*ratio, 0.5);
                assert!(
                    matches!(first.as_ref(), WorkspaceNode::Leaf { surface_id } if *surface_id == SurfaceId::BUFFER)
                );
                assert!(
                    matches!(second.as_ref(), WorkspaceNode::Leaf { surface_id } if *surface_id == new_id)
                );
            }
            _ => panic!("expected Split"),
        }
    }

    #[test]
    fn test_nested_split() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        ws.split_focused(SplitDirection::Vertical, 0.5);
        ws.split_focused(SplitDirection::Horizontal, 0.3);
        assert_eq!(ws.surface_count(), 3);
    }

    #[test]
    fn test_focus_switch() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        let new_id = ws.split_focused(SplitDirection::Vertical, 0.5);
        ws.focus(new_id);
        assert_eq!(ws.focused(), new_id);
        assert_eq!(ws.focus_history(), &[SurfaceId::BUFFER]);
    }

    #[test]
    fn test_focus_same_noop() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        ws.focus(SurfaceId::BUFFER);
        assert!(ws.focus_history().is_empty());
    }

    #[test]
    fn test_focus_nonexistent_noop() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        ws.focus(SurfaceId(99));
        assert_eq!(ws.focused(), SurfaceId::BUFFER);
    }

    #[test]
    fn test_focus_previous() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        let new_id = ws.split_focused(SplitDirection::Vertical, 0.5);
        ws.focus(new_id);
        ws.focus_previous();
        assert_eq!(ws.focused(), SurfaceId::BUFFER);
    }

    #[test]
    fn test_focus_direction_right_uses_visible_geometry() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        let right = ws.split_focused(SplitDirection::Vertical, 0.5);

        let moved = ws.focus_direction(
            crate::pane::FocusDirection::Right,
            Rect {
                x: 0,
                y: 0,
                w: 80,
                h: 24,
            },
        );

        assert_eq!(moved, Some(right));
        assert_eq!(ws.focused(), right);
    }

    #[test]
    fn test_focus_direction_down_prefers_lower_neighbor() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        let bottom = ws.split_focused(SplitDirection::Horizontal, 0.5);

        let moved = ws.focus_direction(
            crate::pane::FocusDirection::Down,
            Rect {
                x: 0,
                y: 0,
                w: 80,
                h: 24,
            },
        );

        assert_eq!(moved, Some(bottom));
        assert_eq!(ws.focused(), bottom);
    }

    #[test]
    fn test_focus_direction_next_cycles_visible_surfaces() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        let second = ws.split_focused(SplitDirection::Vertical, 0.5);
        let third = ws.split_focused(SplitDirection::Horizontal, 0.5);

        let total = Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        };

        assert_eq!(
            ws.focus_direction(crate::pane::FocusDirection::Next, total),
            Some(second)
        );
        assert_eq!(
            ws.focus_direction(crate::pane::FocusDirection::Next, total),
            Some(third)
        );
        assert_eq!(
            ws.focus_direction(crate::pane::FocusDirection::Prev, total),
            Some(second)
        );
        assert_eq!(ws.focused(), second);
        assert_ne!(second, third);
    }

    #[test]
    fn test_dispatch_workspace_command_with_total_handles_focus_direction() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(crate::surface::buffer::KakouneBufferSurface::new()));
        let right = SurfaceId(10);
        reg.try_register(Box::new(WorkspaceTestSurface::new(right, "test.right")))
            .unwrap();

        let mut dirty = DirtyFlags::empty();
        dispatch_workspace_command(
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

        dirty = DirtyFlags::empty();
        dispatch_workspace_command_with_total(
            &mut reg,
            WorkspaceCommand::FocusDirection(crate::pane::FocusDirection::Right),
            &mut dirty,
            Some(Rect {
                x: 0,
                y: 0,
                w: 80,
                h: 24,
            }),
        );

        assert_eq!(reg.workspace().focused(), right);
        assert!(dirty.contains(DirtyFlags::ALL));
    }

    #[test]
    fn test_resize_focused_grows_first_child_ratio() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        let right = ws.split_focused(SplitDirection::Vertical, 0.5);
        ws.focus(SurfaceId::BUFFER);

        assert!(ws.resize_focused(0.1));

        match ws.root() {
            WorkspaceNode::Split { ratio, .. } => {
                assert!((*ratio - 0.6).abs() < f32::EPSILON);
            }
            other => panic!("expected Split root, got {other:?}"),
        }
        assert_ne!(right, SurfaceId::BUFFER);
    }

    #[test]
    fn test_resize_focused_grows_second_child_by_reducing_ratio() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        let right = ws.split_focused(SplitDirection::Vertical, 0.5);
        ws.focus(right);

        assert!(ws.resize_focused(0.1));

        match ws.root() {
            WorkspaceNode::Split { ratio, .. } => {
                assert!((*ratio - 0.4).abs() < f32::EPSILON);
            }
            other => panic!("expected Split root, got {other:?}"),
        }
    }

    #[test]
    fn test_resize_focused_targets_nearest_ancestor_split() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        let right = ws.split_focused(SplitDirection::Vertical, 0.5);
        ws.focus(right);
        let bottom_right = ws.split_focused(SplitDirection::Horizontal, 0.5);
        ws.focus(bottom_right);

        assert!(ws.resize_focused(0.1));

        match ws.root() {
            WorkspaceNode::Split { ratio, second, .. } => {
                assert!((*ratio - 0.5).abs() < f32::EPSILON);
                match second.as_ref() {
                    WorkspaceNode::Split { ratio, .. } => {
                        assert!((*ratio - 0.4).abs() < f32::EPSILON);
                    }
                    other => panic!("expected nested Split, got {other:?}"),
                }
            }
            other => panic!("expected Split root, got {other:?}"),
        }
    }

    #[test]
    fn test_resize_focused_returns_false_without_split() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        assert!(!ws.resize_focused(0.1));
    }

    #[test]
    fn test_dispatch_workspace_command_resize_marks_dirty() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(crate::surface::buffer::KakouneBufferSurface::new()));
        let right = SurfaceId(10);
        reg.try_register(Box::new(WorkspaceTestSurface::new(right, "test.right")))
            .unwrap();

        let mut dirty = DirtyFlags::empty();
        dispatch_workspace_command(
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
        reg.workspace_mut().focus(right);

        dirty = DirtyFlags::empty();
        dispatch_workspace_command(
            &mut reg,
            WorkspaceCommand::Resize { delta: 0.1 },
            &mut dirty,
        );

        assert!(dirty.contains(DirtyFlags::ALL));
        match reg.workspace().root() {
            WorkspaceNode::Split { ratio, .. } => {
                assert!((*ratio - 0.4).abs() < f32::EPSILON);
            }
            other => panic!("expected Split root, got {other:?}"),
        }
    }

    #[test]
    fn test_swap_surfaces_exchanges_split_leaf_positions() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        let right = ws.split_focused(SplitDirection::Vertical, 0.5);

        assert!(ws.swap_surfaces(SurfaceId::BUFFER, right));

        match ws.root() {
            WorkspaceNode::Split { first, second, .. } => {
                assert!(
                    matches!(first.as_ref(), WorkspaceNode::Leaf { surface_id } if *surface_id == right)
                );
                assert!(
                    matches!(second.as_ref(), WorkspaceNode::Leaf { surface_id } if *surface_id == SurfaceId::BUFFER)
                );
            }
            other => panic!("expected Split root, got {other:?}"),
        }
        assert_eq!(ws.focused(), SurfaceId::BUFFER);
    }

    #[test]
    fn test_swap_surfaces_exchanges_tiled_and_floating_positions() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        let floated = ws.split_focused(SplitDirection::Vertical, 0.5);
        let float_rect = Rect {
            x: 12,
            y: 6,
            w: 24,
            h: 7,
        };
        assert!(ws.float_surface(floated, float_rect));

        assert!(ws.swap_surfaces(SurfaceId::BUFFER, floated));

        let rects = ws.compute_rects(Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        });
        assert_eq!(
            rects[&floated],
            Rect {
                x: 0,
                y: 0,
                w: 80,
                h: 24
            }
        );
        assert_eq!(rects[&SurfaceId::BUFFER], float_rect);
    }

    #[test]
    fn test_swap_surfaces_returns_false_for_missing_surface() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        let right = ws.split_focused(SplitDirection::Vertical, 0.5);
        assert!(!ws.swap_surfaces(right, SurfaceId(999)));
    }

    #[test]
    fn test_dispatch_workspace_command_swap_marks_dirty() {
        let mut reg = SurfaceRegistry::new();
        reg.register(Box::new(crate::surface::buffer::KakouneBufferSurface::new()));
        let right = SurfaceId(10);
        reg.try_register(Box::new(WorkspaceTestSurface::new(right, "test.right")))
            .unwrap();

        let mut dirty = DirtyFlags::empty();
        dispatch_workspace_command(
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

        dirty = DirtyFlags::empty();
        dispatch_workspace_command(
            &mut reg,
            WorkspaceCommand::Swap(SurfaceId::BUFFER, right),
            &mut dirty,
        );

        assert!(dirty.contains(DirtyFlags::ALL));
        match reg.workspace().root() {
            WorkspaceNode::Split { first, second, .. } => {
                assert!(
                    matches!(first.as_ref(), WorkspaceNode::Leaf { surface_id } if *surface_id == right)
                );
                assert!(
                    matches!(second.as_ref(), WorkspaceNode::Leaf { surface_id } if *surface_id == SurfaceId::BUFFER)
                );
            }
            other => panic!("expected Split root, got {other:?}"),
        }
    }

    #[test]
    fn test_close_surface() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        let new_id = ws.split_focused(SplitDirection::Vertical, 0.5);
        assert!(ws.close(new_id));
        assert_eq!(ws.surface_count(), 1);
    }

    #[test]
    fn test_close_focused_switches_focus() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        let new_id = ws.split_focused(SplitDirection::Vertical, 0.5);
        ws.focus(new_id);
        assert!(ws.close(new_id));
        assert_eq!(ws.focused(), SurfaceId::BUFFER);
    }

    #[test]
    fn test_cannot_close_last_surface() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        assert!(!ws.close(SurfaceId::BUFFER));
    }

    #[test]
    fn test_close_nonexistent() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        assert!(!ws.close(SurfaceId(99)));
    }

    #[test]
    fn test_compute_rects_single() {
        let ws = Workspace::new(SurfaceId::BUFFER);
        let total = Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        };
        let rects = ws.compute_rects(total);
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[&SurfaceId::BUFFER], total);
    }

    #[test]
    fn test_compute_rects_vertical_split() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        let new_id = ws.split_focused(SplitDirection::Vertical, 0.5);
        let total = Rect {
            x: 0,
            y: 0,
            w: 81,
            h: 24,
        };
        let rects = ws.compute_rects(total);
        assert_eq!(rects.len(), 2);
        let r0 = rects[&SurfaceId::BUFFER];
        let r1 = rects[&new_id];
        assert_eq!(r0.x, 0);
        assert_eq!(r0.w, 40);
        assert_eq!(r1.x, 41);
        assert_eq!(r1.w, 40);
    }

    #[test]
    fn test_compute_rects_horizontal_split() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        let new_id = ws.split_focused(SplitDirection::Horizontal, 0.5);
        let total = Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 25,
        };
        let rects = ws.compute_rects(total);
        let r0 = rects[&SurfaceId::BUFFER];
        let r1 = rects[&new_id];
        assert_eq!(r0.y, 0);
        assert_eq!(r0.h, 12);
        assert_eq!(r1.y, 13);
        assert_eq!(r1.h, 12);
    }

    #[test]
    fn test_surface_at() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        let new_id = ws.split_focused(SplitDirection::Vertical, 0.5);
        let total = Rect {
            x: 0,
            y: 0,
            w: 81,
            h: 24,
        };
        assert_eq!(ws.surface_at(0, 0, total), Some(SurfaceId::BUFFER));
        assert_eq!(ws.surface_at(39, 12, total), Some(SurfaceId::BUFFER));
        assert_eq!(ws.surface_at(41, 0, total), Some(new_id));
        assert_eq!(ws.surface_at(80, 23, total), Some(new_id));
        assert_eq!(ws.surface_at(40, 12, total), None); // divider
    }

    #[test]
    fn test_compute_dividers_vertical_split() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        ws.split_focused(SplitDirection::Vertical, 0.5);
        let total = Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        };

        let dividers = ws.compute_dividers(total);
        assert_eq!(dividers.len(), 1);
        assert_eq!(dividers[0].id, WorkspaceDividerId(0));
        assert_eq!(dividers[0].direction, SplitDirection::Vertical);
        assert_eq!(
            dividers[0].rect,
            Rect {
                x: 40,
                y: 0,
                w: 1,
                h: 24,
            }
        );
        assert_eq!(dividers[0].available_main, 79);
        assert_eq!(ws.divider_at(40, 12, total), Some(dividers[0]));
    }

    #[test]
    fn test_set_divider_ratio_updates_exact_split() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        let right = ws.split_focused(SplitDirection::Vertical, 0.5);
        ws.focus(right);
        ws.split_focused(SplitDirection::Horizontal, 0.5);
        let total = Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        };

        let dividers = ws.compute_dividers(total);
        let horizontal = dividers
            .iter()
            .find(|divider| divider.direction == SplitDirection::Horizontal)
            .copied()
            .expect("expected nested horizontal divider");
        assert!(ws.set_divider_ratio(horizontal.id, 0.25));

        match ws.root() {
            WorkspaceNode::Split { ratio, second, .. } => {
                assert_eq!(*ratio, 0.5, "root split should remain unchanged");
                match second.as_ref() {
                    WorkspaceNode::Split { ratio, .. } => {
                        assert!((*ratio - 0.25).abs() < f32::EPSILON);
                    }
                    other => panic!("expected nested split, got {other:?}"),
                }
            }
            other => panic!("expected root split, got {other:?}"),
        }
    }

    #[test]
    fn test_tabs_node() {
        let node = WorkspaceNode::Tabs {
            tabs: vec![
                WorkspaceNode::leaf(SurfaceId(10)),
                WorkspaceNode::leaf(SurfaceId(11)),
            ],
            active: 0,
            labels: vec!["tab1".into(), "tab2".into()],
        };
        assert_eq!(node.leaf_count(), 2);
        assert!(node.find(SurfaceId(10)).is_some());
        assert!(node.find(SurfaceId(11)).is_some());
    }

    #[test]
    fn test_tabs_remove_collapses() {
        let mut node = WorkspaceNode::Tabs {
            tabs: vec![
                WorkspaceNode::leaf(SurfaceId(10)),
                WorkspaceNode::leaf(SurfaceId(11)),
            ],
            active: 0,
            labels: vec!["tab1".into(), "tab2".into()],
        };
        assert!(node.remove(SurfaceId(10)));
        assert!(matches!(node, WorkspaceNode::Leaf { surface_id } if surface_id == SurfaceId(11)));
    }

    #[test]
    fn test_tabs_compute_rects() {
        let node = WorkspaceNode::Tabs {
            tabs: vec![
                WorkspaceNode::leaf(SurfaceId(10)),
                WorkspaceNode::leaf(SurfaceId(11)),
            ],
            active: 0,
            labels: vec!["tab1".into(), "tab2".into()],
        };
        let area = Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        };
        let rects = node.compute_rects(area);
        assert_eq!(rects.len(), 1);
        let r = rects[&SurfaceId(10)];
        assert_eq!(r.y, 1);
        assert_eq!(r.h, 23);
    }

    #[test]
    fn test_add_tab_wraps_leaf_and_focuses_new_surface() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        assert!(ws.add_tab(SurfaceId::BUFFER, SurfaceId(10), "buffer", "plugin.tab"));
        assert_eq!(ws.focused(), SurfaceId(10));
        match ws.root() {
            WorkspaceNode::Tabs {
                tabs,
                active,
                labels,
            } => {
                assert_eq!(*active, 1);
                assert_eq!(tabs.len(), 2);
                assert_eq!(
                    labels,
                    &vec!["buffer".to_string(), "plugin.tab".to_string()]
                );
            }
            other => panic!("expected Tabs root, got {other:?}"),
        }
    }

    #[test]
    fn test_float_node() {
        let node = WorkspaceNode::Float {
            base: Box::new(WorkspaceNode::leaf(SurfaceId(0))),
            floating: vec![FloatingEntry {
                node: WorkspaceNode::leaf(SurfaceId(10)),
                rect: Rect {
                    x: 10,
                    y: 5,
                    w: 30,
                    h: 10,
                },
                z_order: 0,
                restore: None,
            }],
        };
        assert_eq!(node.leaf_count(), 2);
        assert!(node.find(SurfaceId(0)).is_some());
        assert!(node.find(SurfaceId(10)).is_some());
    }

    #[test]
    fn test_float_compute_rects() {
        let node = WorkspaceNode::Float {
            base: Box::new(WorkspaceNode::leaf(SurfaceId(0))),
            floating: vec![FloatingEntry {
                node: WorkspaceNode::leaf(SurfaceId(10)),
                rect: Rect {
                    x: 10,
                    y: 5,
                    w: 30,
                    h: 10,
                },
                z_order: 0,
                restore: None,
            }],
        };
        let total = Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        };
        let rects = node.compute_rects(total);
        assert_eq!(rects[&SurfaceId(0)], total);
        assert_eq!(
            rects[&SurfaceId(10)],
            Rect {
                x: 10,
                y: 5,
                w: 30,
                h: 10
            }
        );
    }

    #[test]
    fn test_float_remove_floating() {
        let mut node = WorkspaceNode::Float {
            base: Box::new(WorkspaceNode::leaf(SurfaceId(0))),
            floating: vec![FloatingEntry {
                node: WorkspaceNode::leaf(SurfaceId(10)),
                rect: Rect {
                    x: 10,
                    y: 5,
                    w: 30,
                    h: 10,
                },
                z_order: 0,
                restore: None,
            }],
        };
        assert!(node.remove(SurfaceId(10)));
        // Floating should be empty now
        if let WorkspaceNode::Float { floating, .. } = &node {
            assert!(floating.is_empty());
        } else {
            panic!("expected Float");
        }
    }

    #[test]
    fn test_dock_surface_left_wraps_root() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        ws.dock_surface(SurfaceId(10), DockPosition::Left, 0.25);
        let rects = ws.compute_rects(Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        });
        assert_eq!(
            rects[&SurfaceId(10)],
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
    fn test_add_floating_wraps_root_and_assigns_rect() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        let float_rect = Rect {
            x: 10,
            y: 5,
            w: 30,
            h: 8,
        };
        ws.add_floating(SurfaceId(20), float_rect);
        let rects = ws.compute_rects(Rect {
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
                h: 24
            }
        );
        assert_eq!(rects[&SurfaceId(20)], float_rect);
    }

    #[test]
    fn test_float_surface_moves_existing_leaf_into_floating_layer() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        let floated = ws.split_focused(SplitDirection::Vertical, 0.5);
        let float_rect = Rect {
            x: 12,
            y: 6,
            w: 24,
            h: 7,
        };

        assert!(ws.float_surface(floated, float_rect));

        let rects = ws.compute_rects(Rect {
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
        assert_eq!(rects[&floated], float_rect);
    }

    #[test]
    fn test_float_surface_rejects_last_tiled_surface() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        assert!(!ws.float_surface(
            SurfaceId::BUFFER,
            Rect {
                x: 2,
                y: 2,
                w: 10,
                h: 5,
            }
        ));
    }

    #[test]
    fn test_unfloat_surface_retiles_floating_entry() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        let floated = ws.split_focused(SplitDirection::Vertical, 0.3);
        assert!(ws.float_surface(
            floated,
            Rect {
                x: 12,
                y: 6,
                w: 24,
                h: 7,
            }
        ));

        assert!(ws.unfloat_surface(floated, SplitDirection::Horizontal, 0.8));
        assert_eq!(ws.focused(), floated);

        let rects = ws.compute_rects(Rect {
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
                w: 24,
                h: 24,
            }
        );
        assert_eq!(
            rects[&floated],
            Rect {
                x: 25,
                y: 0,
                w: 55,
                h: 24,
            }
        );
    }

    #[test]
    fn test_unfloat_surface_restores_first_side_from_saved_placement() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        let right = ws.split_focused(SplitDirection::Vertical, 0.3);
        assert!(ws.float_surface(
            SurfaceId::BUFFER,
            Rect {
                x: 2,
                y: 2,
                w: 10,
                h: 5,
            }
        ));

        assert!(ws.unfloat_surface(SurfaceId::BUFFER, SplitDirection::Horizontal, 0.8));

        let rects = ws.compute_rects(Rect {
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
                w: 24,
                h: 24,
            }
        );
        assert_eq!(
            rects[&right],
            Rect {
                x: 25,
                y: 0,
                w: 55,
                h: 24,
            }
        );
    }

    #[test]
    fn test_unfloat_surface_returns_false_for_non_floating_surface() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        let tiled = ws.split_focused(SplitDirection::Vertical, 0.5);
        assert!(!ws.unfloat_surface(tiled, SplitDirection::Vertical, 0.5));
    }

    #[test]
    fn test_next_surface_id() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        let id1 = ws.next_surface_id();
        let id2 = ws.next_surface_id();
        assert_eq!(id1.0, SurfaceId::PLUGIN_BASE);
        assert_eq!(id2.0, SurfaceId::PLUGIN_BASE + 1);
    }

    #[test]
    fn test_close_cleans_focus_history() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        let id1 = ws.split_focused(SplitDirection::Vertical, 0.5);
        let id2 = ws.split_focused(SplitDirection::Horizontal, 0.5);
        ws.focus(id1);
        ws.focus(id2);
        ws.close(id1);
        assert!(!ws.focus_history().contains(&id1));
    }

    #[test]
    fn test_workspace_query() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        let new_id = ws.split_focused(SplitDirection::Vertical, 0.5);
        let total = Rect {
            x: 0,
            y: 0,
            w: 81,
            h: 24,
        };
        let query = ws.query(total);
        assert_eq!(query.surface_count(), 2);
        assert_eq!(query.focused(), SurfaceId::BUFFER);
        assert!(query.rect_of(SurfaceId::BUFFER).is_some());
        assert!(query.rect_of(new_id).is_some());
        assert!(query.rect_of(SurfaceId(99)).is_none());
        assert_eq!(query.surfaces().len(), 2);
    }

    #[test]
    fn test_node_find() {
        let mut ws = Workspace::new(SurfaceId::BUFFER);
        let new_id = ws.split_focused(SplitDirection::Vertical, 0.5);
        assert!(ws.root().find(SurfaceId::BUFFER).is_some());
        assert!(ws.root().find(new_id).is_some());
        assert!(ws.root().find(SurfaceId(99)).is_none());
    }

    #[test]
    fn test_node_remove_first() {
        let mut node = WorkspaceNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(WorkspaceNode::leaf(SurfaceId(0))),
            second: Box::new(WorkspaceNode::leaf(SurfaceId(1))),
        };
        assert!(node.remove(SurfaceId(0)));
        assert!(matches!(node, WorkspaceNode::Leaf { surface_id } if surface_id == SurfaceId(1)));
    }

    #[test]
    fn test_node_remove_second() {
        let mut node = WorkspaceNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(WorkspaceNode::leaf(SurfaceId(0))),
            second: Box::new(WorkspaceNode::leaf(SurfaceId(1))),
        };
        assert!(node.remove(SurfaceId(1)));
        assert!(matches!(node, WorkspaceNode::Leaf { surface_id } if surface_id == SurfaceId(0)));
    }

    #[test]
    fn test_split_rect_vertical() {
        let area = Rect {
            x: 0,
            y: 0,
            w: 81,
            h: 24,
        };
        let (a, b) = area.split(SplitDirection::Vertical, 0.5);
        assert_eq!(a.w, 40);
        assert_eq!(b.w, 40);
        assert_eq!(a.x, 0);
        assert_eq!(b.x, 41);
    }

    #[test]
    fn test_split_rect_horizontal() {
        let area = Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 25,
        };
        let (a, b) = area.split(SplitDirection::Horizontal, 0.5);
        assert_eq!(a.h, 12);
        assert_eq!(b.h, 12);
        assert_eq!(a.y, 0);
        assert_eq!(b.y, 13);
    }
}
