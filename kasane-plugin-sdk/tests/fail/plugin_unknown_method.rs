kasane_plugin_sdk::generate!();

use kasane_plugin_sdk::plugin;

struct Bad;

#[plugin]
impl Guest for Bad {
    fn get_id() -> String {
        "bad".into()
    }

    fn on_state_changd_effects(dirty_flags: u16) -> RuntimeEffects {
        let _ = dirty_flags;
        RuntimeEffects::default()
    }
}

export!(Bad);

fn main() {}
