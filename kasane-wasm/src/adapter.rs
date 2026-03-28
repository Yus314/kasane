//! WASM adapter: bridges wasmtime Component Model guests to the `PluginBackend` trait.

use std::any::Any;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use compact_str::CompactString;
use kasane_core::element::{Element, InteractiveId, PluginTag};
use kasane_core::input::{CompiledKeyMap, KeyEvent, KeyResponse, MouseEvent};
use kasane_core::plugin::{
    AnnotateContext, AppView, BackgroundLayer, BlendMode, Command, ContributeContext, Contribution,
    DisplayDirective, Effects, ElementPatch, IoEvent, KeyHandleResult, LineAnnotation,
    OverlayContext, OverlayContribution, PluginAuthorities, PluginBackend, PluginCapabilities,
    PluginDiagnostic, PluginId, SlotId, TransformContext, TransformSubject, TransformTarget,
    VirtualTextItem,
};
use kasane_core::protocol::Atom;
use kasane_core::scroll::{DefaultScrollCandidate, ScrollPolicyResult};
use kasane_core::state::DirtyFlags;
use kasane_core::surface::{
    EventContext, SizeHint, SlotDeclaration, Surface, SurfaceEvent, SurfaceId,
    SurfacePlacementRequest, ViewContext,
};
use kasane_core::workspace::WorkspaceQuery;

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

struct WasmPluginShared {
    runtime: Mutex<WasmPluginRuntime>,
    plugin_id: PluginId,
    plugin_tag: Mutex<PluginTag>,
    cached_state_hash: AtomicU64,
    cached_view_deps: DirtyFlags,
    cached_capabilities: PluginCapabilities,
    process_allowed: bool,
    authorities: PluginAuthorities,
    pending_diagnostics: Mutex<Vec<PluginDiagnostic>>,
}

impl WasmPluginShared {
    fn with_runtime<R>(&self, f: impl FnOnce(&mut WasmPluginRuntime) -> R) -> R {
        let mut runtime = self.runtime.lock().expect("wasm runtime poisoned");
        f(&mut runtime)
    }

    fn record_diagnostic(&self, method: &str, error: &anyhow::Error) {
        let diag = PluginDiagnostic::runtime_error(
            self.plugin_id.clone(),
            method.to_string(),
            error.to_string(),
        );
        let mut pending = self
            .pending_diagnostics
            .lock()
            .expect("diagnostics poisoned");
        if pending.len() >= MAX_PENDING_DIAGNOSTICS {
            pending.remove(0);
        }
        pending.push(diag);
    }

    /// Lock runtime, sync state, call function, log error on failure.
    fn call_synced<R: Default>(
        &self,
        state: &AppView<'_>,
        method: &str,
        f: impl FnOnce(&mut WasmPluginRuntime) -> anyhow::Result<R>,
    ) -> R {
        self.with_runtime(|runtime| {
            host::sync_from_app_state(runtime.store.data_mut(), state.as_app_state());
            runtime.store.data_mut().plugin_tag = *self.plugin_tag.lock().expect("plugin_tag lock");
            match f(runtime) {
                Ok(result) => result,
                Err(e) => {
                    tracing::error!("WASM plugin {}.{method} failed: {e}", self.plugin_id.0);
                    self.record_diagnostic(method, &e);
                    R::default()
                }
            }
        })
    }

