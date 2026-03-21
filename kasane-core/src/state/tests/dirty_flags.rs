use std::collections::{HashMap, HashSet};

use crate::protocol::{Coord, Face, KakouneRequest, MenuStyle};
use crate::scroll::{SMOOTH_SCROLL_CONFIG_KEY, set_smooth_scroll_enabled, smooth_scroll_enabled};
use crate::state::{AppState, DirtyFlags};
use crate::test_utils::make_line;

// --- DirtyFlags split MENU tests ---

#[test]
fn test_menu_show_returns_menu_structure() {
    let mut state = AppState::default();
    let flags = state.apply(KakouneRequest::MenuShow {
        items: vec![make_line("a")],
        anchor: Coord { line: 0, column: 0 },
        selected_item_face: Face::default(),
        menu_face: Face::default(),
        style: MenuStyle::Inline,
    });
    assert!(flags.contains(DirtyFlags::MENU_STRUCTURE));
    assert!(!flags.contains(DirtyFlags::MENU_SELECTION));
}

#[test]
fn test_menu_select_returns_menu_selection() {
    let mut state = AppState::default();
    state.apply(KakouneRequest::MenuShow {
        items: vec![make_line("a"), make_line("b")],
        anchor: Coord { line: 0, column: 0 },
        selected_item_face: Face::default(),
        menu_face: Face::default(),
        style: MenuStyle::Inline,
    });
    let flags = state.apply(KakouneRequest::MenuSelect { selected: 0 });
    assert!(flags.contains(DirtyFlags::MENU_SELECTION));
    assert!(!flags.contains(DirtyFlags::MENU_STRUCTURE));
}

#[test]
fn test_menu_hide_returns_both_menu_flags() {
    let mut state = AppState::default();
    state.apply(KakouneRequest::MenuShow {
        items: vec![make_line("a")],
        anchor: Coord { line: 0, column: 0 },
        selected_item_face: Face::default(),
        menu_face: Face::default(),
        style: MenuStyle::Inline,
    });
    let flags = state.apply(KakouneRequest::MenuHide);
    assert!(flags.contains(DirtyFlags::MENU_STRUCTURE));
    assert!(flags.contains(DirtyFlags::MENU_SELECTION));
    assert!(flags.contains(DirtyFlags::BUFFER_CONTENT));
}

#[test]
fn test_apply_set_config_smooth_scroll_alias_updates_plugin_config() {
    let mut state = AppState::default();
    let mut dirty = DirtyFlags::empty();

    crate::state::apply_set_config(&mut state, &mut dirty, "smooth_scroll", "true");

    assert!(dirty.contains(DirtyFlags::OPTIONS));
    assert!(smooth_scroll_enabled(&state));
    assert_eq!(
        state.plugin_config.get(SMOOTH_SCROLL_CONFIG_KEY),
        Some(&"true".to_string())
    );
    assert!(!state.plugin_config.contains_key("smooth_scroll"));
}

#[test]
fn test_apply_set_config_canonical_smooth_scroll_updates_plugin_config() {
    let mut state = AppState::default();
    let mut dirty = DirtyFlags::empty();

    crate::state::apply_set_config(&mut state, &mut dirty, SMOOTH_SCROLL_CONFIG_KEY, "true");
    crate::state::apply_set_config(&mut state, &mut dirty, SMOOTH_SCROLL_CONFIG_KEY, "false");

    assert!(dirty.contains(DirtyFlags::OPTIONS));
    assert!(!smooth_scroll_enabled(&state));
    assert_eq!(
        state.plugin_config.get(SMOOTH_SCROLL_CONFIG_KEY),
        Some(&"false".to_string())
    );
}

#[test]
fn test_menu_composite_contains_sub_flags() {
    assert!(DirtyFlags::MENU.contains(DirtyFlags::MENU_STRUCTURE));
    assert!(DirtyFlags::MENU.contains(DirtyFlags::MENU_SELECTION));
    assert!(DirtyFlags::ALL.contains(DirtyFlags::MENU_STRUCTURE));
    assert!(DirtyFlags::ALL.contains(DirtyFlags::MENU_SELECTION));
}

#[test]
fn test_available_height() {
    let mut state = AppState::default();
    state.rows = 24;
    assert_eq!(state.available_height(), 23);

    state.rows = 1;
    assert_eq!(state.available_height(), 0);
}

// --- Line-level dirty tracking tests ---

