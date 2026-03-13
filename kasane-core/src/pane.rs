//! Multi-pane management: tree structure, focus tracking, and pane commands.
//!
//! Phase 5a-0: Foundation scaffold. Type definitions and basic tree operations only.
//! No behavioral changes to the existing single-pane rendering pipeline.

use std::collections::HashMap;

use crate::layout::Rect;
use crate::plugin::PluginId;

/// Unique identifier for a pane within a `PaneManager`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PaneId(pub u32);

/// Direction of a pane split.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

/// Direction for focus navigation between panes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusDirection {
    Next,
    Prev,
    Left,
    Right,
    Up,
    Down,
}

/// Content to initialize a new pane with.
#[derive(Debug, Clone)]
pub enum NewPaneContent {
    /// Spawn a new Kakoune process (or connect to an existing session).
    Kakoune {
        session: Option<String>,
        args: Vec<String>,
    },
    /// Spawn a terminal process (e.g. shell, fzf).
    Terminal { command: String, args: Vec<String> },
    /// Plugin-rendered pane content.
    Plugin { plugin_id: PluginId },
}

/// Commands that manipulate the pane tree. Dispatched via `Command::Pane(PaneCommand)`.
#[derive(Debug)]
pub enum PaneCommand {
    /// Split the focused pane.
    Split {
        direction: SplitDirection,
        ratio: f32,
        content: Option<NewPaneContent>,
    },
    /// Close a specific pane.
    Close(PaneId),
    /// Focus a specific pane.
    Focus(PaneId),
    /// Move focus in a direction.
    FocusDirection(FocusDirection),
    /// Resize the focused split divider by delta (-1.0..1.0).
    Resize { delta: f32 },
    /// Create a floating window.
    Float { content: NewPaneContent, rect: Rect },
    /// Create a new tab in the current tab group.
    NewTab { content: Option<NewPaneContent> },
    /// Switch to a tab by index.
    SwitchTab(usize),
    /// Close the current tab.
    CloseTab,
    /// Swap two panes.
    Swap(PaneId, PaneId),
    /// Spawn a terminal in a new split.
    SpawnTerminal {
        command: String,
        args: Vec<String>,
        direction: SplitDirection,
    },
}

/// Pane permission flags for capability-based access control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanePermissions(u32);

impl PanePermissions {
    pub const SPLIT: u32 = 1;
    pub const FOCUS: u32 = 2;
    pub const SPAWN: u32 = 4;
    pub const FLOAT: u32 = 8;
    pub const CROSS_PANE: u32 = 16;
    pub const TABS: u32 = 32;

    pub const fn empty() -> Self {
        PanePermissions(0)
    }

    pub const fn all() -> Self {
        PanePermissions(
            Self::SPLIT | Self::FOCUS | Self::SPAWN | Self::FLOAT | Self::CROSS_PANE | Self::TABS,
        )
    }

    pub const fn contains(self, flag: u32) -> bool {
        self.0 & flag != 0
    }

    pub const fn bits(self) -> u32 {
        self.0
    }

    pub const fn from_bits(bits: u32) -> Self {
        PanePermissions(bits)
    }
}

/// Context information about the current pane, passed to plugins.
#[derive(Debug, Clone)]
pub struct PaneContext {
    pub pane_id: PaneId,
    pub pane_count: usize,
    pub is_focused: bool,
    pub pane_rect: Rect,
}

/// A node in the pane layout tree.
#[derive(Debug, Clone)]
pub enum PaneNode {
    /// A single pane.
    Leaf { id: PaneId },
    /// A split into two sub-trees.
    Split {
        direction: SplitDirection,
        /// Ratio allocated to the first child (0.0..1.0).
        ratio: f32,
        first: Box<PaneNode>,
        second: Box<PaneNode>,
    },
    /// Tab group: multiple pane trees sharing the same screen area.
    Tabs {
        tabs: Vec<PaneNode>,
        active: usize,
        labels: Vec<String>,
    },
}

impl PaneNode {
    /// Create a leaf node.
    pub fn leaf(id: PaneId) -> Self {
        PaneNode::Leaf { id }
    }

    /// Find a node containing the given PaneId and return a reference.
    pub fn find(&self, target: PaneId) -> Option<&PaneNode> {
        match self {
            PaneNode::Leaf { id } if *id == target => Some(self),
            PaneNode::Leaf { .. } => None,
            PaneNode::Split { first, second, .. } => {
                first.find(target).or_else(|| second.find(target))
            }
            PaneNode::Tabs { tabs, .. } => tabs.iter().find_map(|tab| tab.find(target)),
        }
    }

