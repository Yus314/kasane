// γ-3.2.2b: Lifecycle tier1 / tier2 / transparent setter variants.
//
// Confirms:
// - `tier1` modifier emits `on_<name>_tier1` accepting closures whose effect
//   type lifts via `Into<KakouneSideEffects>` (rejects raw `Effects` at
//   compile time — see the corresponding fail test).
// - `tier2` modifier emits `on_<name>_tier2` with `Into<ProcessCapableEffects>`.
// - `transparent` modifier on Lifecycle changes the base setter to use a
//   `Transparency`-aware bound and flips the corresponding flag in the
//   generated `TransparencyFlags`.

use kasane_macros::handler_table;

handler_table! {
    pub mod spec {
        use kasane_core::plugin::{
            AppView, Effects, KakouneSideEffects, PluginState, ProcessCapableEffects, Transparency,
        };

        // Lifecycle entry with `tier1` only — tier-narrowed setter.
        handler init(_app: &AppView<'_>): Lifecycle<Effects>(tier1);

        // Lifecycle entry with `tier2` only.
        handler io_event(_app: &AppView<'_>): Lifecycle<Effects>(tier2);

        // Lifecycle entry with `transparent` only — base setter gains
        // Transparency bound and TransparencyFlags tracking.
        handler command_error(_app: &AppView<'_>): Lifecycle<Effects>(transparent);
    }
}

fn main() {
    use kasane_core::plugin::{KakouneSideEffects, KakouneTransparentEffects, ProcessCapableEffects};

    let mut registry: spec::HandlerRegistry<u32> = spec::HandlerRegistry::new();

    // tier1 setter accepts closures that return KakouneSideEffects.
    registry.on_init_tier1(|state, _app| (*state, KakouneSideEffects::none()));

    // tier2 setter accepts closures that return ProcessCapableEffects.
    registry.on_io_event_tier2(|state, _app| (*state, ProcessCapableEffects::none()));

    // Transparent base setter: register a transparent-typed handler.
    // Closure returns KakouneTransparentEffects (IS_TRANSPARENT = true),
    // so the flag flips on registration.
    registry.on_command_error(|state, _app| (*state, KakouneTransparentEffects::default()));

    let table = registry.into_table();
    assert!(table.init_handler.is_some());
    assert!(table.io_event_handler.is_some());
    assert!(table.command_error_handler.is_some());

    // The transparent handler tracked its transparency.
    assert!(table.transparency.command_error);

    // Lifecycle predicate: command_error registered + transparent → ok.
    // (init / io_event are not transparent-tracked entries, so they don't
    // factor into the predicate.)
    assert!(table.transparency.is_all_lifecycle_transparent(&table));
}
