# ADR-007: Kakoune Process Management — Child Process Spawn + Session Connection

**Status:** Updated (daemon separation)

**Context:**
The question was how Kasane should launch and manage Kakoune.

**Decision:** By default, separate the Kakoune server into a headless daemon (`kak -d`) and connect the primary client via `-c`, matching pane clients. The `-c` option continues to support connection to an externally managed daemon session.

**Startup patterns:**
- `kasane file.txt` → spawns daemon `kak -d -s kasane-<pid> file.txt` + client `kak -ui json -c kasane-<pid>`
- `kasane -s myses file.txt` → spawns daemon `kak -d -s myses file.txt` + client `kak -ui json -c myses`
- `kasane -c mysession` → connects to existing daemon session via `kak -ui json -c mysession` (no daemon spawned)

**Rationale:**
- Kakoune's daemon mode (`kak -d -s` / `kak -c`) is an important multi-client workflow
- Not supporting `-c` would be a major limitation for Kakoune users
- JSON UI connection uses a `kak -ui json -c` process for both new and existing sessions, so the pipe mechanism is identical
- Daemon separation ensures that `:q` on the primary pane produces an EOF on its stdout, so `KakouneDied` fires correctly in multi-pane configurations. Without separation, the co-located server keeps stdout open even after the client portion exits
