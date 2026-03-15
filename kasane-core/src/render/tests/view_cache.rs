use super::super::test_helpers::test_state_80x24;
use super::super::*;
use crate::plugin::PluginRegistry;
use crate::state::DirtyFlags;
use crate::test_utils::make_line;

fn empty_base_result() -> crate::surface::SurfaceComposeResult {
    crate::surface::SurfaceComposeResult {
        base: Some(crate::element::Element::Empty),
        surface_reports: vec![],
    }
}

#[test]
fn test_view_cache_invalidate_buffer_clears_base() {
    let mut cache = ViewCache::new();
    cache.base.value = Some(empty_base_result());
    cache.menu_overlay.value = Some(None);
    cache.info_overlays.value = Some(vec![]);

    cache.invalidate(DirtyFlags::BUFFER);
    assert!(cache.base.value.is_none(), "BUFFER should clear base");
    assert!(
        cache.menu_overlay.value.is_some(),
        "BUFFER should preserve menu"
    );
    assert!(
        cache.info_overlays.value.is_some(),
        "BUFFER should preserve info"
    );
}

#[test]
fn test_view_cache_invalidate_menu_selection_clears_menu() {
    let mut cache = ViewCache::new();
    cache.base.value = Some(empty_base_result());
    cache.menu_overlay.value = Some(None);
    cache.info_overlays.value = Some(vec![]);

    cache.invalidate(DirtyFlags::MENU_SELECTION);
    assert!(
        cache.base.value.is_some(),
        "MENU_SELECTION should preserve base"
    );
    assert!(
        cache.menu_overlay.value.is_none(),
        "MENU_SELECTION should clear menu"
    );
    assert!(
        cache.info_overlays.value.is_some(),
        "MENU_SELECTION should preserve info"
    );
}

#[test]
fn test_view_cache_invalidate_all_clears_everything() {
    let mut cache = ViewCache::new();
    cache.base.value = Some(empty_base_result());
    cache.menu_overlay.value = Some(None);
    cache.info_overlays.value = Some(vec![]);

    cache.invalidate(DirtyFlags::ALL);
    assert!(cache.base.value.is_none());
    assert!(cache.menu_overlay.value.is_none());
    assert!(cache.info_overlays.value.is_none());
}

#[test]
fn test_view_cache_invalidate_info_clears_info() {
    let mut cache = ViewCache::new();
    cache.base.value = Some(empty_base_result());
    cache.menu_overlay.value = Some(None);
    cache.info_overlays.value = Some(vec![]);

    cache.invalidate(DirtyFlags::INFO);
    assert!(cache.base.value.is_some(), "INFO should preserve base");
    assert!(
        cache.menu_overlay.value.is_some(),
        "INFO should preserve menu"
    );
    assert!(
        cache.info_overlays.value.is_none(),
        "INFO should clear info"
    );
}

#[test]
fn test_view_cache_invalidate_status_clears_base() {
    let mut cache = ViewCache::new();
    cache.base.value = Some(empty_base_result());
    cache.menu_overlay.value = Some(None);

    cache.invalidate(DirtyFlags::STATUS);
    assert!(cache.base.value.is_none(), "STATUS should clear base");
    assert!(
        cache.menu_overlay.value.is_some(),
        "STATUS should preserve menu"
    );
}

#[test]
fn test_view_cache_invalidate_options_clears_base() {
    let mut cache = ViewCache::new();
    cache.base.value = Some(empty_base_result());

    cache.invalidate(DirtyFlags::OPTIONS);
    assert!(cache.base.value.is_none(), "OPTIONS should clear base");
}

/// Cached view output must match fresh construction.
#[test]
fn test_view_cached_matches_fresh() {
    let mut state = test_state_80x24();
    state.status_default_face = state.default_face;
    state.lines = vec![make_line("hello"), make_line("world")];
    state.status_line = make_line("status");

    let registry = PluginRegistry::new();

    // Fresh render
    let mut grid_fresh = CellGrid::new(state.cols, state.rows);
    render_pipeline(&state, &registry, &mut grid_fresh);

    // Cached render (ALL dirty — cold cache)
    let mut grid_cached = CellGrid::new(state.cols, state.rows);
    let mut cache = ViewCache::new();
    render_pipeline_cached(
        &state,
        &registry,
        &mut grid_cached,
        DirtyFlags::ALL,
        &mut cache,
    );

    for y in 0..state.rows {
        for x in 0..state.cols {
            let fresh = grid_fresh.get(x, y).unwrap();
            let cached = grid_cached.get(x, y).unwrap();
            assert_eq!(
                fresh.grapheme, cached.grapheme,
                "grapheme mismatch at ({x}, {y})"
            );
            assert_eq!(fresh.face, cached.face, "face mismatch at ({x}, {y})");
        }
    }
}

/// MenuSelect-only dirty should keep base cached and rebuild menu.
#[test]
fn test_view_cache_menu_select_reuses_base() {
    let mut state = test_state_80x24();
    state.status_default_face = state.default_face;
    state.lines = vec![make_line("hello")];
    state.status_line = make_line("status");

    // Show menu
    state.apply(crate::protocol::KakouneRequest::MenuShow {
        items: vec![make_line("item1"), make_line("item2"), make_line("item3")],
        anchor: crate::protocol::Coord { line: 1, column: 0 },
        selected_item_face: crate::protocol::Face::default(),
        menu_face: crate::protocol::Face::default(),
        style: crate::protocol::MenuStyle::Inline,
    });

    let registry = PluginRegistry::new();
    let mut cache = ViewCache::new();

    // Initial render (ALL dirty)
    let mut grid = CellGrid::new(state.cols, state.rows);
    render_pipeline_cached(&state, &registry, &mut grid, DirtyFlags::ALL, &mut cache);
    assert!(
        cache.base.value.is_some(),
        "base should be cached after render"
    );

    // Select item
    state.apply(crate::protocol::KakouneRequest::MenuSelect { selected: 1 });

    // Render with MENU_SELECTION only — base should stay cached
    render_pipeline_cached(
        &state,
        &registry,
        &mut grid,
        DirtyFlags::MENU_SELECTION,
        &mut cache,
    );
    assert!(
        cache.base.value.is_some(),
        "base should remain cached on MENU_SELECTION"
    );
}
