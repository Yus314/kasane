# RFC-107a — UniversalRevealState (host-managed Hide recovery)

**Status:** proposed — decomposition of [#107](https://github.com/Yus314/kasane/issues/107).
**Replaces in part:** the per-plugin manifest witness API proposed in #107.
**Companion RFCs:** 107b (per-plugin declaration, optional refinement), 107c (enforcement policy ADR), 107d (kdl hot-reload, blocks `SettingToggle`).

## Premise correction

#107 states "every `hide_inline()` emission from a WASM plugin is silently dropped". Source verification (`kasane-core/src/plugin/handler_registry/decoration.rs:26`, `kasane-core/src/plugin/handler_table.rs:78-80`) shows that `is_display_recoverable()` is queried only by tests; no production code path filters destructive directives based on the source plugin's `DisplayRecoveryStatus`.

ADR-030 §Level 4 shipped the *infrastructure* (`DisplayRecoveryStatus`, `RecoveryWitness`, `is_visually_faithful`) but not the runtime enforcement. The `Hide` filter mechanism #107 cites does not exist.

Consequently, the markdown-rich symptom that motivated #107 — `hide_inline()` not taking effect from WASM — is **not** caused by the recovery audit. It is either (a) caused by something else in the WASM→algebra translation path, or (b) a precaution shipped against an enforcement policy that has not yet landed.

## Proposal

Mirror the `FoldToggleState` pattern (ADR-030 §"Note: Fold is classified as Preserving") for `Hide` / `HideInline`.

### Type

```rust
// kasane-core/src/display/reveal_state.rs (new)
pub struct UniversalRevealState {
    reveal_all: bool,
}

impl UniversalRevealState {
    pub fn toggle(&mut self) { self.reveal_all = !self.reveal_all; }
    pub fn is_revealed(&self) -> bool { self.reveal_all }
}
```

Stored on `AppState` alongside `FoldToggleState`.

### Algebra integration

`kasane-core/src/display/algebra/normalize.rs` already treats `Content::Hide` as set-union-commutative (lines 251-256). Add a pre-normalize pass:

```rust
if reveal_state.is_revealed() {
    leaves.retain(|leaf| !matches!(leaf.display.content, Content::Hide { .. }));
}
```

`Hide` leaves are dropped before normalization, so no displacement of decorations occurs (cf. RFC-107c discussion of pre-algebra vs post-algebra filtering).

### Input layer

Bind `<F12>` (configurable via `kasane.kdl`) in `BuiltinInputPlugin` to dispatch `Msg::ToggleUniversalReveal`. State update flips the flag and triggers `dirty::display`.

### §10.2a satisfaction

For any `Hide`/`HideInline` directive `x` emitted by *any* plugin: σ = `[<F12>]`, |σ| = 1. The host owns the recovery mechanism; no plugin declaration required; no trust placed in the plugin.

## Why this is strictly stronger than per-plugin witness

| Axis | #107 per-plugin witness | RFC-107a UniversalReveal |
|---|---|---|
| Plugin-side cost | manifest section + actual key binding implementation | zero |
| Host verifiability | declaration only (plugin can lie) | host-enforced via algebra |
| Key conflict surface | N plugins × K keys | 1 host-owned key |
| Discoverability | each plugin documents its own | one entry in `<F1>` help |
| §10.2a "bounded" steps | varies per plugin (1 to ∞) | exactly 1 |
| `Declared` mechanism escape hatch | present | not applicable |
| Implementation footprint | manifest schema + WIT types + adapter wiring + macro extension + ABI bump | one state field, one algebra pass, one keybinding |
| markdown-rich requirement satisfaction | ✅ | ✅ (coarser granularity, but sufficient) |

#107 "Alternative B: Auto-witness every WASM plugin with a hardcoded 'press R to reload plugin'" rejects this kind of host-owned recovery — but on the wrong grounds. That alternative proposed **reload** (destroying plugin state). UniversalReveal proposes **suppression** (algebra-level Hide → Identity), preserving plugin state entirely. The rejection in #107 does not apply.

## Granularity discussion

Coarse: `<F12>` reveals all hidden content, across all plugins, simultaneously. A user running markdown-rich + a hypothetical "hide-comments" plugin gets both reveals at once.

This is acceptable because:
- Multi-plugin Hide overlap is rare (a single Hide-emitting plugin per concern is the common case).
- The global reveal is an escape hatch, not the primary workflow. Users who want per-plugin granularity install RFC-107b.
- `FoldToggleState` is already global with the same tradeoff and has not surfaced complaints.

## Compatibility

- **ABI bump:** none. No WIT, manifest, or SDK change.
- **Existing plugins:** zero migration. `Hide` emissions from native and WASM plugins all become recoverable for free.
- **`is_display_recoverable()`:** semantics shift — every plugin's display becomes faithful by virtue of host-provided recovery, regardless of plugin-side witness. The query becomes redundant; flag for retirement after this RFC ships (one-line patch).
- **`SafeDisplayDirective`:** unchanged. Still useful for plugins that want compile-time non-destructiveness.

## Performance

- One bool read per frame in `normalize()`.
- When `reveal_all == false` (default): zero overhead beyond the bool check.
- When `reveal_all == true`: one extra `retain` pass over `leaves` (typical N < 100 per frame).
- ADR-024 perceptual budget: < 100 ns expected, well within 1 µs slop.

## Acceptance criteria

- [ ] `UniversalRevealState` field on `AppState`, default `reveal_all = false`.
- [ ] `Msg::ToggleUniversalReveal` dispatches through TEA update; emits `dirty::display`.
- [ ] `BuiltinInputPlugin` binds `<F12>` (settable via `kasane.kdl [input.reveal_key]`).
- [ ] `normalize()` pre-pass drops `Content::Hide` leaves when `reveal_all == true`.
- [ ] Test: native plugin emitting `Hide` is suppressed by default, revealed after toggle.
- [ ] Test: two plugins both emitting overlapping `Hide` are both revealed by single toggle.
- [ ] Test: `StyleInline` from a low-priority plugin that was being displaced by a high-priority `Hide` becomes visible on reveal.
- [ ] Test: toggle is idempotent — second toggle restores hidden state.
- [ ] `<F1>` help text mentions the reveal key.
- [ ] `docs/plugin-development.md` § "Visual Faithfulness" updated: native plugins may use raw `on_display` again; UniversalReveal provides framework-maintained recovery analogous to `FoldToggleState`.
- [ ] `docs/decisions/adr-030-*.md` §Level 4 amended (or follow-up ADR) acknowledging the host-managed pattern.

## Out of scope

- **Per-plugin reveal keys:** deferred to RFC-107b (manifest-declared witnesses, optional refinement on top of universal).
- **Per-range reveal:** "reveal only this Hide" requires cursor-aware suppression with no current precedent (see #107 deep analysis §9). Not motivated by markdown-rich.
- **Enforcement of `Unwitnessed`:** the question of whether to drop destructive directives from non-faithful plugins becomes moot once UniversalReveal makes every plugin faithful. RFC-107c is preserved for completeness but its motivation weakens substantially.
- **`SettingToggle` mechanism:** depends on RFC-107d (`kasane.kdl` hot-reload). Independent.

## Open questions

- [ ] Default binding: `<F12>` collides with browser fullscreen on some terminals; `<a-r>` is currently bound to nothing host-side per `kasane-core/src/input/builtin.rs`. Prefer `<a-r>` for "reveal" semantics, fall back to user config?
- [ ] Should reveal state persist across sessions (`workspace::persist`)? Probably no — fresh session, fresh hides.
- [ ] Should there be an *additive* counterpart: a "Hide all the decorations" toggle for users who want raw text? Out of scope; mention only.
- [ ] Should `Fold` directives also be revealed by the universal key, or do they retain their own toggle? Keep separate — fold has semantic meaning (collapsed structure) that differs from Hide (decluttering).

## Related

- Parent issue: [#107](https://github.com/Yus314/kasane/issues/107)
- Tracker: [#103](https://github.com/Yus314/kasane/issues/103)
- Pattern precedent: `FoldToggleState` — `kasane-core/src/display/projection.rs:50-100`, ADR-030 §"Note: Fold is classified as Preserving"
- Algebra Hide-Hide commutativity: `kasane-core/src/display/algebra/normalize.rs:251-256`
- Visual Faithfulness §10.2a: `docs/semantics.md:909-923`
- Recovery witness types (unchanged by this RFC): `kasane-core/src/plugin/algebra/recovery_witness.rs`
