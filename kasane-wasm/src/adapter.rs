//! WASM adapter: builds a `Plugin`-trait facet over a wasmtime
//! Component Model guest. The loader wraps the result in
//! [`PluginBridge`](kasane_core::plugin::PluginBridge), which is what
//! the host actually consumes; this module's `WasmPlugin` adapter is
//! crate-private and only constructed from `WasmPluginLoader::load*`.

use std::sync::Arc;
use std::sync::atomic::{AtomicU16, AtomicU32, AtomicU64, Ordering};

use std::sync::Mutex;

use compact_str::CompactString;
use kasane_core::element::{Element, PluginTag};
use kasane_core::input::{CompiledKeyMap, KeyResponse};
use kasane_core::plugin::{
    AppView, BackgroundLayer, BlendMode, Command, Contribution, ElementPatch, FrameworkAccess,
    HandlerRegistry, KeyHandleResult, LineAnnotation, OverlayContribution, PluginAuthorities,
    PluginCapabilities, PluginDiagnostic, PluginId, TransformSubject, VirtualTextItem,
};
use kasane_core::state::DirtyFlags;
use kasane_core::surface::{
    EventContext, SizeHint, SlotDeclaration, Surface, SurfaceEvent, SurfaceId,
    SurfacePlacementRequest, ViewContext,
};

use crate::bindings;
use crate::bindings::kasane::plugin::types as wit;
use crate::convert;
use crate::host::{self, HostState};

static NEXT_WASM_SURFACE_ID: AtomicU32 = AtomicU32::new(u32::MAX - 1_000_000);

fn next_wasm_surface_id() -> SurfaceId {
    SurfaceId(NEXT_WASM_SURFACE_ID.fetch_add(1, Ordering::Relaxed))
}

struct WasmPluginRuntime {
    store: wasmtime::Store<HostState>,
    instance: bindings::KasanePlugin,
}

/// Maximum pending diagnostics kept per plugin (ring buffer).
const MAX_PENDING_DIAGNOSTICS: usize = 10;

/// Sentinel value for state_hash that forces recollection on the next frame.
const HASH_SENTINEL: u64 = u64::MAX;

/// Per-WASM-call epoch budget. The engine ticker increments every 10ms, so
/// this is approximately the wall-clock budget for a single plugin call.
/// Production target is perceptual imperceptibility (sub-frame); 100ms is
/// well outside that, so a real runaway plugin still traps, but legitimate
/// calls don't trip on CI scheduler jitter.
const RUNTIME_CALL_EPOCH_BUDGET: u64 = 10;

/// When true, WASM plugin call failures panic immediately instead of
/// silently substituting `R::default()`. Tests opt in via
/// [`set_panic_on_trap`] so that a trap surfaces as a loud panic rather
/// than as an empty-result false negative. Production keeps the default
/// (graceful degradation) so a buggy plugin never crashes the editor.
static PANIC_ON_TRAP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Toggle the global panic-on-trap mode. Intended for test setup.
#[doc(hidden)]
#[allow(dead_code)] // only invoked from `#[cfg(test)]` paths in this crate
pub fn set_panic_on_trap(enabled: bool) {
    PANIC_ON_TRAP.store(enabled, std::sync::atomic::Ordering::Relaxed);
}

fn panic_on_trap_enabled() -> bool {
    PANIC_ON_TRAP.load(std::sync::atomic::Ordering::Relaxed)
}

struct WasmPluginShared {
    runtime: Mutex<WasmPluginRuntime>,
    plugin_id: PluginId,
    plugin_tag: AtomicU16,
    cached_state_hash: AtomicU64,
    cached_view_deps: DirtyFlags,
    cached_capabilities: PluginCapabilities,
    process_allowed: bool,
    authorities: PluginAuthorities,
    pending_diagnostics: Mutex<Vec<PluginDiagnostic>>,
    manifest_descriptor: Option<kasane_core::plugin::CapabilityDescriptor>,
    publish_topics: Vec<String>,
    subscribe_topics: Vec<String>,
    has_unified_display_export: bool,
    /// When `true`, host wraps every plugin-originated
    /// `Command::EvalCommand` with a Kakoune `try…catch` pattern so
    /// failures surface as a marker `info_show` attributed to this plugin
    /// (ADR-042). Derived from manifest `[handlers]
    /// command_error_observability`. Defaults to `false`.
    command_error_observability: bool,
    /// Keeps the epoch ticker thread alive as long as this plugin is alive.
    _epoch_ticker: Arc<crate::EpochTicker>,
}

impl WasmPluginShared {
    fn plugin_tag(&self) -> PluginTag {
        PluginTag(self.plugin_tag.load(Ordering::Relaxed))
    }

    fn with_runtime<R>(&self, f: impl FnOnce(&mut WasmPluginRuntime) -> R) -> R {
        let mut runtime = self.runtime.lock().unwrap();
        runtime.store.set_epoch_deadline(RUNTIME_CALL_EPOCH_BUDGET);
        f(&mut runtime)
    }

    /// Centralized error handler for WASM plugin call failures. Emits the
    /// standard tracing log + diagnostic record + cache-poison sentinel,
    /// or panics if [`PANIC_ON_TRAP`] is enabled (test mode).
    fn handle_call_error(&self, method: &str, error: &anyhow::Error) {
        if panic_on_trap_enabled() {
            panic!(
                "WASM plugin {}.{method} trapped: {error:#}",
                self.plugin_id.0
            );
        }
        tracing::error!("WASM plugin {}.{method} failed: {error}", self.plugin_id.0);
        self.record_diagnostic(method, error);
        self.set_state_hash(HASH_SENTINEL);
    }

    fn record_diagnostic(&self, method: &str, error: &anyhow::Error) {
        let diag = PluginDiagnostic::runtime_error(
            self.plugin_id.clone(),
            method.to_string(),
            error.to_string(),
        );
        let mut pending = self.pending_diagnostics.lock().unwrap();
        if pending.len() >= MAX_PENDING_DIAGNOSTICS {
            pending.remove(0);
        }
        pending.push(diag);
    }

    /// Lock runtime, sync state, call function, optionally update hash, log error on failure.
    fn call_synced_inner<R: Default>(
        &self,
        state: &AppView<'_>,
        method: &str,
        update_hash: bool,
        f: impl FnOnce(&mut WasmPluginRuntime) -> anyhow::Result<R>,
    ) -> R {
        self.with_runtime(|runtime| {
            host::sync_from_app_state(
                runtime.store.data_mut(),
                state.as_app_state(),
                self.cached_view_deps,
            );
            runtime.store.data_mut().plugin_tag = self.plugin_tag();
            let result = match f(runtime) {
                Ok(result) => result,
                Err(e) => {
                    self.handle_call_error(method, &e);
                    return R::default();
                }
            };
            if update_hash {
                let plugin_api = runtime.instance.kasane_plugin_plugin_api();
                if let Ok(h) = plugin_api.call_state_hash(&mut runtime.store) {
                    self.set_state_hash(h);
                }
            }
            result
        })
    }

    /// Lock runtime, sync state, call function, log error on failure.
    fn call_synced<R: Default>(
        &self,
        state: &AppView<'_>,
        method: &str,
        f: impl FnOnce(&mut WasmPluginRuntime) -> anyhow::Result<R>,
    ) -> R {
        self.call_synced_inner(state, method, false, f)
    }

    /// Like call_synced but also updates the cached state hash afterwards.
    fn call_synced_with_hash<R: Default>(
        &self,
        state: &AppView<'_>,
        method: &str,
        f: impl FnOnce(&mut WasmPluginRuntime) -> anyhow::Result<R>,
    ) -> R {
        self.call_synced_inner(state, method, true, f)
    }

    fn state_hash(&self) -> u64 {
        self.cached_state_hash.load(Ordering::Relaxed)
    }

    fn set_state_hash(&self, value: u64) {
        self.cached_state_hash.store(value, Ordering::Relaxed);
    }