    /// Collect all leaf PaneIds in this subtree (depth-first order).
    pub fn collect_ids(&self) -> Vec<PaneId> {
        let mut ids = Vec::new();
        self.collect_ids_inner(&mut ids);
        ids
    }

    fn collect_ids_inner(&self, ids: &mut Vec<PaneId>) {
        match self {
            PaneNode::Leaf { id } => ids.push(*id),
            PaneNode::Split { first, second, .. } => {
                first.collect_ids_inner(ids);
                second.collect_ids_inner(ids);
            }
            PaneNode::Tabs { tabs, .. } => {
                for tab in tabs {
                    tab.collect_ids_inner(ids);
                }
            }
        }
    }

    /// Split the leaf node with the given `target` id.
    /// Returns `true` if the split was performed.
    pub fn split(
        &mut self,
        target: PaneId,
        direction: SplitDirection,
        ratio: f32,
        new_id: PaneId,
    ) -> bool {
        match self {
            PaneNode::Leaf { id } if *id == target => {
                let old = PaneNode::Leaf { id: *id };
                let new = PaneNode::Leaf { id: new_id };
                *self = PaneNode::Split {
                    direction,
                    ratio,
                    first: Box::new(old),
                    second: Box::new(new),
                };
                true
            }
            PaneNode::Leaf { .. } => false,
            PaneNode::Split { first, second, .. } => {
                first.split(target, direction, ratio, new_id)
                    || second.split(target, direction, ratio, new_id)
            }
            PaneNode::Tabs { tabs, .. } => tabs
                .iter_mut()
                .any(|tab| tab.split(target, direction, ratio, new_id)),
        }
    }

    /// Remove the leaf with the given `target` id, collapsing the parent split.
    /// Returns `true` if the removal was performed.
    pub fn remove(&mut self, target: PaneId) -> bool {
        match self {
            PaneNode::Leaf { .. } => false, // can't remove self at root level
            PaneNode::Split { first, second, .. } => {
                // Check if first child is the target leaf
                if matches!(first.as_ref(), PaneNode::Leaf { id } if *id == target) {
                    *self = *second.clone();
                    return true;
                }
                // Check if second child is the target leaf
                if matches!(second.as_ref(), PaneNode::Leaf { id } if *id == target) {
                    *self = *first.clone();
                    return true;
                }
                // Recurse
                first.remove(target) || second.remove(target)
            }
            PaneNode::Tabs {
                tabs,
                active,
                labels,
            } => {
                // Check if any direct tab is the target leaf
                if let Some(pos) = tabs
                    .iter()
                    .position(|tab| matches!(tab, PaneNode::Leaf { id } if *id == target))
                {
                    tabs.remove(pos);
                    if pos < labels.len() {
                        labels.remove(pos);
                    }
                    if *active >= tabs.len() && !tabs.is_empty() {
                        *active = tabs.len() - 1;
                    }
                    // If only one tab remains, collapse the Tabs node
                    if tabs.len() == 1 {
                        *self = tabs.remove(0);
                    }
                    return true;
                }
                // Recurse into tabs
                tabs.iter_mut().any(|tab| tab.remove(target))
            }
        }
    }

    /// Count the number of leaf nodes.
    pub fn leaf_count(&self) -> usize {
        match self {
            PaneNode::Leaf { .. } => 1,
            PaneNode::Split { first, second, .. } => first.leaf_count() + second.leaf_count(),
            PaneNode::Tabs { tabs, .. } => tabs.iter().map(|tab| tab.leaf_count()).sum(),
        }
    }

    /// Compute screen rectangles for all leaf panes given the total available area.
    pub fn compute_rects(&self, area: Rect) -> HashMap<PaneId, Rect> {
        let mut rects = HashMap::new();
        self.compute_rects_inner(area, &mut rects);
        rects
    }

