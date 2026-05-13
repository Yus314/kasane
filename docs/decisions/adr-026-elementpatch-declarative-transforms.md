# ADR-026: ElementPatch Declarative Transforms

**Status:** Current

### Context

- `transform()` was an opaque `fn(TransformSubject) -> TransformSubject`, blocking Salsa memoization of transform results
- Debug-mode conflict detection required manual `TransformDescriptor` declarations that could diverge from actual behavior
- No algebraic simplification: an Identity transform still incurred dispatch overhead

### Decision

Introduce `ElementPatch` as a declarative transform algebra:

- Variants: `Identity`, `WrapContainer`, `Prepend`, `Append`, `Replace`, `ModifyFace`, `Compose`, `ModifyAnchor`, `Custom`
- `normalize()` — algebraic simplification (Identity removal, Replace absorption, Compose flattening)
- `apply()` — execute the patch against a `TransformSubject`
- `is_pure()` — true when no `Custom` variants are present (Salsa-memoizable)
- `scope()` — auto-infer `TransformScope` from variant (replaces manual `TransformDescriptor`)
- `impl Composable` — monoid with `Identity` as identity element

The transform chain collects `ElementPatch` from all plugins, composes them, normalizes, and applies. The `Custom` variant wraps `Arc<dyn Fn(TransformSubject) -> TransformSubject>` as an escape hatch for transforms that cannot be expressed declaratively.

### Implications

- Pure patches (no `Custom`) are data, enabling future Salsa memoization of composed transform results
- `TransformDescriptor` can be auto-derived from `ElementPatch::scope()` instead of manual declaration
- `Replace` algebraically absorbs all preceding patches, matching intuition
- Legacy `PluginBackend` transforms are wrapped in `Custom` for backward compatibility
