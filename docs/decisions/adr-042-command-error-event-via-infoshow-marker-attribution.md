# ADR-042: `command-error-event` via `info_show` Marker Attribution

**Status:** Decided (2026-05-11). Landed in `178eeedd` (Phase A) +
`858581db` (Phase B Step 1) + `cfc13952` (Phase B Step 2) +
`4eb241ca` (Phase B Step 3).

**Tracked in:** [Issue #90](https://github.com/Yus314/kasane/issues/90) (closed)

**Context:**

When a plugin emits a Kakoune command via `SendKeys` or `EvalCommand`
and Kakoune rejects it (parse error, unknown option, runtime error),
the error currently surfaces in the Kakoune echo area but the plugin
has no observability — plugin state proceeds as if the command had
succeeded. Sprout dogfooding (Issue #81) hit this concretely:
`declare-user-mode -override sprout` was rejected by Kakoune, and the
eleven subsequent `map global sprout …` commands chained inside the
same `evaluate-commands` block silently failed.

A plugin-observable error event is the only way to close this
silent-failure class.

**The protocol-level constraint:**

The `kak -ui json` channel is unidirectional from Kakoune's
perspective: Kakoune emits **UI commands** (`draw`, `info_show`,
`menu_*`, etc.) and consumes **input** (`keys`). There is no native
`trigger-user-hook` propagation, no `error` method, no back-channel
for hook firings. Any plugin error event must therefore travel from
Kakoune to kasane disguised as a UI command.

**Decision:**

Plugin error observability is implemented as **host-side command
wrapping + `info_show` marker attribution**:

1. The host wraps every plugin-originated command emission as:

   ```
   try %[ <original-cmd> ] catch %[
       info -title '__kasane_plugin_error__' %{
           <plugin-id>
           %val{error}
       }
   ]
   ```

   Wrapping is **opt-in** per plugin via a manifest flag (e.g.
   `[handlers] command-error-observability = true`), keeping the
   default zero-overhead.

2. The host's `info_show` JSON-RPC handler
   (`kasane-core/src/protocol/parse.rs:205-220`) recognises the
   reserved title `__kasane_plugin_error__`, parses the content as
   two newline-separated fields (plugin-id, error message),
   **suppresses** the actual info popup, and routes the parsed
   payload to a host-side plugin-error dispatcher.

3. The dispatcher invokes a new WIT export
   `on-command-error-effects: func(state, error: command-error) ->
   runtime-effects` on the originating plugin (default: no-op
   handler in SDK). The dispatch is **async** — fires on the next
   event loop tick to avoid re-entrancy with the failing emission.

**Empirical validation (2026-05-11):**

Drove `kak -ui json` standalone with:

```
kak -ui json -n -e "try %[ unknown-command ] \
    catch %[ info -title __kasane_plugin_error__ \
        %{ sprout%val{error} } ]"
```

Observed JSON-RPC output:

```json
{"method":"info_show","params":[
  [{"contents":"__kasane_plugin_error__", "face":{…}}],
  [[{"contents":"sprout"}],
   [{"contents":"1:2: 'unknown-command': no such command"}]],
  {"line":0,"column":0}, {…}, "prompt"]}
```

This validates: (a) `try…catch` reliably fires the `info_show`
emission; (b) the title field is delivered verbatim as the first
atom's `contents`, suitable for marker detection; (c) `%val{error}`
substitution carries Kakoune's structured error message (line,
column, command, reason); (d) `%sh{ printf … }` cleanly encodes
plugin-id alongside the error.

**Rationale:**

- **Opt-in wrapping** is mandatory. Wrapping doubles the bytes-on-the-wire per
  plugin command and adds Kakoune-side parse overhead. The vast majority
  of plugin emissions are well-formed (`SendKeys` for navigation, etc.) and
  have no error to observe. Opt-in keeps default overhead at zero.

- **Marker via `-title`** is the cleanest disguise. `info_show` is fired
  per-event (not per-frame like `draw_status`), so the matching overhead
  is bounded. The title field is a plain string under plugin control.

- **Suppression in the parser** keeps the marker out of the user-visible
  UI. The end-user never sees a `__kasane_plugin_error__` popup; the
  marker is purely a transport detail.

- **Async dispatch** matches existing event handlers
  (`on_state_changed_effects`). Synchronous would risk re-entrancy if
  the error handler itself emitted Kakoune commands.

- **No new Kakoune-side requirements**. The catch-info pattern works on
  any Kakoune ≥ 2026.04.12 (kasane's minimum supported version, see
  `kasane-core/src/protocol/parse.rs:232-237`).

**Alternatives considered:**

1. **`draw_status` marker via `echo -markup '{kasane-error}…'`.**
   Rejected: `draw_status` fires per-frame for status-line state, not
   per-event. Marker-matching overhead would scale with frame rate.

2. **`set-register` side-channel + polling.** Rejected: polling is an
   anti-pattern in an event-driven loop; introduces latency.

3. **Patching Kakoune to add a native error event.** Rejected:
   out-of-scope; kasane targets unmodified upstream Kakoune.

4. **Wrap-always (no opt-in).** Rejected: overhead on every plugin
   command without consent. The opt-in flag is cheap to add.

5. **Synchronous dispatch.** Rejected: re-entrancy with the failing
   emission path is hard to reason about. Async on next tick is
   structurally simpler.

**Open design questions:**

- **Plugin-id quoting**: when the host wraps the command, the plugin-id
  is embedded as a literal inside `%{ … }`. Plugin-ids are kebab-case
  snake-case identifiers and don't contain `}` — should be safe, but
  the wrapper must reject ids that contain `%`, `{`, or `}` defensively.

- **Original command echo**: should `command-error` carry the failing
  command body for diagnostics? Useful but adds bytes. Suggest: yes,
  capped at 200 chars, truncated with `…`.

- **Error rate-limiting**: a malformed plugin could emit failing
  commands every frame, flooding the dispatcher. Suggest: cap to 10
  errors/sec per plugin; subsequent errors merge into one summary
  event.

**Implementation phasing:**

This ADR's full landing requires WIT 4.0.0. Phase the work:

- **Phase A (host-internal, no WIT change)**: outbound wrapping behind
  opt-in flag, inbound recognition, **debug-log only** plugin
  attribution. Validates the end-to-end protocol with zero plugin-API
  commitment. Can land any time without ABI impact.

- **Phase B (WIT 4.0.0)**: adds `command-error` record and
  `on-command-error-effects` export. Bundled with ADR-041 for a
  single 3.0 → 4.0 plugin rebuild.

- **Phase C (SDK)**: convenience helper `kasane_plugin_sdk::on_command_error!`
  macro for the common case of "log + surface in a status badge".

**Implications:**

- Phase A is host-only; no plugin-author work needed. Plugins opt in
  via the manifest flag and observe diagnostics in the host log.
- Phase B is an ABI 4.0 break, coordinated with ADR-041.
- Phase C is purely additive to the SDK; no ABI impact.
- Existing examples remain compatible without changes.
- The `__kasane_plugin_error__` title becomes a reserved name; plugin
  authors must not use it for their own `info` popups. Document in
  `docs/abi-versioning.md`.
