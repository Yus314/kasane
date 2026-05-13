# ADR-010: Compiler-Driven Optimization — Svelte-like Two-Layer Rendering

**Status:** Partially implemented, partially under continued research

**Context:**

Svelte's design philosophy is summarized as "the framework is not shipped. The compiler is shipped." It compiles components into efficient imperative code that surgically updates the DOM, eliminating virtual DOM diffing costs. The question was how to incorporate this philosophy into kasane's declarative UI plan (ADR-009).

**Analysis: TEA vs Svelte-like Reactivity**

TEA's model is "State change → view() rebuilds entire Element tree → layout → paint → CellGrid → diff → terminal." Svelte's model is "State change → compiler-generated code directly updates only the changed nodes."

kasane's Element tree is extremely small at 20-50 nodes, orders of magnitude different from web UI's thousands of nodes. Performance analysis shows view() + layout() cost totals ~2 μs (0.1% of frame time), with the bottleneck being terminal I/O (~1,500 μs, 75%). The problem Svelte aims to solve (virtual DOM diffing cost) does not exist in kasane.

Furthermore, Rust's ownership model naturally aligns with TEA (`&State` for view, `&mut State` for update). Component-local state is fundamentally incompatible with Rust's borrowing rules; importing Signals/Runes would result in a storm of `Cell<T>` / `RefCell<T>` / `Rc<T>`, undermining Rust's static safety.

**Decision:** Maintain TEA as the runtime model, and adopt a "two-layer rendering" approach that incrementally introduces optimizations achievable through proc macros (`#[kasane::component]`) and policy-driven cache / patches.

**Adopted:**

| Concept | Implementation Approach | Timing |
|---------|------------------------|--------|
| Compile-time dependency analysis | Proc macro analyzes view() AST to identify input parameters each Element depends on | Phase 2 |
| Static layout cache | Calculate layout once for parts whose structure does not depend on input | Phase 2 |
| Fine-grained update code generation | Per-Element dependency tracking to directly update only changed cells in CellGrid | Phase 2 |
| Two-layer rendering model | Compiled path (proc macro generated) + interpreter path (generic Element tree) | Phase 2 |

**Not adopted:**

| Concept | Reason |
|---------|--------|
| Component-local state | Incompatible with Rust's borrowing rules. TEA's central state management is optimal for Rust |
| Signals / Runes | Undermines Rust's static safety with `Cell<T>` / `RefCell<T>`. TEA's `&T` / `&mut T` is superior |
| JSX / template syntax | Poor IDE support, unclear error messages. Rust's builder pattern is better for type checking and completion |
| `$derived` (derived state) | Manual is sufficient. Formalizing it greatly increases proc macro complexity |

**Two-layer rendering model:**

```
                  +---------------------+
                  |   Declarative API   |  ← what plugin authors interact with
                  |  (Element, view())  |
                  +------+--------------+
                         |
             +-----------+----------+
             v                      v
  +------------------+   +----------------------+
  | Compiled path    |   | Interpreter path     |
  | (proc macro gen) |   | (generic Element     |
  |                  |   |  tree)               |
  | Static structure |   | Element → layout()   |
  |  → direct        |   |  → paint() → CellGrid|
  |   CellGrid update|   |                      |
  +------------------+   +----------------------+
    ^ #[kasane::component]    ^ Plugin::contribute()
    ^ static parts of         ^ dynamic Slot/Decorator/Replacement
      core_view
```

- **Compiled path**: Parts that `#[kasane::component]` can statically analyze. Updates CellGrid directly, bypassing the Element tree. Same structure as Svelte compiling results to imperative code
- **Interpreter path**: Parts where plugins dynamically provide Elements. The full Element → layout → paint path. Always present as a correctness guarantee
- **Fallback safety**: Code written without `#[kasane::component]` runs via the interpreter path. Optimization is opt-in; the interpreter path guarantees correctness

**Rationale:**
- Svelte's true benefit is not "changing the runtime model" but the philosophy of "letting the compiler do the work"
- Positioned as a natural extension of ADR-009's proc macro plan (9-2)
- Achieves the same duality as Svelte: maintaining a declarative API while making execution code imperative
- Comes into its own as plugins increase from Phase 2 onward. Only design considerations in Phase 1, no implementation

**2026-03 note:** The "two-layer rendering" in this section is the name for the overall vision. Stages 1-4 below were originally implemented with manual caching (ViewCache, PaintPatch, etc.), but have since been **superseded by Salsa incremental computation** (ADR-020). `SceneCache` remains as a GPU-path auxiliary cache. The historical implementation record is preserved below for reference.

### Implementation Record

> **Superseded by ADR-020.** ViewCache, ComponentCache, LayoutCache, and PaintPatch have been removed. Salsa is now the sole caching layer for element tree construction and layout. SceneCache remains for GPU-path DrawCommand reuse.

