// ADR-053 chunk 3: an `effects on` block that yields a clipboard-
// requiring effect compiles and emits `REQUIRED_CAPABILITIES` containing
// the `"clipboard"` capability. The const lives on the macro-generated
// `__KasaneEffectSet` marker; we don't probe it from this fixture
// (the marker is intentionally private), but its presence in the
// emitted code is what unlocks the chunk-4 mock-handler rejection path.

use kasane_plugin_sdk::effects::Effect;

kasane_plugin_sdk::define_plugin! {
    id: "effects_block_clipboard",

    effects on StateChanged(flags) {
        let _ = flags;
        yield Effect::PasteClipboard;
    },
}

fn main() {}
