# ADR-028: WASM Capability Inference

**Status:** Current

### Context

- WASM plugins previously reported `PluginCapabilities::all()`, causing the host to dispatch every extension point call across the WASM boundary even for non-participating plugins
- Each unnecessary boundary crossing costs ~6-8 μs (measured in kasane-wasm-bench), significant for the per-frame budget

### Decision

Add `register-capabilities() -> u32` to the WIT interface. WASM plugins return a bitmask of `PluginCapabilities` bits for the extension points they actually implement. The host calls this once at plugin construction and caches the result.

The SDK macro (`define_plugin!`) auto-generates the bitmask by inspecting which handler functions the plugin provides, matching the native `HandlerRegistry` capability inference.

### Implications

- WASM plugins that only provide annotations skip transform, overlay, input, and display directive dispatch
- Fallback for plugins not implementing the export: `PluginCapabilities::all()` (safe, conservative)
- Bit layout matches the native `PluginCapabilities` bitflags exactly
