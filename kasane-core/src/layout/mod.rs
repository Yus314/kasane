//! Layout engine: measure/place, overlay positioning, hit testing.

pub mod flex;
pub(crate) mod grid;
mod hit_map;
mod hit_test;
mod info;
mod position;
mod text;
mod word_wrap;

pub use hit_map::{HitMap, build_hit_map};
pub use hit_test::hit_test;
pub use info::layout_info;
pub use position::{compute_pos, get_menu_rect, layout_menu_inline, layout_single_overlay};
pub(crate) use text::{ASSISTANT_CLIPPY, ASSISTANT_WIDTH};
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

/// Direction of a split (shared by pane and workspace layout trees).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

/// A rectangle on screen (used for obstacle avoidance).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

impl Rect {
    /// Split this rectangle into two sub-rectangles with a 1-cell divider.
    pub fn split(self, direction: SplitDirection, ratio: f32) -> (Rect, Rect) {
        match direction {
            SplitDirection::Vertical => {
                // Side by side, divider is a vertical line (1 column)
                let total = self.w.saturating_sub(1);
                let first_w = ((total as f32) * ratio).round() as u16;
                let second_w = total.saturating_sub(first_w);
                let first = Rect {
                    x: self.x,
                    y: self.y,
                    w: first_w,
                    h: self.h,
                };
                let second = Rect {
                    x: self.x + first_w + 1,
                    y: self.y,
                    w: second_w,
                    h: self.h,
                };
                (first, second)
            }
            SplitDirection::Horizontal => {
                // Stacked top/bottom, divider is a horizontal line (1 row)
                let total = self.h.saturating_sub(1);
                let first_h = ((total as f32) * ratio).round() as u16;
                let second_h = total.saturating_sub(first_h);
                let first = Rect {
                    x: self.x,
                    y: self.y,
                    w: self.w,
                    h: first_h,
                };
                let second = Rect {
                    x: self.x,
                    y: self.y + first_h + 1,
                    w: self.w,
                    h: second_h,
                };
                (first, second)
            }
        }
    }
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

impl From<crate::config::MenuPosition> for MenuPlacement {
    fn from(pos: crate::config::MenuPosition) -> Self {
        match pos {
            crate::config::MenuPosition::Above => MenuPlacement::Above,
            crate::config::MenuPosition::Below => MenuPlacement::Below,
            crate::config::MenuPosition::Auto => MenuPlacement::Auto,
        }
    }
}
