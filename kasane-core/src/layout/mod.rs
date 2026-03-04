mod text;
mod word_wrap;
mod position;
mod info;

pub use text::{
    is_word_char, line_display_width, PROMPT_ASSISTANT_MIN_HEIGHT, PROMPT_ASSISTANT_WIDTH,
};
pub use word_wrap::{word_wrap_line_height, word_wrap_max_row_width};
pub use position::{compute_pos, layout_menu_inline};
pub use info::layout_info;

#[derive(Debug, Clone)]
pub struct FloatingWindow {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

/// A rectangle on screen (used for obstacle avoidance).
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}
