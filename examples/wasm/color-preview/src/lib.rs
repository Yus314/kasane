kasane_plugin_sdk::generate!();

use std::collections::HashMap;

use exports::kasane::plugin::plugin_api::Guest;
use kasane::plugin::types::*;
use kasane::plugin::{element_builder, host_state};
use kasane_plugin_sdk::{dirty, modifiers, plugin};

// ---------------------------------------------------------------------------
// Color detection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColorFormat {
    HashRrggbb,
    HashRgb,
    RgbColon,
}

#[derive(Debug, Clone, PartialEq)]
struct ColorEntry {
    r: u8,
    g: u8,
    b: u8,
    byte_offset: usize,
    format: ColorFormat,
    original: String,
}

#[derive(Debug, Clone, PartialEq)]
struct ColorLine {
    colors: Vec<ColorEntry>,
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn parse_hex2(bytes: &[u8], offset: usize) -> Option<u8> {
    let hi = hex_digit(*bytes.get(offset)?)?;
    let lo = hex_digit(*bytes.get(offset + 1)?)?;
    Some(hi * 16 + lo)
}

fn is_hex(b: u8) -> bool {
    hex_digit(b).is_some()
}

fn detect_colors(text: &str) -> Vec<ColorEntry> {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut colors = Vec::new();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'#' {
            if i > 0 && is_hex(bytes[i - 1]) {
                i += 1;
                continue;
            }
            // Try #RRGGBB first
            if i + 7 <= len {
                if let (Some(r), Some(g), Some(b)) = (
                    parse_hex2(bytes, i + 1),
                    parse_hex2(bytes, i + 3),
                    parse_hex2(bytes, i + 5),
                ) {
                    if i + 7 >= len || !is_hex(bytes[i + 7]) {
                        colors.push(ColorEntry {
                            r,
                            g,
                            b,
                            byte_offset: i,
                            format: ColorFormat::HashRrggbb,
                            original: text[i..i + 7].to_string(),
                        });
                        i += 7;
                        continue;
                    }
                }
            }
            // Try #RGB
            if i + 4 <= len {
                if let (Some(r), Some(g), Some(b)) = (
                    hex_digit(bytes[i + 1]),
                    hex_digit(bytes[i + 2]),
                    hex_digit(bytes[i + 3]),
                ) {
                    if i + 4 >= len || !is_hex(bytes[i + 4]) {
                        colors.push(ColorEntry {
                            r: r * 16 + r,
                            g: g * 16 + g,
                            b: b * 16 + b,
                            byte_offset: i,
                            format: ColorFormat::HashRgb,
                            original: text[i..i + 4].to_string(),
                        });
                        i += 4;
                        continue;
                    }
                }
            }
        } else if i + 10 <= len && bytes[i..i + 4] == *b"rgb:" {
            if let (Some(r), Some(g), Some(b)) = (
                parse_hex2(bytes, i + 4),
                parse_hex2(bytes, i + 6),
                parse_hex2(bytes, i + 8),
            ) {
                if i + 10 >= len || !is_hex(bytes[i + 10]) {
                    colors.push(ColorEntry {
                        r,
                        g,
                        b,
                        byte_offset: i,
                        format: ColorFormat::RgbColon,
                        original: text[i..i + 10].to_string(),
                    });
                    i += 10;
                    continue;
                }
            }
        }
        i += 1;
    }

    colors
}

// ---------------------------------------------------------------------------
// Interactive overlay: ID encoding
// ---------------------------------------------------------------------------

const COLOR_PICKER_BASE: u32 = 2000;

fn encode_picker_id(color_idx: usize, channel: usize, is_down: bool) -> InteractiveId {
    COLOR_PICKER_BASE + (color_idx * 6 + channel + if is_down { 3 } else { 0 }) as u32
}

fn decode_picker_id(id: InteractiveId) -> Option<(usize, usize, bool)> {
    if id < COLOR_PICKER_BASE {
        return None;
    }
    let offset = (id - COLOR_PICKER_BASE) as usize;
    let color_idx = offset / 6;
    let rem = offset % 6;
    let is_down = rem >= 3;
    let channel = if is_down { rem - 3 } else { rem };
    if channel >= 3 {
        return None;
    }
    Some((color_idx, channel, is_down))
}

