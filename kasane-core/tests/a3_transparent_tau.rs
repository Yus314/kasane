//! A3 (Behavioral Equivalence) τ-transition property test for transparent commands.
//!
//! From `docs/semantics.md` §2:
//!
//! > A3 (Behavioral Equivalence): Kasane's observable behaviour is identical to
//! > Kakoune's for all Kakoune-defined transitions.
//!
//! The τ-transition corollary: a transparent command (one that is *not*
//! Kakoune-writing) cannot produce bytes of Kakoune output when executed.
//! This test witnesses that property via proptest over the non-deferred
//! transparent command variants.

use kasane_core::clipboard::SystemClipboard;
use kasane_core::plugin::{KakouneSafeCommand, execute_commands};
use kasane_core::state::DirtyFlags;
use proptest::prelude::*;

/// Strategy that generates arbitrary `KakouneSafeCommand` instances
/// restricted to the two non-deferred variants that reach `execute_commands`:
/// `RequestRedraw` and `Quit`.
fn arb_transparent_command() -> impl Strategy<Value = KakouneSafeCommand> {
    prop_oneof![
        any::<u16>().prop_map(|bits| {
            KakouneSafeCommand::request_redraw(DirtyFlags::from_bits_truncate(bits))
        }),
        any::<bool>().prop_map(|_| KakouneSafeCommand::quit()),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Every KakouneSafeCommand, when executed as an immediate command,
    /// produces zero bytes of Kakoune output. This witnesses the A3
    /// τ-transition property for transparent commands.
    #[test]
    fn transparent_immediate_commands_produce_no_kakoune_output(
        cmd in arb_transparent_command()
    ) {
        let command = cmd.into_command();
        // Only test non-deferred commands (the ones that reach execute_commands).
        if !command.is_deferred() {
            let mut output = Vec::new();
            let mut clipboard = SystemClipboard::noop();
            let _ = execute_commands(vec![command], &mut output, &mut clipboard);
            prop_assert!(
                output.is_empty(),
                "transparent command produced Kakoune output: {} bytes",
                output.len()
            );
        }
    }
}

#[test]
fn transparent_command_is_never_kakoune_writing() {
    // Verify the structural invariant: every KakouneSafeCommand variant
    // maps to a non-writing Command variant.
    let samples = vec![
        KakouneSafeCommand::request_redraw(DirtyFlags::ALL),
        KakouneSafeCommand::quit(),
        KakouneSafeCommand::paste_clipboard(),
        KakouneSafeCommand::set_config("k".into(), "v".into()),
    ];
    for tc in samples {
        let cmd = tc.into_command();
        assert!(
            !cmd.is_kakoune_writing(),
            "KakouneSafeCommand should never be Kakoune-writing, but {} is",
            cmd.variant_name()
        );
    }
}
