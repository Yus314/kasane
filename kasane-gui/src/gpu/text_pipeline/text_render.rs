// Derived from glyphon 0.10.0 (https://github.com/grovesNL/glyphon)
// Original license: MIT OR Apache-2.0 OR Zlib
// Modified: removed custom_glyph support, depth support, simplified prepare()

use crate::gpu::text_pipeline::{
    ColorMode, ContentType, GlyphDetails, GlyphToRender, GpuCacheStatus, PrepareError, RenderError,
    State, TextArea, TextAtlas, Viewport,
};
use cosmic_text::{Color, FontSystem, SwashCache, SwashContent};
use std::slice;
use wgpu::{
    Buffer, BufferDescriptor, BufferUsages, COPY_BUFFER_ALIGNMENT, DepthStencilState, Device,
    Extent3d, MultisampleState, Origin3d, Queue, RenderPass, RenderPipeline, TexelCopyBufferLayout,
    TexelCopyTextureInfo, TextureAspect,
};

/// A text renderer that uses cached glyphs to render text into an existing render pass.
pub struct TextRenderer {
    vertex_buffer: Buffer,
    vertex_buffer_size: u64,
    pipeline: RenderPipeline,
    glyph_vertices: Vec<GlyphToRender>,
}

impl TextRenderer {
    /// Creates a new `TextRenderer`.
    pub fn new(
        atlas: &mut TextAtlas,
        device: &Device,
        multisample: MultisampleState,
        depth_stencil: Option<DepthStencilState>,
    ) -> Self {
        let vertex_buffer_size = next_copy_buffer_size(4096);
        let vertex_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("glyphon vertices"),
            size: vertex_buffer_size,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let pipeline = atlas.get_or_create_pipeline(device, multisample, depth_stencil);

        Self {
            vertex_buffer,
            vertex_buffer_size,
            pipeline,
            glyph_vertices: Vec::new(),
        }
    }

    /// Prepares all of the provided text areas for rendering.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare<'a>(
        &mut self,
        device: &Device,
        queue: &Queue,
        font_system: &mut FontSystem,
        atlas: &mut TextAtlas,
        viewport: &Viewport,
        text_areas: impl IntoIterator<Item = TextArea<'a>>,
        cache: &mut SwashCache,
    ) -> Result<(), PrepareError> {
        self.glyph_vertices.clear();

        let state = State { device, queue };
        let mut system = GlyphSystem {
            atlas,
            cache,
            font_system,
        };
        let resolution = viewport.resolution();

        for text_area in text_areas {
            let bounds = GlyphBounds {
                x: Bounds {
                    min: text_area.bounds.left.max(0),
                    max: text_area.bounds.right.min(resolution.width as i32),
                },
                y: Bounds {
                    min: text_area.bounds.top.max(0),
                    max: text_area.bounds.bottom.min(resolution.height as i32),
                },
            };

            let is_run_visible = |run: &cosmic_text::LayoutRun| {
                let start_y_physical = (text_area.top + (run.line_top * text_area.scale)) as i32;
                let end_y_physical = start_y_physical + (run.line_height * text_area.scale) as i32;

                start_y_physical <= text_area.bounds.bottom
                    && text_area.bounds.top <= end_y_physical
            };

            let layout_runs = text_area
                .buffer
                .layout_runs()
                .skip_while(|run| !is_run_visible(run))
                .take_while(is_run_visible);

            for run in layout_runs {
                for glyph in run.glyphs.iter() {
                    let physical_glyph =
                        glyph.physical((text_area.left, text_area.top), text_area.scale);

                    let color = match glyph.color_opt {
                        Some(some) => some,
                        None => text_area.default_color,
                    };

                    if let Some(glyph_to_render) = prepare_glyph(
                        &state,
                        &mut system,
                        GlyphMetadata {
                            x: physical_glyph.x,
                            y: physical_glyph.y,
                            line_y: run.line_y,
                            color,
                            cache_key: GlyphonCacheKey::Text(physical_glyph.cache_key),
                            scale_factor: text_area.scale,
                        },
                        bounds,
                    )? {
                        self.glyph_vertices.push(glyph_to_render);
                    }
                }
            }
        }

        let will_render = !self.glyph_vertices.is_empty();
        if !will_render {
            return Ok(());
        }

        let vertices = self.glyph_vertices.as_slice();
        let vertices_raw = unsafe {
            slice::from_raw_parts(
                vertices as *const _ as *const u8,
                std::mem::size_of_val(vertices),
            )
        };

        if self.vertex_buffer_size >= vertices_raw.len() as u64 {
            queue.write_buffer(&self.vertex_buffer, 0, vertices_raw);
        } else {
            self.vertex_buffer.destroy();

            let (buffer, buffer_size) = create_oversized_buffer(
                device,
                Some("glyphon vertices"),
                vertices_raw,
                BufferUsages::VERTEX | BufferUsages::COPY_DST,
            );

            self.vertex_buffer = buffer;
            self.vertex_buffer_size = buffer_size;
        }

        Ok(())
    }

    /// Renders all layouts that were previously provided to `prepare`.
    pub fn render(
        &self,
        atlas: &TextAtlas,
        viewport: &Viewport,
        pass: &mut RenderPass<'_>,
    ) -> Result<(), RenderError> {
        if self.glyph_vertices.is_empty() {
            return Ok(());
        }

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &atlas.bind_group, &[]);
        pass.set_bind_group(1, &viewport.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.draw(0..4, 0..self.glyph_vertices.len() as u32);

        Ok(())
    }
}

