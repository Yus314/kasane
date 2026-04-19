//! Compositor: render-to-texture infrastructure for layered effects.
//!
//! Provides intermediate render targets, blit compositing, and Dual-Filter
//! Kawase blur for backdrop blur effects on overlay windows.

pub mod blit;
pub mod blur;
pub mod render_target;

pub use blit::BlitPipeline;
pub use blur::BlurPipeline;
pub use render_target::RenderTarget;
