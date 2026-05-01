use std::collections::{HashMap, HashSet};

/// Key for cached textures.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TextureKey {
    /// File path on disk.
    FilePath(String),
    /// Inline RGBA data, keyed by content sample hash + dimensions.
    Inline(u64),
}

impl TextureKey {
    /// Compute a content-addressed key for inline RGBA data.
    ///
    /// Samples bytes from three positions (start, middle, end) for fast
    /// hashing that is stable across different `Arc` allocations of the
    /// same image data.
    pub fn inline_from_data(data: &[u8], width: u32, height: u32) -> Self {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::hash::DefaultHasher::new();
        width.hash(&mut hasher);
        height.hash(&mut hasher);
        data.len().hash(&mut hasher);
        const SAMPLE: usize = 64;
        // Head
        data[..data.len().min(SAMPLE)].hash(&mut hasher);
        // Middle
        if data.len() > SAMPLE * 2 {
            let mid = data.len() / 2;
            data[mid..mid + SAMPLE.min(data.len() - mid)].hash(&mut hasher);
        }
        // Tail
        if data.len() > SAMPLE {
            data[data.len() - SAMPLE..].hash(&mut hasher);
        }
        TextureKey::Inline(hasher.finish())
    }

    /// Compute a content-addressed key for inline SVG data.
    ///
    /// Uses width=0, height=0 as sentinel values to distinguish from RGBA keys.
    pub fn inline_from_svg_data(data: &[u8]) -> Self {
        Self::inline_from_data(data, 0, 0)
    }
}

/// Decoded image data ready for GPU upload.
pub struct DecodedImage {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl std::fmt::Debug for DecodedImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DecodedImage")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("data_len", &self.data.len())
            .finish()
    }
}

/// Result of a texture lookup.
pub enum LoadState {
    /// Texture is ready for rendering.
    Ready(u32, u32),
    /// Decode in progress on a background thread.
    Pending,
    /// Decode failed (logged, won't retry this frame).
    Failed,
}

struct CachedTexture {
    /// Kept alive to prevent GPU resource deallocation.
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    /// Pre-built bind group for this texture (avoids per-frame allocation).
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
    byte_size: usize,
    last_used_frame: u64,
}

/// LRU texture cache with a memory budget.
pub struct TextureCache {
    entries: HashMap<TextureKey, CachedTexture>,
    /// Insertion order for LRU eviction (oldest first).
    lru_order: Vec<TextureKey>,
    total_bytes: usize,
    budget_bytes: usize,
    frame_counter: u64,
    sampler: wgpu::Sampler,
    bind_group_layout: wgpu::BindGroupLayout,
    /// Keys currently being decoded in background threads.
    pending: HashSet<TextureKey>,
    /// Keys that failed to load (avoid retrying every frame).
    failed: HashSet<TextureKey>,
}

/// Maximum texture dimension (width or height).
const MAX_TEXTURE_DIM: u32 = 8192;

