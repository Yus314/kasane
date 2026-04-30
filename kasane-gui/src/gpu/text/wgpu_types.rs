// Originally derived from glyphon 0.10.0 (https://github.com/grovesNL/glyphon)
// Original license: MIT OR Apache-2.0 OR Zlib
//
// Phase 11 trimmed the glyphon-derived module down to the wgpu primitives
// the Parley renderer reuses: the shared [`Cache`](super::wgpu_cache::Cache)
// (shader, bind layouts, pipeline cache) and the per-frame
// [`Viewport`](super::viewport::Viewport) (resolution uniform). The cosmic
// `TextAtlas` / `TextRenderer` / `TextArea` / `TextBounds` types retired
// with cosmic-text. This file holds the shared shape types
// (`Resolution`, `Params`, `GlyphToRender`) the cache + viewport reference.

/// The screen resolution to use when rendering text.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Resolution {
    /// The width of the screen in pixels.
    pub width: u32,
    /// The height of the screen in pixels.
    pub height: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Params {
    pub(crate) screen_resolution: Resolution,
    pub(crate) _pad: [u32; 2],
}

/// One vertex/instance pushed into the wgpu pipeline. The Parley
/// renderer writes its own vertex buffer with the same byte layout
/// (see [`super::vertex_builder::ParleyGlyphVertex`]); this type
/// remains because [`super::wgpu_cache::Cache::get_or_create_pipeline`]
/// references its `size_of` as the vertex stride.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct GlyphToRender {
    pub(crate) pos: [i32; 2],
    pub(crate) dim: [u16; 2],
    pub(crate) uv: [u16; 2],
    pub(crate) color: u32,
    pub(crate) content_type_with_srgb: [u16; 2],
    pub(crate) depth: f32,
}
