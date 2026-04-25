//! GPU timestamp query support for per-pass timing measurement.
//!
//! Gracefully degrades to no-op when `Features::TIMESTAMP_QUERY` is unavailable.

/// Manages wgpu timestamp queries, resolve/readback buffers, and frame timing.
pub struct GpuTimingState {
    query_set: Option<wgpu::QuerySet>,
    resolve_buffer: Option<wgpu::Buffer>,
    readback_buffer: Option<wgpu::Buffer>,
    timestamp_period: f32,
    enabled: bool,
    /// Number of query pairs (begin+end) allocated. Each pass uses one pair.
    max_passes: u32,
    /// Names for each pass slot, set by the caller.
    pass_names: Vec<&'static str>,
    /// Latest resolved frame timings (populated after readback).
    latest_timings: Option<GpuFrameTimings>,
}

/// Resolved GPU timing data for a single frame.
#[derive(Debug, Clone)]
pub struct GpuFrameTimings {
    pub total_gpu_ms: f64,
    pub pass_timings: Vec<(&'static str, f64)>,
}

impl GpuTimingState {
    /// Maximum number of render passes we instrument (each needs 2 timestamp queries).
    const DEFAULT_MAX_PASSES: u32 = 8;

    /// Create a new timing state. If the device doesn't support timestamp queries,
    /// the state is created in disabled mode (all methods become no-ops).
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let supported = device.features().contains(wgpu::Features::TIMESTAMP_QUERY);

        if !supported {
            tracing::info!("GPU timestamp queries not supported, timing disabled");
            return Self {
                query_set: None,
                resolve_buffer: None,
                readback_buffer: None,
                timestamp_period: 0.0,
                enabled: false,
                max_passes: 0,
                pass_names: Vec::new(),
                latest_timings: None,
            };
        }

        let timestamp_period = queue.get_timestamp_period();
        let max_passes = Self::DEFAULT_MAX_PASSES;
        let query_count = max_passes * 2; // begin + end per pass

        let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("gpu_timing_query_set"),
            ty: wgpu::QueryType::Timestamp,
            count: query_count,
        });

        let resolve_size = (query_count as u64) * 8; // u64 per query
        let resolve_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu_timing_resolve"),
            size: resolve_size,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu_timing_readback"),
            size: resolve_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        tracing::info!("GPU timing enabled: period={timestamp_period}ns, max_passes={max_passes}");

        Self {
            query_set: Some(query_set),
            resolve_buffer: Some(resolve_buffer),
            readback_buffer: Some(readback_buffer),
            timestamp_period,
            enabled: true,
            max_passes,
            pass_names: Vec::with_capacity(max_passes as usize),
            latest_timings: None,
        }
    }

    /// Create a disabled timing state (for when GPU is not available yet).
    pub fn disabled() -> Self {
        Self {
            query_set: None,
            resolve_buffer: None,
            readback_buffer: None,
            timestamp_period: 0.0,
            enabled: false,
            max_passes: 0,
            pass_names: Vec::new(),
            latest_timings: None,
        }
    }

    /// Whether timing is active.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Reset pass names for a new frame.
    pub fn begin_frame(&mut self) {
        self.pass_names.clear();
    }

    /// Get the `TimestampWrites` descriptor for a render pass.
    /// Returns `None` if timing is disabled or all slots are used.
    pub fn timestamp_writes(
        &mut self,
        pass_name: &'static str,
    ) -> Option<wgpu::RenderPassTimestampWrites<'_>> {
        if !self.enabled {
            return None;
        }
        let pass_idx = self.pass_names.len() as u32;
        if pass_idx >= self.max_passes {
            return None;
        }
        self.pass_names.push(pass_name);
        let query_set = self.query_set.as_ref()?;
        Some(wgpu::RenderPassTimestampWrites {
            query_set,
            beginning_of_pass_write_index: Some(pass_idx * 2),
            end_of_pass_write_index: Some(pass_idx * 2 + 1),
        })
    }

    /// Resolve queries and copy to readback buffer. Call after all render passes
    /// in the frame, before queue.submit.
    pub fn resolve(&self, encoder: &mut wgpu::CommandEncoder) {
        if !self.enabled || self.pass_names.is_empty() {
            return;
        }
        let query_set = self.query_set.as_ref().unwrap();
        let resolve_buf = self.resolve_buffer.as_ref().unwrap();
        let readback_buf = self.readback_buffer.as_ref().unwrap();
        let query_count = self.pass_names.len() as u32 * 2;

        encoder.resolve_query_set(query_set, 0..query_count, resolve_buf, 0);
        encoder.copy_buffer_to_buffer(resolve_buf, 0, readback_buf, 0, query_count as u64 * 8);
    }

    /// Map the readback buffer and extract timings. Non-blocking: returns
    /// immediately and updates `latest_timings` when the map completes.
    pub fn readback(&mut self, device: &wgpu::Device) {
        if !self.enabled || self.pass_names.is_empty() {
            return;
        }
        let readback_buf = self.readback_buffer.as_ref().unwrap();
        let query_count = self.pass_names.len() as u32 * 2;
        let size = query_count as u64 * 8;

        let slice = readback_buf.slice(0..size);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        let _ = device.poll(wgpu::PollType::wait_indefinitely());

        if let Ok(Ok(())) = rx.recv() {
            let data = slice.get_mapped_range();
            let timestamps: &[u64] = bytemuck::cast_slice(&data);

            let period_ns = self.timestamp_period as f64;
            let mut pass_timings = Vec::with_capacity(self.pass_names.len());
            let mut total_ns: f64 = 0.0;

            for (i, name) in self.pass_names.iter().enumerate() {
                let begin = timestamps[i * 2];
                let end = timestamps[i * 2 + 1];
                let delta_ns = (end.wrapping_sub(begin)) as f64 * period_ns;
                let delta_ms = delta_ns / 1_000_000.0;
                pass_timings.push((*name, delta_ms));
                total_ns += delta_ns;
            }

            drop(data);
            readback_buf.unmap();

            self.latest_timings = Some(GpuFrameTimings {
                total_gpu_ms: total_ns / 1_000_000.0,
                pass_timings,
            });
        } else {
            readback_buf.unmap();
        }
    }

    /// Get the latest frame timings (from the previous frame's readback).
    pub fn latest_timings(&self) -> Option<&GpuFrameTimings> {
        self.latest_timings.as_ref()
    }
}

impl std::fmt::Display for GpuFrameTimings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "GPU {:.2}ms [", self.total_gpu_ms)?;
        for (i, (name, ms)) in self.pass_timings.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{name}={ms:.2}ms")?;
        }
        write!(f, "]")
    }
}
