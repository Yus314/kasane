use kasane_core::element::{
    BorderConfig, BorderLineStyle, Direction, Element, FlexChild, InteractiveId, Overlay, Style,
};
use kasane_core::protocol::{Coord, CursorMode, Face, Line};
use kasane_core::state::{AppState, InfoState};
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

    // --- v0.4.0 Tier 4: Multi-cursor ---
    pub cursor_count: u32,
    pub secondary_cursors: Vec<Coord>,

    // --- v0.4.0 Tier 5: Config ---
    pub config_values: std::collections::HashMap<String, String>,

    // --- v0.4.0 Tier 6: Info content ---
    pub infos: Vec<InfoState>,

    // --- v0.4.0 Tier 7: Menu details ---
    pub menu_anchor: Option<Coord>,
    pub menu_style: Option<String>,
    pub menu_face: Option<Face>,
    pub menu_selected_face: Option<Face>,

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
            cursor_count: 0,
            secondary_cursors: Vec::new(),
            config_values: std::collections::HashMap::new(),
            infos: Vec::new(),
            menu_anchor: None,
            menu_style: None,
            menu_face: None,
            menu_selected_face: None,
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

    // --- v0.4.0 Tier 4: Multi-cursor ---
    fn get_cursor_count(&mut self) -> u32 {
        self.cursor_count
    }
    fn get_secondary_cursor_count(&mut self) -> u32 {
        self.secondary_cursors.len() as u32
    }
    fn get_secondary_cursor(
        &mut self,
        index: u32,
    ) -> Option<bindings::kasane::plugin::types::Coord> {
        self.secondary_cursors
            .get(index as usize)
            .map(|c| bindings::kasane::plugin::types::Coord {
                line: c.line,
                column: c.column,
            })
    }

    // --- v0.4.0 Tier 5: Config ---
    fn get_config_string(&mut self, key: String) -> Option<String> {
        self.config_values.get(&key).cloned()
    }

    // --- v0.4.0 Tier 6: Info content ---
    fn get_info_title(&mut self, index: u32) -> Option<Vec<bindings::kasane::plugin::types::Atom>> {
        self.infos
            .get(index as usize)
            .map(|info| convert::atoms_to_wit(&info.title))
    }
    fn get_info_content(
        &mut self,
        index: u32,
    ) -> Option<Vec<Vec<bindings::kasane::plugin::types::Atom>>> {
        self.infos.get(index as usize).map(|info| {
            info.content
                .iter()
                .map(|line| convert::atoms_to_wit(line))
                .collect()
        })
    }
    fn get_info_style(&mut self, index: u32) -> Option<String> {
        self.infos
            .get(index as usize)
            .map(|info| convert::info_style_to_string(&info.style))
    }
    fn get_info_anchor(&mut self, index: u32) -> Option<bindings::kasane::plugin::types::Coord> {
        self.infos
            .get(index as usize)
            .map(|info| bindings::kasane::plugin::types::Coord {
                line: info.anchor.line,
                column: info.anchor.column,
            })
    }

    // --- v0.4.0 Tier 7: Menu details ---
    fn get_menu_anchor(&mut self) -> Option<bindings::kasane::plugin::types::Coord> {
        self.menu_anchor
            .map(|c| bindings::kasane::plugin::types::Coord {
                line: c.line,
                column: c.column,
            })
    }
    fn get_menu_style(&mut self) -> Option<String> {
        self.menu_style.clone()
    }
    fn get_menu_face(&mut self) -> Option<bindings::kasane::plugin::types::Face> {
        self.menu_face.map(|f| convert::face_to_wit(&f))
    }
    fn get_menu_selected_face(&mut self) -> Option<bindings::kasane::plugin::types::Face> {
        self.menu_selected_face.map(|f| convert::face_to_wit(&f))
    }
}

impl bindings::kasane::plugin::element_builder::Host for HostState {
    fn create_text(&mut self, content: String, face: bindings::kasane::plugin::types::Face) -> u32 {
        let element = Element::text(content, convert::wit_face_to_face(&face));
        self.store_element(element)
    }

    fn create_styled_line(&mut self, atoms: Vec<bindings::kasane::plugin::types::Atom>) -> u32 {
        let line: Line = atoms.iter().map(convert::wit_atom_to_atom).collect();
        let element = Element::styled_line(line);
        self.store_element(element)
    }

    fn create_column(&mut self, children: Vec<u32>) -> u32 {
        let flex_children = children
            .into_iter()
            .map(|h| FlexChild::fixed(self.take_element(h)))
            .collect();
        let element = Element::column(flex_children);
        self.store_element(element)
    }

