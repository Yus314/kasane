use super::super::test_helpers::test_state_80x24;
use super::super::*;
use crate::plugin::PluginRegistry;
use crate::protocol::{Coord, Face, MenuStyle};
use crate::state::DirtyFlags;
use crate::test_utils::make_line;

#[test]
fn test_scene_cache_invalidate_buffer_clears_base_only() {
    let cs = scene::CellSize {
        width: 10.0,
        height: 20.0,
    };
    let mut cache = SceneCache::new();
    cache.base_commands = Some(vec![]);
    cache.menu_commands = Some(vec![]);
    cache.info_commands = Some(vec![]);
    cache.cached_cell_size = Some((cs.width.to_bits(), cs.height.to_bits()));
    cache.cached_dims = Some((80, 24));

    cache.invalidate(DirtyFlags::BUFFER, cs, 80, 24);
    assert!(cache.base_commands.is_none(), "BUFFER should clear base");
    assert!(cache.menu_commands.is_some(), "BUFFER should preserve menu");
    assert!(cache.info_commands.is_some(), "BUFFER should preserve info");
}

#[test]
fn test_scene_cache_invalidate_menu_clears_menu_only() {
    let cs = scene::CellSize {
        width: 10.0,
        height: 20.0,
    };
    let mut cache = SceneCache::new();
    cache.base_commands = Some(vec![]);
    cache.menu_commands = Some(vec![]);
    cache.info_commands = Some(vec![]);
    cache.cached_cell_size = Some((cs.width.to_bits(), cs.height.to_bits()));
    cache.cached_dims = Some((80, 24));

    cache.invalidate(DirtyFlags::MENU_SELECTION, cs, 80, 24);
    assert!(
        cache.base_commands.is_some(),
        "MENU_SELECTION should preserve base"
    );
    assert!(
        cache.menu_commands.is_none(),
        "MENU_SELECTION should clear menu"
    );
    assert!(
        cache.info_commands.is_some(),
        "MENU_SELECTION should preserve info"
    );
}

#[test]
fn test_scene_cache_invalidate_info_clears_info_only() {
    let cs = scene::CellSize {
        width: 10.0,
        height: 20.0,
    };
    let mut cache = SceneCache::new();
    cache.base_commands = Some(vec![]);
    cache.menu_commands = Some(vec![]);
    cache.info_commands = Some(vec![]);
    cache.cached_cell_size = Some((cs.width.to_bits(), cs.height.to_bits()));
    cache.cached_dims = Some((80, 24));

    cache.invalidate(DirtyFlags::INFO, cs, 80, 24);
    assert!(cache.base_commands.is_some(), "INFO should preserve base");
    assert!(cache.menu_commands.is_some(), "INFO should preserve menu");
    assert!(cache.info_commands.is_none(), "INFO should clear info");
}

#[test]
fn test_scene_cache_cell_size_change_clears_all() {
    let cs1 = scene::CellSize {
        width: 10.0,
        height: 20.0,
    };
    let cs2 = scene::CellSize {
        width: 12.0,
        height: 24.0,
    };
    let mut cache = SceneCache::new();
    cache.base_commands = Some(vec![]);
    cache.menu_commands = Some(vec![]);
    cache.info_commands = Some(vec![]);
    cache.cached_cell_size = Some((cs1.width.to_bits(), cs1.height.to_bits()));
    cache.cached_dims = Some((80, 24));

    // Even with empty dirty flags, a cell size change should clear everything
    cache.invalidate(DirtyFlags::empty(), cs2, 80, 24);
    assert!(
        cache.base_commands.is_none(),
        "cell size change should clear base"
    );
    assert!(
        cache.menu_commands.is_none(),
        "cell size change should clear menu"
    );
    assert!(
        cache.info_commands.is_none(),
        "cell size change should clear info"
    );
}

#[test]
fn test_scene_cache_dims_change_clears_all() {
    let cs = scene::CellSize {
        width: 10.0,
        height: 20.0,
    };
    let mut cache = SceneCache::new();
    cache.base_commands = Some(vec![]);
    cache.menu_commands = Some(vec![]);
    cache.info_commands = Some(vec![]);
    cache.cached_cell_size = Some((cs.width.to_bits(), cs.height.to_bits()));
    cache.cached_dims = Some((80, 24));

    cache.invalidate(DirtyFlags::empty(), cs, 100, 30);
    assert!(
        cache.base_commands.is_none(),
        "dims change should clear base"
    );
    assert!(
        cache.menu_commands.is_none(),
        "dims change should clear menu"
    );
    assert!(
        cache.info_commands.is_none(),
        "dims change should clear info"
    );
}

#[test]
fn test_scene_cache_output_matches_uncached() {
    use super::super::scene_render_pipeline;
    use super::super::scene_render_pipeline_scene_cached;

    let mut state = test_state_80x24();
    state.status_default_face = state.default_face;
    state.lines = vec![make_line("hello"), make_line("world")];
    state.status_line = make_line("status");

    let registry = PluginRegistry::new();
    let cs = scene::CellSize {
        width: 10.0,
        height: 20.0,
    };

    // Uncached (reference)
    let (expected, _) = scene_render_pipeline(&state, &registry, cs);

    // Cached (cold — DirtyFlags::ALL)
    let mut view_cache = ViewCache::new();
    let mut scene_cache = SceneCache::new();
    let (actual, _) = scene_render_pipeline_scene_cached(
        &state,
        &registry,
        cs,
        DirtyFlags::ALL,
        &mut view_cache,
        &mut scene_cache,
    );

    assert_eq!(
        expected,
        actual.to_vec(),
        "scene_cached output must match uncached for same state"
    );
}