    /// Like call_synced but also updates the cached state hash afterwards.
    fn call_synced_with_hash<R: Default>(
        &self,
        state: &AppView<'_>,
        method: &str,
        f: impl FnOnce(&mut WasmPluginRuntime) -> anyhow::Result<R>,
    ) -> R {
        self.with_runtime(|runtime| {
            host::sync_from_app_state(runtime.store.data_mut(), state.as_app_state());
            runtime.store.data_mut().plugin_tag = *self.plugin_tag.lock().expect("plugin_tag lock");
            let result = match f(runtime) {
                Ok(result) => result,
                Err(e) => {
                    tracing::error!("WASM plugin {}.{method} failed: {e}", self.plugin_id.0);
                    self.record_diagnostic(method, &e);
                    return R::default();
                }
            };
            let plugin_api = runtime.instance.kasane_plugin_plugin_api();
            if let Ok(h) = plugin_api.call_state_hash(&mut runtime.store) {
                self.set_state_hash(h);
            }
            result
        })
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
            _ => vec![convert::wit_command_to_command(command)],
        }
    }

    fn convert_commands(self: &Arc<Self>, commands: &[wit::Command]) -> Vec<Command> {
        commands
            .iter()
            .flat_map(|command| self.convert_command(command))
            .collect()
    }

    fn convert_runtime_effects(self: &Arc<Self>, effects: &wit::RuntimeEffects) -> Effects {
        let shared = Arc::clone(self);
        convert::wit_runtime_effects_to_effects_with(effects, move |command| {
            shared.convert_command(command)
        })
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
            host::sync_from_app_state(runtime.store.data_mut(), ctx.state);
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
            host::sync_from_app_state(runtime.store.data_mut(), _ctx.state);
            runtime.store.data_mut().focused = _ctx.focused;
            let plugin_api = runtime.instance.kasane_plugin_plugin_api();
            let wit_event = convert::surface_event_to_wit(&_event);
            let wit_ctx = convert::surface_event_context_to_wit(_ctx);
            match plugin_api.call_handle_surface_event(
                &mut runtime.store,
                &surface_key,
                wit_event,
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
            host::sync_from_app_state(runtime.store.data_mut(), state);
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

/// A WASM Component Model plugin adapted to the native Plugin trait.
pub struct WasmPlugin {
    shared: Arc<WasmPluginShared>,
    /// Compiled key map built from `declare-key-map` at construction time.
    key_map: Option<CompiledKeyMap>,
}

impl WasmPlugin {
    pub(crate) fn new(
        mut store: wasmtime::Store<HostState>,
        instance: bindings::KasanePlugin,
        id: String,
        process_allowed: bool,
        authorities: PluginAuthorities,
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

        Self {
            shared: Arc::new(WasmPluginShared {
                runtime: Mutex::new(WasmPluginRuntime { store, instance }),
                plugin_id: PluginId(id),
                plugin_tag: Mutex::new(PluginTag::UNASSIGNED),
                cached_state_hash: AtomicU64::new(0),
                cached_view_deps,
                cached_capabilities,
                process_allowed,
                authorities,
                pending_diagnostics: Mutex::new(Vec::new()),
            }),
            key_map,
        }
    }
}

impl PluginBackend for WasmPlugin {
    fn id(&self) -> PluginId {
        self.shared.plugin_id.clone()
    }

    fn set_plugin_tag(&mut self, tag: PluginTag) {
        *self.shared.plugin_tag.lock().expect("plugin_tag lock") = tag;
    }

    fn view_deps(&self) -> DirtyFlags {
        self.shared.cached_view_deps
    }

    fn drain_diagnostics(&mut self) -> Vec<PluginDiagnostic> {
        let mut pending = self
            .shared
            .pending_diagnostics
            .lock()
            .expect("diagnostics poisoned");
        std::mem::take(&mut *pending)
    }

    fn on_init_effects(&mut self, state: &AppView<'_>) -> Effects {
        self.shared
            .call_synced_with_hash(state, "on_init_effects", |rt| {
                let api = rt.instance.kasane_plugin_plugin_api();
                Ok(convert::wit_bootstrap_effects_to_effects(
                    &api.call_on_init_effects(&mut rt.store)?,
                ))
            })
    }

    fn on_shutdown(&mut self) {
        self.shared.with_runtime(|runtime| {
            let plugin_api = runtime.instance.kasane_plugin_plugin_api();
            if let Err(e) = plugin_api.call_on_shutdown(&mut runtime.store) {
                tracing::error!(
                    "WASM plugin {}.on_shutdown failed: {e}",
                    self.shared.plugin_id.0
                );
            }
        });
    }

    fn on_active_session_ready_effects(&mut self, state: &AppView<'_>) -> Effects {
        self.shared
            .call_synced_with_hash(state, "on_active_session_ready_effects", |rt| {
                let api = rt.instance.kasane_plugin_plugin_api();
                Ok(convert::wit_session_ready_effects_to_effects(
                    &api.call_on_active_session_ready_effects(&mut rt.store)?,
                ))
            })
    }

    fn on_state_changed_effects(&mut self, state: &AppView<'_>, dirty: DirtyFlags) -> Effects {
        let shared = Arc::clone(&self.shared);
        self.shared
            .call_synced_with_hash(state, "on_state_changed_effects", |rt| {
                let api = rt.instance.kasane_plugin_plugin_api();
                Ok(shared.convert_runtime_effects(
                    &api.call_on_state_changed_effects(&mut rt.store, dirty.bits())?,
                ))
            })
    }

    fn on_io_event_effects(&mut self, event: &IoEvent, state: &AppView<'_>) -> Effects {
        let shared = Arc::clone(&self.shared);
        self.shared
            .call_synced_with_hash(state, "on_io_event_effects", |rt| {
                let api = rt.instance.kasane_plugin_plugin_api();
                let wit_event = convert::io_event_to_wit(event);
                Ok(shared.convert_runtime_effects(
                    &api.call_on_io_event_effects(&mut rt.store, &wit_event)?,
                ))
            })
    }

    fn on_workspace_changed(&mut self, query: &WorkspaceQuery<'_>) {
        let snapshot = convert::workspace_query_to_snapshot(query);
        self.shared.with_runtime(|runtime| {
            let plugin_api = runtime.instance.kasane_plugin_plugin_api();
            if let Err(e) = plugin_api.call_on_workspace_changed(&mut runtime.store, &snapshot) {
                tracing::error!(
                    "WASM plugin {}.on_workspace_changed failed: {e}",
                    self.shared.plugin_id.0
                );
                return;
            }
            if let Ok(hash) = plugin_api.call_state_hash(&mut runtime.store) {
                self.shared.set_state_hash(hash);
            }
        });
    }

    fn state_hash(&self) -> u64 {
        self.shared.state_hash()
    }

    fn observe_key(&mut self, key: &KeyEvent, state: &AppView<'_>) {
        self.shared.call_synced(state, "observe_key", |rt| {
            let api = rt.instance.kasane_plugin_plugin_api();
            let wit_key = convert::key_event_to_wit(key);
            Ok(api.call_observe_key(&mut rt.store, wit_key).map(|_| ())?)
        });
    }

    fn observe_mouse(&mut self, event: &MouseEvent, state: &AppView<'_>) {
        self.shared.call_synced(state, "observe_mouse", |rt| {
            let api = rt.instance.kasane_plugin_plugin_api();
            let wit_event = convert::mouse_event_to_wit(event);
            Ok(api
                .call_observe_mouse(&mut rt.store, wit_event)
                .map(|_| ())?)
        });
    }

    fn handle_key(&mut self, key: &KeyEvent, state: &AppView<'_>) -> Option<Vec<Command>> {
        let shared = Arc::clone(&self.shared);
        self.shared.with_runtime(|runtime| {
            host::sync_from_app_state(runtime.store.data_mut(), state.as_app_state());
            let plugin_api = runtime.instance.kasane_plugin_plugin_api();
            let wit_key = convert::key_event_to_wit(key);
            let result = match plugin_api.call_handle_key(&mut runtime.store, wit_key) {
                Ok(Some(cmds)) => Some(shared.convert_commands(&cmds)),
                Ok(None) => None,
                Err(e) => {
                    tracing::error!(
                        "WASM plugin {}.handle_key failed: {e}",
                        self.shared.plugin_id.0
                    );
                    return None;
                }
            };

            if result.is_some()
                && let Ok(h) = plugin_api.call_state_hash(&mut runtime.store)
            {
                self.shared.set_state_hash(h);
            }

            result
        })
    }

    fn handle_key_middleware(&mut self, key: &KeyEvent, state: &AppView<'_>) -> KeyHandleResult {
        let shared = Arc::clone(&self.shared);
        self.shared
            .call_synced_with_hash(state, "handle_key_middleware", |rt| {
                let api = rt.instance.kasane_plugin_plugin_api();
                let wit_key = convert::key_event_to_wit(key);
                let result = api.call_handle_key_middleware(&mut rt.store, wit_key)?;
                Ok(match result {
                    wit::KeyHandleResult::Consumed(commands) => {
                        KeyHandleResult::Consumed(shared.convert_commands(&commands))
                    }
                    wit::KeyHandleResult::Transformed(next_key) => {
                        match convert::wit_key_event_to_key_event(&next_key) {
                            Ok(next_key) => KeyHandleResult::Transformed(next_key),
                            Err(error) => {
                                tracing::error!(
                                    "WASM plugin {}.handle_key_middleware returned invalid key: {error}",
                                    shared.plugin_id.0
                                );
                                KeyHandleResult::Passthrough
                            }
                        }
                    }
                    wit::KeyHandleResult::Passthrough => KeyHandleResult::Passthrough,
                })
            })
    }

    fn compiled_key_map(&self) -> Option<&CompiledKeyMap> {
        self.key_map.as_ref()
    }

    fn invoke_action(
        &mut self,
        action_id: &str,
        key: &KeyEvent,
        state: &AppView<'_>,
    ) -> KeyResponse {
        let shared = Arc::clone(&self.shared);
        self.shared.with_runtime(|runtime| {
            host::sync_from_app_state(runtime.store.data_mut(), state.as_app_state());
            runtime.store.data_mut().plugin_tag =
                *shared.plugin_tag.lock().expect("plugin_tag lock");
            let api = runtime.instance.kasane_plugin_plugin_api();
            let wit_key = convert::key_event_to_wit(key);
            let result = match api.call_invoke_action(&mut runtime.store, action_id, wit_key) {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!(
                        "WASM plugin {}.invoke_action failed: {e}",
                        shared.plugin_id.0
                    );
                    shared.record_diagnostic("invoke_action", &e.into());
                    return KeyResponse::Pass;
                }
            };
            if let Ok(h) = api.call_state_hash(&mut runtime.store) {
                shared.set_state_hash(h);
            }
            convert::wit_key_response_to_key_response(&result, &|cmds| {
                shared.convert_commands(cmds)
            })
        })
    }

    fn refresh_key_groups(&mut self, state: &AppView<'_>) {
        if let Some(map) = &mut self.key_map {
            for group in &mut map.groups {
                let name = group.name.to_string();
                let active = self.shared.call_synced(state, "is_group_active", |rt| {
                    let api = rt.instance.kasane_plugin_plugin_api();
                    Ok(api.call_is_group_active(&mut rt.store, &name)?)
                });
                group.active = active;
            }
        }
    }

    fn handle_mouse(
        &mut self,
        event: &MouseEvent,
        id: InteractiveId,
        state: &AppView<'_>,
    ) -> Option<Vec<Command>> {
        let shared = Arc::clone(&self.shared);
        self.shared.call_synced(state, "handle_mouse", |rt| {
            let api = rt.instance.kasane_plugin_plugin_api();
            let wit_event = convert::mouse_event_to_wit(event);
            Ok(api
                .call_handle_mouse(&mut rt.store, wit_event, id.local)
                .map(|opt| opt.map(|cmds| shared.convert_commands(&cmds)))?)
        })
    }

    fn handle_default_scroll(
        &mut self,
        candidate: DefaultScrollCandidate,
        state: &AppView<'_>,
    ) -> Option<ScrollPolicyResult> {
        self.shared.with_runtime(|runtime| {
            host::sync_from_app_state(runtime.store.data_mut(), state.as_app_state());
            let plugin_api = runtime.instance.kasane_plugin_plugin_api();
            let wit_candidate = convert::default_scroll_candidate_to_wit(&candidate);
            let result =
                match plugin_api.call_handle_default_scroll(&mut runtime.store, wit_candidate) {
                    Ok(Some(result)) => Some(convert::wit_scroll_policy_result_to_result(&result)),
                    Ok(None) => None,
                    Err(e) => {
                        tracing::error!(
                            "WASM plugin {}.handle_default_scroll failed: {e}",
                            self.shared.plugin_id.0
                        );
                        return None;
                    }
                };

            if result.is_some()
                && let Ok(h) = plugin_api.call_state_hash(&mut runtime.store)
            {
                self.shared.set_state_hash(h);
            }

            result
        })
    }

    fn transform_menu_item(
        &self,
        item: &[Atom],
        index: usize,
        selected: bool,
        state: &AppView<'_>,
    ) -> Option<Vec<Atom>> {
        self.shared.call_synced(state, "transform_menu_item", |rt| {
            let api = rt.instance.kasane_plugin_plugin_api();
            let wit_item = convert::atoms_to_wit(item);
            Ok(api
                .call_transform_menu_item(&mut rt.store, &wit_item, index as u32, selected)
                .map(|opt| opt.map(|t| convert::wit_atoms_to_atoms(&t)))?)
        })
    }

    fn cursor_style_override(
        &self,
        state: &AppView<'_>,
    ) -> Option<kasane_core::render::CursorStyleHint> {
        self.shared
            .call_synced(state, "cursor_style_override", |rt| {
                let api = rt.instance.kasane_plugin_plugin_api();
                Ok(api
                    .call_cursor_style_override(&mut rt.store)?
                    .and_then(|code| {
                        let shape = match code {
                            0 => kasane_core::render::CursorStyle::Block,
                            1 => kasane_core::render::CursorStyle::Bar,
                            2 => kasane_core::render::CursorStyle::Underline,
                            3 => kasane_core::render::CursorStyle::Outline,
                            _ => return None,
                        };
                        Some(kasane_core::render::CursorStyleHint::from(shape))
                    }))
            })
    }

    fn decorate_cells(&self, state: &AppView<'_>) -> Vec<kasane_core::plugin::CellDecoration> {
        self.shared.call_synced(state, "decorate_cells", |rt| {
            let api = rt.instance.kasane_plugin_plugin_api();
            Ok(convert::wit_cell_decorations_to_decorations(
                &api.call_decorate_cells(&mut rt.store)?,
            ))
        })
    }

    fn surfaces(&mut self) -> Vec<Box<dyn Surface>> {
        let shared = Arc::clone(&self.shared);
        self.shared.with_runtime(|runtime| {
            let plugin_api = runtime.instance.kasane_plugin_plugin_api();
            match plugin_api.call_surfaces(&mut runtime.store) {
                Ok(descriptors) => descriptors
                    .into_iter()
                    .map(|descriptor| {
                        let initial_placement = descriptor
                            .initial_placement
                            .as_ref()
                            .map(convert::wit_surface_placement_to_request);
                        shared.hosted_surface(
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
                        self.shared.plugin_id.0
                    );
                    vec![]
                }
            }
        })
    }

    fn contribute_to(
        &self,
        region: &SlotId,
        state: &AppView<'_>,
        ctx: &ContributeContext,
    ) -> Option<Contribution> {
        let wit_region = convert::slot_id_to_wit(region);
        self.shared.call_synced(state, "contribute_to", |rt| {
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
    }

    fn transform(
        &self,
        target: &TransformTarget,
        subject: TransformSubject,
        state: &AppView<'_>,
        ctx: &TransformContext,
    ) -> TransformSubject {
        self.shared.with_runtime(|runtime| {
            host::sync_from_app_state(runtime.store.data_mut(), state.as_app_state());
            runtime.store.data_mut().elements.clear();

            // Convert TransformSubject → WIT transform-subject
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

            let plugin_api = runtime.instance.kasane_plugin_plugin_api();
            let wit_target = convert::transform_target_to_wit(target);
            let wit_ctx = convert::transform_context_to_wit(ctx);
            match plugin_api.call_transform(&mut runtime.store, wit_target, &wit_subject, wit_ctx) {
                Ok(result) => match result {
                    wit::TransformSubject::ElementS(handle) => TransformSubject::Element(
                        runtime.store.data_mut().take_root_element(handle),
                    ),
                    wit::TransformSubject::OverlayS(os) => {
                        let element = runtime.store.data_mut().take_root_element(os.element);
                        let anchor = convert::wit_overlay_anchor_to_overlay_anchor(&os.anchor);
                        TransformSubject::Overlay(kasane_core::element::Overlay { element, anchor })
                    }
                },
                Err(e) => {
                    tracing::error!(
                        "WASM plugin {}.transform failed: {e}",
                        self.shared.plugin_id.0
                    );
                    // Fallback: return original subject
                    subject
                }
            }
        })
    }

    fn transform_patch(
        &self,
        target: &TransformTarget,
        _state: &AppView<'_>,
        ctx: &TransformContext,
    ) -> Option<ElementPatch> {
        self.shared.with_runtime(|runtime| {
            host::sync_from_app_state(runtime.store.data_mut(), _state.as_app_state());
            runtime.store.data_mut().elements.clear();

            let plugin_api = runtime.instance.kasane_plugin_plugin_api();
            let wit_target = convert::transform_target_to_wit(target);
            let wit_ctx = convert::transform_context_to_wit(ctx);
            match plugin_api.call_transform_patch(&mut runtime.store, wit_target, wit_ctx) {
                Ok(ops) if ops.is_empty() => None,
                Ok(ops) => {
                    let patch = convert::wit_element_patch_ops_to_patch(&ops, &mut |handle| {
                        runtime.store.data_mut().take_root_element(handle)
                    });
                    Some(patch)
                }
                Err(e) => {
                    tracing::error!(
                        "WASM plugin {}.transform_patch failed: {e}",
                        self.shared.plugin_id.0
                    );
                    None
                }
            }
        })
    }

    fn transform_priority(&self) -> i16 {
        self.shared.with_runtime(|runtime| {
            let plugin_api = runtime.instance.kasane_plugin_plugin_api();
            match plugin_api.call_transform_priority(&mut runtime.store) {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!(
                        "WASM plugin {}.transform_priority failed: {e}",
                        self.shared.plugin_id.0
                    );
                    0
                }
            }
        })
    }

    fn display_directive_priority(&self) -> i16 {
        // WIT v0.3.0 will add display-directive-priority function.
        // Until then, default to 0.
        0
    }

    fn annotate_line_with_ctx(
        &self,
        line: usize,
        state: &AppView<'_>,
        ctx: &AnnotateContext,
    ) -> Option<LineAnnotation> {
        self.shared.call_synced(state, "annotate_line", |rt| {
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
                        face: convert::wit_face_to_face(&bg.face),
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
    }

    fn display_directives(&self, state: &AppView<'_>) -> Vec<DisplayDirective> {
        self.shared.call_synced(state, "display_directives", |rt| {
            let api = rt.instance.kasane_plugin_plugin_api();
            Ok(convert::wit_display_directives_to_directives(
                &api.call_display_directives(&mut rt.store)?,
            ))
        })
    }

    fn navigation_policy(
        &self,
        unit: &kasane_core::display::unit::DisplayUnit,
    ) -> Option<kasane_core::display::navigation::NavigationPolicy> {
        if !self
            .shared
            .cached_capabilities
            .contains(PluginCapabilities::NAVIGATION_POLICY)
        {
            return None;
        }
        let wit_unit = convert::display_unit_to_wit(unit);
        self.shared.with_runtime(|runtime| {
            let api = runtime.instance.kasane_plugin_plugin_api();
            match api.call_navigation_policy(&mut runtime.store, wit_unit) {
                Ok(kind) => Some(convert::wit_navigation_policy_to_policy(kind)),
                Err(e) => {
                    tracing::error!(
                        "WASM plugin {}.navigation_policy failed: {e}",
                        self.shared.plugin_id.0
                    );
                    None
                }
            }
        })
    }

    fn navigation_action(
        &mut self,
        unit: &kasane_core::display::unit::DisplayUnit,
        action: kasane_core::display::navigation::NavigationAction,
    ) -> Option<kasane_core::display::navigation::ActionResult> {
        if !self
            .shared
            .cached_capabilities
            .contains(PluginCapabilities::NAVIGATION_ACTION)
        {
            return None;
        }
        let wit_unit = convert::display_unit_to_wit(unit);
        let action_kind = convert::navigation_action_to_wit_kind(&action);
        self.shared.with_runtime(|runtime| {
            let api = runtime.instance.kasane_plugin_plugin_api();
            match api.call_on_navigation_action(&mut runtime.store, wit_unit, action_kind) {
                Ok(result) => {
                    let action_result = convert::wit_action_result_to_action_result(result);
                    match action_result {
                        kasane_core::display::navigation::ActionResult::Pass => None,
                        other => Some(other),
                    }
                }
                Err(e) => {
                    tracing::error!(
                        "WASM plugin {}.on_navigation_action failed: {e}",
                        self.shared.plugin_id.0
                    );
                    None
                }
            }
        })
    }

    fn contribute_overlay_with_ctx(
        &self,
        state: &AppView<'_>,
        ctx: &OverlayContext,
    ) -> Option<OverlayContribution> {
        self.shared
            .call_synced(state, "contribute_overlay_v2", |rt| {
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
                            plugin_id: PluginId(String::new()),
                        }
                    }))
            })
    }

    fn capabilities(&self) -> PluginCapabilities {
        self.shared.cached_capabilities
    }

    fn authorities(&self) -> PluginAuthorities {
        self.shared.authorities
    }

    fn allows_process_spawn(&self) -> bool {
        self.shared.process_allowed
    }

    fn update_effects(&mut self, msg: &mut dyn Any, state: &AppView<'_>) -> Effects {
        if let Some(bytes) = msg.downcast_ref::<Vec<u8>>() {
            let shared = Arc::clone(&self.shared);
            self.shared
                .call_synced_with_hash(state, "update_effects", |rt| {
                    let api = rt.instance.kasane_plugin_plugin_api();
                    Ok(shared
                        .convert_runtime_effects(&api.call_update_effects(&mut rt.store, bytes)?))
                })
        } else {
            tracing::warn!(
                "WASM plugin {} received non-byte message, ignoring typed update_effects",
                self.shared.plugin_id.0
            );
            Effects::default()
        }
    }
}
