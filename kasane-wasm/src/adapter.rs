use std::any::Any;
use std::cell::{Cell, RefCell};

use kasane_core::element::{Element, InteractiveId, Overlay};
use kasane_core::input::{KeyEvent, MouseEvent};
#[allow(deprecated)]
use kasane_core::plugin::Slot;
use kasane_core::plugin::{
    AnnotateContext, BackgroundLayer, BlendMode, Command, ContributeContext, Contribution,
    DecorateTarget, LineAnnotation, LineDecoration, OverlayContext, OverlayContribution, Plugin,
    PluginId, ReplaceTarget, SlotId, TransformContext, TransformTarget,
};
use kasane_core::protocol::Atom;
use kasane_core::state::{AppState, DirtyFlags};

use crate::bindings;
use crate::convert;
use crate::host::{self, HostState};

/// A WASM Component Model plugin adapted to the native Plugin trait.
pub struct WasmPlugin {
    store: RefCell<wasmtime::Store<HostState>>,
    instance: bindings::KasanePlugin,
    plugin_id: PluginId,
    cached_state_hash: Cell<u64>,
}

impl WasmPlugin {
    pub(crate) fn new(
        store: wasmtime::Store<HostState>,
        instance: bindings::KasanePlugin,
        id: String,
    ) -> Self {
        Self {
            store: RefCell::new(store),
            instance,
            plugin_id: PluginId(id),
            cached_state_hash: Cell::new(0),
        }
    }
}

#[allow(deprecated)]
impl Plugin for WasmPlugin {
    fn id(&self) -> PluginId {
        self.plugin_id.clone()
    }

    fn on_init(&mut self, state: &AppState) -> Vec<Command> {
        let store = self.store.get_mut();
        host::sync_from_app_state(store.data_mut(), state);
        let plugin_api = self.instance.kasane_plugin_plugin_api();
        match plugin_api.call_on_init(store) {
            Ok(cmds) => convert::wit_commands_to_commands(&cmds),
            Err(e) => {
                tracing::error!("WASM plugin {}.on_init failed: {e}", self.plugin_id.0);
                vec![]
            }
        }
    }

    fn on_shutdown(&mut self) {
        let store = self.store.get_mut();
        let plugin_api = self.instance.kasane_plugin_plugin_api();
        // on_shutdown returns commands but we can't execute them during shutdown
        if let Err(e) = plugin_api.call_on_shutdown(store) {
            tracing::error!("WASM plugin {}.on_shutdown failed: {e}", self.plugin_id.0);
        }
    }

    fn on_state_changed(&mut self, state: &AppState, dirty: DirtyFlags) -> Vec<Command> {
        let store = self.store.get_mut();
        host::sync_from_app_state(store.data_mut(), state);
        let plugin_api = self.instance.kasane_plugin_plugin_api();

        let cmds = match plugin_api.call_on_state_changed(&mut *store, dirty.bits()) {
            Ok(cmds) => convert::wit_commands_to_commands(&cmds),
            Err(e) => {
                tracing::error!(
                    "WASM plugin {}.on_state_changed failed: {e}",
                    self.plugin_id.0
                );
                return vec![];
            }
        };

        // Update cached state hash
        match plugin_api.call_state_hash(store) {
            Ok(h) => self.cached_state_hash.set(h),
            Err(e) => {
                tracing::error!("WASM plugin {}.state_hash failed: {e}", self.plugin_id.0);
            }
        }

        cmds
    }

    fn state_hash(&self) -> u64 {
        self.cached_state_hash.get()
    }

    fn slot_deps(&self, slot: Slot) -> DirtyFlags {
        let mut store = self.store.borrow_mut();
        let plugin_api = self.instance.kasane_plugin_plugin_api();
        match plugin_api.call_slot_deps(&mut *store, slot.index() as u8) {
            Ok(bits) => DirtyFlags::from_bits_truncate(bits),
            Err(e) => {
                tracing::error!("WASM plugin {}.slot_deps failed: {e}", self.plugin_id.0);
                DirtyFlags::ALL
            }
        }
    }

