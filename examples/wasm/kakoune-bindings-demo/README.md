# kakoune-bindings-demo

Demonstrates the **register-Kakoune-APIs-at-startup** pattern: option
declaration, command definition, user-mode declaration, and key mapping.

## What it does

After loading, in any Kakoune session:

1. Run `:enter-user-mode demo`
2. Press `b` to bump the counter
3. Press `?` to inspect the counter value via `:info`

## Patterns shown

| Pattern | Helper | Idempotency idiom |
|---|---|---|
| Option | `kak::declare_option` | natural (no-op on re-declare with same kind) |
| Command | `kak::define_command` | `-override` flag (allowed by Kakoune) |
| User mode | `kak::declare_user_mode` | `try %[ ... ]` wrapper (no `-override` flag) |
| Map | `kak::map` | not idempotent — re-running adds duplicates (intentional) |

## The Kakoune flag asymmetry gotcha

A frequent foot-gun: `define-command` accepts `-override`, but
`declare-user-mode` does **not**. Re-using `-override` blindly produces
`unknown option '-override'` and aborts whatever `evaluate-commands`
block contains it — silently failing every subsequent command in that
block.

The `kak::*` helpers in `kasane_plugin_sdk` encode the correct idiom
for each command and prevent this class of error at the API level.

## Failure isolation

`kakoune_setup_effects![...]` sends each command as its own
`Command::EvalCommand`. One bad command surfaces as a Kakoune echo-area
error but does **not** block the rest from registering.

## Command-error observability (ADR-042 Phase B)

This plugin opts in via `[handlers] command_error_observability = true`
in `kasane-plugin.toml`. The host wraps every `Command::EvalCommand`
this plugin emits with a Kakoune `try…catch`; failures fire an
attributed `info_show` (suppressed from the UI) that the host parses
and dispatches back to this plugin's `on-command-error-effects`
export. The SDK provides a default no-op for the export — plugins
override it to surface failures (e.g. as a status badge or info
popup).
