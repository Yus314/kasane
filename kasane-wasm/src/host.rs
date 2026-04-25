//! Host function implementations for guest-to-host calls defined in the WIT interface.

use kasane_core::element::{
    BorderConfig, BorderLineStyle, Direction, Element, FlexChild, InteractiveId, Overlay,
    PluginTag, Style,
};
use kasane_core::plugin::PluginId;
use kasane_core::plugin::setting::SettingValue;
use kasane_core::protocol::{Coord, CursorMode, Face, Line};
use kasane_core::state::{AppState, DirtyFlags, InfoState};
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
    pub status_style: String,

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

    // --- v0.23.0: Typed plugin settings ---
    pub settings: std::collections::HashMap<String, SettingValue>,

    // --- v0.4.0 Tier 6: Info content ---
    pub infos: Vec<InfoState>,

    // --- v0.4.0 Tier 7: Menu details ---
    pub menu_anchor: Option<Coord>,
    pub menu_style: Option<String>,
    pub menu_face: Option<Face>,
    pub menu_selected_face: Option<Face>,

    // --- v0.7.0 Tier 8: Session metadata ---
    pub session_descriptors: Vec<SessionDescriptorCache>,
    pub active_session_key: Option<String>,

    // --- v0.10.0 Tier 10a: Editor mode ---
    pub editor_mode: u8,

    // --- v0.10.0 Tier 10b: Selections ---
    pub selections: Vec<kasane_core::state::derived::Selection>,

    // --- v0.8.0 Tier 9: Theme / Color context ---
    pub theme: kasane_core::render::theme::Theme,
    pub is_dark: bool,

    // --- Tier 11: Runtime state ---
    pub is_dragging: bool,

    // --- DU-4: Display unit map ---
    pub display_unit_map: Option<kasane_core::display::DisplayUnitMap>,

    // --- v0.25.0: Syntax provider ---
    pub syntax_provider: Option<std::sync::Arc<dyn kasane_core::syntax::SyntaxProvider>>,

    /// Element arena: WASM plugins build elements via host calls, stored here.
    /// Cleared before each `contribute()` call.
    pub elements: Vec<Element>,

    // --- v0.22.0: Plugin identity for logging ---
    pub plugin_id: String,

    /// Plugin ownership tag for interactive ID namespace isolation.
    pub plugin_tag: PluginTag,

    // WASI support (required by wasmtime-wasi for wasm32-wasip2 components)
    pub wasi: wasmtime_wasi::WasiCtx,
    pub table: wasmtime::component::ResourceTable,

    /// Resource limits for the WASM store (memory, tables, instances).
    pub store_limits: wasmtime::StoreLimits,
}

/// Cached session descriptor for WASM host state.
pub(crate) struct SessionDescriptorCache {
    pub key: String,
    pub session_name: Option<String>,
    pub buffer_name: Option<String>,
    pub mode_line: Option<String>,
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
            status_style: "status".into(),
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
            settings: std::collections::HashMap::new(),
            infos: Vec::new(),
            menu_anchor: None,
            menu_style: None,
            menu_face: None,
            menu_selected_face: None,
            session_descriptors: Vec::new(),
            active_session_key: None,
            editor_mode: 0,
            selections: Vec::new(),
            theme: kasane_core::render::theme::Theme::default_theme(),
            is_dark: true,
            is_dragging: false,
            display_unit_map: None,
            syntax_provider: None,
            elements: Vec::new(),
            plugin_id: String::new(),
            plugin_tag: PluginTag::UNASSIGNED,
            wasi: WasiCtxBuilder::new().build(),
            table: wasmtime::component::ResourceTable::new(),
            store_limits: wasmtime::StoreLimitsBuilder::new()
                .memory_size(64 * 1024 * 1024) // 64 MB per plugin
                .table_elements(10_000)
                .instances(10)
                .build(),
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

    fn get_lines_text(&mut self, start: u32, end: u32) -> Vec<String> {
        let len = self.lines.len();
        let s = (start as usize).min(len);
        let e = (end as usize).min(len);
        if s >= e {
            return Vec::new();
        }
        self.lines[s..e]
            .iter()
            .map(|line| line.iter().map(|atom| atom.contents.as_str()).collect())
            .collect()
    }

