//! Color preview — Phase 10 paint_inline_box worked example.
//!
//! Detects color literals (`#rrggbb`, `#rgb`, `rgb:rrggbb`) on each
//! buffer line and reserves a one-cell `InlineBox` slot immediately
//! after every literal. The host queries `paint_inline_box(box_id)`
//! when it actually needs to draw the slot, at which point the plugin
//! decodes `(line_idx, color_idx)` from the `box_id` and returns a
//! styled swatch atom.
//!
//! This is the bundled reference for the Phase 10 InlineBox extension
//! point (see `docs/roadmap.md` Phase 10 row and `docs/decisions.md`
//! ADR-031 §Phase 10 Step 2). The pattern that's worth copying:
//!
//! - `display()` only emits geometry (`inline_box(line, byte_offset,
//!   width_cells, height_lines, box_id, alignment)`). It does not
//!   build the swatch element itself.
//! - `box_id` is built from the plugin's own state shape via
//!   [`encode_inline_box_id`] / [`decode_inline_box_id`] so the host
//!   round-trip is reversible without a side-table lookup. The high
//!   byte tag (`0xCB`) lets `paint_inline_box` reject foreign ids.
//! - `paint_inline_box(box_id)` is the only place that touches paint
//!   state; the host caches layout independently of paint, and may
//!   call paint repeatedly per layout (e.g. on hover) without
//!   re-running `display()`.
//! - Returning `None` from `paint_inline_box` is fine — the host
//!   leaves the reserved slot blank rather than panicking. Stale
//!   `box_id`s after a buffer edit naturally hit this path.

use std::collections::HashMap;

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

// stride must cover max packed value: u8(255) + u8(255<<8) + bool(1<<16) = 131071
kasane_plugin_sdk::interactive_id! {
    enum PickerId(base = 2000, stride = 131072) {
        Picker { color_idx: u8, channel: u8, down: bool },
    }
}

/// Encode `(line_idx, color_idx)` into the `box_id` passed to a
/// `DisplayDirective::InlineBox`. The host hands the same id back
/// through `paint_inline_box`, so the encoding must round-trip.
///
/// `line_idx` occupies the low 32 bits; `color_idx` the next 8.
/// A `0xCB` tag in the high byte distinguishes color-preview's
/// box_ids from any other plugin's, in case a future host shares
/// box_id space across plugins.
fn encode_inline_box_id(line_idx: usize, color_idx: usize) -> u64 {
    let line = line_idx as u64 & 0xFFFF_FFFF;
    let color = (color_idx as u64 & 0xFF) << 32;
    let tag = 0xCB_u64 << 56;
    tag | color | line
}

fn decode_inline_box_id(id: u64) -> Option<(usize, usize)> {
    if (id >> 56) != 0xCB {
        return None;
    }
    let line = (id & 0xFFFF_FFFF) as usize;
    let color = ((id >> 32) & 0xFF) as usize;
    Some((line, color))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn format_color(channels: [u8; 3], format: ColorFormat) -> String {
    match format {
        ColorFormat::RgbColon => {
            format!(
                "rgb:{:02x}{:02x}{:02x}",
                channels[0], channels[1], channels[2]
            )
        }
        _ => format!("#{:02x}{:02x}{:02x}", channels[0], channels[1], channels[2]),
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
            style: style_with(rgb(e.r, e.g, e.b), rgb(e.r, e.g, e.b)),
            contents: "\u{2588}".to_string(), // █
        })
        .collect();
    styled_line(&atoms)
}