    fn contribute_line(&self, line: usize, state: &AppState) -> Option<LineDecoration> {
        let mut store = self.store.borrow_mut();
        host::sync_from_app_state(store.data_mut(), state);
        store.data_mut().elements.clear();
        let plugin_api = self.instance.kasane_plugin_plugin_api();
        match plugin_api.call_contribute_line(&mut *store, line as u32) {
            Ok(Some(dec)) => {
                let left_gutter = dec
                    .left_gutter
                    .map(|h| store.data_mut().take_root_element(h));
                let right_gutter = dec
                    .right_gutter
                    .map(|h| store.data_mut().take_root_element(h));
                let background = dec.background.as_ref().map(convert::wit_face_to_face);
                Some(LineDecoration {
                    left_gutter,
                    right_gutter,
                    background,
                })
            }
            Ok(None) => None,
            Err(e) => {
                tracing::error!(
                    "WASM plugin {}.contribute_line failed: {e}",
                    self.plugin_id.0
                );
                None
            }
        }
    }

    fn contribute(&self, slot: Slot, state: &AppState) -> Option<Element> {
        let mut store = self.store.borrow_mut();
        host::sync_from_app_state(store.data_mut(), state);
        store.data_mut().elements.clear();
        let plugin_api = self.instance.kasane_plugin_plugin_api();
        match plugin_api.call_contribute(&mut *store, slot.index() as u8) {
            Ok(Some(handle)) => Some(store.data_mut().take_root_element(handle)),
            Ok(None) => None,
            Err(e) => {
                tracing::error!(
                    "WASM plugin {}.contribute({slot:?}) failed: {e}",
                    self.plugin_id.0
                );
                None
            }
        }
    }

    fn contribute_overlay(&self, state: &AppState) -> Option<Overlay> {
        let mut store = self.store.borrow_mut();
        host::sync_from_app_state(store.data_mut(), state);
        store.data_mut().elements.clear();
        let plugin_api = self.instance.kasane_plugin_plugin_api();
        match plugin_api.call_contribute_overlay(&mut *store) {
            Ok(Some(wit_overlay)) => {
                let element = store.data_mut().take_root_element(wit_overlay.element);
                let anchor = convert::wit_overlay_anchor_to_overlay_anchor(&wit_overlay.anchor);
                Some(Overlay { element, anchor })
            }
            Ok(None) => None,
            Err(e) => {
                tracing::error!(
                    "WASM plugin {}.contribute_overlay failed: {e}",
                    self.plugin_id.0
                );
                None
            }
        }
    }

    fn observe_key(&mut self, key: &KeyEvent, state: &AppState) {
        let store = self.store.get_mut();
        host::sync_from_app_state(store.data_mut(), state);
        let plugin_api = self.instance.kasane_plugin_plugin_api();
        let wit_key = convert::key_event_to_wit(key);
        if let Err(e) = plugin_api.call_observe_key(store, &wit_key) {
            tracing::error!("WASM plugin {}.observe_key failed: {e}", self.plugin_id.0);
        }
    }

    fn observe_mouse(&mut self, event: &MouseEvent, state: &AppState) {
        let store = self.store.get_mut();
        host::sync_from_app_state(store.data_mut(), state);
        let plugin_api = self.instance.kasane_plugin_plugin_api();
        let wit_event = convert::mouse_event_to_wit(event);
        if let Err(e) = plugin_api.call_observe_mouse(store, wit_event) {
            tracing::error!("WASM plugin {}.observe_mouse failed: {e}", self.plugin_id.0);
        }
    }

    fn handle_key(&mut self, key: &KeyEvent, state: &AppState) -> Option<Vec<Command>> {
        let store = self.store.get_mut();
        host::sync_from_app_state(store.data_mut(), state);
        let plugin_api = self.instance.kasane_plugin_plugin_api();
        let wit_key = convert::key_event_to_wit(key);
        match plugin_api.call_handle_key(store, &wit_key) {
            Ok(Some(cmds)) => Some(convert::wit_commands_to_commands(&cmds)),
            Ok(None) => None,
            Err(e) => {
                tracing::error!("WASM plugin {}.handle_key failed: {e}", self.plugin_id.0);
                None
            }
        }
    }

