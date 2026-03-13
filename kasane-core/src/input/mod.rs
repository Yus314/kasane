pub mod builtin;
pub use builtin::BuiltinInputPlugin;

use bitflags::bitflags;

// ---------------------------------------------------------------------------
// Input event types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Paste(String),
    Resize(u16, u16),
    FocusGained,
    FocusLost,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: Key,
    pub modifiers: Modifiers,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Backspace,
    Delete,
    Enter,
    Tab,
    Escape,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Modifiers: u8 {
        const CTRL  = 0b0000_0001;
        const ALT   = 0b0000_0010;
        const SHIFT = 0b0000_0100;
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MouseEvent {
    pub kind: MouseEventKind,
    pub line: u32,
    pub column: u32,
    pub modifiers: Modifiers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseEventKind {
    Press(MouseButton),
    Release(MouseButton),
    Move,
    Drag(MouseButton),
    ScrollUp,
    ScrollDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

// ---------------------------------------------------------------------------
// Kakoune key format conversion
// ---------------------------------------------------------------------------

pub fn key_to_kakoune(event: &KeyEvent) -> String {
    let base = match &event.key {
        Key::Char(' ') => "space".to_string(),
        Key::Char('<') => "lt".to_string(),
        Key::Char('>') => "gt".to_string(),
        Key::Char('-') if event.modifiers.is_empty() => "minus".to_string(),
        Key::Char(c) => {
            if event.modifiers.contains(Modifiers::SHIFT) && c.is_ascii_lowercase() {
                // Shift + letter → uppercase, no s- prefix
                return format_with_modifiers(
                    &c.to_ascii_uppercase().to_string(),
                    event.modifiers & !Modifiers::SHIFT,
                );
            }
            c.to_string()
        }
        Key::Backspace => "backspace".to_string(),
        Key::Delete => "del".to_string(),
        Key::Enter => "ret".to_string(),
        Key::Tab => "tab".to_string(),
        Key::Escape => "esc".to_string(),
        Key::Up => "up".to_string(),
        Key::Down => "down".to_string(),
        Key::Left => "left".to_string(),
        Key::Right => "right".to_string(),
        Key::Home => "home".to_string(),
        Key::End => "end".to_string(),
        Key::PageUp => "pageup".to_string(),
        Key::PageDown => "pagedown".to_string(),
        Key::F(n) => format!("F{n}"),
    };

    format_with_modifiers(&base, event.modifiers)
}

fn format_with_modifiers(base: &str, modifiers: Modifiers) -> String {
    if modifiers.is_empty() && base.len() == 1 {
        return base.to_string();
    }

    let mut prefix = String::new();
    if modifiers.contains(Modifiers::CTRL) {
        prefix.push_str("c-");
    }
    if modifiers.contains(Modifiers::ALT) {
        prefix.push_str("a-");
    }
    if modifiers.contains(Modifiers::SHIFT) {
        prefix.push_str("s-");
    }

    if prefix.is_empty() && base.len() == 1 {
        base.to_string()
    } else {
        format!("<{prefix}{base}>")
    }
}

/// Convert a mouse event to a KasaneRequest-compatible representation.
pub fn mouse_to_kakoune(
    event: &MouseEvent,
    scroll_amount: i32,
) -> Option<crate::protocol::KasaneRequest> {
    use crate::protocol::KasaneRequest;

    match event.kind {
        MouseEventKind::Press(button) => Some(KasaneRequest::MousePress {
            button: mouse_button_str(button).to_string(),
            line: event.line,
            column: event.column,
        }),
        MouseEventKind::Release(button) => Some(KasaneRequest::MouseRelease {
            button: mouse_button_str(button).to_string(),
            line: event.line,
            column: event.column,
        }),
        MouseEventKind::Move | MouseEventKind::Drag(_) => Some(KasaneRequest::MouseMove {
            line: event.line,
            column: event.column,
        }),
        MouseEventKind::ScrollUp => Some(KasaneRequest::Scroll {
            amount: -scroll_amount,
            line: event.line,
            column: event.column,
        }),
        MouseEventKind::ScrollDown => Some(KasaneRequest::Scroll {
            amount: scroll_amount,
            line: event.line,
            column: event.column,
        }),
    }
}

/// Convert pasted text into Kakoune key sequence.
/// Each character is mapped to the appropriate Kakoune key name.
pub fn paste_text_to_keys(text: &str) -> Vec<String> {
    text.chars()
        .map(|c| match c {
            '\n' => "<ret>".to_string(),
            ' ' => "<space>".to_string(),
            '<' => "<lt>".to_string(),
            '>' => "<gt>".to_string(),
            '-' => "<minus>".to_string(),
            c => c.to_string(),
        })
        .collect()
}

fn mouse_button_str(button: MouseButton) -> &'static str {
    match button {
        MouseButton::Left => "left",
        MouseButton::Middle => "middle",
        MouseButton::Right => "right",
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn key(k: Key, m: Modifiers) -> KeyEvent {
        KeyEvent {
            key: k,
            modifiers: m,
        }
    }

    #[test]
    fn test_plain_char() {
        assert_eq!(
            key_to_kakoune(&key(Key::Char('a'), Modifiers::empty())),
            "a"
        );
        assert_eq!(
            key_to_kakoune(&key(Key::Char('Z'), Modifiers::empty())),
            "Z"
        );
    }

    #[test]
    fn test_ctrl_char() {
        assert_eq!(
            key_to_kakoune(&key(Key::Char('a'), Modifiers::CTRL)),
            "<c-a>"
        );
        assert_eq!(
            key_to_kakoune(&key(Key::Char('x'), Modifiers::CTRL)),
            "<c-x>"
        );
    }

    #[test]
    fn test_alt_char() {
        assert_eq!(
            key_to_kakoune(&key(Key::Char('x'), Modifiers::ALT)),
            "<a-x>"
        );
    }

    #[test]
    fn test_shift_letter() {
        // shift+a → A (uppercase, no s- prefix)
        assert_eq!(key_to_kakoune(&key(Key::Char('a'), Modifiers::SHIFT)), "A");
    }

    #[test]
    fn test_ctrl_alt() {
        assert_eq!(
            key_to_kakoune(&key(Key::Char('a'), Modifiers::CTRL | Modifiers::ALT)),
            "<c-a-a>"
        );
    }

    #[test]
    fn test_special_keys() {
        assert_eq!(
            key_to_kakoune(&key(Key::Enter, Modifiers::empty())),
            "<ret>"
        );
        assert_eq!(
            key_to_kakoune(&key(Key::Escape, Modifiers::empty())),
            "<esc>"
        );
        assert_eq!(key_to_kakoune(&key(Key::Tab, Modifiers::empty())), "<tab>");
        assert_eq!(key_to_kakoune(&key(Key::Tab, Modifiers::SHIFT)), "<s-tab>");
        assert_eq!(
            key_to_kakoune(&key(Key::Backspace, Modifiers::empty())),
            "<backspace>"
        );
        assert_eq!(
            key_to_kakoune(&key(Key::Delete, Modifiers::empty())),
            "<del>"
        );
    }

    #[test]
    fn test_space_and_angle_brackets() {
        assert_eq!(
            key_to_kakoune(&key(Key::Char(' '), Modifiers::empty())),
            "<space>"
        );
        assert_eq!(
            key_to_kakoune(&key(Key::Char('<'), Modifiers::empty())),
            "<lt>"
        );
        assert_eq!(
            key_to_kakoune(&key(Key::Char('>'), Modifiers::empty())),
            "<gt>"
        );
    }

    #[test]
    fn test_arrow_keys() {
        assert_eq!(key_to_kakoune(&key(Key::Up, Modifiers::empty())), "<up>");
        assert_eq!(
            key_to_kakoune(&key(Key::Down, Modifiers::empty())),
            "<down>"
        );
        assert_eq!(
            key_to_kakoune(&key(Key::Left, Modifiers::SHIFT)),
            "<s-left>"
        );
        assert_eq!(
            key_to_kakoune(&key(Key::Right, Modifiers::SHIFT)),
            "<s-right>"
        );
    }

    #[test]
    fn test_function_keys() {
        assert_eq!(key_to_kakoune(&key(Key::F(1), Modifiers::empty())), "<F1>");
        assert_eq!(
            key_to_kakoune(&key(Key::F(12), Modifiers::empty())),
            "<F12>"
        );
    }

    #[test]
    fn test_page_keys() {
        assert_eq!(
            key_to_kakoune(&key(Key::PageUp, Modifiers::empty())),
            "<pageup>"
        );
        assert_eq!(
            key_to_kakoune(&key(Key::PageDown, Modifiers::empty())),
            "<pagedown>"
        );
        assert_eq!(
            key_to_kakoune(&key(Key::Home, Modifiers::empty())),
            "<home>"
        );
        assert_eq!(key_to_kakoune(&key(Key::End, Modifiers::empty())), "<end>");
    }

    #[test]
    fn test_minus_key() {
        assert_eq!(
            key_to_kakoune(&key(Key::Char('-'), Modifiers::empty())),
            "<minus>"
        );
    }

    #[test]
    fn test_mouse_to_kakoune() {
        use crate::protocol::KasaneRequest;

        let evt = MouseEvent {
            kind: MouseEventKind::Press(MouseButton::Left),
            line: 5,
            column: 10,
            modifiers: Modifiers::empty(),
        };
        let req = mouse_to_kakoune(&evt, 3).unwrap();
        assert_eq!(
            req,
            KasaneRequest::MousePress {
                button: "left".to_string(),
                line: 5,
                column: 10,
            }
        );
    }

    #[test]
    fn test_drag_to_kakoune() {
        use crate::protocol::KasaneRequest;

        let evt = MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            line: 3,
            column: 7,
            modifiers: Modifiers::empty(),
        };
        let req = mouse_to_kakoune(&evt, 3).unwrap();
        assert_eq!(req, KasaneRequest::MouseMove { line: 3, column: 7 });
    }

    #[test]
    fn test_right_drag_to_kakoune() {
        use crate::protocol::KasaneRequest;

        let evt = MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Right),
            line: 1,
            column: 2,
            modifiers: Modifiers::empty(),
        };
        let req = mouse_to_kakoune(&evt, 3).unwrap();
        assert_eq!(req, KasaneRequest::MouseMove { line: 1, column: 2 });
    }

    #[test]
    fn test_paste_text_to_keys_basic() {
        let keys = paste_text_to_keys("hello");
        assert_eq!(keys, vec!["h", "e", "l", "l", "o"]);
    }

    #[test]
    fn test_paste_text_to_keys_special_chars() {
        let keys = paste_text_to_keys("a b\n<>-");
        assert_eq!(
            keys,
            vec!["a", "<space>", "b", "<ret>", "<lt>", "<gt>", "<minus>"]
        );
    }

    #[test]
    fn test_paste_text_to_keys_empty() {
        let keys = paste_text_to_keys("");
        assert!(keys.is_empty());
    }

    #[test]
    fn test_paste_text_to_keys_multibyte() {
        let keys = paste_text_to_keys("日本語");
        assert_eq!(keys, vec!["日", "本", "語"]);
    }

    #[test]
    fn test_scroll_to_kakoune() {
        use crate::protocol::KasaneRequest;

        let evt = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            line: 0,
            column: 0,
            modifiers: Modifiers::empty(),
        };
        let req = mouse_to_kakoune(&evt, 3).unwrap();
        assert_eq!(
            req,
            KasaneRequest::Scroll {
                amount: 3,
                line: 0,
                column: 0
            }
        );
    }
}
