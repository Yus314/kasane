use super::super::test_helpers::test_state_80x24;
use super::super::*;
use crate::plugin::PluginRuntime;
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
    cache.buffer_commands = Some(vec![]);
    cache.overlay_commands = Some(vec![]);
    cache.cached_cell_size = Some((cs.width.to_bits(), cs.height.to_bits()));
    cache.cached_dims = Some((80, 24));

    cache.invalidate(DirtyFlags::BUFFER, cs, 80, 24);
    assert!(
        cache.buffer_commands.is_none(),
        "BUFFER should clear buffer_commands"
    );
    assert!(
        cache.overlay_commands.is_some(),
        "BUFFER should preserve overlays"
    );
}

#[test]
fn test_scene_cache_invalidate_menu_clears_overlays() {
    let cs = scene::CellSize {
        width: 10.0,
        height: 20.0,
    };
    let mut cache = SceneCache::new();
    cache.buffer_commands = Some(vec![]);
    cache.overlay_commands = Some(vec![]);
    cache.cached_cell_size = Some((cs.width.to_bits(), cs.height.to_bits()));
    cache.cached_dims = Some((80, 24));

    cache.invalidate(DirtyFlags::MENU_SELECTION, cs, 80, 24);
    assert!(
        cache.buffer_commands.is_some(),
        "MENU_SELECTION should preserve buffer"
    );
    assert!(
        cache.overlay_commands.is_none(),
        "MENU_SELECTION should clear overlays"
    );
}

#[test]
fn test_scene_cache_invalidate_info_clears_overlays() {
    let cs = scene::CellSize {
        width: 10.0,
        height: 20.0,
    };
    let mut cache = SceneCache::new();
    cache.buffer_commands = Some(vec![]);
    cache.overlay_commands = Some(vec![]);
    cache.cached_cell_size = Some((cs.width.to_bits(), cs.height.to_bits()));
    cache.cached_dims = Some((80, 24));

    cache.invalidate(DirtyFlags::INFO, cs, 80, 24);
    assert!(
        cache.buffer_commands.is_some(),
        "INFO should preserve buffer"
    );
    assert!(
        cache.overlay_commands.is_none(),
        "INFO should clear overlays"
    );
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
    cache.buffer_commands = Some(vec![]);
    cache.overlay_commands = Some(vec![]);
    cache.cached_cell_size = Some((cs1.width.to_bits(), cs1.height.to_bits()));
    cache.cached_dims = Some((80, 24));

    cache.invalidate(DirtyFlags::empty(), cs2, 80, 24);
    assert!(
        cache.buffer_commands.is_none(),
        "cell size change should clear buffer"
    );
    assert!(
        cache.overlay_commands.is_none(),
        "cell size change should clear overlays"
    );
}

#[test]
fn test_scene_cache_dims_change_clears_all() {
    let cs = scene::CellSize {
        width: 10.0,
        height: 20.0,
    };
    let mut cache = SceneCache::new();
    cache.buffer_commands = Some(vec![]);
    cache.overlay_commands = Some(vec![]);
    cache.cached_cell_size = Some((cs.width.to_bits(), cs.height.to_bits()));
    cache.cached_dims = Some((80, 24));

    cache.invalidate(DirtyFlags::empty(), cs, 100, 30);
    assert!(
        cache.buffer_commands.is_none(),
        "dims change should clear buffer"
    );
    assert!(
        cache.overlay_commands.is_none(),
        "dims change should clear overlays"
    );
}

#[test]
fn test_scene_cache_status_only_preserves_buffer() {
    let cs = scene::CellSize {
        width: 10.0,
        height: 20.0,
    };
    let mut cache = SceneCache::new();
    cache.buffer_commands = Some(vec![]);
    cache.status_commands = Some(vec![]);
    cache.overlay_commands = Some(vec![]);
    cache.cached_cell_size = Some((cs.width.to_bits(), cs.height.to_bits()));
    cache.cached_dims = Some((80, 24));

    cache.invalidate(DirtyFlags::STATUS, cs, 80, 24);
    assert!(
        cache.buffer_commands.is_some(),
        "STATUS should preserve buffer_commands"
    );
    assert!(
        cache.status_commands.is_none(),
        "STATUS should clear status_commands"
    );
    assert!(
        cache.overlay_commands.is_some(),
        "STATUS should preserve overlays"
    );
}

#[test]
fn test_scene_render_pipeline_deterministic() {
    use super::super::scene_render_pipeline;

    let mut state = test_state_80x24();
    state.observed.status_default_face = state.observed.default_face;
    state.observed.lines = vec![make_line("hello"), make_line("world")];
    state.inference.status_line = make_line("status");

    let registry = PluginRuntime::new();
    let cs = scene::CellSize {
        width: 10.0,
        height: 20.0,
    };

    let (first, _, _) = scene_render_pipeline(&state, &registry.view(), cs);
    let (second, _, _) = scene_render_pipeline(&state, &registry.view(), cs);

    assert_eq!(
        first, second,
        "scene_render_pipeline must produce deterministic output for same state"
    );
}

#[test]
fn test_scene_cache_overlay_ordering_with_menu_and_info() {
    use super::super::scene_render_pipeline;

    let mut state = test_state_80x24();
    state.observed.status_default_face = state.observed.default_face;
    state.observed.lines = vec![make_line("hello"), make_line("world")];
    state.inference.status_line = make_line("status");

    state.apply(crate::protocol::KakouneRequest::MenuShow {
        items: vec![make_line("item1"), make_line("item2")],
        anchor: Coord { line: 1, column: 0 },
        selected_item_face: Face::default(),
        menu_face: Face::default(),
        style: MenuStyle::Inline,
    });

    state.apply(crate::protocol::KakouneRequest::InfoShow {
        title: make_line("Info Title"),
        content: vec![make_line("info content")],
        anchor: Coord { line: 0, column: 0 },
        face: Face::default(),
        style: crate::protocol::InfoStyle::Prompt,
    });

    let mut registry = PluginRuntime::new();
    registry.register_backend(Box::new(crate::render::view::menu::BuiltinMenuPlugin));
    registry.register_backend(Box::new(crate::render::view::info::BuiltinInfoPlugin));
    let cs = scene::CellSize {
        width: 10.0,
        height: 20.0,
    };

    let (first, _, _) = scene_render_pipeline(&state, &registry.view(), cs);
    let (second, _, _) = scene_render_pipeline(&state, &registry.view(), cs);

    let overlay_count = first
        .iter()
        .filter(|c| matches!(c, DrawCommand::BeginOverlay))
        .count();

    assert!(
        overlay_count >= 2,
        "expected at least 2 overlays (menu + info), got {overlay_count}"
    );

    assert_eq!(
        first, second,
        "scene_render_pipeline must produce deterministic output with overlays"
    );
}
