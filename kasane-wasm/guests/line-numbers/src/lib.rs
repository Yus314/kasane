wit_bindgen::generate!({
    world: "kasane-plugin",
    path: "../../wit",
});

use exports::kasane::plugin::plugin_api::Guest;
use kasane::plugin::element_builder;
use kasane::plugin::host_state;
use kasane::plugin::types::{
    Color, ElementHandle, Face, LineBackground, NamedColor,
};

struct LineNumbersPlugin;

/// Slot index for BufferLeft.
const SLOT_BUFFER_LEFT: u8 = 0;

/// DirtyFlags::BUFFER
const DIRTY_BUFFER: u16 = 0x01;

impl Guest for LineNumbersPlugin {
    fn get_id() -> String {
        "wasm_line_numbers".to_string()
    }

    fn on_init() {}
    fn on_shutdown() {}
    fn on_state_changed(_dirty_flags: u16) {}

    fn contribute_line(_line: u32) -> Option<LineBackground> {
        None
    }

    fn contribute(slot: u8) -> Option<ElementHandle> {
        if slot != SLOT_BUFFER_LEFT {
            return None;
        }

        let total = host_state::get_line_count();
        if total == 0 {
            return None;
        }

        let width = digit_count(total).max(2) as usize;
        let mut children = Vec::with_capacity(total as usize);
        for i in 1..=total {
            let num = right_pad(i, width);
            let face = Face {
                fg: Color::Named(NamedColor::Cyan),
                bg: Color::DefaultColor,
                underline: Color::DefaultColor,
                attributes: 0,
            };
            let text_handle = element_builder::create_text(&num, face);
            children.push(text_handle);
        }

        Some(element_builder::create_column(&children))
    }

    fn state_hash() -> u64 {
        host_state::get_line_count() as u64
    }

    fn slot_deps(slot: u8) -> u16 {
        if slot == SLOT_BUFFER_LEFT {
            DIRTY_BUFFER
        } else {
            0
        }
    }
}

/// Right-aligned number with trailing space: "  1 ", " 42 "
fn right_pad(n: u32, width: usize) -> String {
    let s = n.to_string();
    let padding = if width > s.len() { width - s.len() } else { 0 };
    let mut out = String::with_capacity(width + 1);
    for _ in 0..padding {
        out.push(' ');
    }
    out.push_str(&s);
    out.push(' ');
    out
}

/// Count digits in a u32.
fn digit_count(mut n: u32) -> u32 {
    if n == 0 {
        return 1;
    }
    let mut count = 0;
    while n > 0 {
        n /= 10;
        count += 1;
    }
    count
}

export!(LineNumbersPlugin);
