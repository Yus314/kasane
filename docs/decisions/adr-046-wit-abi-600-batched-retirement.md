# ADR-046: WIT ABI 6.0.0 — Batched Retirement

**Status:** Proposed (draft). Triggers when the W1 workstream
([Tier-1 ABI completion](../roadmap.md)) is ready
to ship. Targets `kasane:plugin@6.0.0`.

### Context

Several pending breaking changes are queued behind the
`docs/abi-versioning.md` policy "Remove or rename a function = major
bump":

- **F-1b (ADR-045 §Decision tail):** delete the WIT
  `evaluate-extension` guest export and the `ExtensionPointId`
  re-export. One line in `plugin.wit`, one line in
  `kasane-core/src/plugin/mod.rs`.
- **W1-B (per roadmap.md §2.2):** delete seven
  `#[deprecated(since = "0.7.1")]` lifecycle setters
  (`on_init`, `on_session_ready`, `on_state_changed`, `on_io_event`,
  `on_update`, `on_process_task`, `on_process_task_streaming`).
  Source-level Rust API removal; no WIT signature change, but the
  `#[kasane::plugin]` proc-macro is the sole live consumer and the
  proc-macro rewrite (W1-A) ships in the same release.
- **W1-A:** rewrite `#[kasane::plugin]` macro to emit tier-1 handler
  signatures. Source-level rewrite; the macro's DSL changes shape
  enough that existing plugin source code may need migration.
- **W1-C:** `PluginBackend` trait → `pub(crate)`. Source-level
  visibility narrowing; downstream crates (`kasane-wasm`,
  `kasane-tui`, `kasane-gui`) currently hold `Box<dyn PluginBackend>`
  and need to migrate to `Box<dyn Plugin> + HandlerRegistry`.

Each of these is independently a breaking change. The ABI policy in
`docs/abi-versioning.md` treats `(major, minor)` as a single wire
identity — every major bump invalidates every shipped `.wasm`. The
bundled and fixture wasm count is ~30 artifacts (6 `bundled/*.wasm` +
17 `fixtures/*.wasm` + 7 examples not bundled), and external plugin
authors must bump SDK pin + recompile.

Splitting these into N consecutive major bumps would invalidate the
wasm pipeline N times. Batching reverses the cost: one rebuild covers
all retirements simultaneously.

### Decision

Land the W1 sub-PRs in two waves. Wave 1 carries source-level
changes that don't touch the WIT shape. Wave 2 is a single atomic PR
that bumps the WIT version, deletes the WIT export, ships the wasm
rebuild, and updates `abi-versioning.md`.

| Wave | Sub-PR | Touches WIT? | Independent revert? |
|---|---|---|---|
| 1 | W1-A: rewrite `#[kasane::plugin]` macro | No | Yes |
| 1 | W1-D: split `bridge.rs` into 3 files | No | Yes |
| 1 | W1-E: consolidate 7 dispatch macros | No | Yes |
| 1 | W1-F: retire `effect_tiers.rs` shim | No | Yes (depends on W1-A landing first) |
| 1 | W1-C-prep: migrate `kasane-wasm`/`kasane-tui`/`kasane-gui` off `dyn PluginBackend` | No (Rust internal) | Yes |
| 2 | W1-B: delete 7 `#[deprecated]` setters | No (Rust source) | No — bundled with WIT bump |
| 2 | W1-C: `PluginBackend` → `pub(crate)` | No | No — bundled |
| 2 | F-1b: delete WIT `evaluate-extension` | Yes | No — bundled |
| 2 | WIT package version `5.0.0` → `6.0.0` | Yes | No — bundled |
| 2 | All bundled + fixture wasm rebuild | Yes (binary) | No — bundled |
| 2 | `abi-versioning.md` compatibility table row | No | No — bundled |
| 2 | `docs/migration/0.7-to-0.8.md` | No | No — bundled |

### Rationale

**Why two waves and not one mega-PR.** Wave-1 changes are large
(W1-A alone is 3-4 days of proc-macro work) and benefit from
independent code-review. They don't shift the wire format, so the
host can keep loading existing 5.0.0 wasm throughout Wave 1. Wave 2
is mechanical once Wave 1 stabilises: deletions + WIT bump + atomic
wasm regeneration.

**Why bundle W1-B with the WIT bump.** W1-B is technically a
source-level Rust API removal that doesn't touch the WIT signature.
But the seven setters' default impls live in `PluginBackend`, and
W1-C narrows `PluginBackend` visibility to `pub(crate)`. The cleanest
single PR boundary is "after the migration of all external consumers
off `dyn PluginBackend`" — that boundary is the W1-C-prep PR in
Wave 1, after which W1-B + W1-C land together in Wave 2.

**Why bundle F-1b here.** F-1b is one line of WIT. Doing it as a
standalone major bump would force every wasm to rebuild for that one
line. Bundling with W1 amortises the rebuild to zero marginal cost.
ADR-045 §Decision tail already deferred the WIT removal here.

