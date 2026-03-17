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
fn test_scene_render_pipeline_deterministic() {
    use super::super::scene_render_pipeline;

    let mut state = test_state_80x24();
    state.status_default_face = state.default_face;
    state.lines = vec![make_line("hello"), make_line("world")];
    state.status_line = make_line("status");

    let registry = PluginRegistry::new();
    let cs = scene::CellSize {
        width: 10.0,
        height: 20.0,
    };

    // Two calls to scene_render_pipeline should produce identical output
    let (first, _) = scene_render_pipeline(&state, &registry, cs);
    let (second, _) = scene_render_pipeline(&state, &registry, cs);

    assert_eq!(
        first, second,
        "scene_render_pipeline must produce deterministic output for same state"
    );
}

#[test]
fn test_scene_cache_overlay_ordering_with_menu_and_info() {
    use super::super::scene_render_pipeline;

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

    // Verify scene_render_pipeline produces deterministic overlay output
    let (first, _) = scene_render_pipeline(&state, &registry, cs);
    let (second, _) = scene_render_pipeline(&state, &registry, cs);

    let overlay_count = first
        .iter()
        .filter(|c| matches!(c, DrawCommand::BeginOverlay))
        .count();

    // Menu + info = at least 2 overlays
    assert!(
        overlay_count >= 2,
        "expected at least 2 overlays (menu + info), got {overlay_count}"
    );

    assert_eq!(
        first, second,
        "scene_render_pipeline must produce deterministic output with overlays"
    );
}
