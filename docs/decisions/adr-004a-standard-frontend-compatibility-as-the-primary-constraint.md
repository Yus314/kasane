# ADR-004A: Standard Frontend Compatibility as the Primary Constraint

**Status:** Decided

**Context:**
Kasane on one hand aims for existing Kakoune users to adopt it seamlessly as `kak = kasane`, and on the other hand wants to provide plugin authors with powerful UI extension capabilities. Trying to satisfy both at the same layer risks eroding either standard user compatibility or plugin platform freedom.

**Decision:** As a product, Kasane treats standard frontend compatibility as the primary concern, with plugin platform capabilities layered on top. That is, Default Frontend Semantics are the primary constraint, and Extended Frontend Semantics are positioned as additional capabilities.

**Concrete principles:**
- `kak = kasane` means semantic compatibility, not bitwise-identical UI
- In the default state, compatibility with existing `kakrc`, autoload, existing plugins, and existing workflows is prioritized
- Kasane-specific plugins, surfaces, and restructured UI are added value, not prerequisites for normal use
- Plugin-defined UI does not falsify protocol truth; it participates in core semantics as display policy
- Strong restructuring or observed-eliding transformations belong to opt-in extended semantics

**Rationale:**
- For broad adoption in the Kakoune community, low adoption friction is more important than advanced features
- For existing users, the value lies in improving the UI without breaking existing workflows, rather than joining a new ecosystem
- If plugin platform is the product's primary concern, bundled plugins and a proprietary ecosystem tend to erode standard frontend semantics
- Making the Default/Extended two-tier explicit allows maintaining conservative defaults and strong extensibility simultaneously
