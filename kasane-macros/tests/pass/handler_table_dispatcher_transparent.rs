// γ-3.2.2d-4: Dispatcher `transparent` modifier — generic-Vec rewrite.
//
// Confirms:
// - `transparent` on Dispatcher rewrites the closure's command type from
//   `Vec<Command>` (in the spec) to `Vec<C>` where `C: Into<Command> +
//   Transparency`. The wrapper does `cmds.into_iter().map(Into::into).collect()`.
// - When the registered closure's command type satisfies `Transparency`
//   with `IS_TRANSPARENT = true` (`KakouneTransparentCommand`), the
//   corresponding `TransparencyFlags` bit flips on registration.
// - The generated `is_all_input_transparent(&table)` predicate returns
//   true iff every transparent-marked Dispatcher entry is either
//   unregistered or has been registered with a transparent command type.

use kasane_macros::handler_table;

handler_table! {
    pub mod spec {
        use kasane_core::plugin::{AppView, Command, PluginState, Transparency};
        use kasane_core::protocol::Atom;

        // Dispatcher with `transparent`: the wrapper accepts `Vec<C>`
        // where `C: Into<Command> + Transparency`.
        handler key(_event: &Atom, _app: &AppView<'_>):
            Dispatcher<Vec<Command>>(transparent);
    }
}

fn main() {
    use kasane_core::plugin::KakouneTransparentCommand;

    // 1. Register a transparent-typed command vec — flag should flip.
    let mut registry: spec::HandlerRegistry<u32> = spec::HandlerRegistry::new();
    registry.on_key(|state, _ev, _app| {
        Some((*state, Vec::<KakouneTransparentCommand>::new()))
    });
    let table = registry.into_table();
    assert!(table.transparency.key);
    assert!(table.transparency.is_all_input_transparent(&table));
    // No Lifecycle transparent entries in this spec, so lifecycle predicate
    // trivially holds → fully_transparent is also true.
    assert!(table.transparency.is_fully_transparent(&table));

    // 2. Unregistered: predicate trivially holds via the
    //    `table.<entry>_handler.is_none()` short-circuit.
    let table = spec::HandlerTable::empty();
    assert!(!table.transparency.key);
    assert!(table.transparency.is_all_input_transparent(&table));
}
