# Using Plugins

## Kakoune Plugins

Your existing Kakoune plugins (kak-lsp, fzf.kak, plug.kak, auto-pairs.kak, etc.) work unchanged. They run inside Kakoune and are not affected by Kasane.

## Kasane Plugins

Kasane has its own plugin system for UI extensions. Kasane plugins can add visual elements, decorations, overlays, and input handling that Kakoune's shell-based plugins cannot.

The plugin API is extensible — plugins can:

- Add UI elements at named slots (gutters, status bar sections)
- Annotate individual lines (highlights, markers)
- Show floating overlays (pickers, tooltips)
- Transform existing elements (status bar customization)
- Handle keyboard and mouse input

## Included Example Plugins

Kasane includes several example plugins that demonstrate the plugin system's extension points. Their source is in [`examples/wasm/`](../examples/wasm/):

| Plugin | ID | Demonstrates |
|---|---|---|
| Cursor Line | `cursor_line` | Line annotation (`annotate_line_with_ctx`) |
| Color Preview | `color_preview` | Line annotation + overlay + mouse input |
| Selection Badge | `sel_badge` | Slot contribution (`contribute_to`) |
| Fuzzy Finder | `fuzzy_finder` | Overlay + key input + external process I/O |

A native plugin example is also available at [`examples/line-numbers/`](../examples/line-numbers/).

## Enabling Example Plugins

Example plugins are included in the binary but not loaded by default. Add plugin IDs to the `enabled` list in your configuration:

```toml
# ~/.config/kasane/config.toml
[plugins]
enabled = ["cursor_line", "color_preview"]
```

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

The quickest way to start is with `kasane plugin new`:

```bash
kasane plugin new my-plugin --template hello   # 4-line hello world
cd my-plugin
kasane plugin build        # Build for wasm32-wasip2
kasane plugin install      # Build, validate, and install
```

Other templates: `contribution` (default), `annotation`, `transform`, `overlay`, `process`. See [Plugin Development](plugin-development.md) for a full guide, and [Plugin API](plugin-api.md) for the API reference.
