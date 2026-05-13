# ADR-022: Plugin Trait Rename — PurePlugin → Plugin, Plugin → PluginBackend

**Status:** Accepted

### Background

Since ADR-021, Kasane has had two native plugin models: `Plugin` (mutable, `&mut self`) and `PurePlugin` (state-externalized, pure functions). In practice, `PurePlugin` became the recommended model for the vast majority of plugins — it provides automatic cache invalidation, a path to Salsa memoization, and a simpler mental model.

However, the naming was a source of confusion:

- New plugin authors encountered `Plugin` first (the natural name) but it was the lower-level, internal-facing trait
- `PurePlugin` was the recommended API but its name suggested it was a specialized alternative
- The "Pure" prefix implied a secondary, academic variant rather than the primary API
- Documentation repeatedly had to explain that `PurePlugin` was preferred despite `Plugin` being the more obvious name

### Decision

Rename the traits to reflect their actual roles:

| Before | After | Role |
|--------|-------|------|
| `PurePlugin` | `Plugin` | Primary user-facing plugin trait (state-externalized) |
| `Plugin` | `PluginBackend` | Internal framework trait (mutable, full access) |
| `PurePluginBridge` | `PluginBridge` | Adapter: `Plugin` → `PluginBackend` |
| `IsPurePlugin` | `IsBridgedPlugin` | Marker trait for runtime detection |
| `register_pure()` | `register()` | Registration method for `Plugin` |
| `register()` (old, took `Box<dyn Plugin>`) | `register_backend()` | Registration method for `PluginBackend` |

### Rationale

- The primary API should have the simplest, most discoverable name
- `PluginBackend` clearly communicates that it is an internal/framework-level trait, not the first thing plugin authors should reach for
- `PluginBridge` and `IsBridgedPlugin` are more descriptive of what they actually do (bridging between models)
- `register()` for the common case, `register_backend()` for the advanced case follows the principle of progressive disclosure

### Trade-offs

| For | Against |
|-----|---------|
| Primary API has the natural name | Breaking change for existing native plugin code |
| Reduces confusion in documentation and onboarding | ADR-021 historical references now use old names |
| `PluginBackend` signals "internal, not your first choice" | Two renames in the plugin system's lifetime |

### Migration

- All `impl PurePlugin` → `impl Plugin`
- All `impl Plugin` (old mutable) → `impl PluginBackend`
- `registry.register_pure(x)` → `registry.register(x)`
- `registry.register(Box::new(x))` → `registry.register_backend(Box::new(x))`
- Historical ADR text (ADR-021) preserved with original names; current documentation updated
