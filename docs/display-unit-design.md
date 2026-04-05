# Display Unit Model — Design Document

This document consolidates the design analysis for the Display Unit Model (P-040 through P-043). It serves as the single reference for theoretical foundations, concrete design, plugin API surface, extensibility strategy, and implementation plan.

For the authoritative requirements, see [requirements.md §3.5](./requirements.md). For the original architectural decision, see [decisions.md ADR-018](./decisions.md#adr-018-display-policy-layer-and-display-transformation--display-unit-model). For current semantics, see [semantics.md §10](./semantics.md#10-display-transformation-and-display-units).

## 1. Problem Statement

### 1.1 What Exists

The Display Transformation first slice (P-030, P-031, P-033, P-034) is complete:

- `DisplayDirective` — Fold, Hide, InsertAfter, InsertBefore
- `DisplayMap` — O(1) bidirectional line-level mapping (buffer line ↔ display line)
- `SourceMapping` — BufferLine, LineRange, None
- `InteractionPolicy` — Normal, ReadOnly, Skip
- `DirectiveSet` monoid + `resolve()` — multi-plugin deterministic composition
- Integration into paint, cursor, mouse input, scroll offset, and Salsa memoization

### 1.2 What Is Missing

The current system works at **line granularity only**. A `DisplayEntry` answers "what does display line N correspond to?" but provides no structure for:

- **Navigating through display-transformed content**: fold summaries, virtual text lines, and hidden regions create a display structure that diverges from buffer-line order. The input system has no model for moving through this structure.
- **Operating on display units**: fold summaries are ReadOnly, but there is no mechanism for "click to expand" or "Tab to next foldable region."
- **Plugin-defined navigation policies**: plugins cannot declare how their display units behave during navigation (skip, stop, activate).

These are the gaps that P-040 through P-043 address.

### 1.3 Driving Use Cases

From [requirements.md §4.2](./requirements.md):

| Use case | Required foundations | Key capability gap |
|----------|---------------------|--------------------|
| Code folding | P-030, P-040, P-020, P-010 | Fold summary is ReadOnly but has no expand/collapse interaction |
| Display line navigation | P-040, P-042, P-043 | No model for `gj`/`gk`-equivalent movement through transformed display |

## 2. Theoretical Foundations

### 2.1 Epistemological Status

In the World Model W = (T, I, Π, S) defined in [semantics.md §2.5](./semantics.md#25-world-model):

```
DisplayDirective ∈ Π                              — plugin policy output
DisplayMap       = f(T.lines, resolve(Π.directives)) — derived from Truth + Policy
DisplayUnit      = g(DisplayMap, LayoutResult)     — further derived
```

Display Units are **Derived State** (§3.3) with Policy-dependent derivation. They must not be confused with Truth, must be recomputed when directives change, and are frame-local.

### 2.2 Display Units as the Domain of the Inverse Projection

The projection function (§2.6):

```
Forward:   P(T, I, Π, S) = Ω        — state to presentation
Inverse:   ρ(Ω_P, event) = Intent   — event to intent
```

**Display Units are the formal domain of the inverse projection ρ.** Currently ρ for display-transformed content is ad-hoc (`mouse_to_kakoune()` + `InteractionPolicy` suppression). Display Units give ρ a well-defined codomain:

```
ρ₂'(event) = match display_unit_hit_test(event.coords) {
    Some(u) where π(u) = Normal      → BufferIntent(σ(u), event)
    Some(u) where π(u) = Boundary(a) → ActionIntent(u.owner, a)
    Some(u) where π(u) = Skip        → Suppressed
    None                              → Suppressed
}
```

where σ is the source mapping and π is the navigation policy.

### 2.3 Source Mapping as a Graded Partial Function

Source mapping σ is a partial function from Display Units to buffer regions:

```
σ: DisplayUnit → Option<BufferRegion>

BufferRegion = Line(usize)
             | Range(Range<usize>)
             | Span(usize, Range<usize>)    — future: sub-line
```

**Properties**: σ is not total (virtual text), not injective (folds represent ranges), and monotonic (INV-5: display order preserves buffer order).

**Strength determines interaction policy**:

| σ strength | Condition | Default InteractionPolicy |
|------------|-----------|---------------------------|
| Strong | `Some(Line(_))` — complete inverse exists | Normal |
| Weak | `Some(Range(_))` — many-to-one | ReadOnly |
| Partial | `Some(Span(_, _))` — sub-line only | ReadOnly |
| Absent | `None` — no buffer origin | Skip |

This formalizes §10.6: "Do not leave undefined operation results implicit." The σ strength classification makes the restriction explicit and machine-checkable (DU-INV-4).

### 2.4 InteractionPolicy vs NavigationPolicy: Orthogonal Concerns

InteractionPolicy and NavigationPolicy serve different phases and must remain orthogonal:

| Aspect | InteractionPolicy | NavigationPolicy |
|--------|-------------------|------------------|
| **Phase** | Rendering (paint, cursor suppression) | Input (navigation, activation) |
| **Determination** | Automatic from σ strength | Plugin-declared, with core defaults |
| **Mutability by plugins** | No — σ strength is a fact | Yes — plugins register policies |
| **Composition** | N/A (derived, not composed) | FirstWins (highest priority wins) |

A fold summary has `InteractionPolicy::ReadOnly` (because σ is weak) **and** `NavigationPolicy::Boundary(ToggleFold)` (because the fold plugin declares it). These are independent: ReadOnly prevents cursor placement, Boundary enables activation.

### 2.5 Navigation as Policy-Filtered Traversal

Display Units form a linearly ordered sequence U = (u₁, u₂, ..., uₙ) in display-line order. Navigation is movement through this sequence, filtered by NavigationPolicy π:

```
nav(u, Down) = first v ∈ U where v > u ∧ π(v) ∈ { Normal, Boundary(_) }
nav(u, Up)   = last  v ∈ U where v < u ∧ π(v) ∈ { Normal, Boundary(_) }
```

**Formal properties**:

- **NV-1 (Reachability)**: Any Normal unit is reachable from any other Normal unit via finite nav steps, unless a Boundary unit intervenes. Boundary units are intentional navigation barriers (fold summaries stop traversal).
- **NV-2 (Skip transparency)**: Skip units are invisible to navigation. Removing all Skip units from U yields equivalent navigation behavior.
- **NV-3 (Boundary stopping)**: A Boundary unit stops navigation in both directions. Activation (Activate operation) is required to proceed through it.

### 2.6 Collection/Resolution Separation

Following the established pattern in [compose.rs](../kasane-core/src/plugin/compose.rs):

```
Phase 1 (Collection): NavigationPolicySet — Composable
  Gathers policy declarations from all plugins.
  Sorted by (Reverse(priority), plugin_id).
  Commutative (order-independent after sorting).

Phase 2 (Resolution): resolve_navigation() — Non-compositional
  For each unit, selects the highest-priority policy.
  FirstWins semantics (same pattern as render_ornaments cursor_style).
```

This mirrors the DirectiveSet/resolve() separation: collection is algebraically well-behaved, resolution is priority-based conflict resolution.

## 3. Concrete Design

### 3.1 Types

```rust
/// Operable unit within the display-transformed UI.
pub struct DisplayUnit {
    pub id: DisplayUnitId,
    pub display_line: usize,
    pub role: SemanticRole,
    pub source: UnitSource,
    pub interaction: InteractionPolicy,
}

/// Stable identity derived from content, not from insertion order.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct DisplayUnitId(u64);

/// What this unit represents.
pub enum SemanticRole {
    BufferContent,
    FoldSummary,
    VirtualText,
    /// Plugin-defined role. Core applies Skip default; plugins override.
    Plugin(PluginTag, u32),
}

/// Source mapping to buffer coordinates.
pub enum UnitSource {
    /// Single buffer line (σ strength: Strong).
    Line(usize),
    /// Multiple buffer lines — fold (σ strength: Weak).
    LineRange(Range<usize>),
    /// No buffer origin — virtual text (σ strength: Absent).
    None,
    /// Sub-line range within a buffer line (σ strength: Partial). Future extension.
    Span { line: usize, byte_range: Range<usize> },
}
```

**Design decisions**:

- `SemanticRole` is a **closed enum with Plugin variant**. Core provides exhaustive default policies for known roles; Plugin variant is the escape hatch for plugin-defined roles.
- `UnitSource` is a **closed enum**. Plugins select from available variants but cannot invent new mapping kinds. This preserves DU-INV-4 (core can always determine σ strength).
- `DisplayUnitId` uses content-addressed hashing: `hash(source, role)`. Stable across frames when the underlying transform is stable.

### 3.2 NavigationPolicy

```rust
/// How navigation interacts with a display unit.
pub enum NavigationPolicy {
    /// Standard navigation — cursor can be placed here.
    Normal,
    /// Skip during navigation — invisible to nav traversal.
    Skip,
    /// Navigation stops here. Activation triggers the associated action.
    Boundary { action: NavigationAction },
}

pub enum NavigationAction {
    /// No special action (stop only).
    None,
    /// Toggle fold expansion/collapse.
    ToggleFold,
    /// Plugin-defined action.
    Plugin(PluginTag, u32),
}

/// Result of handling a navigation action.
pub enum ActionResult {
    /// Action handled, no further processing.
    Handled,
    /// Emit key sequence to Kakoune.
    SendKeys(String),
    /// Not applicable, continue default processing.
    Pass,
}
```

### 3.3 DisplayUnitMap

```rust
/// Query interface for display units. Built from DisplayMap after paint.
pub struct DisplayUnitMap {
    units: Vec<DisplayUnit>,
    /// display_line → unit index (1:1 in initial implementation).
    line_to_unit: Vec<usize>,
}
```

In the initial implementation, each display line maps to exactly one DisplayUnit (same granularity as DisplayEntry). Sub-line units are a future extension point — the type design accommodates them but the builder does not yet produce them.

### 3.4 Invariants

The following invariants are enforced in debug builds via `DisplayUnitMap::check_invariants()`:

```
DU-INV-1 (Completeness):
  Every display line maps to exactly one DisplayUnit.

DU-INV-2 (Source Consistency):
  Each unit's source is consistent with the underlying DisplayEntry's SourceMapping.

DU-INV-3 (Order Preservation):
  If unit A precedes unit B in display order and both have defined σ,
  then σ(A).start ≤ σ(B).start. (Inherited from DisplayMap INV-5.)

DU-INV-4 (Policy Soundness):
  σ strength determines interaction policy:
    Absent → interaction ∈ { ReadOnly, Skip }
    Weak   → interaction = ReadOnly
    Strong → interaction = Normal
  No unit with Absent source may have Normal interaction.

DU-INV-5 (Identity Degeneration):
  When DisplayMap.is_identity(), all units are BufferContent/Normal.
```

### 3.5 Relationship to Existing Abstractions

**DisplayEntry ↔ DisplayUnit**: DisplayUnitMap is a **derived view** built from DisplayMap + layout results. DisplayEntry remains the rendering primitive; DisplayUnit is the input/query interface. The two are never mixed.

**HitMap ↔ DisplayUnitMap**: HitMap handles element-level hit testing (overlays, menus, scroll bars). DisplayUnitMap handles buffer-area display-unit hit testing. They compose in series:

```
event → HitMap (ρ₁): element claims?
  → Yes: dispatch to element handler
  → No: DisplayUnitMap (ρ₂'): display unit dispatch
```

**Surface ↔ DisplayUnit**: Display Units exist within buffer surfaces. Each pane with a ClientBufferSurface has its own DisplayUnitMap. Plugin surfaces (non-buffer) do not have Display Units.

### 3.6 Temporal Coherence

Display Units are rebuilt each frame. Cross-frame identity relies on content-addressed `DisplayUnitId`:

```
BufferContent(line=42)         → hash(BufferContent, 42)
FoldSummary(range=10..20)      → hash(FoldSummary, 10, 20)
VirtualText(plugin, pos, hash) → hash(VirtualText, plugin, pos, content_hash)
```

**Focus transfer on fold toggle**:

| Transition | Focus target |
|------------|--------------|
| Expand fold(range) | First Normal unit in range (BufferContent(range.start)) |
| Collapse range | New FoldSummary(range) |

These transitions are deterministic and follow from the content-addressed identity scheme.

### 3.7 Default Frontend Semantics Preservation

```
Theorem T5-DU (Display Unit Default Sufficiency):
  When no plugins register display directives or navigation policies:
    1. DisplayMap = identity
    2. DisplayUnitMap is not constructed (fast path: None)
    3. Input handling falls through to existing mouse_to_kakoune() path
    4. Zero overhead added to the rendering or input hot paths
```

This extends T5 (Default Sufficiency) to the display unit layer.

## 4. Pipeline Integration

### 4.1 Construction Timing

```
render_pipeline_cached()
  → view_sections()           — DisplayMap built (existing)
  → into_element_and_layout() — Layout computed (existing)
  → walk_paint()              — CellGrid / DrawCommands produced (existing)
  → build_display_unit_map()  — ★ New: DisplayMap + layout → DisplayUnitMap
  → Stored in RenderResult
```

DisplayUnitMap is built **after** paint, not on every frame, and **only when DisplayMap is non-identity**. Identity maps skip construction entirely (DU-INV-5 / T5-DU).

### 4.2 Input Processing Integration

Current:
```
Key/Mouse event
  → Kasane bindings (C-w v/s/w etc.)
  → Forward to Kakoune via keys message
```

With display units:
```
Key/Mouse event
  → Kasane bindings (C-w v/s/w etc.)
  → HitMap dispatch (overlays, menus, interactive elements)
  → ★ Display unit dispatch (hit test / navigation)
  → Forward to Kakoune via keys message
```

The display unit dispatch step uses the **previous frame's** DisplayUnitMap, consistent with Axiom A6 (Input Coherence, ~16ms staleness bound).

### 4.3 Salsa Integration

DisplayUnitMap is **not** a Salsa tracked query in the initial implementation. Rationale:

- Used only in the input path (not rendering)
- Built from the previous frame's DisplayMap (already persisted on AppState)
- Input events are infrequent relative to frames; Salsa's dependency tracking overhead is unjustified
- Stored as `AppState.display_unit_map: Option<DisplayUnitMap>`

## 5. Plugin API

### 5.1 Native Plugin API

Two new registration methods on `HandlerRegistry`:

```rust
impl<S: Send + Sync + 'static> HandlerRegistry<S> {
    /// Declare navigation policy for display units produced by this plugin.
    pub fn on_navigation_policy(
        &mut self,
        handler: impl Fn(&S, &DisplayUnit) -> NavigationPolicy + Send + Sync + 'static,
    );

    /// Handle navigation actions on this plugin's display units.
    pub fn on_navigation_action(
        &mut self,
        handler: impl Fn(&mut S, &DisplayUnit, NavigationAction) -> ActionResult + Send + Sync + 'static,
    );
}
```

### 5.2 WASM WIT Extension

Phased WIT additions following the established backward-compatible pattern:

**Phase DU-WIT-1 (types):**
```wit
enum semantic-role { buffer-content, fold-summary, virtual-text, plugin-defined }

record display-unit-info {
    display-line: u32,
    role: semantic-role,
    plugin-tag: option<string>,
    role-id: u32,
}

enum navigation-policy { normal, skip, boundary }

record navigation-action-result {
    handled: bool,
    keys: option<string>,
    effects: option<runtime-effects>,
}
```

**Phase DU-WIT-2 (exports):**
```wit
navigation-policy: func(unit: display-unit-info) -> navigation-policy;
on-navigation-action: func(unit: display-unit-info, action-kind: u32) -> navigation-action-result;
```

**Phase DU-WIT-3 (host-state queries):**
```wit
get-display-unit-at-line: func(display-line: u32) -> option<display-unit-info>;
get-display-unit-count: func() -> u32;
```

**Backward compatibility**: Old plugins that do not implement the new exports fall back to `NavigationPolicy::default_for(role)` via the SDK's default generation. New `PluginCapabilities` bits (`NAVIGATION_POLICY = 1 << 21`, `NAVIGATION_ACTION = 1 << 22`) enable the host to skip WASM boundary crossings for non-participating plugins.

### 5.3 Default Navigation Policies

When no plugin registers a policy for a unit, the core applies:

| SemanticRole | Default NavigationPolicy |
|---|---|
| BufferContent | Normal |
| FoldSummary | Boundary { ToggleFold } |
| VirtualText | Skip |
| Plugin(_, _) | Skip |

### 5.4 Composition in compose.rs

| Extension Point | Monoid? | Commutative? | Type |
|---|---|---|---|
| Navigation policy | Yes | No | `FirstWins<NavigationPolicy>` |
| Navigation action | Yes | No | `FirstWins<ActionResult>` |

These follow established patterns (same structure as render_ornaments and handle_key).

## 6. Extensibility

### 6.1 Dimensions of Extensibility

| Dimension | Extensible? | Mechanism | Rationale |
|---|---|---|---|
| **Unit kinds** (SemanticRole) | Yes | `Plugin(tag, id)` variant | Plugins define custom roles |
| **Navigation policies** | Yes | `on_navigation_policy` registration | Plugins declare how their units behave |
| **Click/activation behavior** | Yes | `on_navigation_action` registration | Plugins handle actions on their units |
| **Source mapping kinds** | No | Closed enum (UnitSource) | Core must determine σ strength for DU-INV-4 |
| **InteractionPolicy override** | No | Derived from σ strength | Prevents undefined cursor placement |
| **Cross-plugin policy override** | Yes | FirstWins priority | Plugin B can override Plugin A's policy with higher priority |
| **resolve() customization** | No | Core-owned conflict resolution | Protects Composition Determinism (T4) |

### 6.2 Intentional Non-Extension Points

- **UnitSource**: Closed enum. Plugins select from available variants but cannot invent new mapping kinds. Opening this would break DU-INV-4 and make InteractionPolicy derivation impossible.
- **InteractionPolicy**: Not directly settable by plugins. Automatically derived from σ strength. Allowing plugins to set Normal on an Absent-source unit would produce undefined cursor behavior.
- **resolve()**: Core-owned. Plugin customization would break other plugins' expectations and violate T4.

### 6.3 Future Extension Points

- **Sub-line Display Units**: `UnitSource::Span` is defined in the type but not yet produced by the builder. Future plugins (clickable links, inline interactive regions) can leverage this when the builder is extended.
- **Plugin-defined extension points for navigation**: A fold plugin could define an extension point (`code-folding.nav-policy`) that other plugins contribute to via `on_extension`. This uses the existing extension point infrastructure and requires no new mechanism.
- **host-state queries for WASM plugins**: Tier 11 state accessors expose DisplayUnitMap to WASM plugins, enabling display-aware key handlers.

### 6.4 Use Case Feasibility

| Use case | Required new API | Existing API sufficient? |
|----------|-----------------|--------------------------|
| Code folding (expand/collapse) | `on_navigation_policy`, `on_navigation_action` | Plus existing `display_directives`, `annotate_line`, `handle_mouse` |
| Display line navigation (gj/gk) | `Command::NavigateDisplayUnit(dir)` | Plus existing `handle_key` |
| Clickable links | Sub-line hit test (future) | Initial: element-level `InteractiveId` + `handle_mouse` |
| Indent guides | None | Fully covered by existing `cell_decoration` |
| Code preview on fold hover | DU-WIT-3 host-state queries | Plus existing `observe_mouse`, `contribute_overlay` |

## 7. Kakoune Protocol Constraints

From [kakoune-protocol-constraints.md](./kakoune-protocol-constraints.md):

| Constraint | Impact on Display Units | Mitigation |
|---|---|---|
| No viewport position | Cannot actively query viewport start | `draw` message passively provides all visible lines |
| No command execution RPC | Cannot place cursor at arbitrary position | `keys` message for cursor movement (fire-and-forget) |
| No buffer content outside viewport | Cannot access content of off-screen folds | Folds operate on visible content only |

**Cursor movement precision**: For display unit navigation, fold toggle requires no cursor movement (plugin-side state change only). Virtual text skip requires computing the buffer-line delta and sending `j`/`k` keys. Arbitrary position targeting carries character-width divergence risk and is deferred from initial scope.

### 7.1 Selection-Oriented Model

Kakoune is fundamentally selection-oriented: every cursor is the end of a selection, and movement operations are defined as selection boundary changes. Display Unit navigation is inherently cursor-oriented ("move to the next unit"). This asymmetry produces specific interactions:

**Selections spanning fold regions**: When a selection range intersects a fold, the selection endpoints are preserved in Kakoune's internal state (they are Truth), but the middle portion becomes invisible on the display. This is a rendering concern — `display_to_buffer` maps the fold summary to `range.start`, and secondary cursors within the fold range are rendered at the fold summary line.

**Multi-cursor limitation**: Display Unit navigation targets the **primary cursor only**. Secondary cursor positions are heuristic (I-1, severity: degraded) and too imprecise for navigation target computation. The primary cursor is observed (`cursor_pos`) and therefore reliable.

**Selection semantics of navigation commands**: When display unit navigation sends keys to Kakoune (e.g., `j` for virtual text skip), the key's selection semantics (move vs. extend) are inherited from the key itself. `NavigateDisplayUnit(Down)` sends `j` (move); a future `NavigateDisplayUnit(Down, Extend)` variant would send `J` (extend). The initial implementation supports move only.

### 7.2 Fire-and-Forget

The `keys` message has no ACK ([kakoune-protocol-constraints.md §7](./kakoune-protocol-constraints.md)). After sending a navigation command:

- Kakoune may or may not have processed it
- The result becomes observable in the next `draw` message (1-2 frame delay)
- Additional navigation events during the gap operate on stale state

This is not a new problem — all existing key forwarding has the same property. For display unit navigation specifically:

- Fold toggle does not send keys to Kakoune (pure display policy change) — no fire-and-forget issue
- Virtual text skip sends simple `j`/`k` keys — same risk profile as normal key forwarding
- Complex cursor positioning is deferred from initial scope

### 7.3 Undo/Redo Interaction

When Kakoune performs undo (`u`), buffer content changes and the next `draw` delivers new lines. If a plugin's fold directive references lines that no longer exist (e.g., `Fold { range: 5..10 }` on a buffer that shrunk to 3 lines), `build()` silently ignores the out-of-bounds directive. The fold naturally disappears.

Fold state itself (which regions are folded) is plugin state (Π), not buffer truth (T). It is **not** subject to Kakoune's undo tree. This matches the behavior of all major editors — fold state is orthogonal to undo history.

## 8. Multi-Pane Considerations

### 8.1 Per-Pane Independence

Each pane runs an independent Kakoune client with its own `AppState`. Display transforms are per-rendering-pass, computed from the focused pane's state:

```
Pane A (file.rs, fold plugin active)  →  DisplayMap: non-identity, DisplayUnitMap: Some(...)
Pane B (file.rs, no fold plugin)      →  DisplayMap: identity, DisplayUnitMap: None
```

Even when two panes display the same file, fold states are independent because they are plugin policy (Π), not buffer truth (T).

### 8.2 Plugin State and Pane Context

Current plugin state is global (shared across panes). A code folding plugin that stores fold state in `S` faces a challenge: toggling a fold in Pane A would also affect Pane B if both use the same plugin instance.

**Resolution**: Plugins that need per-pane state should use `HashMap<SessionId, FoldState>` internally. The `display_directives()` method receives `AppView` which carries the current session context, allowing the plugin to return session-specific directives.

**Scope**: Per-pane display directive context (passing `PaneContext` to `display_directives()`, analogous to `contribute_to()`) is a future enhancement. The initial implementation computes display units for the focused pane only.

## 9. Scroll Interaction

### 9.1 Display Scroll Offset

`compute_display_scroll_offset()` ensures the cursor remains visible when virtual lines push the display line count beyond the viewport height. It uses the cursor's buffer line, maps it to a display line via `DisplayMap`, and computes the minimum scroll offset.

### 9.2 Fold Toggle and Scroll Position

When a fold is expanded, the display line count increases. The cursor position (in buffer coordinates) is unchanged, but its display-line position may shift:

```
Before (fold active, 10 display lines):
  cursor at buffer line 15 → display line 10, viewport fits

After (fold expanded, 20 display lines):
  cursor at buffer line 15 → display line 15, viewport may scroll
```

`compute_display_scroll_offset()` handles this automatically — the cursor remains visible. However, the user may perceive a scroll jump if the fold was above the cursor. A future enhancement could preserve the fold summary's screen position during toggle by biasing the scroll offset, but this is not required for initial correctness.

### 9.3 Scroll Granularity

Scroll operations (mouse wheel, smooth scroll) work in screen-line units. Display units do not change scroll granularity — a "scroll one line" still moves by one display line, whether that line is a buffer line, fold summary, or virtual text.

## 10. Soft Wrapping

Soft wrapping (one buffer line → N visual rows) is **within the theoretical scope** of the display unit model (`UnitSource::Span` can represent wrap segments) but **outside the initial implementation scope**.

Rationale:

1. Kakoune handles soft wrapping natively via the `wrap` option, including `gj`/`gk` movement
2. Kasane-independent wrapping risks divergence with Kakoune's viewport calculations
3. Adding wrap awareness requires fundamental changes to the paint loop (1 display line = 1 row assumption)
4. The type design does not prevent future integration

## 11. Comparison with Other Editors

| Aspect | VS Code | Neovim | Helix/Zed | Kasane |
|--------|---------|--------|-----------|--------|
| Fold definition | Extension (FoldingRangeProvider) | Core (foldmethod) | Core (treesitter) | Plugin (DisplayDirective) |
| Fold operation | Core-fixed | Core-fixed | Core-fixed | Plugin-definable (NavigationPolicy) |
| Virtual text | Extension (Decoration API) | Core (extmarks) | Core (inlays) | Plugin (DisplayDirective) |
| Virtual text tracking | Buffer-anchored | Extmark-tracked | Buffer-anchored | Display-only (re-evaluated per frame) |
| Navigation behavior | Core-fixed | Core-fixed | Core-fixed | Plugin-definable |
| Source mapping | Implicit | Extmark-based | Implicit | Explicit (σ + strength grading) |

Kasane's distinguishing characteristic is **plugin-definable navigation**. Other editors hardcode navigation behavior for folds and virtual text in the core. Kasane provides default policies (§5.3) but allows plugins to override them, enabling use cases that other editors cannot express (e.g., a fold that expands into a preview overlay on hover rather than inline expansion).

The trade-off: Kasane must provide richer default policies to match the out-of-box experience of editors with hardcoded behavior. The defaults in §5.3 are designed to match conventional fold/virtual-text behavior without any plugin policy registration.

## 12. Failure Modes and Defensive Design

### 12.1 Invalid Plugin Output

Plugins may return invalid directives. The existing defense-in-depth strategy applies to display units:

| Layer | Defense | Behavior on invalid input |
|-------|---------|---------------------------|
| `resolve()` | Conflict resolution | Overlapping folds → higher priority wins. Fold-hide overlap → fold removed |
| `build()` | Bounds checking | Out-of-range directives silently ignored |
| INV-1–INV-7 | Structural invariants | Debug-assert on violation; release builds rely on resolve()+build() preconditions |
| DU-INV-1–5 | Display unit invariants | Debug-assert; derived from already-validated DisplayMap |

### 12.2 NavigationPolicy Soundness Enforcement

Plugins declare navigation policies, but the core **filters** the result to enforce DU-INV-4:

```
resolve_policy(unit, plugin_policy):
  if unit.interaction = Skip and plugin_policy = Normal:
    → Skip  (reject: cannot place cursor on Absent-source unit)
  if plugin_policy = Boundary(_):
    → plugin_policy  (always allowed: Boundary does not place cursor)
  otherwise:
    → plugin_policy
```

This prevents a plugin from declaring Normal navigation on a virtual text line (which has no buffer position for cursor placement).

### 12.3 Stale DisplayUnitMap

The DisplayUnitMap from frame N-1 is used for input processing in frame N. If a plugin changes directives between frames:

- Hit test may identify a unit that no longer exists in the current frame's DisplayMap
- Navigation may compute a target based on a stale unit layout

This is the same staleness model as the existing `mouse_to_kakoune()` path (Axiom A6). The staleness bound is one frame (~16ms). In practice, the user cannot perceive a 16ms discrepancy between clicking and the display updating.

### 12.4 Plugin Panic During Navigation Action

If a plugin's `on_navigation_action` handler panics:

- Native plugins: panic propagates (Rust default). The plugin system should catch panics at the bridge boundary (existing pattern for other plugin callbacks)
- WASM plugins: the WASM runtime traps. The adapter catches the trap and returns `ActionResult::Pass`, falling through to default behavior

In both cases, the system degrades to the default navigation policy for that unit type.

## 13. Testing Strategy

### 13.1 Unit Test Hierarchy

```
Level 1: DisplayUnitMap construction
  — DisplayMap → DisplayUnitMap conversion satisfies DU-INV-*
  — Identity map → None fast path
  — Each directive combination (fold, hide, insert, mixed)

Level 2: Navigation computation
  — nav(u, Down/Up) skips Skip units, stops at Boundary
  — NV-1 (reachability), NV-2 (skip transparency)
  — Edge cases: all-Skip, first/last unit navigation

Level 3: Policy resolution
  — Default policies match §5.3
  — FirstWins composition: highest priority wins
  — DU-INV-4 enforcement: Normal rejected for Skip-interaction units

Level 4: Hit testing
  — screen (x, y) → correct DisplayUnit
  — Fold summary click → Boundary unit
  — Virtual text click → Skip unit (suppressed)
```

### 13.2 Property-Based Testing

Following existing proptest patterns (`trace_equivalence.rs`, `display/tests.rs`):

- **Invariant preservation**: Arbitrary directives (1-50 lines, random fold/hide/insert combinations) → resolve → build → DisplayUnitMap → all DU-INV-* hold
- **Navigation skip transparency (NV-2)**: For arbitrary unit sequences, navigation with Skip units present yields the same target as navigation with Skip units removed
- **Default sufficiency (T5-DU)**: Identity DisplayMap always produces None or all-Normal units

### 13.3 Integration Tests

End-to-end scenarios:

1. **Fold toggle roundtrip**: Plugin emits Fold → user clicks fold summary → ToggleFold dispatched → plugin removes directive → DisplayMap rebuilds → expanded lines visible
2. **Virtual text skip**: Plugin emits InsertAfter → user navigates down → virtual text line skipped → next Normal unit reached
3. **Stale map safety**: Plugin changes directives → same-frame input uses previous DisplayUnitMap → no crash, graceful degradation

## 14. Implementation Plan

### Phase DU-1: Types and Construction (P-040 minimal)

**Deliverables**:
- `DisplayUnit`, `DisplayUnitId`, `SemanticRole`, `UnitSource` type definitions
- `DisplayUnitMap::build(display_map, layout)` — construct from DisplayMap
- `AppState.display_unit_map: Option<DisplayUnitMap>` — persisted after render
- Identity DisplayMap → None (fast path)

**Completion criteria**: Unit tests verify DisplayMap → DisplayUnitMap construction correctness. All DU-INV-* invariants enforced in debug builds.

### Phase DU-2: Geometry and Hit Test (P-041 + P-042 first half)

**Deliverables**:
- Layout results annotate each unit with screen Rect
- `DisplayUnitMap::hit_test(x, y) -> Option<&DisplayUnit>`
- `mouse_to_kakoune()` extended: hit test → InteractionPolicy + NavigationPolicy dispatch

**Completion criteria**: Mouse clicks on fold summaries dispatch to plugin handlers.

### Phase DU-3: Navigation (P-042 second half)

**Deliverables**:
- `NavigationPolicy`, `NavigationAction`, `ActionResult` types
- Default policy mapping (§5.3)
- Event handler display unit navigation dispatch
- Fold toggle E2E: click → ToggleFold → directive removal → DisplayMap rebuild → UI update

**Completion criteria**: Fold expand/collapse works via click and keyboard activation.

### Phase DU-4: Plugin API and WIT (P-043)

**Deliverables**:
- `HandlerRegistry::on_navigation_policy()` / `on_navigation_action()`
- WIT extensions (DU-WIT-1 through DU-WIT-3)
- `PluginCapabilities::NAVIGATION_POLICY` / `NAVIGATION_ACTION` bits
- Proof artifact: code folding plugin with expand/collapse interaction

**Completion criteria**: WASM plugin defines navigation policy and handles activation actions.

## 10. Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| Cursor movement precision | Medium | Fold toggle needs no cursor movement. Virtual text skip uses buffer-line delta. Arbitrary positioning deferred |
| Frame delay (A6) | Low | Same 1-frame staleness model as existing DisplayMap input handling |
| Performance | Low | O(display_lines) construction; identity map skips entirely; input path only |
| P-032 (observed/policy separation) enforcement | Low | Orthogonal to display units; can be co-developed for design coherence |

## Related Documents

- [requirements.md §3.5](./requirements.md) — P-040 through P-043 requirements
- [semantics.md §10](./semantics.md) — Display Transformation and Display Units
- [decisions.md ADR-018](./decisions.md) — Display Policy Layer decision
- [kakoune-protocol-constraints.md §7–§8](./kakoune-protocol-constraints.md) — Protocol limitations affecting display units
- [roadmap.md §3.2](./roadmap.md) — Implementation status