// ---------------------------------------------------------------------------
// Key escaping for Kakoune commands
// ---------------------------------------------------------------------------

fn push_literal_keys(keys: &mut Vec<String>, text: &str) {
    for ch in text.chars() {
        match ch {
            ' ' => keys.push("<space>".into()),
            '<' => keys.push("<lt>".into()),
            '>' => keys.push("<gt>".into()),
            '-' => keys.push("<minus>".into()),
            c => keys.push(c.to_string()),
        }
    }
}

fn format_color(channels: [u8; 3], format: ColorFormat) -> String {
    match format {
        ColorFormat::RgbColon => {
            format!("rgb:{:02x}{:02x}{:02x}", channels[0], channels[1], channels[2])
        }
        _ => format!("#{:02x}{:02x}{:02x}", channels[0], channels[1], channels[2]),
    }
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

thread_local! {
    static STATE: std::cell::RefCell<PluginState> = std::cell::RefCell::new(PluginState::default());
}

#[derive(Default)]
struct PluginState {
    color_lines: HashMap<usize, ColorLine>,
    active_line: i32,
    generation: u64,
}

fn default_face() -> Face {
    Face {
        fg: Color::DefaultColor,
        bg: Color::DefaultColor,
        underline: Color::DefaultColor,
        attributes: 0,
    }
}

// ---------------------------------------------------------------------------
// Element builders
// ---------------------------------------------------------------------------

fn build_swatch(colors: &[ColorEntry]) -> ElementHandle {
    let atoms: Vec<Atom> = colors
        .iter()
        .take(4)
        .map(|e| Atom {
            face: Face {
                fg: Color::Rgb(RgbColor { r: e.r, g: e.g, b: e.b }),
                bg: Color::Rgb(RgbColor { r: e.r, g: e.g, b: e.b }),
                underline: Color::DefaultColor,
                attributes: 0,
            },
            contents: "\u{2588}".to_string(), // █
        })
        .collect();
    element_builder::create_styled_line(&atoms)
}

fn build_color_grid(entry: &ColorEntry, color_idx: usize) -> ElementHandle {
    let channels = [entry.r, entry.g, entry.b];

    let columns = vec![
        GridWidth::Fixed(2),
        GridWidth::Fixed(1),
        GridWidth::Fixed(2),
        GridWidth::Fixed(2),
        GridWidth::Fixed(2),
    ];

    let mut children = Vec::with_capacity(15);

    // Row 0: arrows up
    children.push(element_builder::create_empty());
    children.push(element_builder::create_empty());
    for ch in 0..3 {
        let id = encode_picker_id(color_idx, ch, false);
        let text = element_builder::create_text(" \u{25b2}", default_face()); // ▲
        children.push(element_builder::create_interactive(text, id));
    }

    // Row 1: swatch + hex display
    let swatch_atom = Atom {
        face: Face {
            fg: Color::Rgb(RgbColor { r: entry.r, g: entry.g, b: entry.b }),
            bg: Color::Rgb(RgbColor { r: entry.r, g: entry.g, b: entry.b }),
            underline: Color::DefaultColor,
            attributes: 0,
        },
        contents: "\u{2588} ".to_string(), // "█ "
    };
    children.push(element_builder::create_styled_line(&[swatch_atom]));
    children.push(element_builder::create_text("#", default_face()));
    for ch_val in channels {
        children.push(element_builder::create_text(&format!("{ch_val:02x}"), default_face()));
    }

    // Row 2: arrows down
    children.push(element_builder::create_empty());
    children.push(element_builder::create_empty());
    for ch in 0..3 {
        let id = encode_picker_id(color_idx, ch, true);
        let text = element_builder::create_text(" \u{25bc}", default_face()); // ▼
        children.push(element_builder::create_interactive(text, id));
    }

    element_builder::create_grid(&columns, &children, 0, 0)
}

// ---------------------------------------------------------------------------
// Plugin implementation
// ---------------------------------------------------------------------------

struct ColorPreviewPlugin;

#[plugin]
impl Guest for ColorPreviewPlugin {
    fn get_id() -> String {
        "color_preview".to_string()
    }

    fn on_init() -> Vec<Command> {
        vec![]
    }

    fn on_shutdown() -> Vec<Command> {
        vec![]
    }


    fn on_state_changed(dirty_flags: u16) -> Vec<Command> {
        if dirty_flags & dirty::BUFFER == 0 {
            return vec![];
        }

        STATE.with(|state| {
            let mut state = state.borrow_mut();
            let mut changed = false;

            let cursor_line = host_state::get_cursor_line();
            if state.active_line != cursor_line {
                state.active_line = cursor_line;
                changed = true;
            }

            let line_count = host_state::get_line_count();
            for i in 0..line_count {
                if !host_state::is_line_dirty(i) {
                    continue;
                }

                let text = match host_state::get_line_text(i) {
                    Some(t) => t,
                    None => continue,
                };

                let idx = i as usize;
                let colors = detect_colors(&text);

                if colors.is_empty() {
                    if state.color_lines.remove(&idx).is_some() {
                        changed = true;
                    }
                } else {
                    let cl = ColorLine { colors };
                    if state.color_lines.get(&idx) != Some(&cl) {
                        state.color_lines.insert(idx, cl);
                        changed = true;
                    }
                }
            }

            // Remove entries for deleted lines
            let lc = line_count as usize;
            state.color_lines.retain(|&k, _| k < lc);

            if changed {
                state.generation += 1;
            }

            vec![]
        })
    }

    fn handle_mouse(event: MouseEvent, id: InteractiveId) -> Option<Vec<Command>> {
        let (color_idx, channel, is_down) = decode_picker_id(id)?;

        // Consume all events on picker IDs
        let is_left_press = matches!(event.kind, MouseEventKind::Press(MouseButton::Left));
        if !is_left_press {
            return Some(vec![]);
        }

        STATE.with(|state| {
            let state = state.borrow();
            let line_idx = state.active_line as usize;
            let entry = state.color_lines.get(&line_idx)?.colors.get(color_idx)?;

            let step: i16 = if event.modifiers & (modifiers::SHIFT | modifiers::CTRL) != 0 {
                16
            } else {
                1
            };
            let delta = if is_down { -step } else { step };

            let mut channels = [entry.r, entry.g, entry.b];
            channels[channel] = (channels[channel] as i16 + delta).clamp(0, 255) as u8;

            // Safety check: verify old text at expected offset
            let buffer_text = host_state::get_line_text(line_idx as u32).unwrap_or_default();
            let old_text = &entry.original;
            if !buffer_text
                .get(entry.byte_offset..)
                .is_some_and(|s| s.starts_with(old_text.as_str()))
            {
                return Some(vec![]);
            }

            let new_text = format_color(channels, entry.format);

            let kak_line = line_idx + 1; // 0-based to 1-based
            let cmd = format!("exec -draft {kak_line}gxs{old_text}<ret>c{new_text}<esc>");
            let mut keys: Vec<String> = vec!["<esc>".into(), ":".into()];
            push_literal_keys(&mut keys, &cmd);
            keys.push("<ret>".into());

            Some(vec![Command::SendKeys(keys)])
        })
    }

    fn handle_key(_event: KeyEvent) -> Option<Vec<Command>> {
        None
    }

    fn observe_key(_event: KeyEvent) {}
    fn observe_mouse(_event: MouseEvent) {}

    fn state_hash() -> u64 {
        STATE.with(|state| {
            let state = state.borrow();
            // Simple hash combining generation and active_line
            let mut h = state.generation;
            h = h.wrapping_mul(6364136223846793005).wrapping_add(state.active_line as u64);
            h
        })
    }

    fn annotate_line(line: u32, _ctx: AnnotateContext) -> Option<LineAnnotation> {
        STATE.with(|state| {
            let state = state.borrow();
            let cl = state.color_lines.get(&(line as usize))?;
            let swatch = build_swatch(&cl.colors);
            Some(LineAnnotation {
                left_gutter: Some(swatch),
                right_gutter: None,
                background: None,
                priority: 0,
            })
        })
    }

    fn annotate_deps() -> u16 {
        dirty::BUFFER
    }

    fn contribute_overlay_v2(ctx: OverlayContext) -> Option<OverlayContribution> {
        STATE.with(|state| {
            let state = state.borrow();
            let line_idx = state.active_line as usize;
            let cl = state.color_lines.get(&line_idx)?;

            let entries: Vec<FlexEntry> = cl
                .colors
                .iter()
                .enumerate()
                .map(|(idx, entry)| {
                    let grid = build_color_grid(entry, idx);
                    FlexEntry { child: grid, flex: 0.0 }
                })
                .collect();

            let inner = element_builder::create_column_flex(&entries, 1);

            let padding = Edges { top: 0, right: 0, bottom: 0, left: 0 };
            let container = element_builder::create_container(
                inner,
                Some(BorderLineStyle::Rounded),
                false,
                padding,
            );

            let cursor_line = host_state::get_cursor_line();
            let cursor_col = host_state::get_cursor_col();

            // Build avoid list from menu_rect + existing_overlays
            let mut avoid = ctx.existing_overlays;
            if let Some(menu_rect) = ctx.menu_rect {
                avoid.push(menu_rect);
            }

            Some(OverlayContribution {
                element: container,
                anchor: OverlayAnchor::AnchorPoint(AnchorPointConfig {
                    coord: Coord {
                        line: cursor_line,
                        column: cursor_col,
                    },
                    prefer_above: false,
                    avoid,
                }),
                z_index: 0,
            })
        })
    }

}

export!(ColorPreviewPlugin);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_hex_rrggbb() {
        let colors = detect_colors("color: #3498db;");
        assert_eq!(colors.len(), 1);
        assert_eq!((colors[0].r, colors[0].g, colors[0].b), (0x34, 0x98, 0xdb));
        assert_eq!(colors[0].format, ColorFormat::HashRrggbb);
    }

    #[test]
    fn detect_hex_rgb_shorthand() {
        let colors = detect_colors("#f00");
        assert_eq!(colors.len(), 1);
        assert_eq!((colors[0].r, colors[0].g, colors[0].b), (0xff, 0x00, 0x00));
        assert_eq!(colors[0].format, ColorFormat::HashRgb);
    }

    #[test]
    fn detect_kakoune_rgb() {
        let colors = detect_colors("face global default rgb:a0b0c0");
        assert_eq!(colors.len(), 1);
        assert_eq!((colors[0].r, colors[0].g, colors[0].b), (0xa0, 0xb0, 0xc0));
        assert_eq!(colors[0].format, ColorFormat::RgbColon);
    }

    #[test]
    fn detect_multiple_colors() {
        let colors = detect_colors("#ff0000 #00ff00 rgb:0000ff");
        assert_eq!(colors.len(), 3);
    }

    #[test]
    fn no_false_positive_on_non_hex() {
        assert!(detect_colors("#zoo not a color").is_empty());
    }

    #[test]
    fn no_false_positive_on_too_long_hex() {
        assert!(detect_colors("#1234567").is_empty());
    }

    #[test]
    fn no_false_positive_hex_preceded_by_hex_digit() {
        assert!(detect_colors("a#fff").is_empty());
    }

    #[test]
    fn encode_decode_picker_id_roundtrip() {
        for color_idx in 0..3 {
            for channel in 0..3 {
                for is_down in [false, true] {
                    let id = encode_picker_id(color_idx, channel, is_down);
                    let (ci, ch, down) = decode_picker_id(id).unwrap();
                    assert_eq!(ci, color_idx);
                    assert_eq!(ch, channel);
                    assert_eq!(down, is_down);
                }
            }
        }
    }

    #[test]
    fn decode_picker_id_below_base_returns_none() {
        assert!(decode_picker_id(999).is_none());
    }

    #[test]
    fn format_color_hash() {
        assert_eq!(format_color([0x11, 0x00, 0x00], ColorFormat::HashRrggbb), "#110000");
        assert_eq!(format_color([0x11, 0x00, 0x00], ColorFormat::HashRgb), "#110000");
    }

    #[test]
    fn format_color_rgb_colon() {
        assert_eq!(format_color([0x11, 0x00, 0x00], ColorFormat::RgbColon), "rgb:110000");
    }
}
