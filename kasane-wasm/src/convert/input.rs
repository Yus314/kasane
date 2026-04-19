use crate::bindings::kasane::plugin::types as wit;
use kasane_core::input::{
    ChordBinding, CompiledKeyMap, DropEvent, Key, KeyBinding, KeyEvent, KeyGroup, KeyPattern,
    KeyResponse, Modifiers, MouseButton, MouseEvent, MouseEventKind,
};
use kasane_core::plugin::{Command, HttpEvent, IoEvent, ProcessEvent};
use kasane_core::scroll::{
    DefaultScrollCandidate, ResolvedScroll, ScrollAccumulationMode, ScrollCurve, ScrollGranularity,
    ScrollPlan, ScrollPolicyResult,
};

pub(crate) fn io_event_to_wit(event: &IoEvent) -> wit::IoEvent {
    match event {
        IoEvent::Process(pe) => wit::IoEvent::Process(process_event_to_wit(pe)),
        IoEvent::Http(he) => wit::IoEvent::Http(http_event_to_wit(he)),
    }
}

fn http_event_to_wit(he: &HttpEvent) -> wit::HttpEvent {
    match he {
        HttpEvent::Response {
            job_id,
            status,
            headers,
            body,
        } => wit::HttpEvent::Response(wit::HttpResponse {
            job_id: *job_id,
            status: *status,
            headers: headers.clone(),
            body: body.clone(),
        }),
        HttpEvent::Chunk { job_id, data } => wit::HttpEvent::Chunk(wit::HttpChunk {
            job_id: *job_id,
            data: data.clone(),
        }),
        HttpEvent::StreamEnd { job_id } => wit::HttpEvent::StreamEnd(*job_id),
        HttpEvent::Error { job_id, error } => wit::HttpEvent::Error(wit::HttpError {
            job_id: *job_id,
            error: error.clone(),
        }),
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

pub(crate) fn drop_event_to_wit(event: &DropEvent) -> wit::DropEvent {
    wit::DropEvent {
        paths: event
            .paths
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect(),
        col: event.col as u32,
        row: event.row as u32,
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
        Key::Char(c) => wit::KeyCode::Char(*c as u32),
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
        wit::KeyCode::Char(codepoint) => {
            let ch = char::from_u32(*codepoint)
                .ok_or_else(|| format!("invalid Unicode codepoint: {codepoint}"))?;
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

// ---------------------------------------------------------------------------
// KeyResponse conversions
// ---------------------------------------------------------------------------

pub(crate) fn wit_key_response_to_key_response(
    response: &wit::KeyResponse,
    convert_cmds: &dyn Fn(&[wit::Command]) -> Vec<Command>,
) -> KeyResponse {
    match response {
        wit::KeyResponse::Pass => KeyResponse::Pass,
        wit::KeyResponse::Consume => KeyResponse::Consume,
        wit::KeyResponse::ConsumeRedraw => KeyResponse::ConsumeRedraw,
        wit::KeyResponse::ConsumeWith(cmds) => KeyResponse::ConsumeWith(convert_cmds(cmds)),
    }
}

// ---------------------------------------------------------------------------
// Key map protocol conversions
// ---------------------------------------------------------------------------

pub(crate) fn wit_key_group_decls_to_compiled_key_map(
    decls: &[wit::KeyGroupDecl],
) -> Result<CompiledKeyMap, String> {
    let mut groups = Vec::with_capacity(decls.len());
    for decl in decls {
        let mut bindings = Vec::with_capacity(decl.bindings.len());
        for b in &decl.bindings {
            bindings.push(KeyBinding {
                pattern: wit_key_pattern_to_key_pattern(&b.pattern)?,
                action_id: Box::leak(b.action_id.clone().into_boxed_str()),
            });
        }
        let mut chords = Vec::with_capacity(decl.chords.len());
        for c in &decl.chords {
            chords.push(ChordBinding {
                leader: wit_key_event_to_key_event(&c.leader)?,
                follower: wit_key_pattern_to_key_pattern(&c.follower)?,
                action_id: Box::leak(c.action_id.clone().into_boxed_str()),
            });
        }
        groups.push(KeyGroup {
            name: Box::leak(decl.name.clone().into_boxed_str()),
            active: true,
            bindings,
            chords,
        });
    }
    Ok(CompiledKeyMap {
        groups,
        ..Default::default()
    })
}

fn wit_key_pattern_to_key_pattern(pattern: &wit::KeyPattern) -> Result<KeyPattern, String> {
    match &pattern.kind {
        wit::KeyPatternKind::Exact(event) => {
            Ok(KeyPattern::Exact(wit_key_event_to_key_event(event)?))
        }
        wit::KeyPatternKind::AnyChar => Ok(KeyPattern::AnyChar),
        wit::KeyPatternKind::AnyCharPlain => Ok(KeyPattern::AnyCharPlain),
        wit::KeyPatternKind::AnyKey => Ok(KeyPattern::Any),
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