    fn compute_rects_inner(&self, area: Rect, rects: &mut HashMap<PaneId, Rect>) {
        match self {
            PaneNode::Leaf { id } => {
                rects.insert(*id, area);
            }
            PaneNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let (first_area, second_area) = split_rect(area, *direction, *ratio);
                first.compute_rects_inner(first_area, rects);
                second.compute_rects_inner(second_area, rects);
            }
            PaneNode::Tabs { tabs, active, .. } => {
                // Tab bar takes 1 row from the top
                if area.h <= 1 {
                    return;
                }
                let content_area = Rect {
                    x: area.x,
                    y: area.y + 1, // 1 row for tab bar
                    w: area.w,
                    h: area.h - 1,
                };
                if let Some(active_tab) = tabs.get(*active) {
                    active_tab.compute_rects_inner(content_area, rects);
                }
            }
        }
    }
}

/// Split a rectangle into two sub-rectangles with a 1-cell divider.
fn split_rect(area: Rect, direction: SplitDirection, ratio: f32) -> (Rect, Rect) {
    match direction {
        SplitDirection::Vertical => {
            // Side by side, divider is a vertical line (1 column)
            let total = area.w.saturating_sub(1); // 1 col for divider
            let first_w = ((total as f32) * ratio).round() as u16;
            let second_w = total.saturating_sub(first_w);
            let first = Rect {
                x: area.x,
                y: area.y,
                w: first_w,
                h: area.h,
            };
            let second = Rect {
                x: area.x + first_w + 1, // +1 for divider
                y: area.y,
                w: second_w,
                h: area.h,
            };
            (first, second)
        }
        SplitDirection::Horizontal => {
            // Stacked top/bottom, divider is a horizontal line (1 row)
            let total = area.h.saturating_sub(1); // 1 row for divider
            let first_h = ((total as f32) * ratio).round() as u16;
            let second_h = total.saturating_sub(first_h);
            let first = Rect {
                x: area.x,
                y: area.y,
                w: area.w,
                h: first_h,
            };
            let second = Rect {
                x: area.x,
                y: area.y + first_h + 1, // +1 for divider
                w: area.w,
                h: second_h,
            };
            (first, second)
        }
    }
}

/// Manages the pane layout tree and focus state.
pub struct PaneManager {
    root: PaneNode,
    focused: PaneId,
    focus_history: Vec<PaneId>,
    next_id: u32,
}

impl PaneManager {
    /// Create a new PaneManager with a single root pane (PaneId(0)).
    pub fn new() -> Self {
        PaneManager {
            root: PaneNode::leaf(PaneId(0)),
            focused: PaneId(0),
            focus_history: Vec::new(),
            next_id: 1,
        }
    }

    /// Get the root node of the pane tree.
    pub fn root(&self) -> &PaneNode {
        &self.root
    }

    /// Get the currently focused pane id.
    pub fn focused(&self) -> PaneId {
        self.focused
    }

    /// Get the focus history stack.
    pub fn focus_history(&self) -> &[PaneId] {
        &self.focus_history
    }

    /// Total number of leaf panes.
    pub fn pane_count(&self) -> usize {
        self.root.leaf_count()
    }

