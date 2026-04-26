//! Stable font identity derivation for the L2 raster cache (ADR-031, Phase 9b).
//!
//! The L2 [`GlyphRasterCache`](super::raster_cache::GlyphRasterCache) keys
//! glyph bitmaps by `(font_id, glyph_id, size_q, subpx_x, var_hash, hint)`.
//! The `font_id` half must:
//!
//! - Be stable across frames for the same `(font data, face index)` pair so
//!   that L2 hits accumulate.
//! - Discriminate between fonts that produce different glyph shapes for the
//!   same `glyph_id` (different families, different .ttc face indices).
//! - **Not** discriminate by size or by variable-font axes — those are
//!   already captured in `size_q` and `var_hash`.
//!
//! Implementation: `(blob_id ^ index_mix)` reduced to `u32`. The blob id
//! comes from `linebender_resource_handle::Blob`, which assigns each loaded
//! font file a stable monotonic id; mixing in the index distinguishes the
//! faces of a `.ttc` collection. The `var_hash` half of the cache key is
//! computed separately by [`var_hash_from_coords`].

use std::hash::{Hash, Hasher};

use parley::FontData;
use rustc_hash::FxHasher;

/// Stable font identity for the L2 cache.
///
/// Returns the same `u32` for two `FontData` references that share a
/// `(blob_id, index)` pair, regardless of where in the layout they appear.
pub fn font_id_from_data(font: &FontData) -> u32 {
    let blob_id = font.data.id();
    let mixed = blob_id ^ ((font.index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    // Fold to u32 by xoring high and low halves. Plenty of entropy for the
    // typical handful of fonts loaded by an editor session.
    let folded = (mixed >> 32) as u32 ^ (mixed as u32);
    // Avoid 0 — some debugging code uses 0 as an "unset" sentinel.
    if folded == 0 { 1 } else { folded }
}

/// Hash a slice of variable-font normalised coordinates into a `u32` for
/// the L2 cache key.
///
/// `parley::Run::normalized_coords` returns `&[i16]`; an empty slice maps to
/// 0 so static-font glyph entries do not pay any extra distinction cost.
pub fn var_hash_from_coords(coords: &[i16]) -> u32 {
    if coords.is_empty() {
        return 0;
    }
    let mut h = FxHasher::default();
    coords.hash(&mut h);
    let v = h.finish();
    ((v >> 32) as u32 ^ (v as u32)).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kasane_core::config::FontConfig;

    #[test]
    fn font_id_is_stable_for_real_layout_runs() {
        // The only stable way to construct FontData without taking a direct
        // dep on linebender_resource_handle is to drive Parley end-to-end and
        // pull the FontData off a real shaped run. This also exercises the
        // intended use site (Phase 9b).
        use super::super::shaper::shape_line_with_default_family;
        use super::super::styled_line::StyledLine;
        use super::super::{Brush, ParleyText};
        use kasane_core::protocol::{Atom, Face, Style};

        let mut text = ParleyText::new(&FontConfig::default());
        let atoms = vec![Atom {
            face: Face::default(),
            contents: "M".into(),
        }];
        let line = StyledLine::from_atoms(
            &atoms,
            &Style::default(),
            Brush::opaque(255, 255, 255),
            14.0,
            None,
        );
        let layout = shape_line_with_default_family(&mut text, &line);
        let mut ids = Vec::new();
        for line_iter in layout.layout.lines() {
            for run in line_iter.runs() {
                ids.push(font_id_from_data(run.font()));
            }
        }
        // Same shape → same ids on the second pass (stability).
        let layout2 = shape_line_with_default_family(&mut text, &line);
        let mut ids2 = Vec::new();
        for line_iter in layout2.layout.lines() {
            for run in line_iter.runs() {
                ids2.push(font_id_from_data(run.font()));
            }
        }
        assert_eq!(ids, ids2);
        // None of the ids should be zero.
        for id in &ids {
            assert_ne!(*id, 0, "font_id must not collapse to 0");
        }
    }

    #[test]
    fn var_hash_empty_is_zero() {
        assert_eq!(var_hash_from_coords(&[]), 0);
    }

    #[test]
    fn var_hash_consistent_for_same_input() {
        let coords = [100, 200, 300];
        assert_eq!(var_hash_from_coords(&coords), var_hash_from_coords(&coords));
    }

    #[test]
    fn var_hash_differs_for_different_inputs() {
        assert_ne!(
            var_hash_from_coords(&[100, 200]),
            var_hash_from_coords(&[100, 201])
        );
    }

    #[test]
    fn var_hash_never_zero_for_non_empty() {
        assert_ne!(var_hash_from_coords(&[0]), 0);
    }
}
