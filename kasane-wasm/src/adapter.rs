use std::any::Any;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use compact_str::CompactString;
use kasane_core::element::{Element, InteractiveId};
use kasane_core::input::{KeyEvent, MouseEvent};
use kasane_core::plugin::{
    AnnotateContext, BackgroundLayer, BlendMode, Command, ContributeContext, Contribution, IoEvent,
    LineAnnotation, OverlayContext, OverlayContribution, Plugin, PluginCapabilities, PluginId,
    SlotId, TransformContext, TransformTarget,
};
use kasane_core::protocol::Atom;
use kasane_core::state::{AppState, DirtyFlags};
use kasane_core::surface::{
    EventContext, SizeHint, SlotDeclaration, Surface, SurfaceEvent, SurfaceId,
    SurfacePlacementRequest, ViewContext,
};

use crate::bindings;
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

struct WasmPluginShared {
    runtime: Mutex<WasmPluginRuntime>,
    plugin_id: PluginId,
    cached_state_hash: AtomicU64,
    process_allowed: bool,
}

impl WasmPluginShared {
    fn with_runtime<R>(&self, f: impl FnOnce(&mut WasmPluginRuntime) -> R) -> R {
        let mut runtime = self.runtime.lock().expect("wasm runtime poisoned");
        f(&mut runtime)
    }

    fn state_hash(&self) -> u64 {
        self.cached_state_hash.load(Ordering::Relaxed)
    }

