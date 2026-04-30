use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;
use crate::ProviderArtifactStage;
use kasane_plugin_package::manifest::PluginManifest;
use kasane_plugin_package::package::{self, BuildInput, write_package};

struct TempPackageDir {
    path: PathBuf,
}

impl TempPackageDir {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "kasane-wasm-package-tests-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("failed to create temp package dir");
        Self { path }
    }

    fn write_fixture_package_as(
        &self,
        manifest_name: &str,
        wasm_name: &str,
        package_name: &str,
    ) -> PathBuf {
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures");
        let manifest_toml = fs::read_to_string(fixtures.join(manifest_name))
            .expect("failed to read fixture manifest");
        let manifest =
            PluginManifest::parse(&manifest_toml).expect("failed to parse fixture manifest");
        let component = fs::read(fixtures.join(wasm_name)).expect("failed to read fixture wasm");
        let output = package::build_package(BuildInput {
            package_name: format!("fixtures/{package_name}"),
            package_version: "0.1.0".to_string(),
            component_entry: "plugin.wasm".to_string(),
            component,
            manifest,
            assets: Vec::new(),
        })
        .expect("failed to build fixture package");
        let path = self.path.join(package_name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("failed to create nested package dir");
        }
        write_package(&path, &output).expect("failed to write fixture package");
        path
    }

    fn write_invalid_package(&self, file_name: &str) -> PathBuf {
        let path = self.path.join(file_name);
        fs::write(&path, b"not a package").expect("failed to write invalid package");
        path
    }
}

impl Drop for TempPackageDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn load_package_file_loads_fixture_package() {
    let temp = TempPackageDir::new();
    let package_path =
        temp.write_fixture_package_as("cursor-line.toml", "cursor-line.wasm", "cursor-line.kpk");
    let loader = WasmPluginLoader::new().unwrap();

    let plugin = loader
        .load_package_file(&package_path, &crate::WasiCapabilityConfig::default())
        .unwrap();

    assert_eq!(plugin.id(), PluginId("cursor_line".to_string()));
}

#[test]
fn load_package_file_loads_nested_fixture_package() {
    let temp = TempPackageDir::new();
    let package_path = temp.write_fixture_package_as(
        "prompt-highlight.toml",
        "prompt-highlight.wasm",
        "nested/prompt-highlight.kpk",
    );
    let loader = WasmPluginLoader::new().unwrap();

    let plugin = loader
        .load_package_file(&package_path, &crate::WasiCapabilityConfig::default())
        .unwrap();

    assert_eq!(plugin.id(), PluginId("prompt_highlight".to_string()));
}

#[test]
fn inspect_package_file_reports_invalid_package() {
    let temp = TempPackageDir::new();
    let path = temp.write_invalid_package("broken.kpk");

    let err = package::inspect_package_file(&path).unwrap_err();
    assert!(!err.to_string().is_empty());
}

#[test]
fn load_package_file_reports_instantiate_errors() {
    let temp = TempPackageDir::new();
    let path = temp.write_fixture_package_as(
        "instantiate-trap.toml",
        "instantiate-trap.wasm",
        "instantiate-trap.kpk",
    );
    let loader = WasmPluginLoader::new().unwrap();

    let (stage, err) =
        match loader.load_package_file(&path, &crate::WasiCapabilityConfig::default()) {
            Ok(_) => panic!("expected instantiate-trap package to fail"),
            Err(err) => err,
        };

    assert_eq!(stage, ProviderArtifactStage::Instantiate);
    assert!(err.to_string().contains("trap"));
}

#[test]
fn bundled_plugin_artifacts_include_default_enabled_pane_manager() {
    let artifacts = crate::bundled_plugin_artifacts().unwrap();
    let pane_manager = artifacts
        .into_iter()
        .find(|artifact| artifact.plugin_id == "pane_manager")
        .expect("pane_manager bundled plugin");

    assert_eq!(pane_manager.package_name, "builtin/pane-manager");
    assert!(pane_manager.default_enabled);
    assert_eq!(pane_manager.abi_version, "2.0.0");
}

#[test]
fn bundled_plugin_manifest_matches_artifact_metadata() {
    let artifact = crate::bundled_plugin_artifact_by_plugin_id("cursor_line")
        .unwrap()
        .expect("cursor_line bundled plugin");
    let manifest = crate::bundled_plugin_manifest_by_plugin_id("cursor_line")
        .unwrap()
        .expect("cursor_line bundled manifest");

    assert_eq!(artifact.plugin_id, manifest.plugin.id);
    assert_eq!(artifact.abi_version, manifest.plugin.abi_version);
}

#[test]
fn load_bundled_plugin_by_plugin_id_loads_requested_plugin() {
    let plugin = crate::load_bundled_plugin_by_plugin_id(
        "pane_manager",
        &crate::WasiCapabilityConfig::default(),
    )
    .unwrap();

    assert_eq!(plugin.id(), PluginId("pane_manager".to_string()));
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
