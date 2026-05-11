// kak_lint! accepts known commands with valid flags AND unknown commands
// (additive policy: never produces a false positive against a real Kakoune
// command). It expands to the original literal so it can be embedded in
// any `&'static str` context.

fn main() {
    // Known command, valid flag.
    let a: &str = kasane_plugin_sdk::kak_lint!("declare-user-mode -hidden 'sprout'");
    assert!(a.contains("declare-user-mode"));

    // Known command wrapped in `try` — linter recurses into the body.
    let b: &str = kasane_plugin_sdk::kak_lint!("try %[ declare-user-mode 'sprout' ]");
    let _ = b;

    // Unknown command — pass-through (no false positives).
    let c: &str = kasane_plugin_sdk::kak_lint!("super-fancy-cmd -with -anything");
    let _ = c;

    // -docstring takes a value that happens to start with `-`.
    let d: &str = kasane_plugin_sdk::kak_lint!("map -docstring 'help' global user k a");
    let _ = d;
}
