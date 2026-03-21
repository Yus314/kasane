//! Types for default buffer scroll routing and policy decisions.

use std::collections::HashMap;

use crate::input::Modifiers;
use crate::input::{self, MouseButton, MouseEvent, MouseEventKind};
use crate::layout::HitMap;
use crate::plugin::PluginRuntime;
use crate::protocol::KasaneRequest;
use crate::state::{AppState, DragState};

/// Host-independent description of how coarse an incoming scroll delta is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollGranularity {
    #[default]
    Line,
    Page,
    Pixel,
}

/// A concrete scroll request with a resolved Kakoune anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedScroll {
    pub amount: i32,
    pub line: u32,
    pub column: u32,
}

impl ResolvedScroll {
    pub const fn new(amount: i32, line: u32, column: u32) -> Self {
        Self {
            amount,
            line,
            column,
        }
    }

    pub const fn anchor(self) -> (u32, u32) {
        (self.line, self.column)
    }

    pub fn to_kakoune_request(self) -> KasaneRequest {
        KasaneRequest::Scroll {
            amount: self.amount,
            line: self.line,
            column: self.column,
        }
    }
}

/// The core-owned, default buffer scroll candidate before policy is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefaultScrollCandidate {
    pub screen_line: u32,
    pub screen_column: u32,
    pub modifiers: Modifiers,
    pub granularity: ScrollGranularity,
    pub raw_amount: i32,
    pub resolved: ResolvedScroll,
}

impl DefaultScrollCandidate {
    pub const fn new(
        screen_line: u32,
        screen_column: u32,
        modifiers: Modifiers,
        granularity: ScrollGranularity,
        raw_amount: i32,
        resolved: ResolvedScroll,
    ) -> Self {
        Self {
            screen_line,
            screen_column,
            modifiers,
            granularity,
            raw_amount,
            resolved,
        }
    }
}

pub const SMOOTH_SCROLL_CONFIG_KEY: &str = "smooth-scroll.enabled";
pub const SMOOTH_SCROLL_LEGACY_CONFIG_KEY: &str = "smooth_scroll";

fn parse_bool_config(value: Option<&String>) -> bool {
    value
        .and_then(|raw| raw.parse::<bool>().ok())
        .unwrap_or(false)
}

pub fn smooth_scroll_enabled(state: &AppState) -> bool {
    parse_bool_config(
        state
            .plugin_config
            .get(SMOOTH_SCROLL_CONFIG_KEY)
            .or_else(|| state.plugin_config.get(SMOOTH_SCROLL_LEGACY_CONFIG_KEY)),
    )
}

pub fn set_smooth_scroll_enabled(config: &mut HashMap<String, String>, enabled: bool) {
    config.insert(SMOOTH_SCROLL_CONFIG_KEY.to_string(), enabled.to_string());
    config.remove(SMOOTH_SCROLL_LEGACY_CONFIG_KEY);
}

pub fn is_smooth_scroll_config_key(key: &str) -> bool {
    matches!(
        key,
        SMOOTH_SCROLL_CONFIG_KEY | SMOOTH_SCROLL_LEGACY_CONFIG_KEY
    )
}

/// Build the default buffer scroll candidate produced by the current fallback path.
///
/// This intentionally mirrors the existing `mouse_to_kakoune(..., None)` behavior so
/// that new routing can be introduced behind parity tests without changing semantics.
pub fn default_scroll_candidate(
    mouse: &MouseEvent,
    scroll_amount: i32,
) -> Option<DefaultScrollCandidate> {
    match input::mouse_to_kakoune(mouse, scroll_amount, None) {
        Some(KasaneRequest::Scroll {
            amount,
            line,
            column,
        }) => Some(DefaultScrollCandidate::new(
            mouse.line,
            mouse.column,
            mouse.modifiers,
            ScrollGranularity::Line,
            amount,
            ResolvedScroll::new(amount, line, column),
        )),
        _ => None,
    }
}

pub const fn is_scroll_event(mouse: &MouseEvent) -> bool {
    matches!(
        mouse.kind,
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
    )
}

pub const fn selection_scroll_edge_line(rows: u16, mouse: &MouseEvent) -> Option<u32> {
    if !is_scroll_event(mouse) {
        return None;
    }
    Some(match mouse.kind {
        MouseEventKind::ScrollUp => 0,
        _ => rows.saturating_sub(2) as u32,
    })
}