    fn hosted_surface(
        self: &Arc<Self>,
        surface_key: String,
        size_hint: wit::SurfaceSizeHint,
        declared_slots: Vec<wit::DeclaredSlot>,
        initial_placement: Option<SurfacePlacementRequest>,
    ) -> Box<dyn Surface> {
        let declared_slots = declared_slots
            .into_iter()
            .map(|slot| {
                SlotDeclaration::new(slot.name, convert::wit_slot_kind_to_slot_kind(slot.kind))
            })
            .collect();
        Box::new(WasmHostedSurface {
            shared: Arc::clone(self),
            id: next_wasm_surface_id(),
            surface_key,
            size_hint: convert::wit_surface_size_hint_to_size_hint(&size_hint),
            declared_slots,
            initial_placement,
        })
    }

    fn convert_command(self: &Arc<Self>, command: &wit::Command) -> Vec<Command> {
        match command {
            wit::Command::RegisterSurface(config) => {
                let placement = convert::wit_surface_placement_to_request(&config.placement);
                vec![Command::RegisterSurfaceRequested {
                    surface: self.hosted_surface(
                        config.surface_key.clone(),
                        config.size_hint,
                        config.declared_slots.clone(),
                        Some(placement.clone()),
                    ),
                    placement,
                }]
            }
            wit::Command::SetSetting(entry) => {
                let value = convert::wit_setting_value_to_setting_value(&entry.value);
                vec![Command::SetSetting {
                    plugin_id: self.plugin_id.clone(),
                    key: entry.key.clone(),
                    value,
                }]
            }
            // When the plugin opted in via
            // `[handlers] command_error_observability = true` (ADR-042),
            // wrap the Kakoune command body with a `try…catch` that fires
            // an attributed `info_show` on failure. The marker is parsed
            // back by `state/apply.rs` and routed to the plugin's
            // `on-command-error-effects` export (Step 2).
            wit::Command::EvalCommand(cmd) if self.command_error_observability => {
                let wrapped =
                    kasane_core::plugin::effect::error_attribution::wrap_command_with_marker(
                        cmd,
                        self.plugin_id.as_str(),
                    );
                vec![Command::kakoune_command(&wrapped)]
            }
            _ => vec![convert::wit_command_to_command(command)],
        }
    }

    fn convert_commands(self: &Arc<Self>, commands: &[wit::Command]) -> Vec<Command> {
        commands
            .iter()
            .flat_map(|command| self.convert_command(command))
            .collect()
    }

    /// Tier-1 effects projection — produces typed [`KakouneSideEffects`]
    /// (the `on_*_tier1` setter return shape). Callers that need the
    /// type-erased [`Effects`] go through `.into()`.
    fn convert_kakoune_side_effects_typed(
        self: &Arc<Self>,
        effects: &wit::KakouneSideEffects,
    ) -> kasane_core::plugin::KakouneSideEffects {
        let shared = Arc::clone(self);
        convert::wit_kakoune_side_effects_to_kakoune_side_effects_with(effects, move |command| {
            shared.convert_command(command)
        })
    }

    /// Tier-2 counterpart of [`Self::convert_process_capable_effects`] used
    /// by the Plugin-trait (`HandlerRegistry`) path.
    fn convert_process_capable_effects_typed(
        self: &Arc<Self>,
        effects: &wit::ProcessCapableEffects,
    ) -> kasane_core::plugin::ProcessCapableEffects {
        let shared = Arc::clone(self);
        convert::wit_process_capable_effects_to_process_capable_effects_with(
            effects,
            move |command| shared.convert_command(command),
        )
    }

    /// `call_synced_with_hash` variant whose closure operates on
    /// `&Arc<Self>` so it can clone the shared state for inner helpers
    /// like [`Self::convert_kakoune_side_effects_typed`]. Used by the
    /// `Plugin::register` closures on `WasmPlugin`.
    fn call_synced_with_hash_arc<R: Default>(
        self: &Arc<Self>,
        state: &AppView<'_>,
        method: &str,
        f: impl FnOnce(&Arc<Self>, &mut WasmPluginRuntime) -> anyhow::Result<R>,
    ) -> R {
        let shared = Arc::clone(self);
        self.call_synced_inner(state, method, true, move |runtime| f(&shared, runtime))
    }
}

struct WasmHostedSurface {
    shared: Arc<WasmPluginShared>,
    id: SurfaceId,
    surface_key: String,
    size_hint: SizeHint,
    declared_slots: Vec<SlotDeclaration>,
    initial_placement: Option<SurfacePlacementRequest>,
}

impl Surface for WasmHostedSurface {
    fn id(&self) -> SurfaceId {
        self.id
    }

    fn surface_key(&self) -> CompactString {
        self.surface_key.clone().into()
    }

    fn size_hint(&self) -> SizeHint {
        self.size_hint
    }

    fn initial_placement(&self) -> Option<SurfacePlacementRequest> {
        self.initial_placement.clone()
    }

    fn view(&self, ctx: &ViewContext<'_>) -> Element {
        let surface_key = self.surface_key.to_string();
        let wit_ctx = convert::surface_view_context_to_wit(ctx);
        self.shared.with_runtime(|runtime| {
            host::sync_from_app_state(
                runtime.store.data_mut(),
                ctx.state,
                self.shared.cached_view_deps,
            );
            runtime.store.data_mut().focused = ctx.focused;
            runtime.store.data_mut().elements.clear();
            let plugin_api = runtime.instance.kasane_plugin_plugin_api();
            match plugin_api.call_render_surface(&mut runtime.store, &surface_key, wit_ctx) {
                Ok(Some(handle)) => runtime.store.data_mut().take_root_element(handle),
                Ok(None) => Element::Empty,
                Err(e) => {
                    tracing::error!(
                        "WASM plugin {}.render_surface({surface_key}) failed: {e}",
                        self.shared.plugin_id.0
                    );
                    Element::Empty
                }
            }
        })
    }

    fn handle_event(&mut self, _event: SurfaceEvent, _ctx: &EventContext<'_>) -> Vec<Command> {
        let surface_key = self.surface_key.to_string();
        self.shared.with_runtime(|runtime| {
            host::sync_from_app_state(
                runtime.store.data_mut(),
                _ctx.state,
                self.shared.cached_view_deps,
            );
            runtime.store.data_mut().focused = _ctx.focused;
            let plugin_api = runtime.instance.kasane_plugin_plugin_api();
            let wit_event = convert::surface_event_to_wit(&_event);
            let wit_ctx = convert::surface_event_context_to_wit(_ctx);
            match plugin_api.call_handle_surface_event(
                &mut runtime.store,
                &surface_key,
                &wit_event,
                wit_ctx,
            ) {
                Ok(commands) => {
                    if let Ok(hash) = plugin_api.call_state_hash(&mut runtime.store) {
                        self.shared.set_state_hash(hash);
                    }
                    let shared = Arc::clone(&self.shared);
                    shared.convert_commands(&commands)
                }
                Err(e) => {
                    tracing::error!(
                        "WASM plugin {}.handle_surface_event({surface_key}) failed: {e}",
                        self.shared.plugin_id.0
                    );
                    vec![]
                }
            }
        })
    }

    fn on_state_changed(
        &mut self,
        state: &kasane_core::state::AppState,
        dirty: DirtyFlags,
    ) -> Vec<Command> {
        let surface_key = self.surface_key.to_string();
        self.shared.with_runtime(|runtime| {
            host::sync_from_app_state(
                runtime.store.data_mut(),
                state,
                self.shared.cached_view_deps,
            );
            let plugin_api = runtime.instance.kasane_plugin_plugin_api();
            match plugin_api.call_handle_surface_state_changed(
                &mut runtime.store,
                &surface_key,
                dirty.bits(),
            ) {
                Ok(commands) => {
                    if let Ok(hash) = plugin_api.call_state_hash(&mut runtime.store) {
                        self.shared.set_state_hash(hash);
                    }
                    let shared = Arc::clone(&self.shared);
                    shared.convert_commands(&commands)
                }
                Err(e) => {
                    tracing::error!(
                        "WASM plugin {}.handle_surface_state_changed({surface_key}) failed: {e}",
                        self.shared.plugin_id.0
                    );
                    vec![]
                }
            }
        })
    }

    fn state_hash(&self) -> u64 {
        self.shared.state_hash()
    }

    fn declared_slots(&self) -> &[SlotDeclaration] {
        &self.declared_slots
    }
}