impl TextureCache {
    pub fn new(device: &wgpu::Device, budget_bytes: usize) -> Self {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("texture_cache_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("texture_cache_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        TextureCache {
            entries: HashMap::new(),
            lru_order: Vec::new(),
            total_bytes: 0,
            budget_bytes,
            frame_counter: 0,
            sampler,
            bind_group_layout,
            pending: HashSet::new(),
            failed: HashSet::new(),
        }
    }

    /// Increment the frame counter. Call once per frame.
    pub fn frame_tick(&mut self) {
        self.frame_counter += 1;
    }

    /// Get the bind group layout (shared with ImagePipeline).
    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    /// Get the sampler (shared with ImagePipeline).
    pub fn sampler(&self) -> &wgpu::Sampler {
        &self.sampler
    }

    /// Look up a texture, returning its load state.
    ///
    /// For `FilePath` keys that are not yet cached, this dispatches a background
    /// decode thread and returns `LoadState::Pending`. Call `finalize_load()` when
    /// the decoded data arrives via `GuiEvent::ImageLoaded`.
    pub(crate) fn get_or_load(
        &mut self,
        key: &TextureKey,
        proxy: Option<&winit::event_loop::EventLoopProxy<crate::GuiEvent>>,
    ) -> LoadState {
        // Already cached
        if let Some(entry) = self.entries.get_mut(key) {
            entry.last_used_frame = self.frame_counter;
            return LoadState::Ready(entry.width, entry.height);
        }

        // Already known to have failed
        if self.failed.contains(key) {
            return LoadState::Failed;
        }

        // Already being decoded
        if self.pending.contains(key) {
            return LoadState::Pending;
        }

        // Dispatch based on key type
        match key {
            TextureKey::FilePath(path) => {
                let Some(proxy) = proxy else {
                    tracing::warn!(
                        "texture_cache: async file load requested but no event proxy \
                         (headless mode); returning Failed"
                    );
                    self.failed.insert(key.clone());
                    return LoadState::Failed;
                };
                let key_clone = key.clone();
                let path_clone = path.clone();
                let proxy = proxy.clone();
                self.pending.insert(key_clone.clone());
                std::thread::spawn(move || {
                    let result = load_image_file(&path_clone);
                    match result {
                        Ok((data, width, height)) => {
                            let decoded = DecodedImage {
                                data,
                                width,
                                height,
                            };
                            let _ = proxy
                                .send_event(crate::GuiEvent::ImageLoaded(key_clone, Ok(decoded)));
                        }
                        Err(e) => {
                            let _ =
                                proxy.send_event(crate::GuiEvent::ImageLoaded(key_clone, Err(e)));
                        }
                    }
                });
                LoadState::Pending
            }
            TextureKey::Inline(_) => {
                // Inline data should be loaded via `insert_rgba` instead
                LoadState::Failed
            }
        }
    }

    /// Finalize an async load. Called when `GuiEvent::ImageLoaded` arrives.
    /// Returns `true` if the texture was successfully inserted (caller should redraw).
    pub fn finalize_load(
        &mut self,
        key: TextureKey,
        result: Result<DecodedImage, String>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> bool {
        self.pending.remove(&key);
        match result {
            Ok(decoded) => {
                self.insert_texture(
                    key,
                    &decoded.data,
                    decoded.width,
                    decoded.height,
                    device,
                    queue,
                );
                true
            }
            Err(e) => {
                tracing::warn!("failed to load image {key:?}: {e}");
                self.failed.insert(key);
                false
            }
        }
    }

    /// Insert pre-decoded RGBA data into the cache. Returns true on success.
    pub fn insert_rgba(
        &mut self,
        key: TextureKey,
        data: &[u8],
        width: u32,
        height: u32,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> bool {
        if self.entries.contains_key(&key) {
            let entry = self.entries.get_mut(&key).unwrap();
            entry.last_used_frame = self.frame_counter;
            return true;
        }

        self.insert_texture(key.clone(), data, width, height, device, queue);
        self.entries.contains_key(&key)
    }

    /// Get the texture view for a key that is already in the cache.
    pub fn get_view(&self, key: &TextureKey) -> Option<&wgpu::TextureView> {
        self.entries.get(key).map(|e| &e.view)
    }

    /// Get the pre-built bind group for a cached texture.
    pub fn get_bind_group(&self, key: &TextureKey) -> Option<&wgpu::BindGroup> {
        self.entries.get(key).map(|e| &e.bind_group)
    }

    /// Create a bind group for a texture view in this cache.
    pub fn create_bind_group(
        &self,
        device: &wgpu::Device,
        view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("texture_cache_bind_group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }

    /// Evict least-recently-used entries until within budget.
    pub fn evict_to_budget(&mut self) {
        while self.total_bytes > self.budget_bytes && !self.lru_order.is_empty() {
            let oldest_key = self.lru_order.remove(0);
            if let Some(entry) = self.entries.remove(&oldest_key) {
                self.total_bytes -= entry.byte_size;
                tracing::debug!(
                    "evicted texture {:?} ({}x{}, {} bytes)",
                    oldest_key,
                    entry.width,
                    entry.height,
                    entry.byte_size,
                );
            }
        }
    }

    /// Clear all cached textures.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.lru_order.clear();
        self.total_bytes = 0;
        self.pending.clear();
        self.failed.clear();
    }

    fn insert_texture(
        &mut self,
        key: TextureKey,
        data: &[u8],
        width: u32,
        height: u32,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        if width > MAX_TEXTURE_DIM || height > MAX_TEXTURE_DIM {
            tracing::warn!(
                "image too large: {width}x{height} (max {MAX_TEXTURE_DIM}x{MAX_TEXTURE_DIM})"
            );
            return;
        }

        let expected = (width as usize) * (height as usize) * 4;
        if data.len() != expected {
            tracing::warn!(
                "RGBA data size mismatch: expected {expected}, got {}",
                data.len()
            );
            return;
        }

        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cached_texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            size,
        );
        let view = texture.create_view(&Default::default());
        let bind_group = self.create_bind_group(device, &view);
        let byte_size = data.len();

        self.entries.insert(
            key.clone(),
            CachedTexture {
                _texture: texture,
                view,
                bind_group,
                width,
                height,
                byte_size,
                last_used_frame: self.frame_counter,
            },
        );
        self.lru_order.push(key);
        self.total_bytes += byte_size;
    }
}

/// Decode an image file to RGBA8. Returns (data, width, height).
fn load_image_file(path: &str) -> Result<(Vec<u8>, u32, u32), String> {
    if kasane_core::render::svg::is_svg_path(path) {
        let r = kasane_core::render::svg::render_svg_file_to_rgba_intrinsic(path, MAX_TEXTURE_DIM)
            .map_err(|e| e.to_string())?;
        return Ok((r.data, r.width, r.height));
    }
    let img = image::open(path).map_err(|e| format!("{e}"))?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    if w > MAX_TEXTURE_DIM || h > MAX_TEXTURE_DIM {
        return Err(format!(
            "image too large: {w}x{h} (max {MAX_TEXTURE_DIM}x{MAX_TEXTURE_DIM})"
        ));
    }
    Ok((rgba.into_raw(), w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_image_file_nonexistent() {
        let result = load_image_file("/nonexistent/path/test.png");
        assert!(result.is_err());
    }

    #[test]
    fn test_load_image_file_invalid_format() {
        // A text file is not a valid image
        let result = load_image_file("/dev/null");
        assert!(result.is_err());
    }
}