/// Legacy-compatible info popup consumption used while scroll routing is being extracted.
pub fn consume_info_scroll(state: &mut AppState, mouse: &MouseEvent, hit_map: &HitMap) -> bool {
    use crate::element::InteractiveId;

    let (id, rect) = match hit_map.test_with_rect(mouse.column as u16, mouse.line as u16) {
        Some(hit) => hit,
        None => return false,
    };

    if id.0 < InteractiveId::INFO_BASE {
        return false;
    }

    let index = (id.0 - InteractiveId::INFO_BASE) as usize;
    let info = match state.infos.get_mut(index) {
        Some(info) => info,
        None => return false,
    };

    let content_h = info
        .content
        .iter()
        .map(|line| crate::layout::word_wrap_line_height(line, rect.w.saturating_sub(4).max(1)))
        .sum::<u16>();
    let visible_h = rect.h.saturating_sub(2).max(1);

    match mouse.kind {
        MouseEventKind::ScrollUp => {
            info.scroll_offset = info.scroll_offset.saturating_sub(3);
        }
        MouseEventKind::ScrollDown => {
            let max_offset = content_h.saturating_sub(visible_h);
            info.scroll_offset = (info.scroll_offset + 3).min(max_offset);
        }
        _ => return false,
    }

    true
}

/// Which runtime component ended up owning a scroll candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollOwner {
    Core,
    Surface,
    Policy,
}

/// High-level effect of a scroll policy decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollConsumption {
    Pass,
    Suppress,
    Immediate,
    Plan,
}

/// How new wheel input interacts with an already active scroll plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollAccumulationMode {
    #[default]
    Add,
    Replace,
}

/// Host-executed progression curve for a scroll plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollCurve {
    Instant,
    #[default]
    Linear,
}

/// Declarative scroll plan to be executed by the host runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollPlan {
    pub total_amount: i32,
    pub line: u32,
    pub column: u32,
    pub frame_interval_ms: u16,
    pub curve: ScrollCurve,
    pub accumulation: ScrollAccumulationMode,
}

impl ScrollPlan {
    pub const fn new(
        total_amount: i32,
        line: u32,
        column: u32,
        frame_interval_ms: u16,
        curve: ScrollCurve,
        accumulation: ScrollAccumulationMode,
    ) -> Self {
        Self {
            total_amount,
            line,
            column,
            frame_interval_ms,
            curve,
            accumulation,
        }
    }

    pub const fn anchor(self) -> (u32, u32) {
        (self.line, self.column)
    }
}

/// Typed output of a scroll policy plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollPolicyResult {
    Pass,
    Suppress,
    Immediate(ResolvedScroll),
    Plan(ScrollPlan),
}

impl ScrollPolicyResult {
    pub const fn consumption(self) -> ScrollConsumption {
        match self {
            Self::Pass => ScrollConsumption::Pass,
            Self::Suppress => ScrollConsumption::Suppress,
            Self::Immediate(_) => ScrollConsumption::Immediate,
            Self::Plan(_) => ScrollConsumption::Plan,
        }
    }
}

/// Fallback default policy used when no scroll policy plugin claims the candidate.
pub const fn fallback_scroll_policy(candidate: DefaultScrollCandidate) -> ScrollPolicyResult {
    ScrollPolicyResult::Immediate(candidate.resolved)
}

pub fn resolve_default_scroll_policy(
    registry: &mut PluginRuntime,
    state: &AppState,
    candidate: DefaultScrollCandidate,
) -> ScrollPolicyResult {
    match registry.handle_default_scroll(candidate, state) {
        Some((_, ScrollPolicyResult::Pass)) | None => fallback_scroll_policy(candidate),
        Some((_, result)) => result,
    }
}