/// A WASM Component Model plugin adapted to the native [`Plugin`] trait.
///
/// Construct via [`Self::new`] / [`Self::new_from_manifest`] inside
/// [`crate::WasmPluginLoader`] and immediately wrap with
/// `PluginBridge::new(plugin)` — the loader returns the bridge, not
/// this raw adapter. The struct is intentionally `pub(crate)`: outside
/// the crate, `kasane_wasm::WasmPlugin` is a type alias to
/// `PluginBridge` (β-3.3b.12).
pub(crate) struct WasmPlugin {
    shared: Arc<WasmPluginShared>,
    /// Compiled key map built from `declare-key-map` at construction time.
    key_map: Option<CompiledKeyMap>,
    /// Cached projection descriptors from `declare-projections` at construction time.
    cached_projection_descriptors: Vec<kasane_core::display::ProjectionDescriptor>,
}

impl WasmPlugin {
    /// Create a WasmPlugin using pre-resolved manifest metadata.
    ///
    /// Unlike [`new()`], this accepts capabilities and view_deps as params
    /// instead of querying WASM. Still queries `declare_key_map()` since
    /// that is behavioral, not static metadata.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_from_manifest(
        mut store: wasmtime::Store<HostState>,
        instance: bindings::KasanePlugin,
        id: String,
        process_allowed: bool,
        authorities: PluginAuthorities,
        cached_capabilities: PluginCapabilities,
        cached_view_deps: DirtyFlags,
        manifest_descriptor: Option<kasane_core::plugin::CapabilityDescriptor>,
        publish_topics: Vec<String>,
        subscribe_topics: Vec<String>,
        command_error_observability: bool,
        epoch_ticker: Arc<crate::EpochTicker>,
    ) -> Self {
        store.data_mut().plugin_id = id.clone();

        // Query key map declaration at construction time (behavioral, not static).
        let plugin_api = instance.kasane_plugin_plugin_api();
        let key_map = match plugin_api.call_declare_key_map(&mut store) {
            Ok(decls) if !decls.is_empty() => {
                match convert::wit_key_group_decls_to_compiled_key_map(&decls) {
                    Ok(map) => Some(map),
                    Err(e) => {
                        tracing::error!("WASM plugin {id}.declare_key_map conversion error: {e}");
                        None
                    }
                }
            }
            Ok(_) => None,
            Err(_) => None,
        };

        // Query projection declarations at construction time.
        let cached_projection_descriptors = match plugin_api.call_declare_projections(&mut store) {
            Ok(descs) if !descs.is_empty() => {
                convert::wit_projection_descriptors_to_descriptors(&descs)
            }
            _ => Vec::new(),
        };

        // Probe whether the plugin exports the unified `display` function.
        let has_unified_display_export = plugin_api.call_display(&mut store).is_ok();

        Self {
            shared: Arc::new(WasmPluginShared {
                runtime: Mutex::new(WasmPluginRuntime { store, instance }),
                plugin_id: PluginId::from(id),
                plugin_tag: AtomicU16::new(PluginTag::UNASSIGNED.0),
                cached_state_hash: AtomicU64::new(0),
                cached_view_deps,
                cached_capabilities,
                process_allowed,
                authorities,
                pending_diagnostics: Mutex::new(Vec::new()),
                manifest_descriptor,
                publish_topics,
                subscribe_topics,
                has_unified_display_export,
                command_error_observability,
                _epoch_ticker: epoch_ticker,
            }),
            key_map,
            cached_projection_descriptors,
        }
    }

    pub(crate) fn new(
        mut store: wasmtime::Store<HostState>,
        instance: bindings::KasanePlugin,
        id: String,
        process_allowed: bool,
        authorities: PluginAuthorities,
        epoch_ticker: Arc<crate::EpochTicker>,
    ) -> Self {
        // Set plugin_id on HostState so log messages can be attributed.
        store.data_mut().plugin_id = id.clone();

        // Query view_deps once at construction time (static declaration).
        let plugin_api = instance.kasane_plugin_plugin_api();
        let view_deps_bits = plugin_api
            .call_view_deps(&mut store)
            .unwrap_or(DirtyFlags::ALL.bits());
        let cached_view_deps = DirtyFlags::from_bits_truncate(view_deps_bits);

        // Query capabilities once at construction time (v0.23.0+).
        // Falls back to PluginCapabilities::all() for older plugins that
        // don't implement register-capabilities.
        let cached_capabilities = plugin_api
            .call_register_capabilities(&mut store)
            .map(PluginCapabilities::from_bits_truncate)
            .unwrap_or(PluginCapabilities::all());

        // Query key map declaration at construction time (Phase 3+).
        let key_map = match plugin_api.call_declare_key_map(&mut store) {
            Ok(decls) if !decls.is_empty() => {
                match convert::wit_key_group_decls_to_compiled_key_map(&decls) {
                    Ok(map) => Some(map),
                    Err(e) => {
                        tracing::error!("WASM plugin {id}.declare_key_map conversion error: {e}");
                        None
                    }
                }
            }
            Ok(_) => None,
            Err(_) => None, // Plugin doesn't implement declare-key-map — OK
        };

        // Query projection declarations at construction time.
        let cached_projection_descriptors = match plugin_api.call_declare_projections(&mut store) {
            Ok(descs) if !descs.is_empty() => {
                convert::wit_projection_descriptors_to_descriptors(&descs)
            }
            _ => Vec::new(),
        };

        // Probe whether the plugin exports the unified `display` function.
        let has_unified_display_export = plugin_api.call_display(&mut store).is_ok();

        Self {
            shared: Arc::new(WasmPluginShared {
                runtime: Mutex::new(WasmPluginRuntime { store, instance }),
                plugin_id: PluginId::from(id),
                plugin_tag: AtomicU16::new(PluginTag::UNASSIGNED.0),
                cached_state_hash: AtomicU64::new(0),
                cached_view_deps,
                cached_capabilities,
                process_allowed,
                authorities,
                pending_diagnostics: Mutex::new(Vec::new()),
                manifest_descriptor: None,
                publish_topics: Vec::new(),
                subscribe_topics: Vec::new(),
                has_unified_display_export,
                command_error_observability: false,
                _epoch_ticker: epoch_ticker,
            }),
            key_map,
            cached_projection_descriptors,
        }
    }
}

/// Native `Lens` impl backed by a WASM plugin's
/// `lens-display` / `lens-display-line` exports.
///
/// Built inside the `declare_lenses` factory in `Plugin::register`
/// (one adapter per declaration returned from `declare-lenses`).
/// The host's lens dispatcher invokes the trait methods; the
/// adapter forwards each call to the plugin via `call_synced`.
struct WasmLensAdapter {
    shared: Arc<WasmPluginShared>,
    declaration: convert::LensDeclaration,
}

impl WasmLensAdapter {
    fn build_view_view<'a>(
        &self,
        view: &'a kasane_core::plugin::AppView<'_>,
    ) -> &'a kasane_core::plugin::AppView<'a> {
        // No-op helper kept for symmetry with other adapter
        // shapes; AppView lifetime threads naturally.
        view
    }
}

impl kasane_core::lens::Lens for WasmLensAdapter {
    fn id(&self) -> kasane_core::lens::LensId {
        kasane_core::lens::LensId::new(self.shared.plugin_id.clone(), self.declaration.name.clone())
    }

    fn label(&self) -> String {
        self.declaration.label.clone()
    }

    fn priority(&self) -> i16 {
        self.declaration.priority
    }

    fn cache_strategy(&self) -> kasane_core::lens::CacheStrategy {
        self.declaration.cache_strategy
    }

    fn display(
        &self,
        view: &kasane_core::plugin::AppView<'_>,
    ) -> Vec<kasane_core::display::DisplayDirective> {
        let view = self.build_view_view(view);
        let lens_name = self.declaration.name.clone();
        let plugin_tag = self.shared.plugin_tag();
        self.shared
            .call_synced(view, "lens_display", |rt| -> anyhow::Result<_> {
                rt.store.data_mut().elements.clear();
                let api = rt.instance.kasane_plugin_plugin_api();
                let directives = api.call_lens_display(&mut rt.store, &lens_name)?;
                let mut out = Vec::with_capacity(directives.len());
                for d in &directives {
                    out.push(convert::wit_display_directive_to_directive_with_resolver(
                        d,
                        plugin_tag,
                        &mut |handle| rt.store.data_mut().take_root_element(handle),
                    ));
                }
                Ok(out)
            })
    }

