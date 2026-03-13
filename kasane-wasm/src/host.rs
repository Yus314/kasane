use kasane_core::element::{Direction, Element, FlexChild, InteractiveId, Overlay, Style};
use kasane_core::protocol::{CursorMode, Face, Line};
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

    // --- v0.3.0 Tier 1: Status bar state ---
    pub status_prompt: Line,
    pub status_content: Line,
    pub status_line: Line,
    pub status_mode_line: Line,
    pub status_default_face: Face,

    // --- v0.3.0 Tier 2: Menu / Info state ---
    pub has_menu: bool,
    pub menu_items: Vec<Line>,
    pub menu_selected: i32,
    pub has_info: bool,
    pub info_count: u32,

    // --- v0.3.0 Tier 3: General state ---
    pub ui_options: std::collections::HashMap<String, String>,
    pub cursor_mode: u8,
    pub widget_columns: u16,
    pub default_face: Face,
    pub padding_face: Face,

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
            status_prompt: Vec::new(),
            status_content: Vec::new(),
            status_line: Vec::new(),
            status_mode_line: Vec::new(),
            status_default_face: Face::default(),
            has_menu: false,
            menu_items: Vec::new(),
            menu_selected: -1,
            has_info: false,
            info_count: 0,
            ui_options: std::collections::HashMap::new(),
            cursor_mode: 0,
            widget_columns: 0,
            default_face: Face::default(),
            padding_face: Face::default(),
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

    // --- v0.3.0 Tier 1: Status bar ---
    fn get_status_prompt(&mut self) -> Vec<bindings::kasane::plugin::types::Atom> {
        convert::atoms_to_wit(&self.status_prompt)
    }
    fn get_status_content(&mut self) -> Vec<bindings::kasane::plugin::types::Atom> {
        convert::atoms_to_wit(&self.status_content)
    }
    fn get_status_line(&mut self) -> Vec<bindings::kasane::plugin::types::Atom> {
        convert::atoms_to_wit(&self.status_line)
    }
    fn get_status_mode_line(&mut self) -> Vec<bindings::kasane::plugin::types::Atom> {
        convert::atoms_to_wit(&self.status_mode_line)
    }
    fn get_status_default_face(&mut self) -> bindings::kasane::plugin::types::Face {
        convert::face_to_wit(&self.status_default_face)
    }

    // --- v0.3.0 Tier 2: Menu / Info ---
    fn has_menu(&mut self) -> bool {
        self.has_menu
    }
    fn get_menu_item_count(&mut self) -> u32 {
        self.menu_items.len() as u32
    }
    fn get_menu_item(&mut self, index: u32) -> Option<Vec<bindings::kasane::plugin::types::Atom>> {
        self.menu_items
            .get(index as usize)
            .map(|line| convert::atoms_to_wit(line))
    }
    fn get_menu_selected(&mut self) -> i32 {
        self.menu_selected
    }
    fn has_info(&mut self) -> bool {
        self.has_info
    }
    fn get_info_count(&mut self) -> u32 {
        self.info_count
    }

    // --- v0.3.0 Tier 3: General ---
    fn get_ui_option(&mut self, key: String) -> Option<String> {
        self.ui_options.get(&key).cloned()
    }
    fn get_cursor_mode(&mut self) -> u8 {
        self.cursor_mode
    }
    fn get_widget_columns(&mut self) -> u16 {
        self.widget_columns
    }
    fn get_default_face(&mut self) -> bindings::kasane::plugin::types::Face {
        convert::face_to_wit(&self.default_face)
    }
    fn get_padding_face(&mut self) -> bindings::kasane::plugin::types::Face {
        convert::face_to_wit(&self.padding_face)
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
            style: Style::Direct(Face::default()),
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
            direction: Direction::Column,
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
            direction: Direction::Row,
            children: flex_children,
            gap,
            align: kasane_core::element::Align::Start,
            cross_align: kasane_core::element::Align::Start,
        };
        let handle = self.elements.len() as u32;
        self.elements.push(element);
        handle
    }

    // --- v0.3.0: Advanced element builders ---

    fn create_container_styled(
        &mut self,
        child: u32,
        border: Option<bindings::kasane::plugin::types::BorderLineStyle>,
        shadow: bool,
        padding: bindings::kasane::plugin::types::Edges,
        style: bindings::kasane::plugin::types::Face,
        title: Option<Vec<bindings::kasane::plugin::types::Atom>>,
    ) -> u32 {
        let child_element = self.take_element(child);
        let title_line: Option<Line> =
            title.map(|atoms| atoms.iter().map(convert::wit_atom_to_atom).collect());
        let element = Element::Container {
            child: Box::new(child_element),
            border: border.as_ref().map(convert::wit_border_to_border_config),
            shadow,
            padding: convert::wit_edges_to_edges(&padding),
            style: Style::Direct(convert::wit_face_to_face(&style)),
            title: title_line,
        };
        let handle = self.elements.len() as u32;
        self.elements.push(element);
        handle
    }

    fn create_scrollable(&mut self, child: u32, offset: u16, vertical: bool) -> u32 {
        let child_element = self.take_element(child);
        let direction = if vertical {
            Direction::Column
        } else {
            Direction::Row
        };
        let element = Element::Scrollable {
            child: Box::new(child_element),
            offset,
            direction,
        };
        let handle = self.elements.len() as u32;
        self.elements.push(element);
        handle
    }

    fn create_stack(
        &mut self,
        base: u32,
        overlays: Vec<bindings::kasane::plugin::types::Overlay>,
    ) -> u32 {
        let base_element = self.take_element(base);
        let native_overlays: Vec<Overlay> = overlays
            .into_iter()
            .map(|o| {
                let element = self.take_element(o.element);
                let anchor = convert::wit_overlay_anchor_to_overlay_anchor(&o.anchor);
                Overlay { element, anchor }
            })
            .collect();
        let element = Element::Stack {
            base: Box::new(base_element),
            overlays: native_overlays,
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

    /// Inject an existing element into the arena, returning its handle.
    /// Used by the decorator system to pass the original element to the guest.
    pub(crate) fn inject_element(&mut self, element: Element) -> u32 {
        let handle = self.elements.len() as u32;
        self.elements.push(element);
        handle
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

    // Tier 1: Status bar
    host.status_prompt = state.status_prompt.clone();
    host.status_content = state.status_content.clone();
    host.status_line = state.status_line.clone();
    host.status_mode_line = state.status_mode_line.clone();
    host.status_default_face = state.status_default_face;

    // Tier 2: Menu / Info
    host.has_menu = state.has_menu();
    if let Some(menu) = &state.menu {
        host.menu_items = menu.items.clone();
        host.menu_selected = menu.selected.map(|s| s as i32).unwrap_or(-1);
    } else {
        host.menu_items.clear();
        host.menu_selected = -1;
    }
    host.has_info = state.has_info();
    host.info_count = state.infos.len() as u32;

    // Tier 3: General
    host.ui_options.clone_from(&state.ui_options);
    host.cursor_mode = match state.cursor_mode {
        CursorMode::Buffer => 0,
        CursorMode::Prompt => 1,
    };
    host.widget_columns = state.widget_columns;
    host.default_face = state.default_face;
    host.padding_face = state.padding_face;
}
