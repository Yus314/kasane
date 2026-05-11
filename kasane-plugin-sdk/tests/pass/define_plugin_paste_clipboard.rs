// ADR-044: session-ready is tier-1; the body must evaluate to
// `KakouneSideEffects`. `paste_clipboard()` produces a tier-2 `Command`,
// so build via `kakoune_setup_effects!` (returns `KakouneSideEffects`)
// — the macro emits the variant under `KakouneSideCommand::EvalCommand`,
// and a paste-clipboard helper is added to the tier-1 vocabulary via
// the helpers module.
kasane_plugin_sdk::define_plugin! {
    id: "paste_clipboard_helper",

    on_active_session_ready_effects() {
        KakouneSideEffects {
            redraw: dirty::STATUS,
            commands: vec![KakouneSideCommand::PasteClipboard],
            scroll_plans: vec![],
        }
    },
}

fn main() {}
