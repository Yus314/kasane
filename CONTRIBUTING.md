# Contributing to Kasane

## Development Environment

### Nix (recommended)

The project provides a Nix flake with all dependencies. With [Nix](https://nixos.org/) and [direnv](https://direnv.net/) installed:

```bash
git clone https://github.com/Yus314/kasane.git
cd kasane
direnv allow  # Nix flake activates automatically
```

This provides the Rust toolchain (stable, with `wasm32-wasip2` target), GUI dependencies (Vulkan, Wayland, X11, fontconfig), and pre-commit hooks (rustfmt).

### Manual Setup

- [Rust](https://rustup.rs/) stable toolchain
- [Kakoune](https://kakoune.org/) v2026.04.12 or later
- For GUI backend: Vulkan SDK, Wayland/X11 development libraries, fontconfig
- For WASM plugins: `rustup target add wasm32-wasip2`

## Building and Testing

```bash
cargo build                              # TUI only
cargo build --features gui               # Include GPU backend
cargo test                               # All tests
cargo test -p kasane-core                # Single crate
cargo clippy -- -D warnings              # Lint (CI enforces -D warnings)
cargo fmt --check                        # Format check
```

## Commit Conventions

- English, [conventional commits](https://www.conventionalcommits.org/): `feat(scope):`, `fix:`, `refactor:`, `perf:`, `docs:`, `test:`
- A pre-commit hook runs `rustfmt` automatically

## Pull Requests

1. Create a branch from `master`
2. Make your changes — keep PRs focused on a single concern
3. Ensure `cargo test`, `cargo clippy -- -D warnings`, and `cargo fmt --check` pass
4. Open a PR against `master`

## Project Structure

See the Workspace Structure table in [CLAUDE.md](CLAUDE.md) for crate responsibilities. Module-level `//!` doc comments in each source file describe its contents. For architecture and design decisions, see [docs/index.md](docs/index.md).

## Reporting Issues

Open an issue on [GitHub](https://github.com/Yus314/kasane/issues) with:

- Kasane version (`kasane --version`)
- Kakoune version (`kak -version`)
- Terminal emulator and OS
- Steps to reproduce
- Relevant log output (`KASANE_LOG=debug kasane file.txt`)
