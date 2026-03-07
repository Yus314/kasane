use kasane_core::input::{InputEvent, Key, KeyEvent, Modifiers, MouseButton, MouseEvent, MouseEventKind};
use winit::event::{ElementState, Ime, MouseScrollDelta, WindowEvent};
use winit::keyboard::{Key as WinitKey, NamedKey};

use crate::gpu::CellMetrics;

/// Convert a winit `WindowEvent` to a kasane `InputEvent`.
pub fn convert_window_event(
    event: &WindowEvent,
    cell_metrics: &CellMetrics,
    cursor_pos: &mut Option<(f64, f64)>,
    mouse_button_held: &mut Option<MouseButton>,
) -> Vec<InputEvent> {
    match event {
        WindowEvent::KeyboardInput { event, .. } => {
            if event.state != ElementState::Pressed {
                return vec![];
            }
            convert_key_event(event).into_iter().collect()
        }
        WindowEvent::Ime(ime) => convert_ime(ime),
        WindowEvent::Resized(size) => {
            let cols = (size.width as f32 / cell_metrics.cell_width).floor().max(1.0) as u16;
            let rows = (size.height as f32 / cell_metrics.cell_height).floor().max(1.0) as u16;
            vec![InputEvent::Resize(cols, rows)]
        }
        WindowEvent::Focused(true) => vec![InputEvent::FocusGained],
        WindowEvent::Focused(false) => vec![InputEvent::FocusLost],
        WindowEvent::CursorMoved { position, .. } => {
            *cursor_pos = Some((position.x, position.y));
            // Generate drag event if a button is held
            if let Some(btn) = *mouse_button_held {
                let (col, row) = pixel_to_grid(position.x, position.y, cell_metrics);
                vec![InputEvent::Mouse(MouseEvent {
                    kind: MouseEventKind::Drag(btn),
                    line: row as u32,
                    column: col as u32,
                })]
            } else {
                vec![]
            }
        }
        WindowEvent::MouseInput { state, button, .. } => {
            let Some((px, py)) = *cursor_pos else {
                return vec![];
            };
            let (col, row) = pixel_to_grid(px, py, cell_metrics);
            let btn = match button {
                winit::event::MouseButton::Left => MouseButton::Left,
                winit::event::MouseButton::Right => MouseButton::Right,
                winit::event::MouseButton::Middle => MouseButton::Middle,
                _ => return vec![],
            };
            match state {
                ElementState::Pressed => {
                    *mouse_button_held = Some(btn);
                    vec![InputEvent::Mouse(MouseEvent {
                        kind: MouseEventKind::Press(btn),
                        line: row as u32,
                        column: col as u32,
                    })]
                }
                ElementState::Released => {
                    *mouse_button_held = None;
                    vec![InputEvent::Mouse(MouseEvent {
                        kind: MouseEventKind::Release(btn),
                        line: row as u32,
                        column: col as u32,
                    })]
                }
            }
        }
        WindowEvent::MouseWheel { delta, .. } => {
            let Some((px, py)) = *cursor_pos else {
                return vec![];
            };
            let (col, row) = pixel_to_grid(px, py, cell_metrics);
            let lines = match delta {
                MouseScrollDelta::LineDelta(_, y) => *y as i32,
                MouseScrollDelta::PixelDelta(pos) => {
                    (pos.y / cell_metrics.cell_height as f64) as i32
                }
            };
            if lines > 0 {
                vec![InputEvent::Mouse(MouseEvent {
                    kind: MouseEventKind::ScrollUp,
                    line: row as u32,
                    column: col as u32,
                })]
            } else if lines < 0 {
                vec![InputEvent::Mouse(MouseEvent {
                    kind: MouseEventKind::ScrollDown,
                    line: row as u32,
                    column: col as u32,
                })]
            } else {
                vec![]
            }
        }
        WindowEvent::DroppedFile(path) => {
            // Send `:edit <path>` to Kakoune
            let cmd = format!(":edit {}\n", path.display());
            let keys = kasane_core::input::paste_text_to_keys(&cmd);
            keys.into_iter()
                .filter_map(|k| {
                    if k.len() == 1 {
                        let ch = k.chars().next().unwrap();
                        Some(InputEvent::Key(KeyEvent {
                            key: Key::Char(ch),
                            modifiers: Modifiers::empty(),
                        }))
                    } else {
                        // Special key like <ret>, <space>, etc.
                        parse_kakoune_key(&k).map(InputEvent::Key)
                    }
                })
                .collect()
        }
        _ => vec![],
    }
}