pub fn requests_from_policy_result(result: ScrollPolicyResult) -> Vec<KasaneRequest> {
    match result {
        ScrollPolicyResult::Pass | ScrollPolicyResult::Suppress | ScrollPolicyResult::Plan(_) => {
            Vec::new()
        }
        ScrollPolicyResult::Immediate(resolved) => vec![resolved.to_kakoune_request()],
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LegacyScrollDispatch {
    NotHandled,
    ConsumedInfo,
    Requests(Vec<KasaneRequest>),
    Plan(ScrollPlan),
}

pub const fn legacy_smooth_scroll_plan(candidate: DefaultScrollCandidate) -> ScrollPlan {
    ScrollPlan::new(
        candidate.resolved.amount,
        candidate.resolved.line,
        candidate.resolved.column,
        16,
        ScrollCurve::Linear,
        ScrollAccumulationMode::Add,
    )
}

pub fn dispatch_legacy_mouse_scroll(
    state: &mut AppState,
    mouse: &MouseEvent,
    hit_map: &HitMap,
    registry: &mut PluginRuntime,
    scroll_amount: i32,
) -> LegacyScrollDispatch {
    if !is_scroll_event(mouse) {
        return LegacyScrollDispatch::NotHandled;
    }

    if matches!(
        state.drag,
        DragState::Active {
            button: MouseButton::Left,
            ..
        }
    ) {
        if consume_info_scroll(state, mouse, hit_map) {
            return LegacyScrollDispatch::ConsumedInfo;
        }

        if let Some(candidate) = default_scroll_candidate(mouse, scroll_amount) {
            let mut requests = requests_from_policy_result(fallback_scroll_policy(candidate));
            if let Some(edge_line) = selection_scroll_edge_line(state.rows, mouse) {
                requests.push(KasaneRequest::MouseMove {
                    line: edge_line,
                    column: mouse.column,
                });
            }
            return LegacyScrollDispatch::Requests(requests);
        }
    }

    if consume_info_scroll(state, mouse, hit_map) {
        return LegacyScrollDispatch::ConsumedInfo;
    }

    if let Some(candidate) = default_scroll_candidate(mouse, scroll_amount) {
        return match resolve_default_scroll_policy(registry, state, candidate) {
            ScrollPolicyResult::Plan(plan) => LegacyScrollDispatch::Plan(plan),
            other => LegacyScrollDispatch::Requests(requests_from_policy_result(other)),
        };
    }

    LegacyScrollDispatch::NotHandled
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveScrollPlan {
    pub remaining_amount: i32,
    pub line: u32,
    pub column: u32,
    pub frame_interval_ms: u16,
    pub generation: u64,
    pub curve: ScrollCurve,
    pub accumulation: ScrollAccumulationMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScrollRuntime {
    pub initial_resize_complete: bool,
    pub generation: u64,
    pub active_plan: Option<ActiveScrollPlan>,
}

impl ScrollRuntime {
    pub fn enqueue(&mut self, plan: ScrollPlan) {
        if plan.total_amount == 0 {
            return;
        }

        match (self.active_plan.as_mut(), plan.accumulation) {
            (Some(active), ScrollAccumulationMode::Add) if active.generation == self.generation => {
                active.remaining_amount += plan.total_amount;
                active.line = plan.line;
                active.column = plan.column;
                active.frame_interval_ms = plan.frame_interval_ms.max(1);
                active.curve = plan.curve;
                active.accumulation = plan.accumulation;
            }
            _ => {
                self.active_plan = Some(ActiveScrollPlan {
                    remaining_amount: plan.total_amount,
                    line: plan.line,
                    column: plan.column,
                    frame_interval_ms: plan.frame_interval_ms.max(1),
                    generation: self.generation,
                    curve: plan.curve,
                    accumulation: plan.accumulation,
                });
            }
        }
    }

    pub fn set_initial_resize_complete(&mut self, complete: bool) {
        self.initial_resize_complete = complete;
    }

    pub fn complete_initial_resize(&mut self) {
        self.set_initial_resize_complete(true);
    }

    pub fn advance_generation(&mut self) {
        self.generation += 1;
    }

    pub fn cancel_active(&mut self) {
        self.active_plan = None;
    }

    pub const fn has_active_plan(&self) -> bool {
        self.active_plan.is_some()
    }

    pub fn active_frame_interval(&self) -> Option<std::time::Duration> {
        self.active_plan
            .map(|plan| std::time::Duration::from_millis(u64::from(plan.frame_interval_ms.max(1))))
    }

    pub fn suspend(&mut self) {
        self.initial_resize_complete = false;
        self.active_plan = None;
    }

    pub fn tick(&mut self) -> Option<ResolvedScroll> {
        if !self.initial_resize_complete {
            return None;
        }

        let active = self.active_plan.as_mut()?;
        if active.generation != self.generation {
            self.active_plan = None;
            return None;
        }

        let step = match active.curve {
            ScrollCurve::Instant => active.remaining_amount,
            ScrollCurve::Linear => active.remaining_amount.signum(),
        };

        if step == 0 {
            self.active_plan = None;
            return None;
        }

        let resolved = ResolvedScroll::new(step, active.line, active.column);
        active.remaining_amount -= step;
        if active.remaining_amount == 0 {
            self.active_plan = None;
        }
        Some(resolved)
    }
}
