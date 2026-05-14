use super::super::test_helpers::test_state_80x24;
use super::super::*;
use crate::plugin::PluginRuntime;
use crate::protocol::{Coord, MenuStyle};
use crate::salsa_db::KasaneDatabase;
use crate::salsa_sync::{
    SalsaInputHandles, sync_display_directives, sync_inputs_from_state, sync_plugin_contributions,
};
use crate::state::DirtyFlags;
use crate::test_utils::make_line;

fn render_scene_full(
    state: &crate::state::AppState,
    registry: &PluginRuntime,
    cs: scene::CellSize,
) -> Vec<DrawCommand> {
    let mut db = KasaneDatabase::default();
    let mut handles = SalsaInputHandles::new(&mut db);
    sync_inputs_from_state(&mut db, state, &handles);
    sync_display_directives(&mut db, state, &registry.view(), &handles);
    sync_plugin_contributions(state, &registry.view(), &mut handles, DirtyFlags::ALL);
    let mut cache = SceneCache::new();
    let (cmds, _, _) = scene_render_pipeline_cached(
        &db,
        &handles,
        state,
        &registry.view(),
        cs,
        DirtyFlags::ALL,
        &mut cache,
        SceneRenderOptions::default(),
    );
    cmds.to_vec()
}

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
    let mut state = test_state_80x24();
    state.observed.status_default_style = state.observed.default_style.clone();
    state.observed.lines = vec![make_line("hello"), make_line("world")].into();
    state.inference.status_line = make_line("status");

    let registry = PluginRuntime::new();
    let cs = scene::CellSize {
        width: 10.0,
        height: 20.0,
    };

    let first = render_scene_full(&state, &registry, cs);
    let second = render_scene_full(&state, &registry, cs);

    assert_eq!(
        first, second,
        "scene_render_pipeline_cached must produce deterministic output for same state"
    );
}

#[test]
fn test_scene_cache_overlay_ordering_with_menu_and_info() {
    let mut state = test_state_80x24();
    state.observed.status_default_style = state.observed.default_style.clone();
    state.observed.lines = vec![make_line("hello"), make_line("world")].into();
    state.inference.status_line = make_line("status");

    state.apply(crate::protocol::KakouneRequest::MenuShow {
        items: vec![make_line("item1"), make_line("item2")],
        anchor: Coord { line: 1, column: 0 },
        selected_item_style: crate::protocol::default_unresolved_style(),
        menu_style: crate::protocol::default_unresolved_style(),
        style: MenuStyle::Inline,
    });

    state.apply(crate::protocol::KakouneRequest::InfoShow {
        title: make_line("Info Title"),
        content: vec![make_line("info content")],
        anchor: Coord { line: 0, column: 0 },
        info_style: crate::protocol::default_unresolved_style(),
        style: crate::protocol::InfoStyle::Prompt,
    });

    let mut registry = PluginRuntime::new();
    registry.register(crate::render::view::menu::BuiltinMenuPlugin);
    registry.register(crate::render::view::info::BuiltinInfoPlugin);
    let cs = scene::CellSize {
        width: 10.0,
        height: 20.0,
    };

    let first = render_scene_full(&state, &registry, cs);
    let second = render_scene_full(&state, &registry, cs);

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
        "scene_render_pipeline_cached must produce deterministic output with overlays"
    );
}