    fn display_line(
        &self,
        view: &kasane_core::plugin::AppView<'_>,
        line: usize,
    ) -> Vec<kasane_core::display::DisplayDirective> {
        let view = self.build_view_view(view);
        let lens_name = self.declaration.name.clone();
        let plugin_tag = self.shared.plugin_tag();
        self.shared
            .call_synced(view, "lens_display_line", |rt| -> anyhow::Result<_> {
                rt.store.data_mut().elements.clear();
                let api = rt.instance.kasane_plugin_plugin_api();
                let directives =
                    api.call_lens_display_line(&mut rt.store, &lens_name, line as u32)?;
                let mut out = Vec::with_capacity(directives.len());
                for d in &directives {
                    out.push(convert::wit_display_directive_to_directive_with_resolver(
                        d,
                        plugin_tag,
                        &mut |handle| rt.store.data_mut().take_root_element(handle),
                    ));
                }
                Ok(out)
            })
    }
}

/// `Plugin::register` for the WASM adapter.
///
/// Each closure clones `Arc<WasmPluginShared>` and translates one or
/// more `kasane:plugin` WIT exports into a [`HandlerRegistry`]
/// registration. The grouping below mirrors the WIT export families;
/// adapter helpers live on [`WasmPluginShared`].
///
/// WIT exports that are intentionally absent (no migration target):
///
/// - `observe-text-input`, `handle-text-input` — no WIT exports.
/// - `start-process-task` and `workspace-request` — no override on
///   the WASM side; the framework default is sufficient.
///
/// Behavioural caveats:
///
/// - **Transform**: `transform-patch` registers via [`on_transform`];
///   the legacy full-rewrite `transform` export uses
///   [`on_transform_full`] so [`PluginBridge::transform`] can fall
///   back to it when the patch resolves to `ElementPatch::Identity`.
/// - **Annotations**: the unified `annotate-line` WIT export uses the
///   monolithic [`on_annotate_line`] setter to avoid 5× WIT round-trips
///   per annotated line.
/// - **Capabilities**: WIT plugins always register no-op-tolerant
///   handlers (`on_io_event_tier2`, `on_command_error`, …), so the
///   handler-presence inferred capability set is a superset. The
///   adapter calls [`declare_capabilities`] / [`declare_capability_descriptor`]
///   to override with the manifest / `register-capabilities` set.
/// - **State hash**: WIT plugins surface change signals via the
///   `state-hash` export; [`declare_state_hash`] forwards that to the
///   bridge instead of the per-mutation generation counter (which
///   never advances for stateless plugins).
impl kasane_core::plugin::StatelessPlugin for WasmPlugin {
    fn id(&self) -> PluginId {
        self.shared.plugin_id.clone()
    }

