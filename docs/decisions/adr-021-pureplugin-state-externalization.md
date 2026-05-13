# ADR-021: PurePlugin State Externalization

**Status:** Decided — **Note:** The traits introduced here have been renamed in [ADR-022](./adr-022-plugin-trait-rename-pureplugin-plugin-plugin-pluginbackend.md): `PurePlugin` → `Plugin`, `Plugin` → `PluginBackend`, `PurePluginBridge` → `PluginBridge`, `IsPurePlugin` → `IsBridgedPlugin`. The body below preserves the original names at the time of decision.

### Background

Kasane's rendering pipeline uses a Stage 1/2 split (ADR-020): Stage 1 is Salsa-tracked pure functions, Stage 2 is imperative plugin application. The split exists because plugins hold mutable internal state (`&mut self` methods), making them incompatible with Salsa's pure function model.

The `Plugin` trait has 15+ `&mut self` methods for state transitions and 11+ `&self` methods for view generation. Plugin state caching relies on manual `state_hash() -> u64` (L1) combined with `DirtyFlags`-based slot dependency tracking (L3). This has two weaknesses:

1. Hash collisions can cause stale cache hits (hash-based, not structural equality)
2. Plugin state changes are opaque to the framework (no `PartialEq`, no direct state access)

### Decision

Introduce `PurePlugin` as an alternative to `Plugin` where the framework owns the state:

- **State externalization**: `PurePlugin::State` is a framework-owned `Clone + PartialEq + Debug + Default` type
- **Pure functions**: All methods are `(&self, &State, ...) → (State, effects)` — no `&mut self`
- **Automatic change detection**: State changes detected via `PartialEq` comparison, eliminating manual `state_hash()`
- **Adapter pattern**: `PurePluginBridge` wraps `PurePlugin` into `Plugin`, allowing coexistence

### Trade-offs

| For | Against |
|-----|---------|
| Automatic, collision-free state change detection | State clone cost on every transition (negligible for small states) |
| Pure functions enable future Salsa memoization of Stage 2 | `Plugin` cannot use `Surface` or workspace observation |
| Framework-owned state enables snapshotting and diffing | Blanket `PluginState` impl causes method resolution ambiguity with `Box<dyn PluginState>` (mitigated by using `&mut dyn PluginState` in erased interface) |
| Zero boilerplate for state types (blanket impl) | WASM plugins cannot externalize state to host without serialization overhead |
| Opt-in migration — existing plugins unchanged | Two plugin models to maintain during transition |

### Implementation

- `PluginState` trait with blanket impl for `T: Clone + PartialEq + Debug + Send + 'static`
- `PurePlugin` trait with explicit `State` associated type
- `ErasedPurePlugin` (object-safe, `pub(crate)`) erases the `State` type parameter
- `PurePluginBridge` adapts erased pure plugin to `Plugin` trait with generation-counter `state_hash()`
- `DirtyFlags::PLUGIN_STATE` (bit 7) added for explicit plugin state change signaling
- `IsPurePlugin` marker trait for runtime detection of pure-plugin-backed `dyn Plugin` objects