    /// Allocate the next available PaneId.
    pub fn next_pane_id(&mut self) -> PaneId {
        let id = PaneId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Split the focused pane, returning the new pane's id.
    pub fn split_focused(&mut self, direction: SplitDirection, ratio: f32) -> PaneId {
        let new_id = self.next_pane_id();
        self.root.split(self.focused, direction, ratio, new_id);
        new_id
    }

    /// Focus a specific pane. Pushes the previous focus onto the history stack.
    pub fn focus(&mut self, target: PaneId) {
        if target == self.focused {
            return;
        }
        if self.root.find(target).is_some() {
            self.focus_history.push(self.focused);
            self.focused = target;
        }
    }

    /// Return to the previously focused pane.
    pub fn focus_previous(&mut self) -> Option<PaneId> {
        while let Some(prev) = self.focus_history.pop() {
            // Only focus if the pane still exists
            if self.root.find(prev).is_some() {
                let old = self.focused;
                self.focused = prev;
                return Some(old);
            }
        }
        None
    }

    /// Close a pane by removing it from the tree.
    /// If the closed pane was focused, focus moves to the previous pane or the first leaf.
    /// Returns `true` if removal succeeded (cannot remove the last pane).
    pub fn close(&mut self, target: PaneId) -> bool {
        if self.root.leaf_count() <= 1 {
            return false; // can't close the last pane
        }
        if !self.root.remove(target) {
            return false;
        }
        // If we closed the focused pane, pick a new focus
        if self.focused == target {
            // Try focus history
            if self.focus_previous().is_none() {
                // Fall back to first leaf
                let ids = self.root.collect_ids();
                if let Some(&first) = ids.first() {
                    self.focused = first;
                }
            }
        }
        // Clean up history references to removed pane
        self.focus_history.retain(|id| *id != target);
        true
    }

    /// Compute screen rectangles for all panes.
    pub fn compute_rects(&self, total: Rect) -> HashMap<PaneId, Rect> {
        self.root.compute_rects(total)
    }

    /// Find which pane contains the given screen coordinates.
    pub fn pane_at(&self, x: u16, y: u16, total: Rect) -> Option<PaneId> {
        let rects = self.compute_rects(total);
        rects.into_iter().find_map(|(id, rect)| {
            if x >= rect.x && x < rect.x + rect.w && y >= rect.y && y < rect.y + rect.h {
                Some(id)
            } else {
                None
            }
        })
    }

    /// Build a `PaneContext` for the given pane id.
    pub fn context_for(&self, pane_id: PaneId, total: Rect) -> Option<PaneContext> {
        let rects = self.compute_rects(total);
        rects.get(&pane_id).map(|&pane_rect| PaneContext {
            pane_id,
            pane_count: self.pane_count(),
            is_focused: pane_id == self.focused,
            pane_rect,
        })
    }
}

impl Default for PaneManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_pane_manager() {
        let pm = PaneManager::new();
        assert_eq!(pm.focused(), PaneId(0));
        assert_eq!(pm.pane_count(), 1);
        assert!(pm.focus_history().is_empty());
        assert!(matches!(pm.root(), PaneNode::Leaf { id } if *id == PaneId(0)));
    }

    #[test]
    fn test_leaf_collect_ids() {
        let node = PaneNode::leaf(PaneId(0));
        assert_eq!(node.collect_ids(), vec![PaneId(0)]);
    }

    #[test]
    fn test_split_creates_two_leaves() {
        let mut pm = PaneManager::new();
        let new_id = pm.split_focused(SplitDirection::Vertical, 0.5);
        assert_eq!(new_id, PaneId(1));
        assert_eq!(pm.pane_count(), 2);

        let ids = pm.root().collect_ids();
        assert!(ids.contains(&PaneId(0)));
        assert!(ids.contains(&PaneId(1)));
    }

