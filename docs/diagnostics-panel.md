# Plugin Diagnostics Panel

When a plugin fails to load, fails its activation contract, or hits a
runtime error, Kasane surfaces the failure on three layers:

| Layer            | Lifetime                  | Where                                     |
|------------------|---------------------------|-------------------------------------------|
| Popup overlay    | Transient (warnings) /    | Top-right corner, painted above editor    |
|                  | until dismissed (errors)  | content                                   |
| Diagnostic panel | While the user has it open| Centered modal, opened with `<c-?>`       |
| Log file         | Permanent (rotated daily) | `~/.local/state/kasane/kasane.log.*`      |

The popup is meant as a "you may have just lost a feature" notification.
The panel is the persistent surface for reading what actually happened.
The log file is the audit trail.

## Popup overlay

Top-right floating box. The footer always shows the path to the active
log file so you can find the structured trace after the popup goes
away. Severity controls dismiss behavior:

- **Warnings** auto-dismiss after ~4 seconds.
- **Errors** persist until you dismiss them — opening the panel via
  `<c-?>` clears the popup as a side effect, so the modal owns the
  screen exclusively.

If multiple diagnostics arrive within a 750 ms window they are
coalesced into a single popup; the title shows `(N/M)` when more
diagnostics exist than the popup can display.

## Diagnostic panel

| Key                    | Action                                      |
|------------------------|---------------------------------------------|
| `<c-?>`                | Toggle the panel (open if closed, close if open) |
| `j` / `↓`              | Move selection down                         |
| `k` / `↑`              | Move selection up                           |
| `g`                    | Jump to the most recent entry               |
| `G`                    | Jump to the oldest in-buffer entry          |
| `PageDown` / `<c-d>`   | Move 10 entries down                        |
| `PageUp` / `<c-u>`     | Move 10 entries up                          |
| `y`                    | Yank a structured copy of the selected entry to the system clipboard |
| `r`                    | Trigger a plugin reload (whole-set; closes the panel) |
| `Enter`                | Open the active log file in Kakoune (closes the panel) |
| `q` / `Esc` / `<c-?>`  | Close the panel                             |

While open, the panel intercepts every key event before normal input
dispatch (`KEY_PRE_DISPATCH` capability), so the editor underneath
stays inert until the panel is closed.

### Layout

```
┌─ Plugin Diagnostics ─────────────────────────────────────────────┐
│ Plugin Diagnostics — 12 entries, 3 errors  <c-?> close           │
│ E   2s  session-ui: instantiation failed                         │
│ ▶ E  10s color-preview.runtime: channel decode failed            │
│ w  1m   pane-manager.config: unknown key 'autohide'              │
│ ...                                                              │
│ ── details ──────────────────────────────────────────────────── │
│ [runtime] plugin color-preview                                   │
│ message: channel decode failed: unexpected variant tag           │
│ method: on_state_changed                                         │
│ ↑↓/jk nav │ g/G top/bottom │ enter open log │ y yank │ q close   │
│ log: /home/user/.local/state/kasane/kasane.log                   │
└──────────────────────────────────────────────────────────────────┘
```

The selected row is highlighted on a yellow background. The detail
block beneath the entry list always reserves four rows so the footer
position never shifts as you navigate. Header includes the older-
entries-truncated count when the in-memory ring buffer (default 500
entries) has overflowed; older entries remain in the log file.

### Yank format

`y` copies a multi-line block suited for pasting into a bug report:

```
[ERROR] plugin session-ui (init)
  message: wasm trap: unreachable
  previous: bundled:session-ui@v0.4.1
  attempted: file:session-ui-v0.5.0.wasm@v0.5.0
  log: /home/user/.local/state/kasane/kasane.log
```

The clipboard write is silently a no-op on platforms where arboard
cannot acquire a clipboard handle (typically headless CI / Wayland
without a compositor).

### Open log in Kakoune

`Enter` closes the panel and sends `:edit <log>` to the active Kakoune
client. Because `tracing-appender::rolling::daily` writes to
`kasane.log.YYYY-MM-DD`, the panel walks the log directory at
keypress time and opens the most recently modified `kasane.log*`
file rather than the un-suffixed base path. If the log directory is
unreachable the panel falls back to the configured base path so
Kakoune still opens *something* the user can see.

## Log file

```
~/.local/state/kasane/kasane.log.YYYY-MM-DD
```

Override by setting either:

- `KASANE_LOG_STDERR=1` — redirect tracing to stderr (no file).
  Useful when running TUI under `2> log.txt`. Disables the panel
  footer's log-path hint.
- `XDG_STATE_HOME=/some/path` — relocate the entire kasane state
  directory.
- `log.file = "/explicit/path/kasane.log"` in `kasane.kdl` —
  override the directory only; the date suffix is still applied.

Filtering is controlled via `KASANE_LOG` (defaults to the value of
`log.level` in `kasane.kdl`):

```
KASANE_LOG=debug kasane              # see everything
KASANE_LOG=kasane_core::plugin=trace # narrow to one module
```

## History buffer

In addition to the file log, Kasane keeps the last 500 diagnostics
in memory so the panel can render them without going to disk. The
buffer is indexed by sequence number so navigation stays stable when
new entries arrive while the panel is open. Older entries that fall
out of the ring are reflected in the header as `(+N older in log)`.

Capacity is currently a build-time constant
(`DEFAULT_DIAGNOSTIC_HISTORY_CAPACITY` in
`kasane-core/src/plugin/diagnostics/history.rs`); making it
configurable is in the roadmap.

### Reload trigger

`r` touches the plugin reload sentinel file (`<plugins_dir>/.reload`),
which the long-running watcher thread polls every 500 ms. The watcher
fires `PluginReload`, the resolve pipeline re-runs, and any new or
changed plugin sources are loaded. Reload is whole-set: the existing
infrastructure has no per-plugin reload path. The panel closes on `r`
because the live plugin instances are about to be torn down and the
in-flight panel state would be stale.

If the configured plugins directory cannot be created (sandboxed
environment, read-only home directory), the reload is a no-op and a
debug-level tracing entry is logged. Per-plugin retry — bound to the
selected diagnostic's `plugin_id` rather than the whole set — is on
the roadmap.

## Out of scope (not yet implemented)

- Filter mode (`/`) to narrow by plugin name / severity / keyword
- Per-plugin reload (currently `r` triggers a whole-set reload)
- Persistent history across kasane restarts
- Public plugin API for reading diagnostic history (currently only
  the internal `BuiltinDiagnosticsPanelPlugin` reads it)