    fn get_lines_atoms(
        &mut self,
        start: u32,
        end: u32,
    ) -> Vec<Vec<bindings::kasane::plugin::types::Atom>> {
        let len = self.lines.len();
        let s = (start as usize).min(len);
        let e = (end as usize).min(len);
        if s >= e {
            return Vec::new();
        }
        self.lines[s..e]
            .iter()
            .map(|line| convert::atoms_to_wit(line))
            .collect()
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
    fn get_status_style(&mut self) -> String {
        self.status_style.clone()
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
            .map(|c| (*c).into())
    }

    fn get_all_secondary_cursors(&mut self) -> Vec<bindings::kasane::plugin::types::Coord> {
        self.secondary_cursors.iter().map(|c| (*c).into()).collect()
    }

    // --- v0.4.0 Tier 5: Config ---
    fn get_config_string(&mut self, key: String) -> Option<String> {
        self.config_values.get(&key).cloned()
    }

    // --- v0.23.0: Typed plugin settings ---
    fn get_setting_bool(&mut self, key: String) -> Option<bool> {
        match self.settings.get(&key) {
            Some(SettingValue::Bool(v)) => Some(*v),
            _ => None,
        }
    }
    fn get_setting_integer(&mut self, key: String) -> Option<i64> {
        match self.settings.get(&key) {
            Some(SettingValue::Integer(v)) => Some(*v),
            _ => None,
        }
    }
    fn get_setting_float(&mut self, key: String) -> Option<f64> {
        match self.settings.get(&key) {
            Some(SettingValue::Float(v)) => Some(*v),
            _ => None,
        }
    }
    fn get_setting_string(&mut self, key: String) -> Option<String> {
        match self.settings.get(&key) {
            Some(SettingValue::Str(v)) => Some(v.to_string()),
            _ => None,
        }
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
            .map(|info| info.anchor.into())
    }

    // --- v0.4.0 Tier 7: Menu details ---
    fn get_menu_anchor(&mut self) -> Option<bindings::kasane::plugin::types::Coord> {
        self.menu_anchor.map(|c| c.into())
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

    // --- v0.7.0 Tier 8: Session metadata ---
    fn get_session_count(&mut self) -> u32 {
        self.session_descriptors.len() as u32
    }
    fn get_session(
        &mut self,
        index: u32,
    ) -> Option<bindings::kasane::plugin::types::SessionDescriptor> {
        self.session_descriptors.get(index as usize).map(|d| {
            bindings::kasane::plugin::types::SessionDescriptor {
                key: d.key.clone(),
                session_name: d.session_name.clone(),
                buffer_name: d.buffer_name.clone(),
                mode_line: d.mode_line.clone(),
            }
        })
    }
    fn get_active_session_key(&mut self) -> Option<String> {
        self.active_session_key.clone()
    }
    fn get_active_session_name(&mut self) -> Option<String> {
        let key = self.active_session_key.as_deref()?;
        self.session_descriptors
            .iter()
            .find(|d| d.key == key)
            .and_then(|d| d.session_name.clone())
    }

    // --- v0.8.0 Tier 9: Theme / Color context ---
    fn get_theme_face(&mut self, token: String) -> Option<bindings::kasane::plugin::types::Face> {
        let st = kasane_core::element::StyleToken::new(token);
        self.theme.get(&st).map(convert::face_to_wit)
    }

    fn is_dark_background(&mut self) -> bool {
        self.is_dark
    }

    // --- v0.10.0 Tier 10a: Editor mode ---
    fn get_editor_mode(&mut self) -> u8 {
        self.editor_mode
    }

    // --- v0.10.0 Tier 10b: Selections ---
    fn get_selection_count(&mut self) -> u32 {
        self.selections.len() as u32
    }

    fn get_selection(&mut self, index: u32) -> Option<bindings::kasane::plugin::types::Selection> {
        self.selections
            .get(index as usize)
            .map(|s| bindings::kasane::plugin::types::Selection {
                anchor: s.anchor.into(),
                cursor: s.cursor.into(),
                is_primary: s.is_primary,
            })
    }

    fn get_all_selections(&mut self) -> Vec<bindings::kasane::plugin::types::Selection> {
        self.selections
            .iter()
            .map(|s| bindings::kasane::plugin::types::Selection {
                anchor: s.anchor.into(),
                cursor: s.cursor.into(),
                is_primary: s.is_primary,
            })
            .collect()
    }

    // --- v0.9.0 Tier 10: Buffer file metadata ---
    fn get_buffer_file_path(&mut self) -> Option<String> {
        self.ui_options
            .get("kasane_buffile")
            .filter(|v| !v.is_empty())
            .cloned()
    }

    fn get_display_unit_at_line(
        &mut self,
        display_line: u32,
    ) -> Option<bindings::kasane::plugin::types::DisplayUnitInfo> {
        self.display_unit_map
            .as_ref()
            .and_then(|dum| dum.unit_at_line(display_line as usize))
            .map(convert::display_unit_to_wit)
    }

    fn get_display_unit_count(&mut self) -> u32 {
        self.display_unit_map
            .as_ref()
            .map(|dum| dum.unit_count() as u32)
            .unwrap_or(0)
    }

    fn get_syntax_generation(&mut self) -> u64 {
        self.syntax_provider
            .as_ref()
            .map(|sp| sp.generation())
            .unwrap_or(0)
    }

    fn get_fold_ranges(&mut self) -> Vec<(u32, u32)> {
        self.syntax_provider
            .as_ref()
            .map(|sp| {
                sp.fold_ranges()
                    .into_iter()
                    .map(|r| (r.start as u32, r.end as u32))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn get_scopes_at(&mut self, line: u32, byte_offset: u32) -> Vec<String> {
        self.syntax_provider
            .as_ref()
            .map(|sp| sp.scopes_at(line as usize, byte_offset as usize))
            .unwrap_or_default()
    }

    fn get_indent_level(&mut self, line: u32) -> u32 {
        self.syntax_provider
            .as_ref()
            .map(|sp| sp.indent_level(line as usize))
            .unwrap_or(0)
    }

    fn is_dragging(&mut self) -> bool {
        self.is_dragging
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
            id: InteractiveId::new(id, self.plugin_tag),
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

    // --- v0.20.0: Image element ---

    fn create_image(
        &mut self,
        source: bindings::kasane::plugin::types::ImageSource,
        width: u16,
        height: u16,
        fit: bindings::kasane::plugin::types::ImageFit,
        opacity: f32,
    ) -> u32 {
        let native_source = match source {
            bindings::kasane::plugin::types::ImageSource::FilePath(path) => {
                // Security: resolve relative paths against buffer directory,
                // reject path traversal.
                let resolved = if std::path::Path::new(&path).is_absolute() {
                    path
                } else if let Some(buf_path) = self
                    .ui_options
                    .get("kasane_buffile")
                    .filter(|v| !v.is_empty())
                {
                    let buf_dir = std::path::Path::new(&buf_path)
                        .parent()
                        .unwrap_or(std::path::Path::new("/"));
                    let candidate = buf_dir.join(&path);
                    match candidate.canonicalize() {
                        Ok(canon) if canon.starts_with(buf_dir) => {
                            canon.to_string_lossy().into_owned()
                        }
                        _ => {
                            tracing::warn!("image path rejected (traversal or missing): {path}");
                            return self.store_element(Element::Empty);
                        }
                    }
                } else {
                    path
                };
                kasane_core::element::ImageSource::FilePath(resolved)
            }
            bindings::kasane::plugin::types::ImageSource::RgbaData(rgba) => {
                // Validate data size
                let expected = rgba.width as usize * rgba.height as usize * 4;
                if rgba.data.len() != expected {
                    tracing::warn!(
                        "RGBA data size mismatch: expected {expected}, got {}",
                        rgba.data.len()
                    );
                    return self.store_element(Element::Empty);
                }
                // Reject oversized data (16 MB)
                if rgba.data.len() > 16 * 1024 * 1024 {
                    tracing::warn!("RGBA data too large: {} bytes (max 16 MB)", rgba.data.len());
                    return self.store_element(Element::Empty);
                }
                kasane_core::element::ImageSource::Rgba {
                    data: rgba.data.into(),
                    width: rgba.width,
                    height: rgba.height,
                }
            }
            bindings::kasane::plugin::types::ImageSource::SvgData(svg_bytes) => {
                if svg_bytes.len() > 4 * 1024 * 1024 {
                    tracing::warn!("SVG data too large: {} bytes (max 4 MB)", svg_bytes.len());
                    return self.store_element(Element::Empty);
                }
                if svg_bytes.is_empty() {
                    tracing::warn!("SVG data is empty");
                    return self.store_element(Element::Empty);
                }
                kasane_core::element::ImageSource::SvgData {
                    data: svg_bytes.into(),
                }
            }
        };
        let native_fit = convert::wit_image_fit_to_image_fit(&fit);
        let element = Element::Image {
            source: native_source,
            size: (width, height),
            fit: native_fit,
            opacity: opacity.clamp(0.0, 1.0),
        };
        self.store_element(element)
    }

    fn create_canvas(
        &mut self,
        width: u16,
        height: u16,
        ops: Vec<bindings::kasane::plugin::types::CanvasDrawOp>,
    ) -> u32 {
        let mut content = kasane_core::plugin::canvas::CanvasContent::new();
        for op in &ops {
            content.push(convert::wit_canvas_op_to_canvas_op(op));
        }
        let element = Element::Canvas {
            size: (width, height),
            content,
        };
        self.store_element(element)
    }

    fn create_text_panel(
        &mut self,
        lines: Vec<Vec<bindings::kasane::plugin::types::Atom>>,
        scroll_offset: u32,
        cursor_line: Option<u32>,
        cursor_col: Option<u32>,
        line_numbers: bool,
        wrap: bool,
    ) -> u32 {
        let native_lines: Vec<Vec<kasane_core::protocol::Atom>> = lines
            .into_iter()
            .map(|line| {
                line.into_iter()
                    .map(|a| convert::wit_atom_to_atom(&a))
                    .collect()
            })
            .collect();
        let cursor = cursor_line.map(|l| (l as usize, cursor_col.unwrap_or(0) as usize));
        let element = Element::TextPanel {
            lines: native_lines,
            scroll_offset: scroll_offset as usize,
            cursor,
            line_numbers,
            wrap,
        };
        self.store_element(element)
    }
}

impl bindings::kasane::plugin::host_log::Host for HostState {
    fn log_message(
        &mut self,
        level: bindings::kasane::plugin::host_log::LogLevel,
        message: String,
    ) {
        use bindings::kasane::plugin::host_log::LogLevel;
        let plugin = &self.plugin_id;
        match level {
            LogLevel::Debug => tracing::debug!(plugin = %plugin, "{message}"),
            LogLevel::Info => tracing::info!(plugin = %plugin, "{message}"),
            LogLevel::Warn => tracing::warn!(plugin = %plugin, "{message}"),
            LogLevel::Error => tracing::error!(plugin = %plugin, "{message}"),
        }
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
    pub(crate) fn take_element(&mut self, handle: u32) -> Element {
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
///
/// `view_deps` gates which tiers are synced: tiers whose DirtyFlags don't
/// intersect `view_deps` are skipped entirely.  Tier 0 (scalars) is always
/// synced because it is cheap and universally needed.
pub(crate) fn sync_from_app_state(host: &mut HostState, state: &AppState, view_deps: DirtyFlags) {
    // Tier 0 (always): cheap scalars every plugin needs
    host.cursor_line = state.observed.cursor_pos.line;
    host.cursor_col = state.observed.cursor_pos.column;
    host.line_count = state.observed.lines.len() as u32;
    host.cols = state.runtime.cols;
    host.rows = state.runtime.rows;
    host.focused = state.runtime.focused;
    host.cursor_mode = match state.inference.cursor_mode {
        CursorMode::Buffer => 0,
        CursorMode::Prompt => 1,
    };
    host.is_dragging = !matches!(state.runtime.drag, kasane_core::state::DragState::None);
    host.editor_mode = match state.inference.editor_mode {
        kasane_core::state::derived::EditorMode::Normal => 0,
        kasane_core::state::derived::EditorMode::Insert => 1,
        kasane_core::state::derived::EditorMode::Replace => 2,
        kasane_core::state::derived::EditorMode::Prompt => 3,
        kasane_core::state::derived::EditorMode::Unknown => 255,
    };

    // Tier 1 (BUFFER_CONTENT): lines, lines_dirty — O(buffer_size) clone
    if view_deps.intersects(DirtyFlags::BUFFER_CONTENT) {
        host.lines = state.observed.lines.clone();
        host.lines_dirty = state.inference.lines_dirty.clone();
    }

    // Tier 2 (STATUS): status bar fields
    if view_deps.intersects(DirtyFlags::STATUS) {
        host.status_prompt = state.observed.status_prompt.clone();
        host.status_content = state.observed.status_content.clone();
        host.status_line = state.inference.status_line.clone();
        host.status_mode_line = state.observed.status_mode_line.clone();
        host.status_default_face = state.observed.status_default_face;
        host.status_style = convert::status_style_to_string(&state.observed.status_style);
    }

    // Tier 3 (MENU): menu structure + selection
    if view_deps.intersects(DirtyFlags::MENU) {
        host.has_menu = state.has_menu();
        if let Some(menu) = &state.observed.menu {
            host.menu_items = menu.items.clone();
            host.menu_selected = menu.selected.map(|s| s as i32).unwrap_or(-1);
            host.menu_anchor = Some(menu.anchor);
            host.menu_style = Some(convert::menu_style_to_string(&menu.style));
            host.menu_face = Some(menu.menu_face);
            host.menu_selected_face = Some(menu.selected_item_face);
        } else {
            host.menu_items.clear();
            host.menu_selected = -1;
            host.menu_anchor = None;
            host.menu_style = None;
            host.menu_face = None;
            host.menu_selected_face = None;
        }
    }

    // Tier 4 (INFO): info panels
    if view_deps.intersects(DirtyFlags::INFO) {
        host.has_info = state.has_info();
        host.info_count = state.observed.infos.len() as u32;
        host.infos.clone_from(&state.observed.infos);
    }

    // Tier 5 (OPTIONS): ui_options, config, theme, faces
    if view_deps.intersects(DirtyFlags::OPTIONS) {
        host.ui_options.clone_from(&state.observed.ui_options);
        host.widget_columns = state.observed.widget_columns;
        host.default_face = state.observed.default_face;
        host.padding_face = state.observed.padding_face;
        host.theme = state.config.theme.clone();
        host.is_dark = state.inference.color_context.is_dark;

        host.config_values.clear();
        host.config_values.insert(
            "shadow_enabled".into(),
            state.config.shadow_enabled.to_string(),
        );
        host.config_values
            .insert("padding_char".into(), state.config.padding_char.clone());
        host.config_values.insert(
            "menu_position".into(),
            convert::menu_position_to_string(&state.config.menu_position),
        );
        host.config_values.insert(
            "status_at_top".into(),
            state.config.status_at_top.to_string(),
        );
        host.config_values.insert(
            "search_dropdown".into(),
            state.config.search_dropdown.to_string(),
        );
        host.config_values.insert(
            "cursor.secondary_blend".into(),
            state.config.secondary_blend_ratio.to_string(),
        );
        host.config_values.insert(
            "scrollbar.thumb".into(),
            state.config.scrollbar_thumb.clone(),
        );
        host.config_values.insert(
            "scrollbar.track".into(),
            state.config.scrollbar_track.clone(),
        );
        for (k, v) in &state.config.plugin_config {
            host.config_values.insert(k.clone(), v.clone());
        }
    }

    // Tier (SETTINGS): per-plugin typed settings
    if view_deps.intersects(DirtyFlags::SETTINGS)
        && let Some(ps) = state
            .config
            .plugin_settings
            .get(&PluginId(host.plugin_id.clone()))
    {
        host.settings.clone_from(ps);
    }

    // Tier 6 (SESSION): session metadata
    if view_deps.intersects(DirtyFlags::SESSION) {
        host.session_descriptors = state
            .session
            .session_descriptors
            .iter()
            .map(|d| SessionDescriptorCache {
                key: d.key.clone(),
                session_name: d.session_name.clone(),
                buffer_name: d.buffer_name.clone(),
                mode_line: d.mode_line.clone(),
            })
            .collect();
        host.active_session_key = state.session.active_session_key.clone();
    }

    // Tier 7 (BUFFER_CURSOR): multi-cursor, selections
    if view_deps.intersects(DirtyFlags::BUFFER_CURSOR) {
        host.cursor_count = state.inference.cursor_count as u32;
        host.secondary_cursors
            .clone_from(&state.inference.secondary_cursors);
        host.selections.clone_from(&state.inference.selections);
    }

    // DU-4: Display unit map (synced when buffer content changes)
    if view_deps.intersects(DirtyFlags::BUFFER_CONTENT) {
        host.display_unit_map = state.runtime.display_unit_map.clone();
    }

    // Syntax provider (synced when buffer content changes)
    if view_deps.intersects(DirtyFlags::BUFFER_CONTENT) {
        host.syntax_provider = state.runtime.syntax_provider.clone();
    }
}
