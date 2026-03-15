# Vision

## Why Kasane Exists

Kakoune exposes a `kak -ui json` protocol that decouples the editor core from its UI. This means the terminal rendering layer can be replaced entirely — without forking Kakoune, without modifying its source code.

Kasane takes advantage of this protocol to provide an independent rendering layer with a plugin platform. Terminal UIs have inherent limitations: no direct plugin API, limited to cell-based rendering, and shell-script-only extension. Kasane addresses these by sitting between Kakoune and the user, rendering the same UI but with additional capabilities.

## The `alias kak=kasane` Principle

Compatibility is the starting point, not a feature.

Users should be able to `alias kak=kasane` without changing their kakrc, their keybindings, their Kakoune plugins, or their workflow. Everything that works with Kakoune works with Kasane.

Progressive disclosure guides the design:

1. **Use** — `alias kak=kasane`, everything works
2. **Discover** — notice flicker-free rendering, clipboard, CJK support
3. **Configure** — enable smooth scrolling, themes, plugins
4. **Extend** — write plugins for custom UI elements
5. **Contribute** — improve the platform

## Relationship to Kakoune

Kasane is a frontend, not a fork. A complement, not a replacement.

Kasane depends on Kakoune — there is no Kasane without Kakoune. Every editing operation, every buffer change, every cursor movement happens in Kakoune. Kasane only handles rendering and UI extensions.

Kakoune plugins (shell-based, via `%sh{}` and `kak -p`) and Kasane plugins (WASM and native) coexist. They serve different purposes: Kakoune plugins extend the editor's behavior; Kasane plugins extend the editor's UI.

## Where Kasane Is Going

- **Plugin ecosystem** — more plugins, easier distribution, stable API
- **GPU backend maturity** — full feature parity with TUI, font rendering improvements
- **Multi-session management** — split panes, tab-like session switching

## Design Principles

- **Plugin-first** — features belong in plugins, not core. The core provides primitives; plugins build features.
- **Compatibility by default** — conservative defaults that match Kakoune's standard behavior. No surprises for new users.
- **Performance as prerequisite** — ~49 us/frame at 80x24. A plugin platform that slows down the editor is not viable.
- **Declarative UI** — TEA (The Elm Architecture) with a pure `view()` function. Plugins declare what to render, not how to render it.
