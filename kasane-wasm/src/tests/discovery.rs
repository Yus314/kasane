use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;
use crate::{WasmPluginOrigin, WasmPluginRevision};
use kasane_core::plugin::{PluginDiagnosticKind, PluginProvider, ProviderArtifactStage};
use kasane_plugin_package::manifest::PluginManifest;
use kasane_plugin_package::package::{BuildInput, build_package, write_package};

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
        let manifest_name = PathBuf::from(fixture_name).with_extension("toml");
        let package_name = PathBuf::from(fixture_name).with_extension("kpk");
        self.write_fixture_package_as(
            manifest_name
                .to_str()
                .expect("fixture manifest name must be UTF-8"),
            fixture_name,
            package_name
                .to_str()
                .expect("fixture package name must be UTF-8"),
        );
    }

    fn write_fixture_package_as(&self, manifest_name: &str, wasm_name: &str, package_name: &str) {
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures");
        let manifest_toml = fs::read_to_string(fixtures.join(manifest_name))
            .expect("failed to read fixture manifest");
        let manifest =
            PluginManifest::parse(&manifest_toml).expect("failed to parse fixture manifest");
        let component = fs::read(fixtures.join(wasm_name)).expect("failed to read fixture wasm");
        let output = build_package(BuildInput {
            package_name: format!("fixtures/{package_name}"),
            package_version: "0.1.0".to_string(),
            component_entry: "plugin.wasm".to_string(),
            component,
            manifest,
            assets: Vec::new(),
        })
        .expect("failed to build fixture package");
        write_package(self.path.join(package_name), &output)
            .expect("failed to write fixture package");
    }

<<<<<<< HEAD
    fn write_invalid_package(&self, file_name: &str) {
        fs::write(self.path.join(file_name), b"not a package").expect("failed to write package");
    }
}

impl Drop for TempPluginDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn resolve_wasm_plugins_loads_fixtures_directory() {
    let temp = TempPluginDir::new();
    temp.copy_fixture("cursor-line.wasm");
    temp.copy_fixture("prompt-highlight.wasm");

    let config = PluginsConfig {
        path: Some(temp.path.to_string_lossy().into_owned()),
        disabled: vec!["pane_manager".to_string()],
        ..Default::default()
    };

    let resolved = crate::resolve_wasm_plugins(&config).unwrap();
    let snapshot = resolved.snapshot();

    assert_eq!(resolved.len(), 2);
    let cursor_line = PluginId("cursor_line".to_string());
    assert!(snapshot.contains(&cursor_line));
    assert!(matches!(
        snapshot.revision(&cursor_line),
        Some(WasmPluginRevision {
            origin: WasmPluginOrigin::FilesystemPackage(path),
            ..
        }) if path.ends_with("cursor-line.kpk")
    ));
}

