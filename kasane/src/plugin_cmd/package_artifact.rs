use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use kasane_core::config::Config;
use kasane_plugin_package::manifest::PluginManifest;
use kasane_plugin_package::package::{self, BuildInput, InspectedPackage};
use serde::Deserialize;

use crate::plugin_store::PluginStore;

use super::build;

#[derive(Debug, Clone)]
pub struct BuiltPackage {
    pub path: PathBuf,
    pub inspected: InspectedPackage,
}

#[derive(Debug, Clone)]
pub struct InstalledPackage {
    pub path: PathBuf,
    pub inspected: InspectedPackage,
    pub lock_path: PathBuf,
}

#[derive(Debug)]
pub enum DiscoveredPackage {
    Valid {
        path: PathBuf,
        inspected: Box<InspectedPackage>,
    },
    Invalid {
        path: PathBuf,
        error: package::PackageError,
    },
}

pub fn build_project_package(project_dir: &str, release: bool) -> Result<BuiltPackage> {
    let project_path = Path::new(project_dir);
    let component_path = build::build_component(project_dir, release)?;
    let manifest = load_plugin_manifest(project_path)?;
    let cargo_manifest = load_cargo_manifest(project_path)?;
    let component = fs::read(&component_path)
        .with_context(|| format!("failed to read {}", component_path.display()))?;

    let output = package::build_package(BuildInput {
        package_name: cargo_manifest.package.name.clone(),
        package_version: cargo_manifest.package.version.clone(),
        component_entry: "plugin.wasm".to_string(),
        component,
        manifest,
        assets: Vec::new(),
    })
    .context("failed to build plugin package")?;

    let inspected = package::inspect_package(&output.bytes).context("failed to inspect package")?;
    let package_dir = project_path.join("target").join("kasane");
    fs::create_dir_all(&package_dir)
        .with_context(|| format!("failed to create {}", package_dir.display()))?;

    let package_path = package_dir.join(package_filename(&inspected));
    package::write_package(&package_path, &output)
        .with_context(|| format!("failed to write {}", package_path.display()))?;

    Ok(BuiltPackage {
        path: package_path,
        inspected,
    })
}

pub fn install_package_file(path: &Path) -> Result<InstalledPackage> {
    let config = Config::try_load()?;
    let plugins_dir = config.plugins.plugins_dir();
    let store = PluginStore::from_plugins_dir(&plugins_dir);
    let stored = store.put_verified_package(path)?;
    let inspected = package::inspect_package_file(&stored.path)
        .with_context(|| format!("failed to inspect {}", stored.path.display()))?;

    let saved = super::resolve::resolve_and_save(
        &config,
        super::resolve::ResolveOptions::reconcile()
            .request_artifact(stored.plugin_id.clone(), stored.artifact_digest.clone()),
    )?;

    Ok(InstalledPackage {
        path: stored.path,
        inspected,
        lock_path: saved.lock_path,
    })
}

pub fn discover_installed_packages(plugins_dir: &Path) -> Result<Vec<DiscoveredPackage>> {
    let mut package_paths = discover_package_paths(plugins_dir)?;
    package_paths.sort();

    let mut discovered = Vec::with_capacity(package_paths.len());
    for path in package_paths {
        match package::inspect_package_file(&path) {
            Ok(inspected) => discovered.push(DiscoveredPackage::Valid {
                path,
                inspected: Box::new(inspected),
            }),
            Err(error) => discovered.push(DiscoveredPackage::Invalid { path, error }),
        }
    }

    Ok(discovered)
}

pub fn package_label(inspected: &InspectedPackage) -> String {
    format!(
        "{}@{}",
        inspected.header.package.name, inspected.header.package.version
    )
}

#[derive(Debug, Deserialize)]
struct CargoManifest {
    package: CargoPackage,
}

#[derive(Debug, Deserialize)]
struct CargoPackage {
    name: String,
    version: String,
}

fn load_plugin_manifest(project_path: &Path) -> Result<PluginManifest> {
    let manifest_path = project_path.join("kasane-plugin.toml");
    if !manifest_path.exists() {
        bail!(
            "no kasane-plugin.toml found in '{}'. Is this a plugin project?",
            project_path.display()
        );
    }

    let contents = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    PluginManifest::parse(&contents)
        .with_context(|| format!("failed to parse {}", manifest_path.display()))
}

fn load_cargo_manifest(project_path: &Path) -> Result<CargoManifest> {
    let cargo_toml = project_path.join("Cargo.toml");
    let contents = fs::read_to_string(&cargo_toml)
        .with_context(|| format!("failed to read {}", cargo_toml.display()))?;
    toml::from_str(&contents).with_context(|| format!("failed to parse {}", cargo_toml.display()))
}

fn package_filename(inspected: &InspectedPackage) -> String {
    format!(
        "{}-{}.kpk",
        inspected.header.package.name.replace('/', "-"),
        inspected.header.package.version
    )
}

pub(super) fn touch_reload_sentinel(plugins_dir: &Path) {
    let reload_sentinel = plugins_dir.join(".reload");
    let _ = fs::write(reload_sentinel, "");
}

fn discover_package_paths(root: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    collect_package_paths(root, &mut paths)?;
    Ok(paths)
}

fn collect_package_paths(root: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err).with_context(|| format!("failed to read {}", root.display())),
    };

    for entry in entries {
        let entry = entry.with_context(|| format!("failed to read {}", root.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to inspect {}", path.display()))?;
        if file_type.is_dir() {
            collect_package_paths(&path, out)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("kpk") {
            out.push(path);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_filename_uses_package_name_and_version() {
        let inspected = package::inspect_package(
            &package::build_package(BuildInput {
                package_name: "example/demo-plugin".to_string(),
                package_version: "0.1.0".to_string(),
                component_entry: "plugin.wasm".to_string(),
                component: b"\0asmcomponent".to_vec(),
                manifest: PluginManifest::parse(
                    r#"
[plugin]
id = "demo_plugin"
abi_version = "0.25.0"
"#,
                )
                .unwrap(),
                assets: Vec::new(),
            })
            .unwrap()
            .bytes,
        )
        .unwrap();

        assert_eq!(
            package_filename(&inspected),
            "example-demo-plugin-0.1.0.kpk"
        );
    }
}