    fn create_row(&mut self, children: Vec<u32>) -> u32 {
        let flex_children = children
            .into_iter()
            .map(|h| FlexChild::fixed(self.take_element(h)))
            .collect();
        let element = Element::row(flex_children);
        self.store_element(element)
    }

    fn create_interactive(&mut self, child: u32, id: u32) -> u32 {
        let child_element = self.take_element(child);
        let element = Element::Interactive {
            child: Box::new(child_element),
            id: InteractiveId(id),
        };
        self.store_element(element)
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
        self.store_element(element)
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
        self.store_element(element)
    }

    fn create_empty(&mut self) -> u32 {
        self.store_element(Element::Empty)
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
        self.store_element(element)
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
        self.store_element(element)
    }

    fn create_slot_placeholder(
        &mut self,
        slot: bindings::kasane::plugin::types::SlotId,
        direction: bindings::kasane::plugin::types::LayoutDirection,
        gap: u16,
    ) -> u32 {
        let slot = convert::wit_slot_id_to_slot_id(&slot);
        let element = Element::SlotPlaceholder {
            slot_name: slot.0,
            direction: convert::wit_layout_direction_to_direction(direction),
            gap,
        };
        self.store_element(element)
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
        self.store_element(element)
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
        self.store_element(element)
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
        self.store_element(element)
    }

    fn create_container_custom_border(
        &mut self,
        child: u32,
        border_chars: Vec<String>,
        shadow: bool,
        padding: bindings::kasane::plugin::types::Edges,
        style: bindings::kasane::plugin::types::Face,
        title: Option<Vec<bindings::kasane::plugin::types::Atom>>,
    ) -> u32 {
        let child_element = self.take_element(child);
        let title_line: Option<Line> =
            title.map(|atoms| atoms.iter().map(convert::wit_atom_to_atom).collect());

        // Parse 11 border chars: [TL, T, TR, R, BR, B, BL, L, title-left, title-right, shadow]
        let border_config = if border_chars.len() == 11 {
            let mut chars = [' '; 11];
            for (i, s) in border_chars.iter().enumerate() {
                chars[i] = s.chars().next().unwrap_or(' ');
            }
            Some(BorderConfig::new(BorderLineStyle::Custom(Box::new(chars))))
        } else {
            None
        };

        let element = Element::Container {
            child: Box::new(child_element),
            border: border_config,
            shadow,
            padding: convert::wit_edges_to_edges(&padding),
            style: Style::Direct(convert::wit_face_to_face(&style)),
            title: title_line,
        };
        self.store_element(element)
    }
}

impl HostState {
    /// Store an element in the arena and return its handle.
    fn store_element(&mut self, element: Element) -> u32 {
        let handle = self.elements.len() as u32;
        self.elements.push(element);
        handle
    }

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
        self.store_element(element)
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

    // Tier 4: Multi-cursor
    host.cursor_count = state.cursor_count as u32;
    host.secondary_cursors.clone_from(&state.secondary_cursors);

    // Tier 5: Config
    host.config_values.clear();
    host.config_values
        .insert("shadow_enabled".into(), state.shadow_enabled.to_string());
    host.config_values
        .insert("padding_char".into(), state.padding_char.clone());
    host.config_values.insert(
        "menu_position".into(),
        convert::menu_position_to_string(&state.menu_position),
    );
    host.config_values
        .insert("status_at_top".into(), state.status_at_top.to_string());
    host.config_values
        .insert("smooth_scroll".into(), state.smooth_scroll.to_string());
    host.config_values
        .insert("search_dropdown".into(), state.search_dropdown.to_string());
    host.config_values.insert(
        "cursor.secondary_blend".into(),
        state.secondary_blend_ratio.to_string(),
    );
    host.config_values
        .insert("scrollbar.thumb".into(), state.scrollbar_thumb.clone());
    host.config_values
        .insert("scrollbar.track".into(), state.scrollbar_track.clone());

    // Include plugin-defined config
    for (k, v) in &state.plugin_config {
        host.config_values.insert(k.clone(), v.clone());
    }

    // Tier 6: Info content
    host.infos.clone_from(&state.infos);

    // Tier 7: Menu details
    if let Some(menu) = &state.menu {
        host.menu_anchor = Some(menu.anchor);
        host.menu_style = Some(convert::menu_style_to_string(&menu.style));
        host.menu_face = Some(menu.menu_face);
        host.menu_selected_face = Some(menu.selected_item_face);
    } else {
        host.menu_anchor = None;
        host.menu_style = None;
        host.menu_face = None;
        host.menu_selected_face = None;
    }
}