#[test]
fn test_apply_draw_lines_dirty_single_change() {
    let mut state = AppState::default();
    state.apply(KakouneRequest::Draw {
        lines: vec![make_line("aaa"), make_line("bbb"), make_line("ccc")],
        cursor_pos: Coord::default(),
        default_face: Face::default(),
        padding_face: Face::default(),
        widget_columns: 0,
    });

    // Change only middle line
    state.apply(KakouneRequest::Draw {
        lines: vec![make_line("aaa"), make_line("BBB"), make_line("ccc")],
        cursor_pos: Coord::default(),
        default_face: Face::default(),
        padding_face: Face::default(),
        widget_columns: 0,
    });
    assert_eq!(state.lines_dirty, vec![false, true, false]);
}

#[test]
fn test_apply_draw_lines_dirty_face_change() {
    let mut state = AppState::default();
    state.apply(KakouneRequest::Draw {
        lines: vec![make_line("aaa"), make_line("bbb")],
        cursor_pos: Coord::default(),
        default_face: Face::default(),
        padding_face: Face::default(),
        widget_columns: 0,
    });

    // Same lines but different default_face → all dirty
    let new_face = Face {
        fg: crate::protocol::Color::Named(crate::protocol::NamedColor::Red),
        ..Face::default()
    };
    state.apply(KakouneRequest::Draw {
        lines: vec![make_line("aaa"), make_line("bbb")],
        cursor_pos: Coord::default(),
        default_face: new_face,
        padding_face: Face::default(),
        widget_columns: 0,
    });
    assert_eq!(state.lines_dirty, vec![true, true]);
}

#[test]
fn test_apply_draw_lines_dirty_length_change() {
    let mut state = AppState::default();
    state.apply(KakouneRequest::Draw {
        lines: vec![make_line("aaa"), make_line("bbb")],
        cursor_pos: Coord::default(),
        default_face: Face::default(),
        padding_face: Face::default(),
        widget_columns: 0,
    });

    // Different number of lines → all dirty
    state.apply(KakouneRequest::Draw {
        lines: vec![make_line("aaa"), make_line("bbb"), make_line("ccc")],
        cursor_pos: Coord::default(),
        default_face: Face::default(),
        padding_face: Face::default(),
        widget_columns: 0,
    });
    assert_eq!(state.lines_dirty, vec![true, true, true]);
}

#[test]
fn test_apply_draw_lines_dirty_no_change() {
    let mut state = AppState::default();
    state.apply(KakouneRequest::Draw {
        lines: vec![make_line("aaa"), make_line("bbb")],
        cursor_pos: Coord::default(),
        default_face: Face::default(),
        padding_face: Face::default(),
        widget_columns: 0,
    });

    // Identical draw → all clean
    state.apply(KakouneRequest::Draw {
        lines: vec![make_line("aaa"), make_line("bbb")],
        cursor_pos: Coord::default(),
        default_face: Face::default(),
        padding_face: Face::default(),
        widget_columns: 0,
    });
    assert_eq!(state.lines_dirty, vec![false, false]);
}

#[test]
fn test_apply_draw_lines_dirty_first_draw() {
    let mut state = AppState::default();
    // First draw (no prior lines) → all dirty
    state.apply(KakouneRequest::Draw {
        lines: vec![make_line("aaa"), make_line("bbb")],
        cursor_pos: Coord::default(),
        default_face: Face::default(),
        padding_face: Face::default(),
        widget_columns: 0,
    });
    assert_eq!(state.lines_dirty, vec![true, true]);
}

#[test]
fn test_menu_select_no_scroll_returns_selection_only() {
    let mut state = AppState::default();
    state.rows = 24;
    state.cols = 80;
    // 3 items fit in win_height without scrolling
    state.apply(KakouneRequest::MenuShow {
        items: vec![make_line("a"), make_line("b"), make_line("c")],
        anchor: Coord { line: 0, column: 0 },
        selected_item_face: Face::default(),
        menu_face: Face::default(),
        style: MenuStyle::Inline,
    });
    state.apply(KakouneRequest::MenuSelect { selected: 0 });

    // Moving selection within the same visible window → no scroll
    let flags = state.apply(KakouneRequest::MenuSelect { selected: 1 });
    assert!(flags.contains(DirtyFlags::MENU_SELECTION));
    assert!(!flags.contains(DirtyFlags::MENU_STRUCTURE));
}

