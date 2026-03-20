kasane_plugin_sdk::generate!();

use std::cell::Cell;

use kasane_plugin_sdk::{dirty, plugin};

thread_local! {
    static ACTIVE_LINE: Cell<i32> = const { Cell::new(-1) };
}

struct CursorLinePlugin;

#[plugin]
impl Guest for CursorLinePlugin {
    fn get_id() -> String {
        "cursor_line".to_string()
    }

    fn on_state_changed(dirty_flags: u16) -> Vec<Command> {
        if dirty_flags & dirty::BUFFER != 0 {
            let line = host_state::get_cursor_line();
            ACTIVE_LINE.set(line);
        }
        vec![]
    }

    fn state_hash() -> u64 {
        ACTIVE_LINE.get() as u64
    }

    fn annotate_line(line: u32, _ctx: AnnotateContext) -> Option<LineAnnotation> {
        let active = ACTIVE_LINE.get();
        (line as i32 == active).then(|| bg_annotation(face_bg(rgb(40, 40, 50))))
    }
}

export!(CursorLinePlugin);
