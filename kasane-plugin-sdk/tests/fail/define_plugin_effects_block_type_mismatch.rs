// ADR-053 chunk 3: argument-type errors inside a `yield Effect::<Variant>(...)`
// expression are not the macro's concern — they surface as ordinary rustc
// type errors. This test pins the *quality* of that surfaced error: the
// span should point at the offending argument inside the user's source,
// not into macro-generated code.

use kasane_plugin_sdk::effects::Effect;

kasane_plugin_sdk::define_plugin! {
    id: "effects_block_type_mismatch",

    effects on StateChanged(flags) {
        let _ = flags;
        yield Effect::Redraw("not-a-u16");
    },
}

fn main() {}
