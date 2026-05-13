// γ-3.2.2d-3: View `recovery` modifier (display family).
//
// Confirms:
// - `recovery` modifier emits the trio of setters: `on_<name>` (Unwitnessed),
//   `on_<name>_safe` (NonDestructive, accepts Vec<SafeDisplayDirective>),
//   `on_<name>_witnessed` (Witnessed, takes a RecoveryWitness arg).
// - The HandlerTable gains a `<name>_recovery: DisplayRecoveryStatus`
//   field that the setters flip on registration.
// - The local `DisplayRecoveryStatus` enum is generated when any entry
//   uses the recovery modifier, mirroring the manual `pub(crate)` enum.

use kasane_macros::handler_table;

handler_table! {
    pub mod spec {
        use kasane_core::plugin::{
            AppView, DisplayDirective, PluginState, RecoveryWitness, SafeDisplayDirective,
        };

        handler display(_app: &AppView<'_>): View<Vec<DisplayDirective>>(recovery);
    }
}

fn main() {
    use kasane_core::plugin::{RecoveryMechanism, RecoveryWitness, SafeDisplayDirective};

    // 1. NotRegistered — empty table.
    let table = spec::HandlerTable::empty();
    assert!(matches!(
        table.display_recovery,
        spec::DisplayRecoveryStatus::NotRegistered
    ));

    // 2. Unwitnessed — base setter (handler returns full DisplayDirective vec).
    let mut registry: spec::HandlerRegistry<u32> = spec::HandlerRegistry::new();
    registry.on_display(|_state, _app| Vec::new());
    let table = registry.into_table();
    assert!(matches!(
        table.display_recovery,
        spec::DisplayRecoveryStatus::Unwitnessed
    ));

    // 3. NonDestructive — `_safe` setter (handler returns SafeDisplayDirective vec).
    let mut registry: spec::HandlerRegistry<u32> = spec::HandlerRegistry::new();
    registry.on_display_safe(|_state, _app| Vec::<SafeDisplayDirective>::new());
    let table = registry.into_table();
    assert!(matches!(
        table.display_recovery,
        spec::DisplayRecoveryStatus::NonDestructive
    ));

    // 4. Witnessed — `_witnessed` setter takes a RecoveryWitness.
    let witness = RecoveryWitness {
        mechanism: RecoveryMechanism::Declared {
            description: "test",
        },
    };
    let mut registry: spec::HandlerRegistry<u32> = spec::HandlerRegistry::new();
    registry.on_display_witnessed(witness, |_state, _app| Vec::new());
    let table = registry.into_table();
    assert!(matches!(
        table.display_recovery,
        spec::DisplayRecoveryStatus::Witnessed(_)
    ));
}
