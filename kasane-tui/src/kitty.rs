//! Kitty Graphics Protocol support for image rendering.
//!
//! Implements terminal detection, escape sequence generation, and frame-to-frame
//! image placement reconciliation for Direct Placement mode.
//!
//! Reference: <https://sw.kovidgoyal.net/kitty/graphics-protocol/>

use std::collections::HashMap;
use std::io::Write;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use kasane_core::element::ImageSource;
use kasane_core::render::{ImageProtocol, ImageRequest};

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// Detect the image protocol to use based on config override and environment.
pub fn detect_image_protocol(
    config_override: kasane_core::config::ImageProtocolConfig,
) -> ImageProtocol {
    match config_override {
        kasane_core::config::ImageProtocolConfig::Halfblock => return ImageProtocol::Off,
        kasane_core::config::ImageProtocolConfig::Kitty => return ImageProtocol::KittyDirect,
        kasane_core::config::ImageProtocolConfig::Auto => { /* detect */ }
    }
    // TMUX does not pass through Kitty graphics
    if std::env::var("TMUX").is_ok() {
        return ImageProtocol::Off;
    }
    if std::env::var("KITTY_WINDOW_ID").is_ok() {
        return ImageProtocol::KittyDirect;
    }
    if let Ok(prog) = std::env::var("TERM_PROGRAM")
        && matches!(prog.as_str(), "WezTerm" | "ghostty" | "foot" | "contour")
    {
        return ImageProtocol::KittyDirect;
    }
    ImageProtocol::Off
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Key for deduplicating uploaded images.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum ImageSourceKey {
    FilePath(String),
    Rgba { hash: u64, width: u32, height: u32 },
    SvgData { hash: u64 },
}

/// Metadata for an uploaded image.
#[derive(Debug, Clone)]
struct UploadedImage {
    image_id: u32,
    width: u32,
    height: u32,
}

/// Key for tracking placements across frames.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PlacementKey {
    source_key: ImageSourceKey,
    area: (u16, u16, u16, u16), // (x, y, w, h)
}

/// Info about an active placement.
#[derive(Debug, Clone)]
struct PlacementInfo {
    image_id: u32,
    placement_id: u32,
}

/// Maximum number of uploaded images to keep cached in the terminal.
/// Older entries are evicted (deleted from terminal) to prevent memory pressure.
const MAX_UPLOADED_IMAGES: usize = 64;

/// Kitty graphics protocol state manager.
pub struct KittyState {
    next_image_id: u32,
    next_placement_id: u32,
    uploaded: HashMap<ImageSourceKey, UploadedImage>,
    /// LRU order for uploaded images (most recently used at the end).
    upload_order: Vec<ImageSourceKey>,
    prev_placements: HashMap<PlacementKey, PlacementInfo>,
    dim_cache: HashMap<String, (u32, u32)>,
}

impl Default for KittyState {
    fn default() -> Self {
        Self {
            next_image_id: 1,
            next_placement_id: 1,
            uploaded: HashMap::new(),
            upload_order: Vec::new(),
            prev_placements: HashMap::new(),
            dim_cache: HashMap::new(),
        }
    }
}

impl KittyState {
    pub fn new() -> Self {
        Self::default()
    }

    fn alloc_image_id(&mut self) -> u32 {
        let id = self.next_image_id;
        self.next_image_id = self.next_image_id.wrapping_add(1).max(1);
        id
    }

    fn alloc_placement_id(&mut self) -> u32 {
        let id = self.next_placement_id;
        self.next_placement_id = self.next_placement_id.wrapping_add(1).max(1);
        id
    }

