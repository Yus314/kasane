//! Workspace tree node types and pure tree operations.

use std::collections::HashMap;

use crate::layout::{Rect, SplitDirection};
use crate::surface::SurfaceId;

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

    /// Apply a function to all direct children (including floating entries).
    fn for_each_child(&self, f: &mut impl FnMut(&WorkspaceNode)) {
        match self {
            WorkspaceNode::Leaf { .. } => {}
            WorkspaceNode::Split { first, second, .. } => {
                f(first);
                f(second);
            }
            WorkspaceNode::Tabs { tabs, .. } => {
                for tab in tabs {
                    f(tab);
                }
            }
            WorkspaceNode::Float { base, floating } => {
                f(base);
                for entry in floating {
                    f(&entry.node);
                }
            }
        }
    }

    /// Apply a function to all direct children mutably (including floating entries).
    fn for_each_child_mut(&mut self, f: &mut impl FnMut(&mut WorkspaceNode)) {
        match self {
            WorkspaceNode::Leaf { .. } => {}
            WorkspaceNode::Split { first, second, .. } => {
                f(first);
                f(second);
            }
            WorkspaceNode::Tabs { tabs, .. } => {
                for tab in tabs {
                    f(tab);
                }
            }
            WorkspaceNode::Float { base, floating } => {
                f(base);
                for entry in floating {
                    f(&mut entry.node);
                }
            }
        }
    }

    /// Test whether any direct child satisfies a predicate (including floating entries).
    #[allow(dead_code)]
    fn any_child(&self, f: &mut impl FnMut(&WorkspaceNode) -> bool) -> bool {
        match self {
            WorkspaceNode::Leaf { .. } => false,
            WorkspaceNode::Split { first, second, .. } => f(first) || f(second),
            WorkspaceNode::Tabs { tabs, .. } => tabs.iter().any(&mut *f),
            WorkspaceNode::Float { base, floating } => {
                f(base) || floating.iter().any(|e| f(&e.node))
            }
        }
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
        if let WorkspaceNode::Leaf { surface_id } = self {
            ids.push(*surface_id);
        } else {
            self.for_each_child(&mut |child| child.collect_ids_inner(ids));
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

    pub(crate) fn split_with_side(
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
        if matches!(self, WorkspaceNode::Leaf { .. }) {
            return 1;
        }
        let mut sum = 0;
        self.for_each_child(&mut |child| sum += child.leaf_count());
        sum
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

    pub(crate) fn compute_dividers_inner(
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

    pub(crate) fn set_divider_ratio(
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
    pub(crate) fn resize_target(&mut self, target: SurfaceId, delta: f32) -> bool {
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

    /// Adjust the nearest ancestor split ratio for `target`, but only at
    /// splits whose direction matches `dir`.
    ///
    /// Positive `delta` grows the subtree containing `target`.
    pub(crate) fn resize_direction_target(
        &mut self,
        target: SurfaceId,
        dir: SplitDirection,
        delta: f32,
    ) -> bool {
        match self {
            WorkspaceNode::Leaf { .. } => false,
            WorkspaceNode::Split {
                direction,
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

                if in_first && first.resize_direction_target(target, dir, delta) {
                    return true;
                }
                if in_second && second.resize_direction_target(target, dir, delta) {
                    return true;
                }

                if *direction != dir {
                    return false;
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
                    return active_tab.resize_direction_target(target, dir, delta);
                }
                false
            }
            WorkspaceNode::Float { base, .. } => {
                if base.find(target).is_some() {
                    base.resize_direction_target(target, dir, delta)
                } else {
                    false
                }
            }
        }
    }

    /// Swap the positions of two leaf surfaces.
    pub(crate) fn swap_leaf_ids(&mut self, a: SurfaceId, b: SurfaceId) {
        if let WorkspaceNode::Leaf { surface_id } = self {
            if *surface_id == a {
                *surface_id = b;
            } else if *surface_id == b {
                *surface_id = a;
            }
        } else {
            self.for_each_child_mut(&mut |child| child.swap_leaf_ids(a, b));
        }
    }

    pub(crate) fn first_leaf_id(&self) -> Option<SurfaceId> {
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

    /// Check whether `target` is on the trailing edge of this subtree
    /// for the given split direction.
    ///
    /// For a vertical split, "trailing" = rightmost column.
    /// For a horizontal split, "trailing" = bottommost row.
    ///
    /// When a split has the same direction, only the `second` child is on the
    /// trailing edge. When a split has the cross direction, both children
    /// are on the trailing edge (they stack along the other axis).
    pub(crate) fn has_on_trailing_edge(&self, target: SurfaceId, dir: SplitDirection) -> bool {
        match self {
            WorkspaceNode::Leaf { surface_id } => *surface_id == target,
            WorkspaceNode::Split {
                direction,
                first,
                second,
                ..
            } => {
                if *direction == dir {
                    // Same direction: only `second` is on the trailing edge
                    second.has_on_trailing_edge(target, dir)
                } else {
                    // Cross direction: both children share the trailing edge
                    first.has_on_trailing_edge(target, dir)
                        || second.has_on_trailing_edge(target, dir)
                }
            }
            WorkspaceNode::Tabs { tabs, active, .. } => tabs
                .get(*active)
                .is_some_and(|tab| tab.has_on_trailing_edge(target, dir)),
            WorkspaceNode::Float { base, .. } => base.has_on_trailing_edge(target, dir),
        }
    }

    /// Check whether `target` is on the leading edge of this subtree
    /// for the given split direction.
    ///
    /// Mirror of [`has_on_trailing_edge`]: for a vertical split, "leading" =
    /// leftmost column; for horizontal, "leading" = topmost row.
    pub(crate) fn has_on_leading_edge(&self, target: SurfaceId, dir: SplitDirection) -> bool {
        match self {
            WorkspaceNode::Leaf { surface_id } => *surface_id == target,
            WorkspaceNode::Split {
                direction,
                first,
                second,
                ..
            } => {
                if *direction == dir {
                    // Same direction: only `first` is on the leading edge
                    first.has_on_leading_edge(target, dir)
                } else {
                    // Cross direction: both children share the leading edge
                    first.has_on_leading_edge(target, dir)
                        || second.has_on_leading_edge(target, dir)
                }
            }
            WorkspaceNode::Tabs { tabs, active, .. } => tabs
                .get(*active)
                .is_some_and(|tab| tab.has_on_leading_edge(target, dir)),
            WorkspaceNode::Float { base, .. } => base.has_on_leading_edge(target, dir),
        }
    }

    pub(crate) fn capture_restore_placement(&self, target: SurfaceId) -> Option<RestorePlacement> {
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
