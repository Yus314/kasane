use std::collections::{HashMap, HashSet};

use compact_str::CompactString;

use crate::element::{Direction, Element, FlexChild, ResolvedSlotInstanceId};
use crate::layout::Rect;
use crate::layout::flex::{self, Constraints, LayoutResult};
use crate::plugin::{
    AppView, ContribSizeHint, ContributeContext, Contribution, PaneContext, PluginView, SlotId,
};
use crate::state::AppState;

use super::SurfaceDescriptor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedSlotContentKind {
    Empty,
    Single,
    Aggregate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSlotRecord {
    pub surface_key: CompactString,
    pub slot_name: CompactString,
    pub instance_id: ResolvedSlotInstanceId,
    pub direction: Direction,
    pub gap: u16,
    pub contribution_count: usize,
    pub content_kind: ResolvedSlotContentKind,
    pub area: Option<Rect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnerValidationErrorKind {
    UnexpectedResolvedSlot,
    UnresolvedSlotPlaceholder,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerValidationError {
    pub surface_key: CompactString,
    pub kind: OwnerValidationErrorKind,
    pub detail: CompactString,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContributorIssueKind {
    UnexpectedResolvedSlot,
    UnresolvedSlotPlaceholder,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContributorIssue {
    pub surface_key: CompactString,
    pub slot_name: CompactString,
    pub slot_instance_id: Option<ResolvedSlotInstanceId>,
    pub contributor: CompactString,
    pub kind: ContributorIssueKind,
    pub detail: CompactString,
}

#[derive(Debug, Clone)]
pub struct ResolvedTree {
    root: Element,
    pub slot_records: Vec<ResolvedSlotRecord>,
}

impl ResolvedTree {
    pub fn new(
        root: Element,
        slot_records: Vec<ResolvedSlotRecord>,
    ) -> Result<Self, CompactString> {
        if contains_slot_placeholder(&root) {
            return Err("resolved tree still contains SlotPlaceholder".into());
        }
        Ok(Self { root, slot_records })
    }

    /// Access the root element of the resolved tree.
    pub fn root(&self) -> &Element {
        &self.root
    }

    /// Consume the tree and return the root element.
    pub fn into_root(self) -> Element {
        self.root
    }
}

#[derive(Debug, Clone, Default)]
pub struct SurfaceRenderReport {
    pub surface_key: CompactString,
    pub slot_records: Vec<ResolvedSlotRecord>,
    pub absent_declared_slots: Vec<CompactString>,
    pub owner_errors: Vec<OwnerValidationError>,
    pub contributor_issues: Vec<ContributorIssue>,
}

impl SurfaceRenderReport {
    pub fn new(surface_key: impl Into<CompactString>) -> Self {
        Self {
            surface_key: surface_key.into(),
            ..Self::default()
        }
    }

    pub fn from_descriptor(descriptor: &SurfaceDescriptor) -> Self {
        Self::new(descriptor.surface_key.clone())
    }
}

#[derive(Debug, Clone)]
pub struct SurfaceRenderOutcome {
    pub tree: Option<ResolvedTree>,
    pub report: SurfaceRenderReport,
}

#[derive(Debug, Clone, Default)]
pub struct SurfaceComposeResult {
    pub base: Option<Element>,
    pub surface_reports: Vec<SurfaceRenderReport>,
}

/// A slot placeholder discovered during the scan stage.
#[derive(Debug, Clone)]
pub struct ScannedSlot {
    pub name: CompactString,
    pub direction: Direction,
    pub gap: u16,
    /// Constraints at the slot's position in the element tree.
    pub constraints: Constraints,
}

/// Output of Stage A: element tree slot scan results.
#[derive(Debug)]
pub struct SlotPlan {
    /// Valid slots discovered in tree-traversal order.
    pub slots: Vec<ScannedSlot>,
    /// Owner validation errors from the scan.
    pub owner_errors: Vec<OwnerValidationError>,
    /// Names of slot placeholders that were seen and validated.
    pub seen_names: HashSet<CompactString>,
}

/// Pre-collected data for a single resolved slot.
#[derive(Debug)]
pub struct CollectedSlotData {
    pub instance_id: ResolvedSlotInstanceId,
    pub surface_key: CompactString,
    pub slot_name: CompactString,
    pub direction: Direction,
    pub gap: u16,
    pub children: Vec<FlexChild>,
}

/// Output of Stage B: collected contributions for all slots.
#[derive(Debug)]
pub struct CollectedSlots {
    pub slots: HashMap<CompactString, CollectedSlotData>,
    pub slot_records: Vec<ResolvedSlotRecord>,
    pub contributor_issues: Vec<ContributorIssue>,
}

pub fn resolve_surface_tree(
    descriptor: &SurfaceDescriptor,
    root: Element,
    state: &AppState,
    registry: &PluginView<'_>,
    rect: Rect,
    pane_context: PaneContext,
) -> SurfaceRenderOutcome {
    let root_constraints = Constraints::tight(rect.w, rect.h);

    // Stage A: scan for slot placeholders
    let plan = scan_slots(descriptor, &root, root_constraints, state);

    // Stage B: collect contributions for each slot
    let mut collected =
        collect_slot_contributions(&plan, descriptor, state, registry, pane_context);

    // Stage C: substitute placeholders with collected contributions
    let resolved_root = substitute_slots(root, &mut collected.slots);

    // Assemble report
    let mut report = SurfaceRenderReport::from_descriptor(descriptor);
    report.slot_records = collected.slot_records.clone();
    report.contributor_issues = collected.contributor_issues;
    report.owner_errors = plan.owner_errors;
    report.absent_declared_slots = descriptor
        .declared_slots
        .iter()
        .filter(|slot| !plan.seen_names.contains(slot.name.as_str()))
        .map(|slot| slot.name.clone())
        .collect();

    let tree = if report.owner_errors.is_empty() {
        match ResolvedTree::new(resolved_root, collected.slot_records) {
            Ok(tree) => Some(tree),
            Err(detail) => {
                report.owner_errors.push(OwnerValidationError {
                    surface_key: descriptor.surface_key.clone(),
                    kind: OwnerValidationErrorKind::UnresolvedSlotPlaceholder,
                    detail,
                });
                None
            }
        }
    } else {
        None
    };

    SurfaceRenderOutcome { tree, report }
}

/// Stage A: Scan the element tree for slot placeholders.
///
/// Walks the tree computing constraints at each position and validates
/// slot names against the surface descriptor. Pure function (no plugin queries).
pub fn scan_slots(
    descriptor: &SurfaceDescriptor,
    root: &Element,
    root_constraints: Constraints,
    state: &AppState,
) -> SlotPlan {
    let mut scanner = SlotScanner {
        descriptor,
        state,
        seen_names: HashSet::new(),
        slots: Vec::new(),
        owner_errors: Vec::new(),
    };
    scanner.scan(root, PlaceholderPolicy::Allowed, root_constraints);
    SlotPlan {
        slots: scanner.slots,
        owner_errors: scanner.owner_errors,
        seen_names: scanner.seen_names,
    }
}

/// Stage B: Collect contributions for each scanned slot.
///
/// Queries plugins for each valid slot, allocates instance IDs,
/// and validates contribution trees.
pub fn collect_slot_contributions(
    plan: &SlotPlan,
    descriptor: &SurfaceDescriptor,
    state: &AppState,
    registry: &PluginView<'_>,
    pane_context: PaneContext,
) -> CollectedSlots {
    let mut next_instance_id: u64 = 1;
    let mut slots = HashMap::new();
    let mut slot_records = Vec::new();
    let mut contributor_issues = Vec::new();

    for scanned in &plan.slots {
        let slot_id = SlotId::new(scanned.name.clone());
        let ctx = ContributeContext::from_constraints_in_pane(
            &AppView::new(state),
            scanned.constraints,
            pane_context,
        );
        let sourced =
            registry.collect_contributions_with_sources(&slot_id, &AppView::new(state), &ctx);

        let instance_id = ResolvedSlotInstanceId(next_instance_id);
        next_instance_id += 1;

        let mut children = Vec::new();
        for sc in sourced {
            match validate_contribution_tree(&sc.contribution.element) {
                Ok(()) => {
                    children.push(contribution_to_flex_child(sc.contribution));
                }
                Err((kind, detail)) => {
                    contributor_issues.push(ContributorIssue {
                        surface_key: descriptor.surface_key.clone(),
                        slot_name: scanned.name.clone(),
                        slot_instance_id: Some(instance_id),
                        contributor: sc.contributor.0.into(),
                        kind,
                        detail,
                    });
                }
            }
        }

        let content_kind = match children.len() {
            0 => ResolvedSlotContentKind::Empty,
            1 => ResolvedSlotContentKind::Single,
            _ => ResolvedSlotContentKind::Aggregate,
        };

        slot_records.push(ResolvedSlotRecord {
            surface_key: descriptor.surface_key.clone(),
            slot_name: scanned.name.clone(),
            instance_id,
            direction: scanned.direction,
            gap: scanned.gap,
            contribution_count: children.len(),
            content_kind,
            area: None,
        });

        slots.insert(
            scanned.name.clone(),
            CollectedSlotData {
                instance_id,
                surface_key: descriptor.surface_key.clone(),
                slot_name: scanned.name.clone(),
                direction: scanned.direction,
                gap: scanned.gap,
                children,
            },
        );
    }

    CollectedSlots {
        slots,
        slot_records,
        contributor_issues,
    }
}

pub fn backfill_surface_report_areas(
    reports: &mut [SurfaceRenderReport],
    element: &Element,
    layout: &LayoutResult,
) {
    let mut areas = HashMap::new();
    collect_resolved_slot_areas(element, layout, &mut areas);
    for report in reports {
        for record in &mut report.slot_records {
            record.area = areas
                .get(&(report.surface_key.clone(), record.instance_id))
                .copied();
        }
    }
}

fn collect_resolved_slot_areas(
    element: &Element,
    layout: &LayoutResult,
    areas: &mut HashMap<(CompactString, ResolvedSlotInstanceId), Rect>,
) {
    match element {
        Element::ResolvedSlot {
            surface_key,
            instance_id,
            children,
            ..
        } => {
            areas.insert((surface_key.clone(), *instance_id), layout.area);
            for (child, child_layout) in children.iter().zip(layout.children.iter()) {
                collect_resolved_slot_areas(&child.element, child_layout, areas);
            }
        }
        Element::Flex { children, .. } => {
            for (child, child_layout) in children.iter().zip(layout.children.iter()) {
                collect_resolved_slot_areas(&child.element, child_layout, areas);
            }
        }
        Element::Stack { base, overlays } => {
            if let Some(base_layout) = layout.children.first() {
                collect_resolved_slot_areas(base, base_layout, areas);
            }
            for (overlay, overlay_layout) in overlays.iter().zip(layout.children.iter().skip(1)) {
                collect_resolved_slot_areas(&overlay.element, overlay_layout, areas);
            }
        }
        Element::Scrollable { child, .. }
        | Element::Container { child, .. }
        | Element::Interactive { child, .. } => {
            if let Some(child_layout) = layout.children.first() {
                collect_resolved_slot_areas(child, child_layout, areas);
            }
        }
        Element::Grid { children, .. } => {
            for (child, child_layout) in children.iter().zip(layout.children.iter()) {
                collect_resolved_slot_areas(child, child_layout, areas);
            }
        }
        Element::Text(..)
        | Element::StyledLine(..)
        | Element::SlotPlaceholder { .. }
        | Element::Image { .. }
        | Element::Empty
        | Element::BufferRef { .. } => {}
    }
}

fn contains_slot_placeholder(element: &Element) -> bool {
    match element {
        Element::SlotPlaceholder { .. } => true,
        Element::ResolvedSlot { children, .. } | Element::Flex { children, .. } => children
            .iter()
            .any(|child| contains_slot_placeholder(&child.element)),
        Element::Stack { base, overlays } => {
            contains_slot_placeholder(base)
                || overlays
                    .iter()
                    .any(|overlay| contains_slot_placeholder(&overlay.element))
        }
        Element::Scrollable { child, .. }
        | Element::Container { child, .. }
        | Element::Interactive { child, .. } => contains_slot_placeholder(child),
        Element::Grid { children, .. } => children.iter().any(contains_slot_placeholder),
        Element::Text(..)
        | Element::StyledLine(..)
        | Element::Image { .. }
        | Element::Empty
        | Element::BufferRef { .. } => false,
    }
}

#[derive(Clone, Copy)]
enum PlaceholderPolicy {
    Allowed,
    ForbiddenInGrid,
}

struct SlotScanner<'a> {
    descriptor: &'a SurfaceDescriptor,
    state: &'a AppState,
    seen_names: HashSet<CompactString>,
    slots: Vec<ScannedSlot>,
    owner_errors: Vec<OwnerValidationError>,
}

impl SlotScanner<'_> {
    fn scan(&mut self, element: &Element, policy: PlaceholderPolicy, constraints: Constraints) {
        match element {
            Element::SlotPlaceholder {
                slot_name,
                direction,
                gap,
            } => self.scan_placeholder(slot_name, *direction, *gap, policy, constraints),
            Element::ResolvedSlot { slot_name, .. } => {
                self.owner_errors.push(OwnerValidationError {
                    surface_key: self.descriptor.surface_key.clone(),
                    kind: OwnerValidationErrorKind::UnexpectedResolvedSlot,
                    detail: format!("surface input tree contains resolved slot {slot_name}").into(),
                });
            }
            Element::Flex {
                direction,
                children,
                gap,
                ..
            } => self.scan_flex_children(*direction, children, *gap, policy, constraints),
            Element::Stack { base, overlays } => {
                self.scan(base, policy, constraints);
                for overlay in overlays {
                    self.scan(&overlay.element, policy, constraints);
                }
            }
            Element::Scrollable {
                child, direction, ..
            } => self.scan(
                child,
                policy,
                scroll_child_constraints(*direction, constraints),
            ),
            Element::Container {
                child,
                border,
                padding,
                ..
            } => self.scan(
                child,
                policy,
                container_child_constraints(border.is_some(), *padding, constraints),
            ),
            Element::Interactive { child, .. } => self.scan(child, policy, constraints),
            Element::Grid { children, .. } => {
                for child in children {
                    self.scan(child, PlaceholderPolicy::ForbiddenInGrid, constraints);
                }
            }
            Element::Text(..)
            | Element::StyledLine(..)
            | Element::Image { .. }
            | Element::Empty
            | Element::BufferRef { .. } => {}
        }
    }

    fn scan_placeholder(
        &mut self,
        slot_name: &CompactString,
        direction: Direction,
        gap: u16,
        policy: PlaceholderPolicy,
        constraints: Constraints,
    ) {
        if matches!(policy, PlaceholderPolicy::ForbiddenInGrid) {
            self.owner_errors.push(OwnerValidationError {
                surface_key: self.descriptor.surface_key.clone(),
                kind: OwnerValidationErrorKind::UnresolvedSlotPlaceholder,
                detail: format!("slot placeholder {slot_name} is not supported inside Grid").into(),
            });
            return;
        }

        let slot = if let Some(slot) = self.descriptor.declared_slot(slot_name.as_str()) {
            slot
        } else {
            self.owner_errors.push(OwnerValidationError {
                surface_key: self.descriptor.surface_key.clone(),
                kind: OwnerValidationErrorKind::UnresolvedSlotPlaceholder,
                detail: format!("undeclared slot placeholder {slot_name}").into(),
            });
            return;
        };

        if !self.seen_names.insert(slot.name.clone()) {
            self.owner_errors.push(OwnerValidationError {
                surface_key: self.descriptor.surface_key.clone(),
                kind: OwnerValidationErrorKind::UnresolvedSlotPlaceholder,
                detail: format!("duplicate slot placeholder {}", slot.name).into(),
            });
            return;
        }

        self.slots.push(ScannedSlot {
            name: slot.name.clone(),
            direction,
            gap,
            constraints,
        });
    }

    fn scan_flex_children(
        &mut self,
        direction: Direction,
        children: &[FlexChild],
        gap: u16,
        policy: PlaceholderPolicy,
        constraints: Constraints,
    ) {
        if children.is_empty() {
            return;
        }

        let constraint_size = flex::Size {
            width: constraints.max_width,
            height: constraints.max_height,
        };
        let main_total = direction.main(constraint_size);
        let cross_total = direction.cross(constraint_size);
        let total_gaps = if children.len() > 1 {
            gap * (children.len() as u16 - 1)
        } else {
            0
        };

        let mut total_fixed = 0u16;
        let mut total_flex = 0.0f32;
        let mut pending_flex: Vec<(usize, f32, Option<u16>, Option<u16>)> = Vec::new();

        for (index, child) in children.iter().enumerate() {
            if child.flex > 0.0 {
                total_flex += child.flex;
                pending_flex.push((index, child.flex, child.min_size, child.max_size));
                continue;
            }

            let child_constraints = flex_child_constraints(direction, main_total, cross_total);
            if !contains_slot_placeholder(&child.element) {
                let size = flex::measure(&child.element, child_constraints, self.state);
                let (main, _) = direction.decompose(size);
                total_fixed += apply_child_bounds(main, child.min_size, child.max_size);
            }
            self.scan(&child.element, policy, child_constraints);
        }

        if total_flex > 0.0 {
            let remaining = main_total.saturating_sub(total_fixed + total_gaps);
            let flex_count = pending_flex.len();
            let mut distributed = 0u16;
            for (i, &(index, child_flex, min_size, max_size)) in pending_flex.iter().enumerate() {
                let share = if i + 1 == flex_count {
                    remaining.saturating_sub(distributed)
                } else {
                    (remaining as f32 * child_flex / total_flex) as u16
                };
                let share = apply_child_bounds(share, min_size, max_size);
                distributed = distributed.saturating_add(share);
                let child_constraints = flex_child_constraints(direction, share, cross_total);
                self.scan(&children[index].element, policy, child_constraints);
            }
        }
    }
}

/// Stage C: Substitute slot placeholders with collected contributions.
fn substitute_slots(
    element: Element,
    slots: &mut HashMap<CompactString, CollectedSlotData>,
) -> Element {
    match element {
        Element::SlotPlaceholder { slot_name, .. } => {
            if let Some(data) = slots.remove(&slot_name) {
                Element::ResolvedSlot {
                    surface_key: data.surface_key,
                    slot_name,
                    instance_id: data.instance_id,
                    direction: data.direction,
                    children: data.children,
                    gap: data.gap,
                }
            } else {
                Element::Empty
            }
        }
        Element::ResolvedSlot { .. } => Element::Empty,
        Element::Flex {
            direction,
            children,
            gap,
            align,
            cross_align,
        } => Element::Flex {
            direction,
            children: children
                .into_iter()
                .map(|child| FlexChild {
                    element: substitute_slots(child.element, slots),
                    ..child
                })
                .collect(),
            gap,
            align,
            cross_align,
        },
        Element::Stack { base, overlays } => Element::Stack {
            base: Box::new(substitute_slots(*base, slots)),
            overlays: overlays
                .into_iter()
                .map(|overlay| crate::element::Overlay {
                    element: substitute_slots(overlay.element, slots),
                    anchor: overlay.anchor,
                })
                .collect(),
        },
        Element::Scrollable {
            child,
            offset,
            direction,
        } => Element::Scrollable {
            child: Box::new(substitute_slots(*child, slots)),
            offset,
            direction,
        },
        Element::Container {
            child,
            border,
            shadow,
            padding,
            style,
            title,
        } => Element::Container {
            child: Box::new(substitute_slots(*child, slots)),
            border,
            shadow,
            padding,
            style,
            title,
        },
        Element::Interactive { child, id } => Element::Interactive {
            child: Box::new(substitute_slots(*child, slots)),
            id,
        },
        Element::Grid {
            columns,
            children,
            col_gap,
            row_gap,
            align,
            cross_align,
        } => Element::Grid {
            columns,
            children: children
                .into_iter()
                .map(|child| substitute_slots(child, slots))
                .collect(),
            col_gap,
            row_gap,
            align,
            cross_align,
        },
        Element::Text(..)
        | Element::StyledLine(..)
        | Element::Image { .. }
        | Element::Empty
        | Element::BufferRef { .. } => element,
    }
}

fn validate_contribution_tree(
    element: &Element,
) -> Result<(), (ContributorIssueKind, CompactString)> {
    match element {
        Element::SlotPlaceholder { slot_name, .. } => Err((
            ContributorIssueKind::UnresolvedSlotPlaceholder,
            format!("contribution contains unresolved slot placeholder {slot_name}").into(),
        )),
        Element::ResolvedSlot { slot_name, .. } => Err((
            ContributorIssueKind::UnexpectedResolvedSlot,
            format!("contribution contains resolved slot {slot_name}").into(),
        )),
        Element::Flex { children, .. } => {
            for child in children {
                validate_contribution_tree(&child.element)?;
            }
            Ok(())
        }
        Element::Stack { base, overlays } => {
            validate_contribution_tree(base)?;
            for overlay in overlays {
                validate_contribution_tree(&overlay.element)?;
            }
            Ok(())
        }
        Element::Scrollable { child, .. }
        | Element::Container { child, .. }
        | Element::Interactive { child, .. } => validate_contribution_tree(child),
        Element::Grid { children, .. } => {
            for child in children {
                validate_contribution_tree(child)?;
            }
            Ok(())
        }
        Element::Text(..)
        | Element::StyledLine(..)
        | Element::Image { .. }
        | Element::Empty
        | Element::BufferRef { .. } => Ok(()),
    }
}

fn contribution_to_flex_child(contribution: Contribution) -> FlexChild {
    match contribution.size_hint {
        ContribSizeHint::Auto => FlexChild::fixed(contribution.element),
        ContribSizeHint::Fixed(n) => FlexChild {
            element: contribution.element,
            flex: 0.0,
            min_size: Some(n),
            max_size: Some(n),
        },
        ContribSizeHint::Flex(flex) => FlexChild::flexible(contribution.element, flex),
    }
}

fn container_child_constraints(
    has_border: bool,
    padding: crate::element::Edges,
    constraints: Constraints,
) -> Constraints {
    let border_size = if has_border { 2 } else { 0 };
    let extra_w = padding.horizontal() + border_size;
    let extra_h = padding.vertical() + border_size;
    Constraints {
        min_width: constraints.min_width.saturating_sub(extra_w),
        max_width: constraints.max_width.saturating_sub(extra_w),
        min_height: constraints.min_height.saturating_sub(extra_h),
        max_height: constraints.max_height.saturating_sub(extra_h),
    }
}

fn scroll_child_constraints(direction: Direction, constraints: Constraints) -> Constraints {
    match direction {
        Direction::Column => Constraints {
            min_width: constraints.min_width,
            max_width: constraints.max_width,
            min_height: 0,
            max_height: u16::MAX,
        },
        Direction::Row => Constraints {
            min_width: 0,
            max_width: u16::MAX,
            min_height: constraints.min_height,
            max_height: constraints.max_height,
        },
    }
}

fn flex_child_constraints(direction: Direction, main: u16, cross: u16) -> Constraints {
    let size = direction.compose(main, cross);
    Constraints::loose(size.width, size.height)
}

fn apply_child_bounds(size: u16, min: Option<u16>, max: Option<u16>) -> u16 {
    let mut bounded = size;
    if let Some(min) = min {
        bounded = bounded.max(min);
    }
    if let Some(max) = max {
        bounded = bounded.min(max);
    }
    bounded
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::element::FlexChild;
    use crate::element::{Edges, Style};
    use crate::layout::flex;
    use crate::plugin::{AppView, PluginBackend, PluginCapabilities, PluginId, PluginRuntime};
    use crate::protocol::Face;
    use crate::state::AppState;
    use crate::surface::{SlotDeclaration, SlotKind, SurfaceId, SurfaceRegistry};
    use crate::test_support::TestSurfaceBuilder;

    #[test]
    fn test_resolved_tree_rejects_placeholder() {
        let root = Element::slot_placeholder("kasane.buffer.left", Direction::Row);
        let err = ResolvedTree::new(root, vec![]).unwrap_err();
        assert_eq!(err.as_str(), "resolved tree still contains SlotPlaceholder");
    }

    #[test]
    fn test_resolved_tree_accepts_resolved_slot() {
        let root = Element::ResolvedSlot {
            surface_key: "kasane.buffer".into(),
            slot_name: "kasane.buffer.left".into(),
            instance_id: ResolvedSlotInstanceId(1),
            direction: Direction::Row,
            children: vec![FlexChild::fixed(Element::text("ok", Face::default()))],
            gap: 0,
        };
        let tree = ResolvedTree::new(root, vec![]).unwrap();
        assert_eq!(tree.slot_records.len(), 0);
    }

    #[test]
    fn test_backfill_surface_report_areas_sets_slot_area() {
        let element = Element::ResolvedSlot {
            surface_key: "kasane.buffer".into(),
            slot_name: "kasane.buffer.left".into(),
            instance_id: ResolvedSlotInstanceId(7),
            direction: Direction::Row,
            children: vec![FlexChild::fixed(Element::text("ok", Face::default()))],
            gap: 0,
        };
        let area = Rect {
            x: 0,
            y: 0,
            w: 10,
            h: 2,
        };
        let layout = flex::place(&element, area, &AppState::default());
        let mut reports = vec![SurfaceRenderReport {
            surface_key: "kasane.buffer".into(),
            slot_records: vec![ResolvedSlotRecord {
                surface_key: "kasane.buffer".into(),
                slot_name: "kasane.buffer.left".into(),
                instance_id: ResolvedSlotInstanceId(7),
                direction: Direction::Row,
                gap: 0,
                contribution_count: 1,
                content_kind: ResolvedSlotContentKind::Single,
                area: None,
            }],
            absent_declared_slots: vec![],
            owner_errors: vec![],
            contributor_issues: vec![],
        }];

        backfill_surface_report_areas(&mut reports, &element, &layout);

        assert_eq!(reports[0].slot_records[0].area, Some(area));
    }

    #[derive(Clone)]
    struct RecordingPlugin {
        seen: Rc<RefCell<Vec<ContributeContext>>>,
    }

    impl PluginBackend for RecordingPlugin {
        fn id(&self) -> PluginId {
            PluginId("recording_plugin".into())
        }

        fn capabilities(&self) -> PluginCapabilities {
            PluginCapabilities::CONTRIBUTOR
        }

        fn contribute_to(
            &self,
            region: &SlotId,
            _state: &AppView<'_>,
            ctx: &ContributeContext,
        ) -> Option<Contribution> {
            if region.as_str() == "test.surface.slot" {
                self.seen.borrow_mut().push(ctx.clone());
                Some(Contribution {
                    element: Element::text("x", Face::default()),
                    priority: 0,
                    size_hint: ContribSizeHint::Auto,
                })
            } else {
                None
            }
        }
    }

    fn resolve_with_surface(
        root: Element,
        slots: Vec<SlotDeclaration>,
        state: &AppState,
        registry: &PluginView<'_>,
    ) -> SurfaceRenderOutcome {
        let mut surface_registry = SurfaceRegistry::new();
        surface_registry
            .try_register(
                TestSurfaceBuilder::new(SurfaceId(900))
                    .key("test.surface")
                    .slots(slots)
                    .root(root.clone())
                    .build(),
            )
            .unwrap();
        let descriptor = surface_registry.descriptor(SurfaceId(900)).unwrap().clone();
        resolve_surface_tree(
            &descriptor,
            root,
            state,
            registry,
            Rect {
                x: 0,
                y: 0,
                w: 20,
                h: 10,
            },
            PaneContext::default(),
        )
    }

    #[test]
    fn test_resolve_placeholder_uses_container_child_constraints() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let mut registry = PluginRuntime::new();
        registry.register_backend(Box::new(RecordingPlugin { seen: seen.clone() }));
        let state = AppState::default();

        let root = Element::Container {
            child: Box::new(Element::slot_placeholder(
                "test.surface.slot",
                Direction::Row,
            )),
            border: None,
            shadow: false,
            padding: Edges {
                top: 1,
                right: 3,
                bottom: 2,
                left: 2,
            },
            style: Style::from(Face::default()),
            title: None,
        };

        let outcome = resolve_with_surface(
            root,
            vec![SlotDeclaration::new(
                "test.surface.slot",
                SlotKind::LeftRail,
            )],
            &state,
            &registry.view(),
        );
        assert!(outcome.report.owner_errors.is_empty());

        let seen = seen.borrow();
        assert_eq!(seen.len(), 1);
        assert_eq!(
            seen[0],
            ContributeContext {
                min_width: 15,
                max_width: Some(15),
                min_height: 7,
                max_height: Some(7),
                visible_lines: state.visible_line_range(),
                screen_cols: state.cols,
                screen_rows: state.rows,
                pane_surface_id: None,
                pane_focused: true,
            }
        );
    }

    #[test]
    fn test_resolve_placeholder_uses_flex_share_constraints() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let mut registry = PluginRuntime::new();
        registry.register_backend(Box::new(RecordingPlugin { seen: seen.clone() }));
        let state = AppState::default();

        let root = Element::row(vec![
            FlexChild::fixed(Element::text("abc", Face::default())),
            FlexChild::flexible(
                Element::slot_placeholder("test.surface.slot", Direction::Row),
                1.0,
            ),
        ]);

        let outcome = resolve_with_surface(
            root,
            vec![SlotDeclaration::new(
                "test.surface.slot",
                SlotKind::LeftRail,
            )],
            &state,
            &registry.view(),
        );
        assert!(outcome.report.owner_errors.is_empty());

        let seen = seen.borrow();
        assert_eq!(seen.len(), 1);
        assert_eq!(
            seen[0],
            ContributeContext {
                min_width: 0,
                max_width: Some(17),
                min_height: 0,
                max_height: Some(10),
                visible_lines: state.visible_line_range(),
                screen_cols: state.cols,
                screen_rows: state.rows,
                pane_surface_id: None,
                pane_focused: true,
            }
        );
    }

    #[test]
    fn test_resolve_overlay_slot_inside_stack_overlay() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let mut registry = PluginRuntime::new();
        registry.register_backend(Box::new(RecordingPlugin { seen: seen.clone() }));
        let state = AppState::default();

        let root = Element::stack(
            Element::text("base", Face::default()),
            vec![crate::element::Overlay {
                element: Element::slot_placeholder("test.surface.slot", Direction::Column),
                anchor: crate::element::OverlayAnchor::Fill,
            }],
        );

        let outcome = resolve_with_surface(
            root,
            vec![SlotDeclaration::new("test.surface.slot", SlotKind::Overlay)],
            &state,
            &registry.view(),
        );
        assert!(outcome.report.owner_errors.is_empty());
        let tree = outcome.tree.expect("overlay slot should resolve");

        match tree.into_root() {
            Element::Stack { overlays, .. } => {
                assert_eq!(overlays.len(), 1);
                assert!(matches!(
                    overlays[0].anchor,
                    crate::element::OverlayAnchor::Fill
                ));
                assert!(matches!(overlays[0].element, Element::ResolvedSlot { .. }));
            }
            other => panic!(
                "expected resolved stack root, got {:?}",
                std::mem::discriminant(&other)
            ),
        }

        let seen = seen.borrow();
        assert_eq!(seen.len(), 1);
        assert_eq!(
            seen[0],
            ContributeContext {
                min_width: 20,
                max_width: Some(20),
                min_height: 10,
                max_height: Some(10),
                visible_lines: state.visible_line_range(),
                screen_cols: state.cols,
                screen_rows: state.rows,
                pane_surface_id: None,
                pane_focused: true,
            }
        );
    }
}
