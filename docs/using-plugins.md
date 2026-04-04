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

A native plugin example is also available at [`examples/line-numbers/`](../examples/line-numbers/).

## Enabling Bundled Plugins

Most bundled WASM plugins are not loaded by default. Add plugin IDs to the `enabled`
list in your configuration. `pane_manager` is the exception — it loads automatically
unless explicitly disabled.

```toml
# ~/.config/kasane/config.toml
[plugins]
enabled = ["cursor_line", "color_preview"]
```

## Additional Source Examples

Some plugin examples are provided as source only and must be built and
installed as external plugins before use.

| Plugin | ID | Demonstrates | Source |
|---|---|---|---|
| Prompt Highlight | `prompt_highlight` | Element transform (`transform`) | [`examples/wasm/prompt-highlight/`](../examples/wasm/prompt-highlight/) |
| Session UI | `session_ui` | Slot contribution + overlay + session commands | [`examples/wasm/session-ui/`](../examples/wasm/session-ui/) |
| Smooth Scroll | `smooth_scroll` | Default wheel scroll policy (`handle_default_scroll`) | [`examples/wasm/smooth-scroll/`](../examples/wasm/smooth-scroll/) |
| Image Preview | `image_preview` | Image element display (GPU backend) | [`examples/wasm/image-preview/`](../examples/wasm/image-preview/) |

## Installing External Plugins

### WASM Plugins

Place `.wasm` files in the plugins directory:

```
~/.local/share/kasane/plugins/
```

Or, if `$XDG_DATA_HOME` is set:

```
$XDG_DATA_HOME/kasane/plugins/
```

Kasane automatically discovers and loads `.wasm` files from this directory on startup. Disable auto-discovery with:

```toml
[plugins]
auto_discover = false
```

Current Kasane releases expect WASM plugins built against
`kasane:plugin@0.25.0`. If you are upgrading from an older build,
rebuild and reinstall those plugins before startup; older artifacts
will not load.

For example, `smooth_scroll` is not embedded in the binary. Build and install
the WASM from [`examples/wasm/smooth-scroll/`](../examples/wasm/smooth-scroll/)
if you want to enable it.

### Disabling Plugins

Discovered plugins can be disabled by ID:

```toml
[plugins]
disabled = ["some_plugin"]
```

### Restricting Plugin Capabilities

WASM plugins run in a sandbox. You can further restrict their capabilities:

```toml
[plugins.deny_capabilities]
untrusted_plugin = ["filesystem", "environment"]
```

## Writing Your Own

See [Plugin Development](plugin-development.md) for a step-by-step guide
and [Plugin API](plugin-api.md) for the API reference.