Original 4 stages: (1) DirtyFlags-based view memoization, (2) verified dependency tracking via `#[kasane::component(deps(...))]`, (3) SceneCache for DrawCommand-level caching, (4) compiled PaintPatch with StatusBarPatch / MenuSelectionPatch / CursorPatch.

### Implementation Status (Historical)

#### Stage 1: DirtyFlags-Based View Memoization — Superseded by Salsa

| Metric | Value |
|---|---|
| view() cost | 5.0 us (0 plugins) / 10.4 us (10 plugins) |
| Implementation | ~~ViewCache, ComponentCache\<T\>~~ → Salsa tracked functions. DirtyFlags u16, MENU→MENU_STRUCTURE+MENU_SELECTION split retained |
| Result | view() sections skipped entirely when Salsa inputs are unchanged (PartialEq early-cutoff) |

#### Stage 2: Verified Dependency Tracking — Superseded by Salsa

| Metric | Value |
|---|---|
| Implementation | ~~`#[kasane::component(deps(FLAG, ...))]` proc macro, AST-based field access analysis, FIELD_FLAG_MAP~~ → Salsa structural dependency tracking |
| Note | `#[kasane::component]` now validates purity only (return type + no &mut). Deps/field-access analysis removed |

#### Stage 3: SceneCache (DrawCommand-Level Caching) — Active (GPU only)

| Metric | Value |
|---|---|
| Implementation | Per-section DrawCommand caching (base, menu, info) |
| Invalidation | DirtyFlags-based: BUFFER\|STATUS\|OPTIONS→base, MENU→menu, INFO→info |
| GPU benefit | Cursor-only frames reuse cached scene (0 us pipeline work) |
| Cold/Warm ratio | 22.8 μs cold → 7.0 μs warm (3.3x speedup) |

#### Stage 4: Compiled Paint Patches — Superseded by Salsa

| Metric | Value |
|---|---|
| ~~StatusBarPatch~~ | Removed — Salsa handles status section memoization |
| ~~MenuSelectionPatch~~ | Removed — Salsa handles menu section memoization |
| ~~CursorPatch~~ | Removed — Salsa handles cursor-related memoization |
| ~~LayoutCache~~ | Removed — Salsa handles layout memoization |

#### Overall Result

Salsa incremental computation replaced the manual multi-layer caching. The pipeline relies on Salsa's automatic dependency tracking and PartialEq early-cutoff for memoization, with SceneCache providing an additional GPU-specific optimization layer.

### Component Macro Details

The `#[kasane::component]` macro follows Svelte's "let the compiler do the work" philosophy, progressively generating optimized code from declarative `view()` functions:

**Stage 1: Input Memoization** — Retains previous input parameter values and skips Element construction when all inputs are identical:

```rust
#[kasane::component]
fn file_tree(entries: &[Entry], selected: usize) -> Element { ... }
// → If entries and selected are unchanged, returns cached Element
```

**Stage 2: Static Layout Cache** — The proc macro detects structurally static parts and calculates layout only once.

**Stage 3: Fine-Grained Update Code Generation** — The proc macro statically analyzes each Element's input parameter dependencies at the AST level and generates code that directly updates only the changed cells in CellGrid.

**Two-Layer Rendering Model:**

```
              +---------------------+
              |  Declarative API    |  ← Plugin authors work here
              |  (Element, view())  |
              +------+--------------+
                     |
         +-----------+----------+
         v                      v
  Compiled path          Interpreter path
  (proc macro gen)       (generic Element tree)
         |                      |
  Static structure →     Element → layout()
    direct CellGrid        → paint() → CellGrid
    update
```

- **Compiled path**: Parts that `#[kasane::component]` can statically analyze. Updates CellGrid directly, bypassing the Element tree.
- **Interpreter path**: Parts where plugins dynamically provide Elements via `Plugin::contribute()`. Uses the full pipeline.
- **Fallback**: Code without `#[kasane::component]` runs through the interpreter path. Optimization is opt-in; correctness is always guaranteed by the interpreter path.

**Stage 5: Compiled rendering for plugins (design analysis)**

(Status: Analysis ongoing. Plugins themselves already exist and L1-equivalent caching is implemented. Partial layout / automatic patch generation for generic plugin views is not yet implemented)

*Problem redefinition:*

Built-in views (StatusBar, Menu, Info, Buffer) are finite in number with known structure, so hand-written PaintPatches are sufficient for optimization. Compiler-driven auto-generation becomes necessary for **plugin authoring** — as the number of plugins increases, individual manual optimization does not scale. Requiring plugin authors to hand-write PaintPatches is unrealistic.

*Auto-generation approach analysis results:*

Five approaches were examined, all with fundamental barriers when applied to built-in views:

