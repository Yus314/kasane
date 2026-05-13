# `#[handler_table]` DSL — Phase γ-3.1 Specification

> **Status**: spec for γ-3.1 (Phase γ — Architectural Cleanup). Implementation
> lands in γ-3.2 (`kasane-macros`); rewrite of existing manual code lands in
> γ-3.3. This document is the contract that γ-3.2 implements and γ-3.3 is
> validated against.

## 1. Motivation

Adding a new plugin extension point today touches four mechanically-coupled
sites with no compiler-enforced consistency:

1. `kasane-core/src/plugin/handler_table.rs` — erased type alias + `HandlerTable`
   field + `HandlerTable::empty()` initializer + (optionally) capability
   inference + (optionally) transparency-flag tracking.
2. `kasane-core/src/plugin/handler_registry/<axis>.rs` — typed setter
   (`on_<name>` / `on_<name>_tier1` / `on_<name>_transparent`) + downcast
   wrapper using one of the four `register_*!` macros.
3. `kasane-core/src/plugin/plugin_bridge.rs` — `impl PluginBackend for
   PluginBridge` dispatch method that consumes the field and calls the closure.
4. `EXPECTED_HANDLER_NAMES` in `plugin_bridge.rs:1607-1644` plus the
   `AllHandlersPlugin::register` body in
   `exhaustive_handler_dispatch_coverage` — a runtime test that compares
   "what the bridge dispatches" against "what the registry can register"
   and panics on mismatch.

`.claude/rules/plugin-handlers.md` exists specifically because the
compiler catches **none** of the misalignments between (1)/(2) and (3): a
handler can be registered correctly but silently never invoked. The test
in (4) is the only line of defence.

The `#[handler_table]` DSL replaces the four sites with a single
declarative spec module from which the macro generates everything.
Successful generation **is** the consistency proof; the runtime test in
(4) becomes redundant and is deleted in γ-3.3.

### Goals

- One source of truth for every dispatch entry. Adding / removing /
  renaming a handler is a one-line edit.
- Compiler-enforced 4-way consistency: type alias, table field, registry
  setter, dispatch site cannot drift.
- Preserve the current public registration surface: every existing
  `on_*` / `declare_*` method on `HandlerRegistry<S>` must remain
  callable with the same signature so plugin authors are not affected.
- Preserve transparency / recovery / tier semantics (ADR-030 §Levels 3–5,
  ADR-044 effect tiers).

### Non-goals

- Changing `Plugin` trait surface, `PluginBackend` method shapes, or
  WIT export semantics.
- Auto-generating WASM-host shims (`kasane-wasm/src/plugin/*`). Those
  follow a different shape and stay manual.
- Hot-path optimisation (γ-4 owns `PluginId` interning and atom-vector
  scratch).

## 2. Vocabulary

| Term | Meaning |
|---|---|
| **Spec entry** | One DSL line; corresponds to one logical extension point. |
| **Shape** | One of four dispatch contracts (§3): `Lifecycle`, `Observer`, `Dispatcher`, `View`. Each shape selects a code-gen template. |
| **Modifier** | An attribute that decorates a `View` entry to alter storage / dispatch (`PerSlot`, `Prioritized`, `Unified`). |
| **Effect tier** | The narrowness of the side-effect type a Lifecycle entry accepts: `Tier1` (`KakouneSideEffects`, no process spawn), `Tier2` (`ProcessCapableEffects`, process spawn allowed), `Untyped` (`Effects`, no guarantee). |
| **Transparency variant** | The `_transparent` setter pair generated for handlers that can produce `Command`s, capturing ADR-030 Levels 3 / 5 compile-time non-write proofs. |
| **Config entry** | A non-handler spec line that sets a `HandlerTable` configuration field (interests, authorities, …). Generated separately; no dispatch site. |

## 3. The four dispatch shapes

Every handler in the codebase reduces to exactly one of these. The shape
determines (a) the erased function signature, (b) the downcast-and-box
template the registry setter uses, and (c) the bridge dispatch body.

### 3.1 `Lifecycle<E>`

> Mutates state **and** produces side effects.

```text
fn(&S, args…) -> (S, E)
```
where `E: Into<Effects>` (or a tier-narrowed alias — see §6).

**Generated:**
- Erased alias: `Box<dyn Fn(&dyn PluginState, args…) -> (Box<dyn PluginState>, Effects) + Send + Sync>`.
- Registry setter wraps via the existing `register_state_effect!` template:
  downcast `&S`, call handler, box new state, lift effects via `Into`.
- Bridge dispatch invokes the closure when present and forwards the result.

