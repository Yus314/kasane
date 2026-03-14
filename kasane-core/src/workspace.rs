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
        match self {
            WorkspaceNode::Leaf { surface_id } if *surface_id == target => {
                let old = WorkspaceNode::Leaf {
                    surface_id: *surface_id,
                };
                let new = WorkspaceNode::Leaf { surface_id: new_id };
                *self = WorkspaceNode::Split {
                    direction,
                    ratio,
                    first: Box::new(old),
                    second: Box::new(new),
                };
                true
            }
            WorkspaceNode::Leaf { .. } => false,
            WorkspaceNode::Split { first, second, .. } => {
                first.split(target, direction, ratio, new_id)
                    || second.split(target, direction, ratio, new_id)
            }
            WorkspaceNode::Tabs { tabs, .. } => tabs
                .iter_mut()
                .any(|tab| tab.split(target, direction, ratio, new_id)),
            WorkspaceNode::Float { base, floating } => {
                base.split(target, direction, ratio, new_id)
                    || floating
                        .iter_mut()
                        .any(|entry| entry.node.split(target, direction, ratio, new_id))
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
    match cmd {
        WorkspaceCommand::AddSurface {
            surface_id,
            placement,
        } => {
            let ws = surface_registry.workspace_mut();
            match placement {
                Placement::SplitFocused { direction, ratio } => {
                    let focused = ws.focused();
                    ws.root_mut().split(focused, direction, ratio, surface_id);
                }
                Placement::SplitFrom {
                    target,
                    direction,
                    ratio,
                } => {
                    ws.root_mut().split(target, direction, ratio, surface_id);
                }
                _ => {} // Tab, Dock, Float — handled in future phases
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
        WorkspaceCommand::FocusDirection(_) => {
            // Direction-based focus navigation — future phase
        }
        WorkspaceCommand::Swap(a, b) => {
            let _ = (a, b); // Swap — future phase
            *dirty |= DirtyFlags::ALL;
        }
        WorkspaceCommand::Resize { .. }
        | WorkspaceCommand::Float { .. }
        | WorkspaceCommand::Unfloat(_) => {
            // Future phases
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

    /// Build a read-only query handle for inspecting the workspace.
    pub fn query(&self, total: Rect) -> WorkspaceQuery<'_> {
        WorkspaceQuery {
            workspace: self,
            rects: self.compute_rects(total),
        }
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