| Approach | Overview | Barrier |
|----------|----------|---------|
| A: Macro code generation | Extend `#[kasane_component]` to auto-derive patch code from view function AST | proc_macro operates on single-item local AST transformation. Cannot expand external functions or statically resolve layout |
| B: Runtime tracking | Record cell provenance during paint, identify affected cells via dirty flags | Can identify affected cells but **cannot compute new values** — view → layout → paint still required |
| C: Incremental diffing (React-style) | Redraw only changed parts via Element tree diffing | Already covered by Salsa memoization + section splitting. Additional diff layer not worth the complexity |
| D: Patch templates | Define repaintable slots, partially re-execute view + paint | **Most realistic**. Sub-section granularity pipeline execution |
| E: Declarative DSL | Describe patches in a DSL, macro generates PaintPatch impl | Paint logic still hand-written. Gap between DSL expressiveness and Rust expressiveness is problematic |

Root cause: Rust view functions contain mixed algorithmic computation (word wrap, bin-packing, truncation, obstacle-avoidance positioning) that a compiler cannot statically analyze or transform.

Fundamental difference from Svelte:

```
[Svelte]
Template → Compiler → DOM API calls
                         ↓
              Browser's layout engine (implicit, automatic)
                         ↓
                    Screen pixels

[Kasane]
view() → Element tree → place() → LayoutResult → paint() → CellGrid → diff() → Terminal
           ↑               ↑                        ↑
        Self-built       Self-computed             Self-rendered
```

On the web, `element.textContent = "new"` causes the browser to automatically recalculate layout and repaint. The Svelte compiler relies on this **implicit layout engine** — the compiler only needs to specify "what to change," and the browser resolves "where to place it." Kasane has no equivalent mechanism; writing to CellGrid requires coordinates computed by the application itself.

Detailed barriers for Approach A (7 compilation passes):

1. **Element construction tracking**: Requires symbolic execution of `Vec::push()` sequences. Pattern space grows exponentially with conditional pushes
2. **External function expansion**: proc_macro can only operate on a single item and cannot reference other function bodies
3. **Static layout resolution**: `measure` is recursive and always computed at runtime. Unicode width of Text is statically undecidable
4. **Specialized paint code generation**: Mechanically possible if Element variants are statically known
5. **DirtyFlags conditional insertion**: A single view function uses fields depending on different DirtyFlags in a mixed fashion
6. **GPU path (DrawCommand) generation**: Must also generate DrawCommand sequences in addition to CellGrid, doubling code volume
7. **Correctness verification code generation**: Full pipeline comparison code for debugging

Difficult aspects of DSL (Approach E):

1. **Mixed algorithmic computation**: word wrap, bin-packing, truncation are inseparable from Element construction
2. **Content-dependent layout**: Info popup size depends on word wrap results (circular)
3. **Inter-component position dependency**: Info overlay position depends on Menu Rect + preceding overlay Rect
4. **Structural variations**: Menu 4-way branching, Info 3-way branching cause combinatorial explosion
5. **Layout result propagation to paint**: LayoutResult tree's recursive structure must be flattened to inline code
6. **DSL and Rust dual-world problem**: Helper functions need to be redefined as DSL primitives
7. **Stack + Overlay self-referential structure**: Non-monotonic draw order breaks the assumption that "each Element can be patched independently"

*Why plugins have an advantage:*

| Barrier | Built-in view | Plugin Slot function |
|---------|---------------|---------------------|
| Algorithmic computation | word_wrap, packing, truncate | **Almost none** — mainly raw data display |
| Content-dependent layout | measure → place circular | **Slot Rect is externally provided** — no self-positioning needed |
| Inter-component position dependency | Info avoids Menu | **Slot positions are fixed** — no inter-Slot interference |
| Structural variations | MenuStyle 4-way branching | **Usually 1 pattern** |
| Nesting depth | 5+ levels | **1-2 levels typical** |
| External function calls | Many internal helpers | **Self-contained** |
| Stack + Overlay | Info prompt self-referential structure | **No Overlay in Slots** (Overlay is a separate Slot) |

Fundamental reason: Plugin Slot contributions are a **constrained task** — "insert a small Element at a known position." Built-in views are an **unconstrained task** — "construct the structure of the entire screen." This difference determines the feasibility of DSL/compilation.

*5-stage roadmap (L0-L5):*

Recommended introduction order: L0 → L1 → L3 → L2 → L4 → L5 (maximum effect at minimum cost)

- **L0: Initial state (historical)** — Plugin contributions were rebuilt via the full pipeline
- **L1: Plugin state cache (implemented)** — `PluginSlotCache` in `PluginRuntime` caches `contribute_to()` results per slot, invalidating only when `state_hash()` changes
- **L3: Explicit DirtyFlags dependencies (removed)** — `contribute_deps()` / `transform_deps()` / `annotate_deps()` were removed; Salsa handles dependency tracking automatically
- **L2: Slot position cache (not implemented)** — Per-slot Rect cache for partial repaint when plugin state changes
- **L4: Automatic patch code generation (not implemented)** — Auto-generate `apply_grid()` equivalent for simple plugin views, falling back to L2 for unsupported patterns
- **L5: Decorator pattern recognition (not implemented)** — Recognize typical Decorator patterns and inject style overrides at the end of existing patches
