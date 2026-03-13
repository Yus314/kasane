wit_bindgen::generate!({
    world: "kasane-plugin",
    path: "../../wit",
});

use std::cell::Cell;

use exports::kasane::plugin::plugin_api::Guest;
use kasane::plugin::types::{Color, ElementHandle, Face, LineBackground, RgbColor};

thread_local! {
    static ACTIVE_LINE: Cell<i32> = const { Cell::new(-1) };
}

struct CursorLinePlugin;

impl Guest for CursorLinePlugin {
    fn get_id() -> String {
        "wasm_cursor_line".to_string()
    }

    fn on_init() {}
    fn on_shutdown() {}

    fn on_state_changed(dirty_flags: u16) {
        // BUFFER flag = 0x01
        if dirty_flags & 0x01 != 0 {
            let line = kasane::plugin::host_state::get_cursor_line();
            ACTIVE_LINE.set(line);
        }
    }

    fn contribute_line(line: u32) -> Option<LineBackground> {
        let active = ACTIVE_LINE.get();
        if line as i32 == active {
            Some(LineBackground {
                face: Face {
                    fg: Color::DefaultColor,
                    bg: Color::Rgb(RgbColor {
                        r: 40,
                        g: 40,
                        b: 50,
                    }),
                    underline: Color::DefaultColor,
                    attributes: 0,
                },
            })
        } else {
            None
        }
    }

    fn contribute(_slot: u8) -> Option<ElementHandle> {
        None
    }

    fn state_hash() -> u64 {
        ACTIVE_LINE.get() as u64
    }

    fn slot_deps(_slot: u8) -> u16 {
        0
    }
}

export!(CursorLinePlugin);
