# ADR-040: `kasane.kdl` Auto-Reload for `plugins` and `settings`

**Status:** Decided

**Context:**
Until 2026-Q2, edits to the `plugins` block (`enabled` / `disabled` /
`selection` / `deny_capabilities` / `deny_authorities`) and the
`settings` block in `kasane.kdl` had no live-update path:

- `plugins.enabled` was an input to `kasane plugin resolve`, which
  rewrites `plugins.lock`. Editing the kdl alone did nothing — the
  user had to run `resolve` and restart the editor.
- `settings.<plugin_id> { ... }` changes were silently ignored at
  runtime (`Config::restart_required_diff` did not even compare the
  field — see the Phase 0 fix).
- The `LockedWasmPluginProvider` snapshotted `PluginsConfig` and
  per-plugin settings at construction; subsequent kdl edits did not
  propagate to the running provider.

This produced a confusing UX: kdl is the user-facing source of truth,
but two of its sections required manual CLI invocations and a restart
to take effect. Hot-reload of theme / menu / search / clipboard / mouse
already worked, so plugins were the conspicuous exception.

**Decision:**
Introduce a `plugins.auto_reload` config flag (default `#false`) that
opts into a live update pipeline:

1. `Config::restart_required_diff` now compares `settings` (closing
   the silent-ignore bug) and includes `plugins` as before.
2. `PluginProvider` gains an `update_config` trait method (default
   no-op). `LockedWasmPluginProvider` wraps its dynamic state in
   `RwLock`s and replaces it under `update_config`.
3. A `ReloadOrchestrator` trait lives in `kasane-core` so the
   kdl-watcher path can drive the WASM resolve pipeline (which lives
   in the binary crate) without forcing a circular dependency.
   `kasane::orchestrator::DefaultReloadOrchestrator` implements it by
   calling `resolve_and_save` and touching the existing reload
   sentinel.
4. The kdl watcher in both TUI and GUI checks `auto_reload`; when it
   is on and `plugins`/`settings` changed, it pushes the new config to
   providers, runs resolve (for plugins changes), or fires
   `PluginReload` directly (for settings-only changes), and filters
   `plugins`/`settings` out of the restart-required warning.
5. Plugin unload now also cleans up exposed variables
   (`PluginVariableStore::clear_for_plugin`) and child processes
   (`ProcessDispatcher::kill_all_for_plugin`), so live reload doesn't
   leak per-plugin state.

**Rationale:**

- **Opt-in default** preserves the existing "resolve, then restart"
  workflow that CI and reproducible-build setups rely on. Users opt in
  per-config, and the existing failure mode (no automation, restart
  required) remains the safe default.
- **Lock file as cache, not source of truth** is rejected
  structurally: `plugins.lock` continues to record the resolved
  digests so reproducibility is preserved. `auto_reload` only changes
  *when* the lock is regenerated.
- **Trait-based orchestrator** keeps `kasane-core` free of
  `kasane_wasm` / `kasane_plugin_package` dependencies. A no-op
  implementation is provided for tests and non-WASM builds.
- **Per-plugin teardown** for variables and processes was a real bug
  even before this ADR (re-loading a plugin via the existing
  `kasane plugin install` sentinel path leaked the same way), so
  the cleanup work pays off independently of `auto_reload`.

**Alternatives considered:**

1. **Auto-reload on by default.** Rejected for now: this changes the
   behavior of CI scripts and silently re-runs the resolve pipeline
   on every save. Revisit after the opt-in period.
2. **Drop `plugins.lock`, read `enabled` directly.** Rejected:
   destroys digest-pinning and reproducibility; the lock file is a
   load-bearing primitive for `pin-digest` / `pin-package`.
3. **Per-field auto-reload (`plugins.auto_reload_settings #true`).**
   Deferred: the cost of the broader switch is small (resolve is
   milliseconds), and the bookkeeping isn't worth it until users
   report a need.
