use crate::layout::Rect;
use crate::layout::flex;
use crate::state::DirtyFlags;

// ---------------------------------------------------------------------------
// LayoutCache — cached layout positions for section-level repaint
// ---------------------------------------------------------------------------

/// Cache for layout positions so that section-level repaints can target the
/// correct grid region without a full layout pass.
#[derive(Debug, Default)]
pub struct LayoutCache {
    /// Cached base layout result.
    pub(crate) base_layout: Option<flex::LayoutResult>,
    /// Y position of the status bar row.
    pub(crate) status_row: Option<u16>,
    /// Root area used for the last layout.
    pub(crate) root_area: Option<Rect>,
}

impl LayoutCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Invalidate cached layout based on dirty flags and resize.
    pub fn invalidate(&mut self, dirty: DirtyFlags, cols: u16, rows: u16) {
        // Resize → clear everything
        if let Some(root) = self.root_area
            && (root.w != cols || root.h != rows)
        {
            self.base_layout = None;
            self.status_row = None;
            self.root_area = None;
            return;
        }
        if dirty.intersects(DirtyFlags::BUFFER_CONTENT | DirtyFlags::STATUS | DirtyFlags::OPTIONS) {
            self.base_layout = None;
            self.status_row = None;
        }
    }
}
