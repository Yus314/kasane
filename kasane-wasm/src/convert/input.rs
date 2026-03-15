use crate::bindings::kasane::plugin::types as wit;
use kasane_core::input::{Key, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use kasane_core::plugin::{IoEvent, ProcessEvent};

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

fn mouse_button_to_wit(b: MouseButton) -> wit::MouseButton {
    match b {
        MouseButton::Left => wit::MouseButton::Left,
        MouseButton::Middle => wit::MouseButton::Middle,
        MouseButton::Right => wit::MouseButton::Right,
    }
}

pub(crate) fn key_event_to_wit(event: &KeyEvent) -> wit::KeyEvent {
    wit::KeyEvent {
        key: key_to_wit(&event.key),
        modifiers: event.modifiers.bits(),
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