#[repr(u16)]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum TextColorConversion {
    None = 0,
    ConvertToLinear = 1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum GlyphonCacheKey {
    Text(cosmic_text::CacheKey),
}

fn next_copy_buffer_size(size: u64) -> u64 {
    let align_mask = COPY_BUFFER_ALIGNMENT - 1;
    ((size.next_power_of_two() + align_mask) & !align_mask).max(COPY_BUFFER_ALIGNMENT)
}

fn create_oversized_buffer(
    device: &Device,
    label: Option<&str>,
    contents: &[u8],
    usage: BufferUsages,
) -> (Buffer, u64) {
    let size = next_copy_buffer_size(contents.len() as u64);
    let buffer = device.create_buffer(&BufferDescriptor {
        label,
        size,
        usage,
        mapped_at_creation: true,
    });
    buffer.slice(..).get_mapped_range_mut()[..contents.len()].copy_from_slice(contents);
    buffer.unmap();
    (buffer, size)
}

// GetGlyphImageResult was removed — image data is now obtained directly
// from SwashCache in prepare_glyph().

struct GlyphMetadata {
    x: i32,
    y: i32,
    line_y: f32,
    scale_factor: f32,
    color: Color,
    cache_key: GlyphonCacheKey,
}

#[derive(Clone, Copy)]
struct Bounds {
    min: i32,
    max: i32,
}

#[derive(Clone, Copy)]
struct GlyphBounds {
    x: Bounds,
    y: Bounds,
}

struct GlyphSystem<'a> {
    atlas: &'a mut TextAtlas,
    cache: &'a mut SwashCache,
    font_system: &'a mut FontSystem,
}

