# Troubleshooting

## Kasane Crashed

Your Kakoune session is still running. Kasane is only the UI layer — the editor process continues independently.

**Reconnect:**

If you used a named session (`-c` or `-s`), Kasane shows the reconnect command in the crash output:

```
Your Kakoune session is still running.
Reconnect with: kasane -c <session_name>
```

If you don't know the session name:

```bash
kak -l          # List running sessions
kasane -c NAME  # Reconnect to a session
```

You can also reconnect with `kak -c NAME` directly (without Kasane).

## Display Looks Wrong

1. **Compare with kak directly** — run `kak file.txt` (without Kasane) to see if the same problem occurs. If it does, the issue is in Kakoune or your terminal, not Kasane.

2. **Character width misalignment** — Kasane uses the `unicode-width` crate instead of the terminal's `wcwidth()`. In rare cases this may produce different results. If you notice specific characters causing misalignment, please report them.

3. **Terminal compatibility** — Kasane uses crossterm for terminal I/O. Most modern terminals are supported. If you experience rendering issues, try a different terminal emulator to isolate the problem.

## Kakoune Version Mismatch

If you see:

```
Kasane requires Kakoune 2024.12.09 or later (commit 3dd6f30d).
Your Kakoune appears to use an older protocol (set_cursor method detected).
Please update Kakoune: https://github.com/mawww/kakoune
```

Your Kakoune is too old. Check your version:

```bash
kak -version
```

Update Kakoune to 2024.12.09 or later. See the [Kakoune installation guide](https://github.com/mawww/kakoune#building) for instructions.

## Performance Issues

Enable debug logging:

```bash
KASANE_LOG=debug kasane file.txt
```

Or set in configuration:

```toml
[log]
level = "debug"
```

Log files are written to:

```
~/.local/state/kasane/kasane.log
```

Or `$XDG_STATE_HOME/kasane/kasane.log` if `$XDG_STATE_HOME` is set.

## Plugin Issues

### Bundled plugins not loading

Bundled plugins require explicit opt-in. Add them to your config:

```toml
[plugins]
enabled = ["cursor_line", "color_preview"]
```

Then rebuild the active set:

```bash
kasane plugin resolve
```

### External plugin not loading

1. Check the package was installed through `kasane plugin install` or `kasane plugin dev`
2. Rebuild `plugins.lock`: `kasane plugin resolve`
3. Check the plugin is not in the `disabled` list
4. Run `kasane plugin doctor`
5. Remove stale package artifacts if needed: `kasane plugin gc`
6. Check the log for loading errors: `KASANE_LOG=info kasane file.txt`

## Reporting Bugs

Open an issue on [GitHub](https://github.com/Yus314/kasane/issues) with:

- Kasane version (`kasane --version`)
- Kakoune version (`kak -version`)
- Terminal emulator and version
- Operating system
- Steps to reproduce
- Relevant log output (`KASANE_LOG=debug`)
