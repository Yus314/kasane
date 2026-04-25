//! GPU renderer core: wgpu pipelines, cell rendering, scene composition.
//!
//! Changes here should be coordinated with `kasane-core/src/render/scene/` which
//! defines the `DrawCommand` and scene cache layer consumed by this renderer.

pub mod cell_renderer;
pub mod compositor;
pub mod depth_stencil;
pub mod image_pipeline;
pub mod metrics;
pub(crate) mod pipeline_common;
pub mod quad_pipeline;
pub mod retained_scene;
pub mod scene_graph;
pub mod scene_renderer;
pub mod text_effects;
mod text_helpers;
pub(crate) mod text_pipeline;
pub mod texture_cache;
pub mod timing;

pub use metrics::CellMetrics;

/// Width of the cursor bar (CursorStyle::Bar) in pixels.
pub(crate) const CURSOR_BAR_WIDTH: f32 = 2.0;
/// Height of the cursor underline (CursorStyle::Underline) in pixels.
pub(crate) const CURSOR_UNDERLINE_HEIGHT: f32 = 2.0;
/// Thickness of the cursor outline (CursorStyle::Outline) border in pixels.
pub(crate) const CURSOR_OUTLINE_THICKNESS: f32 = 1.0;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use cosmic_text::Family;
use winit::window::Window;

/// Map generic CSS family names to glyphon's `Family` enum variants.
/// Specific font names (e.g. "JetBrains Mono") pass through as `Family::Name`.
pub fn to_family(name: &str) -> Family<'_> {
    match name {
        "monospace" => Family::Monospace,
        "serif" => Family::Serif,
        "sans-serif" => Family::SansSerif,
        "cursive" => Family::Cursive,
        "fantasy" => Family::Fantasy,
        _ => Family::Name(name),
    }
}

/// Holds all wgpu state: instance, adapter, device, queue, surface.
pub struct GpuState {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    /// Set to `true` when the device reports an error (e.g. device loss).
    pub device_error: Arc<AtomicBool>,
    /// Pipeline cache for faster subsequent pipeline creation.
    pub pipeline_cache: Option<wgpu::PipelineCache>,
}

impl GpuState {
    /// Synchronously initialize the GPU. Called from `ApplicationHandler::resumed()`.
    pub fn new(window: Arc<Window>, present_mode: Option<&str>) -> anyhow::Result<Self> {
        pollster::block_on(Self::new_async(window, present_mode))
    }

    async fn new_async(window: Arc<Window>, present_mode: Option<&str>) -> anyhow::Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(
            Box::new(window.clone()),
        ));
        let surface = instance.create_surface(window.clone())?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await?;

        tracing::info!(
            "GPU adapter: {} ({:?})",
            adapter.get_info().name,
            adapter.get_info().backend
        );

        // Request optional features if the adapter supports them.
        let mut required_features = wgpu::Features::empty();
        if adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            required_features |= wgpu::Features::TIMESTAMP_QUERY;
        }
        if adapter.features().contains(wgpu::Features::PIPELINE_CACHE) {
            required_features |= wgpu::Features::PIPELINE_CACHE;
        }

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_features,
                ..Default::default()
            })
            .await?;

        let device_error = Arc::new(AtomicBool::new(false));
        let error_flag = device_error.clone();
        device.on_uncaptured_error(Arc::new(move |e| {
            tracing::warn!("wgpu device error: {e}");
            error_flag.store(true, Ordering::Relaxed);
        }));

        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or_else(|| anyhow::anyhow!("no compatible surface format"))?;
        if let Some(mode) = present_mode {
            config.present_mode = match mode {
                "Fifo" => wgpu::PresentMode::Fifo,
                "FifoRelaxed" => wgpu::PresentMode::FifoRelaxed,
                "Mailbox" => wgpu::PresentMode::Mailbox,
                "Immediate" => wgpu::PresentMode::Immediate,
                "AutoVsync" => wgpu::PresentMode::AutoVsync,
                "AutoNoVsync" => wgpu::PresentMode::AutoNoVsync,
                other => {
                    tracing::warn!("unknown present_mode {:?}, using default", other);
                    config.present_mode
                }
            };
        }
        tracing::info!(
            "surface format: {:?}, present mode: {:?}",
            config.format,
            config.present_mode
        );
        surface.configure(&device, &config);

        // Create pipeline cache if the device supports it.
        let pipeline_cache = if device.features().contains(wgpu::Features::PIPELINE_CACHE) {
            // SAFETY: We pass no initial data, so this is safe.
            let cache = unsafe {
                device.create_pipeline_cache(&wgpu::PipelineCacheDescriptor {
                    label: Some("kasane_pipeline_cache"),
                    data: None,
                    fallback: true,
                })
            };
            Some(cache)
        } else {
            None
        };

        Ok(GpuState {
            surface,
            device,
            queue,
            config,
            device_error,
            pipeline_cache,
        })
    }

    /// Reconfigure the surface after a resize.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }
}
