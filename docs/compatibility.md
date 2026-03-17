# Compatibility

## Kakoune Version

Kasane requires **Kakoune 2024.12.09** or later (commit `3dd6f30d`). This version introduced the `cursor_pos` field in the `draw` method. The `widget_columns` parameter ([PR #5455](https://github.com/mawww/kakoune/pull/5455)) is used when available but is not required.

If your Kakoune is too old, Kasane will show:

```
Kasane requires Kakoune 2024.12.09 or later (commit 3dd6f30d).
Your Kakoune appears to use an older protocol (set_cursor method detected).
Please update Kakoune: https://github.com/mawww/kakoune
```

Check your version with:

```bash
kak -version
```

## What Works

- All kakrc configurations
- All Kakoune CLI arguments (`-c`, `-s`, `-l`, `-e`, `-f`, `-p`, `-d`, `-n`, `-ro`, etc.)
- All Kakoune keybindings
- All Kakoune plugins (kak-lsp, fzf.kak, plug.kak, auto-pairs.kak, kakoune-buffers, etc.)
- Existing session workflows (`kak -c`, `kak -l`, `kak -s`)
- Environment variables (`KAKOUNE_SESSION`, `KAKOUNE_CLIENT`, etc.)

## Terminal Environments

Kasane uses [crossterm](https://github.com/crossterm-rs/crossterm) for terminal I/O, providing broad terminal compatibility.

| Environment | Status |
|---|---|
| Direct terminal (kitty, alacritty, wezterm, foot, etc.) | Works |
| tmux / screen | Works |
| SSH | Works |
| macOS Terminal.app | Works |
| Windows Terminal (WSL) | Works |

## GPU Backend

The GPU backend (`--ui gui`) requires a local display server (X11, Wayland, or equivalent). It is not available over SSH.

## Known Differences

These are intentional improvements, not compatibility issues. See [What's Different](whats-different.md) for the full list.

- **Character width**: Kasane uses the `unicode-width` crate for character width calculation instead of the terminal's `wcwidth()`. This produces correct results for CJK characters and emoji. In rare cases, Kasane's width calculation may differ from your terminal's — if you notice misalignment, please report it.

- **Rendering**: Kasane renders independently of the terminal's built-in rendering. Flicker-free double-buffered output replaces Kakoune's direct terminal writes.

- **Clipboard**: System clipboard integration is built in via the `arboard` crate. Kakoune's native clipboard support (`%sh{xclip}` etc.) still works alongside it.

## Reporting Issues

If something doesn't work as expected:

1. **Check if it's a Kakoune issue**: try `kak` directly (without Kasane) to see if the same problem occurs
2. **Check the log**: set `KASANE_LOG=debug` and look at `~/.local/state/kasane/kasane.log`
3. **Report**: open an issue on [GitHub](https://github.com/Yus314/kasane/issues) with your Kasane version, Kakoune version, terminal, and OS

See [Troubleshooting](troubleshooting.md) for common issues and solutions.
