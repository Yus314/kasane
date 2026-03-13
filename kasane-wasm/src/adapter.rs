use std::any::Any;
use std::cell::{Cell, RefCell};

use kasane_core::element::Element;
use kasane_core::plugin::{Command, LineDecoration, Plugin, PluginId, Slot};
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

impl Plugin for WasmPlugin {
    fn id(&self) -> PluginId {
        self.plugin_id.clone()
    }

    fn on_init(&mut self, state: &AppState) -> Vec<Command> {
        let store = self.store.get_mut();
        host::sync_from_app_state(store.data_mut(), state);
        let plugin_api = self.instance.kasane_plugin_plugin_api();
        if let Err(e) = plugin_api.call_on_init(store) {
            tracing::error!("WASM plugin {}.on_init failed: {e}", self.plugin_id.0);
        }
        vec![]
    }

    fn on_shutdown(&mut self) {
        let store = self.store.get_mut();
        let plugin_api = self.instance.kasane_plugin_plugin_api();
        if let Err(e) = plugin_api.call_on_shutdown(store) {
            tracing::error!("WASM plugin {}.on_shutdown failed: {e}", self.plugin_id.0);
        }
    }

    fn on_state_changed(&mut self, state: &AppState, dirty: DirtyFlags) -> Vec<Command> {
        let store = self.store.get_mut();
        host::sync_from_app_state(store.data_mut(), state);
        let plugin_api = self.instance.kasane_plugin_plugin_api();

        if let Err(e) = plugin_api.call_on_state_changed(&mut *store, dirty.bits()) {
            tracing::error!(
                "WASM plugin {}.on_state_changed failed: {e}",
                self.plugin_id.0
            );
            return vec![];
        }

        // Update cached state hash
        match plugin_api.call_state_hash(store) {
            Ok(h) => self.cached_state_hash.set(h),
            Err(e) => {
                tracing::error!("WASM plugin {}.state_hash failed: {e}", self.plugin_id.0);
            }
        }

        vec![]
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
        let plugin_api = self.instance.kasane_plugin_plugin_api();
        match plugin_api.call_contribute_line(&mut *store, line as u32) {
            Ok(Some(bg)) => Some(LineDecoration {
                left_gutter: None,
                right_gutter: None,
                background: Some(convert::wit_face_to_face(&bg.face)),
            }),
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

    fn update(&mut self, _msg: Box<dyn Any>, _state: &AppState) -> Vec<Command> {
        // WASM plugins cannot receive Box<dyn Any> messages
        vec![]
    }
}
