use crate::bindings::kasane::plugin::types as wit;
use kasane_core::input::{Key, KeyEvent, Modifiers, MouseButton, MouseEvent, MouseEventKind};
use kasane_core::plugin::{IoEvent, ProcessEvent};
use kasane_core::scroll::{
    DefaultScrollCandidate, ResolvedScroll, ScrollAccumulationMode, ScrollCurve, ScrollGranularity,
    ScrollPlan, ScrollPolicyResult,
};

pub(crate) fn io_event_to_wit(event: &IoEvent) -> wit::IoEvent {
    match event {
        IoEvent::Process(pe) => wit::IoEvent::Process(process_event_to_wit(pe)),
    }
}

fn process_event_to_wit(pe: &ProcessEvent) -> wit::ProcessEvent {
    wit::ProcessEvent {
        job_id: match pe {
            ProcessEvent::Stdout { job_id, .. }
            | ProcessEvent::Stderr { job_id, .. }
            | ProcessEvent::Exited { job_id, .. }
            | ProcessEvent::SpawnFailed { job_id, .. } => *job_id,
        },
        kind: match pe {
            ProcessEvent::Stdout { data, .. } => wit::ProcessEventKind::Stdout(data.clone()),
            ProcessEvent::Stderr { data, .. } => wit::ProcessEventKind::Stderr(data.clone()),
            ProcessEvent::Exited { exit_code, .. } => wit::ProcessEventKind::Exited(*exit_code),
            ProcessEvent::SpawnFailed { error, .. } => {
                wit::ProcessEventKind::SpawnFailed(error.clone())
            }
        },
    }
}

pub(crate) fn mouse_event_to_wit(event: &MouseEvent) -> wit::MouseEvent {
    wit::MouseEvent {
        kind: mouse_event_kind_to_wit(&event.kind),
        line: event.line,
        column: event.column,
        modifiers: event.modifiers.bits(),
    }
}

fn mouse_event_kind_to_wit(kind: &MouseEventKind) -> wit::MouseEventKind {
    match kind {
        MouseEventKind::Press(b) => wit::MouseEventKind::Press(mouse_button_to_wit(*b)),
        MouseEventKind::Release(b) => wit::MouseEventKind::Release(mouse_button_to_wit(*b)),
        MouseEventKind::Move => wit::MouseEventKind::MoveEvent,
        MouseEventKind::Drag(b) => wit::MouseEventKind::Drag(mouse_button_to_wit(*b)),
        MouseEventKind::ScrollUp => wit::MouseEventKind::ScrollUp,
        MouseEventKind::ScrollDown => wit::MouseEventKind::ScrollDown,
    }
}

enum_convert! {
    mouse_button_to_wit: MouseButton => wit::MouseButton,
    { Left, Middle, Right }
}

pub(crate) fn key_event_to_wit(event: &KeyEvent) -> wit::KeyEvent {
    wit::KeyEvent {
        key: key_to_wit(&event.key),
        modifiers: event.modifiers.bits(),
    }
}

pub(crate) fn wit_key_event_to_key_event(event: &wit::KeyEvent) -> Result<KeyEvent, String> {
    Ok(KeyEvent {
        key: wit_key_code_to_key(&event.key)?,
        modifiers: Modifiers::from_bits_truncate(event.modifiers),
    })
}

pub(crate) fn default_scroll_candidate_to_wit(
    candidate: &DefaultScrollCandidate,
) -> wit::DefaultScrollCandidate {
    wit::DefaultScrollCandidate {
        screen_line: candidate.screen_line,
        screen_column: candidate.screen_column,
        modifiers: candidate.modifiers.bits(),
        granularity: scroll_granularity_to_wit(candidate.granularity),
        raw_amount: candidate.raw_amount,
        resolved: resolved_scroll_to_wit(candidate.resolved),
    }
}

