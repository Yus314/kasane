kasane_plugin_sdk::generate!();

use std::cell::Cell;

use kasane_plugin_sdk::{dirty, plugin};

thread_local! {
    static CURSOR_COUNT: Cell<u32> = const { Cell::new(0) };
}

struct SelBadgePlugin;

#[plugin]
impl Guest for SelBadgePlugin {
    fn get_id() -> String {
        "sel_badge".to_string()
    }

    fn on_state_changed(dirty_flags: u16) -> Vec<Command> {
        if dirty_flags & dirty::BUFFER != 0 {
            CURSOR_COUNT.set(host_state::get_cursor_count());
        }
        vec![]
    }

    fn state_hash() -> u64 {
        CURSOR_COUNT.get() as u64
    }

    fn contribute_to(region: SlotId, _ctx: ContributeContext) -> Option<Contribution> {
        kasane_plugin_sdk::route_slot_ids!(region, {
            STATUS_RIGHT => {
                let count = CURSOR_COUNT.get();
                if count > 1 {
                    let text = format!(" {} sel ", count);
                    let el = element_builder::create_text(&text, default_face());
                    Some(Contribution {
                        element: el,
                        priority: 0,
                        size_hint: ContribSizeHint::Auto,
                    })
                } else {
                    None
                }
            },
        })
    }

    fn contribute_deps(region: SlotId) -> u16 {
        kasane_plugin_sdk::route_slot_id_deps!(region, {
            STATUS_RIGHT => dirty::BUFFER,
        })
    }
}

export!(SelBadgePlugin);
