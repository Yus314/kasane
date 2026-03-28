kasane_plugin_sdk::generate!();

use exports::kasane::plugin::plugin_api::Guest;

struct InstantiateTrapPlugin;

impl Guest for InstantiateTrapPlugin {
    fn get_id() -> String {
        panic!("instantiate trap fixture");
    }

    kasane_plugin_sdk::default_typed_lifecycle!();
    kasane_plugin_sdk::default_surfaces!();
    kasane_plugin_sdk::default_render_surface!();
    kasane_plugin_sdk::default_handle_surface_event!();
    kasane_plugin_sdk::default_handle_surface_state_changed!();
    kasane_plugin_sdk::default_workspace_changed!();
    kasane_plugin_sdk::default_line!();
    kasane_plugin_sdk::default_cache!();
    kasane_plugin_sdk::default_input!();
    kasane_plugin_sdk::default_overlay!();
    kasane_plugin_sdk::default_menu_transform!();
    kasane_plugin_sdk::default_replace!();
    kasane_plugin_sdk::default_decorate!();
    kasane_plugin_sdk::default_decorator_priority!();
    kasane_plugin_sdk::default_typed_runtime!();
    kasane_plugin_sdk::default_cursor_style!();
    kasane_plugin_sdk::default_named_slot!();
    kasane_plugin_sdk::default_transform!();
    kasane_plugin_sdk::default_transform_priority!();
    kasane_plugin_sdk::default_annotate!();
    kasane_plugin_sdk::default_display_directives!();
    kasane_plugin_sdk::default_overlay_v2!();
    kasane_plugin_sdk::default_contribute!();
    kasane_plugin_sdk::default_contribute_to!();
    kasane_plugin_sdk::default_decorate_cells!();
    kasane_plugin_sdk::default_capabilities!();
    kasane_plugin_sdk::default_authorities!();
    kasane_plugin_sdk::default_view_deps!();

    fn register_capabilities() -> u32 {
        0xFFFFFFFF
    }
}

export!(InstantiateTrapPlugin);
