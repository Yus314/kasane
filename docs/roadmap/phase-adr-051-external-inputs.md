# Phase ADR-051 — External Data as Salsa Inputs

**Status:** In progress (DDD-CST Phase α). [ADR-051](../decisions/adr-051-external-data-as-salsa-inputs.md)
remains Proposed (2026-05-22). Chunks 1+2 landed; Chunk 3 awaits open-decision
resolution.

Conceptual roots: [vision.md §4.1.8](../ddd-cst-vision.md) (push-to-set /
pull-to-derive split) and [ADR-050](../decisions/adr-050-salsa-scope-policy-observed-inputs-only.md)
(Salsa applies to tracked-query-backed inputs only).

## Progress

| Chunk | Description | Commit | Status |
|---|---|---|---|
| **1** | `ExternalInputRegistry` skeleton + 8 unit tests | `408cbe27` | ✓ landed |
| **2** | Frame-boundary `drain()` / `clear_dirty()` wiring | `ef51cf0c` | ✓ landed |
| **3a** | `kasane-syntax/src/watcher.rs` standalone module | — | open |
| **3b** | `SyntaxManager` integration (parallel path) | — | not started |
| **3c** | Registry-mediated reads (consumer migration) | — | not started |
| **3d** | Remove mtime-poll path | — | not started |
| **3e** | Integration property tests + `delta-24` perf comparison | — | not started |

## Chunk 3 — file-watcher → ExternalInputRegistry migration

### Goal

Connect the registry to a real **out-of-frame producer thread**. The
current `SyntaxManager` polls mtime synchronously in
`PreRenderHook::pre_render`; replacing this with a `notify`-backed
watcher thread is the smallest change that exercises the registry's
cross-thread / push-from-outside semantics.

### Architecture

```
[notify thread]                [main thread]
  watch_event ────channel────► try_recv_all()
                               registry.commit(id, path)
                                  ↓
                       sync_salsa_for_render():
                       1. handles.external.drain()
                       2. (downstream Salsa sync)
                       3. handles.external.clear_dirty()
                                  ↓
                       registry.last(id) → provider.update()
```

### Sub-chunk decomposition

| Sub-chunk | Deliverable | Estimated effort |
|---|---|---|
| 3a | `kasane-syntax/src/watcher.rs` standalone with `tempfile`-based integration test; no production hookup | 1–2 days |
| 3b | `SyntaxManager` holds `FileWatcher`; `pre_render` drains channel into registry; mtime poll **retained** for cross-validation log | 1 day |
| 3c | Consumer side reads `registry.last(id)` instead of internal state | 0.5 day |
| 3d | Remove mtime poll path (conditional on G2 below) | 0.5 day |
| 3e | Property tests + `cargo bench --bench rendering_pipeline -- --baseline delta-24` | 1 day |

Total: ~4–5 days. Each sub-chunk is an independent PR.

### Design decisions

| # | Question | Working answer | Status |
|---|---|---|---|
| **D1** | Value type held by registry | `PathBuf` (file I/O stays main-thread) | confirmed |
| **D2** | What to watch | Current buffer's file only; workspace-wide watching deferred | confirmed |
| **D3** | Channel form | `std::sync::mpsc::channel` (unbounded; back-pressure lives in registry policy) | confirmed |
| **D4** | `notify` library variant | `notify-debouncer-mini` vs `notify-debouncer-full` vs raw `notify` | **OPEN** |
| **D5** | Channel-drain call site | Inside `SyntaxManager::pre_render` initially; promote to dedicated hook if other sources adopt | confirmed |
| **D6** | Migration strategy | parallel path (3b retains mtime poll, 3d removes it) vs hard cutover at 3b | **OPEN** |

### Gotchas (known traps; address explicitly in sub-chunks)

| # | Trap | Mitigation |
|---|---|---|
| G1 | macOS FSEvents emits multiple events per save | debouncer required (drives D4) |
| **G2** | NFS / FUSE filesystems lack working inotify | **OPEN**: retain mtime poll as fallback permanently, or drop it (drives D6) |
| G3 | Editor save dance (`.swp` write + rename) emits `Create + Remove`, not just `Modify` | Watcher must accept any event kind on the watched path |
| G4 | Symlink vs canonical path mismatch | `canonicalize` once at `watch()` time; compare canonical forms |
| G5 | CI containers cap `fs.inotify.max_user_watches` (often 128 or 8192) | `FileWatcher::new()` must surface the OS error rather than panic; tests must cover graceful failure |
| G6 | Thread shutdown ordering at session teardown | Explicit `stop()` API on `FileWatcher`; do not rely solely on `Drop` |
| G7 | Watcher thread death is asynchronous | Main thread's `try_recv` returns `Err(Disconnected)`; check and either restart or fall through |

