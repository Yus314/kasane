kasane_plugin_sdk::generate!("../../../kasane-plugin-sdk/wit");

use std::cell::Cell;

use exports::kasane::plugin::plugin_api::Guest;
use kasane::plugin::host_state;
use kasane::plugin::types::*;
use kasane_plugin_sdk::dirty;

thread_local! {
    static ACTIVE_LINE: Cell<i32> = const { Cell::new(-1) };
}

struct CursorLinePlugin;

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

    fn contribute_line(line: u32) -> Option<LineDecoration> {
        let active = ACTIVE_LINE.get();
        if line as i32 == active {
            Some(LineDecoration {
                left_gutter: None,
                right_gutter: None,
                background: Some(Face {
                    fg: Color::DefaultColor,
                    bg: Color::Rgb(RgbColor {
                        r: 40,
                        g: 40,
                        b: 50,
                    }),
                    underline: Color::DefaultColor,
                    attributes: 0,
                }),
            })
        } else {
            None
        }
    }

    fn state_hash() -> u64 {
        ACTIVE_LINE.get() as u64
    }

    fn slot_deps(_slot: u8) -> u16 {
        0
    }

    fn annotate_line(line: u32, _ctx: AnnotateContext) -> Option<LineAnnotation> {
        let active = ACTIVE_LINE.get();
        if line as i32 == active {
            Some(LineAnnotation {
                left_gutter: None,
                right_gutter: None,
                background: Some(BackgroundLayer {
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
                    z_order: 0,
                    blend_opaque: true,
                }),
            })
        } else {
            None
        }
    }

    fn annotate_deps() -> u16 {
        dirty::BUFFER
    }

    kasane_plugin_sdk::default_init!();
    kasane_plugin_sdk::default_shutdown!();
    kasane_plugin_sdk::default_contribute!();
    kasane_plugin_sdk::default_input!();
    kasane_plugin_sdk::default_overlay!();
    kasane_plugin_sdk::default_menu_transform!();
    kasane_plugin_sdk::default_replace!();
    kasane_plugin_sdk::default_decorate!();
    kasane_plugin_sdk::default_decorator_priority!();
    kasane_plugin_sdk::default_update!();
    kasane_plugin_sdk::default_cursor_style!();
    kasane_plugin_sdk::default_named_slot!();
    kasane_plugin_sdk::default_contribute_to!();
    kasane_plugin_sdk::default_transform!();
    kasane_plugin_sdk::default_transform_priority!();
    kasane_plugin_sdk::default_overlay_v2!();
    kasane_plugin_sdk::default_contribute_deps!();
    kasane_plugin_sdk::default_transform_deps!();
}

export!(CursorLinePlugin);
