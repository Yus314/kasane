# ADR-029: Topic-Based Pub/Sub and Plugin-Defined Extension Points

**Status:** Current

### Context

- Inter-plugin communication was limited to `PluginMessage` (untyped, point-to-point) and `ConfigEntry` (string key-value, delayed by one frame)
- Plugins could not define new extension points without framework source changes
- Common patterns (e.g., "broadcast current git branch to all interested plugins") had no clean expression

### Decision

Introduce two complementary mechanisms:

**Topic-based Pub/Sub** (`TopicBus`):
- `TopicId` identifies a topic (e.g., `"git.branch"`)
- Publishers register via `r.publish::<T>(topic, handler)`; subscribers via `r.subscribe::<T>(topic, handler)`
- Two-phase evaluation: (1) collect all publications, (2) deliver to subscribers
- Cycle prevention: publishing during delivery panics in debug, returns error in release
- Type-erased via `Box<dyn Any + Send>` with downcast at delivery

**Plugin-defined Extension Points** (`ExtensionPointId` + `CompositionRule`):
- `ExtensionPointId` identifies an extension point (e.g., `"lint.diagnostics"`)
- Defining plugin: `r.define_extension::<I, O>(id, rule)` with optional own handler
- Contributing plugins: `r.on_extension::<I, O>(id, handler)`
- `CompositionRule`: `Merge` (collect all), `FirstWins` (first non-empty), `Chain` (sequential pipe)
- Results collected via `PluginRuntime::evaluate_extensions()` returning `ExtensionResults`

### Implications

- Plugins can define new extension points without framework changes, enabling ecosystem-driven extensibility
- Pub/sub enables broadcast communication patterns without point-to-point message routing
- Type safety is runtime-enforced (downcast), not compile-time — mismatched types are silently filtered
- Both mechanisms integrate with the existing `PluginBackend` trait via default methods, keeping backward compatibility
