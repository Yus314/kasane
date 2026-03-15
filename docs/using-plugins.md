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

## Available Example Plugins

These plugins are bundled with Kasane and can be enabled via configuration:

| Plugin | ID | Description |
|---|---|---|
| Cursor Line | `cursor_line` | Highlight the current line with a background color |
| Color Preview | `color_preview` | Detect color codes in text and show interactive color preview |
| Selection Badge | `sel_badge` | Show selection count in the status bar when multiple cursors are active |
| Fuzzy Finder | `fuzzy_finder` | Fuzzy file finder overlay |

An example native plugin is also available at [examples/line-numbers/](../examples/line-numbers/).

## Enabling Bundled Plugins

Add plugin IDs to the `enabled` list in your configuration:

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

See [Plugin Development](plugin-development.md) for a guide to writing plugins, and [Plugin API](plugin-api.md) for the API reference.
