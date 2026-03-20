use std::path::PathBuf;

use super::*;
use crate::{WasmPluginOrigin, WasmPluginRevision};

#[test]
fn resolve_wasm_plugins_loads_fixtures_directory() {
    let config = PluginsConfig {
        auto_discover: true,
        path: Some(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures")
                .to_string_lossy()
                .into_owned(),
        ),
        disabled: vec![],
        ..Default::default()
    };

    let resolved = crate::resolve_wasm_plugins(&config).unwrap();
    let snapshot = resolved.snapshot();

    assert_eq!(resolved.len(), 8);
    let cursor_line = PluginId("cursor_line".to_string());
    assert!(snapshot.contains(&cursor_line));
    assert!(matches!(
        snapshot.revision(&cursor_line),
        Some(WasmPluginRevision {
            origin: WasmPluginOrigin::Filesystem(path),
            ..
        }) if path.ends_with("cursor-line.wasm")
    ));
}

#[test]
fn resolve_wasm_plugins_skips_disabled_plugins() {
    let config = PluginsConfig {
        auto_discover: true,
        path: Some(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures")
                .to_string_lossy()
                .into_owned(),
        ),
        disabled: vec!["cursor_line".to_string()],
        ..Default::default()
    };

    let resolved = crate::resolve_wasm_plugins(&config).unwrap();
    let snapshot = resolved.snapshot();

    assert_eq!(resolved.len(), 7);
    assert!(!snapshot.contains(&PluginId("cursor_line".to_string())));
}

#[test]
fn resolve_wasm_plugins_includes_enabled_bundled_plugins() {
    let config = PluginsConfig {
        auto_discover: false,
        path: None,
        enabled: vec![
            "cursor_line".into(),
            "color_preview".into(),
            "sel_badge".into(),
            "fuzzy_finder".into(),
        ],
        disabled: vec![],
        ..Default::default()
    };

    let resolved = crate::resolve_wasm_plugins(&config).unwrap();
    let snapshot = resolved.snapshot();

    assert_eq!(resolved.len(), 4);
    assert!(matches!(
        snapshot.revision(&PluginId("cursor_line".to_string())),
        Some(WasmPluginRevision {
            origin: WasmPluginOrigin::Bundled("cursor_line"),
            ..
        })
    ));
}

#[test]
fn resolve_wasm_plugins_prefers_filesystem_over_bundled_for_same_id() {
    let config = PluginsConfig {
        auto_discover: true,
        path: Some(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures")
                .to_string_lossy()
                .into_owned(),
        ),
        enabled: vec![
            "cursor_line".into(),
            "color_preview".into(),
            "sel_badge".into(),
            "fuzzy_finder".into(),
        ],
        disabled: vec![],
        ..Default::default()
    };

    let resolved = crate::resolve_wasm_plugins(&config).unwrap();
    let snapshot = resolved.snapshot();

    assert_eq!(resolved.len(), 8);
    assert!(matches!(
        snapshot.revision(&PluginId("cursor_line".to_string())),
        Some(WasmPluginRevision {
            origin: WasmPluginOrigin::Filesystem(path),
            ..
        }) if path.ends_with("cursor-line.wasm")
    ));
}

#[test]
fn discover_loads_fixtures_directory() {
    let config = PluginsConfig {
        auto_discover: true,
        path: Some(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures")
                .to_string_lossy()
                .into_owned(),
        ),
        disabled: vec![],
        ..Default::default()
    };
    let mut registry = PluginRegistry::new();
    crate::discover_and_register(&config, &mut registry);

    // Should have loaded cursor-line.wasm, prompt-highlight.wasm, etc.
    assert!(registry.plugin_count() >= 2, "expected at least 2 plugins");
}

#[test]
fn discover_skips_disabled_plugins() {
    let config = PluginsConfig {
        auto_discover: true,
        path: Some(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures")
                .to_string_lossy()
                .into_owned(),
        ),
        disabled: vec!["cursor_line".to_string()],
        ..Default::default()
    };
    let mut registry = PluginRegistry::new();
    crate::discover_and_register(&config, &mut registry);

    // cursor-line skipped; the remaining fixtures still load.
    assert_eq!(registry.plugin_count(), 7);
}

#[test]
fn discover_does_nothing_when_disabled() {
    let config = PluginsConfig {
        auto_discover: false,
        path: Some(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures")
                .to_string_lossy()
                .into_owned(),
        ),
        disabled: vec![],
        ..Default::default()
    };
    let mut registry = PluginRegistry::new();
    crate::discover_and_register(&config, &mut registry);
    assert_eq!(registry.plugin_count(), 0);
}