/// Convert a winit keyboard event to a kasane KeyEvent.
fn convert_key_event(event: &winit::event::KeyEvent) -> Option<InputEvent> {
    let modifiers = Modifiers::empty(); // filled in from ModifiersChanged state
    let key = match &event.logical_key {
        WinitKey::Character(s) => {
            let ch = s.chars().next()?;
            Key::Char(ch)
        }
        WinitKey::Named(named) => match named {
            NamedKey::Enter => Key::Enter,
            NamedKey::Escape => Key::Escape,
            NamedKey::Backspace => Key::Backspace,
            NamedKey::Delete => Key::Delete,
            NamedKey::Tab => Key::Tab,
            NamedKey::ArrowUp => Key::Up,
            NamedKey::ArrowDown => Key::Down,
            NamedKey::ArrowLeft => Key::Left,
            NamedKey::ArrowRight => Key::Right,
            NamedKey::Home => Key::Home,
            NamedKey::End => Key::End,
            NamedKey::PageUp => Key::PageUp,
            NamedKey::PageDown => Key::PageDown,
            NamedKey::F1 => Key::F(1),
            NamedKey::F2 => Key::F(2),
            NamedKey::F3 => Key::F(3),
            NamedKey::F4 => Key::F(4),
            NamedKey::F5 => Key::F(5),
            NamedKey::F6 => Key::F(6),
            NamedKey::F7 => Key::F(7),
            NamedKey::F8 => Key::F(8),
            NamedKey::F9 => Key::F(9),
            NamedKey::F10 => Key::F(10),
            NamedKey::F11 => Key::F(11),
            NamedKey::F12 => Key::F(12),
            NamedKey::Space => Key::Char(' '),
            _ => return None,
        },
        _ => return None,
    };
    Some(InputEvent::Key(KeyEvent { key, modifiers }))
}

/// Apply winit modifier state to a kasane KeyEvent.
///
/// For `Key::Char`, winit's `logical_key` already reflects the Shift state
/// (e.g., `;` + Shift → `:`). Kakoune only accepts Shift on special keys
/// and lowercase ASCII, so we only add Shift for `Key::Char` when the
/// character is a lowercase letter (a-z). For all other characters, the
/// shifted form is already encoded in the character itself.
pub fn apply_modifiers(event: &mut InputEvent, winit_mods: &winit::keyboard::ModifiersState) {
    if let InputEvent::Key(ke) = event {
        let mut mods = Modifiers::empty();
        if winit_mods.control_key() {
            mods |= Modifiers::CTRL;
        }
        if winit_mods.alt_key() {
            mods |= Modifiers::ALT;
        }
        if winit_mods.shift_key() {
            let apply_shift = match ke.key {
                // Shift is already baked into the character by winit.
                // Only keep it for lowercase ASCII (Kakoune's <s-a> = A).
                Key::Char(ch) => ch.is_ascii_lowercase(),
                // Special keys always accept Shift.
                _ => true,
            };
            if apply_shift {
                mods |= Modifiers::SHIFT;
            }
        }
        ke.modifiers = mods;
    }
}

fn convert_ime(ime: &Ime) -> Vec<InputEvent> {
    match ime {
        Ime::Commit(text) => text
            .chars()
            .map(|ch| {
                InputEvent::Key(KeyEvent {
                    key: Key::Char(ch),
                    modifiers: Modifiers::empty(),
                })
            })
            .collect(),
        Ime::Preedit(_, _) | Ime::Enabled | Ime::Disabled => vec![],
    }
}

/// Convert pixel coordinates to grid (col, row), clamped to grid bounds.
pub fn pixel_to_grid(px: f64, py: f64, metrics: &CellMetrics) -> (u16, u16) {
    let col = (px as f32 / metrics.cell_width).floor().max(0.0) as u16;
    let row = (py as f32 / metrics.cell_height).floor().max(0.0) as u16;
    (
        col.min(metrics.cols.saturating_sub(1)),
        row.min(metrics.rows.saturating_sub(1)),
    )
}