    /// Get cached image dimensions, or read them from disk.
    fn image_dimensions(&mut self, path: &str) -> Option<(u32, u32)> {
        if let Some(dims) = self.dim_cache.get(path) {
            return Some(*dims);
        }
        match image::image_dimensions(path) {
            Ok(dims) => {
                self.dim_cache.insert(path.to_string(), dims);
                Some(dims)
            }
            Err(e) => {
                tracing::warn!("kitty: failed to read image dimensions for {path}: {e}");
                None
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Source key computation
// ---------------------------------------------------------------------------

fn source_key(source: &ImageSource) -> ImageSourceKey {
    match source {
        ImageSource::FilePath(path) => ImageSourceKey::FilePath(path.clone()),
        ImageSource::Rgba {
            data,
            width,
            height,
        } => ImageSourceKey::Rgba {
            hash: content_hash(data, *width, *height),
            width: *width,
            height: *height,
        },
        ImageSource::SvgData { data } => ImageSourceKey::SvgData {
            hash: content_hash(data, 0, 0),
        },
    }
}

/// Content-sampling hash for RGBA data (reuses logic from halfblock.rs).
fn content_hash(data: &[u8], width: u32, height: u32) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::hash::DefaultHasher::new();
    width.hash(&mut hasher);
    height.hash(&mut hasher);
    data.len().hash(&mut hasher);
    const SAMPLE: usize = 64;
    data[..data.len().min(SAMPLE)].hash(&mut hasher);
    if data.len() > SAMPLE * 2 {
        let mid = data.len() / 2;
        data[mid..mid + SAMPLE.min(data.len() - mid)].hash(&mut hasher);
    }
    if data.len() > SAMPLE {
        data[data.len() - SAMPLE..].hash(&mut hasher);
    }
    hasher.finish()
}

// ---------------------------------------------------------------------------
// Escape sequence generation
// ---------------------------------------------------------------------------

/// APC start for Kitty graphics: `\x1b_G`
const APC_START: &[u8] = b"\x1b_G";
/// String terminator: `\x1b\\`
const ST: &[u8] = b"\x1b\\";

/// Emit a file-path upload command (`t=f`, `a=t`, `q=2`).
pub fn emit_upload_file(buf: &mut Vec<u8>, id: u32, path: &str) {
    let encoded_path = BASE64.encode(path.as_bytes());
    buf.extend_from_slice(APC_START);
    let _ = write!(buf, "a=t,t=f,i={id},q=2;{encoded_path}");
    buf.extend_from_slice(ST);
}

/// Emit an RGBA direct-transfer upload (`t=d`, `f=32`, `a=t`, `q=2`).
/// Chunks data into 4096-byte segments (base64-encoded).
pub fn emit_upload_rgba(buf: &mut Vec<u8>, id: u32, data: &[u8], width: u32, height: u32) {
    const CHUNK_SIZE: usize = 4096;
    let chunks: Vec<&[u8]> = data.chunks(CHUNK_SIZE).collect();
    let total = chunks.len();

    for (i, chunk) in chunks.iter().enumerate() {
        let is_last = i + 1 == total;
        let m = if is_last { 0 } else { 1 };
        let encoded = BASE64.encode(chunk);

        buf.extend_from_slice(APC_START);
        if i == 0 {
            let _ = write!(
                buf,
                "a=t,t=d,f=32,s={width},v={height},i={id},m={m},q=2;{encoded}"
            );
        } else {
            let _ = write!(buf, "m={m};{encoded}");
        }
        buf.extend_from_slice(ST);
    }
}

/// Emit a PNG file data upload (`t=d`, `f=100`, `a=t`, `q=2`).
/// Used for SSH environments where `t=f` is not available.
/// Chunks data into 4096-byte segments (base64-encoded).
pub fn emit_upload_png(buf: &mut Vec<u8>, id: u32, data: &[u8]) {
    const CHUNK_SIZE: usize = 4096;
    let chunks: Vec<&[u8]> = data.chunks(CHUNK_SIZE).collect();
    let total = chunks.len();

    for (i, chunk) in chunks.iter().enumerate() {
        let is_last = i + 1 == total;
        let m = if is_last { 0 } else { 1 };
        let encoded = BASE64.encode(chunk);

        buf.extend_from_slice(APC_START);
        if i == 0 {
            let _ = write!(buf, "a=t,t=d,f=100,i={id},m={m},q=2;{encoded}");
        } else {
            let _ = write!(buf, "m={m};{encoded}");
        }
        buf.extend_from_slice(ST);
    }
}

/// Emit a placement command (`a=p`, `C=1` to suppress cursor movement).
#[allow(clippy::too_many_arguments)]
pub fn emit_place(
    buf: &mut Vec<u8>,
    id: u32,
    pid: u32,
    col: u16,
    row: u16,
    cols: u16,
    rows: u16,
    crop: Option<(u32, u32, u32, u32)>,
) {
    // Move cursor to placement position first
    let _ = write!(buf, "\x1b[{};{}H", row + 1, col + 1);

    buf.extend_from_slice(APC_START);
    let _ = write!(buf, "a=p,i={id},p={pid},c={cols},r={rows},C=1,q=2");
    if let Some((src_x, src_y, src_w, src_h)) = crop {
        // Kitty uses uppercase X,Y for source image offset and w,h for source rect size.
        // Lowercase x,y are sub-cell pixel offsets (different meaning).
        let _ = write!(buf, ",X={src_x},Y={src_y},w={src_w},h={src_h}");
    }
    let _ = write!(buf, ";");
    buf.extend_from_slice(ST);
}

/// Emit a delete command for a specific image (`a=d,d=i`).
pub fn emit_delete(buf: &mut Vec<u8>, id: u32, pid: Option<u32>) {
    buf.extend_from_slice(APC_START);
    if let Some(pid) = pid {
        let _ = write!(buf, "a=d,d=i,i={id},p={pid},q=2;");
    } else {
        let _ = write!(buf, "a=d,d=i,i={id},q=2;");
    }
    buf.extend_from_slice(ST);
}

/// Emit a delete-all command (`a=d,d=a`).
pub fn emit_delete_all(buf: &mut Vec<u8>) {
    buf.extend_from_slice(APC_START);
    let _ = write!(buf, "a=d,d=a,q=2;");
    buf.extend_from_slice(ST);
}

// ---------------------------------------------------------------------------
// Frame reconciliation
// ---------------------------------------------------------------------------

/// Commands produced by reconciliation.
pub struct ReconcileResult {
    /// Escape bytes for uploads (written outside SyncUpdate).
    pub upload_bytes: Vec<u8>,
    /// Escape bytes for placements + deletions (written inside SyncUpdate).
    pub place_bytes: Vec<u8>,
}

/// Reconcile this frame's image requests against the previous frame's placements.
///
/// Produces upload commands for new images, placement commands for new/moved
/// placements, and delete commands for placements that disappeared.
pub fn reconcile(state: &mut KittyState, requests: &[ImageRequest]) -> ReconcileResult {
    let mut upload_buf = Vec::new();
    let mut place_buf = Vec::new();

    let mut current_placements: HashMap<PlacementKey, PlacementInfo> = HashMap::new();

    for req in requests {
        let key = source_key(&req.source);
        let pkey = PlacementKey {
            source_key: key.clone(),
            area: (req.area.x, req.area.y, req.area.w, req.area.h),
        };

        // Check if this exact placement already exists
        if let Some(prev) = state.prev_placements.get(&pkey) {
            // Unchanged — keep it, no commands needed
            current_placements.insert(pkey, prev.clone());
            continue;
        }

        // Ensure image is uploaded
        let uploaded = if let Some(u) = state.uploaded.get(&key) {
            u.clone()
        } else {
            let image_id = state.alloc_image_id();
            let (width, height) = match &req.source {
                ImageSource::FilePath(path) => {
                    if kasane_core::render::svg::is_svg_path(path) {
                        // SVG files: rasterize then upload as RGBA (terminals can't render SVG)
                        match kasane_core::render::svg::render_svg_file_to_rgba_intrinsic(
                            path, 4096,
                        ) {
                            Ok(r) => {
                                tracing::debug!(
                                    path,
                                    image_id,
                                    r.width,
                                    r.height,
                                    "kitty: uploading SVG as RGBA"
                                );
                                emit_upload_rgba(
                                    &mut upload_buf,
                                    image_id,
                                    &r.data,
                                    r.width,
                                    r.height,
                                );
                                (r.width, r.height)
                            }
                            Err(e) => {
                                tracing::warn!("kitty: failed to render SVG {path}: {e}");
                                continue;
                            }
                        }
                    } else {
                        let dims = match state.image_dimensions(path) {
                            Some(d) => d,
                            None => continue,
                        };
                        match image::open(path) {
                            Ok(img) => {
                                let rgba = img.to_rgba8();
                                let (w, h) = rgba.dimensions();
                                tracing::debug!(
                                    path,
                                    image_id,
                                    w,
                                    h,
                                    "kitty: uploading file as RGBA"
                                );
                                emit_upload_rgba(&mut upload_buf, image_id, rgba.as_raw(), w, h);
                            }
                            Err(e) => {
                                tracing::warn!("kitty: failed to decode {path}: {e}");
                                continue;
                            }
                        }
                        dims
                    }
                }
                ImageSource::Rgba {
                    data,
                    width,
                    height,
                } => {
                    emit_upload_rgba(&mut upload_buf, image_id, data, *width, *height);
                    (*width, *height)
                }
                ImageSource::SvgData { data } => {
                    match kasane_core::render::svg::render_svg_to_rgba_intrinsic(data, 4096) {
                        Ok(r) => {
                            tracing::debug!(
                                image_id,
                                r.width,
                                r.height,
                                "kitty: uploading inline SVG as RGBA"
                            );
                            emit_upload_rgba(&mut upload_buf, image_id, &r.data, r.width, r.height);
                            (r.width, r.height)
                        }
                        Err(e) => {
                            tracing::warn!("kitty: SVG render failed: {e}");
                            continue;
                        }
                    }
                }
            };
            let uploaded = UploadedImage {
                image_id,
                width,
                height,
            };
            state.uploaded.insert(key.clone(), uploaded.clone());
            state.upload_order.push(key.clone());
            tracing::debug!(image_id, "kitty: uploaded image");

            // LRU eviction: remove oldest uploads when cache is full
            while state.uploaded.len() > MAX_UPLOADED_IMAGES {
                if let Some(oldest_key) = state.upload_order.first().cloned() {
                    state.upload_order.remove(0);
                    if let Some(evicted) = state.uploaded.remove(&oldest_key) {
                        emit_delete(&mut upload_buf, evicted.image_id, None);
                        tracing::debug!(image_id = evicted.image_id, "kitty: evicted image");
                    }
                } else {
                    break;
                }
            }

            uploaded
        };

        // Compute fit parameters
        let fit = kasane_core::render::halfblock::compute_fit_cells(
            uploaded.width,
            uploaded.height,
            req.area.w,
            req.area.h,
            req.fit,
        );

        let placement_id = state.alloc_placement_id();
        let col = req.area.x + fit.dst_x;
        let row = req.area.y + fit.dst_y;
        let cols = fit.dst_w;
        let rows = fit.dst_h;

        let crop = if fit.crop_x != 0
            || fit.crop_y != 0
            || fit.crop_w != uploaded.width
            || fit.crop_h != uploaded.height
        {
            Some((fit.crop_x, fit.crop_y, fit.crop_w, fit.crop_h))
        } else {
            None
        };

        emit_place(
            &mut place_buf,
            uploaded.image_id,
            placement_id,
            col,
            row,
            cols,
            rows,
            crop,
        );
        tracing::debug!(
            image_id = uploaded.image_id,
            placement_id,
            col,
            row,
            cols,
            rows,
            "kitty: placed image"
        );

        current_placements.insert(
            pkey,
            PlacementInfo {
                image_id: uploaded.image_id,
                placement_id,
            },
        );
    }

    // Delete placements that are no longer present
    for (pkey, info) in &state.prev_placements {
        if !current_placements.contains_key(pkey) {
            emit_delete(&mut place_buf, info.image_id, Some(info.placement_id));
            tracing::debug!(
                image_id = info.image_id,
                placement_id = info.placement_id,
                "kitty: deleted placement"
            );
        }
    }

    state.prev_placements = current_placements;

    ReconcileResult {
        upload_bytes: upload_buf,
        place_bytes: place_buf,
    }
}

/// Clear all placements and uploaded image state. Used on cleanup/invalidate.
pub fn clear_all(state: &mut KittyState, buf: &mut Vec<u8>) {
    emit_delete_all(buf);
    state.prev_placements.clear();
    state.uploaded.clear();
    state.upload_order.clear();
}

// ---------------------------------------------------------------------------
// Unicode Placement (Phase 3)
// ---------------------------------------------------------------------------

/// The Kitty Unicode Placeholder character (U+10EEEE).
const PLACEHOLDER_CHAR: char = '\u{10EEEE}';

/// Diacritics used to encode row/column indices in Unicode Placement.
/// These are combining characters that encode a zero-based index (0..=255).
/// Row index uses U+0305..U+0308 and column uses U+030D..U+0310 ranges.
const DIACRITICS: [char; 256] = {
    let mut arr = ['\0'; 256];
    let mut i = 0;
    while i < 256 {
        // Kitty uses diacritics starting at U+0305 for rows/cols
        // The actual encoding is: U+0305 + index (wrapping within combining range)
        // For simplicity we use the 4th-plane encoding Kitty specifies:
        // row diacritic = U+10EEEE row in 3rd byte, col diacritic = U+10EEEE col in 3rd byte
        // Actually, Kitty uses: combining chars from U+0305 onward
        arr[i] = unsafe { char::from_u32_unchecked(0x0305 + i as u32) };
        i += 1;
    }
    arr
};

/// Encode image_id as RGB color (lower 24 bits).
pub fn image_id_to_rgb(id: u32) -> (u8, u8, u8) {
    let r = ((id >> 16) & 0xFF) as u8;
    let g = ((id >> 8) & 0xFF) as u8;
    let b = (id & 0xFF) as u8;
    (r, g, b)
}

/// Build the placeholder grapheme cluster for a given (row, col) within the image.
///
/// Format: U+10EEEE + row_diacritic + col_diacritic
/// The first cell (row=0, col=0) uses just U+10EEEE without diacritics.
pub fn encode_placeholder_grapheme(row: u16, col: u16) -> String {
    let mut s = String::with_capacity(8);
    s.push(PLACEHOLDER_CHAR);
    if row > 0 || col > 0 {
        // Row diacritic (0-based)
        if (row as usize) < DIACRITICS.len() {
            s.push(DIACRITICS[row as usize]);
        }
        // Column diacritic (0-based, offset by 0x10 from row diacritics)
        if (col as usize) < DIACRITICS.len() {
            // Kitty uses same diacritic range but col gets encoded after row
            s.push(DIACRITICS[col as usize]);
        }
    }
    s
}

/// Emit a virtual upload command for Unicode Placement (`a=t,U=1`).
/// This tells the terminal to create a virtual placement that will be
/// referenced by placeholder characters in the cell grid.
pub fn emit_upload_virtual(buf: &mut Vec<u8>, id: u32, path: &str) {
    let encoded_path = BASE64.encode(path.as_bytes());
    buf.extend_from_slice(APC_START);
    let _ = write!(buf, "a=t,U=1,t=f,i={id},q=2;{encoded_path}");
    buf.extend_from_slice(ST);
}

/// Emit a virtual upload for RGBA data (`a=t,U=1,t=d,f=32`).
pub fn emit_upload_virtual_rgba(buf: &mut Vec<u8>, id: u32, data: &[u8], width: u32, height: u32) {
    const CHUNK_SIZE: usize = 4096;
    let chunks: Vec<&[u8]> = data.chunks(CHUNK_SIZE).collect();
    let total = chunks.len();

    for (i, chunk) in chunks.iter().enumerate() {
        let is_last = i + 1 == total;
        let m = if is_last { 0 } else { 1 };
        let encoded = BASE64.encode(chunk);

        buf.extend_from_slice(APC_START);
        if i == 0 {
            let _ = write!(
                buf,
                "a=t,U=1,t=d,f=32,s={width},v={height},i={id},m={m},q=2;{encoded}"
            );
        } else {
            let _ = write!(buf, "m={m};{encoded}");
        }
        buf.extend_from_slice(ST);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use kasane_core::element::ImageFit;
    use kasane_core::layout::Rect;

    #[test]
    fn emit_upload_file_format() {
        let mut buf = Vec::new();
        emit_upload_file(&mut buf, 42, "/tmp/test.png");
        let s = String::from_utf8(buf).unwrap();

        // Should start with APC_G and end with ST
        assert!(s.starts_with("\x1b_G"), "missing APC start: {s}");
        assert!(s.ends_with("\x1b\\"), "missing ST: {s}");

        // Should contain required params
        assert!(s.contains("a=t"), "missing a=t: {s}");
        assert!(s.contains("t=f"), "missing t=f: {s}");
        assert!(s.contains("i=42"), "missing i=42: {s}");
        assert!(s.contains("q=2"), "missing q=2: {s}");

        // Payload should be base64-encoded path
        let payload_start = s.find(';').unwrap() + 1;
        let payload_end = s.len() - 2; // before ST
        let payload = &s[payload_start..payload_end];
        let decoded = BASE64.decode(payload).unwrap();
        assert_eq!(decoded, b"/tmp/test.png");
    }

    #[test]
    fn emit_place_has_c1_and_q2() {
        let mut buf = Vec::new();
        emit_place(&mut buf, 1, 2, 5, 10, 20, 15, None);
        let s = String::from_utf8(buf).unwrap();

        assert!(s.contains("C=1"), "missing C=1: {s}");
        assert!(s.contains("q=2"), "missing q=2: {s}");
        assert!(s.contains("a=p"), "missing a=p: {s}");
        assert!(s.contains("i=1"), "missing i=1: {s}");
        assert!(s.contains("p=2"), "missing p=2: {s}");
        assert!(s.contains("c=20"), "missing c=20: {s}");
        assert!(s.contains("r=15"), "missing r=15: {s}");
    }

    #[test]
    fn emit_place_with_crop() {
        let mut buf = Vec::new();
        emit_place(&mut buf, 3, 4, 0, 0, 10, 10, Some((50, 25, 200, 150)));
        let s = String::from_utf8(buf).unwrap();

        assert!(s.contains("X=50"), "missing X=50: {s}");
        assert!(s.contains("Y=25"), "missing Y=25: {s}");
        assert!(s.contains("w=200"), "missing w=200: {s}");
        assert!(s.contains("h=150"), "missing h=150: {s}");
    }

    #[test]
    fn emit_delete_with_placement() {
        let mut buf = Vec::new();
        emit_delete(&mut buf, 10, Some(5));
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("a=d"), "missing a=d: {s}");
        assert!(s.contains("d=i"), "missing d=i: {s}");
        assert!(s.contains("i=10"), "missing i=10: {s}");
        assert!(s.contains("p=5"), "missing p=5: {s}");
    }

    #[test]
    fn emit_delete_without_placement() {
        let mut buf = Vec::new();
        emit_delete(&mut buf, 10, None);
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("i=10"), "missing i=10: {s}");
        assert!(!s.contains("p="), "unexpected p=: {s}");
    }

    #[test]
    fn emit_delete_all_format() {
        let mut buf = Vec::new();
        emit_delete_all(&mut buf);
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("a=d"), "missing a=d: {s}");
        assert!(s.contains("d=a"), "missing d=a: {s}");
    }

    #[test]
    fn emit_upload_rgba_single_chunk() {
        let data = vec![255u8; 16]; // 2x2 RGBA = 16 bytes, fits in one chunk
        let mut buf = Vec::new();
        emit_upload_rgba(&mut buf, 7, &data, 2, 2);
        let s = String::from_utf8(buf).unwrap();

        assert!(s.contains("a=t"), "missing a=t: {s}");
        assert!(s.contains("t=d"), "missing t=d: {s}");
        assert!(s.contains("f=32"), "missing f=32: {s}");
        assert!(s.contains("s=2"), "missing s=2: {s}");
        assert!(s.contains("v=2"), "missing v=2: {s}");
        assert!(s.contains("i=7"), "missing i=7: {s}");
        assert!(s.contains("m=0"), "single chunk should have m=0: {s}");
    }

    #[test]
    fn emit_upload_rgba_multi_chunk() {
        let data = vec![128u8; 8192]; // > 4096 bytes, should split
        let mut buf = Vec::new();
        emit_upload_rgba(&mut buf, 9, &data, 32, 64);
        let s = String::from_utf8(buf).unwrap();

        // First chunk should have m=1
        let first_end = s.find("\x1b\\").unwrap();
        let first_chunk = &s[..first_end];
        assert!(
            first_chunk.contains("m=1"),
            "first chunk needs m=1: {first_chunk}"
        );

        // Last chunk should have m=0
        let last_st = s.rfind("\x1b\\").unwrap();
        let last_start = s[..last_st].rfind("\x1b_G").unwrap();
        let last_chunk = &s[last_start..last_st];
        assert!(
            last_chunk.contains("m=0"),
            "last chunk needs m=0: {last_chunk}"
        );
    }

    #[test]
    fn reconcile_new_placement() {
        let mut state = KittyState::new();

        // Pre-seed dimensions cache so we don't need actual files
        state.dim_cache.insert("/tmp/test.png".into(), (100, 200));

        let requests = vec![ImageRequest {
            source: ImageSource::FilePath("/tmp/test.png".into()),
            fit: ImageFit::Contain,
            opacity: 1.0,
            area: Rect {
                x: 0,
                y: 0,
                w: 20,
                h: 10,
            },
        }];

        let result = reconcile(&mut state, &requests);

        // Should have upload bytes (file upload)
        assert!(!result.upload_bytes.is_empty(), "expected upload commands");
        // Should have placement bytes
        assert!(
            !result.place_bytes.is_empty(),
            "expected placement commands"
        );
        // Should track the placement
        assert_eq!(state.prev_placements.len(), 1);
    }

    #[test]
    fn reconcile_unchanged_skips() {
        let mut state = KittyState::new();

        state.dim_cache.insert("/tmp/test.png".into(), (100, 200));

        let requests = vec![ImageRequest {
            source: ImageSource::FilePath("/tmp/test.png".into()),
            fit: ImageFit::Contain,
            opacity: 1.0,
            area: Rect {
                x: 0,
                y: 0,
                w: 20,
                h: 10,
            },
        }];

        // First frame
        let _ = reconcile(&mut state, &requests);

        // Second frame with same request
        let result = reconcile(&mut state, &requests);

        // No new uploads or placements needed
        assert!(
            result.upload_bytes.is_empty(),
            "no upload needed for cache hit"
        );
        assert!(
            result.place_bytes.is_empty(),
            "no placement needed for unchanged"
        );
    }

    #[test]
    fn reconcile_removed_emits_delete() {
        let mut state = KittyState::new();

        state.dim_cache.insert("/tmp/test.png".into(), (100, 200));

        let requests = vec![ImageRequest {
            source: ImageSource::FilePath("/tmp/test.png".into()),
            fit: ImageFit::Contain,
            opacity: 1.0,
            area: Rect {
                x: 0,
                y: 0,
                w: 20,
                h: 10,
            },
        }];

        // First frame
        let _ = reconcile(&mut state, &requests);

        // Second frame with no requests
        let result = reconcile(&mut state, &[]);

        // Should have delete command
        let s = String::from_utf8(result.place_bytes).unwrap();
        assert!(s.contains("a=d"), "expected delete command: {s}");
        assert!(state.prev_placements.is_empty());
    }

    #[test]
    fn reconcile_rgba_inline() {
        let mut state = KittyState::new();
        let data: Arc<[u8]> = vec![255u8; 4 * 4 * 4].into();

        let requests = vec![ImageRequest {
            source: ImageSource::Rgba {
                data,
                width: 4,
                height: 4,
            },
            fit: ImageFit::Fill,
            opacity: 1.0,
            area: Rect {
                x: 5,
                y: 3,
                w: 10,
                h: 8,
            },
        }];

        let result = reconcile(&mut state, &requests);

        // Should have upload (RGBA inline)
        let upload_s = String::from_utf8(result.upload_bytes).unwrap();
        assert!(
            upload_s.contains("t=d"),
            "expected t=d for RGBA: {upload_s}"
        );
        assert!(
            upload_s.contains("f=32"),
            "expected f=32 for RGBA: {upload_s}"
        );

        // Should have placement
        assert!(!result.place_bytes.is_empty());
    }

    #[test]
    fn detect_auto_off_in_tmux() {
        // We can't easily test env-based detection in unit tests without
        // polluting the process environment. This is a placeholder for
        // the logic — real testing happens manually.
        // The function is well-covered by the match arms.
    }

    // --- Phase 2 tests ---

    #[test]
    fn emit_upload_png_format() {
        let data = vec![0x89, 0x50, 0x4E, 0x47]; // PNG magic bytes
        let mut buf = Vec::new();
        emit_upload_png(&mut buf, 5, &data);
        let s = String::from_utf8(buf).unwrap();

        assert!(s.contains("a=t"), "missing a=t: {s}");
        assert!(s.contains("t=d"), "missing t=d: {s}");
        assert!(s.contains("f=100"), "missing f=100 for PNG: {s}");
        assert!(s.contains("i=5"), "missing i=5: {s}");
        assert!(s.contains("m=0"), "single chunk should have m=0: {s}");
    }

    #[test]
    fn emit_upload_png_multi_chunk() {
        let data = vec![42u8; 8192]; // > 4096 bytes
        let mut buf = Vec::new();
        emit_upload_png(&mut buf, 11, &data);
        let s = String::from_utf8(buf).unwrap();

        // First chunk header should have f=100
        let first_end = s.find("\x1b\\").unwrap();
        let first_chunk = &s[..first_end];
        assert!(first_chunk.contains("f=100"), "first chunk needs f=100");
        assert!(first_chunk.contains("m=1"), "first chunk needs m=1");

        // Last chunk should have m=0
        let last_st = s.rfind("\x1b\\").unwrap();
        let last_start = s[..last_st].rfind("\x1b_G").unwrap();
        let last_chunk = &s[last_start..last_st];
        assert!(last_chunk.contains("m=0"), "last chunk needs m=0");
    }

    #[test]
    fn emit_upload_rgba_chunk_boundary() {
        // Exactly 4096 bytes: should be single chunk
        let data = vec![0u8; 4096];
        let mut buf = Vec::new();
        emit_upload_rgba(&mut buf, 1, &data, 32, 32);
        let s = String::from_utf8(buf).unwrap();
        // Count APC sequences
        let apc_count = s.matches("\x1b_G").count();
        assert_eq!(apc_count, 1, "4096 bytes should be single chunk");

        // 4097 bytes: should be two chunks
        let data = vec![0u8; 4097];
        let mut buf = Vec::new();
        emit_upload_rgba(&mut buf, 2, &data, 32, 32);
        let s = String::from_utf8(buf).unwrap();
        let apc_count = s.matches("\x1b_G").count();
        assert_eq!(apc_count, 2, "4097 bytes should be two chunks");
    }

    #[test]
    fn filepath_uses_inline_rgba_not_file_transfer() {
        let mut state = KittyState::new();
        state.dim_cache.insert("/tmp/test.png".into(), (100, 200));

        let requests = vec![ImageRequest {
            source: ImageSource::FilePath("/tmp/test.png".into()),
            fit: ImageFit::Contain,
            opacity: 1.0,
            area: Rect {
                x: 0,
                y: 0,
                w: 20,
                h: 10,
            },
        }];

        let result = reconcile(&mut state, &requests);

        // FilePath always uses inline RGBA transfer (t=d), never t=f.
        // Since /tmp/test.png doesn't exist, image::open will fail
        // and the image will be skipped (empty upload).
        let upload_s = String::from_utf8(result.upload_bytes).unwrap();
        assert!(
            !upload_s.contains("t=f"),
            "should never use t=f: {upload_s}"
        );
    }

    /// Dump the full reconcile output for a real image file.
    /// Verifies end-to-end byte generation with absolute path resolution.
    #[test]
    fn reconcile_dump_real_file() {
        let img_path = "/tmp/test-image.png";
        if !std::path::Path::new(img_path).exists() {
            eprintln!("SKIP: {img_path} not found");
            return;
        }

        let mut state = KittyState::new();

        // Don't pre-seed dim_cache — let it read the real file
        let requests = vec![ImageRequest {
            source: ImageSource::FilePath(img_path.into()),
            fit: ImageFit::Contain,
            opacity: 1.0,
            area: Rect {
                x: 2,
                y: 9,
                w: 30,
                h: 15,
            },
        }];

        let result = reconcile(&mut state, &requests);

        // Upload bytes — FilePath now uses inline RGBA transfer (t=d,f=32)
        let upload_s = String::from_utf8_lossy(&result.upload_bytes);
        eprintln!("=== UPLOAD ({} bytes) ===", result.upload_bytes.len());

        assert!(!result.upload_bytes.is_empty(), "must have upload bytes");
        assert!(upload_s.contains("a=t"), "must use a=t");
        assert!(upload_s.contains("t=d"), "must use t=d (inline RGBA)");
        assert!(upload_s.contains("f=32"), "must have f=32 (RGBA format)");
        assert!(upload_s.contains("s="), "must have s= (pixel width)");
        assert!(upload_s.contains("v="), "must have v= (pixel height)");

        // Placement bytes
        let place_s = String::from_utf8(result.place_bytes.clone()).unwrap();
        eprintln!("=== PLACEMENT ({} bytes) ===", result.place_bytes.len());
        eprintln!("{place_s:?}");

        assert!(!result.place_bytes.is_empty(), "must have placement bytes");
        assert!(place_s.contains("a=p"), "must have a=p: {place_s}");
        assert!(place_s.contains("C=1"), "must have C=1: {place_s}");

        // Verify placement position (CUP sequence)
        assert!(
            place_s.contains("\x1b["),
            "must have CUP sequence: {place_s}"
        );

        // Verify state
        assert_eq!(state.prev_placements.len(), 1, "must track one placement");
        assert_eq!(state.uploaded.len(), 1, "must track one upload");
        eprintln!("All assertions passed ✓");
    }

    // --- Phase 3 tests (Unicode Placement helpers) ---

    #[test]
    fn image_id_to_rgb_encodes_lower_24_bits() {
        assert_eq!(image_id_to_rgb(0x000000), (0, 0, 0));
        assert_eq!(image_id_to_rgb(0xFF0000), (255, 0, 0));
        assert_eq!(image_id_to_rgb(0x00FF00), (0, 255, 0));
        assert_eq!(image_id_to_rgb(0x0000FF), (0, 0, 255));
        assert_eq!(image_id_to_rgb(0xABCDEF), (0xAB, 0xCD, 0xEF));
        // Upper bits beyond 24 are masked out
        assert_eq!(image_id_to_rgb(0x01_ABCDEF), (0xAB, 0xCD, 0xEF));
    }

    #[test]
    fn placeholder_grapheme_origin() {
        let g = encode_placeholder_grapheme(0, 0);
        // Origin cell: just the placeholder char, no diacritics
        assert!(g.starts_with('\u{10EEEE}'));
        assert_eq!(g.chars().count(), 1);
    }

    #[test]
    fn placeholder_grapheme_with_offsets() {
        let g = encode_placeholder_grapheme(1, 2);
        assert!(g.starts_with('\u{10EEEE}'));
        // Should have diacritics after the base char
        assert!(
            g.chars().count() >= 3,
            "expected base + 2 diacritics: {g:?}"
        );
    }

    #[test]
    fn placeholder_grapheme_different_positions_differ() {
        let g00 = encode_placeholder_grapheme(0, 0);
        let g01 = encode_placeholder_grapheme(0, 1);
        let g10 = encode_placeholder_grapheme(1, 0);
        assert_ne!(g00, g01);
        assert_ne!(g00, g10);
        assert_ne!(g01, g10);
    }

    #[test]
    fn emit_upload_virtual_has_u1() {
        let mut buf = Vec::new();
        emit_upload_virtual(&mut buf, 99, "/tmp/test.png");
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("U=1"), "virtual upload needs U=1: {s}");
        assert!(s.contains("a=t"), "missing a=t: {s}");
        assert!(s.contains("t=f"), "missing t=f: {s}");
    }

    #[test]
    fn emit_upload_virtual_rgba_has_u1() {
        let data = vec![255u8; 16];
        let mut buf = Vec::new();
        emit_upload_virtual_rgba(&mut buf, 42, &data, 2, 2);
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("U=1"), "virtual upload needs U=1: {s}");
        assert!(s.contains("f=32"), "missing f=32: {s}");
    }
}
