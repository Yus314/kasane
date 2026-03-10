use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use crate::element::{
    BorderConfig, BorderLineStyle, Edges, Element, FlexChild, GridColumn, InteractiveId, Overlay,
    OverlayAnchor, Style,
};
use crate::input::{Modifiers, MouseButton, MouseEvent, MouseEventKind};
use crate::plugin::{Command, LineDecoration, Plugin, PluginId, Slot};
use crate::protocol::{Atom, Color, Coord, Face, KasaneRequest};
use crate::state::{AppState, DirtyFlags};
use compact_str::CompactString;

// ---------------------------------------------------------------------------
// Color detection with position tracking
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColorFormat {
    HashRrggbb,
    HashRgb,
    RgbColon,
}

#[derive(Debug, Clone, PartialEq)]
struct ColorEntry {
    color: Color,
    byte_offset: usize,
    format: ColorFormat,
    /// Original text from the buffer (preserves case).
    original: String,
}

impl ColorEntry {
    #[cfg(test)]
    fn original_len(&self) -> usize {
        match self.format {
            ColorFormat::HashRrggbb => 7,
            ColorFormat::HashRgb => 4,
            ColorFormat::RgbColon => 10,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct ColorLine {
    colors: Vec<ColorEntry>,
}

#[derive(Default)]
struct State {
    color_lines: HashMap<usize, ColorLine>,
    active_line: i32,
    generation: u64,
}

impl Hash for State {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.generation.hash(state);
        self.active_line.hash(state);
    }
}

#[derive(Default)]
pub struct ColorPreviewPlugin {
    state: State,
}

impl ColorPreviewPlugin {
    pub fn new() -> Self {
        Self::default()
    }
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

/// Detect color codes in a string. Supports `#RRGGBB`, `#RGB`, and `rgb:RRGGBB`.
fn detect_colors(text: &str) -> Vec<ColorEntry> {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut colors = Vec::new();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'#' {
            // Require word boundary: start of string or non-hex-digit before `#`
            if i > 0 && is_hex(bytes[i - 1]) {
                i += 1;
                continue;
            }
            // Try #RRGGBB first
            if i + 7 <= len
                && let (Some(r), Some(g), Some(b)) = (
                    parse_hex2(bytes, i + 1),
                    parse_hex2(bytes, i + 3),
                    parse_hex2(bytes, i + 5),
                )
                && (i + 7 >= len || !is_hex(bytes[i + 7]))
            {
                colors.push(ColorEntry {
                    color: Color::Rgb { r, g, b },
                    byte_offset: i,
                    format: ColorFormat::HashRrggbb,
                    original: text[i..i + 7].to_string(),
                });
                i += 7;
                continue;
            }
            // Try #RGB
            if i + 4 <= len
                && let (Some(r), Some(g), Some(b)) = (
                    hex_digit(bytes[i + 1]),
                    hex_digit(bytes[i + 2]),
                    hex_digit(bytes[i + 3]),
                )
                && (i + 4 >= len || !is_hex(bytes[i + 4]))
            {
                colors.push(ColorEntry {
                    color: Color::Rgb {
                        r: r * 16 + r,
                        g: g * 16 + g,
                        b: b * 16 + b,
                    },
                    byte_offset: i,
                    format: ColorFormat::HashRgb,
                    original: text[i..i + 4].to_string(),
                });
                i += 4;
                continue;
            }
        } else if i + 10 <= len
            && bytes[i..i + 4] == *b"rgb:"
            && let (Some(r), Some(g), Some(b)) = (
                parse_hex2(bytes, i + 4),
                parse_hex2(bytes, i + 6),
                parse_hex2(bytes, i + 8),
            )
            && (i + 10 >= len || !is_hex(bytes[i + 10]))
        {
            colors.push(ColorEntry {
                color: Color::Rgb { r, g, b },
                byte_offset: i,
                format: ColorFormat::RgbColon,
                original: text[i..i + 10].to_string(),
            });
            i += 10;
            continue;
        }
        i += 1;
    }

    colors
}

fn build_swatch(colors: &[ColorEntry]) -> Element {
    let atoms: Vec<Atom> = colors
        .iter()
        .take(4)
        .map(|e| Atom {
            face: Face {
                fg: e.color,
                bg: e.color,
                ..Face::default()
            },
            contents: CompactString::const_new("█"),
        })
        .collect();
    Element::StyledLine(atoms)
}

// ---------------------------------------------------------------------------
// Interactive overlay: ID encoding
// ---------------------------------------------------------------------------

const COLOR_PICKER_BASE: u32 = 2000;

/// Encode a picker interactive ID.
/// Layout: BASE + color_idx * 6 + channel * 2 + direction
/// channel: 0=R, 1=G, 2=B; direction: 0=up, 1=down
fn encode_picker_id(color_idx: usize, channel: usize, is_down: bool) -> InteractiveId {
    InteractiveId(
        COLOR_PICKER_BASE + (color_idx * 6 + channel + if is_down { 3 } else { 0 }) as u32,
    )
}

fn decode_picker_id(id: InteractiveId) -> Option<(usize, usize, bool)> {
    if id.0 < COLOR_PICKER_BASE {
        return None;
    }
    let offset = (id.0 - COLOR_PICKER_BASE) as usize;
    let color_idx = offset / 6;
    let rem = offset % 6;
    let is_down = rem >= 3;
    let channel = if is_down { rem - 3 } else { rem };
    if channel >= 3 {
        return None;
    }
    Some((color_idx, channel, is_down))
}

/// Build a 5-column x 3-row Grid for a single color entry.
/// Columns: [swatch(2), prefix(1), RR(2), GG(2), BB(2)]
fn build_color_grid(entry: &ColorEntry, color_idx: usize) -> Element {
    let Color::Rgb { r, g, b } = entry.color else {
        return Element::Empty;
    };
    let channels = [r, g, b];

    let columns = vec![
        GridColumn::fixed(2),
        GridColumn::fixed(1),
        GridColumn::fixed(2),
        GridColumn::fixed(2),
        GridColumn::fixed(2),
    ];

    let arrow_face = Face::default();
    let mut children = Vec::with_capacity(15);

    // Row 0: arrows up
    children.push(Element::Empty);
    children.push(Element::Empty);
    for ch in 0..3 {
        let id = encode_picker_id(color_idx, ch, false);
        children.push(Element::Interactive {
            child: Box::new(Element::text(" ▲", arrow_face)),
            id,
        });
    }

    // Row 1: swatch + hex display
    let swatch_atom = Atom {
        face: Face {
            fg: entry.color,
            bg: entry.color,
            ..Face::default()
        },
        contents: CompactString::const_new("█ "),
    };
    children.push(Element::StyledLine(vec![swatch_atom]));
    children.push(Element::text("#", Face::default()));
    for ch_val in channels {
        children.push(Element::text(format!("{ch_val:02x}"), Face::default()));
    }

    // Row 2: arrows down
    children.push(Element::Empty);
    children.push(Element::Empty);
    for ch in 0..3 {
        let id = encode_picker_id(color_idx, ch, true);
        children.push(Element::Interactive {
            child: Box::new(Element::text(" ▼", arrow_face)),
            id,
        });
    }

    Element::Grid {
        columns,
        children,
        col_gap: 0,
        row_gap: 0,
        align: crate::element::Align::Start,
        cross_align: crate::element::Align::Start,
    }
}

/// Push each character of `text` as a Kakoune key, escaping specials.
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

// ---------------------------------------------------------------------------
// Plugin implementation
// ---------------------------------------------------------------------------

impl Plugin for ColorPreviewPlugin {
    fn id(&self) -> PluginId {
        PluginId("color_preview".into())
    }

    fn on_state_changed(&mut self, state: &AppState, dirty: DirtyFlags) -> Vec<Command> {
        if !dirty.intersects(DirtyFlags::BUFFER) {
            return vec![];
        }

        let mut changed = false;

        if self.state.active_line != state.cursor_pos.line {
            self.state.active_line = state.cursor_pos.line;
            changed = true;
        }

        let active_idx = self.state.active_line as usize;
        for (i, line) in state.lines.iter().enumerate() {
            // Skip unchanged lines when dirty tracking is available
            if i < state.lines_dirty.len() && !state.lines_dirty[i] {
                if i == active_idx {
                    tracing::debug!(line = i, "color_preview: active line NOT dirty, skipping");
                }
                continue;
            }
            if i == active_idx {
                let text: String = line.iter().map(|a| a.contents.as_str()).collect();
                tracing::debug!(
                    line = i,
                    text,
                    "color_preview: active line IS dirty, re-detecting"
                );
            }

            let text: String = line.iter().map(|a| a.contents.as_str()).collect();
            let colors = detect_colors(&text);

            if colors.is_empty() {
                if self.state.color_lines.remove(&i).is_some() {
                    changed = true;
                }
            } else {
                let cl = ColorLine { colors };
                if self.state.color_lines.get(&i) != Some(&cl) {
                    self.state.color_lines.insert(i, cl);
                    changed = true;
                }
            }
        }

        // Remove entries for deleted lines
        let line_count = state.lines.len();
        self.state.color_lines.retain(|&k, _| k < line_count);

        if changed {
            self.state.generation += 1;
        }

        vec![]
    }

    fn state_hash(&self) -> u64 {
        let mut hasher = std::hash::DefaultHasher::new();
        self.state.hash(&mut hasher);
        hasher.finish()
    }

    fn slot_deps(&self, _slot: Slot) -> DirtyFlags {
        DirtyFlags::empty()
    }

    fn contribute_overlay(&self, state: &AppState) -> Option<Overlay> {
        let line_idx = self.state.active_line as usize;
        let cl = self.state.color_lines.get(&line_idx)?;

        let rows: Vec<FlexChild> = cl
            .colors
            .iter()
            .enumerate()
            .map(|(idx, entry)| FlexChild::fixed(build_color_grid(entry, idx)))
            .collect();

        let inner = Element::Flex {
            direction: crate::element::Direction::Column,
            children: rows,
            gap: 1,
            align: crate::element::Align::Start,
            cross_align: crate::element::Align::Start,
        };

        let element = Element::Container {
            child: Box::new(inner),
            border: Some(BorderConfig::from(BorderLineStyle::Rounded)),
            shadow: false,
            padding: Edges::ZERO,
            style: Style::Direct(Face::default()),
            title: None,
        };

        Some(Overlay {
            element,
            anchor: OverlayAnchor::AnchorPoint {
                coord: Coord {
                    line: state.cursor_pos.line,
                    column: state.cursor_pos.column,
                },
                prefer_above: false,
                avoid: vec![],
            },
        })
    }

    fn handle_mouse(
        &mut self,
        event: &MouseEvent,
        id: InteractiveId,
        state: &AppState,
    ) -> Option<Vec<Command>> {
        // Consume all events on picker IDs to prevent release from leaking to Kakoune
        let (color_idx, channel, is_down) = decode_picker_id(id)?;
        tracing::debug!(color_idx, channel, is_down, kind = ?event.kind, "color_picker handle_mouse");

        // Only act on left press
        if !matches!(event.kind, MouseEventKind::Press(MouseButton::Left)) {
            return Some(vec![]);
        }

        let line_idx = self.state.active_line as usize;
        let entry = self
            .state
            .color_lines
            .get(&line_idx)?
            .colors
            .get(color_idx)?;

        let step: i16 = if event
            .modifiers
            .intersects(Modifiers::SHIFT | Modifiers::CTRL)
        {
            16
        } else {
            1
        };
        let delta = if is_down { -step } else { step };

        let Color::Rgb { r, g, b } = entry.color else {
            return None;
        };
        let mut channels = [r, g, b];
        channels[channel] = (channels[channel] as i16 + delta).clamp(0, 255) as u8;

        // Get the buffer line text for safety check and char offset calculation
        let buffer_text: String = state
            .lines
            .get(line_idx)
            .map(|atoms| atoms.iter().map(|a| a.contents.as_str()).collect())
            .unwrap_or_default();

        // Safety check: verify old text is at expected byte offset
        let old_text = &entry.original;
        if !buffer_text
            .get(entry.byte_offset..)
            .is_some_and(|s| s.starts_with(old_text.as_str()))
        {
            tracing::warn!(
                old_text,
                byte_offset = entry.byte_offset,
                buffer_text,
                "color_picker: stale offset, skipping"
            );
            return Some(vec![]);
        }

        let new_text = match entry.format {
            ColorFormat::RgbColon => {
                format!(
                    "rgb:{:02x}{:02x}{:02x}",
                    channels[0], channels[1], channels[2]
                )
            }
            _ => format!("#{:02x}{:02x}{:02x}", channels[0], channels[1], channels[2]),
        };

        // Use exec -draft with search to avoid display-vs-buffer byte offset issues.
        // The draw text may contain leading padding bytes not in the actual buffer,
        // so absolute byte offsets are unreliable. Search-based replacement is robust.
        let kak_line = line_idx + 1; // 0-based → 1-based
        let cmd = format!("exec -draft {kak_line}gxs{old_text}<ret>c{new_text}<esc>");
        let mut keys: Vec<String> = vec!["<esc>".into(), ":".into()];
        push_literal_keys(&mut keys, &cmd);
        keys.push("<ret>".into());

        tracing::debug!(old_text, new_text, kak_line, "color_picker sending keys");

        Some(vec![Command::SendToKakoune(KasaneRequest::Keys(keys))])
    }

    fn contribute_line(&self, line: usize, _state: &AppState) -> Option<LineDecoration> {
        self.state.color_lines.get(&line).map(|cl| LineDecoration {
            left_gutter: Some(build_swatch(&cl.colors)),
            right_gutter: None,
            background: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Atom;

    fn make_state_with_lines(lines: &[&str]) -> AppState {
        let mut state = AppState::default();
        state.lines = lines
            .iter()
            .map(|s| {
                vec![Atom {
                    face: Face::default(),
                    contents: CompactString::new(s),
                }]
            })
            .collect();
        state.lines_dirty = vec![true; lines.len()];
        state
    }

    #[test]
    fn detect_hex_rrggbb() {
        let colors = detect_colors("color: #3498db;");
        assert_eq!(colors.len(), 1);
        assert_eq!(
            colors[0].color,
            Color::Rgb {
                r: 0x34,
                g: 0x98,
                b: 0xdb
            }
        );
        assert_eq!(colors[0].format, ColorFormat::HashRrggbb);
    }

    #[test]
    fn detect_hex_rgb_shorthand() {
        let colors = detect_colors("#f00");
        assert_eq!(colors.len(), 1);
        assert_eq!(
            colors[0].color,
            Color::Rgb {
                r: 0xff,
                g: 0x00,
                b: 0x00
            }
        );
        assert_eq!(colors[0].format, ColorFormat::HashRgb);
    }

    #[test]
    fn detect_kakoune_rgb() {
        let colors = detect_colors("face global default rgb:a0b0c0");
        assert_eq!(colors.len(), 1);
        assert_eq!(
            colors[0].color,
            Color::Rgb {
                r: 0xa0,
                g: 0xb0,
                b: 0xc0
            }
        );
        assert_eq!(colors[0].format, ColorFormat::RgbColon);
    }

    #[test]
    fn detect_multiple_colors() {
        let colors = detect_colors("#ff0000 #00ff00 rgb:0000ff");
        assert_eq!(colors.len(), 3);
        assert_eq!(
            colors[0].color,
            Color::Rgb {
                r: 0xff,
                g: 0x00,
                b: 0x00
            }
        );
        assert_eq!(
            colors[1].color,
            Color::Rgb {
                r: 0x00,
                g: 0xff,
                b: 0x00
            }
        );
        assert_eq!(
            colors[2].color,
            Color::Rgb {
                r: 0x00,
                g: 0x00,
                b: 0xff
            }
        );
    }

    #[test]
    fn no_false_positive_on_non_hex() {
        let colors = detect_colors("#zoo not a color");
        assert!(colors.is_empty());
    }

    #[test]
    fn no_false_positive_on_too_long_hex() {
        let colors = detect_colors("#1234567");
        assert!(colors.is_empty());
    }

    #[test]
    fn no_false_positive_hex_preceded_by_hex_digit() {
        let colors = detect_colors("a#fff");
        assert!(colors.is_empty());
    }

    #[test]
    fn contribute_line_with_color() {
        let mut plugin = ColorPreviewPlugin::new();
        let state = make_state_with_lines(&["#ff0000"]);
        plugin.on_state_changed(&state, DirtyFlags::BUFFER);

        let dec = plugin.contribute_line(0, &state);
        assert!(dec.is_some());
        let dec = dec.unwrap();
        assert!(dec.left_gutter.is_some());
        assert!(dec.background.is_none());
    }

    #[test]
    fn contribute_line_without_color() {
        let mut plugin = ColorPreviewPlugin::new();
        let state = make_state_with_lines(&["no colors here"]);
        plugin.on_state_changed(&state, DirtyFlags::BUFFER);

        assert!(plugin.contribute_line(0, &state).is_none());
    }

    #[test]
    fn multiple_colors_swatch() {
        let mut plugin = ColorPreviewPlugin::new();
        let state = make_state_with_lines(&["#ff0000 #00ff00 #0000ff"]);
        plugin.on_state_changed(&state, DirtyFlags::BUFFER);

        let dec = plugin.contribute_line(0, &state).unwrap();
        if let Some(Element::StyledLine(atoms)) = dec.left_gutter {
            assert_eq!(atoms.len(), 3);
            assert_eq!(
                atoms[0].face.fg,
                Color::Rgb {
                    r: 0xff,
                    g: 0,
                    b: 0
                }
            );
            assert_eq!(
                atoms[1].face.fg,
                Color::Rgb {
                    r: 0,
                    g: 0xff,
                    b: 0
                }
            );
            assert_eq!(
                atoms[2].face.fg,
                Color::Rgb {
                    r: 0,
                    g: 0,
                    b: 0xff
                }
            );
        } else {
            panic!("Expected StyledLine swatch");
        }
    }

    #[test]
    fn on_state_changed_updates_generation() {
        let mut plugin = ColorPreviewPlugin::new();
        let h1 = plugin.state_hash();

        let state = make_state_with_lines(&["#aabbcc"]);
        plugin.on_state_changed(&state, DirtyFlags::BUFFER);
        let h2 = plugin.state_hash();

        assert_ne!(h1, h2);
    }

    #[test]
    fn skips_update_on_non_buffer_dirty() {
        let mut plugin = ColorPreviewPlugin::new();
        let h1 = plugin.state_hash();

        let state = make_state_with_lines(&["#aabbcc"]);
        plugin.on_state_changed(&state, DirtyFlags::STATUS);
        let h2 = plugin.state_hash();

        assert_eq!(h1, h2);
    }

    #[test]
    fn overlay_shown_on_color_line() {
        let mut plugin = ColorPreviewPlugin::new();
        let mut state = make_state_with_lines(&["#3498db and #2c3e50"]);
        state.cursor_pos = crate::protocol::Coord { line: 0, column: 0 };
        plugin.on_state_changed(&state, DirtyFlags::BUFFER);

        let overlay = plugin.contribute_overlay(&state);
        assert!(overlay.is_some());
    }

    #[test]
    fn overlay_hidden_on_plain_line() {
        let mut plugin = ColorPreviewPlugin::new();
        let mut state = make_state_with_lines(&["no colors here", "#ff0000"]);
        state.cursor_pos = crate::protocol::Coord { line: 0, column: 0 };
        plugin.on_state_changed(&state, DirtyFlags::BUFFER);

        assert!(plugin.contribute_overlay(&state).is_none());
    }

    #[test]
    fn overlay_row_count_matches_colors() {
        let mut plugin = ColorPreviewPlugin::new();
        let mut state = make_state_with_lines(&["#3498db #2c3e50 #e74c3c"]);
        state.cursor_pos = crate::protocol::Coord { line: 0, column: 0 };
        plugin.on_state_changed(&state, DirtyFlags::BUFFER);

        let overlay = plugin.contribute_overlay(&state).unwrap();
        match overlay.element {
            Element::Container { child, .. } => match *child {
                Element::Flex { children, .. } => {
                    assert_eq!(children.len(), 3);
                }
                _ => panic!("Expected Flex inside Container"),
            },
            _ => panic!("Expected Container"),
        }
    }

    #[test]
    fn max_four_swatches() {
        let colors = detect_colors("#111111 #222222 #333333 #444444 #555555");
        assert_eq!(colors.len(), 5);

        let swatch = build_swatch(&colors);
        if let Element::StyledLine(atoms) = swatch {
            assert_eq!(atoms.len(), 4); // capped at 4
        } else {
            panic!("Expected StyledLine");
        }
    }

    // --- New tests for position tracking ---

    #[test]
    fn detect_colors_returns_byte_offset() {
        let colors = detect_colors("color: #3498db;");
        assert_eq!(colors[0].byte_offset, 7); // "color: " is 7 bytes

        let colors = detect_colors("#f00");
        assert_eq!(colors[0].byte_offset, 0);

        let colors = detect_colors("face rgb:a0b0c0");
        assert_eq!(colors[0].byte_offset, 5);
    }

    #[test]
    fn color_entry_original_len() {
        let entries = detect_colors("#aabbcc #f00 rgb:112233");
        assert_eq!(entries[0].original_len(), 7);
        assert_eq!(entries[1].original_len(), 4);
        assert_eq!(entries[2].original_len(), 10);
    }

    // --- Interactive overlay tests ---

    #[test]
    fn overlay_has_interactive_arrows() {
        let mut plugin = ColorPreviewPlugin::new();
        let mut state = make_state_with_lines(&["#ff0000"]);
        state.cursor_pos = Coord { line: 0, column: 0 };
        plugin.on_state_changed(&state, DirtyFlags::BUFFER);

        let overlay = plugin.contribute_overlay(&state).unwrap();
        // Container > Flex > [Grid]
        let grid = match overlay.element {
            Element::Container { child, .. } => match *child {
                Element::Flex { children, .. } => {
                    assert_eq!(children.len(), 1);
                    children.into_iter().next().unwrap().element
                }
                _ => panic!("Expected Flex"),
            },
            _ => panic!("Expected Container"),
        };

        // Grid should have 15 children (5 cols x 3 rows)
        match grid {
            Element::Grid {
                children, columns, ..
            } => {
                assert_eq!(columns.len(), 5);
                assert_eq!(children.len(), 15);

                // Check that row 0 has Interactive arrows at positions 2, 3, 4
                assert!(matches!(children[2], Element::Interactive { .. }));
                assert!(matches!(children[3], Element::Interactive { .. }));
                assert!(matches!(children[4], Element::Interactive { .. }));

                // Row 2 (down arrows) at positions 12, 13, 14
                assert!(matches!(children[12], Element::Interactive { .. }));
                assert!(matches!(children[13], Element::Interactive { .. }));
                assert!(matches!(children[14], Element::Interactive { .. }));
            }
            _ => panic!("Expected Grid"),
        }
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
        assert!(decode_picker_id(InteractiveId(999)).is_none());
    }

    // --- handle_mouse tests ---

    fn setup_plugin_with_color(hex: &str) -> (ColorPreviewPlugin, AppState) {
        let mut plugin = ColorPreviewPlugin::new();
        let mut state = make_state_with_lines(&[hex]);
        state.cursor_pos = Coord { line: 0, column: 0 };
        plugin.on_state_changed(&state, DirtyFlags::BUFFER);
        (plugin, state)
    }

    fn make_click(modifiers: Modifiers) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Press(MouseButton::Left),
            line: 0,
            column: 0,
            modifiers,
        }
    }

    #[test]
    fn handle_mouse_increments_r() {
        let (mut plugin, state) = setup_plugin_with_color("#100000");
        let id = encode_picker_id(0, 0, false); // R up
        let commands = plugin.handle_mouse(&make_click(Modifiers::empty()), id, &state);
        let commands = commands.unwrap();
        assert_eq!(commands.len(), 1);

        // Should produce Keys with exec -draft + search replacement
        if let Command::SendToKakoune(KasaneRequest::Keys(keys)) = &commands[0] {
            let joined: String = keys.join("");
            assert!(
                joined.contains("exec"),
                "Expected exec command in: {joined}"
            );
            assert!(joined.contains("#110000"), "Expected #110000 in: {joined}");
        } else {
            panic!("Expected SendToKakoune Keys");
        }
    }

    #[test]
    fn handle_mouse_decrements_g() {
        let (mut plugin, state) = setup_plugin_with_color("#001000");
        let id = encode_picker_id(0, 1, true); // G down
        let commands = plugin
            .handle_mouse(&make_click(Modifiers::empty()), id, &state)
            .unwrap();

        if let Command::SendToKakoune(KasaneRequest::Keys(keys)) = &commands[0] {
            let joined: String = keys.join("");
            assert!(joined.contains("#000f00"), "Expected #000f00 in: {joined}");
        } else {
            panic!("Expected SendToKakoune Keys");
        }
    }

    #[test]
    fn handle_mouse_shift_step_16() {
        let (mut plugin, state) = setup_plugin_with_color("#200000");
        let id = encode_picker_id(0, 0, false); // R up
        let commands = plugin
            .handle_mouse(&make_click(Modifiers::SHIFT), id, &state)
            .unwrap();

        if let Command::SendToKakoune(KasaneRequest::Keys(keys)) = &commands[0] {
            let joined: String = keys.join("");
            assert!(joined.contains("#300000"), "Expected #300000 in: {joined}");
        } else {
            panic!("Expected SendToKakoune Keys");
        }
    }

    #[test]
    fn handle_mouse_ctrl_step_16() {
        let (mut plugin, state) = setup_plugin_with_color("#200000");
        let id = encode_picker_id(0, 0, false); // R up
        let commands = plugin
            .handle_mouse(&make_click(Modifiers::CTRL), id, &state)
            .unwrap();

        if let Command::SendToKakoune(KasaneRequest::Keys(keys)) = &commands[0] {
            let joined: String = keys.join("");
            assert!(joined.contains("#300000"), "Expected #300000 in: {joined}");
        } else {
            panic!("Expected SendToKakoune Keys");
        }
    }

    #[test]
    fn handle_mouse_clamps_at_255() {
        let (mut plugin, state) = setup_plugin_with_color("#ff0000");
        let id = encode_picker_id(0, 0, false); // R up, already at max
        let commands = plugin
            .handle_mouse(&make_click(Modifiers::empty()), id, &state)
            .unwrap();

        if let Command::SendToKakoune(KasaneRequest::Keys(keys)) = &commands[0] {
            let joined: String = keys.join("");
            assert!(
                joined.contains("#ff0000"),
                "Expected #ff0000 (clamped) in: {joined}"
            );
        } else {
            panic!("Expected SendToKakoune Keys");
        }
    }

    #[test]
    fn handle_mouse_clamps_at_0() {
        let (mut plugin, state) = setup_plugin_with_color("#000000");
        let id = encode_picker_id(0, 0, true); // R down, already at min
        let commands = plugin
            .handle_mouse(&make_click(Modifiers::empty()), id, &state)
            .unwrap();

        if let Command::SendToKakoune(KasaneRequest::Keys(keys)) = &commands[0] {
            let joined: String = keys.join("");
            assert!(
                joined.contains("#000000"),
                "Expected #000000 (clamped) in: {joined}"
            );
        } else {
            panic!("Expected SendToKakoune Keys");
        }
    }

    #[test]
    fn handle_mouse_rgb_shorthand_expands() {
        let (mut plugin, state) = setup_plugin_with_color("#f00");
        let id = encode_picker_id(0, 0, true); // R down
        let commands = plugin
            .handle_mouse(&make_click(Modifiers::empty()), id, &state)
            .unwrap();

        if let Command::SendToKakoune(KasaneRequest::Keys(keys)) = &commands[0] {
            let joined: String = keys.join("");
            // Old text is #f00 (4 bytes), select should cover bytes 1-4
            assert!(joined.contains("exec"), "Expected exec in: {joined}");
            // New text should be expanded to #RRGGBB
            assert!(
                joined.contains("#fe0000"),
                "Expected #fe0000 (expanded) in: {joined}"
            );
        } else {
            panic!("Expected SendToKakoune Keys");
        }
    }

    #[test]
    fn handle_mouse_rgb_colon_format() {
        let (mut plugin, state) = setup_plugin_with_color("rgb:100000");
        let id = encode_picker_id(0, 0, false); // R up
        let commands = plugin
            .handle_mouse(&make_click(Modifiers::empty()), id, &state)
            .unwrap();

        if let Command::SendToKakoune(KasaneRequest::Keys(keys)) = &commands[0] {
            let joined: String = keys.join("");
            // rgb:RRGGBB is 10 bytes, found via search
            assert!(joined.contains("exec"), "Expected exec in: {joined}");
            // Write-back should use rgb: format
            assert!(
                joined.contains("rgb:110000"),
                "Expected rgb:110000 in: {joined}"
            );
        } else {
            panic!("Expected SendToKakoune Keys");
        }
    }

    #[test]
    fn handle_mouse_consumes_release_without_action() {
        let (mut plugin, state) = setup_plugin_with_color("#ff0000");
        let id = encode_picker_id(0, 0, false);
        let event = MouseEvent {
            kind: MouseEventKind::Release(MouseButton::Left),
            line: 0,
            column: 0,
            modifiers: Modifiers::empty(),
        };
        // Release on picker ID is consumed (prevents leak to Kakoune) but produces no commands
        let result = plugin.handle_mouse(&event, id, &state);
        assert!(result.is_some());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn handle_mouse_preserves_uppercase_replacement() {
        let (mut plugin, state) = setup_plugin_with_color("#E74C3C");
        let id = encode_picker_id(0, 0, false); // R up
        let commands = plugin
            .handle_mouse(&make_click(Modifiers::empty()), id, &state)
            .unwrap();

        if let Command::SendToKakoune(KasaneRequest::Keys(keys)) = &commands[0] {
            let joined: String = keys.join("");
            // Finds 7-byte color via search
            assert!(joined.contains("exec"), "Expected exec in: {joined}");
            // Replacement is lowercase
            assert!(
                joined.contains("#e84c3c"),
                "Expected #e84c3c in replacement: {joined}"
            );
        } else {
            panic!("Expected SendToKakoune Keys");
        }
    }

    #[test]
    fn handle_mouse_uses_search_for_replacement() {
        // Color at byte offset 7: "color: #100000"
        let (mut plugin, state) = setup_plugin_with_color("color: #100000");
        let id = encode_picker_id(0, 0, false); // R up
        let commands = plugin
            .handle_mouse(&make_click(Modifiers::empty()), id, &state)
            .unwrap();

        if let Command::SendToKakoune(KasaneRequest::Keys(keys)) = &commands[0] {
            let joined: String = keys.join("");
            // exec -draft searches for old text and replaces with new
            assert!(joined.contains("#100000"), "Expected old text in: {joined}");
            assert!(
                joined.contains("#110000"),
                "Expected replacement in: {joined}"
            );
        } else {
            panic!("Expected SendToKakoune Keys");
        }
    }

    #[test]
    fn handle_mouse_safety_check_stale_offset() {
        // Set up plugin with a color, then change the buffer text
        let mut plugin = ColorPreviewPlugin::new();
        let mut state = make_state_with_lines(&["#ff0000"]);
        state.cursor_pos = Coord { line: 0, column: 0 };
        plugin.on_state_changed(&state, DirtyFlags::BUFFER);

        // Now change the buffer text (simulating stale data)
        state.lines = vec![vec![Atom {
            face: Face::default(),
            contents: CompactString::new("no colors here"),
        }]];

        let id = encode_picker_id(0, 0, false);
        let result = plugin
            .handle_mouse(&make_click(Modifiers::empty()), id, &state)
            .unwrap();
        // Safety check should prevent sending keys
        assert!(result.is_empty(), "Expected empty commands on stale offset");
    }

    #[test]
    fn handle_mouse_ignores_unknown_id() {
        let (mut plugin, state) = setup_plugin_with_color("#ff0000");
        let id = InteractiveId(42); // not a picker ID
        assert!(
            plugin
                .handle_mouse(&make_click(Modifiers::empty()), id, &state)
                .is_none()
        );
    }
}
