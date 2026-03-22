kasane_plugin_sdk::generate!();

use std::cell::Cell;

use kasane_plugin_sdk::{dirty, plugin};

thread_local! {
    static ACTIVE_LINE: Cell<i32> = const { Cell::new(-1) };
}

struct CursorLinePlugin;

fn refresh_active_line(dirty_flags: u16) {
    if dirty_flags & dirty::BUFFER != 0 {
        let line = host_state::get_cursor_line();
        ACTIVE_LINE.set(line);
    }
}

#[plugin]
impl Guest for CursorLinePlugin {
    fn get_id() -> String {
        "cursor_line".to_string()
    }

    fn on_state_changed_effects(dirty_flags: u16) -> RuntimeEffects {
        refresh_active_line(dirty_flags);
        RuntimeEffects::default()
    }

    fn state_hash() -> u64 {
        ACTIVE_LINE.get() as u64
    }

    fn annotate_line(line: u32, _ctx: AnnotateContext) -> Option<LineAnnotation> {
        let active = ACTIVE_LINE.get();
        if line as i32 != active {
            return None;
        }
        let bg = theme_face_or(
            "cursor.line.bg",
            if is_dark_background() {
                face_bg(rgb(40, 40, 50))
            } else {
                face_bg(rgb(220, 220, 235))
            },
        );
        Some(bg_annotation(bg))
    }
}

export!(CursorLinePlugin);