    fn handle_mouse(
        &mut self,
        event: &MouseEvent,
        id: InteractiveId,
        state: &AppState,
    ) -> Option<Vec<Command>> {
        let store = self.store.get_mut();
        host::sync_from_app_state(store.data_mut(), state);
        let plugin_api = self.instance.kasane_plugin_plugin_api();
        let wit_event = convert::mouse_event_to_wit(event);
        match plugin_api.call_handle_mouse(store, wit_event, id.0) {
            Ok(Some(cmds)) => Some(convert::wit_commands_to_commands(&cmds)),
            Ok(None) => None,
            Err(e) => {
                tracing::error!("WASM plugin {}.handle_mouse failed: {e}", self.plugin_id.0);
                None
            }
        }
    }

    // --- v0.3.0: Menu transformation ---

    fn transform_menu_item(
        &self,
        item: &[Atom],
        index: usize,
        selected: bool,
        state: &AppState,
    ) -> Option<Vec<Atom>> {
        let mut store = self.store.borrow_mut();
        host::sync_from_app_state(store.data_mut(), state);
        let plugin_api = self.instance.kasane_plugin_plugin_api();
        let wit_item = convert::atoms_to_wit(item);
        match plugin_api.call_transform_menu_item(&mut *store, &wit_item, index as u32, selected) {
            Ok(Some(transformed)) => Some(convert::wit_atoms_to_atoms(&transformed)),
            Ok(None) => None,
            Err(e) => {
                tracing::error!(
                    "WASM plugin {}.transform_menu_item failed: {e}",
                    self.plugin_id.0
                );
                None
            }
        }
    }

    // --- v0.3.0: Replacement ---

    fn replace(&self, target: ReplaceTarget, state: &AppState) -> Option<Element> {
        let mut store = self.store.borrow_mut();
        host::sync_from_app_state(store.data_mut(), state);
        store.data_mut().elements.clear();
        let plugin_api = self.instance.kasane_plugin_plugin_api();
        let wit_target = convert::replace_target_to_wit(&target);
        match plugin_api.call_replace(&mut *store, wit_target) {
            Ok(Some(handle)) => Some(store.data_mut().take_root_element(handle)),
            Ok(None) => None,
            Err(e) => {
                tracing::error!(
                    "WASM plugin {}.replace({target:?}) failed: {e}",
                    self.plugin_id.0
                );
                None
            }
        }
    }

    // --- v0.3.0: Decorator ---

    fn decorate(&self, target: DecorateTarget, element: Element, state: &AppState) -> Element {
        let mut store = self.store.borrow_mut();
        host::sync_from_app_state(store.data_mut(), state);
        store.data_mut().elements.clear();
        // Inject the existing element as handle 0
        let original_handle = store.data_mut().inject_element(element);
        let plugin_api = self.instance.kasane_plugin_plugin_api();
        let wit_target = convert::decorate_target_to_wit(&target);
        match plugin_api.call_decorate(&mut *store, wit_target, original_handle) {
            Ok(result_handle) => store.data_mut().take_root_element(result_handle),
            Err(e) => {
                tracing::error!(
                    "WASM plugin {}.decorate({target:?}) failed: {e}",
                    self.plugin_id.0
                );
                // On error, try to recover the original element
                store.data_mut().take_root_element(original_handle)
            }
        }
    }

