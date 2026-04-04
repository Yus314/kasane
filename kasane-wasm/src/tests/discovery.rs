use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;
use crate::{WasmPluginOrigin, WasmPluginRevision};
use kasane_core::plugin::{PluginDiagnosticKind, PluginProvider, ProviderArtifactStage};

fn test_provider(plugins_config: PluginsConfig) -> crate::WasmPluginProvider {
    crate::WasmPluginProvider::new(plugins_config, std::collections::HashMap::new())
}

struct TempPluginDir {
    path: PathBuf,
}

impl TempPluginDir {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "kasane-wasm-provider-discovery-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("failed to create temp plugin dir");
        Self { path }
    }

    fn copy_fixture(&self, fixture_name: &str) {
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures");
        let src = fixtures.join(fixture_name);
        let dst = self.path.join(fixture_name);
        fs::copy(&src, &dst).expect("failed to copy fixture");

        // Also copy sibling .toml manifest if it exists
        let toml_name = PathBuf::from(fixture_name).with_extension("toml");
        let toml_src = fixtures.join(&toml_name);
        if toml_src.exists() {
            let toml_dst = self.path.join(&toml_name);
            fs::copy(toml_src, toml_dst).expect("failed to copy fixture manifest");
        }
    }

    fn write_invalid_wasm(&self, file_name: &str) {
        fs::write(self.path.join(file_name), b"not a wasm component")
            .expect("failed to write invalid wasm");
    }

    /// Write a minimal valid manifest TOML for a fixture that doesn't have one.
    fn write_manifest(&self, toml_name: &str, plugin_id: &str) {
        let content = format!("[plugin]\nid = \"{plugin_id}\"\nabi_version = \"0.25.0\"\n");
        fs::write(self.path.join(toml_name), content).expect("failed to write manifest");
    }

    fn create_wasm_dir(&self, file_name: &str) {
        fs::create_dir(self.path.join(file_name)).expect("failed to create wasm directory");
    }
}

impl Drop for TempPluginDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

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

    // 8 loadable fixtures (instantiate-trap now fails: P2.2 ID verification calls get_id())
    // + pane_manager (bundled default-enabled) = 9
    assert_eq!(resolved.len(), 9);
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

    // 7 remaining loadable fixtures (8 loadable - 1 disabled) + pane_manager (bundled default-enabled)
    assert_eq!(resolved.len(), 8);
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

    // 4 explicitly enabled + pane_manager (bundled default-enabled)
    assert_eq!(resolved.len(), 5);
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

    // 8 loadable fixtures + pane_manager (bundled default-enabled) = 9
    assert_eq!(resolved.len(), 9);
    assert!(matches!(
        snapshot.revision(&PluginId("cursor_line".to_string())),
        Some(WasmPluginRevision {
            origin: WasmPluginOrigin::Filesystem(path),
            ..
        }) if path.ends_with("cursor-line.wasm")
    ));
}

#[test]
fn wasm_provider_collect_reports_artifact_load_failures_without_dropping_valid_plugins() {
    let dir = TempPluginDir::new();
    dir.copy_fixture("cursor-line.wasm");
    dir.write_invalid_wasm("broken.wasm");
    dir.write_manifest("broken.toml", "broken");

    let provider = test_provider(PluginsConfig {
        auto_discover: true,
        path: Some(dir.path.to_string_lossy().into_owned()),
        disabled: vec![],
        ..Default::default()
    });

    let collected = provider.collect().unwrap();

    assert!(
        collected
            .factories
            .iter()
            .any(|factory| factory.descriptor().id == PluginId("cursor_line".to_string()))
    );
    assert_eq!(collected.diagnostics.len(), 1);
    assert!(matches!(
        collected.diagnostics[0].kind,
        PluginDiagnosticKind::ProviderArtifactFailed {
            ref artifact,
            stage: ProviderArtifactStage::Load,
        } if artifact == "broken.wasm"
    ));
    assert_eq!(
        collected.diagnostics[0].provider_name(),
        Some("kasane_wasm::WasmPluginProvider")
    );
}

#[test]
fn wasm_provider_collect_reports_artifact_read_failures_without_dropping_valid_plugins() {
    let dir = TempPluginDir::new();
    dir.copy_fixture("cursor-line.wasm");
    dir.create_wasm_dir("unreadable.wasm");
    dir.write_manifest("unreadable.toml", "unreadable");

    let provider = test_provider(PluginsConfig {
        auto_discover: true,
        path: Some(dir.path.to_string_lossy().into_owned()),
        disabled: vec![],
        ..Default::default()
    });

    let collected = provider.collect().unwrap();

    assert!(
        collected
            .factories
            .iter()
            .any(|factory| factory.descriptor().id == PluginId("cursor_line".to_string()))
    );
    assert_eq!(collected.diagnostics.len(), 1);
    assert!(matches!(
        collected.diagnostics[0].kind,
        PluginDiagnosticKind::ProviderArtifactFailed {
            ref artifact,
            stage: ProviderArtifactStage::Read,
        } if artifact == "unreadable.wasm"
    ));
}

