kasane_plugin_sdk::generate!("../../../kasane-plugin-sdk/wit");

use exports::kasane::plugin::plugin_api::Guest;
use kasane::plugin::element_builder;
use kasane::plugin::host_state;
use kasane::plugin::types::*;
use kasane_plugin_sdk::{dirty, plugin, slot};

struct LineNumbersPlugin;

#[plugin]
impl Guest for LineNumbersPlugin {
    fn get_id() -> String {
        "wasm_line_numbers".to_string()
    }

    fn contribute(slot: u8) -> Option<ElementHandle> {
        kasane_plugin_sdk::route_slots!(slot, {
            slot::BUFFER_LEFT => {
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
            },
        })
    }

    fn state_hash() -> u64 {
        host_state::get_line_count() as u64
    }

    fn slot_deps(slot: u8) -> u16 {
        kasane_plugin_sdk::route_slot_deps!(slot, {
            slot::BUFFER_LEFT => dirty::BUFFER,
        })
    }

    fn contribute_to(region: SlotId, _ctx: ContributeContext) -> Option<Contribution> {
        kasane_plugin_sdk::route_slot_ids!(region, {
            BUFFER_LEFT => {
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

                let el = element_builder::create_column(&children);
                Some(Contribution {
                    element: el,
                    priority: 0,
                    size_hint: ContribSizeHint::Auto,
                })
            },
        })
    }

    fn contribute_deps(region: SlotId) -> u16 {
        kasane_plugin_sdk::route_slot_id_deps!(region, {
            BUFFER_LEFT => dirty::BUFFER,
        })
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
