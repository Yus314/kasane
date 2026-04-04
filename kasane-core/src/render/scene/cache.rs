use super::{CellSize, DrawCommand};
use crate::state::DirtyFlags;

/// Cache for memoized `DrawCommand` lists per view section.
/// Each DirtyFlag clears only its corresponding section.
#[derive(Debug, Default)]
pub struct SceneCache {
    pub(in crate::render) base_commands: Option<Vec<DrawCommand>>,
    pub(in crate::render) menu_commands: Option<Vec<DrawCommand>>,
    pub(in crate::render) info_commands: Option<Vec<DrawCommand>>,
    composed: Vec<DrawCommand>,
    pub(in crate::render) cached_cell_size: Option<(u32, u32)>,
    pub(in crate::render) cached_dims: Option<(u16, u16)>,
}

impl SceneCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Invalidate cached sections based on dirty flags and cell size / dims changes.
    pub fn invalidate(&mut self, dirty: DirtyFlags, cell_size: CellSize, cols: u16, rows: u16) {
        let cs_key = (cell_size.width.to_bits(), cell_size.height.to_bits());
        let dims_key = (cols, rows);

        if self.cached_cell_size != Some(cs_key) || self.cached_dims != Some(dims_key) {
            self.base_commands = None;
            self.menu_commands = None;
            self.info_commands = None;
            self.cached_cell_size = Some(cs_key);
            self.cached_dims = Some(dims_key);
            return;
        }

        if dirty.intersects(
            DirtyFlags::BUFFER_CONTENT
                | DirtyFlags::STATUS
                | DirtyFlags::OPTIONS
                | DirtyFlags::PLUGIN_STATE,
        ) {
            self.base_commands = None;
        }
        if dirty.intersects(DirtyFlags::MENU | DirtyFlags::OPTIONS) {
            self.menu_commands = None;
        }
        if dirty.intersects(DirtyFlags::INFO | DirtyFlags::OPTIONS | DirtyFlags::MENU_STRUCTURE) {
            self.info_commands = None;
        }
    }

    /// Returns true if all sections are cached.
    pub fn is_fully_cached(&self) -> bool {
        self.base_commands.is_some() && self.menu_commands.is_some() && self.info_commands.is_some()
    }

    /// Assemble the composed output from cached sections.
    pub fn compose(&mut self) {
        self.composed.clear();
        if let Some(ref base) = self.base_commands {
            self.composed.extend_from_slice(base);
        }
        if let Some(ref menu) = self.menu_commands
            && !menu.is_empty()
        {
            self.composed.push(DrawCommand::BeginOverlay);
            self.composed.extend_from_slice(menu);
        }
        if let Some(ref info) = self.info_commands {
            self.composed.extend_from_slice(info);
        }
    }

    /// Get a reference to the composed output.
    pub fn composed_ref(&self) -> &[DrawCommand] {
        &self.composed
    }
}
