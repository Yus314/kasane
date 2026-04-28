//! Input conversion from frontend key/mouse events to Kakoune protocol input.

pub mod builtin;
pub mod builtin_drag;
pub mod builtin_fold;
pub mod builtin_mouse;
pub mod key_map;
pub use builtin::BuiltinInputPlugin;
pub use builtin_drag::BuiltinDragPlugin;
pub use builtin_fold::BuiltinFoldPlugin;
pub use builtin_mouse::BuiltinMouseFallbackPlugin;
pub use key_map::{ChordBinding, ChordState, CompiledKeyMap, KeyBinding, KeyGroup};

use std::path::{Path, PathBuf};

use bitflags::bitflags;

use crate::plugin::Command;
use crate::protocol::StatusStyle;
use crate::session::SessionId;
use crate::state::AppState;
use crate::state::derived::EditorMode;

// ---------------------------------------------------------------------------
// Input event types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    Key(KeyEvent),
    TextInput(String),
    Mouse(MouseEvent),
    Paste(String),
    Resize(u16, u16),
    FocusGained,
    FocusLost,
    Drop(DropEvent),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextInputTargetKind {
    Buffer,
    Prompt(StatusStyle),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextInputTargetAuthority {
    ObservedStatusStyle,
    ObservedPromptCursor,
    HeuristicModeLine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedTextInputTarget {
    pub session_id: Option<SessionId>,
    pub kind: TextInputTargetKind,
    pub authority: TextInputTargetAuthority,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DropEvent {
    pub paths: Vec<PathBuf>,
    pub col: u16,
    pub row: u16,
}

/// Quote a file path for Kakoune's command parser.
/// Uses single-quote wrapping: `'` is escaped as `''`.
pub fn kakoune_quote_path(path: &Path) -> String {
    let s = path.to_string_lossy();
    format!("'{}'", s.replace('\'', "''"))
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
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Modifiers: u8 {
        const CTRL  = 0b0000_0001;
        const ALT   = 0b0000_0010;
        const SHIFT = 0b0000_0100;
    }
}

// ---------------------------------------------------------------------------
// KeyResponse — action result from key map dispatch
// ---------------------------------------------------------------------------

/// Result of a key action invoked via `CompiledKeyMap` dispatch.
///
/// Unlike [`KeyHandleResult`](crate::plugin::KeyHandleResult), this type is
/// designed for the new declarative key map system and does not include
/// `Transformed` (key rewriting is not part of the key map protocol).
pub enum KeyResponse {
    /// Key was not handled — pass to next plugin.
    Pass,
    /// Key was consumed, no commands to emit.
    Consume,
    /// Key was consumed, request a redraw.
    ConsumeRedraw,
    /// Key was consumed, emit these commands.
    ConsumeWith(Vec<Command>),
}

// ---------------------------------------------------------------------------
// KeyPattern — declarative key matching
// ---------------------------------------------------------------------------

/// A pattern for matching [`KeyEvent`]s in key map bindings.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum KeyPattern {
    /// Match an exact key+modifier combination.
    Exact(KeyEvent),
    /// Match any `Key::Char(_)`, regardless of modifiers.
    AnyChar,
    /// Match any `Key::Char(_)` with no Ctrl or Alt modifiers.
    AnyCharPlain,
    /// Catch-all: matches any key event.
    Any,
}

impl KeyPattern {
    /// Test whether this pattern matches the given key event.
    pub fn matches(&self, event: &KeyEvent) -> bool {
        match self {
            KeyPattern::Exact(expected) => event == expected,
            KeyPattern::AnyChar => matches!(event.key, Key::Char(_)),
            KeyPattern::AnyCharPlain => {
                matches!(event.key, Key::Char(_))
                    && !event.modifiers.intersects(Modifiers::CTRL | Modifiers::ALT)
            }
            KeyPattern::Any => true,
        }
    }
}

// ---------------------------------------------------------------------------
// KeyEvent convenience constructors and matchers
// ---------------------------------------------------------------------------

impl KeyEvent {
    /// A plain character key with no modifiers.
    pub fn char_plain(c: char) -> Self {
        Self {
            key: Key::Char(c),
            modifiers: Modifiers::empty(),
        }
    }

    /// A Ctrl+char key.
    pub fn ctrl(c: char) -> Self {
        Self {
            key: Key::Char(c),
            modifiers: Modifiers::CTRL,
        }
    }

    /// Test whether this event is Ctrl+`c`.
    pub fn matches_ctrl(&self, c: char) -> bool {
        self.key == Key::Char(c) && self.modifiers == Modifiers::CTRL
    }

    /// Test whether this event is a plain (no Ctrl/Alt) character `c`.
    pub fn matches_char_plain(&self, c: char) -> bool {
        self.key == Key::Char(c) && !self.modifiers.intersects(Modifiers::CTRL | Modifiers::ALT)
    }

    /// Extract the character if this is a plain key (no Ctrl/Alt).
    pub fn plain_char(&self) -> Option<char> {
        match self.key {
            Key::Char(c) if !self.modifiers.intersects(Modifiers::CTRL | Modifiers::ALT) => Some(c),
            _ => None,
        }
    }
}

pub fn resolve_text_input_target(
    state: &AppState,
    session_id: Option<SessionId>,
) -> Option<ResolvedTextInputTarget> {
    match state.observed.status_style {
        StatusStyle::Command | StatusStyle::Search | StatusStyle::Prompt => {
            return Some(ResolvedTextInputTarget {
                session_id,
                kind: TextInputTargetKind::Prompt(state.observed.status_style),
                authority: TextInputTargetAuthority::ObservedStatusStyle,
            });
        }
        StatusStyle::Status => {}
    }

    if state.observed.status_content_cursor_pos >= 0 {
        return Some(ResolvedTextInputTarget {
            session_id,
            kind: TextInputTargetKind::Prompt(state.observed.status_style),
            authority: TextInputTargetAuthority::ObservedPromptCursor,
        });
    }

    match state.inference.editor_mode {
        EditorMode::Insert | EditorMode::Replace => Some(ResolvedTextInputTarget {
            session_id,
            kind: TextInputTargetKind::Buffer,
            authority: TextInputTargetAuthority::HeuristicModeLine,
        }),
        _ => None,
    }
}

/// Normalize plain character keys into semantic text input when a text target exists.
pub fn normalize_text_input_event(input: InputEvent, state: &AppState) -> InputEvent {
    if let InputEvent::Key(ref key) = input
        && resolve_text_input_target(state, None).is_some()
        && let Some(ch) = key.plain_char()
    {
        return InputEvent::TextInput(ch.to_string());
    }
    input
}

impl std::hash::Hash for KeyEvent {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.key.hash(state);
        self.modifiers.hash(state);
    }
}

impl std::hash::Hash for Key {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        match self {
            Key::Char(c) => c.hash(state),
            Key::F(n) => n.hash(state),
            _ => {}
        }
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
///
/// When `display_map` is provided and non-identity, `event.line` (which is in
/// display-space) is translated back to buffer-space before being sent to
/// Kakoune.  Lines with `InteractionPolicy::ReadOnly` or `Skip` suppress the
/// event (returns `None`).
pub fn mouse_to_kakoune(
    event: &MouseEvent,
    scroll_amount: i32,
    display_map: Option<&crate::display::DisplayMap>,
    display_scroll_offset: usize,
    segment_map: Option<&crate::display::segment_map::SegmentMap>,
) -> Option<crate::protocol::KasaneRequest> {
    use crate::display::InverseResult;
    use crate::protocol::KasaneRequest;

    let (line, column) = if let Some(sm) = segment_map {
        // Two-layer inverse: screen_y → display_line → buffer_line
        let screen_y = event.line as usize;
        let display_y = sm.screen_y_to_display_line(screen_y)?;
        let dm = display_map?;
        match dm.display_to_buffer(crate::display::DisplayLine(display_y)) {
            InverseResult::Actionable(bl) => (bl.0 as u32, event.column),
            _ => return None,
        }
    } else if let Some(dm) = display_map.filter(|dm| !dm.is_identity()) {
        let display_y = event.line as usize + display_scroll_offset;
        // Inverse projection: only Actionable (strong source) generates a Kakoune event.
        // Informational (fold), OutOfRange → suppress.
        match dm.display_to_buffer(crate::display::DisplayLine(display_y)) {
            InverseResult::Actionable(bl) => (bl.0 as u32, event.column),
            _ => return None,
        }
    } else {
        (event.line + display_scroll_offset as u32, event.column)
    };

    match event.kind {
        MouseEventKind::Press(button) => Some(KasaneRequest::MousePress {
            button: mouse_button_str(button).to_string(),
            line,
            column,
        }),
        MouseEventKind::Release(button) => Some(KasaneRequest::MouseRelease {
            button: mouse_button_str(button).to_string(),
            line,
            column,
        }),
        MouseEventKind::Move | MouseEventKind::Drag(_) => {
            Some(KasaneRequest::MouseMove { line, column })
        }
        MouseEventKind::ScrollUp => Some(KasaneRequest::Scroll {
            amount: -scroll_amount,
            line,
            column,
        }),
        MouseEventKind::ScrollDown => Some(KasaneRequest::Scroll {
            amount: scroll_amount,
            line,
            column,
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
    use crate::state::AppState;
    use crate::state::derived::EditorMode;

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
        let req = mouse_to_kakoune(&evt, 3, None, 0, None).unwrap();
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
        let req = mouse_to_kakoune(&evt, 3, None, 0, None).unwrap();
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
        let req = mouse_to_kakoune(&evt, 3, None, 0, None).unwrap();
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
    fn test_normalize_text_input_event_upgrades_prompt_plain_char() {
        let mut state = AppState::default();
        state.observed.status_content_cursor_pos = 0;
        let normalized =
            normalize_text_input_event(InputEvent::Key(KeyEvent::char_plain('a')), &state);
        assert_eq!(normalized, InputEvent::TextInput("a".into()));
    }

    #[test]
    fn test_normalize_text_input_event_upgrades_insert_plain_char() {
        let mut state = AppState::default();
        state.inference.editor_mode = EditorMode::Insert;
        let normalized = normalize_text_input_event(
            InputEvent::Key(KeyEvent {
                key: Key::Char('A'),
                modifiers: Modifiers::SHIFT,
            }),
            &state,
        );
        assert_eq!(normalized, InputEvent::TextInput("A".into()));
    }

    #[test]
    fn test_normalize_text_input_event_preserves_non_text_key_paths() {
        let state = AppState::default();
        let normal = normalize_text_input_event(InputEvent::Key(KeyEvent::char_plain('a')), &state);
        assert_eq!(normal, InputEvent::Key(KeyEvent::char_plain('a')));

        let ctrl = normalize_text_input_event(InputEvent::Key(KeyEvent::ctrl('c')), &{
            let mut s = AppState::default();
            s.inference.editor_mode = EditorMode::Insert;
            s
        });
        assert_eq!(ctrl, InputEvent::Key(KeyEvent::ctrl('c')));
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
        let req = mouse_to_kakoune(&evt, 3, None, 0, None).unwrap();
        assert_eq!(
            req,
            KasaneRequest::Scroll {
                amount: 3,
                line: 0,
                column: 0
            }
        );
    }

    // -----------------------------------------------------------------------
    // kakoune_quote_path tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_kakoune_quote_path_simple() {
        use std::path::Path;
        assert_eq!(
            kakoune_quote_path(Path::new("/tmp/foo.txt")),
            "'/tmp/foo.txt'"
        );
    }

    #[test]
    fn test_kakoune_quote_path_spaces() {
        use std::path::Path;
        assert_eq!(
            kakoune_quote_path(Path::new("/tmp/my file.txt")),
            "'/tmp/my file.txt'"
        );
    }

    #[test]
    fn test_kakoune_quote_path_single_quote() {
        use std::path::Path;
        assert_eq!(
            kakoune_quote_path(Path::new("/tmp/it's a file.txt")),
            "'/tmp/it''s a file.txt'"
        );
    }

    // -----------------------------------------------------------------------
    // DisplayMap-aware mouse coordinate translation tests
    // -----------------------------------------------------------------------

    /// Click on a display line after a fold: buffer line should be translated
    /// via display_to_buffer.
    #[test]
    fn test_mouse_with_fold_display_map() {
        use crate::display::{DisplayDirective, DisplayMap};
        use crate::protocol::{Atom, KasaneRequest};

        // 10 buffer lines, fold lines 2..5 into a summary
        let dm = DisplayMap::build(
            10,
            &[DisplayDirective::Fold {
                range: 2..5,
                summary: vec![Atom::from_face(Default::default(), "--- folded ---")],
            }],
        );
        // Display lines: 0=buf0, 1=buf1, 2=fold(2..5), 3=buf5, 4=buf6, ...
        // Click on display line 4 (= buffer line 6)
        let evt = MouseEvent {
            kind: MouseEventKind::Press(MouseButton::Left),
            line: 4,
            column: 3,
            modifiers: Modifiers::empty(),
        };
        let req = mouse_to_kakoune(&evt, 3, Some(&dm), 0, None).unwrap();
        assert_eq!(
            req,
            KasaneRequest::MousePress {
                button: "left".to_string(),
                line: 6,
                column: 3,
            }
        );
    }

    /// Click on a fold summary line (ReadOnly) should be suppressed.
    #[test]
    fn test_mouse_on_fold_summary_suppressed() {
        use crate::display::{DisplayDirective, DisplayMap};
        use crate::protocol::Atom;

        let dm = DisplayMap::build(
            10,
            &[DisplayDirective::Fold {
                range: 2..5,
                summary: vec![Atom::from_face(Default::default(), "--- folded ---")],
            }],
        );
        // Display line 2 is the fold summary (ReadOnly)
        let evt = MouseEvent {
            kind: MouseEventKind::Press(MouseButton::Left),
            line: 2,
            column: 0,
            modifiers: Modifiers::empty(),
        };
        assert!(mouse_to_kakoune(&evt, 3, Some(&dm), 0, None).is_none());
    }

    /// Click on a display line after hidden lines: buffer line should be
    /// correctly offset past the hidden range.
    #[test]
    fn test_mouse_with_hidden_lines() {
        use crate::display::{DisplayDirective, DisplayMap};
        use crate::protocol::KasaneRequest;

        // 10 buffer lines, hide lines 3..6
        let dm = DisplayMap::build(10, &[DisplayDirective::Hide { range: 3..6 }]);
        // Display lines: 0=buf0, 1=buf1, 2=buf2, 3=buf6, 4=buf7, ...
        let evt = MouseEvent {
            kind: MouseEventKind::Press(MouseButton::Left),
            line: 3,
            column: 5,
            modifiers: Modifiers::empty(),
        };
        let req = mouse_to_kakoune(&evt, 3, Some(&dm), 0, None).unwrap();
        assert_eq!(
            req,
            KasaneRequest::MousePress {
                button: "left".to_string(),
                line: 6,
                column: 5,
            }
        );
    }

    /// Click with both scroll offset and DisplayMap: both offsets applied correctly.
    #[test]
    fn test_mouse_with_scroll_offset_and_display_map() {
        use crate::display::{DisplayDirective, DisplayMap};
        use crate::protocol::{Atom, KasaneRequest};

        // 20 buffer lines, fold lines 5..10 into summary
        let dm = DisplayMap::build(
            20,
            &[DisplayDirective::Fold {
                range: 5..10,
                summary: vec![Atom::from_face(Default::default(), "folded")],
            }],
        );
        // Display lines: 0-4=buf0-4, 5=fold(5..10), 6=buf10, 7=buf11, ...
        // With scroll offset 3, display line 0 on screen maps to display line 3 in the map
        // Click on screen line 4 → display line 7 → buf11
        let evt = MouseEvent {
            kind: MouseEventKind::Press(MouseButton::Left),
            line: 4,
            column: 0,
            modifiers: Modifiers::empty(),
        };
        let req = mouse_to_kakoune(&evt, 3, Some(&dm), 3, None).unwrap();
        assert_eq!(
            req,
            KasaneRequest::MousePress {
                button: "left".to_string(),
                line: 11,
                column: 0,
            }
        );
    }
}
