use super::*;
use kasane_core::protocol::KasaneRequest;

fn load_tier1_state_plugin() -> crate::WasmPlugin {
    let loader = WasmPluginLoader::new().expect("failed to create loader");
    let bytes = crate::load_wasm_fixture("tier1-state.wasm").expect("failed to load fixture");
    loader
        .load(&bytes, &crate::WasiCapabilityConfig::default())
        .expect("failed to load plugin")
}

/// ADR-044 Phase B-3: the `define_plugin!` `on_state_changed_tier1_effects`
/// arm emits the tier-1 wire export. The host calls both the tier-1 and
/// the legacy export per tick and merges. This fixture only declares
/// the tier-1 form, so the legacy export stays at the SDK default
/// (empty effects) and the merged result equals just the tier-1 output.
#[test]
fn tier1_export_drives_state_change_effects() {
    let mut plugin = load_tier1_state_plugin();
    assert_eq!(plugin.id().0, "tier1_state");

    let state = AppState::default();
    let effects = plugin.on_state_changed_effects(&AppView::new(&state), DirtyFlags::BUFFER);

    assert!(effects.scroll_plans.is_empty());
    assert_eq!(
        effects.commands.len(),
        1,
        "tier-1 export should contribute exactly one command (legacy default is empty)",
    );

    match &effects.commands[0] {
        Command::SendToKakoune(KasaneRequest::Keys(keys)) => {
            // `kakoune_side_setup_effects!["echo tier1-fired"]` produces a
            // `KakouneSideCommand::EvalCommand` that the host lifts to
            // `Command::kakoune_command`, which encodes the body as the
            // key sequence `<esc>:echo<space>tier1<minus>fired<ret>`.
            assert_eq!(
                keys.first().map(String::as_str),
                Some("<esc>"),
                "EvalCommand should be wrapped in <esc>:cmd<ret>",
            );
            assert_eq!(keys.last().map(String::as_str), Some("<ret>"));
            assert!(
                keys.iter().any(|k| k == "<space>"),
                "expected a <space> token from the eval-command body",
            );
            assert!(
                keys.iter().any(|k| k == "<minus>"),
                "expected a <minus> token from `tier1-fired`",
            );
        }
        _ => panic!("expected SendToKakoune(Keys) from tier-1 export"),
    }
}
