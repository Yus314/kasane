// Regression test for Issue #93: `declare-user-mode -override` was the
// sprout-dogfooding bug that motivated kak_lint!. Kakoune's
// declare-user-mode does NOT accept `-override`, so this should fail
// at compile time.

fn main() {
    let _ = kasane_plugin_sdk::kak_lint!("declare-user-mode -override 'sprout'");
}