    fn decorator_priority(&self) -> u32 {
        let mut store = self.store.borrow_mut();
        let plugin_api = self.instance.kasane_plugin_plugin_api();
        match plugin_api.call_decorator_priority(&mut *store) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!(
                    "WASM plugin {}.decorator_priority failed: {e}",
                    self.plugin_id.0
                );
                0
            }
        }
    }

    // --- v0.4.0: Cursor style override ---

    fn cursor_style_override(&self, state: &AppState) -> Option<kasane_core::render::CursorStyle> {
        let mut store = self.store.borrow_mut();
        host::sync_from_app_state(store.data_mut(), state);
        let plugin_api = self.instance.kasane_plugin_plugin_api();
        match plugin_api.call_cursor_style_override(&mut *store) {
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
                    self.plugin_id.0
                );
                None
            }
        }
    }

    // --- v0.4.0: Named slot contributions ---

    fn contribute_named_slot(&self, name: &str, state: &AppState) -> Option<Element> {
        let mut store = self.store.borrow_mut();
        host::sync_from_app_state(store.data_mut(), state);
        store.data_mut().elements.clear();
        let plugin_api = self.instance.kasane_plugin_plugin_api();
        match plugin_api.call_contribute_named(&mut *store, name) {
            Ok(Some(handle)) => Some(store.data_mut().take_root_element(handle)),
            Ok(None) => None,
            Err(e) => {
                tracing::error!(
                    "WASM plugin {}.contribute_named({name}) failed: {e}",
                    self.plugin_id.0
                );
                None
            }
        }
    }

    // --- SlotId-based contributions ---

    fn contribute_slot(&self, slot_id: &SlotId, state: &AppState) -> Option<Element> {
        if let Some(legacy) = slot_id.to_legacy() {
            self.contribute(legacy, state)
        } else {
            self.contribute_named_slot(slot_id.as_str(), state)
        }
    }

    // --- v0.5.0: Contribute / Transform / Annotate ---

    fn contribute_to(
        &self,
        region: &SlotId,
        state: &AppState,
        ctx: &ContributeContext,
    ) -> Option<Contribution> {
        let slot_index = convert::slot_id_to_index(region);
        let mut store = self.store.borrow_mut();
        host::sync_from_app_state(store.data_mut(), state);
        store.data_mut().elements.clear();
        let plugin_api = self.instance.kasane_plugin_plugin_api();
        let wit_ctx = convert::contribute_context_to_wit(ctx);
        match plugin_api.call_contribute_to(&mut *store, slot_index, wit_ctx) {
            Ok(Some(wit_contrib)) => {
                let element = store.data_mut().take_root_element(wit_contrib.element);
                Some(Contribution {
                    element,
                    priority: wit_contrib.priority,
                    size_hint: convert::wit_size_hint_to_size_hint(&wit_contrib.size_hint),
                })
            }
            Ok(None) => None,
            Err(e) => {
                tracing::error!("WASM plugin {}.contribute_to failed: {e}", self.plugin_id.0);
                None
            }
        }
    }

    fn contribute_deps(&self, region: &SlotId) -> DirtyFlags {
        let slot_index = convert::slot_id_to_index(region);
        let mut store = self.store.borrow_mut();
        let plugin_api = self.instance.kasane_plugin_plugin_api();
        match plugin_api.call_contribute_deps(&mut *store, slot_index) {
            Ok(bits) => DirtyFlags::from_bits_truncate(bits),
            Err(e) => {
                tracing::error!(
                    "WASM plugin {}.contribute_deps failed: {e}",
                    self.plugin_id.0
                );
                DirtyFlags::ALL
            }
        }
    }

    fn transform(
        &self,
        target: &TransformTarget,
        element: Element,
        state: &AppState,
        ctx: &TransformContext,
    ) -> Element {
        let mut store = self.store.borrow_mut();
        host::sync_from_app_state(store.data_mut(), state);
        store.data_mut().elements.clear();
        let original_handle = store.data_mut().inject_element(element);
        let plugin_api = self.instance.kasane_plugin_plugin_api();
        let wit_target = convert::transform_target_to_wit(target);
        let wit_ctx = convert::transform_context_to_wit(ctx);
        match plugin_api.call_transform_element(&mut *store, wit_target, original_handle, wit_ctx) {
            Ok(result_handle) => store.data_mut().take_root_element(result_handle),
            Err(e) => {
                tracing::error!("WASM plugin {}.transform failed: {e}", self.plugin_id.0);
                store.data_mut().take_root_element(original_handle)
            }
        }
    }

    fn transform_priority(&self) -> i16 {
        let mut store = self.store.borrow_mut();
        let plugin_api = self.instance.kasane_plugin_plugin_api();
        match plugin_api.call_transform_priority(&mut *store) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!(
                    "WASM plugin {}.transform_priority failed: {e}",
                    self.plugin_id.0
                );
                0
            }
        }
    }

    fn transform_deps(&self, target: &TransformTarget) -> DirtyFlags {
        let mut store = self.store.borrow_mut();
        let plugin_api = self.instance.kasane_plugin_plugin_api();
        let wit_target = convert::transform_target_to_wit(target);
        match plugin_api.call_transform_deps(&mut *store, wit_target) {
            Ok(bits) => DirtyFlags::from_bits_truncate(bits),
            Err(e) => {
                tracing::error!(
                    "WASM plugin {}.transform_deps failed: {e}",
                    self.plugin_id.0
                );
                DirtyFlags::ALL
            }
        }
    }

    fn annotate_line_with_ctx(
        &self,
        line: usize,
        state: &AppState,
        ctx: &AnnotateContext,
    ) -> Option<LineAnnotation> {
        let mut store = self.store.borrow_mut();
        host::sync_from_app_state(store.data_mut(), state);
        store.data_mut().elements.clear();
        let plugin_api = self.instance.kasane_plugin_plugin_api();
        let wit_ctx = convert::annotate_context_to_wit(ctx);
        match plugin_api.call_annotate_line(&mut *store, line as u32, wit_ctx) {
            Ok(Some(wit_ann)) => {
                let left_gutter = wit_ann
                    .left_gutter
                    .map(|h| store.data_mut().take_root_element(h));
                let right_gutter = wit_ann
                    .right_gutter
                    .map(|h| store.data_mut().take_root_element(h));
                let background = wit_ann.background.as_ref().map(|bg| BackgroundLayer {
                    face: convert::wit_face_to_face(&bg.face),
                    z_order: bg.z_order,
                    blend: BlendMode::Opaque, // blend_opaque reserved for future use
                });
                Some(LineAnnotation {
                    left_gutter,
                    right_gutter,
                    background,
                })
            }
            Ok(None) => None,
            Err(e) => {
                tracing::error!("WASM plugin {}.annotate_line failed: {e}", self.plugin_id.0);
                None
            }
        }
    }

    fn annotate_deps(&self) -> DirtyFlags {
        let mut store = self.store.borrow_mut();
        let plugin_api = self.instance.kasane_plugin_plugin_api();
        match plugin_api.call_annotate_deps(&mut *store) {
            Ok(bits) => DirtyFlags::from_bits_truncate(bits),
            Err(e) => {
                tracing::error!("WASM plugin {}.annotate_deps failed: {e}", self.plugin_id.0);
                DirtyFlags::ALL
            }
        }
    }

    fn contribute_overlay_with_ctx(
        &self,
        state: &AppState,
        ctx: &OverlayContext,
    ) -> Option<OverlayContribution> {
        let mut store = self.store.borrow_mut();
        host::sync_from_app_state(store.data_mut(), state);
        store.data_mut().elements.clear();
        let plugin_api = self.instance.kasane_plugin_plugin_api();
        let wit_ctx = convert::overlay_context_to_wit(ctx);
        match plugin_api.call_contribute_overlay_v2(&mut *store, &wit_ctx) {
            Ok(Some(wit_oc)) => {
                let element = store.data_mut().take_root_element(wit_oc.element);
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
                    self.plugin_id.0
                );
                None
            }
        }
    }

    // Note: capabilities() uses default (excludes CONTRIBUTOR/TRANSFORMER/ANNOTATOR)
    // WASM plugins that implement new APIs will need capabilities to be detected
    // from which WIT exports they override vs return defaults.

    // --- v0.3.0: Inter-plugin messaging ---

    fn update(&mut self, msg: Box<dyn Any>, state: &AppState) -> Vec<Command> {
        // WASM plugins receive messages as Vec<u8> bytes
        if let Ok(bytes) = msg.downcast::<Vec<u8>>() {
            let store = self.store.get_mut();
            host::sync_from_app_state(store.data_mut(), state);
            let plugin_api = self.instance.kasane_plugin_plugin_api();
            match plugin_api.call_update(store, &bytes) {
                Ok(cmds) => convert::wit_commands_to_commands(&cmds),
                Err(e) => {
                    tracing::error!("WASM plugin {}.update failed: {e}", self.plugin_id.0);
                    vec![]
                }
            }
        } else {
            tracing::warn!(
                "WASM plugin {} received non-byte message, ignoring",
                self.plugin_id.0
            );
            vec![]
        }
    }
}