#[test]
fn test_scene_cache_warm_matches_cold() {
    use super::super::scene_render_pipeline_scene_cached;

    let mut state = test_state_80x24();
    state.status_default_face = state.default_face;
    state.lines = vec![make_line("hello"), make_line("world")];
    state.status_line = make_line("status");

    let registry = PluginRegistry::new();
    let cs = scene::CellSize {
        width: 10.0,
        height: 20.0,
    };

    // Cold render
    let mut view_cache = ViewCache::new();
    let mut scene_cache = SceneCache::new();
    let (cold, _) = scene_render_pipeline_scene_cached(
        &state,
        &registry,
        cs,
        DirtyFlags::ALL,
        &mut view_cache,
        &mut scene_cache,
    );
    let cold = cold.to_vec();

    // Warm render (empty dirty)
    let (warm, _) = scene_render_pipeline_scene_cached(
        &state,
        &registry,
        cs,
        DirtyFlags::empty(),
        &mut view_cache,
        &mut scene_cache,
    );

    assert_eq!(
        cold,
        warm.to_vec(),
        "warm cache must produce identical commands to cold cache"
    );
}

#[test]
fn test_scene_cache_menu_select_preserves_base() {
    use super::super::scene_render_pipeline_scene_cached;

    let mut state = test_state_80x24();
    state.status_default_face = state.default_face;
    state.lines = vec![make_line("hello")];
    state.status_line = make_line("status");

    // Show menu
    state.apply(crate::protocol::KakouneRequest::MenuShow {
        items: vec![make_line("item1"), make_line("item2"), make_line("item3")],
        anchor: Coord { line: 1, column: 0 },
        selected_item_face: Face::default(),
        menu_face: Face::default(),
        style: MenuStyle::Inline,
    });

    let registry = PluginRegistry::new();
    let cs = scene::CellSize {
        width: 10.0,
        height: 20.0,
    };

    // Initial render
    let mut view_cache = ViewCache::new();
    let mut scene_cache = SceneCache::new();
    scene_render_pipeline_scene_cached(
        &state,
        &registry,
        cs,
        DirtyFlags::ALL,
        &mut view_cache,
        &mut scene_cache,
    );

    // Verify base is cached
    assert!(
        scene_cache.base_commands.is_some(),
        "base should be cached after initial render"
    );

    // Select item
    state.apply(crate::protocol::KakouneRequest::MenuSelect { selected: 1 });

    // Render with MENU_SELECTION only
    scene_render_pipeline_scene_cached(
        &state,
        &registry,
        cs,
        DirtyFlags::MENU_SELECTION,
        &mut view_cache,
        &mut scene_cache,
    );

    assert!(
        scene_cache.base_commands.is_some(),
        "base should remain cached on MENU_SELECTION"
    );
}

#[test]
fn test_scene_cache_overlay_ordering_with_menu_and_info() {
    use super::super::scene_render_pipeline;
    use super::super::scene_render_pipeline_scene_cached;

    let mut state = test_state_80x24();
    state.status_default_face = state.default_face;
    state.lines = vec![make_line("hello"), make_line("world")];
    state.status_line = make_line("status");

    // Show menu
    state.apply(crate::protocol::KakouneRequest::MenuShow {
        items: vec![make_line("item1"), make_line("item2")],
        anchor: Coord { line: 1, column: 0 },
        selected_item_face: Face::default(),
        menu_face: Face::default(),
        style: MenuStyle::Inline,
    });

    // Show info
    state.apply(crate::protocol::KakouneRequest::InfoShow {
        title: make_line("Info Title"),
        content: vec![make_line("info content")],
        anchor: Coord { line: 0, column: 0 },
        face: Face::default(),
        style: crate::protocol::InfoStyle::Prompt,
    });

    let registry = PluginRegistry::new();
    let cs = scene::CellSize {
        width: 10.0,
        height: 20.0,
    };

    // Uncached
    let (uncached, _) = scene_render_pipeline(&state, &registry, cs);
    let uncached_overlay_count = uncached
        .iter()
        .filter(|c| matches!(c, DrawCommand::BeginOverlay))
        .count();

    // Cached
    let mut view_cache = ViewCache::new();
    let mut scene_cache = SceneCache::new();
    let (cached, _) = scene_render_pipeline_scene_cached(
        &state,
        &registry,
        cs,
        DirtyFlags::ALL,
        &mut view_cache,
        &mut scene_cache,
    );
    let cached_overlay_count = cached
        .iter()
        .filter(|c| matches!(c, DrawCommand::BeginOverlay))
        .count();

    // Both should have the same number of BeginOverlay markers
    assert_eq!(
        uncached_overlay_count, cached_overlay_count,
        "BeginOverlay count must match: uncached={uncached_overlay_count}, cached={cached_overlay_count}"
    );
    // Menu + info = at least 2 overlays
    assert!(
        cached_overlay_count >= 2,
        "expected at least 2 overlays (menu + info), got {cached_overlay_count}"
    );
}
