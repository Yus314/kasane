# ADR-036: Cross-File Inlining (reserved)

**Status**: Reserved slot.

This ADR ID is reserved for a future Cross-File Inlining design.
ADR-034 (Display Algebra) ships the type slot via
`Content::Reference(SegmentRef)`; the resolver treats `Reference`
as opaque and forwards it until the ADR-036 design lands.

Live references to this reserved slot:

- `docs/decisions.md` ADR-034 §Decisions (Cross-buffer composition).
- `kasane-core/src/display/algebra/primitives.rs` — `Content::Reference`
  doc comment and `SegmentRef` definition.
- `kasane-core/src/display/algebra/runtime_bridge.rs` — preimage-mapping
  fallthrough comments.
