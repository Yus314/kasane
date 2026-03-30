use std::collections::{HashMap, HashSet};

use crate::plugin::AppView;
use crate::protocol::{Coord, Face, KakouneRequest, MenuStyle};
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
fn test_apply_set_setting_updates_plugin_settings() {
    use crate::plugin::PluginId;
    use crate::plugin::setting::SettingValue;

    let mut state = AppState::default();
    let mut dirty = DirtyFlags::empty();
    let plugin_id = PluginId("smooth_scroll".to_string());

    crate::state::apply_set_setting(
        &mut state,
        &mut dirty,
        &plugin_id,
        "enabled",
        SettingValue::Bool(true),
    );

    assert!(dirty.contains(DirtyFlags::SETTINGS));
    assert_eq!(
        state
            .plugin_settings
            .get(&plugin_id)
            .and_then(|s| s.get("enabled")),
        Some(&SettingValue::Bool(true))
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
        ("status_style", &["STATUS"]),
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
        ("plugin_settings", &["SETTINGS"]),
        ("secondary_blend_ratio", &["BUFFER_CONTENT"]),
        ("theme", &["OPTIONS"]),
        ("color_context", &["BUFFER_CONTENT"]),
        ("editor_mode", &["STATUS"]),
        ("selections", &["BUFFER_CONTENT"]),
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
    let expected_free: HashSet<&str> = [
        "cols",
        "rows",
        "focused",
        "drag",
        "hit_map",
        "cursor_cache",
        "display_scroll_offset",
        "display_map",
        "display_unit_map",
        "fold_toggle_state",
    ]
    .iter()
    .copied()
    .collect();
    let actual_free: HashSet<&str> = AppState::FREE_READ_FIELDS.iter().copied().collect();
    assert_eq!(actual_free, expected_free);
}

// --- Epistemic classification tests ---

#[test]
fn test_field_epistemic_map_complete() {
    let expected: HashMap<&str, &str> = HashMap::from([
        // Observed (14)
        ("lines", "observed"),
        ("default_face", "observed"),
        ("padding_face", "observed"),
        ("cursor_pos", "observed"),
        ("status_prompt", "observed"),
        ("status_content", "observed"),
        ("status_content_cursor_pos", "observed"),
        ("status_mode_line", "observed"),
        ("status_default_face", "observed"),
        ("status_style", "observed"),
        ("widget_columns", "observed"),
        ("menu", "observed"),
        ("infos", "observed"),
        ("ui_options", "observed"),
        // Derived (5)
        ("lines_dirty", "derived"),
        ("cursor_mode", "derived"),
        ("status_line", "derived"),
        ("color_context", "derived"),
        ("editor_mode", "derived"),
        // Heuristic (3)
        ("cursor_count", "heuristic"),
        ("secondary_cursors", "heuristic"),
        ("selections", "heuristic"),
        // Config (12)
        ("shadow_enabled", "config"),
        ("padding_char", "config"),
        ("menu_max_height", "config"),
        ("menu_position", "config"),
        ("search_dropdown", "config"),
        ("status_at_top", "config"),
        ("scrollbar_thumb", "config"),
        ("scrollbar_track", "config"),
        ("assistant_art", "config"),
        ("plugin_config", "config"),
        ("secondary_blend_ratio", "config"),
        ("plugin_settings", "config"),
        ("theme", "config"),
        // Session (2)
        ("session_descriptors", "session"),
        ("active_session_key", "session"),
        // Runtime (9)
        ("focused", "runtime"),
        ("drag", "runtime"),
        ("cols", "runtime"),
        ("rows", "runtime"),
        ("hit_map", "runtime"),
        ("cursor_cache", "runtime"),
        ("display_scroll_offset", "runtime"),
        ("display_map", "runtime"),
        ("display_unit_map", "runtime"),
        ("fold_toggle_state", "runtime"),
    ]);

    let actual: HashMap<&str, &str> = AppState::FIELD_EPISTEMIC_MAP.iter().copied().collect();

    assert_eq!(
        actual.len(),
        expected.len(),
        "field count mismatch: actual={}, expected={}. \
         actual_only={:?}, expected_only={:?}",
        actual.len(),
        expected.len(),
        actual
            .keys()
            .filter(|k| !expected.contains_key(*k))
            .collect::<Vec<_>>(),
        expected
            .keys()
            .filter(|k| !actual.contains_key(*k))
            .collect::<Vec<_>>(),
    );

    for (field, cat) in &expected {
        assert_eq!(
            actual.get(field),
            Some(cat),
            "category mismatch for field `{field}`"
        );
    }
}

#[test]
fn test_heuristic_fields_have_rule_and_severity() {
    let expected: HashSet<(&str, &str, &str)> = HashSet::from([
        ("cursor_count", "I-1", "degraded"),
        ("secondary_cursors", "I-1", "degraded"),
        ("selections", "I-7", "degraded"),
    ]);

    let actual: HashSet<(&str, &str, &str)> = AppState::HEURISTIC_FIELDS.iter().copied().collect();

    assert_eq!(actual, expected);
}

#[test]
fn test_derived_fields_match() {
    let expected: HashSet<(&str, &str)> = HashSet::from([
        ("lines_dirty", "line equality diff (R-3)"),
        ("cursor_mode", "content_cursor_pos sign (I-3)"),
        ("status_line", "prompt + content concatenation"),
        ("color_context", "default_face luminance analysis"),
        ("editor_mode", "cursor_mode + mode_line (I-2)"),
    ]);

    let actual: HashSet<(&str, &str)> = AppState::DERIVED_FIELDS.iter().copied().collect();

    assert_eq!(actual, expected);
}

#[test]
fn test_fields_by_category_partition() {
    // Collect all fields from FIELDS_BY_CATEGORY
    let mut all_fields_from_categories: Vec<&str> = Vec::new();
    let mut seen_categories: HashSet<&str> = HashSet::new();

    for (cat, fields) in AppState::FIELDS_BY_CATEGORY {
        assert!(
            seen_categories.insert(cat),
            "duplicate category `{cat}` in FIELDS_BY_CATEGORY"
        );
        all_fields_from_categories.extend_from_slice(fields);
    }

    // Should be a complete partition: every field in FIELD_EPISTEMIC_MAP appears exactly once
    let epistemic_fields: HashSet<&str> = AppState::FIELD_EPISTEMIC_MAP
        .iter()
        .map(|(f, _)| *f)
        .collect();

    let category_fields: HashSet<&str> = all_fields_from_categories.iter().copied().collect();

    // No duplicates
    assert_eq!(
        all_fields_from_categories.len(),
        category_fields.len(),
        "duplicate fields in FIELDS_BY_CATEGORY"
    );

    // Complete
    assert_eq!(
        category_fields,
        epistemic_fields,
        "FIELDS_BY_CATEGORY is not a complete partition of FIELD_EPISTEMIC_MAP. \
         missing={:?}, extra={:?}",
        epistemic_fields
            .difference(&category_fields)
            .collect::<Vec<_>>(),
        category_fields
            .difference(&epistemic_fields)
            .collect::<Vec<_>>(),
    );
}

#[test]
fn test_epistemic_dirty_cross_consistency() {
    let free_set: HashSet<&str> = AppState::FREE_READ_FIELDS.iter().copied().collect();
    let dirty_map: HashMap<&str, &[&str]> = AppState::FIELD_DIRTY_MAP
        .iter()
        .map(|(f, flags)| (*f, *flags))
        .collect();

    for (field, cat) in AppState::FIELD_EPISTEMIC_MAP {
        match *cat {
            "runtime" => {
                assert!(
                    free_set.contains(field),
                    "runtime field `{field}` should be #[dirty(free)]"
                );
            }
            "session" => {
                let flags = dirty_map.get(field).unwrap_or_else(|| {
                    panic!("session field `{field}` not found in FIELD_DIRTY_MAP")
                });
                assert!(
                    flags.contains(&"SESSION"),
                    "session field `{field}` should have dirty(SESSION), got {flags:?}"
                );
            }
            _ => {}
        }
    }
}