**Why not also touch the manifest schema.** The
`handlers.extensions_defined` / `handlers.extensions_consumed`
manifest fields are now inert (ADR-045 deleted their dispatch). They
could be dropped from the manifest parser in this batch, but doing so
forces a manifest schema rev that ripples through every external
plugin's `kasane-plugin.toml`. The host migration window (single major
ABI bump) is the right time, but only if the field removal is
**user-visible useful**. The fields are pure overhead — recommend
ASKING during review whether to include manifest schema rev (deferred
to wave-2 PR design).

### Implications

- **`docs/abi-versioning.md`:** new compatibility row for 6.0.0; the
  "Remove or rename a function" major-bump example gets concrete
  citations (W1-B + F-1b).
- **`docs/migration/0.7-to-0.8.md`:** new migration guide with
  rewrite recipes for the seven deprecated setters → tier-1 siblings,
  the `define_extension` → `r.subscribe` recipe (cross-link
  ADR-045), and the `PluginBackend` → `Plugin` + `HandlerRegistry`
  recipe.
- **`docs/plugin-api.md`:** retire all `PluginBackend` references
  from authoring docs; the trait is `pub(crate)` and not a plugin
  surface.
- **`docs/plugin-development.md`:** macro DSL section rewritten for
  the new W1-A shape.
- **`kasane-plugin-sdk` major bump:** the SDK's tier aliases survive
  but its `#[deprecated]` companion `Effects`-returning helpers
  retire alongside the host setters.
- **`kasane-wasm`, `kasane-tui`, `kasane-gui`:** internal
  refactoring to move off `dyn PluginBackend`. No public API change
  at the binary boundary.
- **External plugins:** must rebuild against SDK 0.8.x. Migration
  guide covers all required source changes.
- **CHANGELOG:** single 0.8.0 release entry covering W1 + F-1b in
  one logical block. The two-wave merge structure is an internal
  process detail; the release is one shape change to the ABI.

### Risks

1. **W1-A is the schedule pacemaker.** The proc-macro rewrite is the
   only Wave-1 PR with significant design risk (compile-time
   return-type inference, DSL shape for tier-1 setters). If W1-A
   stalls, the whole batch stalls. Mitigation: land W1-A first; if
   blocked >2 weeks, descope W1-B to keep deprecation warnings (the
   ABI bump still ships, just without the source-level deletions).

2. **External plugins relying on `PluginBackend` directly.** The
   0-consumer audit assumed in-tree producers. External plugin
   authors who reached into `PluginBackend` instead of `Plugin` +
   `HandlerRegistry` will need to migrate. Mitigation: migration
   guide + `cargo-public-api` check pre-bump documenting the
   surface change.

3. **Wave-1 + Wave-2 interleaving risk.** If Wave-2 is in flight
   while a Wave-1 PR lands, conflicts arise in the same files
   (`bridge.rs`, `handler_registry/mod.rs`). Mitigation: Wave 2 is
   one atomic PR after Wave 1 is fully merged; no overlap window.

4. **`#[kasane::plugin]` proc-macro source breakage.** If W1-A
   changes the macro DSL shape (e.g. requires explicit
   `#[on_state_changed]` attributes instead of name-based inference),
   external plugin authors must rewrite plugin source. Mitigation:
   `define_plugin!` already supports tier-typed handlers (ADR-044
   Phase B-3); the macro rewrite should preserve the existing
   tier-typed DSL and only retire the legacy inference path.

### Alternatives considered

1. **One major bump per breaking change.** Rejected. F-1b alone
   forces ~30 wasm rebuilds. W1-B alone is source-only and could ship
   as a 5.x minor (per policy "remove a Rust function") if we
   carefully avoided the WIT touch, but bundling with the wave-2
   atomic is operationally simpler.

2. **Defer F-1b to 7.0.0.** Rejected. F-1b is one line; the migration
   window is already open via W1's major bump. Bundling has zero
   marginal cost.

3. **Skip the wave split — single mega-PR.** Rejected. W1-A's
   proc-macro rewrite is 3-4 days of focused work; combining it
   atomically with W1-B/C/F + WIT bump + 30-wasm rebuild produces
   a PR larger than reviewers can audit confidently. The wave split
   localises code-review cost.

4. **Manifest schema rev in the same batch.** Conditionally rejected
   (deferred to wave-2 PR design). Including it would touch every
   external plugin's `kasane-plugin.toml`; excluding it leaves two
   dead fields in the schema. Recommend reviewers decide at wave-2
   time.

### Open questions for review

- **Q1:** Should manifest fields `extensions_defined` /
  `extensions_consumed` be removed in this batch, or left as
  documented-inert? (See §Implications "Why not also touch the
  manifest schema".)
- **Q2:** Does W1-A's macro DSL need a compatibility shim for
  ABI 5.x plugin source? (Per ADR-044 Phase B-3, `define_plugin!`
  already supports tier-typed handlers; the question is whether to
  also retire the inference path or keep it for source-compat.)
- **Q3:** Does the `kasane-plugin.toml` `abi_version = "6.0.0"`
  string get host-validated against `kasane:plugin@6.0.0`, or
  against `kasane:plugin@6.X.Y`? (The existing
  `abi-versioning.md` policy is `(major, minor)` exact match; new
  6.0.0 should follow precedent unchanged.)

---
