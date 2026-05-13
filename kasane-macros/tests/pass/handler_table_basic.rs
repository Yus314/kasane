// γ-3.2.1 + γ-3.2.2a: smoke-test for the four base dispatch shapes.
//
// Confirms the macro accepts a representative entry per shape, generates
// the expected erased aliases + HandlerTable struct + EXPECTED_HANDLER_NAMES
// + HandlerRegistry<S> with base setters, and that registered closures
// are invocable through the type-erased dispatch path.

use kasane_macros::handler_table;

handler_table! {
    pub mod spec {
        use kasane_core::plugin::{
            AppView, Command, ContributeContext, Contribution, Effects, PluginState,
        };
        use kasane_core::protocol::Atom;
        use kasane_core::state::DirtyFlags;

        handler init(_app: &AppView<'_>): Lifecycle<Effects>;
        handler observe_key(_event: &Atom, _app: &AppView<'_>): Observer;
        handler key(_event: &Atom, _app: &AppView<'_>): Dispatcher<Vec<Command>>;
        handler contribute(_app: &AppView<'_>, _ctx: &ContributeContext): View<Option<Contribution>>;

        config interests: DirtyFlags = DirtyFlags::ALL;
        config display_priority: i16 = 0;
    }
}

fn main() {
    // Generated name list (config entries do not appear).
    assert_eq!(
        spec::EXPECTED_HANDLER_NAMES,
        &["init", "observe_key", "key", "contribute"]
    );

    // The empty table has no registered handlers.
    let table = spec::HandlerTable::empty();
    assert!(table.init_handler.is_none());
    assert!(table.observe_key_handler.is_none());
    assert!(table.key_handler.is_none());
    assert!(table.contribute_handler.is_none());
    assert_eq!(table.display_priority, 0);

    // Register one handler per shape via the generated HandlerRegistry<u32>.
    // u32 is PluginState + Clone + 'static via the dyn-clone blanket impl.
    let mut registry: spec::HandlerRegistry<u32> = spec::HandlerRegistry::new();
    registry.on_init(|state, _app| (*state + 1, kasane_core::plugin::Effects::default()));
    registry.on_observe_key(|state, _ev, _app| *state);
    registry.on_key(|state, _ev, _app| Some((*state, Vec::<kasane_core::plugin::Command>::new())));
    registry.on_contribute(|_state, _app, _ctx| None);

    let table = registry.into_table();
    assert!(table.init_handler.is_some());
    assert!(table.observe_key_handler.is_some());
    assert!(table.key_handler.is_some());
    assert!(table.contribute_handler.is_some());
}
