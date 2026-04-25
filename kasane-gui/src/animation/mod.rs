//! Animation system: track-based engine with cursor adapter.
//!
//! The `AnimationEngine` provides a general-purpose track-based animation
//! system. `CursorAnimation` wraps it with the cursor-specific API (blink,
//! movement easing) used by the rest of the GUI backend.

pub mod cursor;
pub mod element_key;
pub mod engine;
pub mod keyframe;
pub mod property_track;
pub mod spring;
pub mod track;

pub use cursor::{CursorAnimation, CursorRenderState};
pub use element_key::ElementKey;
pub use engine::AnimationEngine;
pub use keyframe::KeyframeTrack;
pub use property_track::{PropertyName, PropertyTrack};
pub use spring::SpringPhysics;
pub use track::TrackId;