fn prepare_glyph(
    state: &State,
    system: &mut GlyphSystem,
    metadata: GlyphMetadata,
    bounds: GlyphBounds,
) -> Result<Option<GlyphToRender>, PrepareError> {
    let details = if let Some(details) =
        system.atlas.mask_atlas.glyph_cache.get(&metadata.cache_key)
    {
        system
            .atlas
            .mask_atlas
            .glyphs_in_use
            .insert(metadata.cache_key);
        details
    } else if let Some(details) = system
        .atlas
        .color_atlas
        .glyph_cache
        .get(&metadata.cache_key)
    {
        system
            .atlas
            .color_atlas
            .glyphs_in_use
            .insert(metadata.cache_key);
        details
    } else {
        let GlyphonCacheKey::Text(text_cache_key) = metadata.cache_key;

        let Some(image) = system
            .cache
            .get_image_uncached(system.font_system, text_cache_key)
        else {
            return Ok(None);
        };

        let content_type = match image.content {
            SwashContent::Color => ContentType::Color,
            SwashContent::Mask => ContentType::Mask,
            SwashContent::SubpixelMask => {
                // Not implemented yet, but don't panic if this happens.
                ContentType::Mask
            }
        };

        let should_rasterize = image.placement.width > 0 && image.placement.height > 0;

        let (gpu_cache, atlas_id, inner) = if should_rasterize {
            let width = image.placement.width as u16;
            let height = image.placement.height as u16;
            let mut inner = system.atlas.inner_for_content_mut(content_type);

            // Find a position in the packer
            let allocation = loop {
                match inner.try_allocate(width as usize, height as usize) {
                    Some(a) => break a,
                    None => {
                        if !system
                            .atlas
                            .grow(state, system.font_system, system.cache, content_type)
                        {
                            return Err(PrepareError::AtlasFull);
                        }

                        inner = system.atlas.inner_for_content_mut(content_type);
                    }
                }
            };
            let atlas_min = allocation.rectangle.min;

            state.queue.write_texture(
                TexelCopyTextureInfo {
                    texture: &inner.texture,
                    mip_level: 0,
                    origin: Origin3d {
                        x: atlas_min.x as u32,
                        y: atlas_min.y as u32,
                        z: 0,
                    },
                    aspect: TextureAspect::All,
                },
                &image.data,
                TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width as u32 * inner.num_channels() as u32),
                    rows_per_image: None,
                },
                Extent3d {
                    width: width as u32,
                    height: height as u32,
                    depth_or_array_layers: 1,
                },
            );

            (
                GpuCacheStatus::InAtlas {
                    x: atlas_min.x as u16,
                    y: atlas_min.y as u16,
                    content_type,
                },
                Some(allocation.id),
                inner,
            )
        } else {
            let inner = &mut system.atlas.color_atlas;
            (GpuCacheStatus::SkipRasterization, None, inner)
        };

        inner.glyphs_in_use.insert(metadata.cache_key);
        // Insert the glyph into the cache and return the details reference
        inner
            .glyph_cache
            .get_or_insert(metadata.cache_key, || GlyphDetails {
                width: image.placement.width as u16,
                height: image.placement.height as u16,
                gpu_cache,
                atlas_id,
                top: image.placement.top as i16,
                left: image.placement.left as i16,
            })
    };

    let mut x = metadata.x + details.left as i32;
    let mut y =
        (metadata.line_y * metadata.scale_factor).round() as i32 + metadata.y - details.top as i32;

    let (mut atlas_x, mut atlas_y, content_type) = match details.gpu_cache {
        GpuCacheStatus::InAtlas { x, y, content_type } => (x, y, content_type),
        GpuCacheStatus::SkipRasterization => return Ok(None),
    };

    let mut width = details.width as i32;
    let mut height = details.height as i32;

    // Starts beyond right edge or ends beyond left edge
    let max_x = x + width;
    if x > bounds.x.max || max_x < bounds.x.min {
        return Ok(None);
    }

    // Starts beyond bottom edge or ends beyond top edge
    let max_y = y + height;
    if y > bounds.y.max || max_y < bounds.y.min {
        return Ok(None);
    }

    // Clip left edge
    if x < bounds.x.min {
        let right_shift = bounds.x.min - x;

        x = bounds.x.min;
        width = max_x - bounds.x.min;
        atlas_x += right_shift as u16;
    }

    // Clip right edge
    if x + width > bounds.x.max {
        width = bounds.x.max - x;
    }

    // Clip top edge
    if y < bounds.y.min {
        let bottom_shift = bounds.y.min - y;

        y = bounds.y.min;
        height = max_y - bounds.y.min;
        atlas_y += bottom_shift as u16;
    }

    // Clip bottom edge
    if y + height > bounds.y.max {
        height = bounds.y.max - y;
    }

    Ok(Some(GlyphToRender {
        pos: [x, y],
        dim: [width as u16, height as u16],
        uv: [atlas_x, atlas_y],
        color: metadata.color.0,
        content_type_with_srgb: [
            content_type as u16,
            match system.atlas.color_mode {
                ColorMode::Accurate => TextColorConversion::ConvertToLinear,
                ColorMode::Web => TextColorConversion::None,
            } as u16,
        ],
        depth: 0.0,
    }))
}
