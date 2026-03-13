use kasane_core::element::Element;
use kasane_core::state::AppState;
use wasmtime_wasi::WasiCtxBuilder;

use crate::bindings;
use crate::convert;

/// Host-side state accessible to WASM plugins via imported functions.
///
/// Contains cached copies of AppState fields. Updated before each WASM call
/// via [`sync_from_app_state`].
pub(crate) struct HostState {
    pub cursor_line: i32,
    pub cursor_col: i32,
    pub line_count: u32,
    pub cols: u16,
    pub rows: u16,
    pub focused: bool,
    /// Element arena: WASM plugins build elements via host calls, stored here.
    /// Cleared before each `contribute()` call.
    pub elements: Vec<Element>,
    // WASI support (required by wasmtime-wasi for wasm32-wasip2 components)
    pub wasi: wasmtime_wasi::WasiCtx,
    pub table: wasmtime::component::ResourceTable,
}

impl Default for HostState {
    fn default() -> Self {
        Self {
            cursor_line: 0,
            cursor_col: 0,
            line_count: 0,
            cols: 80,
            rows: 24,
            focused: true,
            elements: Vec::new(),
            wasi: WasiCtxBuilder::new().build(),
            table: wasmtime::component::ResourceTable::new(),
        }
    }
}

impl wasmtime_wasi::WasiView for HostState {
    fn ctx(&mut self) -> wasmtime_wasi::WasiCtxView<'_> {
        wasmtime_wasi::WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

impl bindings::kasane::plugin::host_state::Host for HostState {
    fn get_cursor_line(&mut self) -> i32 {
        self.cursor_line
    }
    fn get_cursor_col(&mut self) -> i32 {
        self.cursor_col
    }
    fn get_line_count(&mut self) -> u32 {
        self.line_count
    }
    fn get_cols(&mut self) -> u16 {
        self.cols
    }
    fn get_rows(&mut self) -> u16 {
        self.rows
    }
    fn is_focused(&mut self) -> bool {
        self.focused
    }
}

impl bindings::kasane::plugin::element_builder::Host for HostState {
    fn create_text(&mut self, content: String, face: bindings::kasane::plugin::types::Face) -> u32 {
        let element = Element::text(content, convert::wit_face_to_face(&face));
        let handle = self.elements.len() as u32;
        self.elements.push(element);
        handle
    }

    fn create_column(&mut self, children: Vec<u32>) -> u32 {
        let flex_children = children
            .into_iter()
            .map(|h| {
                let element = self.take_element(h);
                kasane_core::element::FlexChild::fixed(element)
            })
            .collect();
        let element = Element::column(flex_children);
        let handle = self.elements.len() as u32;
        self.elements.push(element);
        handle
    }

    fn create_row(&mut self, children: Vec<u32>) -> u32 {
        let flex_children = children
            .into_iter()
            .map(|h| {
                let element = self.take_element(h);
                kasane_core::element::FlexChild::fixed(element)
            })
            .collect();
        let element = Element::row(flex_children);
        let handle = self.elements.len() as u32;
        self.elements.push(element);
        handle
    }
}

impl HostState {
    /// Take an element from the arena by handle, replacing it with Empty.
    fn take_element(&mut self, handle: u32) -> Element {
        let idx = handle as usize;
        if idx < self.elements.len() {
            std::mem::replace(&mut self.elements[idx], Element::Empty)
        } else {
            tracing::warn!("invalid element handle: {handle}");
            Element::Empty
        }
    }

    /// Take the final element by handle, consuming it from the arena.
    pub(crate) fn take_root_element(&mut self, handle: u32) -> Element {
        let element = self.take_element(handle);
        self.elements.clear();
        element
    }
}

/// Copy relevant AppState fields into HostState for WASM access.
pub(crate) fn sync_from_app_state(host: &mut HostState, state: &AppState) {
    host.cursor_line = state.cursor_pos.line;
    host.cursor_col = state.cursor_pos.column;
    host.line_count = state.lines.len() as u32;
    host.cols = state.cols;
    host.rows = state.rows;
    host.focused = state.focused;
}
