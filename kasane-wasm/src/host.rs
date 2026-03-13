use kasane_core::element::{Element, FlexChild, InteractiveId};
use kasane_core::protocol::Line;
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
    /// Buffer lines from AppState (cloned on sync for get-line-text).
    pub lines: Vec<Line>,
    /// Per-line dirty flags.
    pub lines_dirty: Vec<bool>,
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
            lines: Vec::new(),
            lines_dirty: Vec::new(),
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
    fn get_line_text(&mut self, line: u32) -> Option<String> {
        let idx = line as usize;
        if idx < self.lines.len() {
            let text: String = self.lines[idx]
                .iter()
                .map(|atom| atom.contents.as_str())
                .collect();
            Some(text)
        } else {
            None
        }
    }
    fn is_line_dirty(&mut self, line: u32) -> bool {
        let idx = line as usize;
        self.lines_dirty.get(idx).copied().unwrap_or(false)
    }
}

impl bindings::kasane::plugin::element_builder::Host for HostState {
    fn create_text(&mut self, content: String, face: bindings::kasane::plugin::types::Face) -> u32 {
        let element = Element::text(content, convert::wit_face_to_face(&face));
        let handle = self.elements.len() as u32;
        self.elements.push(element);
        handle
    }

    fn create_styled_line(&mut self, atoms: Vec<bindings::kasane::plugin::types::Atom>) -> u32 {
        let line: Line = atoms.iter().map(convert::wit_atom_to_atom).collect();
        let element = Element::styled_line(line);
        let handle = self.elements.len() as u32;
        self.elements.push(element);
        handle
    }

    fn create_column(&mut self, children: Vec<u32>) -> u32 {
        let flex_children = children
            .into_iter()
            .map(|h| FlexChild::fixed(self.take_element(h)))
            .collect();
        let element = Element::column(flex_children);
        let handle = self.elements.len() as u32;
        self.elements.push(element);
        handle
    }

    fn create_row(&mut self, children: Vec<u32>) -> u32 {
        let flex_children = children
            .into_iter()
            .map(|h| FlexChild::fixed(self.take_element(h)))
            .collect();
        let element = Element::row(flex_children);
        let handle = self.elements.len() as u32;
        self.elements.push(element);
        handle
    }

    fn create_interactive(&mut self, child: u32, id: u32) -> u32 {
        let child_element = self.take_element(child);
        let element = Element::Interactive {
            child: Box::new(child_element),
            id: InteractiveId(id),
        };
        let handle = self.elements.len() as u32;
        self.elements.push(element);
        handle
    }

    fn create_grid(
        &mut self,
        columns: Vec<bindings::kasane::plugin::types::GridWidth>,
        children: Vec<u32>,
        col_gap: u16,
        row_gap: u16,
    ) -> u32 {
        let grid_columns: Vec<_> = columns
            .iter()
            .map(convert::wit_grid_width_to_grid_column)
            .collect();
        let child_elements: Vec<_> = children.into_iter().map(|h| self.take_element(h)).collect();
        let element = Element::Grid {
            columns: grid_columns,
            children: child_elements,
            col_gap,
            row_gap,
            align: kasane_core::element::Align::Start,
            cross_align: kasane_core::element::Align::Start,
        };
        let handle = self.elements.len() as u32;
        self.elements.push(element);
        handle
    }

    fn create_container(
        &mut self,
        child: u32,
        border: Option<bindings::kasane::plugin::types::BorderLineStyle>,
        shadow: bool,
        padding: bindings::kasane::plugin::types::Edges,
    ) -> u32 {
        let child_element = self.take_element(child);
        let element = Element::Container {
            child: Box::new(child_element),
            border: border.as_ref().map(convert::wit_border_to_border_config),
            shadow,
            padding: convert::wit_edges_to_edges(&padding),
            style: kasane_core::element::Style::Direct(kasane_core::protocol::Face::default()),
            title: None,
        };
        let handle = self.elements.len() as u32;
        self.elements.push(element);
        handle
    }

    fn create_empty(&mut self) -> u32 {
        let handle = self.elements.len() as u32;
        self.elements.push(Element::Empty);
        handle
    }

    fn create_column_flex(
        &mut self,
        children: Vec<bindings::kasane::plugin::types::FlexEntry>,
        gap: u16,
    ) -> u32 {
        let flex_children: Vec<_> = children
            .into_iter()
            .map(|entry| {
                let element = self.take_element(entry.child);
                FlexChild {
                    element,
                    flex: entry.flex,
                    min_size: None,
                    max_size: None,
                }
            })
            .collect();
        let element = Element::Flex {
            direction: kasane_core::element::Direction::Column,
            children: flex_children,
            gap,
            align: kasane_core::element::Align::Start,
            cross_align: kasane_core::element::Align::Start,
        };
        let handle = self.elements.len() as u32;
        self.elements.push(element);
        handle
    }

    fn create_row_flex(
        &mut self,
        children: Vec<bindings::kasane::plugin::types::FlexEntry>,
        gap: u16,
    ) -> u32 {
        let flex_children: Vec<_> = children
            .into_iter()
            .map(|entry| {
                let element = self.take_element(entry.child);
                FlexChild {
                    element,
                    flex: entry.flex,
                    min_size: None,
                    max_size: None,
                }
            })
            .collect();
        let element = Element::Flex {
            direction: kasane_core::element::Direction::Row,
            children: flex_children,
            gap,
            align: kasane_core::element::Align::Start,
            cross_align: kasane_core::element::Align::Start,
        };
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
    host.lines = state.lines.clone();
    host.lines_dirty = state.lines_dirty.clone();
}