fn build_color_grid(entry: &ColorEntry, color_idx: u8) -> ElementHandle {
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
    for ch in 0..3u8 {
        let id = PickerId::Picker { color_idx, channel: ch, down: false }.encode();
        children.push(interactive(text(" \u{25b2}", default_style()), id)); // ▲
    }

    // Row 1: swatch + hex display
    let swatch_atom = Atom {
        style: style_with(
            rgb(entry.r, entry.g, entry.b),
            rgb(entry.r, entry.g, entry.b),
        ),
        contents: "\u{2588} ".to_string(), // "█ "
    };
    children.push(styled_line(&[swatch_atom]));
    children.push(text("#", default_style()));
    for ch_val in channels {
        children.push(text(&format!("{ch_val:02x}"), default_style()));
    }

    // Row 2: arrows down
    children.push(element_builder::create_empty());
    children.push(element_builder::create_empty());
    for ch in 0..3u8 {
        let id = PickerId::Picker { color_idx, channel: ch, down: true }.encode();
        children.push(interactive(text(" \u{25bc}", default_style()), id)); // ▼
    }

    element_builder::create_grid(&columns, &children, 0, 0)
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

kasane_plugin_sdk::define_plugin! {
    manifest: "kasane-plugin.toml",

    state {
        color_lines: HashMap<usize, ColorLine> = HashMap::new(),
        #[bind(host_state::get_cursor_line(), on: dirty::BUFFER)]
        active_line: i32 = 0,
    },

    on_state_changed_effects(dirty) {
        if dirty & dirty::BUFFER != 0 {
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
                    state.color_lines.remove(&idx);
                } else {
                    let cl = ColorLine { colors };
                    if state.color_lines.get(&idx) != Some(&cl) {
                        state.color_lines.insert(idx, cl);
                    }
                }
            }

            let lc = line_count as usize;
            state.color_lines.retain(|&k, _| k < lc);
        }
        effects(vec![])
    },

    handle_mouse(event, id) {
        let PickerId::Picker { color_idx, channel, down: is_down } = PickerId::decode(id)?;

        // Consume all events on picker IDs
        let is_left_press = matches!(event.kind, MouseEventKind::Press(MouseButton::Left));
        if !is_left_press {
            return Some(vec![]);
        }

        let line_idx = state.active_line as usize;
        let entry = state.color_lines.get(&line_idx)?.colors.get(color_idx as usize)?;

        let step: i16 = if event.modifiers & (modifiers::SHIFT | modifiers::CTRL) != 0 {
            16
        } else {
            1
        };
        let delta = if is_down { -step } else { step };

        let mut channels = [entry.r, entry.g, entry.b];
        channels[channel as usize] = (channels[channel as usize] as i16 + delta).clamp(0, 255) as u8;

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
        let mut kak_keys: Vec<String> = vec!["<esc>".into(), ":".into()];
        keys::push_literal(&mut kak_keys, &cmd);
        kak_keys.push("<ret>".into());

        Some(vec![Command::SendKeys(kak_keys)])
    },

    display() {
        // Two presentations of the same data:
        //
        // - `gutter_left`: swatches at the start of each line, useful
        //   when the line has many colors and you want a summary.
        // - `inline_box`: a swatch reserved next to each color literal
        //   in the source text. The host calls `paint_inline_box(box_id)`
        //   below to obtain the actual paint content for each slot.
        //
        // The InlineBox path is the Phase 10 exemplar: the directive
        // declares only a slot's geometry, not its paint content. That
        // separation lets the host cache layout independently of the
        // (potentially expensive or stateful) paint, and lets one
        // plugin's slot be repainted on hover / selection without
        // re-running every other plugin's `display()` callback.
        let mut out: Vec<DisplayDirective> = Vec::new();
        for (&line_idx, cl) in state.color_lines.iter() {
            let swatch = build_swatch(&cl.colors);
            out.push(gutter_left(line_idx as u32, swatch, 0));
            for (color_idx, entry) in cl.colors.iter().enumerate() {
                // Place the box at the byte offset right after the
                // color literal so the swatch sits immediately to the
                // right of `#abcdef` / `rgb:abcdef`.
                let after = (entry.byte_offset + entry.original.len()) as u32;
                out.push(inline_box(
                    line_idx as u32,
                    after,
                    /* width_cells   */ 1.5,
                    /* height_lines  */ 1.0,
                    encode_inline_box_id(line_idx, color_idx),
                    InlineBoxAlignment::Center,
                ));
            }
        }
        out
    },

    paint_inline_box(box_id) {
        // Decode the slot identity, find the still-current color in
        // state, and paint it as a single solid block.
        //
        // Returning `None` is safe: the host treats it as "leave the
        // reserved slot blank" rather than panicking. That happens
        // naturally when the buffer changes between the host calling
        // `display()` and the renderer querying `paint_inline_box` —
        // e.g. the user just deleted the line that owned the slot.
        let (line_idx, color_idx) = decode_inline_box_id(box_id)?;
        let entry = state.color_lines.get(&line_idx)?.colors.get(color_idx)?;
        let swatch = Atom {
            style: style_with(rgb(entry.r, entry.g, entry.b), rgb(entry.r, entry.g, entry.b)),
            contents: "\u{2588}".to_string(),
        };
        Some(styled_line(&[swatch]))
    },

    overlay(ctx) {
        let line_idx = state.active_line as usize;
        let cl = state.color_lines.get(&line_idx)?;

        let entries: Vec<FlexEntry> = cl
            .colors
            .iter()
            .enumerate()
            .map(|(idx, entry)| {
                let grid = build_color_grid(entry, idx as u8);
                FlexEntry {
                    child: grid,
                    flex: 0.0,
                }
            })
            .collect();

        let inner = element_builder::create_column_flex(&entries, 1);

        let el = container(inner)
            .border(BorderLineStyle::Rounded)
            .build();

        let cursor_line = host_state::get_cursor_line();
        let cursor_col = host_state::get_cursor_col();

        // Build avoid list from menu_rect + existing_overlays
        let mut avoid = ctx.existing_overlays;
        if let Some(menu_rect) = ctx.menu_rect {
            avoid.push(menu_rect);
        }

        Some(OverlayContribution {
            element: el,
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
    },
}

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
        for color_idx in 0..3u8 {
            for channel in 0..3u8 {
                for down in [false, true] {
                    let id = PickerId::Picker { color_idx, channel, down }.encode();
                    let PickerId::Picker { color_idx: ci, channel: ch, down: d } = PickerId::decode(id).unwrap();
                    assert_eq!(ci, color_idx);
                    assert_eq!(ch, channel);
                    assert_eq!(d, down);
                }
            }
        }
    }

    #[test]
    fn decode_picker_id_below_base_returns_none() {
        assert!(PickerId::decode(999).is_none());
    }

    #[test]
    fn inline_box_id_roundtrips() {
        // Cover the full u32 line_idx range in steps that hit the
        // upper byte plus a handful of low / mid / high color_idx.
        for &line in &[0usize, 1, 23, 99, 32_768, 1_048_575, 0xFFFF_FFFF] {
            for color in 0..8usize {
                let id = encode_inline_box_id(line, color);
                let (l, c) = decode_inline_box_id(id).expect("round trip");
                assert_eq!(l, line);
                assert_eq!(c, color);
            }
        }
    }

    #[test]
    fn inline_box_id_rejects_other_tags() {
        // A `box_id` minted by an unrelated source must NOT decode as
        // ours. Without the tag check, color-preview could pick up a
        // box_id intended for another plugin and try to paint it.
        let foreign = 0x00_00_00_05_FF_FF_FF_FFu64; // tag = 0x00, not 0xCB
        assert!(decode_inline_box_id(foreign).is_none());
    }

    #[test]
    fn format_color_hash() {
        assert_eq!(
            format_color([0x11, 0x00, 0x00], ColorFormat::HashRrggbb),
            "#110000"
        );
        assert_eq!(
            format_color([0x11, 0x00, 0x00], ColorFormat::HashRgb),
            "#110000"
        );
    }

    #[test]
    fn format_color_rgb_colon() {
        assert_eq!(
            format_color([0x11, 0x00, 0x00], ColorFormat::RgbColon),
            "rgb:110000"
        );
    }
}
