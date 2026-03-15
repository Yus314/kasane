kasane_plugin_sdk::generate!("../../../kasane-plugin-sdk/wit");

use std::cell::Cell;

use exports::kasane::plugin::plugin_api::Guest;
use kasane::plugin::element_builder;
use kasane::plugin::host_state;
use kasane::plugin::types::*;
use kasane_plugin_sdk::{dirty, slot};

thread_local! {
    static CURSOR_COUNT: Cell<u32> = const { Cell::new(0) };
}

struct SelBadgePlugin;

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

    fn contribute(s: u8) -> Option<ElementHandle> {
        kasane_plugin_sdk::route_slots!(s, {
            slot::STATUS_RIGHT => {
                let count = CURSOR_COUNT.get();
                if count > 1 {
                    let text = format!(" {} sel ", count);
                    let face = Face {
                        fg: Color::DefaultColor,
                        bg: Color::DefaultColor,
                        underline: Color::DefaultColor,
                        attributes: 0,
                    };
                    Some(element_builder::create_text(&text, face))
                } else {
                    None
                }
            },
        })
    }

    fn state_hash() -> u64 {
        CURSOR_COUNT.get() as u64
    }

    fn slot_deps(s: u8) -> u16 {
        kasane_plugin_sdk::route_slot_deps!(s, {
            slot::STATUS_RIGHT => dirty::BUFFER,
        })
    }

    fn contribute_to(region: SlotId, _ctx: ContributeContext) -> Option<Contribution> {
        kasane_plugin_sdk::route_slot_ids!(region, {
            STATUS_RIGHT => {
                let count = CURSOR_COUNT.get();
                if count > 1 {
                    let text = format!(" {} sel ", count);
                    let face = Face {
                        fg: Color::DefaultColor,
                        bg: Color::DefaultColor,
                        underline: Color::DefaultColor,
                        attributes: 0,
                    };
                    let el = element_builder::create_text(&text, face);
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

    kasane_plugin_sdk::default_init!();
    kasane_plugin_sdk::default_shutdown!();
    kasane_plugin_sdk::default_surfaces!();
    kasane_plugin_sdk::default_render_surface!();
    kasane_plugin_sdk::default_handle_surface_event!();
    kasane_plugin_sdk::default_handle_surface_state_changed!();
    kasane_plugin_sdk::default_line!();
    kasane_plugin_sdk::default_input!();
    kasane_plugin_sdk::default_overlay!();
    kasane_plugin_sdk::default_menu_transform!();
    kasane_plugin_sdk::default_replace!();
    kasane_plugin_sdk::default_decorate!();
    kasane_plugin_sdk::default_decorator_priority!();
    kasane_plugin_sdk::default_update!();
    kasane_plugin_sdk::default_cursor_style!();
    kasane_plugin_sdk::default_named_slot!();
    // contribute_to already implemented above with route_slots! fallback
    kasane_plugin_sdk::default_transform!();
    kasane_plugin_sdk::default_transform_priority!();
    kasane_plugin_sdk::default_annotate!();
    kasane_plugin_sdk::default_overlay_v2!();
    kasane_plugin_sdk::default_transform_deps!();
    kasane_plugin_sdk::default_annotate_deps!();
    kasane_plugin_sdk::default_capabilities!();
    kasane_plugin_sdk::default_io_event!();
}

export!(SelBadgePlugin);
