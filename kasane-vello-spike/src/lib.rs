//! ADR-032 W5 — Vello adoption spike.
//!
//! This crate is an isolated workspace member that hosts an
//! experimental [`GpuBackend`](kasane_gui::gpu::backend::GpuBackend)
//! implementation backed by `vello_hybrid`. It exists to feed
//! decision-grade data into ADR-032 §Spike Findings; it is **not** a
//! production backend.
//!
//! ## Status
//!
//! Currently a *stub*. The `with-vello` feature flag gates the actual
//! Vello dependency; without the feature, all `GpuBackend` methods
//! return [`BackendError::Unsupported`]. With the feature, the impl is
//! still a `todo!()` placeholder — actual rendering is deferred until:
//!
//! 1. **Glifo** ships on crates.io with a stable API for atlas-based
//!    glyph caching driven by Parley layouts, and
//! 2. A GPU-capable environment is available for the spike runs (the
//!    sandbox this crate was authored in lacks `/dev/dri` access).
//!
//! ## Spike Plan (5-day timebox)
//!
//! Once gates open, fill the impl by:
//!
//! 1. **Day 1**: instantiate `vello_hybrid::Renderer` against a
//!    headless wgpu device; render a single solid quad to a texture;
//!    confirm the readback matches the W2 golden harness format.
//! 2. **Day 2**: Translate `DrawCommand::FillRect` and `DrawAtoms` via
//!    Glifo. **Halt gate**: 80×24 warm frame ≤ 100 µs.
//! 3. **Day 3**: Color emoji + variable font parity with swash. **Halt
//!    gate**: DSSIM ≤ 0.05 vs. WgpuBackend goldens.
//! 4. **Day 4**: BiDi, complex scripts (Arabic, Devanagari, CJK
//!    ligatures), subpixel positioning. **Halt gate**: ≤ 2 matrix
//!    rows red.
//! 5. **Day 5**: Document findings into `docs/decisions.md`
//!    `ADR-032 §Spike Findings`. Whether positive or negative, the
//!    findings get written; the spike branch is **not** merged into
//!    main.
//!
//! See `docs/decisions.md` ADR-032 for the full evaluation framework.

use kasane_core::config::FontConfig;
use kasane_core::protocol::Color;
use kasane_core::render::{CursorStyle, DrawCommand, VisualHints};
use kasane_gui::animation::CursorRenderState;
use kasane_gui::colors::ColorResolver;
use kasane_gui::gpu::GpuState;
use kasane_gui::gpu::backend::{AtlasKind, BackendCapabilities, BackendError, GpuBackend};
use winit::dpi::PhysicalSize;

/// Experimental Vello-backed renderer. Stubbed; see crate docs.
pub struct VelloBackend {
    #[allow(dead_code)]
    width: u32,
    #[allow(dead_code)]
    height: u32,

    #[cfg(feature = "with-vello")]
    #[allow(dead_code)]
    renderer: Option<vello_hybrid::Renderer>,
}

impl VelloBackend {
    /// Construct a stub backend. With the `with-vello` feature off,
    /// this is a no-op container. With the feature on, it allocates
    /// a `vello_hybrid::Renderer` (still wrapped in `Option` until
    /// the spike fills in the actual init code).
    pub fn new(_gpu: &GpuState, window_size: PhysicalSize<u32>) -> anyhow::Result<Self> {
        Ok(Self {
            width: window_size.width,
            height: window_size.height,
            #[cfg(feature = "with-vello")]
            renderer: None,
        })
    }

    /// Whether the underlying Vello renderer is available. Used by
    /// callers (and tests) that want to gate behaviour without
    /// matching on cargo features directly.
    pub const fn is_active() -> bool {
        cfg!(feature = "with-vello")
    }
}

impl GpuBackend for VelloBackend {
    fn render_with_cursor(
        &mut self,
        _gpu: &GpuState,
        _commands: &[DrawCommand],
        _color_resolver: &ColorResolver,
        _cursor_style: CursorStyle,
        _cursor_state: &CursorRenderState,
        _cursor_color: Color,
        _overlay_opacities: &[f32],
        _visual_hints: &VisualHints,
    ) -> Result<(), BackendError> {
        #[cfg(feature = "with-vello")]
        {
            // ADR-032 W5 Day 1-4 fill in here.
            // Translate DrawCommand → vello_hybrid::Scene calls,
            // route TextRun via Glifo, submit + present.
            Err(BackendError::Unsupported(
                "VelloBackend::render_with_cursor (spike fill-in pending — Glifo + Vello 1.0)",
            ))
        }
        #[cfg(not(feature = "with-vello"))]
        {
            Err(BackendError::Unsupported(
                "VelloBackend compiled without 'with-vello' feature",
            ))
        }
    }

    fn resize(
        &mut self,
        _gpu: &GpuState,
        _font_config: &FontConfig,
        _scale_factor: f64,
        window_size: PhysicalSize<u32>,
    ) {
        self.width = window_size.width;
        self.height = window_size.height;
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            // Vello supports vector paths, but gated on the feature
            // flag — the stub-only build exposes no path support.
            supports_paths: cfg!(feature = "with-vello"),
            // vello_hybrid is the GPU/CPU mixed path — ostensibly
            // does not require pure compute shaders. The full Vello
            // (compute) path would set this to true; we deliberately
            // chose the hybrid variant per ADR-032 §Decision.
            supports_compute: false,
            atlas_kind: AtlasKind::Glifo,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke test: the stub backend must construct and report
    /// reasonable capabilities even with `--no-default-features`.
    #[test]
    fn stub_backend_constructs_and_reports_capabilities() {
        // We cannot construct GpuState in this test (requires winit),
        // so we exercise the capability-only path.
        let backend = VelloBackend {
            width: 800,
            height: 600,
            #[cfg(feature = "with-vello")]
            renderer: None,
        };
        let caps = backend.capabilities();
        assert_eq!(caps.atlas_kind, AtlasKind::Glifo);
        assert_eq!(caps.supports_compute, false);
        assert_eq!(caps.supports_paths, cfg!(feature = "with-vello"));
    }

    #[test]
    fn is_active_matches_feature_flag() {
        assert_eq!(VelloBackend::is_active(), cfg!(feature = "with-vello"));
    }
}