#[test]
fn test_menu_select_with_scroll_returns_structure() {
    let mut state = AppState::default();
    state.rows = 24;
    state.cols = 80;
    // Many items: win_height will be limited, so scrolling past visible range triggers first_item change
    let items: Vec<_> = (0..30).map(|i| make_line(&format!("item{i}"))).collect();
    state.apply(KakouneRequest::MenuShow {
        items,
        anchor: Coord { line: 0, column: 0 },
        selected_item_face: Face::default(),
        menu_face: Face::default(),
        style: MenuStyle::Inline,
    });
    state.apply(KakouneRequest::MenuSelect { selected: 0 });
    let first_before = state.menu.as_ref().unwrap().first_item;

    // Select an item far enough to force scroll (beyond win_height * columns)
    let flags = state.apply(KakouneRequest::MenuSelect { selected: 25 });
    let first_after = state.menu.as_ref().unwrap().first_item;

    // first_item must have changed → MENU_STRUCTURE should be set
    assert_ne!(first_before, first_after, "scroll should have occurred");
    assert!(flags.contains(DirtyFlags::MENU_SELECTION));
    assert!(flags.contains(DirtyFlags::MENU_STRUCTURE));
}

#[test]
fn test_session_flag_value() {
    assert_eq!(DirtyFlags::SESSION.bits(), 0x100);
}

#[test]
fn test_all_contains_session() {
    assert!(DirtyFlags::ALL.contains(DirtyFlags::SESSION));
}

#[test]
fn test_session_fields_preserved_on_reset() {
    use crate::session::SessionDescriptor;

    let mut state = AppState::default();
    state.session_descriptors = vec![SessionDescriptor {
        key: "work".into(),
        session_name: Some("project".into()),
        buffer_name: None,
        mode_line: None,
    }];
    state.active_session_key = Some("work".into());
    state.lines = vec![vec![]]; // session-owned data

    state.reset_for_session_switch();

    // Session fields preserved
    assert_eq!(state.session_descriptors.len(), 1);
    assert_eq!(state.session_descriptors[0].key, "work");
    assert_eq!(state.active_session_key.as_deref(), Some("work"));
    // Session-owned data reset
    assert!(state.lines.is_empty());
}

#[test]
fn test_reset_preserves_all_config_and_runtime_fields() {
    use crate::config::MenuPosition;
    use crate::session::SessionDescriptor;

    let mut state = AppState::default();

    // Set all preserved fields to non-default values
    state.cols = 200;
    state.rows = 50;
    state.focused = false;
    state.shadow_enabled = false;
    state.padding_char = "x".into();
    state.menu_max_height = 20;
    state.menu_position = MenuPosition::Below;
    state.search_dropdown = true;
    state.status_at_top = true;
    state.scrollbar_thumb = "T".into();
    state.scrollbar_track = "t".into();
    state.assistant_art = Some(vec!["art".into()]);
    state.plugin_config.insert("key".into(), "value".into());
    state.secondary_blend_ratio = 0.8;
    set_smooth_scroll_enabled(&mut state.plugin_config, true);
    state.session_descriptors = vec![SessionDescriptor {
        key: "work".into(),
        session_name: Some("proj".into()),
        buffer_name: None,
        mode_line: None,
    }];
    state.active_session_key = Some("work".into());

    // Set some protocol fields to non-default values
    state.lines = vec![vec![]];
    state.cursor_count = 3;
    state.cursor_pos = Coord {
        line: 5,
        column: 10,
    };

    state.reset_for_session_switch();

    // All preserved fields must retain their non-default values
    assert_eq!(state.cols, 200);
    assert_eq!(state.rows, 50);
    assert!(!state.focused);
    assert!(!state.shadow_enabled);
    assert_eq!(state.padding_char, "x");
    assert_eq!(state.menu_max_height, 20);
    assert_eq!(state.menu_position, MenuPosition::Below);
    assert!(state.search_dropdown);
    assert!(state.status_at_top);
    assert_eq!(state.scrollbar_thumb, "T");
    assert_eq!(state.scrollbar_track, "t");
    assert_eq!(state.assistant_art.as_ref().unwrap()[0], "art");
    assert_eq!(state.plugin_config.get("key").unwrap(), "value");
    assert_eq!(state.secondary_blend_ratio, 0.8);
    assert!(smooth_scroll_enabled(&state));
    assert_eq!(
        state.plugin_config.get(SMOOTH_SCROLL_CONFIG_KEY),
        Some(&"true".to_string())
    );
    assert_eq!(state.session_descriptors.len(), 1);
    assert_eq!(state.active_session_key.as_deref(), Some("work"));

    // All protocol/ephemeral fields must be reset to defaults
    assert!(state.lines.is_empty());
    assert_eq!(state.cursor_count, 0);
    assert_eq!(state.cursor_pos, Coord::default());
    assert_eq!(state.default_face, Face::default());
    assert!(state.menu.is_none());
    assert!(state.infos.is_empty());
    assert!(state.ui_options.is_empty());
    assert_eq!(state.drag, crate::state::DragState::None);
}

