//! Animation system: track-based engine with cursor adapter.
//!
//! The `AnimationEngine` provides a general-purpose track-based animation
//! system. `CursorAnimation` wraps it with the cursor-specific API (blink,
//! movement easing) used by the rest of the GUI backend.

pub mod cursor;
pub mod engine;
pub mod track;

pub use cursor::{CursorAnimation, CursorRenderState};
pub use engine::AnimationEngine;
pub use track::TrackId;
