# ADR-023: Session Management Boundaries — Mechanism / Policy Split

**Status:** Current

### Context

Kasane's `SessionManager` manages multiple Kakoune processes, with `SessionStateStore` preserving `AppState` snapshots for inactive sessions. Prior to this decision, session information was invisible to plugins: there was no query API, no lifecycle event notification, and no command for plugins to switch sessions.

The roadmap identifies two active workstreams: Session/Surface parity (automatic surface generation per session) and Multi-session UI parity (session switcher/list). The question is which parts of these belong to core and which to plugins.

### Decision

Apply the principle of "mechanism, not policy" to session management:

- **Core owns mechanism**: process lifecycle, state snapshots, session-bound surface generation, switching mechanics
- **Plugins own policy**: session UI presentation, switching keybindings, status indicators, list decoration

Core additionally provides **infrastructure for plugin observability**:

1. Session descriptors exposed in observable state (session list, active session ID)
2. Session lifecycle dirty flag (`DirtyFlags::SESSION`) for cache invalidation
3. Session switch command exposed to plugins (including WIT)

### Rationale

The decision criterion is "Does a single correct implementation exist?":

- Process management, snapshot atomicity, and surface binding have single correct implementations → Core
- Session UI presentation varies by user preference → Plugin
- Observation and command infrastructure is owned by core (source of truth) but exists to enable plugins

This separation means the default session UI can ship as a bundled WASM plugin, replaceable by users. Core remains minimal and policy-free.

### Alternatives Considered

| Alternative | Rejected because |
|---|---|
| All-core (session UI in core) | Session UI is display policy; hardcoding it prevents customization and contradicts the layer model |
| All-plugin (session lifecycle in plugins) | Process management requires backend-specific wiring (reader/writer streams) that cannot be safely exposed to plugins |

### Implementation Order

1. ~~Core infrastructure: session descriptors in observable state, `DirtyFlags::SESSION`, `SessionCommand::Switch`~~ — Done
2. Session/Surface parity: automatic surface generation and deterministic switching
3. Session UI plugin: bundled WASM providing default session switcher