// --- DirtyTracked derive consistency tests ---

#[test]
fn test_field_dirty_map_matches_macro_analysis() {
    // Build a HashMap from the derive-generated FIELD_DIRTY_MAP
    let derive_map: HashMap<&str, HashSet<&str>> = AppState::FIELD_DIRTY_MAP
        .iter()
        .map(|(field, flags)| (*field, flags.iter().copied().collect::<HashSet<_>>()))
        .collect();

    // Build a HashMap from the proc-macro's FIELD_FLAG_MAP
    // (imported from kasane_macros::analysis via the FIELD_FLAG_MAP constant embedded in analysis.rs)
    // Since we can't directly reference the proc macro's internal constant, we duplicate it here
    // and the test ensures both stay in sync.
    let macro_map: &[(&str, &[&str])] = &[
        ("lines", &["BUFFER_CONTENT"]),
        ("lines_dirty", &["BUFFER_CONTENT"]),
        ("default_face", &["BUFFER_CONTENT"]),
        ("padding_face", &["BUFFER_CONTENT"]),
        ("widget_columns", &["BUFFER_CONTENT"]),
        ("cursor_mode", &["BUFFER_CURSOR"]),
        ("cursor_pos", &["BUFFER_CURSOR"]),
        ("cursor_count", &["BUFFER_CURSOR"]),
        ("secondary_cursors", &["BUFFER_CURSOR"]),
        ("status_prompt", &["STATUS"]),
        ("status_content", &["STATUS"]),
        ("status_content_cursor_pos", &["STATUS"]),
        ("status_line", &["STATUS"]),
        ("status_mode_line", &["STATUS"]),
        ("status_default_face", &["STATUS"]),
        ("menu", &["MENU_STRUCTURE", "MENU_SELECTION"]),
        ("infos", &["INFO"]),
        ("ui_options", &["OPTIONS"]),
        ("shadow_enabled", &["OPTIONS"]),
        ("padding_char", &["OPTIONS"]),
        ("menu_max_height", &["OPTIONS"]),
        ("menu_position", &["OPTIONS"]),
        ("search_dropdown", &["OPTIONS"]),
        ("status_at_top", &["OPTIONS"]),
        ("scrollbar_thumb", &["MENU_STRUCTURE"]),
        ("scrollbar_track", &["MENU_STRUCTURE"]),
        ("assistant_art", &["OPTIONS"]),
        ("plugin_config", &["OPTIONS"]),
        ("secondary_blend_ratio", &["BUFFER_CONTENT"]),
        ("session_descriptors", &["SESSION"]),
        ("active_session_key", &["SESSION"]),
    ];

    let macro_hashmap: HashMap<&str, HashSet<&str>> = macro_map
        .iter()
        .map(|(field, flags)| (*field, flags.iter().copied().collect::<HashSet<_>>()))
        .collect();

    // Both maps should have identical entries
    assert_eq!(
        derive_map.len(),
        macro_hashmap.len(),
        "field count mismatch: derive={}, macro={}. \
         derive_only={:?}, macro_only={:?}",
        derive_map.len(),
        macro_hashmap.len(),
        derive_map
            .keys()
            .filter(|k| !macro_hashmap.contains_key(*k))
            .collect::<Vec<_>>(),
        macro_hashmap
            .keys()
            .filter(|k| !derive_map.contains_key(*k))
            .collect::<Vec<_>>(),
    );

    for (field, macro_flags) in &macro_hashmap {
        let derive_flags = derive_map.get(field).unwrap_or_else(|| {
            panic!("field `{field}` in macro FIELD_FLAG_MAP but not in derive FIELD_DIRTY_MAP")
        });
        assert_eq!(
            derive_flags, macro_flags,
            "flag mismatch for field `{field}`: derive={derive_flags:?}, macro={macro_flags:?}"
        );
    }
}

#[test]
fn test_free_read_fields_match() {
    let expected_free: HashSet<&str> = ["cols", "rows", "focused", "drag", "hit_map"]
        .iter()
        .copied()
        .collect();
    let actual_free: HashSet<&str> = AppState::FREE_READ_FIELDS.iter().copied().collect();
    assert_eq!(actual_free, expected_free);
}