#[test]
fn discover_handles_missing_directory() {
    let config = PluginsConfig {
        auto_discover: true,
        path: Some("/nonexistent/path/to/plugins".to_string()),
        disabled: vec![],
        ..Default::default()
    };
    let mut registry = PluginRegistry::new();
    // Should not panic, just silently skip
    crate::discover_and_register(&config, &mut registry);
    assert_eq!(registry.plugin_count(), 0);
}

// --- bundled plugin tests ---

#[test]
fn register_bundled_plugins_loads_four() {
    let config = PluginsConfig {
        auto_discover: false,
        path: None,
        enabled: vec![
            "cursor_line".into(),
            "color_preview".into(),
            "sel_badge".into(),
            "fuzzy_finder".into(),
        ],
        disabled: vec![],
        ..Default::default()
    };
    let mut registry = PluginRegistry::new();
    crate::register_bundled_plugins(&config, &mut registry);

    assert_eq!(registry.plugin_count(), 4);
}

#[test]
fn register_bundled_plugins_respects_disabled() {
    let config = PluginsConfig {
        auto_discover: false,
        path: None,
        enabled: vec![
            "cursor_line".into(),
            "color_preview".into(),
            "sel_badge".into(),
            "fuzzy_finder".into(),
        ],
        disabled: vec!["color_preview".to_string()],
        ..Default::default()
    };
    let mut registry = PluginRegistry::new();
    crate::register_bundled_plugins(&config, &mut registry);

    assert_eq!(registry.plugin_count(), 3);
}

#[test]
fn filesystem_plugin_overrides_bundled() {
    let config = PluginsConfig {
        auto_discover: false,
        path: None,
        enabled: vec![
            "cursor_line".into(),
            "color_preview".into(),
            "sel_badge".into(),
            "fuzzy_finder".into(),
        ],
        disabled: vec![],
        ..Default::default()
    };
    let mut registry = PluginRegistry::new();
    crate::register_bundled_plugins(&config, &mut registry);
    assert_eq!(registry.plugin_count(), 4);

    // Register another plugin with the same ID
    let loader = WasmPluginLoader::new().unwrap();
    let bytes = crate::load_wasm_fixture("cursor-line.wasm").unwrap();
    let plugin = loader
        .load(&bytes, &crate::WasiCapabilityConfig::default())
        .unwrap();
    assert_eq!(plugin.id().0, "cursor_line");
    registry.register_backend(Box::new(plugin));

    // Should still be 4, not 5 (replaced, not added)
    assert_eq!(registry.plugin_count(), 4);
}

#[test]
fn sdk_wit_matches_host_wit() {
    let host_wit = include_str!("../../wit/plugin.wit");
    let sdk_wit = include_str!("../../../kasane-plugin-sdk/wit/plugin.wit");
    assert_eq!(
        host_wit, sdk_wit,
        "SDK WIT and host WIT are out of sync — update kasane-plugin-sdk/wit/plugin.wit"
    );
    let macros_wit = include_str!("../../../kasane-plugin-sdk-macros/wit/plugin.wit");
    assert_eq!(
        host_wit, macros_wit,
        "Macros WIT and host WIT are out of sync — update kasane-plugin-sdk-macros/wit/plugin.wit"
    );
}

#[test]
fn sdk_slot_names_match_core() {
    use kasane_core::plugin::SlotId;

    // Verify SDK slot_name constants match core SlotId string representations
    let expected = [
        (SlotId::BUFFER_LEFT, "kasane.buffer.left"),
        (SlotId::BUFFER_RIGHT, "kasane.buffer.right"),
        (SlotId::ABOVE_BUFFER, "kasane.buffer.above"),
        (SlotId::BELOW_BUFFER, "kasane.buffer.below"),
        (SlotId::ABOVE_STATUS, "kasane.status.above"),
        (SlotId::STATUS_LEFT, "kasane.status.left"),
        (SlotId::STATUS_RIGHT, "kasane.status.right"),
        (SlotId::OVERLAY, "kasane.overlay"),
    ];

    for (slot_id, sdk_name) in &expected {
        assert_eq!(
            slot_id.as_str(),
            *sdk_name,
            "SDK slot_name constant mismatch for {:?}",
            slot_id
        );
    }
}
