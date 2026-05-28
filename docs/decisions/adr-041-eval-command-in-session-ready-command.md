# ADR-041: `eval-command` in `session-ready-command`

**Status:** Decided (2026-05-11). Landed in commit `dd2fbe3a`.

**Tracked in:** [Issue #89](https://github.com/Yus314/kasane/issues/89) (closed)

**Context:**

`session-ready-command` (WIT 3.0.0, `kasane-wasm/wit/plugin.wit`) is
restricted to three variants:

```wit
variant session-ready-command {
    send-keys(list<string>),
    paste-clipboard,
    plugin-message(message-config),
}
```

The general-purpose `command` variant includes a fourth, `eval-command(string)`,
which is the canonical clean way to issue a Kakoune command body without
keystroke simulation. Plugins registering Kakoune-side APIs at
`on_active_session_ready_effects` (defining commands, declaring user
modes, mapping keys) cannot use it — they must fall back to
`send-keys(keys::command(cmd))`, which:

1. Carries an `<esc>` mode-reset side-effect.
2. Translates each character into a separate `Vec<String>` element
   (memory + IPC overhead).
3. Requires per-character escaping (`<` → `<lt>`, `%` → `<percent>`).
4. Cannot represent multi-line bodies (`\n` is silently broken at
   the Kakoune prompt; the SDK debug-asserts).

This was surfaced by the sprout dogfooding tracker (Issue #81).

**Decision:**

Add `eval-command(string)` to the `session-ready-command` variant,
matching its position in the general `command` variant. Bump the WIT
ABI from 3.0.0 → 4.0.0 (variant cases are wire-ordered; the addition
is *additive in source* but *breaking in wire encoding*).

```wit
variant session-ready-command {
    send-keys(list<string>),
    eval-command(string),    // NEW
    paste-clipboard,
    plugin-message(message-config),
}
```

Host-side translation (`kasane-wasm/src/convert/command.rs`
`wit_session_ready_effects_to_effects`) gains one match arm:

```rust
wit::SessionReadyCommand::EvalCommand(cmd) => Command::kakoune_command(cmd),
```

**Rationale:**

The pre-RFC investigation ([Issue #89](https://github.com/Yus314/kasane/issues/89))
established that the exclusion is **cosmetic, not technical**:

- `Command::EvalCommand` is internally identical to a wrapped
  `Command::SendKeys` at the host level: `Command::kakoune_command`
  (`kasane-core/src/plugin/command.rs`) routes both through
  `KasaneRequest::Keys(<esc>:cmd<ret>)`.
- `KasaneRequest` itself has only one Kakoune-bound command variant
  (`Keys(Vec<String>)`); `EvalCommand` is a plugin-API ergonomic
  surface, not a separate wire protocol.
- The host connection state requirement is identical for the two
  forms; both reach Kakoune through the same `keys` JSON-RPC method.
- The original 3-variant set landed in commit `3733839a` (2026-03-20)
  as the typed-effects MVP — the minimum needed by then-existing
  plugins. No deliberate exclusion of `eval-command` was recorded.

Implementation cost is therefore near-zero:

- WIT: 1 line.
- Host translation: 1 match arm.
- SDK regeneration via `wit-bindgen`: automatic.
- Plugin migration: pure recompile against ABI 4.0.0 + opt-in to
  the new variant where ergonomic.

**Alternatives considered:**

1. **Bundle `set-config` and `set-setting` into the same RFC.**
   Rejected: both are legitimately useful at session-ready (snapshot a
   plugin counter into a Kakoune option, etc.) but each introduces its
   own design decisions (persistence semantics, ordering against
   user-config). Bundling delays consensus on the simple case. Track
   as follow-up issues.

2. **Add `eval-command` without an ABI bump.** Not viable: WIT
   variant ordering is part of the wire encoding (see
   `docs/abi-versioning.md` Appendix A); adding a case shifts the
   discriminants of subsequent cases. The host's exact major.minor
   match (`kasane-plugin-package/src/manifest.rs` `abi_compatible`)
   would reject pre-4.0.0 binaries cleanly regardless.

3. **Backport: ship as a 3.1.0 minor bump.** Rejected for the same
   reason as #2 — minor *is* breaking on the wire by policy.

**Coordination with ADR for #90:**

[Issue #90](https://github.com/Yus314/kasane/issues/90)
(`command-error-event` for plugin observability) also targets ABI
4.0.0. If both proposals reach Decided status before any host
release, bundle them into a single 3.0.0 → 4.0.0 bump so plugin
authors recompile once. If #90 takes longer, ship this ADR's
addition under 4.0.0 alone and route #90 to 4.1.0 (still a recompile,
under the minor-is-breaking policy).

**Implications:**

- Every WASM plugin in the ecosystem rebuilds against ABI 4.0.0 (one
  mechanical `cargo component build`; no source changes required).
- Bundled examples + fixtures regenerate.
- `kasane_plugin_sdk` minor-bumps to 0.7.0.
- The SDK gains a convenience helper for `eval-command`-style session-ready
  effects, deprecating the `keys::command`-wrapped fallback for the same
  body (the fallback remains valid for ABI 3.x plugins).
- Migration guide entry in `docs/migration/0.6-to-0.7.md`: one row in
  the symbol table, no source-level breakage.
