//! Salsa tracked view functions (Phase 2 — pure Element generation).
//!
//! These tracked functions produce Element trees from Salsa inputs
//! WITHOUT any plugin interaction. Plugin contributions, transforms,
//! and annotations are applied in Stage 2 (outside Salsa).
//!
//! All functions use `#[salsa::tracked(no_eq)]` because `Element` does
//! not implement `PartialEq`. Memoization still works: if inputs haven't
//! changed, the cached result is returned without re-execution.

mod buffer;
pub(crate) mod display_map;
mod info;
mod menu;
mod status;

pub use buffer::pure_buffer_element;
pub use display_map::display_map_query;
pub use info::pure_info_overlays;
pub use menu::pure_menu_overlay;
pub use status::pure_status_element;
