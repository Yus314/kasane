//! Crossterm event conversion to kasane-core input types.

use crossterm::event::{
    Event, KeyCode, KeyEvent as CtKeyEvent, KeyEventKind, KeyModifiers,
    MouseButton as CtMouseButton, MouseEvent as CtMouseEvent, MouseEventKind as CtMouseEventKind,
};
use kasane_core::input::{
    InputEvent, Key, KeyEvent, Modifiers, MouseButton, MouseEvent, MouseEventKind,
};

/// Convert a crossterm Event into a kasane InputEvent.
/// Returns None for events we don't handle (e.g. key release/repeat).
pub fn convert_event(event: Event) -> Option<InputEvent> {
    match event {
        Event::Key(key_event) => convert_key(key_event),
        Event::Mouse(mouse_event) => convert_mouse(mouse_event),
        Event::Paste(text) => Some(InputEvent::Paste(text)),
        Event::Resize(cols, rows) => Some(InputEvent::Resize(cols, rows)),
        Event::FocusGained => Some(InputEvent::FocusGained),
        Event::FocusLost => Some(InputEvent::FocusLost),
    }
}

fn convert_key(event: CtKeyEvent) -> Option<InputEvent> {
    // Only process Press events (not Release or Repeat)
    if event.kind != KeyEventKind::Press {
        return None;
    }

    let modifiers = convert_modifiers(event.modifiers);

    let key = match event.code {
        KeyCode::Char(c) => Key::Char(c),
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        KeyCode::Enter => Key::Enter,
        KeyCode::Tab => Key::Tab,
        KeyCode::BackTab => {
            // BackTab = Shift+Tab
            return Some(InputEvent::Key(KeyEvent {
                key: Key::Tab,
                modifiers: modifiers | Modifiers::SHIFT,
            }));
        }
        KeyCode::Esc => Key::Escape,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::F(n) => Key::F(n),
        _ => return None,
    };

    Some(InputEvent::Key(KeyEvent { key, modifiers }))
}

fn convert_modifiers(mods: KeyModifiers) -> Modifiers {
    let mut result = Modifiers::empty();
    if mods.contains(KeyModifiers::CONTROL) {
        result |= Modifiers::CTRL;
    }
    if mods.contains(KeyModifiers::ALT) {
        result |= Modifiers::ALT;
    }
    if mods.contains(KeyModifiers::SHIFT) {
        result |= Modifiers::SHIFT;
    }
    result
}

fn convert_mouse(event: CtMouseEvent) -> Option<InputEvent> {
    let kind = match event.kind {
        CtMouseEventKind::Down(button) => MouseEventKind::Press(convert_button(button)),
        CtMouseEventKind::Up(button) => MouseEventKind::Release(convert_button(button)),
        CtMouseEventKind::Drag(button) => MouseEventKind::Drag(convert_button(button)),
        CtMouseEventKind::Moved => MouseEventKind::Move,
        CtMouseEventKind::ScrollUp => MouseEventKind::ScrollUp,
        CtMouseEventKind::ScrollDown => MouseEventKind::ScrollDown,
        _ => return None,
    };

    Some(InputEvent::Mouse(MouseEvent {
        kind,
        line: event.row as u32,
        column: event.column as u32,
        modifiers: convert_modifiers(event.modifiers),
    }))
}

fn convert_button(button: CtMouseButton) -> MouseButton {
    match button {
        CtMouseButton::Left => MouseButton::Left,
        CtMouseButton::Middle => MouseButton::Middle,
        CtMouseButton::Right => MouseButton::Right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_key_press() {
        let ct_event = Event::Key(CtKeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        let result = convert_event(ct_event).unwrap();
        assert_eq!(
            result,
            InputEvent::Key(KeyEvent {
                key: Key::Char('a'),
                modifiers: Modifiers::empty(),
            })
        );
    }

    #[test]
    fn test_convert_ctrl_key() {
        let ct_event = Event::Key(CtKeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        let result = convert_event(ct_event).unwrap();
        assert_eq!(
            result,
            InputEvent::Key(KeyEvent {
                key: Key::Char('c'),
                modifiers: Modifiers::CTRL,
            })
        );
    }

    #[test]
    fn test_convert_backtab() {
        let ct_event = Event::Key(CtKeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
        let result = convert_event(ct_event).unwrap();
        assert_eq!(
            result,
            InputEvent::Key(KeyEvent {
                key: Key::Tab,
                modifiers: Modifiers::SHIFT,
            })
        );
    }

    #[test]
    fn test_convert_key_release_ignored() {
        let mut ct_event = CtKeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        ct_event.kind = KeyEventKind::Release;
        let result = convert_event(Event::Key(ct_event));
        assert!(result.is_none());
    }

    #[test]
    fn test_convert_resize() {
        let ct_event = Event::Resize(120, 40);
        let result = convert_event(ct_event).unwrap();
        assert_eq!(result, InputEvent::Resize(120, 40));
    }

    #[test]
    fn test_convert_focus() {
        assert_eq!(
            convert_event(Event::FocusGained).unwrap(),
            InputEvent::FocusGained
        );
        assert_eq!(
            convert_event(Event::FocusLost).unwrap(),
            InputEvent::FocusLost
        );
    }

    #[test]
    fn test_convert_mouse_click() {
        let ct_event = Event::Mouse(CtMouseEvent {
            kind: CtMouseEventKind::Down(CtMouseButton::Left),
            column: 10,
            row: 5,
            modifiers: KeyModifiers::NONE,
        });
        let result = convert_event(ct_event).unwrap();
        match result {
            InputEvent::Mouse(m) => {
                assert_eq!(m.kind, MouseEventKind::Press(MouseButton::Left));
                assert_eq!(m.line, 5);
                assert_eq!(m.column, 10);
            }
            _ => panic!("expected Mouse event"),
        }
    }

    #[test]
    fn test_convert_drag_left() {
        let ct_event = Event::Mouse(CtMouseEvent {
            kind: CtMouseEventKind::Drag(CtMouseButton::Left),
            column: 5,
            row: 3,
            modifiers: KeyModifiers::NONE,
        });
        let result = convert_event(ct_event).unwrap();
        match result {
            InputEvent::Mouse(m) => {
                assert_eq!(m.kind, MouseEventKind::Drag(MouseButton::Left));
                assert_eq!(m.line, 3);
                assert_eq!(m.column, 5);
            }
            _ => panic!("expected Mouse event"),
        }
    }

    #[test]
    fn test_convert_drag_right() {
        let ct_event = Event::Mouse(CtMouseEvent {
            kind: CtMouseEventKind::Drag(CtMouseButton::Right),
            column: 1,
            row: 2,
            modifiers: KeyModifiers::NONE,
        });
        let result = convert_event(ct_event).unwrap();
        match result {
            InputEvent::Mouse(m) => {
                assert_eq!(m.kind, MouseEventKind::Drag(MouseButton::Right));
            }
            _ => panic!("expected Mouse event"),
        }
    }

    #[test]
    fn test_convert_paste() {
        let ct_event = Event::Paste("hello world".to_string());
        let result = convert_event(ct_event).unwrap();
        assert_eq!(result, InputEvent::Paste("hello world".to_string()));
    }

    #[test]
    fn test_convert_scroll() {
        let ct_event = Event::Mouse(CtMouseEvent {
            kind: CtMouseEventKind::ScrollUp,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });
        let result = convert_event(ct_event).unwrap();
        match result {
            InputEvent::Mouse(m) => {
                assert_eq!(m.kind, MouseEventKind::ScrollUp);
            }
            _ => panic!("expected Mouse event"),
        }
    }
}