    fn register(&self, r: &mut HandlerRegistry<()>) {
        r.declare_interests(self.shared.cached_view_deps);

        // on_init_effects → on_init_tier1
        let shared = Arc::clone(&self.shared);
        r.on_init_tier1(move |_state, app| {
            let effects = shared.call_synced_with_hash(app, "on_init_effects", |rt| {
                let api = rt.instance.kasane_plugin_plugin_api();
                Ok(convert::wit_bootstrap_effects_to_kakoune_side_effects(
                    &api.call_on_init_effects(&mut rt.store)?,
                ))
            });
            ((), effects)
        });

        // on_active_session_ready_effects → on_session_ready_tier1
        let shared = Arc::clone(&self.shared);
        r.on_session_ready_tier1(move |_state, app| {
            let effects =
                shared.call_synced_with_hash(app, "on_active_session_ready_effects", |rt| {
                    let api = rt.instance.kasane_plugin_plugin_api();
                    Ok(convert::wit_session_ready_effects_to_kakoune_side_effects(
                        &api.call_on_active_session_ready_effects(&mut rt.store)?,
                    ))
                });
            ((), effects)
        });

        // on_state_changed_effects → on_state_changed_tier1
        let shared = Arc::clone(&self.shared);
        r.on_state_changed_tier1(move |_state, app, dirty| {
            let effects =
                shared.call_synced_with_hash_arc(app, "on_state_changed_effects", |s, rt| {
                    let api = rt.instance.kasane_plugin_plugin_api();
                    let wit_effects =
                        api.call_on_state_changed_effects(&mut rt.store, dirty.bits())?;
                    Ok(s.convert_kakoune_side_effects_typed(&wit_effects))
                });
            ((), effects)
        });

        // on_io_event_effects → on_io_event_tier2
        let shared = Arc::clone(&self.shared);
        r.on_io_event_tier2(move |_state, event, app| {
            let effects = shared.call_synced_with_hash_arc(app, "on_io_event_effects", |s, rt| {
                let api = rt.instance.kasane_plugin_plugin_api();
                let wit_event = convert::io_event_to_wit(event);
                Ok(s.convert_process_capable_effects_typed(
                    &api.call_on_io_event_effects(&mut rt.store, &wit_event)?,
                ))
            });
            ((), effects)
        });

        // on_workspace_changed
        let shared = Arc::clone(&self.shared);
        r.on_workspace_changed(move |_state, query| {
            let snapshot = convert::workspace_query_to_snapshot(query);
            shared.with_runtime(|runtime| {
                let api = runtime.instance.kasane_plugin_plugin_api();
                if let Err(e) = api.call_on_workspace_changed(&mut runtime.store, &snapshot) {
                    tracing::error!(
                        "WASM plugin {}.on_workspace_changed failed: {e}",
                        shared.plugin_id.0
                    );
                    return;
                }
                if let Ok(hash) = api.call_state_hash(&mut runtime.store) {
                    shared.set_state_hash(hash);
                }
            });
        });

        // on_shutdown
        let shared = Arc::clone(&self.shared);
        r.on_shutdown(move |_state| {
            shared.with_runtime(|runtime| {
                let api = runtime.instance.kasane_plugin_plugin_api();
                if let Err(e) = api.call_on_shutdown(&mut runtime.store) {
                    tracing::error!("WASM plugin {}.on_shutdown failed: {e}", shared.plugin_id.0);
                }
            });
        });

        // ---- β-3.3b.2 — Input observers ----
        // WIT exports: observe-key / observe-mouse / observe-drop. There is
        // no `observe-text-input` WIT export today, so the corresponding
        // `on_observe_text_input` registry slot stays empty for WASM
        // plugins.

        // observe_key → on_observe_key (gated by INPUT_HANDLER capability)
        let shared = Arc::clone(&self.shared);
        r.on_observe_key(move |_state, key, app| {
            if !shared
                .cached_capabilities
                .contains(PluginCapabilities::INPUT_HANDLER)
            {
                return;
            }
            shared.call_synced(app, "observe_key", |rt| {
                let api = rt.instance.kasane_plugin_plugin_api();
                let wit_key = convert::key_event_to_wit(key);
                Ok(api.call_observe_key(&mut rt.store, wit_key).map(|_| ())?)
            });
        });

        // observe_mouse → on_observe_mouse (gated by INPUT_HANDLER capability)
        let shared = Arc::clone(&self.shared);
        r.on_observe_mouse(move |_state, event, app| {
            if !shared
                .cached_capabilities
                .contains(PluginCapabilities::INPUT_HANDLER)
            {
                return;
            }
            shared.call_synced(app, "observe_mouse", |rt| {
                let api = rt.instance.kasane_plugin_plugin_api();
                let wit_event = convert::mouse_event_to_wit(event);
                Ok(api
                    .call_observe_mouse(&mut rt.store, wit_event)
                    .map(|_| ())?)
            });
        });

        // observe_drop → on_observe_drop (gated by DROP_HANDLER capability)
        let shared = Arc::clone(&self.shared);
        r.on_observe_drop(move |_state, event, app| {
            if !shared
                .cached_capabilities
                .contains(PluginCapabilities::DROP_HANDLER)
            {
                return;
            }
            shared.call_synced(app, "observe_drop", |rt| {
                let api = rt.instance.kasane_plugin_plugin_api();
                let wit_event = convert::drop_event_to_wit(event);
                Ok(api
                    .call_observe_drop(&mut rt.store, &wit_event)
                    .map(|_| ())?)
            });
        });

        // ---- β-3.3b.3 — Input handlers ----
        // WIT exports: handle-key / handle-key-middleware / handle-mouse /
        // handle-drop / handle-default-scroll. There is no
        // `handle-text-input` WIT export; the corresponding `on_text_input`
        // registry slot stays empty for WASM plugins.

        // handle_key → on_key
        let shared = Arc::clone(&self.shared);
        r.on_key(move |_state, key, app| {
            let shared_for_call = Arc::clone(&shared);
            shared.with_runtime(|runtime| {
                host::sync_from_app_state(
                    runtime.store.data_mut(),
                    app.as_app_state(),
                    shared_for_call.cached_view_deps,
                );
                let api = runtime.instance.kasane_plugin_plugin_api();
                let wit_key = convert::key_event_to_wit(key);
                let result = match api.call_handle_key(&mut runtime.store, wit_key) {
                    Ok(Some(cmds)) => Some(shared_for_call.convert_commands(&cmds)),
                    Ok(None) => None,
                    Err(e) => {
                        tracing::error!(
                            "WASM plugin {}.handle_key failed: {e}",
                            shared_for_call.plugin_id.0
                        );
                        return None;
                    }
                };

                if result.is_some()
                    && let Ok(h) = api.call_state_hash(&mut runtime.store)
                {
                    shared_for_call.set_state_hash(h);
                }

                result.map(|cmds| ((), cmds))
            })
        });

        // handle_key_middleware → on_key_middleware
        let shared = Arc::clone(&self.shared);
        r.on_key_middleware(move |_state, key, app| {
            let result = shared.call_synced_with_hash_arc(
                app,
                "handle_key_middleware",
                |s, rt| {
                    let api = rt.instance.kasane_plugin_plugin_api();
                    let wit_key = convert::key_event_to_wit(key);
                    let wit_result = api.call_handle_key_middleware(&mut rt.store, wit_key)?;
                    Ok(match wit_result {
                        wit::KeyHandleResult::Consumed(commands) => {
                            KeyHandleResult::Consumed(s.convert_commands(&commands))
                        }
                        wit::KeyHandleResult::Transformed(next_key) => {
                            match convert::wit_key_event_to_key_event(&next_key) {
                                Ok(next_key) => KeyHandleResult::Transformed(next_key),
                                Err(error) => {
                                    tracing::error!(
                                        "WASM plugin {}.handle_key_middleware returned invalid key: {error}",
                                        s.plugin_id.0
                                    );
                                    KeyHandleResult::Passthrough
                                }
                            }
                        }
                        wit::KeyHandleResult::Passthrough => KeyHandleResult::Passthrough,
                    })
                },
            );
            ((), result)
        });

        // handle_mouse → on_handle_mouse (matches trait path: call_synced
        // without hash update; per-click state propagation rides on the
        // returned Effects/commands cycle).
        let shared = Arc::clone(&self.shared);
        r.on_handle_mouse(move |_state, event, id, app| {
            let shared_for_call = Arc::clone(&shared);
            shared
                .call_synced(app, "handle_mouse", |rt| {
                    let api = rt.instance.kasane_plugin_plugin_api();
                    let wit_event = convert::mouse_event_to_wit(event);
                    Ok(api
                        .call_handle_mouse(&mut rt.store, wit_event, id.local)
                        .map(|opt| opt.map(|cmds| shared_for_call.convert_commands(&cmds)))?)
                })
                .map(|cmds| ((), cmds))
        });

        // handle_drop → on_handle_drop (matches trait path: call_synced without
        // hash update).
        let shared = Arc::clone(&self.shared);
        r.on_handle_drop(move |_state, event, id, app| {
            let shared_for_call = Arc::clone(&shared);
            shared
                .call_synced(app, "handle_drop", |rt| {
                    let api = rt.instance.kasane_plugin_plugin_api();
                    let wit_event = convert::drop_event_to_wit(event);
                    Ok(api
                        .call_handle_drop(&mut rt.store, &wit_event, id.local)
                        .map(|opt| opt.map(|cmds| shared_for_call.convert_commands(&cmds)))?)
                })
                .map(|cmds| ((), cmds))
        });

        // handle_default_scroll → on_default_scroll
        let shared = Arc::clone(&self.shared);
        r.on_default_scroll(move |_state, candidate, app| {
            let shared_for_call = Arc::clone(&shared);
            shared.with_runtime(|runtime| {
                host::sync_from_app_state(
                    runtime.store.data_mut(),
                    app.as_app_state(),
                    shared_for_call.cached_view_deps,
                );
                let api = runtime.instance.kasane_plugin_plugin_api();
                let wit_candidate = convert::default_scroll_candidate_to_wit(&candidate);
                let result = match api.call_handle_default_scroll(&mut runtime.store, wit_candidate)
                {
                    Ok(Some(result)) => Some(convert::wit_scroll_policy_result_to_result(&result)),
                    Ok(None) => None,
                    Err(e) => {
                        tracing::error!(
                            "WASM plugin {}.handle_default_scroll failed: {e}",
                            shared_for_call.plugin_id.0
                        );
                        return None;
                    }
                };

                if result.is_some()
                    && let Ok(h) = api.call_state_hash(&mut runtime.store)
                {
                    shared_for_call.set_state_hash(h);
                }

                result.map(|res| ((), res))
            })
        });

        // ---- β-3.3b.4 — Input dispatch helpers ----
        // declare_key_map: install the WIT-built CompiledKeyMap directly.
        // The native plugin path uses the in-process KeyMapBuilder DSL;
        // WASM plugins compile groups + bindings out-of-process via the
        // `declare-key-map` WIT export, which `WasmPlugin::new*` already
        // converted into `self.key_map` at load time.
        if let Some(map) = self.key_map.clone() {
            r.declare_key_map(map);
        }

        // refresh_key_groups → on_group_refresh
        let shared = Arc::clone(&self.shared);
        r.on_group_refresh(move |_state, app, map| {
            for group in &mut map.groups {
                let name = group.name.to_string();
                let active = shared.call_synced(app, "is_group_active", |rt| {
                    let api = rt.instance.kasane_plugin_plugin_api();
                    Ok(api.call_is_group_active(&mut rt.store, &name)?)
                });
                group.active = active;
            }
        });

        // invoke_action → on_action
        let shared = Arc::clone(&self.shared);
        r.on_action(move |_state, action_id, key, app| {
            let shared_for_call = Arc::clone(&shared);
            let response = shared.with_runtime(|runtime| {
                host::sync_from_app_state(
                    runtime.store.data_mut(),
                    app.as_app_state(),
                    shared_for_call.cached_view_deps,
                );
                runtime.store.data_mut().plugin_tag = shared_for_call.plugin_tag();
                let api = runtime.instance.kasane_plugin_plugin_api();
                let wit_key = convert::key_event_to_wit(key);
                let result = match api.call_invoke_action(&mut runtime.store, action_id, wit_key) {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::error!(
                            "WASM plugin {}.invoke_action failed: {e}",
                            shared_for_call.plugin_id.0
                        );
                        shared_for_call.record_diagnostic("invoke_action", &e.into());
                        return KeyResponse::Pass;
                    }
                };
                if let Ok(h) = api.call_state_hash(&mut runtime.store) {
                    shared_for_call.set_state_hash(h);
                }
                convert::wit_key_response_to_key_response(&result, &|cmds| {
                    shared_for_call.convert_commands(cmds)
                })
            });
            ((), response)
        });

        // ---- β-3.3b.5 — View / contribute / transform ----
        // contribute_to → on_contribute_any. WASM plugins delegate slot
        // routing to the `contribute-to(region, …)` WIT export, so the
        // host registers a single any-slot handler instead of one entry
        // per slot.
        let shared = Arc::clone(&self.shared);
        r.on_contribute_any(move |_state, region, app, ctx| {
            let wit_region = convert::slot_id_to_wit(region);
            shared.call_synced(app, "contribute_to", |rt| {
                rt.store.data_mut().elements.clear();
                let api = rt.instance.kasane_plugin_plugin_api();
                let wit_ctx = convert::contribute_context_to_wit(ctx);
                Ok(api
                    .call_contribute_to(&mut rt.store, &wit_region, wit_ctx)?
                    .map(|wit_contrib| {
                        let element = rt.store.data_mut().take_root_element(wit_contrib.element);
                        Contribution {
                            element,
                            priority: wit_contrib.priority,
                            size_hint: convert::wit_size_hint_to_size_hint(&wit_contrib.size_hint),
                        }
                    }))
            })
        });