### Open decisions blocking Chunk 3a

Four items must be resolved before writing `kasane-syntax/src/watcher.rs`:

1. **D4 (debouncer choice).** Affects dependency footprint and API
   shape. `mini` is lighter, `full` adds debounce-with-content-hash and
   better cross-platform behaviour. Recommended: start with
   `notify-debouncer-full` unless workspace dep policy objects.
2. **D6 (migration strategy).** Parallel path is safer (regressions
   visible in cross-validation log); hard cutover is leaner. Recommended:
   parallel path.
3. **G2 (polling fallback retention).** If yes, Chunk 3d becomes
   "polling fallback for NFS only" rather than "remove poll".
   Recommended: yes (VS Code and JetBrains both retain fallback).
4. **Test strategy.** CI integration tests that touch real filesystem
   are flaky on some CI environments. Options: skip in CI with
   `#[ignore]`, gate by env var, or always run and tolerate flakes.
   Recommended: gate by `KASANE_RUN_FS_WATCH_TESTS=1` env var.

### Chunk 3a — concrete starting point

New file `kasane-syntax/src/watcher.rs`:

```rust
// API sketch — concrete types depend on D4 resolution

pub struct FileWatcher {
    inner: /* debouncer handle, exact type pending D4 */,
    rx: std::sync::mpsc::Receiver<PathBuf>,
    watched: Option<PathBuf>,
}

#[derive(Debug)]
pub enum FileWatcherError {
    Init(notify::Error),
    Watch(notify::Error),
    Unwatch(notify::Error),
}

impl FileWatcher {
    pub fn new() -> Result<Self, FileWatcherError>;
    pub fn watch(&mut self, path: &Path) -> Result<(), FileWatcherError>;
    pub fn unwatch(&mut self) -> Result<(), FileWatcherError>;
    /// Drain all pending events. Non-blocking. Returns paths in arrival order.
    pub fn try_recv_all(&mut self) -> Vec<PathBuf>;
    /// Explicit shutdown (G6). Drop also works but does not join the thread.
    pub fn stop(self);
}
```

Production code is **not touched in Chunk 3a**. The PR adds a new
module and tests only.

#### Tests (Chunk 3a)

Gate the real-FS tests behind `KASANE_RUN_FS_WATCH_TESTS=1` (per
"Test strategy" decision):

1. **happy path** — `tempdir`, write file, `watch`, modify file, poll
   `try_recv_all` with timeout, assert path appears.
2. **save dance** (G3) — write + rename, assert canonical path appears.
3. **graceful init failure** (G5) — set `RLIMIT_NOFILE` low or use a
   path on a `tmpfs` mount in an unwatch-supported way; assert
   `FileWatcherError::Init` returned, no panic.
4. **shutdown** (G6) — `stop()`, assert subsequent `try_recv_all`
   returns `Err(Disconnected)` (or empty Vec depending on API choice).

### Exit criterion for Chunk 3 (overall)

- Real `notify`-based watcher commits to the registry from a background
  thread; no mtime polling remains except (if G2 retained) the explicit
  NFS-fallback path.
- Property tests verify glitch-freedom and bounded memory under
  sustained push from a producer thread.
- `delta-24` baseline: rendering-pipeline benchmark within 110% (per
  ADR-051 exit criterion).
- The `#![allow(dead_code)]` attribute on `salsa_inputs/external.rs`
  is removed (first production caller has landed).

### What Chunk 3 does **not** do

- Workspace-wide watching (D2 deferred)
- Watching plugin source files (separate concern; existing `.reload`
  sentinel path stays in place)
- LSP-diagnostics integration (depends on a future LSP transport;
  out of Phase α scope per vision §8)
- Plugin-facing `ExternalInputId<T>` API (vision §8 explicitly defers
  this to a later step)

## Where to resume

A future session should:

1. Read this doc.
2. Confirm or revise the four **OPEN** decisions above (D4, D6, G2,
   test strategy). Recommended starting positions are in §"Open
   decisions blocking Chunk 3a".
3. Begin Chunk 3a per §"Chunk 3a — concrete starting point".