    fn set_state_hash(&self, value: u64) {
        self.cached_state_hash.store(value, Ordering::Relaxed);
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
                &wit_event,
                wit_ctx,
            ) {
                Ok(commands) => {
                    if let Ok(hash) = plugin_api.call_state_hash(&mut runtime.store) {
                        self.shared.set_state_hash(hash);
                    }
                    convert::wit_commands_to_commands(&commands)
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

    fn on_state_changed(&mut self, state: &AppState, dirty: DirtyFlags) -> Vec<Command> {
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
                    convert::wit_commands_to_commands(&commands)
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
}

impl WasmPlugin {
    pub(crate) fn new(
        store: wasmtime::Store<HostState>,
        instance: bindings::KasanePlugin,
        id: String,
        process_allowed: bool,
    ) -> Self {
        Self {
            shared: Arc::new(WasmPluginShared {
                runtime: Mutex::new(WasmPluginRuntime { store, instance }),
                plugin_id: PluginId(id),
                cached_state_hash: AtomicU64::new(0),
                process_allowed,
            }),
        }
    }
}

impl Plugin for WasmPlugin {
    fn id(&self) -> PluginId {
        self.shared.plugin_id.clone()
    }

    fn on_init(&mut self, state: &AppState) -> Vec<Command> {
        self.shared.with_runtime(|runtime| {
            host::sync_from_app_state(runtime.store.data_mut(), state);
            let plugin_api = runtime.instance.kasane_plugin_plugin_api();
            match plugin_api.call_on_init(&mut runtime.store) {
                Ok(cmds) => convert::wit_commands_to_commands(&cmds),
                Err(e) => {
                    tracing::error!(
                        "WASM plugin {}.on_init failed: {e}",
                        self.shared.plugin_id.0
                    );
                    vec![]
                }
            }
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

    fn on_state_changed(&mut self, state: &AppState, dirty: DirtyFlags) -> Vec<Command> {
        self.shared.with_runtime(|runtime| {
            host::sync_from_app_state(runtime.store.data_mut(), state);
            let plugin_api = runtime.instance.kasane_plugin_plugin_api();

            let cmds = match plugin_api.call_on_state_changed(&mut runtime.store, dirty.bits()) {
                Ok(cmds) => convert::wit_commands_to_commands(&cmds),
                Err(e) => {
                    tracing::error!(
                        "WASM plugin {}.on_state_changed failed: {e}",
                        self.shared.plugin_id.0
                    );
                    return vec![];
                }
            };

            match plugin_api.call_state_hash(&mut runtime.store) {
                Ok(h) => self.shared.set_state_hash(h),
                Err(e) => {
                    tracing::error!(
                        "WASM plugin {}.state_hash failed: {e}",
                        self.shared.plugin_id.0
                    );
                }
            }

            cmds
        })
    }

    fn on_io_event(&mut self, event: &IoEvent, state: &AppState) -> Vec<Command> {
        self.shared.with_runtime(|runtime| {
            host::sync_from_app_state(runtime.store.data_mut(), state);
            let plugin_api = runtime.instance.kasane_plugin_plugin_api();
            let wit_event = convert::io_event_to_wit(event);
            let cmds = match plugin_api.call_on_io_event(&mut runtime.store, &wit_event) {
                Ok(cmds) => convert::wit_commands_to_commands(&cmds),
                Err(e) => {
                    tracing::error!(
                        "WASM plugin {}.on_io_event failed: {e}",
                        self.shared.plugin_id.0
                    );
                    return vec![];
                }
            };

            if let Ok(h) = plugin_api.call_state_hash(&mut runtime.store) {
                self.shared.set_state_hash(h);
            }

            cmds
        })
    }

    fn state_hash(&self) -> u64 {
        self.shared.state_hash()
    }

    fn observe_key(&mut self, key: &KeyEvent, state: &AppState) {
        self.shared.with_runtime(|runtime| {
            host::sync_from_app_state(runtime.store.data_mut(), state);
            let plugin_api = runtime.instance.kasane_plugin_plugin_api();
            let wit_key = convert::key_event_to_wit(key);
            if let Err(e) = plugin_api.call_observe_key(&mut runtime.store, &wit_key) {
                tracing::error!(
                    "WASM plugin {}.observe_key failed: {e}",
                    self.shared.plugin_id.0
                );
            }
        });
    }

    fn observe_mouse(&mut self, event: &MouseEvent, state: &AppState) {
        self.shared.with_runtime(|runtime| {
            host::sync_from_app_state(runtime.store.data_mut(), state);
            let plugin_api = runtime.instance.kasane_plugin_plugin_api();
            let wit_event = convert::mouse_event_to_wit(event);
            if let Err(e) = plugin_api.call_observe_mouse(&mut runtime.store, wit_event) {
                tracing::error!(
                    "WASM plugin {}.observe_mouse failed: {e}",
                    self.shared.plugin_id.0
                );
            }
        });
    }

    fn handle_key(&mut self, key: &KeyEvent, state: &AppState) -> Option<Vec<Command>> {
        self.shared.with_runtime(|runtime| {
            host::sync_from_app_state(runtime.store.data_mut(), state);
            let plugin_api = runtime.instance.kasane_plugin_plugin_api();
            let wit_key = convert::key_event_to_wit(key);
            let result = match plugin_api.call_handle_key(&mut runtime.store, &wit_key) {
                Ok(Some(cmds)) => Some(convert::wit_commands_to_commands(&cmds)),
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

    fn handle_mouse(
        &mut self,
        event: &MouseEvent,
        id: InteractiveId,
        state: &AppState,
    ) -> Option<Vec<Command>> {
        self.shared.with_runtime(|runtime| {
            host::sync_from_app_state(runtime.store.data_mut(), state);
            let plugin_api = runtime.instance.kasane_plugin_plugin_api();
            let wit_event = convert::mouse_event_to_wit(event);
            match plugin_api.call_handle_mouse(&mut runtime.store, wit_event, id.0) {
                Ok(Some(cmds)) => Some(convert::wit_commands_to_commands(&cmds)),
                Ok(None) => None,
                Err(e) => {
                    tracing::error!(
                        "WASM plugin {}.handle_mouse failed: {e}",
                        self.shared.plugin_id.0
                    );
                    None
                }
            }
        })
    }

    fn transform_menu_item(
        &self,
        item: &[Atom],
        index: usize,
        selected: bool,
        state: &AppState,
    ) -> Option<Vec<Atom>> {
        self.shared.with_runtime(|runtime| {
            host::sync_from_app_state(runtime.store.data_mut(), state);
            let plugin_api = runtime.instance.kasane_plugin_plugin_api();
            let wit_item = convert::atoms_to_wit(item);
            match plugin_api.call_transform_menu_item(
                &mut runtime.store,
                &wit_item,
                index as u32,
                selected,
            ) {
                Ok(Some(transformed)) => Some(convert::wit_atoms_to_atoms(&transformed)),
                Ok(None) => None,
                Err(e) => {
                    tracing::error!(
                        "WASM plugin {}.transform_menu_item failed: {e}",
                        self.shared.plugin_id.0
                    );
                    None
                }
            }
        })
    }

    fn cursor_style_override(&self, state: &AppState) -> Option<kasane_core::render::CursorStyle> {
        self.shared.with_runtime(|runtime| {
            host::sync_from_app_state(runtime.store.data_mut(), state);
            let plugin_api = runtime.instance.kasane_plugin_plugin_api();
            match plugin_api.call_cursor_style_override(&mut runtime.store) {
                Ok(Some(code)) => match code {
                    0 => Some(kasane_core::render::CursorStyle::Block),
                    1 => Some(kasane_core::render::CursorStyle::Bar),
                    2 => Some(kasane_core::render::CursorStyle::Underline),
                    3 => Some(kasane_core::render::CursorStyle::Outline),
                    _ => None,
                },
                Ok(None) => None,
                Err(e) => {
                    tracing::error!(
                        "WASM plugin {}.cursor_style_override failed: {e}",
                        self.shared.plugin_id.0
                    );
                    None
                }
            }
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
                        let declared_slots = descriptor
                            .declared_slots
                            .into_iter()
                            .map(|slot| {
                                SlotDeclaration::new(
                                    slot.name,
                                    convert::wit_slot_kind_to_slot_kind(slot.kind),
                                )
                            })
                            .collect();
                        let initial_placement = descriptor
                            .initial_placement
                            .as_ref()
                            .map(convert::wit_surface_placement_to_request);
                        Box::new(WasmHostedSurface {
                            shared: Arc::clone(&shared),
                            id: next_wasm_surface_id(),
                            surface_key: descriptor.surface_key,
                            size_hint: convert::wit_surface_size_hint_to_size_hint(
                                &descriptor.size_hint,
                            ),
                            declared_slots,
                            initial_placement,
                        }) as Box<dyn Surface>
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
        state: &AppState,
        ctx: &ContributeContext,
    ) -> Option<Contribution> {
        let wit_region = convert::slot_id_to_wit(region);
        self.shared.with_runtime(|runtime| {
            host::sync_from_app_state(runtime.store.data_mut(), state);
            runtime.store.data_mut().elements.clear();
            let plugin_api = runtime.instance.kasane_plugin_plugin_api();
            let wit_ctx = convert::contribute_context_to_wit(ctx);
            match plugin_api.call_contribute_to(&mut runtime.store, &wit_region, wit_ctx) {
                Ok(Some(wit_contrib)) => {
                    let element = runtime
                        .store
                        .data_mut()
                        .take_root_element(wit_contrib.element);
                    Some(Contribution {
                        element,
                        priority: wit_contrib.priority,
                        size_hint: convert::wit_size_hint_to_size_hint(&wit_contrib.size_hint),
                    })
                }
                Ok(None) => None,
                Err(e) => {
                    tracing::error!(
                        "WASM plugin {}.contribute_to failed: {e}",
                        self.shared.plugin_id.0
                    );
                    None
                }
            }
        })
    }

    fn contribute_deps(&self, region: &SlotId) -> DirtyFlags {
        let wit_region = convert::slot_id_to_wit(region);
        self.shared.with_runtime(|runtime| {
            let plugin_api = runtime.instance.kasane_plugin_plugin_api();
            match plugin_api.call_contribute_deps(&mut runtime.store, &wit_region) {
                Ok(bits) => DirtyFlags::from_bits_truncate(bits),
                Err(e) => {
                    tracing::error!(
                        "WASM plugin {}.contribute_deps failed: {e}",
                        self.shared.plugin_id.0
                    );
                    DirtyFlags::ALL
                }
            }
        })
    }

    fn transform(
        &self,
        target: &TransformTarget,
        element: Element,
        state: &AppState,
        ctx: &TransformContext,
    ) -> Element {
        self.shared.with_runtime(|runtime| {
            host::sync_from_app_state(runtime.store.data_mut(), state);
            runtime.store.data_mut().elements.clear();
            let original_handle = runtime.store.data_mut().inject_element(element);
            let plugin_api = runtime.instance.kasane_plugin_plugin_api();
            let wit_target = convert::transform_target_to_wit(target);
            let wit_ctx = convert::transform_context_to_wit(ctx);
            match plugin_api.call_transform_element(
                &mut runtime.store,
                wit_target,
                original_handle,
                wit_ctx,
            ) {
                Ok(result_handle) => runtime.store.data_mut().take_root_element(result_handle),
                Err(e) => {
                    tracing::error!(
                        "WASM plugin {}.transform failed: {e}",
                        self.shared.plugin_id.0
                    );
                    runtime.store.data_mut().take_root_element(original_handle)
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

    fn transform_deps(&self, target: &TransformTarget) -> DirtyFlags {
        self.shared.with_runtime(|runtime| {
            let plugin_api = runtime.instance.kasane_plugin_plugin_api();
            let wit_target = convert::transform_target_to_wit(target);
            match plugin_api.call_transform_deps(&mut runtime.store, wit_target) {
                Ok(bits) => DirtyFlags::from_bits_truncate(bits),
                Err(e) => {
                    tracing::error!(
                        "WASM plugin {}.transform_deps failed: {e}",
                        self.shared.plugin_id.0
                    );
                    DirtyFlags::ALL
                }
            }
        })
    }

    fn annotate_line_with_ctx(
        &self,
        line: usize,
        state: &AppState,
        ctx: &AnnotateContext,
    ) -> Option<LineAnnotation> {
        self.shared.with_runtime(|runtime| {
            host::sync_from_app_state(runtime.store.data_mut(), state);
            runtime.store.data_mut().elements.clear();
            let plugin_api = runtime.instance.kasane_plugin_plugin_api();
            let wit_ctx = convert::annotate_context_to_wit(ctx);
            match plugin_api.call_annotate_line(&mut runtime.store, line as u32, wit_ctx) {
                Ok(Some(wit_ann)) => {
                    let left_gutter = wit_ann
                        .left_gutter
                        .map(|h| runtime.store.data_mut().take_root_element(h));
                    let right_gutter = wit_ann
                        .right_gutter
                        .map(|h| runtime.store.data_mut().take_root_element(h));
                    let background = wit_ann.background.as_ref().map(|bg| BackgroundLayer {
                        face: convert::wit_face_to_face(&bg.face),
                        z_order: bg.z_order,
                        blend: BlendMode::Opaque,
                    });
                    Some(LineAnnotation {
                        left_gutter,
                        right_gutter,
                        background,
                        priority: wit_ann.priority,
                    })
                }
                Ok(None) => None,
                Err(e) => {
                    tracing::error!(
                        "WASM plugin {}.annotate_line failed: {e}",
                        self.shared.plugin_id.0
                    );
                    None
                }
            }
        })
    }

    fn annotate_deps(&self) -> DirtyFlags {
        self.shared.with_runtime(|runtime| {
            let plugin_api = runtime.instance.kasane_plugin_plugin_api();
            match plugin_api.call_annotate_deps(&mut runtime.store) {
                Ok(bits) => DirtyFlags::from_bits_truncate(bits),
                Err(e) => {
                    tracing::error!(
                        "WASM plugin {}.annotate_deps failed: {e}",
                        self.shared.plugin_id.0
                    );
                    DirtyFlags::ALL
                }
            }
        })
    }

    fn contribute_overlay_with_ctx(
        &self,
        state: &AppState,
        ctx: &OverlayContext,
    ) -> Option<OverlayContribution> {
        self.shared.with_runtime(|runtime| {
            host::sync_from_app_state(runtime.store.data_mut(), state);
            runtime.store.data_mut().elements.clear();
            let plugin_api = runtime.instance.kasane_plugin_plugin_api();
            let wit_ctx = convert::overlay_context_to_wit(ctx);
            match plugin_api.call_contribute_overlay_v2(&mut runtime.store, &wit_ctx) {
                Ok(Some(wit_oc)) => {
                    let element = runtime.store.data_mut().take_root_element(wit_oc.element);
                    let anchor = convert::wit_overlay_anchor_to_overlay_anchor(&wit_oc.anchor);
                    Some(OverlayContribution {
                        element,
                        anchor,
                        z_index: wit_oc.z_index,
                    })
                }
                Ok(None) => None,
                Err(e) => {
                    tracing::error!(
                        "WASM plugin {}.contribute_overlay_v2 failed: {e}",
                        self.shared.plugin_id.0
                    );
                    None
                }
            }
        })
    }

    fn capabilities(&self) -> PluginCapabilities {
        PluginCapabilities::all()
    }

    fn allows_process_spawn(&self) -> bool {
        self.shared.process_allowed
    }

    fn update(&mut self, msg: Box<dyn Any>, state: &AppState) -> Vec<Command> {
        if let Ok(bytes) = msg.downcast::<Vec<u8>>() {
            self.shared.with_runtime(|runtime| {
                host::sync_from_app_state(runtime.store.data_mut(), state);
                let plugin_api = runtime.instance.kasane_plugin_plugin_api();
                match plugin_api.call_update(&mut runtime.store, &bytes) {
                    Ok(cmds) => convert::wit_commands_to_commands(&cmds),
                    Err(e) => {
                        tracing::error!(
                            "WASM plugin {}.update failed: {e}",
                            self.shared.plugin_id.0
                        );
                        vec![]
                    }
                }
            })
        } else {
            tracing::warn!(
                "WASM plugin {} received non-byte message, ignoring",
                self.shared.plugin_id.0
            );
            vec![]
        }
    }
}