/// Parse a Kakoune key name back to a KeyEvent (for D&D file path injection).
fn parse_kakoune_key(s: &str) -> Option<KeyEvent> {
    let inner = s.strip_prefix('<').and_then(|s| s.strip_suffix('>'))?;
    let key = match inner {
        "ret" => Key::Enter,
        "space" => Key::Char(' '),
        "lt" => Key::Char('<'),
        "gt" => Key::Char('>'),
        "minus" => Key::Char('-'),
        "tab" => Key::Tab,
        "esc" => Key::Escape,
        "backspace" => Key::Backspace,
        "del" => Key::Delete,
        _ => return None,
    };
    Some(KeyEvent {
        key,
        modifiers: Modifiers::empty(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pixel_to_grid_basic() {
        let metrics = CellMetrics {
            cell_width: 10.0,
            cell_height: 20.0,
            baseline: 15.0,
            cols: 80,
            rows: 24,
        };
        assert_eq!(pixel_to_grid(0.0, 0.0, &metrics), (0, 0));
        assert_eq!(pixel_to_grid(15.0, 25.0, &metrics), (1, 1));
        assert_eq!(pixel_to_grid(799.0, 479.0, &metrics), (79, 23));
    }

    #[test]
    fn test_pixel_to_grid_clamped() {
        let metrics = CellMetrics {
            cell_width: 10.0,
            cell_height: 20.0,
            baseline: 15.0,
            cols: 80,
            rows: 24,
        };
        assert_eq!(pixel_to_grid(10000.0, 10000.0, &metrics), (79, 23));
        assert_eq!(pixel_to_grid(-5.0, -5.0, &metrics), (0, 0));
    }

    #[test]
    fn test_pixel_to_grid_hidpi() {
        // HiDPI: physical pixels are already accounted for in CellMetrics
        let metrics = CellMetrics {
            cell_width: 20.0, // 10 * 2.0 scale
            cell_height: 40.0,
            baseline: 30.0,
            cols: 80,
            rows: 24,
        };
        assert_eq!(pixel_to_grid(30.0, 50.0, &metrics), (1, 1));
    }

    #[test]
    fn test_parse_kakoune_key() {
        let ke = parse_kakoune_key("<ret>").unwrap();
        assert_eq!(ke.key, Key::Enter);

        let ke = parse_kakoune_key("<space>").unwrap();
        assert_eq!(ke.key, Key::Char(' '));

        assert!(parse_kakoune_key("a").is_none());
    }

    #[test]
    fn test_apply_modifiers_shift_symbol_stripped() {
        // Shift+; → ':' — Shift should NOT be added
        let mods = winit::keyboard::ModifiersState::SHIFT;
        let mut event = InputEvent::Key(KeyEvent {
            key: Key::Char(':'),
            modifiers: Modifiers::empty(),
        });
        apply_modifiers(&mut event, &mods);
        if let InputEvent::Key(ke) = event {
            assert!(!ke.modifiers.contains(Modifiers::SHIFT));
        }
    }

    #[test]
    fn test_apply_modifiers_shift_lowercase_kept() {
        // Ctrl+Shift held, char is lowercase 'a' — Shift should be kept
        let mods = winit::keyboard::ModifiersState::SHIFT | winit::keyboard::ModifiersState::CONTROL;
        let mut event = InputEvent::Key(KeyEvent {
            key: Key::Char('a'),
            modifiers: Modifiers::empty(),
        });
        apply_modifiers(&mut event, &mods);
        if let InputEvent::Key(ke) = event {
            assert!(ke.modifiers.contains(Modifiers::SHIFT));
            assert!(ke.modifiers.contains(Modifiers::CTRL));
        }
    }

    #[test]
    fn test_apply_modifiers_shift_special_key_kept() {
        // Shift+Left — Shift should be kept for special keys
        let mods = winit::keyboard::ModifiersState::SHIFT;
        let mut event = InputEvent::Key(KeyEvent {
            key: Key::Left,
            modifiers: Modifiers::empty(),
        });
        apply_modifiers(&mut event, &mods);
        if let InputEvent::Key(ke) = event {
            assert!(ke.modifiers.contains(Modifiers::SHIFT));
        }
    }

    #[test]
    fn test_apply_modifiers_shift_uppercase_stripped() {
        // Shift held, char is 'A' — Shift should NOT be added
        let mods = winit::keyboard::ModifiersState::SHIFT;
        let mut event = InputEvent::Key(KeyEvent {
            key: Key::Char('A'),
            modifiers: Modifiers::empty(),
        });
        apply_modifiers(&mut event, &mods);
        if let InputEvent::Key(ke) = event {
            assert!(!ke.modifiers.contains(Modifiers::SHIFT));
        }
    }
}
