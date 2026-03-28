kasane_plugin_sdk::generate!();

use exports::kasane::plugin::plugin_api::Guest;

struct InstantiateTrapPlugin;

#[kasane_plugin_sdk::plugin]
impl Guest for InstantiateTrapPlugin {
    fn get_id() -> String {
        panic!("instantiate trap fixture");
    }

    fn register_capabilities() -> u32 {
        0xFFFFFFFF
    }
}

export!(InstantiateTrapPlugin);
