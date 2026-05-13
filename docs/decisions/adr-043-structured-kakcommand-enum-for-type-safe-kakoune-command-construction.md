# ADR-043: Structured `KakCommand` enum for type-safe Kakoune command construction

**Status:** Decided (2026-05-11). Lands in this commit.

**Tracked in:** [Issue #94](https://github.com/Yus314/kasane/issues/94)
(the last open child of the sprout-dogfooding tracker
[#81](https://github.com/Yus314/kasane/issues/81)).

**Context:**

The sprout dogfooding tracker has produced three layered approaches to
Kakoune-command construction from WASM plugins:

| Layer | Surfaced in | Catches |
|---|---|---|
| Raw string + `keys::command` | original SDK | nothing |
| `kak::*` builders (#87) | ADR-pre-041 SDK | bad flag combinations encoded per command |
| `kak_lint!` macro (#93) | ADR-043 precursor | typo'd / missing flags at compile time |

Each layer addresses a slice of the construction-error space but none
gives a **single composable Rust value** that:

1. Can be inspected programmatically (e.g., a plugin builds a command,
   another plugin transforms it, a third writes it out).
2. Is provably correct by construction at the type level — no string
   concatenation, no escaping bugs, no possible "unknown flag".
3. Centralizes Kakoune syntax / quoting knowledge in one renderer.

The current `kak::*` helpers return `String`, so composition above a
helper boundary collapses back to string-substring inspection — which
is what `kak_lint!` re-validates. The two layers do not naturally
strengthen each other: a `kak::*` helper's output is already trusted by
the linter (no need to re-check), but a plugin that hand-composes a
non-trivial command body cannot use the helper output as input to its
own programmatic transform.

**Decision:**

Add a `kasane_plugin_sdk::kak_cmd` module exposing a `KakCommand` enum
that **represents** (rather than emits) a Kakoune command. Each variant
wraps a per-command args struct with builder methods for optional
flags. A `KakCommand::render(&self) -> String` method produces the
canonical Kakoune syntax string with all escaping applied. Existing
`kak::*` string helpers and `kak_lint!` remain — they cover different
niches.

```rust
use kasane_plugin_sdk::kak_cmd::{KakCommand, DeclareUserMode, DefineCommand, Map, Scope};

let setup: Vec<KakCommand> = vec![
    DeclareUserMode::new("sprout").into(),
    DefineCommand::new("bump", "increment-counter")
        .override_existing()
        .docstring("bump the sprout counter")
        .into(),
    Map::new(Scope::Global, "sprout", "b", ":bump<ret>")
        .docstring("bump")
        .into(),
];

// Render and ship as effects:
Effects::with_kak_commands(setup)
```

Initial catalog matches the `kak_lint!` catalog (12 commands):
`declare-user-mode`, `define-command`, `map`, `declare-option`,
`set-option`, `unset-option`, `evaluate-commands`, `hook`, `alias`,
`echo`, `info`, `try`. Adding a command is additive — one variant + one
args struct.

**Shape:**

Each command gets:
1. A `pub struct CmdName` with positional fields in the constructor and
   builder methods for each flag. Builder methods consume `self` and
   return `Self`, enabling chained construction.
2. A `KakCommand` variant `CmdName(CmdName)`.
3. A `From<CmdName> for KakCommand` impl so the args struct can be
   collected into a `Vec<KakCommand>` via `.into()`.
4. Rendering lives on `KakCommand`. The per-command `render()` is a
   private impl detail; the public API is `KakCommand::render()`.

```rust
pub struct DeclareUserMode {
    pub name: String,
    pub hidden: bool,
    pub docstring: Option<String>,
    // NOTE: no `override` field — Kakoune does not accept it on this
    // command (#81 regression bug).
}

impl DeclareUserMode {
    pub fn new(name: impl Into<String>) -> Self { … }
    pub fn hidden(mut self) -> Self { self.hidden = true; self }
    pub fn docstring(mut self, s: impl Into<String>) -> Self { … }
}

pub enum KakCommand {
    DeclareUserMode(DeclareUserMode),
    DefineCommand(DefineCommand),
    Map(Map),
    DeclareOption(DeclareOption),
    SetOption(SetOption),
    UnsetOption(UnsetOption),
    EvaluateCommands(EvaluateCommands),
    Hook(Hook),
    Alias(Alias),
    Echo(Echo),
    Info(Info),
    Try(Box<KakCommand>),
}

impl KakCommand {
    pub fn render(&self) -> String { … }
}
```

`EvaluateCommands` and `Try` take inner `KakCommand`s so compound
commands round-trip cleanly:

```rust
KakCommand::Try(Box::new(
    DeclareUserMode::new("sprout").into()
)).render()
// => "try %[ declare-user-mode 'sprout' ]"
```

**Integration with `Effects`:**

Add a convenience method:

```rust
impl Effects {
    pub fn with_kak_commands(cmds: impl IntoIterator<Item = KakCommand>) -> Self {
        let commands = cmds.into_iter()
            .map(|c| Command::EvalCommand(c.render()))
            .collect();
        Self { redraw: 0, commands, scroll_plans: vec![] }
    }
}
```

Renders each `KakCommand` as its own `EvalCommand` (matching
`kakoune_setup_effects!`'s failure-isolation policy: one bad command
does not block the rest).

**Relationship to `kak_lint!`:**

`KakCommand::render()`'s output is by construction valid against
`kak_lint!`'s catalog — the renderer cannot produce an unknown flag.
This is testable: each variant has a unit test that lints its rendered
output. The two layers compose:

- New code uses `KakCommand` directly (most rigorous).
- Existing code that builds Kakoune strings by hand uses `kak_lint!`
  to validate at compile time.
- The `kak::*` string helpers remain for one-liner cases where
  introducing a `Vec<KakCommand>` round-trip is friction.

**Rationale:**

A structured form unlocks three concrete capabilities that the existing
string-based layers cannot provide:

1. **Programmatic composition.** A plugin can synthesize a command from
   inputs (e.g., a fuzzy-finder that builds a `define-command` body
   dynamically), inspect it, and pass it to another plugin via the pub/
   sub channel. Today this is round-tripped through opaque strings.
2. **Per-variant tests.** Unit-testing a string-builder requires
   substring matches against an expected canonical form. With
   `KakCommand`, tests inspect typed fields directly:
   `assert!(matches!(cmd, KakCommand::DeclareUserMode(d) if !d.hidden))`.
3. **Future ABI surfacing.** If a later WIT bump wants to ship typed
   Kakoune commands over the wire (skipping per-string parsing on the
   host), `KakCommand` is the source-of-truth shape. ADR scope is the
   SDK-only form; a wire-level variant is a separate decision.

**Alternatives considered:**

1. **Single trait `KakRender { fn render(&self) -> String }` instead of
   an enum.** Plugins implement the trait for their own types.
   Rejected: defeats centralized inspection (a plugin can't introspect
   a third-party `dyn KakRender` to know it's a `declare-user-mode`).
   The enum is the boundary that lets programmatic transforms see
   what they're holding.

2. **Procedural-macro DSL: `kak! { declare-user-mode { hidden, name:
   "sprout" } }`.** Rejected for now: the builder-pattern form is
   already terse and integrates with Rust tooling (rust-analyzer
   completion on struct fields, hover docs on each builder method).
   A macro could be added later as a thin sugar over the same enum.

3. **Make builder methods take `&mut self`.** Rejected: the
   consume-and-return form chains in expression position, which is
   what `Vec<…>` literals need. The mutable form forces statements:
   `let mut c = DefineCommand::new(…); c.override_existing(); c.into()`.

4. **Skip `KakCommand::Try` and expose `try` only via a helper method
   `KakCommand::wrapped_in_try(self)`.** Rejected: `try` is a
   first-class Kakoune command (a plugin can produce a literal
   `try` for catch-blocks unrelated to idempotency). Exposing it as
   a variant preserves the option to wrap *any* `KakCommand`.

5. **Defer to a future ABI bump shipping wire-level `KakCommand`.**
   Rejected: SDK-side ergonomics deliver immediate value to plugin
   authors (especially sprout, which prompted this tracker), and the
   wire-level decision is independent — it can re-use the same enum
   shape later if the ABI team chooses.

**Implications:**

- Pure SDK addition: no WIT change, no ABI bump, no plugin recompile
  required. The new module is opt-in.
- `kasane-plugin-sdk` minor-bumps when this lands.
- `kak::*` string helpers are not deprecated. The cookbook will
  document the choice between layers (one-liner string vs.
  programmatic enum).
- `kak_lint!` becomes redundant for users who go all-in on
  `KakCommand`. It remains essential for code that still composes
  command strings by hand.
- Per-variant rendering invariants are tested by feeding the rendered
  output through `kak_lint!` — guarantees zero false-negative
  divergence between the two layers.
