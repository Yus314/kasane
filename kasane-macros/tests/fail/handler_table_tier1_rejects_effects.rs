// γ-3.2.2b: tier1's `E: Into<KakouneSideEffects>` bound rejects closures
// that return raw `Effects`. There is intentionally no
// `From<Effects> for KakouneSideEffects` impl — the tier1 type cannot
// carry process-spawn variants.

use kasane_macros::handler_table;

handler_table! {
    pub mod spec {
        use kasane_core::plugin::{AppView, Effects, KakouneSideEffects, PluginState};

        handler init(_app: &AppView<'_>): Lifecycle<Effects>(tier1);
    }
}

fn main() {
    use kasane_core::plugin::Effects;

    let mut registry: spec::HandlerRegistry<u32> = spec::HandlerRegistry::new();
    // Closure returns Effects directly — tier1's `Into<KakouneSideEffects>`
    // bound has no `From<Effects>` to satisfy, so this fails to compile.
    registry.on_init_tier1(|state, _app| (*state, Effects::default()));
}
