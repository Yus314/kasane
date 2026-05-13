# ADR-018: Display Policy Layer and Display Transformation / Display Unit Model

**Status:** Decided

### Background

While organizing Kasane's requirements framework, it became necessary to make the following distinctions explicit:

- Core features that Kasane itself directly guarantees
- Capabilities that Kasane provides as an extension infrastructure
- Proof-of-concept use cases realized on top of that infrastructure

In particular, to handle overlay, folding, auxiliary region UI, display-line navigation, workspace UI, etc. consistently, it became clear that simply "drawing the Observed State as-is" is insufficient, and a display policy layer is needed on the frontend side.

Previously, `Overlay`, `Decorator`, `Replacement`, `Transform`, and `Surface` existed individually, but it was unclear what theory they were part of. As a result, issue-driven requirements tended to flow into enumeration of individual features, and "what Kasane directly implements" vs. "what Kasane enables" became conflated.

### Decision

Kasane adopts the `Display Policy Layer` as a first-class design concept.

This layer determines "what display structure to project into" before passing Observed State to rendering, and includes at least the following:

- overlay composition
- contributions to auxiliary regions
- display transformation
- surrogate display
- display unit grouping
- interaction policy

### 18-1: Permit Display Transformation

Kasane permits plugins and future standard UI to restructure Observed State using `Display Transformation`.

Display Transformation may include:

- elision
- surrogate display
- additional display
- restructuring

However, this is **display policy**, not falsification of protocol truth.

### 18-2: Permit Observed-Eliding Transformation

Kasane permits not only `Observed-preserving transformation` but also `Observed-eliding transformation`.

Examples:

- Summary display of multiple lines via fold summary
- Restructuring into a different structure via outline view
- Relocation of content to auxiliary UI

However, elided Observed State must not be treated as "a fact sent by upstream as such." Elision is a display-level omission, not deletion of truth.

### 18-3: Introduce Display Unit Model

Kasane introduces `Display Unit` as the smallest operable unit of the restructured UI.

A Display Unit is not merely a layout box; it may have at least the following:

- geometry
- semantic role
- source mapping
- interaction policy
- navigation relationships with other units

This enables meaningful hit test, focus, navigation, and selection even for UI that has undergone display restructuring.

### 18-4: Handling When Source Mapping Is Weak

When a Display Unit does not have a complete inverse mapping to its source, Kasane may treat that unit as read-only or with restricted interaction.

The important thing is not to leave undefined operations implicit. Kasane should be able to explicitly represent units where interaction is impossible or restricted.

### 18-5: Core and Plugin Responsibility Allocation

What plugins are responsible for:

- Defining transformation policy
- Introducing display units
- Interaction policy for plugin-specific UI

What core is responsible for:

- Separation of protocol truth and display policy
- Placing plugin-defined UI under the same composition rules as standard UI
- Infrastructure for representing display units as targets for hit test, focus, and navigation
- Semantics for degraded mode when source mapping is weak

### 18-6: Relationship with Existing APIs

In the current API, dedicated abstractions for `Display Transformation` and `Display Unit` are incomplete.

Current proof-of-concept means:

- `Overlay`
- `Decorator`
- `Replacement`
- `Transform`
- `LineDecoration`
- `Surface`

These are fragmentary representations of the future Display Policy Layer, not complete equivalents. In particular, source mapping and display-oriented navigation are subjects for future infrastructure development.

### 18-7: Non-goals

This ADR does not mean immediately becoming a general-purpose UI framework.

Kasane continues to be a Kakoune-specific frontend runtime, and the Display Policy Layer is also designed with the assumption of Observed State received from Kakoune's JSON UI.

### 18-8: Consequences

With this decision, the requirements documents are organized as follows:

- Core requirements
- Extension infrastructure requirements
- Proof-of-concept targets and representative use cases
- Upstream dependencies and degraded behavior

Additionally, the semantics document treats `Display Policy State`, `Display Transformation`, and `Display Unit` as first-class concepts.

The next implementation steps are to incrementally introduce the following in Phase 5:

1. display transformation hook
2. display unit model
3. display-oriented hit test / navigation
4. source mapping and interaction policy development