        // transform_patch → on_transform(priority, …). The transform
        // priority is queried from WIT once at register() time and baked
        // into the TransformEntry; the WIT plugin's `transform-priority`
        // export is treated as static metadata, matching the native
        // PluginBridge path. The full-rewrite `transform` WIT export is
        // intentionally not migrated here — PluginBridge auto-derives
        // `transform()` by applying the registered patch to the subject,
        // so plugins implementing `transform-patch` get the same
        // observable behavior. WIT-only `transform` (without
        // `transform-patch`) loses this path on the eventual loader-flip;
        // no production plugin relies on that today.
        let priority = self.shared.with_runtime(|runtime| {
            let api = runtime.instance.kasane_plugin_plugin_api();
            api.call_transform_priority(&mut runtime.store).unwrap_or(0)
        });
        let shared = Arc::clone(&self.shared);
        r.on_transform(priority, move |_state, target, app, ctx| {
            shared.with_runtime(|runtime| {
                host::sync_from_app_state(
                    runtime.store.data_mut(),
                    app.as_app_state(),
                    shared.cached_view_deps,
                );
                runtime.store.data_mut().elements.clear();

                let api = runtime.instance.kasane_plugin_plugin_api();
                let wit_target = convert::transform_target_to_wit(target);
                let wit_ctx = convert::transform_context_to_wit(ctx);
                match api.call_transform_patch(&mut runtime.store, &wit_target, wit_ctx) {
                    Ok(ops) if ops.is_empty() => ElementPatch::Identity,
                    Ok(ops) => convert::wit_element_patch_ops_to_patch(&ops, &mut |handle| {
                        runtime.store.data_mut().take_root_element(handle)
                    }),
                    Err(e) => {
                        tracing::error!(
                            "WASM plugin {}.transform_patch failed: {e}",
                            shared.plugin_id.0
                        );
                        ElementPatch::Identity
                    }
                }
            })
        });

        // transform → on_transform_full. WIT plugins that implement the
        // imperative full-rewrite `transform` export instead of (or in
        // addition to) `transform-patch` retain that path: when the
        // patch handler resolves to `ElementPatch::Identity` the bridge
        // falls through to this closure, which round-trips the subject
        // through the WIT call.
        let shared = Arc::clone(&self.shared);
        r.on_transform_full(move |_state, target, subject, app, ctx| {
            shared.with_runtime(|runtime| {
                host::sync_from_app_state(
                    runtime.store.data_mut(),
                    app.as_app_state(),
                    shared.cached_view_deps,
                );
                runtime.store.data_mut().elements.clear();

                let wit_subject = match &subject {
                    TransformSubject::Element(el) => {
                        let handle = runtime.store.data_mut().inject_element(el.clone());
                        wit::TransformSubject::ElementS(handle)
                    }
                    TransformSubject::Overlay(overlay) => {
                        let handle = runtime
                            .store
                            .data_mut()
                            .inject_element(overlay.element.clone());
                        let wit_anchor = convert::overlay_anchor_to_wit(&overlay.anchor);
                        wit::TransformSubject::OverlayS(wit::OverlaySubject {
                            element: handle,
                            anchor: wit_anchor,
                        })
                    }
                };

                let api = runtime.instance.kasane_plugin_plugin_api();
                let wit_target = convert::transform_target_to_wit(target);
                let wit_ctx = convert::transform_context_to_wit(ctx);
                match api.call_transform(&mut runtime.store, &wit_target, &wit_subject, wit_ctx) {
                    Ok(result) => match result {
                        wit::TransformSubject::ElementS(handle) => TransformSubject::Element(
                            runtime.store.data_mut().take_root_element(handle),
                        ),
                        wit::TransformSubject::OverlayS(os) => {
                            let element = runtime.store.data_mut().take_root_element(os.element);
                            let anchor = convert::wit_overlay_anchor_to_overlay_anchor(&os.anchor);
                            TransformSubject::Overlay(kasane_core::element::Overlay {
                                element,
                                anchor,
                            })
                        }
                    },
                    Err(e) => {
                        tracing::error!("WASM plugin {}.transform failed: {e}", shared.plugin_id.0);
                        subject
                    }
                }
            })
        });

        // transform_menu_item → on_menu_transform
        let shared = Arc::clone(&self.shared);
        r.on_menu_transform(move |_state, item, index, selected, app| {
            shared.call_synced(app, "transform_menu_item", |rt| {
                let api = rt.instance.kasane_plugin_plugin_api();
                let wit_item = convert::atoms_to_wit(item);
                Ok(api
                    .call_transform_menu_item(&mut rt.store, &wit_item, index as u32, selected)
                    .map(|opt| opt.map(|t| convert::wit_atoms_to_atoms(&t)))?)
            })
        });

        // ---- β-3.3b.6 — Annotations + ornaments ----
        // annotate_line_with_ctx → on_annotate_line. The WIT
        // `annotate-line` export produces all annotation parts in one
        // call; using the monolithic registry path avoids the 5x WIT
        // round-trips a decomposed migration would incur.
        let shared = Arc::clone(&self.shared);
        r.on_annotate_line(move |_state, line, app, ctx| {
            shared.call_synced(app, "annotate_line", |rt| {
                rt.store.data_mut().elements.clear();
                let api = rt.instance.kasane_plugin_plugin_api();
                let wit_ctx = convert::annotate_context_to_wit(ctx);
                Ok(api
                    .call_annotate_line(&mut rt.store, line as u32, wit_ctx)?
                    .map(|wit_ann| {
                        let left_gutter = wit_ann
                            .left_gutter
                            .map(|h| rt.store.data_mut().take_root_element(h));
                        let right_gutter = wit_ann
                            .right_gutter
                            .map(|h| rt.store.data_mut().take_root_element(h));
                        let background = wit_ann.background.as_ref().map(|bg| BackgroundLayer {
                            style: convert::wit_style_to_style(&bg.style),
                            z_order: bg.z_order,
                            blend: BlendMode::Opaque,
                        });
                        let vt_items = wit_ann
                            .virtual_text
                            .into_iter()
                            .map(|item| VirtualTextItem {
                                atoms: item.atoms.iter().map(convert::wit_atom_to_atom).collect(),
                                priority: item.priority,
                            })
                            .collect();
                        LineAnnotation {
                            left_gutter,
                            right_gutter,
                            background,
                            priority: wit_ann.priority,
                            inline: wit_ann.inline.map(|wit_inline| {
                                convert::wit_inline_decoration_to_inline_decoration(&wit_inline)
                            }),
                            virtual_text: vt_items,
                        }
                    }))
            })
        });

        // render_ornaments → on_render_ornament
        let shared = Arc::clone(&self.shared);
        r.on_render_ornament(move |_state, app, ctx| {
            shared.call_synced(app, "render_ornaments", |rt| {
                let api = rt.instance.kasane_plugin_plugin_api();
                let wit_ctx = convert::render_ornament_context_to_wit(ctx);
                Ok(convert::wit_ornament_batch_to_ornament_batch(
                    &api.call_render_ornaments(&mut rt.store, wit_ctx)?,
                ))
            })
        });

        // paint_inline_box → on_paint_inline_box
        let shared = Arc::clone(&self.shared);
        r.on_paint_inline_box(move |_state, box_id, app| {
            shared.call_synced(app, "paint_inline_box", |rt| -> anyhow::Result<_> {
                rt.store.data_mut().elements.clear();
                let api = rt.instance.kasane_plugin_plugin_api();
                let handle = api.call_paint_inline_box(&mut rt.store, box_id)?;
                Ok(handle.map(|h| rt.store.data_mut().take_root_element(h)))
            })
        });

        // ---- β-3.3b.7 — Display + projections ----
        // display_directives → on_display. Always registered; the WIT
        // call falls back to an empty Vec via `call_synced` if the plugin
        // doesn't implement the `display-directives` export.
        let shared = Arc::clone(&self.shared);
        r.on_display(move |_state, app| {
            shared.call_synced(app, "display_directives", |rt| {
                let api = rt.instance.kasane_plugin_plugin_api();
                let wit_directives = api.call_display_directives(&mut rt.store)?;
                let plugin_tag = rt.store.data().plugin_tag;
                Ok(convert::wit_display_directives_to_directives_with_resolver(
                    &wit_directives,
                    plugin_tag,
                    &mut |handle| rt.store.data_mut().take_element(handle),
                ))
            })
        });