### 3.2 `Observer`

> Mutates state, produces no side effects.

```text
fn(&S, args…) -> S
```
or void: `fn(&S, args…)`.

**Generated:**
- Erased alias: `Box<dyn Fn(&dyn PluginState, args…) -> Box<dyn PluginState> + Send + Sync>` (or `… ()` for void).
- Registry setter wraps via `register_state_only!` (state return) or
  `register_void!` (no return).
- Bridge dispatch invokes the closure when present, replaces state.

### 3.3 `Dispatcher<C>`

> Optionally short-circuits with `(new_state, commands)` or yields control
> to the next handler / builtin.

```text
fn(&S, args…) -> Option<(S, C)>
```
where `C: Into<Vec<Command>>` (or transparent variant; see §6).

**Generated:**
- Erased alias: `Box<dyn Fn(&dyn PluginState, args…) -> Option<(Box<dyn PluginState>, Vec<Command>)> + Send + Sync>`.
- Registry setter downcasts, calls handler, threads the `Option` through;
  on `Some` boxes new state, lifts `C` to `Vec<Command>`.
- Bridge dispatch returns `None` if the field is empty; otherwise forwards
  the closure's `Option` outcome.

### 3.4 `View<Out>`

> Read-only query: pure projection of state into render-time data.

```text
fn(&S, args…) -> Out
```

**Generated (default storage):**
- Erased alias: `Box<dyn Fn(&dyn PluginState, args…) -> Out + Send + Sync>`.
- Registry setter is the trivial `register_view!` form (downcast, forward).
- Bridge dispatch returns the field's call result, or `Out::default()` /
  `None` / empty vec depending on `Out`'s shape.

`View` is the only shape that admits modifiers (§4) — Lifecycle / Observer
/ Dispatcher are always single-slot scalar fields.

## 4. View modifiers

The view-side handlers are the only ones whose storage geometry varies.
Three modifiers cover every existing case:

### 4.1 `PerSlot { key: K }`

Storage becomes `Vec<Entry<K, Out>>`. Each registration appends one entry;
dispatch iterates, matching by `key`. First match wins (existing
`contribute` / `gutter` semantics).

**Used by:** `contribute` (`SlotId`), `gutter` (`GutterSide`), `projection`
(`ProjectionDescriptor`).

### 4.2 `Prioritized`

Adds an `i16 priority` field to the entry struct. Combined with `PerSlot`
or used standalone.

**Used by:** `transform` (single entry, priority exposed via
`transform_priority()`), `gutter` (per-entry priority for ordering).

### 4.3 `Unified`

Marks a "monolithic" alternative whose registration suppresses the
decomposed handlers. The macro emits a `has_<name>()` predicate the
bridge consults to decide which dispatch path to take.

**Used by:** `annotate_line` (suppresses `gutter` / `background` /
`inline` / `virtual_text`), `unified_display` (suppresses `display` for
the same plugin).

Modifiers compose: `View<Out>` + `[PerSlot, Prioritized]` is legal
(`gutter` uses both).

## 5. DSL syntax

### 5.1 Surface form

The macro is **function-like** (`handler_table! { … }`), not an attribute.
Attribute macros require their input to parse as a syntactic Rust item,
which would reject the custom `handler …;` / `config …;` keywords; the
function-like form passes raw tokens through to our `syn::Parse`
implementation. The argument is a `pub mod NAME { … }` block.

```rust
kasane_macros::handler_table! {
    pub mod handler_table_spec {
        use super::*;

        // Lifecycle (default tier — Untyped)
        handler init(app: &AppView<'_>): Lifecycle<Effects>;

    // Lifecycle, tier-narrowed
    handler init: Lifecycle<KakouneSideEffects>(tier1, transparent);
    handler io_event(event: &IoEvent, app: &AppView<'_>):
        Lifecycle<ProcessCapableEffects>(tier2, transparent);

    // Observer (state-only)
    handler observe_key(key: &KeyEvent, app: &AppView<'_>): Observer;

    // Dispatcher
    handler key(key: &KeyEvent, app: &AppView<'_>): Dispatcher<Vec<Command>>(transparent);

    // View — plain
    handler overlay(app: &AppView<'_>, ctx: &OverlayContext):
        View<Option<OverlayContribution>>;

    // View — PerSlot
    handler contribute(app: &AppView<'_>, ctx: &ContributeContext):
        View<Option<Contribution>>(per_slot = SlotId);

    // View — Prioritized
    handler transform(target: &TransformTarget, app: &AppView<'_>, ctx: &TransformContext):
        View<ElementPatch>(prioritized, targets = Vec<TransformTarget>, full_fallback);

    // View — Unified
    handler annotate_line(line: usize, app: &AppView<'_>, ctx: &AnnotateContext):
        View<Option<LineAnnotation>>(
            unified,
            suppresses = [gutter, background, inline, virtual_text],
        );

        // Config (no dispatch site)
        config interests: DirtyFlags = DirtyFlags::ALL;
        config allows_process_spawn: bool = true;
        config display_priority: i16 = 0;
    }
}
```

