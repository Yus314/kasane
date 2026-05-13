# Using Plugins

## Kakoune Plugins

Your existing Kakoune plugins (kak-lsp, fzf.kak, plug.kak, auto-pairs.kak, etc.) work unchanged. They run inside Kakoune and are not affected by Kasane.

## Kasane Plugins

Kasane has its own plugin system for UI extensions. Kasane plugins can add visual elements, decorations, overlays, and input handling that Kakoune's shell-based plugins cannot.

The plugin API provides extension points for UI contributions, line annotations, overlays, transforms, input handling, and more. For the full list, see [plugin-api.md §1.2](./plugin-api.md#12-choosing-a-mechanism).

## Bundled WASM Plugins

Kasane embeds a curated pair of WASM plugins in the binary. Source lives in
[`examples/wasm/`](../examples/wasm/):

| Plugin | ID | Default | Demonstrates |
|---|---|---|---|
| Cursor Line | `cursor_line` | Off | Display directives (`display`) — line styling |
| Color Preview | `color_preview` | Off | Line annotation + overlay + mouse input |

The wider example fleet (sel-badge, fuzzy-finder, pane-manager, smooth-scroll,
prompt-highlight, session-ui, image-preview, line-numbers, virtual-text-demo,
selection-algebra, etc.) is slated to move to a future external
`kasane-plugin-gallery` repo (γ/δ-3 cleanup); historical sources are
recoverable from this repo's git log.

## Enabling Bundled Plugins

Bundled WASM plugins are off by default. Add plugin IDs to the `enabled` list
in your configuration to activate them:

```kdl
// ~/.config/kasane/kasane.kdl
plugins {
    enabled "cursor_line" "color_preview"
}
```

## Installing External Plugins

### WASM Plugins

Install `.kpk` files with:

```bash
kasane plugin install path/to/my-plugin-0.1.0.kpk
```

Kasane stores verified packages under `~/.local/share/kasane/plugins/` (or
`$XDG_DATA_HOME/kasane/plugins/` if `$XDG_DATA_HOME` is set), writes the active
set into `plugins.lock`, and activates plugins from that lock file on startup.

If you change `enabled`, `disabled`, or selection policy in `kasane.kdl`, run:

```bash
kasane plugin resolve
```

to rebuild `plugins.lock`.

### Auto-reload on `kasane.kdl` changes

By default, edits to the `plugins` or `settings` blocks in `kasane.kdl`
require running `kasane plugin resolve` and restarting kasane. To make
both happen automatically when the file is saved, set
`plugins.auto_reload`:

```kdl
plugins {
    auto_reload #true
    enabled "cursor_line"
}
```

With `auto_reload #true`:

- Editing `plugins.enabled` / `plugins.disabled` / `plugins.selection`
  triggers an automatic `resolve`, rewrites `plugins.lock`, and live-swaps
  the running plugin set.
- Editing `settings.<plugin_id> { ... }` updates the per-plugin settings
  in place — no lock change required, no restart.
- Resolution failures (missing packages, version conflicts) are surfaced
  through the diagnostic overlay; the previous lock is kept so the editor
  keeps working.
- Changes to `font`, `window`, `ui.backend`, and `log` still require a
  restart — those touch backend lifecycle in ways live reload can't
  cover.

The default is `#false` to preserve the explicit "resolve, then restart"
workflow that CI and reproducible setups rely on.

To remove old package artifacts that are no longer referenced by `plugins.lock`, run:

```bash
kasane plugin gc
```

`plugin gc` keeps artifacts referenced by the current `plugins.lock` and archived
lock generations, so recent `kasane plugin rollback` targets remain restorable.

To prune older rollback generations and let GC reclaim packages that are no longer
needed by those generations, run:

```bash
kasane plugin gc --prune-history --keep 10
```

If a recent `resolve`, `pin`, `update`, or install selected the wrong active set, you can
restore the previous `plugins.lock` generation with:

```bash
kasane plugin rollback
```

To inspect the archived generations before rolling back, run:

```bash
kasane plugin rollback --list
```

Current Kasane releases expect WASM plugins built against
`kasane:plugin@6.0.0`. If you are upgrading from an older build,
rebuild and reinstall those plugins before startup; older artifacts
will not load. The 1.0.0 ABI replaces the legacy `face` record with the
post-resolve `style` record (12 fields covering colour, weight, slant,
font features and variations, letter-spacing, decorations, plus blink /
reverse / dim) and renames `color` → `brush`; see
[plugin-development.md §Migrating to ABI 1.0.0](./plugin-development.md#migrating-to-abi-100).

External (non-bundled) plugins are installed via `kasane plugin install
path/to/my-plugin-x.y.z.kpk` — see Installing External Plugins below.

### Disabling Plugins

Installed packages and bundled plugins can be disabled by ID:

```kdl
plugins {
    disabled "some_plugin"
}
```

### Restricting Plugin Capabilities

WASM plugins run in a sandbox. You can further restrict their capabilities:

```kdl
plugins {
    deny_capabilities {
        untrusted_plugin "filesystem" "environment"
    }
}
```

## Writing Your Own

See [Plugin Development](plugin-development.md) for a step-by-step guide
and [Plugin API](plugin-api.md) for the API reference.