        // unified_display → on_unified_display. Only registered when the
        // WIT plugin exports the unified `display` function (probed at
        // construction time and cached on `WasmPluginShared`). Skipping
        // this when the export is absent matches the trait method's
        // `has_unified_display = false` behavior so collection.rs takes
        // the separate-display path instead.
        if self.shared.has_unified_display_export {
            let shared = Arc::clone(&self.shared);
            r.on_unified_display(move |_state, app| {
                shared.call_synced(app, "display", |rt| {
                    let api = rt.instance.kasane_plugin_plugin_api();
                    let wit_directives = api.call_display(&mut rt.store)?;
                    let plugin_tag = rt.store.data().plugin_tag;
                    Ok(convert::wit_display_directives_to_directives_with_resolver(
                        &wit_directives,
                        plugin_tag,
                        &mut |handle| rt.store.data_mut().take_element(handle),
                    ))
                })
            });
        }

        // projection_descriptors + projection_directives → one
        // `define_projection` per cached descriptor. WasmPlugin caches
        // the descriptor list at construction time (from the
        // `declare-projections` WIT export), so the registry sees the
        // same set the trait method's `projection_descriptors()` exposed.
        for descriptor in &self.cached_projection_descriptors {
            let shared = Arc::clone(&self.shared);
            let id_str = descriptor.id.0.to_string();
            r.define_projection(descriptor.clone(), move |_state, app| {
                shared.call_synced(app, "projection_directives", |rt| {
                    let api = rt.instance.kasane_plugin_plugin_api();
                    let wit_directives = api.call_projection_directives(&mut rt.store, &id_str)?;
                    let plugin_tag = rt.store.data().plugin_tag;
                    Ok(convert::wit_display_directives_to_directives_with_resolver(
                        &wit_directives,
                        plugin_tag,
                        &mut |handle| rt.store.data_mut().take_element(handle),
                    ))
                })
            });
        }

        // display_directive_priority is always 0 in the trait impl
        // (the WIT export does not exist yet); the registry's
        // `display_priority` field defaults to 0, so no explicit
        // `declare_display_priority` call is needed.

        // ---- β-3.3b.8 — Navigation + overlay + edit intercept ----

        // navigation_policy → on_navigation_policy. Gated by the
        // NAVIGATION_POLICY capability so plugins that don't implement
        // the WIT export skip registration entirely; PluginBridge's
        // `navigation_policy` returns None when the handler is absent,
        // matching the trait method's early-return.
        if self
            .shared
            .cached_capabilities
            .contains(PluginCapabilities::NAVIGATION_POLICY)
        {
            let shared = Arc::clone(&self.shared);
            r.on_navigation_policy(move |_state, unit| {
                let wit_unit = convert::display_unit_to_wit(unit);
                shared.with_runtime(|runtime| {
                    let api = runtime.instance.kasane_plugin_plugin_api();
                    match api.call_navigation_policy(&mut runtime.store, wit_unit) {
                        Ok(kind) => convert::wit_navigation_policy_to_policy(kind),
                        Err(e) => {
                            tracing::error!(
                                "WASM plugin {}.navigation_policy failed: {e}",
                                shared.plugin_id.0
                            );
                            // Trait method returns None on error and the
                            // bridge collapses absence to "no opinion";
                            // emit `Normal` as the fallback policy so the
                            // semantics line up (any registered handler
                            // produces some answer, never None).
                            kasane_core::display::navigation::NavigationPolicy::Normal
                        }
                    }
                })
            });
        }

        // navigation_action → on_navigation_action. Gated by the
        // NAVIGATION_ACTION capability. The closure returns the raw
        // ActionResult; PluginBridge::navigation_action collapses
        // Pass to None, matching the trait method.
        if self
            .shared
            .cached_capabilities
            .contains(PluginCapabilities::NAVIGATION_ACTION)
        {
            let shared = Arc::clone(&self.shared);
            r.on_navigation_action(move |_state, unit, action| {
                let wit_unit = convert::display_unit_to_wit(unit);
                let action_kind = convert::navigation_action_to_wit_kind(&action);
                let result = shared.with_runtime(|runtime| {
                    let api = runtime.instance.kasane_plugin_plugin_api();
                    match api.call_on_navigation_action(&mut runtime.store, wit_unit, action_kind) {
                        Ok(result) => convert::wit_action_result_to_action_result(result),
                        Err(e) => {
                            tracing::error!(
                                "WASM plugin {}.on_navigation_action failed: {e}",
                                shared.plugin_id.0
                            );
                            kasane_core::display::navigation::ActionResult::Pass
                        }
                    }
                });
                ((), result)
            });
        }

        // contribute_overlay_with_ctx → on_overlay. The trait method
        // built an OverlayContribution with `plugin_id: PluginId::EMPTY`
        // because the bridge later fills in the owning plugin's id from
        // the dispatch context; preserve that contract here.
        let shared = Arc::clone(&self.shared);
        r.on_overlay(move |_state, app, ctx| {
            shared.call_synced(app, "contribute_overlay_v2", |rt| {
                rt.store.data_mut().elements.clear();
                let api = rt.instance.kasane_plugin_plugin_api();
                let wit_ctx = convert::overlay_context_to_wit(ctx);
                Ok(api
                    .call_contribute_overlay_v2(&mut rt.store, &wit_ctx)?
                    .map(|wit_oc| {
                        let element = rt.store.data_mut().take_root_element(wit_oc.element);
                        let anchor = convert::wit_overlay_anchor_to_overlay_anchor(&wit_oc.anchor);
                        OverlayContribution {
                            element,
                            anchor,
                            z_index: wit_oc.z_index,
                            plugin_id: PluginId::from(""),
                        }
                    }))
            })
        });

        // intercept_buffer_edit → on_buffer_edit_intercept. The trait
        // method's `call_synced` falls back to `BufferEditVerdict::default()`
        // (= PassThrough) on trap or missing export; the closure mirrors
        // that semantics.
        let shared = Arc::clone(&self.shared);
        r.on_buffer_edit_intercept(move |_state, edit, app| {
            use kasane_core::state::shadow_cursor::BufferEditVerdict;
            let wit_edit = convert::buffer_edit_to_wit(edit);
            let verdict = shared.call_synced(
                app,
                "intercept_buffer_edit",
                |rt| -> anyhow::Result<BufferEditVerdict> {
                    let api = rt.instance.kasane_plugin_plugin_api();
                    let wit_verdict = api.call_intercept_buffer_edit(&mut rt.store, &wit_edit)?;
                    Ok(convert::wit_shadow_edit_verdict_to_native(wit_verdict))
                },
            );
            ((), verdict)
        });

        // ---- β-3.3b.9 — Persistence + workspace ----
        // persist_state → on_persist_state. Empty WIT response and
        // trap return None, matching the trait method's `Ok(data) if
        // !data.is_empty()` filter.
        let shared = Arc::clone(&self.shared);
        r.on_persist_state(move |_state| {
            shared.with_runtime(|runtime| {
                let api = runtime.instance.kasane_plugin_plugin_api();
                match api.call_persist_state(&mut runtime.store) {
                    Ok(data) if !data.is_empty() => Some(data),
                    Ok(_) => None,
                    Err(e) => {
                        tracing::warn!(
                            "WASM plugin {}.persist_state failed: {e}",
                            shared.plugin_id.0
                        );
                        None
                    }
                }
            })
        });

        // restore_state → on_restore_state.
        let shared = Arc::clone(&self.shared);
        r.on_restore_state(move |_state, data| {
            shared.with_runtime(|runtime| {
                let api = runtime.instance.kasane_plugin_plugin_api();
                match api.call_restore_state(&mut runtime.store, data) {
                    Ok(success) => success,
                    Err(e) => {
                        tracing::warn!(
                            "WASM plugin {}.restore_state failed: {e}",
                            shared.plugin_id.0
                        );
                        false
                    }
                }
            })
        });