### 5.2 Grammar (BNF)

```text
spec_module    = "mod" IDENT "{" item* "}"
item           = handler_decl | config_decl

handler_decl   = "handler" IDENT "(" arg_list? ")" ":" shape modifiers? ";"
shape          = "Lifecycle" "<" type ">"
               | "Observer"
               | "Dispatcher" "<" type ">"
               | "View" "<" type ">"
modifiers      = "(" modifier ("," modifier)* ")"
modifier       = "tier1" | "tier2"
               | "transparent"            -- emit _transparent setter pair
               | "per_slot" "=" type      -- View only
               | "prioritized"            -- View only
               | "unified"                -- View only
               | "suppresses" "=" "[" IDENT ("," IDENT)* "]"
               | "targets" "=" type       -- transform-style metadata
               | "full_fallback"          -- transform legacy path
               | "void"                   -- Observer-only: no return value
               | "default" "=" expr       -- View only: dispatch fallback

config_decl    = "config" IDENT ":" type ("=" expr)? ";"

arg_list       = arg ("," arg)*
arg            = IDENT ":" type
```

The signature for the closure body is reconstructed as
`(&S, <arg.types>) -> <shape return>`. Argument **names** are kept so the
generated bridge code reads naturally (`fn dispatch_init(&self, app: &AppView<'_>) { … }`).

### 5.3 Required vs. optional modifiers per shape

| Shape | Allowed modifiers |
|---|---|
| `Lifecycle<E>` | `tier1`, `tier2`, `transparent` |
| `Observer` | `void` |
| `Dispatcher<C>` | `transparent` |
| `View<Out>` | `per_slot`, `prioritized`, `unified`, `suppresses`, `targets`, `full_fallback`, `default` |

Mismatches (e.g. `transparent` on `View`) fail at macro expansion time
with a `compile_error!` cite-able to the offending DSL line. γ-3.2 ships
a `trybuild` suite covering each invalid combination.

## 6. Effect tiers and transparency

### 6.1 Effect tier alias resolution

When a Lifecycle entry's `E` parameter binds to one of the recognised
tier aliases, the registry setter signature uses the narrower bound:

| `E` | Setter bound | Setter suffix |
|---|---|---|
| `Effects` | `E: Into<Effects>` | `on_<name>` (default) |
| `KakouneSideEffects` | `E: Into<KakouneSideEffects>` | `on_<name>_tier1` |
| `ProcessCapableEffects` | `E: Into<ProcessCapableEffects>` | `on_<name>_tier2` |

Multiple tier aliases on one spec entry generate **multiple** setters
(one per alias). The current code-base has tier1 setters for `init`,
`session_ready`, `state_changed` and tier2 setters for `io_event`,
`update`. The DSL expresses this as a list:

```rust
handler init(app: &AppView<'_>): Lifecycle<Effects>(tier1);
// expands to: on_init (Effects bound), on_init_tier1 (KakouneSideEffects bound)
```

`tier1` and `tier2` are independent attributes — listing both on one
entry yields three setters. Defaulting to all three is intentionally not
auto-applied because most entries only use one or two tiers in practice.

### 6.2 Transparency variant generation

When `transparent` is set on a `Lifecycle` or `Dispatcher` entry, the
macro generates:

- The "normal" setter accepting `E: Into<Effects>` (or `C: Into<Vec<Command>>`).
- A second setter named `<base>_transparent` that requires
  `E: Into<Effects> + Transparency` (or the `Command` analogue) and sets
  the corresponding `TransparencyFlags` bit on registration.

The `TransparencyFlags` struct itself is generated from the union of all
spec entries that carry `transparent`. Adding a new transparent handler
adds its bit to the struct automatically; `is_all_input_transparent` /
`is_all_lifecycle_transparent` / `is_fully_transparent` are emitted as
`impl` items partitioned by an entry-level `category` attribute (input
vs. lifecycle).

> **Open question (γ-3.2 to resolve):** the category split currently
> hard-codes "input" vs "lifecycle" in the `TransparencyFlags` impl.
> Either (a) the DSL gains a `category = input | lifecycle` modifier
> per transparent entry, or (b) the macro infers the category from the
> shape (`Dispatcher` → input, `Lifecycle` → lifecycle). Recommendation:
> (b) — the current code already splits along this axis and a hand-coded
> override is not needed.

