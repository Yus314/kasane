# ADR-005: Development Environment Management — Nix flake + direnv

**Status:** Decided

**Context:**
A consistent environment for the Rust toolchain (rustc, cargo, clippy, rustfmt) and system-dependent libraries (various C libraries used by crossterm, Phase 4 wgpu dependencies, etc.) needed to be provided across developers.

**Decision:** Manage the development environment with `flake.nix` + `.envrc` (`use flake`).

**Rationale:**
- `nix develop` / `direnv allow` provides the toolchain and dependency libraries in one step
- `flake.lock` guarantees build reproducibility
- A single `flake.nix` can support both macOS (darwin) and Linux platforms
- Using the same Nix environment in CI avoids "works locally but fails in CI" problems
- The Rust toolchain is managed via `rust-overlay` or `fenix`, kept consistent with `rust-toolchain.toml`
