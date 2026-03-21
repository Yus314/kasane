# Documentation

Start here: [Getting Started](getting-started.md) · [What's Different](whats-different.md) · [Configuration](config.md)

## By Reader

**Use Kasane:**
[Getting Started](getting-started.md) → [Configuration](config.md)

**What's different from Kakoune?**
[What's Different](whats-different.md)

**Use plugins:**
[Using Plugins](using-plugins.md)

**Build plugins:**
[Plugin Development](plugin-development.md) → [Plugin API](plugin-api.md) → [WASM Constraints](wasm-constraints.md)

**Use GPU backend:**
[What's Different](whats-different.md#opt-in-gpu-backend) → [Configuration](config.md#gui-backend)

**Troubleshooting:**
[Troubleshooting](troubleshooting.md)

**Understand internals:**
[Semantics](semantics.md) (state, rendering, plugin composition)

**Design decisions:**
[Vision](vision.md) → [Decisions](decisions.md)

**Project status:**
[Roadmap](roadmap.md)

## System Architecture

Kasane sits between the user and Kakoune as an independent rendering layer.
Kakoune handles editing; Kasane handles display, plugin composition, and
frontend-native capabilities. For formal definitions, see
[semantics.md](semantics.md) §2.

### Three-Layer Responsibility Model

| Layer | Definition | Decision Criteria |
|---|---|---|
| Upstream (Kakoune) | Protocol-level concerns | Does it require a protocol change? |
| Core (`kasane-core`) | Faithful rendering of the protocol + frontend-native capabilities | Does a single correct implementation exist? |
| Plugin | Features where policy may vary | Everything else |

```text
Want to add feature F
  │
  ▼
1. Does it require a protocol change?
  │  Yes → Upstream (record in upstream-dependencies.md)
  │  No ↓
  ▼
2. Does a single correct implementation exist?
  │  Yes → Core (kasane-core)
  │  No ↓
  ▼
3. Plugin
  │  Otherwise → External plugin (WASM or native)
  │  Insufficient API? → Plugin trait / WIT extension comes first
```

For the full decision record, see [ADR-012](decisions.md#adr-012-layer-responsibility-model).

## Document Classification

### Canonical

Authoritative specifications, responsibilities, and usage guides.

- [requirements.md](requirements.md) — core and extension requirements
- [semantics.md](semantics.md) — current semantics authority
- [plugin-development.md](plugin-development.md) — plugin development guide
- [plugin-api.md](plugin-api.md) — plugin API reference
- [wasm-constraints.md](wasm-constraints.md) — WASM plugin constraints and evolution path
- [config.md](config.md) — configuration reference
- [performance.md](performance.md) — performance principles, benchmarks, and optimization status

### User-facing

Guides for users and plugin authors.

- [getting-started.md](getting-started.md) — installation and first run
- [whats-different.md](whats-different.md) — features and improvements
- [using-plugins.md](using-plugins.md) — enabling and managing plugins
- [troubleshooting.md](troubleshooting.md) — common issues and solutions
- [vision.md](vision.md) — project goals and direction

### Tracking

State, progress, and blockers.

- [roadmap.md](roadmap.md) — implementation phases and incomplete items
- [upstream-dependencies.md](upstream-dependencies.md) — upstream dependencies and reintegration conditions

### Historical / Research

History, analysis, and background.

- [decisions.md](decisions.md) — ADR and design decision history
- [kakoune-protocol-constraints.md](kakoune-protocol-constraints.md) — protocol constraint analysis

### Supporting Reference

- [profiling.md](profiling.md) — measurement procedures
