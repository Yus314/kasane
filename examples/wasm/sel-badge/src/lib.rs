kasane_plugin_sdk::generate!();

use std::cell::Cell;

use kasane_plugin_sdk::{dirty, plugin};

thread_local! {
    static CURSOR_COUNT: Cell<u32> = const { Cell::new(0) };
}

struct SelBadgePlugin;

fn refresh_cursor_count(dirty_flags: u16) {
    if dirty_flags & dirty::BUFFER != 0 {
        CURSOR_COUNT.set(host_state::get_cursor_count());
    }
}

#[plugin]
impl Guest for SelBadgePlugin {
    fn get_id() -> String {
        "sel_badge".to_string()
    }

    fn on_state_changed_effects(dirty_flags: u16) -> RuntimeEffects {
        refresh_cursor_count(dirty_flags);
        RuntimeEffects::default()
    }

    fn state_hash() -> u64 {
        CURSOR_COUNT.get() as u64
    }

    kasane_plugin_sdk::slots! {
        STATUS_RIGHT => |_ctx| {
            let count = CURSOR_COUNT.get();
            (count > 1).then(|| {
                auto_contribution(text(&format!(" {} sel ", count), default_face()))
            })
        },
    }
}

export!(SelBadgePlugin);
