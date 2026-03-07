pub mod flex;
mod hit_test;
mod info;
mod position;
mod text;
mod word_wrap;

pub use hit_test::hit_test;
pub use info::layout_info;
pub use position::{compute_pos, layout_menu_inline};
pub use text::{
    PROMPT_ASSISTANT_MIN_HEIGHT, PROMPT_ASSISTANT_WIDTH, is_word_char, line_display_width,
};
pub use word_wrap::{
    WrapSegment, word_wrap_line_height, word_wrap_max_row_width, word_wrap_segments,
};

#[derive(Debug, Clone)]
pub struct FloatingWindow {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

/// A rectangle on screen (used for obstacle avoidance).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

/// Inline menu placement preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuPlacement {
    /// Default: try below anchor, flip above if needed.
    Auto,
    /// Force above the anchor line.
    Above,
    /// Force below the anchor line.
    Below,
}
