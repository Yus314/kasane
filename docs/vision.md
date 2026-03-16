# Vision

## Why Kasane Exists

Kakoune exposes a `kak -ui json` protocol that decouples the editor core from its UI. This means the terminal rendering layer can be replaced entirely — without forking Kakoune, without modifying its source code.

Kasane takes advantage of this protocol to serve as a platform between Kakoune and the user. Terminal UIs have inherent limitations: no direct plugin API, limited to cell-based rendering, shell-script-only extension, and dependence on external tools like tmux for session management. Kasane removes these environmental constraints while preserving everything that makes Kakoune great.

## The `alias kak=kasane` Principle

Compatibility is the starting point, not a feature.

Users should be able to `alias kak=kasane` without changing their kakrc, their keybindings, their Kakoune plugins, or their workflow. Everything that works with Kakoune works with Kasane. Migration friction should be as close to zero as possible.

Progressive disclosure guides the design:

1. **Use** — `alias kak=kasane`, everything works
2. **Discover** — notice flicker-free rendering, clipboard, CJK support
3. **Configure** — enable smooth scrolling, themes, plugins
4. **Extend** — write plugins for custom UI elements
5. **Contribute** — improve the platform

Beyond compatibility, Kasane actively provides improvements in core where they are **objectively better** — not matters of taste or preference, but measurable, unambiguous enhancements that every user benefits from.

## Relationship to Kakoune

Kasane is a frontend, not a fork. A complement, not a replacement.

Kasane follows the Unix philosophy of clear responsibility separation. Kakoune handles editing — every buffer change, every cursor movement, every text operation. Kasane handles rendering and UI extension. External plugins extend both through their respective mechanisms. Kasane sits between them as a lubricant, ensuring they work together smoothly.

Kakoune plugins (shell-based, via `%sh{}` and `kak -p`) and Kasane plugins (WASM and native) coexist. They serve different purposes: Kakoune plugins extend the editor's behavior; Kasane plugins extend the editor's UI.

When improvements are needed in Kakoune's domain, Kasane contributes upstream rather than working around limitations.

## Kasane as a Platform

Kasane is not just an alternative UI — it is a platform that removes constraints and opens possibilities.

By providing both TUI and GPU backends as first-class options, Kasane eliminates dependence on tmux, window managers, or terminal-specific workarounds. Both backends receive equal attention; neither is a second-class citizen.

On this foundation, Kasane offers unbounded extensibility. The plugin system is designed so that anything a plugin author can imagine should be achievable — from simple line decorations to complex interactive overlays that transcend terminal limitations.

## For Plugin Authors

The plugin experience is central to Kasane's design:

- **Easy to write** — minimal code for powerful extensions. The framework handles state, caching, and lifecycle.
- **Easy to distribute** — WASM plugins are portable, single-file artifacts with no platform-specific builds.
- **Multi-language** — WASM enables plugins in any language that compiles to the Component Model.
- **Safe** — plugins run sandboxed and cannot crash or corrupt the editor.
- **Expressive** — access to rich UI primitives beyond what terminal cells can offer.
- **Thoroughly extensible** — contribution slots, transforms, line annotations, overlays, and more. The goal is that nothing is out of reach.
- **Low barrier** — comprehensive documentation, working examples, and an SDK that guides authors from first line to finished plugin.

The declarative UI model (currently TEA with a pure `view()` function) exists to serve plugin authors — it is a means to provide the most ergonomic authoring experience, not an end in itself. If a better approach emerges, the architecture can evolve.

## Plugin Ecosystem

- Simple plugin installation and management
- Dependency resolution handled by Kasane
- Peaceful coexistence with existing Kakoune plugin assets
- A community where plugin authors can share, discover, and build on each other's work

## Design Principles

- **Plugin-first** — features belong in plugins, not core. The core provides primitives; plugins build features.
- **Compatibility by default** — conservative defaults that match Kakoune's standard behavior. No surprises for new users.
- **Performance as prerequisite** — the most perceptive user on the best hardware (240 Hz displays, experienced typists) should be unable to perceive any difference from native Kakoune. ~49 μs/frame at 80×24 is a measured baseline, not an aspiration.
- **Correctness over convenience** — technology choices favor performance, extensibility, logical correctness, and safety, even when this means more implementation effort.
