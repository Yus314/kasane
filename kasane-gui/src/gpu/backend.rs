//! GPU rendering backend abstraction (ADR-032).
//!
//! The [`GpuBackend`] trait formalises the contract a future Vello-class
//! renderer would need to fulfil. The current production renderer
//! ([`super::scene_renderer::SceneRenderer`]) implements this trait via a
//! thin pass-through; the trait itself does not change any runtime
//! behaviour.
//!
//! Why the abstraction lives here, in advance of any second backend:
//!
//! - **Decision-grade artefact.** ADR-032 calls for a backend trait so a
//!   spike (`kasane-vello-spike`) can plug in without churning production
//!   call sites. The contract has to be visible *somewhere* before a spike
//!   can target it.
//! - **No production code change.** `SceneRenderer` retains its inherent
//!   `pub fn render_with_cursor`, `resize`, etc. The call site in
//!   `crate::app::render` (`submit_render`) continues to use the inherent
//!   methods. The trait is *additive*.
//! - **Capability advertisement.** [`BackendCapabilities`] surfaces
//!   per-backend differences (path support, compute shaders, atlas kind)
//!   in one place. Plugin contribution code that wants to query whether
//!   vector paths are renderable can do so without coupling to a concrete
//!   backend type.
//!
//! Adding new variants to [`super::scene_graph::GpuPrimitive`] (e.g. a
//! `Path` variant once Vello adoption is committed) will go through this
//! module: the trait gains the variant in its accepted input, and each
//! impl either renders or returns [`BackendError::Unsupported`].

use kasane_core::config::FontConfig;
use kasane_core::protocol::Color;
use kasane_core::render::{CursorStyle, DrawCommand, VisualHints};
use winit::dpi::PhysicalSize;

use crate::animation::CursorRenderState;
use crate::colors::ColorResolver;

use super::GpuState;

/// What a backend can render.
///
/// Used by callers (currently none in production; pre-positioned for
/// plugin-author APIs and the spike crate) to negotiate features at
/// runtime instead of at compile time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendCapabilities {
    /// Whether the backend can render arbitrary vector paths
    /// (Bezier curves, strokes with caps/joins). The current
    /// `WgpuBackend` returns `false`; a future Vello backend
    /// would return `true`.
    pub supports_paths: bool,
    /// Whether the backend uses compute shaders. The current
    /// fragment-only pipeline returns `false`. Affects platform
    /// support negotiation (some weak GPUs lack robust compute).
    pub supports_compute: bool,
    /// Glyph atlas implementation in use.
    pub atlas_kind: AtlasKind,
}

/// Glyph atlas implementation strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtlasKind {
    /// `etagere` shelf-pack atlas with a swash-driven L1/L2/L3 cache
    /// hierarchy. Used by the current production `WgpuBackend`
    /// (see ADR-031 §"GPU Pipeline Redesign").
    EtagereShelf,
    /// Glifo-managed atlas (formerly `parley_draw`, now in the Vello
    /// repo). Reserved for a future spike-validated backend; not
    /// currently constructed by any production code path.
    Glifo,
}

/// Error returned by a [`GpuBackend`] operation.
#[derive(Debug)]
pub enum BackendError {
    /// The backend does not support the requested feature
    /// (e.g. a `Path` primitive submitted to `WgpuBackend`).
    /// The argument is a short human-readable feature name.
    Unsupported(&'static str),
    /// An underlying renderer error (wgpu surface lost, allocation
    /// failure, shader compilation, etc.).
    Render(anyhow::Error),
}

impl std::fmt::Display for BackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendError::Unsupported(feature) => {
                write!(f, "backend does not support {feature}")
            }
            BackendError::Render(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for BackendError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            BackendError::Unsupported(_) => None,
            BackendError::Render(e) => Some(e.as_ref()),
        }
    }
}

impl From<anyhow::Error> for BackendError {
    fn from(e: anyhow::Error) -> Self {
        BackendError::Render(e)
    }
}

/// A GPU rendering backend.
///
/// The trait is intentionally minimal and shaped after the existing
/// [`super::scene_renderer::SceneRenderer`] surface: it is not an
/// abstract design from first principles, but the *current* contract
/// hoisted into a trait so a second implementor (Vello) can be
/// substituted by the spike crate without changing the call site.
///
/// Implementors:
///   - [`super::scene_renderer::SceneRenderer`] — current production
///     `WgpuBackend` (winit + wgpu + Parley + swash).
///   - `kasane_vello_spike::VelloBackend` — spike-only, when present.
pub trait GpuBackend {
    /// Render a single frame.
    ///
    /// `commands` is the output of `kasane_core::render::scene_paint`
    /// for the current frame. The backend is responsible for translating
    /// it into GPU work and presenting the surface.
    #[allow(clippy::too_many_arguments)]
    fn render_with_cursor(
        &mut self,
        gpu: &GpuState,
        commands: &[DrawCommand],
        color_resolver: &ColorResolver,
        cursor_style: CursorStyle,
        cursor_state: &CursorRenderState,
        cursor_color: Color,
        overlay_opacities: &[f32],
        visual_hints: &VisualHints,
    ) -> Result<(), BackendError>;

    /// Resize the rendering surface and refresh derived state
    /// (font metrics, scale-dependent caches).
    fn resize(
        &mut self,
        gpu: &GpuState,
        font_config: &FontConfig,
        scale_factor: f64,
        window_size: PhysicalSize<u32>,
    );

    /// Capability advertisement; constant per backend instance.
    fn capabilities(&self) -> BackendCapabilities;
}
