use std::collections::{HashMap, HashSet};

use compact_str::CompactString;

use crate::element::{Direction, Element, FlexChild, ResolvedSlotInstanceId};
use crate::layout::Rect;
use crate::layout::flex::{self, Constraints, LayoutResult};
use crate::plugin::{
    ContribSizeHint, ContributeContext, Contribution, PluginRegistry, SlotId, SourcedContribution,
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
    pub root: Element,
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

pub fn resolve_surface_tree(
    descriptor: &SurfaceDescriptor,
    root: Element,
    state: &AppState,
    registry: &PluginRegistry,
    rect: Rect,
) -> SurfaceRenderOutcome {
    let root_constraints = Constraints::tight(rect.w, rect.h);
    let mut resolver = Resolver {
        descriptor,
        state,
        registry,
        seen_slots: HashSet::new(),
        next_instance_id: 1,
        slot_records: Vec::new(),
        owner_errors: Vec::new(),
        contributor_issues: Vec::new(),
    };

    let resolved_root =
        resolver.resolve_element(root, PlaceholderPolicy::Allowed, root_constraints);
    let mut report = SurfaceRenderReport::from_descriptor(descriptor);
    report.slot_records = resolver.slot_records.clone();
    report.contributor_issues = resolver.contributor_issues;
    report.owner_errors = resolver.owner_errors;
    report.absent_declared_slots = descriptor
        .declared_slots
        .iter()
        .filter(|slot| !resolver.seen_slots.contains(slot.name.as_str()))
        .map(|slot| slot.name.clone())
        .collect();

    let tree = if report.owner_errors.is_empty() {
        match ResolvedTree::new(resolved_root, resolver.slot_records) {
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
        | Element::Empty
        | Element::BufferRef { .. } => false,
    }
}

#[derive(Clone, Copy)]
enum PlaceholderPolicy {
    Allowed,
    ForbiddenInGrid,
}

struct Resolver<'a> {
    descriptor: &'a SurfaceDescriptor,
    state: &'a AppState,
    registry: &'a PluginRegistry,
    seen_slots: HashSet<&'a str>,
    next_instance_id: u64,
    slot_records: Vec<ResolvedSlotRecord>,
    owner_errors: Vec<OwnerValidationError>,
    contributor_issues: Vec<ContributorIssue>,
}

impl Resolver<'_> {
    fn resolve_element(
        &mut self,
        element: Element,
        policy: PlaceholderPolicy,
        constraints: Constraints,
    ) -> Element {
        match element {
            Element::SlotPlaceholder {
                slot_name,
                direction,
                gap,
            } => self.resolve_placeholder(slot_name, direction, gap, policy, constraints),
            Element::ResolvedSlot { slot_name, .. } => {
                self.owner_errors.push(OwnerValidationError {
                    surface_key: self.descriptor.surface_key.clone(),
                    kind: OwnerValidationErrorKind::UnexpectedResolvedSlot,
                    detail: format!("surface input tree contains resolved slot {slot_name}").into(),
                });
                Element::Empty
            }
            Element::Flex {
                direction,
                children,
                gap,
                align,
                cross_align,
            } => {
                let children =
                    self.resolve_flex_children(direction, children, gap, policy, constraints);
                Element::Flex {
                    direction,
                    children,
                    gap,
                    align,
                    cross_align,
                }
            }
            Element::Stack { base, overlays } => Element::Stack {
                base: Box::new(self.resolve_element(*base, policy, constraints)),
                overlays: overlays
                    .into_iter()
                    .map(|overlay| crate::element::Overlay {
                        element: self.resolve_element(overlay.element, policy, constraints),
                        anchor: overlay.anchor,
                    })
                    .collect(),
            },
            Element::Scrollable {
                child,
                offset,
                direction,
            } => Element::Scrollable {
                child: Box::new(self.resolve_element(
                    *child,
                    policy,
                    scroll_child_constraints(direction, constraints),
                )),
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
                child: Box::new(self.resolve_element(
                    *child,
                    policy,
                    container_child_constraints(border.is_some(), padding, constraints),
                )),
                border,
                shadow,
                padding,
                style,
                title,
            },
            Element::Interactive { child, id } => Element::Interactive {
                child: Box::new(self.resolve_element(*child, policy, constraints)),
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
                    .map(|child| {
                        self.resolve_element(child, PlaceholderPolicy::ForbiddenInGrid, constraints)
                    })
                    .collect(),
                col_gap,
                row_gap,
                align,
                cross_align,
            },
            Element::Text(..)
            | Element::StyledLine(..)
            | Element::Empty
            | Element::BufferRef { .. } => element,
        }
    }

    fn resolve_flex_children(
        &mut self,
        direction: Direction,
        children: Vec<FlexChild>,
        gap: u16,
        policy: PlaceholderPolicy,
        constraints: Constraints,
    ) -> Vec<FlexChild> {
        if children.is_empty() {
            return Vec::new();
        }

        let constraint_size = crate::layout::flex::Size {
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

        let mut resolved_children: Vec<Option<FlexChild>> = std::iter::repeat_with(|| None)
            .take(children.len())
            .collect();
        let mut total_fixed = 0u16;
        let mut total_flex = 0.0f32;
        let mut pending_flex = Vec::new();

        for (index, child) in children.into_iter().enumerate() {
            if child.flex > 0.0 {
                total_flex += child.flex;
                pending_flex.push((index, child));
                continue;
            }

            let child_constraints = flex_child_constraints(direction, main_total, cross_total);
            let resolved_child = self.resolve_flex_child(child, policy, child_constraints);
            let size = flex::measure(&resolved_child.element, child_constraints, self.state);
            let (main, _) = direction.decompose(size);
            total_fixed +=
                apply_child_bounds(main, resolved_child.min_size, resolved_child.max_size);
            resolved_children[index] = Some(resolved_child);
        }

        if total_flex > 0.0 {
            let remaining = main_total.saturating_sub(total_fixed + total_gaps);
            let flex_count = pending_flex.len();
            let mut distributed = 0u16;
            for (i, (index, child)) in pending_flex.into_iter().enumerate() {
                let share = if i + 1 == flex_count {
                    remaining.saturating_sub(distributed)
                } else {
                    (remaining as f32 * child.flex / total_flex) as u16
                };
                let share = apply_child_bounds(share, child.min_size, child.max_size);
                distributed = distributed.saturating_add(share);
                let child_constraints = flex_child_constraints(direction, share, cross_total);
                resolved_children[index] =
                    Some(self.resolve_flex_child(child, policy, child_constraints));
            }
        }

        resolved_children
            .into_iter()
            .map(|child| child.expect("flex child should be resolved"))
            .collect()
    }

    fn resolve_flex_child(
        &mut self,
        child: FlexChild,
        policy: PlaceholderPolicy,
        constraints: Constraints,
    ) -> FlexChild {
        FlexChild {
            element: self.resolve_element(child.element, policy, constraints),
            ..child
        }
    }

    fn resolve_placeholder(
        &mut self,
        slot_name: CompactString,
        direction: Direction,
        gap: u16,
        policy: PlaceholderPolicy,
        constraints: Constraints,
    ) -> Element {
        if matches!(policy, PlaceholderPolicy::ForbiddenInGrid) {
            self.owner_errors.push(OwnerValidationError {
                surface_key: self.descriptor.surface_key.clone(),
                kind: OwnerValidationErrorKind::UnresolvedSlotPlaceholder,
                detail: format!("slot placeholder {slot_name} is not supported inside Grid").into(),
            });
            return Element::Empty;
        }

        let slot = if let Some(slot) = self.descriptor.declared_slot(slot_name.as_str()) {
            slot
        } else {
            self.owner_errors.push(OwnerValidationError {
                surface_key: self.descriptor.surface_key.clone(),
                kind: OwnerValidationErrorKind::UnresolvedSlotPlaceholder,
                detail: format!("undeclared slot placeholder {slot_name}").into(),
            });
            return Element::Empty;
        };

        if !self.seen_slots.insert(slot.name.as_str()) {
            self.owner_errors.push(OwnerValidationError {
                surface_key: self.descriptor.surface_key.clone(),
                kind: OwnerValidationErrorKind::UnresolvedSlotPlaceholder,
                detail: format!("duplicate slot placeholder {}", slot.name).into(),
            });
            return Element::Empty;
        }

        let slot_id = SlotId::new(slot.name.clone());
        let ctx = ContributeContext::from_constraints(self.state, constraints);
        let sourced = self
            .registry
            .collect_contributions_with_sources(&slot_id, self.state, &ctx);

        let instance_id = self.allocate_instance_id();
        let mut children = Vec::new();
        for sourced_contribution in sourced {
            if let Some(child) =
                self.resolve_contribution(slot.name.as_str(), instance_id, sourced_contribution)
            {
                children.push(child);
            }
        }

        let content_kind = match children.len() {
            0 => ResolvedSlotContentKind::Empty,
            1 => ResolvedSlotContentKind::Single,
            _ => ResolvedSlotContentKind::Aggregate,
        };
        self.slot_records.push(ResolvedSlotRecord {
            surface_key: self.descriptor.surface_key.clone(),
            slot_name: slot.name.clone(),
            instance_id,
            direction,
            gap,
            contribution_count: children.len(),
            content_kind,
            area: None,
        });

        Element::ResolvedSlot {
            surface_key: self.descriptor.surface_key.clone(),
            slot_name,
            instance_id,
            direction,
            children,
            gap,
        }
    }

    fn resolve_contribution(
        &mut self,
        slot_name: &str,
        instance_id: ResolvedSlotInstanceId,
        sourced: SourcedContribution,
    ) -> Option<FlexChild> {
        let SourcedContribution {
            contributor,
            contribution,
        } = sourced;
        match self.validate_contribution_tree(&contribution.element) {
            Ok(()) => Some(contribution_to_flex_child(contribution)),
            Err((kind, detail)) => {
                self.contributor_issues.push(ContributorIssue {
                    surface_key: self.descriptor.surface_key.clone(),
                    slot_name: slot_name.into(),
                    slot_instance_id: Some(instance_id),
                    contributor: contributor.0.into(),
                    kind,
                    detail,
                });
                None
            }
        }
    }

    fn validate_contribution_tree(
        &self,
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
                    self.validate_contribution_tree(&child.element)?;
                }
                Ok(())
            }
            Element::Stack { base, overlays } => {
                self.validate_contribution_tree(base)?;
                for overlay in overlays {
                    self.validate_contribution_tree(&overlay.element)?;
                }
                Ok(())
            }
            Element::Scrollable { child, .. }
            | Element::Container { child, .. }
            | Element::Interactive { child, .. } => self.validate_contribution_tree(child),
            Element::Grid { children, .. } => {
                for child in children {
                    self.validate_contribution_tree(child)?;
                }
                Ok(())
            }
            Element::Text(..)
            | Element::StyledLine(..)
            | Element::Empty
            | Element::BufferRef { .. } => Ok(()),
        }
    }

    fn allocate_instance_id(&mut self) -> ResolvedSlotInstanceId {
        let id = ResolvedSlotInstanceId(self.next_instance_id);
        self.next_instance_id += 1;
        id
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

    use compact_str::CompactString;

    use super::*;
    use crate::element::FlexChild;
    use crate::element::{Edges, Style};
    use crate::layout::flex;
    use crate::plugin::{PluginBackend, PluginCapabilities, PluginId};
    use crate::protocol::Face;
    use crate::state::AppState;
    use crate::surface::{
        EventContext, SizeHint, SlotDeclaration, SlotKind, Surface, SurfaceEvent, SurfaceId,
        SurfaceRegistry, ViewContext,
    };

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
            _state: &AppState,
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

    struct ResolveTestSurface {
        root: Element,
        slots: Vec<SlotDeclaration>,
    }

    impl Surface for ResolveTestSurface {
        fn id(&self) -> SurfaceId {
            SurfaceId(900)
        }

        fn surface_key(&self) -> CompactString {
            "test.surface".into()
        }

        fn size_hint(&self) -> SizeHint {
            SizeHint::fill()
        }

        fn view(&self, _ctx: &ViewContext<'_>) -> Element {
            self.root.clone()
        }

        fn handle_event(
            &mut self,
            _event: SurfaceEvent,
            _ctx: &EventContext<'_>,
        ) -> Vec<crate::plugin::Command> {
            vec![]
        }

        fn declared_slots(&self) -> &[SlotDeclaration] {
            &self.slots
        }
    }

    fn resolve_with_surface(
        root: Element,
        slots: Vec<SlotDeclaration>,
        state: &AppState,
        registry: &PluginRegistry,
    ) -> SurfaceRenderOutcome {
        let mut surface_registry = SurfaceRegistry::new();
        surface_registry
            .try_register(Box::new(ResolveTestSurface {
                root: root.clone(),
                slots,
            }))
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
        )
    }

    #[test]
    fn test_resolve_placeholder_uses_container_child_constraints() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let mut registry = PluginRegistry::new();
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
            &registry,
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
            }
        );
    }

    #[test]
    fn test_resolve_placeholder_uses_flex_share_constraints() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let mut registry = PluginRegistry::new();
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
            &registry,
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
            }
        );
    }

    #[test]
    fn test_resolve_overlay_slot_inside_stack_overlay() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let mut registry = PluginRegistry::new();
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
            &registry,
        );
        assert!(outcome.report.owner_errors.is_empty());
        let tree = outcome.tree.expect("overlay slot should resolve");

        match tree.root {
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
            }
        );
    }
}
