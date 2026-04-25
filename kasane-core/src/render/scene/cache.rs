use super::{CellSize, DrawCommand};
use crate::state::DirtyFlags;

/// Cache for memoized `DrawCommand` lists per view section.
/// Each DirtyFlag clears only its corresponding section.
///
/// The base layer is split into `buffer_commands` (buffer content, widgets)
/// and `status_commands` (status line) so that STATUS-only changes avoid
/// regenerating the buffer content.
#[derive(Debug, Default)]
pub struct SceneCache {
    pub(in crate::render) buffer_commands: Option<Vec<DrawCommand>>,
    pub(in crate::render) status_commands: Option<Vec<DrawCommand>>,
    pub(in crate::render) overlay_commands: Option<Vec<DrawCommand>>,
    composed: Vec<DrawCommand>,
    pub(in crate::render) cached_cell_size: Option<(u32, u32)>,
    pub(in crate::render) cached_dims: Option<(u16, u16)>,
}

impl SceneCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Backward-compatible setter: stores commands as buffer_commands.
    #[inline]
    pub(in crate::render) fn set_base_commands(&mut self, cmds: Vec<DrawCommand>) {
        self.buffer_commands = Some(cmds);
    }

    /// Backward-compatible getter: checks if both buffer and status (if applicable) are cached.
    #[inline]
    pub(in crate::render) fn has_base_commands(&self) -> bool {
        self.buffer_commands.is_some()
    }

    /// Invalidate cached sections based on dirty flags and cell size / dims changes.
    pub fn invalidate(&mut self, dirty: DirtyFlags, cell_size: CellSize, cols: u16, rows: u16) {
        let cs_key = (cell_size.width.to_bits(), cell_size.height.to_bits());
        let dims_key = (cols, rows);

        if self.cached_cell_size != Some(cs_key) || self.cached_dims != Some(dims_key) {
            self.buffer_commands = None;
            self.status_commands = None;
            self.overlay_commands = None;
            self.cached_cell_size = Some(cs_key);
            self.cached_dims = Some(dims_key);
            return;
        }

        if dirty
            .intersects(DirtyFlags::BUFFER_CONTENT | DirtyFlags::OPTIONS | DirtyFlags::PLUGIN_STATE)
        {
            self.buffer_commands = None;
            // STATUS depends on buffer layout, so invalidate it too
            self.status_commands = None;
        }
        if dirty.intersects(DirtyFlags::STATUS | DirtyFlags::OPTIONS | DirtyFlags::PLUGIN_STATE) {
            self.status_commands = None;
        }
        if dirty.intersects(
            DirtyFlags::MENU
                | DirtyFlags::INFO
                | DirtyFlags::OPTIONS
                | DirtyFlags::MENU_STRUCTURE
                | DirtyFlags::PLUGIN_STATE,
        ) {
            self.overlay_commands = None;
        }
    }

    /// Returns true if all sections are cached.
    pub fn is_fully_cached(&self) -> bool {
        self.buffer_commands.is_some() && self.overlay_commands.is_some()
    }

    /// Assemble the composed output from cached sections.
    pub fn compose(&mut self) {
        self.composed.clear();
        if let Some(ref buf) = self.buffer_commands {
            self.composed.extend_from_slice(buf);
        }
        if let Some(ref status) = self.status_commands {
            self.composed.extend_from_slice(status);
        }
        if let Some(ref overlays) = self.overlay_commands {
            self.composed.extend_from_slice(overlays);
        }
    }

    /// Get a reference to the composed output.
    pub fn composed_ref(&self) -> &[DrawCommand] {
        &self.composed
    }
}
