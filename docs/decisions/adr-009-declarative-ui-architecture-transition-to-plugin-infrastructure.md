# ADR-009: Declarative UI Architecture — Transition to Plugin Infrastructure

**Status:** Decided

**Context:**
Transform kasane from a mere Kakoune frontend into a UI infrastructure for plugin authors. Prioritize extensibility and configurability over direct feature delivery. Migrate from an imperative rendering pipeline to a declarative Element tree base.

**Decision:** Adopt the following 7 design decisions as a package.

See [plugin-development.md](./plugin-development.md) for detailed design.

### 9-1: Protocol Coupling — Kakoune-specific

**Status:** Revoked (originally decided as "gradual decoupling." Reconfirmed that Kasane is a Kakoune-specific UI infrastructure, and generalization is out of scope)

**Decision:** Design with tight coupling to the Kakoune protocol. No decoupling into a general-purpose UI infrastructure.

**Rationale:**
- Kasane is a UI infrastructure for Kakoune plugin authors; generalization to other editors is out of scope
- Unnecessary abstraction increases code complexity and degrades the Kakoune plugin developer experience
- Specializing in Kakoune's JSON UI protocol enables optimal design decisions

### 9-2: Native Plugin Development Path — trait + proc macro

**Status:** Partially updated (the first choice for runtime loading is WASM per [ADR-013](./adr-013-wasm-plugin-runtime-component-model-adoption.md). The native path itself remains current)

**Decision:** Native plugins are implemented as Rust crates. Direct implementation of the `Plugin` trait is maintained as the primary path, while `#[kasane::plugin]` / `#[kasane::component]` proc macros are used alongside for boilerplate reduction and verification assistance.

**Rationale:**
- Maximum type safety. Invalid Msg sends cause compile errors
- Zero-cost abstraction. No runtime overhead due to monomorphization
- Proc macro benefits: compile-time structural validation, boilerplate reduction, layout optimization (Svelte-like approach)
- Plugins distributable via the Rust ecosystem (crates.io, semver)

**Trade-offs:**
- Rebuilding required to add plugins. Users need a Rust toolchain
- Plugin authors need to write Rust

**Subsequent updates:**
- [ADR-013](./adr-013-wasm-plugin-runtime-component-model-adoption.md) added the WASM Component Model, and the recommended distribution path is now WASM
- The native path continues for registration via `kasane::run()`, full access to `&AppState`, and features such as `Surface`
- Hook parity of the `#[kasane_plugin]` macro is being expanded incrementally; currently some hooks still require direct trait implementation
- [ADR-022](./adr-022-plugin-trait-rename-pureplugin-plugin-plugin-pluginbackend.md) renamed the traits: the `Plugin` trait referenced above is now called `PluginBackend` (internal), and the primary user-facing trait is the new `Plugin` (state-externalized, formerly `PurePlugin`)

### 9-3: Element Memory Model — Owned

**Decision:** `Element` has no lifetime parameters and owns all its data.

**Rationale:**
- Lifetimes do not propagate throughout the API. Lowest cognitive load for plugin authors
- No lifetime insertion needed in proc macro generated code
- Ownership transfer allows free transformation when Decorators receive and process Elements
- TUI Element trees are small (20-50 nodes), and clone cost is in the microsecond range, negligible

**Trade-offs:**
- Data copies from State occur (not zero-copy)
- Mitigated by Svelte-like optimization via proc macros (direct rendering bypassing the Element tree)

### 9-4: State Management — TEA (The Elm Architecture)

**Decision:** Adopt global TEA + per-plugin nested TEA.

**Rationale:**
- The existing `AppState::apply(KakouneRequest)` is already TEA-like. Low migration cost
- The Kakoune protocol itself is TEA-like (Kakoune→Frontend: Msg, Frontend→Kakoune: Command)
- Aligns with Rust's ownership model (`&State` for view, `&mut State` for update)
- Plugins have their own State/Msg/update/view, composed by the framework. No inter-plugin interference
- High testability. update() is testable as a pure function
- Component-local state is fundamentally incompatible with Rust's borrowing rules

### 9-5: Plugin Extension Model — Slot + Decorator + Replacement

**Decision:** Provide all three tiers of extension mechanisms.

- **Slot:** Insert Elements at predefined extension points
- **Decorator:** Receive and wrap existing Elements
- **Replacement:** Completely replace existing components

**Rationale:**
- Slots alone provide insufficient extensibility (extensions not anticipated by the framework are impossible)
- Decorators enable extending existing elements (adding line numbers, changing borders, etc.)
- Replacements enable fundamental UI changes (replacing menus with fzf-style, etc.)
- Having levels of freedom allows plugin authors to choose the appropriate level

**Risk mitigation:**
- Decorator application order managed via priority + user settings
- Replacement targets limited to components with low risk of protocol inconsistency
- Explicit opt-in for Replacement (something like an `#[unsafe_replace]` marker) is being considered

**Three-tier composition rules:**
- When a Replacement is registered for a target, the default Element construction is skipped and the Replacement's Element is used
- Decorators are applied even to Replacement output. Replacements handle content substitution, Decorators handle styling (borders, shadows, etc.), achieving separation of concerns. This allows theme plugins (Decorator) and custom menu plugins (Replacement) to coexist naturally
- Decorators must not assume the internal structure of the Element they receive (since the structure may change due to Replacement composition). Only the pattern of wrapping the Element in a Container as-is is safe
- Ignoring the input Element in a Decorator and returning an entirely different Element is discouraged as it overlaps with Replacement's intent. If substitution is the goal, Replacement should be used

**Key event routing:**
- No explicit focus concept; all plugins' `handle_key()` are queried in priority order
- Each plugin refers to `AppState` to self-determine whether it should handle the event (e.g., a Menu Replacement plugin processes when `state.menu.is_some()`)
- Aligns with TEA principles (state is the source of truth), avoiding the complexity of implicit focus state transitions
- See the event propagation section in [plugin-development.md](./plugin-development.md) for details

### 9-6: Layout Model — Flex + Overlay + Grid

**Decision:** A hybrid model with a simplified Flexbox as the base, plus Stack/Overlay and Grid.

**Rationale:**
- Flexbox (Direction + flex-grow + min/max) can express nearly all TUI layouts
- Overlay is essential for Kakoune's menu/info popup positioning (compute_pos). Flexbox alone cannot express layering
- Grid is needed for tabular formats such as column alignment in completion menus
- Constraint-based (Cassowary) is overkill for TUI. Ratatui has precedent moving from constraint-based to a Flexbox-like approach
- Computable in O(n). Can be implemented incrementally (first Flex, then Overlay, finally Grid)

### 9-7: Event Propagation — Hybrid (Central Dispatch + InteractiveId)

**Decision:** Key events are centralized in TEA's update(). Mouse events use InteractiveId attached to Elements for hit testing, then pass the identified target to update().

**Rationale:**
- In kasane, most key inputs are forwarded to Kakoune. "Default behavior for most, exceptional plugin handling" is optimal for central dispatch
- Elements remain pure data structures without closures (consistent with Owned Elements)
- The framework automatically performs mouse hit testing using layout results, so plugins need no coordinate calculations
- InteractiveId is lightweight (enum or integer) with natural Clone/Debug/PartialEq implementations