pub(crate) fn wit_scroll_policy_result_to_result(
    result: &wit::ScrollPolicyResult,
) -> ScrollPolicyResult {
    match result {
        wit::ScrollPolicyResult::Pass => ScrollPolicyResult::Pass,
        wit::ScrollPolicyResult::Suppress => ScrollPolicyResult::Suppress,
        wit::ScrollPolicyResult::Immediate(resolved) => {
            ScrollPolicyResult::Immediate(wit_resolved_scroll_to_resolved_scroll(resolved))
        }
        wit::ScrollPolicyResult::Plan(plan) => {
            ScrollPolicyResult::Plan(wit_scroll_plan_to_scroll_plan(plan))
        }
    }
}

fn key_to_wit(key: &Key) -> wit::KeyCode {
    match key {
        Key::Char(c) => wit::KeyCode::Character(c.to_string()),
        Key::Backspace => wit::KeyCode::Backspace,
        Key::Delete => wit::KeyCode::Delete,
        Key::Enter => wit::KeyCode::Enter,
        Key::Tab => wit::KeyCode::Tab,
        Key::Escape => wit::KeyCode::Escape,
        Key::Up => wit::KeyCode::Up,
        Key::Down => wit::KeyCode::Down,
        Key::Left => wit::KeyCode::LeftArrow,
        Key::Right => wit::KeyCode::RightArrow,
        Key::Home => wit::KeyCode::Home,
        Key::End => wit::KeyCode::End,
        Key::PageUp => wit::KeyCode::PageUp,
        Key::PageDown => wit::KeyCode::PageDown,
        Key::F(n) => wit::KeyCode::FKey(*n),
    }
}

fn wit_key_code_to_key(key: &wit::KeyCode) -> Result<Key, String> {
    match key {
        wit::KeyCode::Character(chars) => {
            let mut iter = chars.chars();
            let ch = iter
                .next()
                .ok_or_else(|| "character key must not be empty".to_string())?;
            if iter.next().is_some() {
                return Err(format!(
                    "character key must contain exactly one scalar value: {chars:?}"
                ));
            }
            Ok(Key::Char(ch))
        }
        wit::KeyCode::Backspace => Ok(Key::Backspace),
        wit::KeyCode::Delete => Ok(Key::Delete),
        wit::KeyCode::Enter => Ok(Key::Enter),
        wit::KeyCode::Tab => Ok(Key::Tab),
        wit::KeyCode::Escape => Ok(Key::Escape),
        wit::KeyCode::Up => Ok(Key::Up),
        wit::KeyCode::Down => Ok(Key::Down),
        wit::KeyCode::LeftArrow => Ok(Key::Left),
        wit::KeyCode::RightArrow => Ok(Key::Right),
        wit::KeyCode::Home => Ok(Key::Home),
        wit::KeyCode::End => Ok(Key::End),
        wit::KeyCode::PageUp => Ok(Key::PageUp),
        wit::KeyCode::PageDown => Ok(Key::PageDown),
        wit::KeyCode::FKey(n) => Ok(Key::F(*n)),
    }
}

enum_convert! {
    scroll_granularity_to_wit: ScrollGranularity => wit::ScrollGranularity,
    { Line, Page, Pixel }
}

fn resolved_scroll_to_wit(resolved: ResolvedScroll) -> wit::ResolvedScroll {
    wit::ResolvedScroll {
        amount: resolved.amount,
        line: resolved.line,
        column: resolved.column,
    }
}

fn wit_resolved_scroll_to_resolved_scroll(resolved: &wit::ResolvedScroll) -> ResolvedScroll {
    ResolvedScroll::new(resolved.amount, resolved.line, resolved.column)
}

pub(crate) fn wit_scroll_plan_to_scroll_plan(plan: &wit::ScrollPlan) -> ScrollPlan {
    ScrollPlan::new(
        plan.total_amount,
        plan.line,
        plan.column,
        plan.frame_interval_ms,
        wit_scroll_curve_to_scroll_curve(plan.curve),
        wit_scroll_accumulation_to_scroll_accumulation(plan.accumulation),
    )
}

enum_convert! {
    wit_scroll_curve_to_scroll_curve: wit::ScrollCurve => ScrollCurve,
    { Instant, Linear }
}

enum_convert! {
    wit_scroll_accumulation_to_scroll_accumulation: wit::ScrollAccumulationMode => ScrollAccumulationMode,
    { Add, Replace }
}