    #[test]
    fn test_split_tree_structure() {
        let mut pm = PaneManager::new();
        pm.split_focused(SplitDirection::Vertical, 0.5);
        match pm.root() {
            PaneNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                assert_eq!(*direction, SplitDirection::Vertical);
                assert_eq!(*ratio, 0.5);
                assert!(matches!(first.as_ref(), PaneNode::Leaf { id } if *id == PaneId(0)));
                assert!(matches!(second.as_ref(), PaneNode::Leaf { id } if *id == PaneId(1)));
            }
            _ => panic!("expected Split"),
        }
    }

    #[test]
    fn test_nested_split() {
        let mut pm = PaneManager::new();
        pm.split_focused(SplitDirection::Vertical, 0.5);
        // Focus remains on PaneId(0), split it again
        pm.split_focused(SplitDirection::Horizontal, 0.3);
        assert_eq!(pm.pane_count(), 3);

        let ids = pm.root().collect_ids();
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn test_focus_switch() {
        let mut pm = PaneManager::new();
        pm.split_focused(SplitDirection::Vertical, 0.5);

        pm.focus(PaneId(1));
        assert_eq!(pm.focused(), PaneId(1));
        assert_eq!(pm.focus_history(), &[PaneId(0)]);
    }

    #[test]
    fn test_focus_same_pane_noop() {
        let mut pm = PaneManager::new();
        pm.focus(PaneId(0));
        assert!(pm.focus_history().is_empty());
    }

    #[test]
    fn test_focus_nonexistent_pane_noop() {
        let mut pm = PaneManager::new();
        pm.focus(PaneId(99));
        assert_eq!(pm.focused(), PaneId(0));
        assert!(pm.focus_history().is_empty());
    }

    #[test]
    fn test_focus_previous() {
        let mut pm = PaneManager::new();
        pm.split_focused(SplitDirection::Vertical, 0.5);

        pm.focus(PaneId(1));
        assert_eq!(pm.focused(), PaneId(1));

        pm.focus_previous();
        assert_eq!(pm.focused(), PaneId(0));
    }

    #[test]
    fn test_close_pane() {
        let mut pm = PaneManager::new();
        pm.split_focused(SplitDirection::Vertical, 0.5);
        assert_eq!(pm.pane_count(), 2);

        assert!(pm.close(PaneId(1)));
        assert_eq!(pm.pane_count(), 1);
        assert!(matches!(pm.root(), PaneNode::Leaf { id } if *id == PaneId(0)));
    }

    #[test]
    fn test_close_focused_pane_switches_focus() {
        let mut pm = PaneManager::new();
        pm.split_focused(SplitDirection::Vertical, 0.5);
        pm.focus(PaneId(1));

        assert!(pm.close(PaneId(1)));
        assert_eq!(pm.focused(), PaneId(0));
    }

    #[test]
    fn test_cannot_close_last_pane() {
        let mut pm = PaneManager::new();
        assert!(!pm.close(PaneId(0)));
        assert_eq!(pm.pane_count(), 1);
    }

    #[test]
    fn test_close_nonexistent_pane() {
        let mut pm = PaneManager::new();
        assert!(!pm.close(PaneId(99)));
    }

    #[test]
    fn test_node_find() {
        let mut pm = PaneManager::new();
        pm.split_focused(SplitDirection::Vertical, 0.5);

        assert!(pm.root().find(PaneId(0)).is_some());
        assert!(pm.root().find(PaneId(1)).is_some());
        assert!(pm.root().find(PaneId(99)).is_none());
    }

    #[test]
    fn test_node_remove_first_child() {
        let mut node = PaneNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(PaneNode::leaf(PaneId(0))),
            second: Box::new(PaneNode::leaf(PaneId(1))),
        };
        assert!(node.remove(PaneId(0)));
        assert!(matches!(node, PaneNode::Leaf { id } if id == PaneId(1)));
    }

    #[test]
    fn test_node_remove_second_child() {
        let mut node = PaneNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(PaneNode::leaf(PaneId(0))),
            second: Box::new(PaneNode::leaf(PaneId(1))),
        };
        assert!(node.remove(PaneId(1)));
        assert!(matches!(node, PaneNode::Leaf { id } if id == PaneId(0)));
    }

    #[test]
    fn test_compute_rects_single_pane() {
        let pm = PaneManager::new();
        let total = Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        };
        let rects = pm.compute_rects(total);
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[&PaneId(0)], total);
    }

    #[test]
    fn test_compute_rects_vertical_split() {
        let mut pm = PaneManager::new();
        pm.split_focused(SplitDirection::Vertical, 0.5);
        let total = Rect {
            x: 0,
            y: 0,
            w: 81, // 81 = 40 + 1 divider + 40
            h: 24,
        };
        let rects = pm.compute_rects(total);
        assert_eq!(rects.len(), 2);

        let r0 = rects[&PaneId(0)];
        let r1 = rects[&PaneId(1)];
        assert_eq!(r0.x, 0);
        assert_eq!(r0.w, 40);
        assert_eq!(r1.x, 41); // 40 + 1 divider
        assert_eq!(r1.w, 40);
        assert_eq!(r0.h, 24);
        assert_eq!(r1.h, 24);
    }

    #[test]
    fn test_compute_rects_horizontal_split() {
        let mut pm = PaneManager::new();
        pm.split_focused(SplitDirection::Horizontal, 0.5);
        let total = Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 25, // 25 = 12 + 1 divider + 12
        };
        let rects = pm.compute_rects(total);
        assert_eq!(rects.len(), 2);

        let r0 = rects[&PaneId(0)];
        let r1 = rects[&PaneId(1)];
        assert_eq!(r0.y, 0);
        assert_eq!(r0.h, 12);
        assert_eq!(r1.y, 13); // 12 + 1 divider
        assert_eq!(r1.h, 12);
        assert_eq!(r0.w, 80);
        assert_eq!(r1.w, 80);
    }

    #[test]
    fn test_pane_at() {
        let mut pm = PaneManager::new();
        pm.split_focused(SplitDirection::Vertical, 0.5);
        let total = Rect {
            x: 0,
            y: 0,
            w: 81,
            h: 24,
        };

        assert_eq!(pm.pane_at(0, 0, total), Some(PaneId(0)));
        assert_eq!(pm.pane_at(39, 12, total), Some(PaneId(0)));
        assert_eq!(pm.pane_at(41, 0, total), Some(PaneId(1)));
        assert_eq!(pm.pane_at(80, 23, total), Some(PaneId(1)));
        // On the divider (col 40) — not inside any pane
        assert_eq!(pm.pane_at(40, 12, total), None);
    }

    #[test]
    fn test_context_for() {
        let mut pm = PaneManager::new();
        pm.split_focused(SplitDirection::Vertical, 0.5);
        let total = Rect {
            x: 0,
            y: 0,
            w: 81,
            h: 24,
        };

        let ctx = pm.context_for(PaneId(0), total).unwrap();
        assert_eq!(ctx.pane_id, PaneId(0));
        assert_eq!(ctx.pane_count, 2);
        assert!(ctx.is_focused); // PaneId(0) is focused by default
        assert_eq!(ctx.pane_rect.w, 40);

        let ctx1 = pm.context_for(PaneId(1), total).unwrap();
        assert!(!ctx1.is_focused);
    }

    #[test]
    fn test_tabs_node() {
        let node = PaneNode::Tabs {
            tabs: vec![PaneNode::leaf(PaneId(0)), PaneNode::leaf(PaneId(1))],
            active: 0,
            labels: vec!["tab1".into(), "tab2".into()],
        };
        assert_eq!(node.leaf_count(), 2);
        assert_eq!(node.collect_ids(), vec![PaneId(0), PaneId(1)]);
        assert!(node.find(PaneId(0)).is_some());
        assert!(node.find(PaneId(1)).is_some());
    }

    #[test]
    fn test_tabs_remove_collapses() {
        let mut node = PaneNode::Tabs {
            tabs: vec![PaneNode::leaf(PaneId(0)), PaneNode::leaf(PaneId(1))],
            active: 0,
            labels: vec!["tab1".into(), "tab2".into()],
        };
        assert!(node.remove(PaneId(0)));
        // Should collapse to a single leaf
        assert!(matches!(node, PaneNode::Leaf { id } if id == PaneId(1)));
    }

    #[test]
    fn test_tabs_compute_rects() {
        let node = PaneNode::Tabs {
            tabs: vec![PaneNode::leaf(PaneId(0)), PaneNode::leaf(PaneId(1))],
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
        // Only active tab gets a rect
        assert_eq!(rects.len(), 1);
        let r = rects[&PaneId(0)];
        assert_eq!(r.y, 1); // tab bar takes 1 row
        assert_eq!(r.h, 23);
    }

    #[test]
    fn test_next_pane_id_increments() {
        let mut pm = PaneManager::new();
        assert_eq!(pm.next_pane_id(), PaneId(1));
        assert_eq!(pm.next_pane_id(), PaneId(2));
        assert_eq!(pm.next_pane_id(), PaneId(3));
    }

    #[test]
    fn test_close_cleans_focus_history() {
        let mut pm = PaneManager::new();
        pm.split_focused(SplitDirection::Vertical, 0.5);
        let id2 = pm.split_focused(SplitDirection::Horizontal, 0.5);

        // Build up focus history: 0 → 1 → 2
        pm.focus(PaneId(1));
        pm.focus(id2);

        // Close PaneId(1), should be removed from history
        pm.close(PaneId(1));
        assert!(!pm.focus_history().contains(&PaneId(1)));
    }

    #[test]
    fn test_pane_permissions() {
        let empty = PanePermissions::empty();
        assert!(!empty.contains(PanePermissions::SPLIT));
        assert_eq!(empty.bits(), 0);

        let all = PanePermissions::all();
        assert!(all.contains(PanePermissions::SPLIT));
        assert!(all.contains(PanePermissions::FOCUS));
        assert!(all.contains(PanePermissions::TABS));

        let custom = PanePermissions::from_bits(PanePermissions::SPLIT | PanePermissions::FOCUS);
        assert!(custom.contains(PanePermissions::SPLIT));
        assert!(custom.contains(PanePermissions::FOCUS));
        assert!(!custom.contains(PanePermissions::SPAWN));
    }

    #[test]
    fn test_split_rect_vertical() {
        let area = Rect {
            x: 0,
            y: 0,
            w: 81,
            h: 24,
        };
        let (a, b) = split_rect(area, SplitDirection::Vertical, 0.5);
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
        let (a, b) = split_rect(area, SplitDirection::Horizontal, 0.5);
        assert_eq!(a.h, 12);
        assert_eq!(b.h, 12);
        assert_eq!(a.y, 0);
        assert_eq!(b.y, 13);
    }
}