        // surfaces → declare_surfaces. The factory queries the WIT
        // `surfaces` export each time (matching the trait method's
        // per-call WIT round-trip); the host invokes the factory during
        // bootstrap preflight, so the WIT call happens once during
        // workspace materialization.
        let shared = Arc::clone(&self.shared);
        r.declare_surfaces(move |_state| {
            let shared_for_surfaces = Arc::clone(&shared);
            shared.with_runtime(|runtime| {
                let api = runtime.instance.kasane_plugin_plugin_api();
                match api.call_surfaces(&mut runtime.store) {
                    Ok(descriptors) => descriptors
                        .into_iter()
                        .map(|descriptor| {
                            let initial_placement = descriptor
                                .initial_placement
                                .as_ref()
                                .map(convert::wit_surface_placement_to_request);
                            shared_for_surfaces.hosted_surface(
                                descriptor.surface_key,
                                descriptor.size_hint,
                                descriptor.declared_slots,
                                initial_placement,
                            )
                        })
                        .collect(),
                    Err(e) => {
                        tracing::error!(
                            "WASM plugin {}.surfaces failed: {e}",
                            shared_for_surfaces.plugin_id.0
                        );
                        vec![]
                    }
                }
            })
        });

        // workspace_request: WasmPlugin does not override the trait
        // default (`None`), so no `declare_workspace_request` call is
        // needed. The cap remains `None` once the loader flips.

        // ---- β-3.3b.10 — Process tasks + pubsub + lens ----

        // update_effects → on_update_tier2. Closure downcasts the
        // `&mut dyn Any` message to `Vec<u8>` (the WIT cross-boundary
        // shape) and discards anything else with a warning, matching
        // the trait method.
        let shared = Arc::clone(&self.shared);
        r.on_update_tier2(move |_state, msg, app| {
            let effects = if let Some(bytes) = msg.downcast_ref::<Vec<u8>>() {
                let bytes = bytes.clone();
                shared.call_synced_with_hash_arc(app, "update_effects", move |s, rt| {
                    let api = rt.instance.kasane_plugin_plugin_api();
                    Ok(s.convert_process_capable_effects_typed(
                        &api.call_update_effects(&mut rt.store, &bytes)?,
                    ))
                })
            } else {
                tracing::warn!(
                    "WASM plugin {} received non-byte message, ignoring typed update_effects",
                    shared.plugin_id.0
                );
                kasane_core::plugin::ProcessCapableEffects::none()
            };
            ((), effects)
        });

        // collect_publications → publish_raw per topic in publish_topics.
        // The closure clones the WIT topic name and calls
        // `publish-value(topic) -> option<channel-value>` per frame.
        for topic_str in &self.shared.publish_topics {
            let shared = Arc::clone(&self.shared);
            let topic = kasane_core::plugin::TopicId::new(topic_str.clone());
            let wit_topic = topic_str.clone();
            r.publish_raw(topic, move |_state, app| {
                shared.call_synced(app, "publish_value", |rt| {
                    let api = rt.instance.kasane_plugin_plugin_api();
                    Ok(api
                        .call_publish_value(&mut rt.store, &wit_topic)?
                        .map(|wv| convert::wit_channel_value_to_core(&wv)))
                })
            });
        }

        // deliver_subscriptions → subscribe_raw + on_subscription. The
        // bridge's `deliver_subscriptions` iterates `subscribers` to
        // know which topics this plugin observes and dispatches the
        // batch through `subscription_handler` per topic. WasmPlugin's
        // dispatch is the WIT `on-subscription(topic, values)` export;
        // we register one `subscribe_raw` per declared topic to declare
        // interest, then a single `on_subscription` that routes the
        // matching topic batch into the WIT call.
        if !self.shared.subscribe_topics.is_empty() {
            for topic_str in &self.shared.subscribe_topics {
                let topic = kasane_core::plugin::TopicId::new(topic_str.clone());
                r.subscribe_raw(topic);
            }
            let shared = Arc::clone(&self.shared);
            r.on_subscription(move |_state, topic, values, _app| {
                let wit_values: Vec<_> = values.iter().map(convert::channel_value_to_wit).collect();
                if wit_values.is_empty() {
                    return ((), kasane_core::plugin::Effects::default());
                }
                let wit_topic = topic.to_string();
                let shared_for_call = Arc::clone(&shared);
                let effects = shared.with_runtime(|runtime| {
                    let api = runtime.instance.kasane_plugin_plugin_api();
                    match api.call_on_subscription(&mut runtime.store, &wit_topic, &wit_values) {
                        Ok(eff) => {
                            let converted: kasane_core::plugin::Effects = shared_for_call
                                .convert_kakoune_side_effects_typed(&eff)
                                .into();
                            if let Ok(h) = api.call_state_hash(&mut runtime.store) {
                                shared_for_call.set_state_hash(h);
                            }
                            converted
                        }
                        Err(e) => {
                            tracing::error!(
                                "WASM plugin {}.on_subscription failed: {e}",
                                shared_for_call.plugin_id.0
                            );
                            kasane_core::plugin::Effects::default()
                        }
                    }
                });
                ((), effects)
            });
        }

        // on_command_error_effects → on_command_error.
        let shared = Arc::clone(&self.shared);
        r.on_command_error(move |_state, error, app| {
            let wit_error = convert::plugin_error_event_to_wit(error);
            let effects: kasane_core::plugin::KakouneSideEffects = shared
                .call_synced_with_hash_arc(app, "on_command_error_effects", move |s, rt| {
                    let api = rt.instance.kasane_plugin_plugin_api();
                    Ok(s.convert_kakoune_side_effects_typed(
                        &api.call_on_command_error_effects(&mut rt.store, &wit_error)?,
                    ))
                });
            ((), kasane_core::plugin::Effects::from(effects))
        });

        // register_lenses → declare_lenses. The factory queries the WIT
        // `declare-lenses` export each invocation and constructs one
        // WasmLensAdapter per declaration; the host's lens-sync step
        // calls the factory once per registration phase, matching the
        // trait method's `register_lenses_into` shape.
        let shared = Arc::clone(&self.shared);
        r.declare_lenses(move || {
            let declarations = shared.with_runtime(|rt| {
                match rt
                    .instance
                    .kasane_plugin_plugin_api()
                    .call_declare_lenses(&mut rt.store)
                {
                    Ok(decls) => decls,
                    Err(e) => {
                        tracing::error!(
                            "WASM plugin {}.declare_lenses failed: {e}",
                            shared.plugin_id.0
                        );
                        Vec::new()
                    }
                }
            });
            declarations
                .into_iter()
                .map(|wit_decl| {
                    let decl = convert::wit_lens_declaration_to_native(&wit_decl);
                    Arc::new(WasmLensAdapter {
                        shared: Arc::clone(&shared),
                        declaration: decl,
                    }) as Arc<dyn kasane_core::lens::Lens>
                })
                .collect()
        });

        // ---- β-3.3b.11 — Static metadata + cleanup ----
        // The WIT-supplied capabilities and capability descriptor often
        // diverge from what the host can infer from registered handlers
        // (e.g. WASM plugins always register `on_io_event_tier2` even
        // when the WIT module never exports `on-io-event-effects`, so
        // auto-inference would over-report `IO_HANDLER`). The override
        // setters preserve the trait method's exact-cap semantics.
        r.declare_authorities(self.shared.authorities);
        r.declare_capabilities(self.shared.cached_capabilities);
        if !self.shared.process_allowed {
            r.deny_process_spawn();
        }
        if let Some(descriptor) = self.shared.manifest_descriptor.clone() {
            r.declare_capability_descriptor(descriptor);
        }
        let shared = Arc::clone(&self.shared);
        r.declare_state_hash(move || shared.state_hash());

        // id, set_plugin_tag, drain_diagnostics, state_hash are
        // bridge-internal: PluginBridge derives id from `Plugin::id()`
        // (already provided above), maintains its own plugin_tag /
        // pending_diagnostics / generation counter, so no register-time
        // wiring is needed. WasmPlugin's WIT-side state hash continues
        // to ride on the closure side via `call_synced_with_hash`; the
        // bridge's generation counter advances independently as the
        // (unit) state changes (it never does for WasmPlugin), but the
        // change-detection hashes from the WIT calls keep the per-frame
        // staleness signal accurate through the same machinery the trait
        // method relied on.
    }
}