### 6.3 Recovery (Visual Faithfulness §10.2a)

`display`-family entries gain a fixed family of recovery variants:

```rust
handler display(app: &AppView<'_>): View<Vec<DisplayDirective>>(recovery);
```

The `recovery` modifier expands the registry surface to three setters per
spec entry:

- `on_<name>(handler)` → `DisplayRecoveryStatus::Unwitnessed`
- `on_<name>_safe(handler)` → handler must return `SafeDisplayDirective`,
  status = `NonDestructive`
- `on_<name>_witnessed(witness, handler)` → status = `Witnessed(witness)`

`define_projection` and `define_additive_projection` retain their
bespoke surface (they take both a descriptor and a handler and derive
recovery from the descriptor's `Structural` / `Additive` category) and
stay hand-written; the spec models them as a single `View(per_slot =
ProjectionDescriptor, recovery)` entry. As of γ-3.3c-3, the macro emits
recovery on the per-entry struct (`ProjectionEntry { key, handler,
recovery }`) instead of a table-level `<name>_recovery` field, so the
generated `on_projection(key, recovery, handler)` setter is a usable
lower-level building block over which `define_projection` wraps. The
**registry surface** stays bespoke (the descriptor-derived recovery
inference) but the **storage shape** is fully macro-generated.

## 7. Generated artefacts per spec entry

For a single handler entry, the macro emits six things:

1. **Erased type alias** in the generated `handler_table` module:
   ```rust
   pub(crate) type Erased<Name>Handler = Box<dyn Fn(...) -> ... + Send + Sync>;
   ```
2. **`HandlerTable` field** with appropriate storage (`Option`, `Vec`, with
   metadata struct for PerSlot/Prioritized).
3. **`HandlerTable::empty()` initializer line** for the new field.
4. **`HandlerRegistry::on_<name>` setter** (plus tier / transparent / safe /
   witnessed variants as modifiers dictate).
5. **Bridge dispatch method** in `impl PluginBackend for PluginBridge`,
   matching the existing method shape of the corresponding
   `PluginBackend` trait method.
6. **`EXPECTED_HANDLER_NAMES` const entry** — the macro's coverage const
   replaces the hand-written one and `exhaustive_handler_dispatch_coverage`
   is deleted (the macro structurally guarantees what the test asserted).

For Config entries the macro emits only (2) + (3) + a `declare_<name>`
setter on `HandlerRegistry`.

## 8. Handler enumeration (canonical entries)

This table is the empirical ground truth γ-3.2 implements against. Every
row must round-trip to a working DSL line. Source columns are
file:line locators in the current tree (commit `201eb803`).

### 8.1 Lifecycle entries (state + effects)

| Spec name | Args | Tier(s) | Transparent? | Source field |
|---|---|---|---|---|
| `init` | `&AppView` | `tier1` | yes | `init_handler` (`handler_table.rs:55`) |
| `session_ready` | `&AppView` | `tier1` | yes | `session_ready_handler:57` |
| `state_changed` | `&AppView, DirtyFlags` | `tier1` | yes | `state_changed_handler:59` |
| `io_event` | `&IoEvent, &AppView` | `tier2` | yes | `io_event_handler:64` |
| `update` | `&mut dyn Any, &AppView` | `tier2` | yes | `update_handler:88` |
| `command_error` | `&PluginErrorEvent, &AppView` | (untyped) | yes | `command_error_handler:98` |
| `subscription` | `&str, &[ChannelValue], &AppView` | (untyped) | yes | `subscription_handler:118` |
| `key_middleware` | `&KeyEvent, &AppView` | (returns `KeyHandleResult`) | yes (KeyHandleResult carries `Transparency`) | `key_middleware_handler:139` |
| `key_pre_dispatch` | `&KeyEvent, &AppView` | (returns `KeyPreDispatchResult`) | yes | `key_pre_dispatch_handler:186` |
| `mouse_pre_dispatch` | `&MouseEvent, &AppView` | (returns `MousePreDispatchResult`) | yes | `mouse_pre_dispatch_handler:195` |
| `text_input_pre_dispatch` | `&str, &AppView` | (returns `TextInputPreDispatchResult`) | yes | `text_input_pre_dispatch_handler:204` |
| `mouse_fallback` | `&MouseEvent, i32, &AppView` | (returns `(state, Option<Vec<Command>>)`) | yes | `mouse_fallback_handler:213` |
| `action` | `&str, &KeyEvent, &AppView` | (returns `KeyResponse`) | no (KeyResponse-bound) | `action_handler:226` |
| `navigation_action` | `&DisplayUnit, NavigationAction` | (returns `ActionResult`) | no | `navigation_action_handler:318` |
| `virtual_edit` | `&VirtualEditContext, &AppView` | (commands only) | no | `virtual_edit_handler:325` |
| `buffer_edit_intercept` | `&BufferEdit, &AppView` | (returns `BufferEditVerdict`) | no | `buffer_edit_intercept_handler:342` |
| `process_task` | `&ProcessTaskResult, &AppView` | `tier2` | yes (placeholder; no `_transparent` setter exists) | `process_tasks: Vec<ProcessTaskEntry>` (§8.5; **carve-out — see §9.5**) |

`key_middleware` / `*_pre_dispatch` / `mouse_fallback` / `action` /
`navigation_action` / `virtual_edit` / `buffer_edit_intercept` follow the
Lifecycle shape but their effect type is structured (not a plain `E:
Into<Effects>`). The DSL models them as `Lifecycle<KeyHandleResult>`,
`Lifecycle<KeyPreDispatchResult>`, etc. — i.e. the second-element type is
free, not constrained to `Effects`. The shape's contract is "produces
`(new_state, T)`", which is what the existing dispatch already does.

### 8.2 Observer entries (state-only)

| Spec name | Args | Source field |
|---|---|---|
| `workspace_changed` | `&WorkspaceQuery` | `workspace_changed_handler:69` |
| `workspace_restore` | `&serde_json::Value` | `workspace_restore_handler:73` |
| `observe_key` | `&KeyEvent, &AppView` | `observe_key_handler:144` |
| `observe_text_input` | `&str, &AppView` | `observe_text_input_handler:146` |
| `observe_mouse` | `&MouseEvent, &AppView` | `observe_mouse_handler:148` |
| `observe_drop` | `&DropEvent, &AppView` | `observe_drop_handler:165` |
| `shutdown` | (none) | `shutdown_handler:83` (`void`) |

`subscribe` (the per-value typed handler — distinct from §8.1
`subscription` which is per-topic batched) is also Observer-shaped but
stored as `Vec<SubscribeEntry>` keyed by `TopicId`. Modeled as
`Observer(per_slot = TopicId)` with the macro adapting the entry struct.

### 8.3 Dispatcher entries

| Spec name | Args | Transparent? | Source field |
|---|---|---|---|
| `key` | `&KeyEvent, &AppView` | yes | `key_handler:130` |
| `text_input` | `&str, &AppView` | yes | `text_input_handler:150` |
| `handle_mouse` | `&MouseEvent, InteractiveId, &AppView` | yes | `handle_mouse_handler:155` |
| `handle_drop` | `&DropEvent, InteractiveId, &AppView` | yes | `handle_drop_handler:167` |
| `default_scroll` | `DefaultScrollCandidate, &AppView` | no (returns `ScrollPolicyResult`) | `default_scroll_handler:177` |
| `paint_inline_box` | `u64, &AppView` | n/a (returns `Option<Element>`) | `inline_box_paint_handler:361` (treated as Dispatcher because Option-shaped) |

`paint_inline_box` is borderline View vs Dispatcher. The current bridge
dispatches by checking `Some(element)` to paint, `None` to fall through
to the placeholder — that's the Dispatcher contract. Modeled as
`Dispatcher<()>` with `Out = Option<Element>` would conflate; the
recommendation is to keep `paint_inline_box` as `View<Option<Element>>`
because it has no `Vec<Command>` analogue. γ-3.2 adopts the View
classification and the bridge dispatches the same way.

### 8.4 View entries

| Spec name | Args | Out | Modifiers | Source field |
|---|---|---|---|---|
| `contribute` | `&AppView, &ContributeContext` | `Option<Contribution>` | `per_slot = SlotId` | `contribute_handlers:585` |
| `contribute_any` | `&SlotId, &AppView, &ContributeContext` | `Option<Contribution>` | — | `contribute_any_handler:586` |
| `transform` | `&TransformTarget, &AppView, &TransformContext` | `ElementPatch` | `prioritized, targets = Vec<TransformTarget>, full_fallback` | `transform_handler:587` |
| `gutter` | `usize, &AppView, &AnnotateContext` | `Option<Element>` | `per_slot = GutterSide, prioritized` | `gutter_handlers:588` |
| `background` | `usize, &AppView, &AnnotateContext` | `Option<BackgroundLayer>` | — | `background_handler:589` |
| `inline` | `usize, &AppView, &AnnotateContext` | `Option<InlineDecoration>` | — | `inline_handler:590` |
| `virtual_text` | `usize, &AppView, &AnnotateContext` | `Vec<VirtualTextItem>` | `default = vec![]` | `virtual_text_handler:591` |
| `annotate_line` | `usize, &AppView, &AnnotateContext` | `Option<LineAnnotation>` | `unified, suppresses = [gutter, background, inline, virtual_text]` | `annotate_line_handler:598` |
| `overlay` | `&AppView, &OverlayContext` | `Option<OverlayContribution>` | — | `overlay_handler:599` |
| `display` | `&AppView` | `Vec<DisplayDirective>` | `recovery` | `display_handler:600` |
| `unified_display` | `&AppView` | `Vec<DisplayDirective>` | `unified, recovery, suppresses = [display]` | `unified_display_handler:604` |
| `projection` | `&AppView` | `Vec<DisplayDirective>` | `per_slot = ProjectionDescriptor, recovery` (carve-out: setter manual) | `projection_entries:601` |
| `content_annotation` | `&AppView, &AnnotateContext` | `Vec<ContentAnnotation>` | `default = vec![]` | `content_annotation_handler:602` |
| `render_ornament` | `&AppView, &RenderOrnamentContext` | `OrnamentBatch` | `default = OrnamentBatch::default()` | `render_ornament_handler:603` |
| `menu_transform` | `&[Atom], usize, bool, &AppView` | `Option<Vec<Atom>>` | — | `menu_transform_handler:605` |
| `menu_renderer` | `&AppView, &PluginView` | `Option<Overlay>` | — | `menu_renderer_handler:611` |
| `info_renderer` | `&AppView, &[Rect], &PluginView` | `Option<Vec<Overlay>>` | — | `info_renderer_handler:612` |
| `display_scroll_offset` | `usize, usize, usize, &AppView` | `Option<usize>` | — | `display_scroll_offset_handler:608` |
| `navigation_policy` | `&DisplayUnit` | `NavigationPolicy` | `default = NavigationPolicy::Normal` | `navigation_policy_handler:316` |
| `paint_inline_box` | `u64, &AppView` | `Option<Element>` | — | `inline_box_paint_handler:361` |
| `workspace_save` | (none) | `Option<serde_json::Value>` | — | `workspace_save_handler:71` |
| `persist_state` | (none) | `Option<Vec<u8>>` | — | `persist_state_handler:79` |
| `restore_state` | `&[u8]` | `bool` | — | `restore_state_handler:81` |
| `state_hash` | (none) | `u64` | — | `state_hash_handler:660` (manual setter — see §8.6) |
| `key_map_builder` | (none) | `CompiledKeyMap` | — | `key_map_builder:580` |
| `group_refresh` | `&AppView, &mut CompiledKeyMap` | `()` (`void`) | — | `group_refresh_handler:582` |
| `surfaces` | (none) | `Vec<Box<dyn Surface>>` | `default = vec![]` | `surfaces_factory:635` |
| `lenses` | (none, no state arg) | `Vec<Arc<dyn Lens>>` | `default = vec![], stateless` | `lenses_factory:674` |

`workspace_restore`, `persist_state`, `restore_state`,
`workspace_save` are partly View / partly Observer — the current
codebase boxes them behind erased fns that return `Option`-typed bytes
or replace state. They classify cleanly as View (the result drives
external persistence; no `Effects`). `restore_state` returns `bool`
(success flag) — Observer-shaped semantically but no `S` produced; the
DSL models it as `View<bool>` because the registry already passes
`(state, bytes) -> S` and the bridge box-replaces.

### 8.5 Vec-storage entries (PerSlot variants of any shape)

The macro treats Vec storage as a `per_slot = K` modifier on whatever
shape the entry already has. Three current entries use this:

| Spec name | Shape | Key | Source field |
|---|---|---|---|
| `process_task` | `Lifecycle<ProcessCapableEffects>(tier2)` (carve-out §9.5 — Vec keyed by `&'static str` plus `ProcessTaskSpec` + `streaming` payload doesn't generalize) | `&'static str` + `ProcessTaskSpec` (registration time) | `process_tasks:632` |
| `publish` | `View<Option<ChannelValue>>` | `TopicId` | `publishers:628` |
| `subscribe` | `Observer` | `TopicId` | `subscribers:629` |

### 8.6 Config entries

| Spec name | Type | Default | Source field |
|---|---|---|---|
| `interests` | `DirtyFlags` | `DirtyFlags::ALL` | `interests:677` |
| `transparency` | `TransparencyFlags` | `default` (generated) | `transparency:680` |
| `<name>_recovery` | `DisplayRecoveryStatus` (one per `recovery`-marked entry) | `NotRegistered` | `display_recovery` / `unified_display_recovery` (singletons; per-entry on `ProjectionEntry` for the projection carve-out) |
| `authorities` | `PluginAuthorities` | `empty` | `authorities:664` |
| `allows_process_spawn` | `bool` | `true` | `allows_process_spawn:641` |
| `display_priority` | `i16` | `0` | `display_priority:669` |
| `key_map` | `Option<CompiledKeyMap>` | `None` | `key_map:579` |
| `workspace_request` | `Option<Placement>` | `None` | `workspace_request:636` |
| `capabilities_override` | `Option<PluginCapabilities>` | `None` | `capabilities_override:647` |
| `capability_descriptor_override` | `Option<CapabilityDescriptor>` | `None` | `capability_descriptor_override:652` |
| `state_hash` | `Option<Box<dyn Fn() -> u64>>` | `None` | `state_hash_handler:660` |
| `suppressed_builtins` | `HashSet<BuiltinTarget>` | `empty` | `suppressed_builtins:686` |

`transparency` is spec-internal: the macro generates the struct
definition by collecting the `transparent` modifiers from handler
entries and emits the `is_*` query methods as part of (3.5) above. The
`recovery` modifier expands to one `<name>_recovery: DisplayRecoveryStatus`
field per marked entry on `HandlerTable` directly (no wrapper struct);
the predicate `is_display_recoverable` ANDs every field's
`is_visually_faithful()` and is wired by the macro. Plugin code names
neither — both surface only through the registry's setter side effects.

### 8.7 Total entry count

- Lifecycle: 16 (spec; `process_task` carved out per §9.5)
- Observer: 8
- Dispatcher: 5
- View: 28 (spec; `projection` setter carved out per §9.1)
- Config: 12
- Carve-out (hand-authored alongside generated members): 5 (§9)

**69 spec entries** when each canonical handler counts once (57 +
config 12). The
roadmap's "22 entries" estimate is a stale undercount; γ-3.2 is sized
for ~70 entries, and the spec module body is on the order of 250 lines
of DSL (averaging 3–4 lines per entry including modifiers and arg
list). The roadmap row will be updated when γ-3.1 lands to reflect
this number; the LoC target (`handler_table.rs` 990 → ~50) refers to
the **post-macro** footprint, which is what matters.

> The undercount stems from γ-3.1's program-open notes pre-dating ADR-044
> (effect tiers), ADR-035 (BDT/shadow-cursor handlers), and the ADR-031
> Phase-10 inline-box work — each added ~3-7 entries. The shape geometry
> assumed for the DSL still holds.

## 9. The carve-out list

Some entries do not round-trip cleanly through the macro and stay
hand-authored:

1. **`define_projection` / `define_additive_projection`**. They take both
   a `ProjectionDescriptor` and a handler closure plus a recovery hint;
   the surface is bespoke. The DSL declares the *storage* (PerSlot Vec
   keyed by descriptor) but the setters live in `handler_registry/render.rs`
   alongside the macro-generated members.
2. **Key-map setup** (`on_key_map`). The single `on_key_map` setter
   composes three internal fields (`key_map`, `key_map_builder`,
   `action_handler`, `group_refresh_handler`) from a `KeyMapBuilder`
   closure. The DSL declares the underlying fields; the
   `on_key_map` orchestrator is hand-written.
3. **`on_state_changed_for(DirtyFlags::…, …)` filter sugar**. This is a
   convenience wrapper over `on_state_changed_tier1` — generated from
   the Lifecycle entry by the macro, no separate spec line needed.
4. **`on_transform_full` companion** (`full_fallback` modifier). The
   full-rewrite path's signature mentions `TransformSubject`, a type that
   does not appear elsewhere in the spec model and would force a
   one-off branch in the macro for `transform`'s sake alone. The macro
   generates the entry-struct storage (with `priority + targets +
   handler` fields and an externally-populatable `full_handler:
   Option<…>` companion) but rejects the `full_fallback` modifier with
   a carve-out diagnostic. γ-3.3 hand-writes the `on_transform_full`
   setter alongside the macro-generated members, mirroring carve-outs
   (1) and (2).
5. **`on_process_task_tier2` / `on_process_task_streaming_tier2`**. The
   storage `process_tasks: Vec<ProcessTaskEntry { name, spec, handler,
   streaming, transparent }>` is a Vec-of-metadata-Lifecycle shape: the
   key is a `&'static str`, the registration carries a `ProcessTaskSpec`
   plus a `streaming` flag, and the handler closure is keyed and
   accumulated rather than singleton or per_slot-keyed-by-type. The
   macro's per_slot path is single-key + handler [+priority]; extending
   it to support arbitrary registration-time payload + per-entry flags
   for the sake of one entry would compromise the path's clarity. The
   spec carries no entry; the storage and setters live entirely in
   `handler_table.rs` + `handler_registry/lifecycle.rs` alongside the
   macro-generated members. (γ-3.3b-4 closure note.)

The carve-outs are documented inline in the spec module so γ-3.3's
"replace existing manual code" pass knows what to leave alone.

## 10. Migration path (γ-3.2 → γ-3.3)

γ-3.2 lands the macro and a parallel spec module **without** removing
hand-written code. Two test gates protect the cut-over:

1. The macro-generated `EXPECTED_HANDLER_NAMES_AUTO` and the existing
   hand-written `EXPECTED_HANDLER_NAMES` must agree (asserted in a test
   that runs as part of γ-3.2's PR).
2. A type-equality check (`assert_type_eq!(generated::HandlerTable,
   manual::HandlerTable)` via `static_assertions`) confirms the macro
   output is structurally identical.

γ-3.3 then deletes the manual definitions in the order:
`handler_table.rs` body → `handler_registry/<axis>.rs` typed setters →
`plugin_bridge.rs` dispatch methods → `EXPECTED_HANDLER_NAMES` const +
`exhaustive_handler_dispatch_coverage` test.

`.claude/rules/plugin-handlers.md` 4-place-sync rule is removed in the
same commit that deletes the test (the rule's reason for existence is
the silent-failure window the test covers).

## 11. Open questions for γ-3.2

Items the spec deliberately leaves under-specified for the
implementation phase to settle with concrete ergonomic feedback:

- **Spec module location.** Either `kasane-core/src/plugin/handler_table_spec.rs`
  (close to consumers) or a dedicated `kasane-core/src/plugin/spec/mod.rs`
  with a sibling `tests.rs`. Recommendation: the former; one file is
  enough for ~70 entries.
- **Macro crate placement.** `kasane-macros` exists today and hosts
  `#[kasane::plugin]` / `#[kasane::component]`. `#[handler_table]` joins
  it; no new crate needed.
- **Argument-name collision with `state` / `app`.** The macro reserves
  `state` (always the first arg, name fixed) and `app` for the AppView
  reference where applicable. Spec entries should not name args `state`.
- **Spec re-export of types.** The DSL block lives inside a `mod
  handler_table_spec` so `use super::*` brings in `Effects`, `AppView`,
  `Element`, etc. Currently 30+ types — the recommendation is one
  blanket `use super::*;` at module top rather than per-entry imports.
- **`#[doc]` propagation.** Plugin authors read rustdoc on
  `HandlerRegistry::on_*`. The DSL needs a way to attach doc comments
  per spec entry that the macro forwards to the generated setter.
  Proposal: standard `///` doc comments on each `handler` line are
  picked up via `attrs` and re-emitted on the setter signature.

These are scoped to γ-3.2 and do not change the spec contract above.

## 12. Acceptance checklist for γ-3.1 (this document)

- [x] §3 enumerates the four shapes with semantics, generated alias, and
      bridge dispatch description.
- [x] §4 enumerates the three View modifiers and their composition rules.
- [x] §5 gives the surface form and a complete grammar.
- [x] §6 specifies effect-tier alias resolution and transparency variant
      generation.
- [x] §7 lists every artefact the macro emits per entry.
- [x] §8 enumerates every current handler with shape + modifiers +
      file:line locator (the empirical ground truth).
- [x] §9 lists the carve-outs that remain hand-written.
- [x] §10 specifies the parallel-implementation cut-over discipline.
- [x] §11 surfaces deferred design decisions for γ-3.2.

## 13. References

- `.claude/rules/plugin-handlers.md` — the 4-place sync rule this DSL
  retires.
- `docs/decisions.md` ADR-030 — Plugin Transparency (Levels 3–5).
- `docs/decisions.md` ADR-044 — Tier hierarchy / effect typing.
- `docs/decisions.md` ADR-048 — `PluginBackend` extinction (γ-3 is the
  follow-on to that program; the dispatch surface this DSL drives is
  the post-extinction `impl PluginBackend for PluginBridge`).
- `docs/roadmap.md` Phase γ-3 row — the line items γ-3.1 / γ-3.2 / γ-3.3
  this spec lives under.