#[test]
fn instantiate_trap_fixture_reports_diagnostic_with_manifest() {
    // The instantiate-trap fixture traps in get_id(). With P2.2 ID verification,
    // load_with_manifest now calls get_id() to verify consistency, so the trap
    // causes an instantiation failure reported as a diagnostic.
    let dir = TempPluginDir::new();
    dir.copy_fixture("cursor-line.wasm");
    dir.copy_fixture("instantiate-trap.wasm");

    let provider = test_provider(PluginsConfig {
        auto_discover: true,
        path: Some(dir.path.to_string_lossy().into_owned()),
        disabled: vec![],
        ..Default::default()
    });

    let collected = provider.collect().unwrap();

    assert!(
        collected
            .factories
            .iter()
            .any(|factory| factory.descriptor().id == PluginId("cursor_line".to_string()))
    );
    // instantiate-trap should NOT load (get_id traps during ID verification)
    assert!(
        !collected
            .factories
            .iter()
            .any(|factory| factory.descriptor().id == PluginId("instantiate_trap".to_string()))
    );
    // Should have one diagnostic for the instantiate failure
    assert_eq!(collected.diagnostics.len(), 1);
    assert!(matches!(
        collected.diagnostics[0].kind,
        PluginDiagnosticKind::ProviderArtifactFailed {
            stage: ProviderArtifactStage::Instantiate,
            ..
        }
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
    let mut registry = PluginRuntime::new();
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
    let mut registry = PluginRuntime::new();
    crate::discover_and_register(&config, &mut registry);

    // cursor-line skipped; the remaining 7 loadable fixtures still load
    // (instantiate-trap fails: P2.2 ID verification traps).
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
    let mut registry = PluginRuntime::new();
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
    let mut registry = PluginRuntime::new();
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
    let mut registry = PluginRuntime::new();
    crate::register_bundled_plugins(&config, &mut registry);

    // 4 explicitly enabled + pane_manager (bundled default-enabled)
    assert_eq!(registry.plugin_count(), 5);
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
    let mut registry = PluginRuntime::new();
    crate::register_bundled_plugins(&config, &mut registry);

    // 3 explicitly enabled + pane_manager (bundled default-enabled)
    assert_eq!(registry.plugin_count(), 4);
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
    let mut registry = PluginRuntime::new();
    crate::register_bundled_plugins(&config, &mut registry);
    // 4 explicitly enabled + pane_manager (bundled default-enabled)
    assert_eq!(registry.plugin_count(), 5);

    // Register another plugin with the same ID
    let loader = WasmPluginLoader::new().unwrap();
    let bytes = crate::load_wasm_fixture("cursor-line.wasm").unwrap();
    let plugin = loader
        .load(&bytes, &crate::WasiCapabilityConfig::default())
        .unwrap();
    assert_eq!(plugin.id().0, "cursor_line");
    registry.register_backend(Box::new(plugin));

    // Should still be 5, not 6 (replaced, not added)
    assert_eq!(registry.plugin_count(), 5);
}

// --- manifest-first discovery tests ---

#[test]
fn wasm_without_manifest_is_not_discovered() {
    let dir = TempPluginDir::new();
    // Copy only the .wasm, no .toml manifest
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("cursor-line.wasm");
    fs::copy(src, dir.path.join("cursor-line.wasm")).expect("failed to copy");

    let provider = test_provider(PluginsConfig {
        auto_discover: true,
        path: Some(dir.path.to_string_lossy().into_owned()),
        disabled: vec!["pane_manager".to_string()],
        ..Default::default()
    });

    let collected = provider.collect().unwrap();
    // No .toml found → no filesystem plugins discovered; pane_manager disabled
    assert!(collected.factories.is_empty());
    assert!(collected.diagnostics.is_empty());
}

#[test]
fn invalid_manifest_toml_is_skipped() {
    let dir = TempPluginDir::new();
    dir.copy_fixture("cursor-line.wasm");
    // Overwrite the manifest with invalid TOML
    fs::write(dir.path.join("cursor-line.toml"), "not valid [[ toml")
        .expect("failed to write bad toml");

    let provider = test_provider(PluginsConfig {
        auto_discover: true,
        path: Some(dir.path.to_string_lossy().into_owned()),
        disabled: vec!["pane_manager".to_string()],
        ..Default::default()
    });

    let collected = provider.collect().unwrap();
    // Invalid TOML is skipped at parse time in discover_plugin_artifacts() (logged, not diagnostic)
    assert!(collected.factories.is_empty());
    assert!(collected.diagnostics.is_empty());
}

#[test]
fn manifest_abi_mismatch_reports_manifest_stage_diagnostic() {
    let dir = TempPluginDir::new();
    dir.copy_fixture("cursor-line.wasm");
    // Write manifest with wrong ABI version
    let bad_manifest = "[plugin]\nid = \"cursor_line\"\nabi_version = \"99.0.0\"\n";
    fs::write(dir.path.join("cursor-line.toml"), bad_manifest).expect("failed to write manifest");

    let provider = test_provider(PluginsConfig {
        auto_discover: true,
        path: Some(dir.path.to_string_lossy().into_owned()),
        disabled: vec!["pane_manager".to_string()],
        ..Default::default()
    });

    let collected = provider.collect().unwrap();
    assert!(collected.factories.is_empty());
    assert_eq!(collected.diagnostics.len(), 1);
    assert!(matches!(
        collected.diagnostics[0].kind,
        PluginDiagnosticKind::ProviderArtifactFailed {
            ref artifact,
            stage: ProviderArtifactStage::Manifest,
        } if artifact == "cursor-line.wasm"
    ));
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