#[test]
fn resolve_wasm_plugins_skips_disabled_plugins() {
    let temp = TempPluginDir::new();
    temp.copy_fixture("cursor-line.wasm");
    temp.copy_fixture("prompt-highlight.wasm");

    let config = PluginsConfig {
        path: Some(temp.path.to_string_lossy().into_owned()),
        disabled: vec!["cursor_line".to_string(), "pane_manager".to_string()],
        ..Default::default()
    };

    let resolved = crate::resolve_wasm_plugins(&config).unwrap();
    let snapshot = resolved.snapshot();

    assert_eq!(resolved.len(), 1);
    assert!(!snapshot.contains(&PluginId("cursor_line".to_string())));
    assert!(snapshot.contains(&PluginId("prompt_highlight".to_string())));
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
    let temp = TempPluginDir::new();
    temp.copy_fixture("cursor-line.wasm");

    let config = PluginsConfig {
        path: Some(temp.path.to_string_lossy().into_owned()),
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

    assert!(matches!(
        snapshot.revision(&PluginId("cursor_line".to_string())),
        Some(WasmPluginRevision {
            origin: WasmPluginOrigin::FilesystemPackage(path),
            ..
        }) if path.ends_with("cursor-line.kpk")
    ));
}

#[test]
fn resolve_wasm_plugins_loads_package_artifacts() {
    let temp = TempPluginDir::new();
    temp.write_fixture_package_as("cursor-line.toml", "cursor-line.wasm", "cursor-line.kpk");

    let config = PluginsConfig {
        auto_discover: true,
        path: Some(temp.path.to_string_lossy().into_owned()),
        disabled: vec![],
        ..Default::default()
    };

    let resolved = crate::resolve_wasm_plugins(&config).unwrap();
    let snapshot = resolved.snapshot();
    let cursor_line = PluginId("cursor_line".to_string());

    assert!(snapshot.contains(&cursor_line));
    assert!(matches!(
        snapshot.revision(&cursor_line),
        Some(WasmPluginRevision {
            origin: WasmPluginOrigin::FilesystemPackage(path),
            ..
        }) if path.ends_with("cursor-line.kpk")
    ));
}

#[test]
fn resolve_wasm_plugins_discovers_packages_recursively() {
    let temp = TempPluginDir::new();
    fs::create_dir_all(temp.path.join("nested")).unwrap();
    temp.write_fixture_package_as(
        "cursor-line.toml",
        "cursor-line.wasm",
        "nested/cursor-line.kpk",
    );

    let config = PluginsConfig {
        path: Some(temp.path.to_string_lossy().into_owned()),
        disabled: vec![],
        ..Default::default()
    };

    let resolved = crate::resolve_wasm_plugins(&config).unwrap();
    let snapshot = resolved.snapshot();
    let cursor_line = PluginId("cursor_line".to_string());

    assert!(matches!(
        snapshot.revision(&cursor_line),
        Some(WasmPluginRevision {
            origin: WasmPluginOrigin::FilesystemPackage(path),
            ..
        }) if path.ends_with("nested/cursor-line.kpk")
    ));
}

#[test]
fn wasm_provider_collect_reports_invalid_packages_without_dropping_valid_plugins() {
    let dir = TempPluginDir::new();
    dir.copy_fixture("cursor-line.wasm");
    dir.write_invalid_package("broken.kpk");

    let provider = test_provider(PluginsConfig {
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
            stage: ProviderArtifactStage::Manifest,
        } if artifact.ends_with("broken.kpk")
    ));
    assert_eq!(
        collected.diagnostics[0].provider_name(),
        Some("kasane_wasm::WasmPluginProvider")
    );
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
    let temp = TempPluginDir::new();
    temp.copy_fixture("cursor-line.wasm");
    temp.copy_fixture("prompt-highlight.wasm");

    let config = PluginsConfig {
        path: Some(temp.path.to_string_lossy().into_owned()),
        disabled: vec!["pane_manager".to_string()],
        ..Default::default()
    };
    let mut registry = PluginRuntime::new();
    crate::discover_and_register(&config, &mut registry);

    assert_eq!(registry.plugin_count(), 2);
}

#[test]
fn discover_skips_disabled_plugins() {
    let temp = TempPluginDir::new();
    temp.copy_fixture("cursor-line.wasm");
    temp.copy_fixture("prompt-highlight.wasm");

    let config = PluginsConfig {
        path: Some(temp.path.to_string_lossy().into_owned()),
        disabled: vec!["cursor_line".to_string(), "pane_manager".to_string()],
        ..Default::default()
    };
    let mut registry = PluginRuntime::new();
    crate::discover_and_register(&config, &mut registry);

    assert_eq!(registry.plugin_count(), 1);
}

#[test]
fn discover_ignores_auto_discover_flag_for_packages() {
    let temp = TempPluginDir::new();
    temp.copy_fixture("cursor-line.wasm");

    let config = PluginsConfig {
        auto_discover: false,
        path: Some(temp.path.to_string_lossy().into_owned()),
        disabled: vec!["pane_manager".to_string()],
        ..Default::default()
    };
    let mut registry = PluginRuntime::new();
    crate::discover_and_register(&config, &mut registry);
    assert_eq!(registry.plugin_count(), 1);
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
