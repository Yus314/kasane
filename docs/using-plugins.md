# Using Plugins

## Kakoune Plugins

Your existing Kakoune plugins (kak-lsp, fzf.kak, plug.kak, auto-pairs.kak, etc.) work unchanged. They run inside Kakoune and are not affected by Kasane.

## Kasane Plugins

Kasane has its own plugin system for UI extensions. Kasane plugins can add visual elements, decorations, overlays, and input handling that Kakoune's shell-based plugins cannot.

The plugin API provides extension points for UI contributions, line annotations, overlays, transforms, input handling, and more. For the full list, see [plugin-api.md §1.2](./plugin-api.md#12-choosing-a-mechanism).

## Bundled WASM Plugins

Kasane embeds a small set of WASM plugins in the binary. Their source is in
[`examples/wasm/`](../examples/wasm/):

| Plugin | ID | Default | Demonstrates |
|---|---|---|---|
| Cursor Line | `cursor_line` | Off | Line annotation (`annotate_line_with_ctx`) |
| Color Preview | `color_preview` | Off | Line annotation + overlay + mouse input |
| Selection Badge | `sel_badge` | Off | Slot contribution (`contribute_to`) |
| Fuzzy Finder | `fuzzy_finder` | Off | Overlay + key input + external process I/O |
| Pane Manager | `pane_manager` | **On** | Workspace authority + pane split/focus commands |
| Smooth Scroll | `smooth_scroll` | Off | Default wheel scroll policy (`handle_default_scroll`) |

A native plugin example is also available at [`examples/line-numbers/`](../examples/line-numbers/).

## Enabling Bundled Plugins

Most bundled WASM plugins are not loaded by default. Add plugin IDs to the `enabled`
list in your configuration. `pane_manager` is the exception — it loads automatically
unless explicitly disabled.

```kdl
// ~/.config/kasane/kasane.kdl
plugins {
    enabled "cursor_line" "color_preview"
}
```

## Additional Source Examples

Some plugin examples are provided as source only and must be built and
installed as external plugins before use.

| Plugin | ID | Demonstrates | Source |
|---|---|---|---|
| Prompt Highlight | `prompt_highlight` | Element transform (`transform`) | [`examples/wasm/prompt-highlight/`](../examples/wasm/prompt-highlight/) |
| Session UI | `session_ui` | Slot contribution + overlay + session commands | [`examples/wasm/session-ui/`](../examples/wasm/session-ui/) |
| Image Preview | `image_preview` | Image element display (GPU backend) | [`examples/wasm/image-preview/`](../examples/wasm/image-preview/) |

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
`kasane:plugin@1.1.0`. If you are upgrading from an older build,
rebuild and reinstall those plugins before startup; older artifacts
will not load. The 1.0.0 ABI replaces the legacy `face` record with the
post-resolve `style` record (12 fields covering colour, weight, slant,
font features and variations, letter-spacing, decorations, plus blink /
reverse / dim) and renames `color` → `brush`; see
[plugin-development.md §Migrating to ABI 1.0.0](./plugin-development.md#migrating-to-abi-100).

For example, `prompt_highlight` is not embedded in the binary. Build and install
the WASM from [`examples/wasm/prompt-highlight/`](../examples/wasm/prompt-highlight/)
if you want to enable it.

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
