use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use kasane_plugin_package::package::{self, InspectedPackage};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredArtifact {
    pub artifact_digest: String,
    pub code_digest: String,
    pub path: PathBuf,
    pub plugin_id: String,
    pub package_name: String,
    pub package_version: String,
    pub abi_version: String,
}

#[derive(Debug, Clone)]
pub struct PluginStore {
    root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GarbageCollectResult {
    pub removed_paths: Vec<PathBuf>,
    pub removed_bytes: u64,
}

impl PluginStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn default() -> Self {
        Self::new(default_store_root())
    }

    pub fn from_plugins_dir(plugins_dir: impl Into<PathBuf>) -> Self {
        Self::new(plugins_dir.into().join("store"))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn path_for(&self, artifact_digest: &str) -> Result<PathBuf> {
        let (algorithm, hex) = split_digest(artifact_digest)?;
        if hex.len() < 4 {
            bail!("artifact digest is too short: {artifact_digest}");
        }
        Ok(self
            .root
            .join(algorithm)
            .join(&hex[0..2])
            .join(&hex[2..4])
            .join(format!("{hex}.kpk")))
    }

    pub fn contains(&self, artifact_digest: &str) -> Result<bool> {
        Ok(self.path_for(artifact_digest)?.exists())
    }

    pub fn put_verified_package(&self, source: &Path) -> Result<StoredArtifact> {
        let inspected = package::verify_package_file(source)
            .with_context(|| format!("failed to verify {}", source.display()))?;
        let artifact = StoredArtifact::from_inspected(
            self.path_for(&inspected.header.digests.artifact)?,
            &inspected,
        );

        if artifact.path.exists() {
            return Ok(artifact);
        }

        if let Some(parent) = artifact.path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let temp_path = temp_store_path(&artifact.path);
        fs::copy(source, &temp_path).with_context(|| {
            format!(
                "failed to copy {} to {}",
                source.display(),
                temp_path.display()
            )
        })?;
        if let Err(err) = fs::rename(&temp_path, &artifact.path) {
            if artifact.path.exists() {
                let _ = fs::remove_file(&temp_path);
            } else {
                return Err(err).with_context(|| {
                    format!(
                        "failed to atomically store {} at {}",
                        source.display(),
                        artifact.path.display()
                    )
                });
            }
        }

        Ok(artifact)
    }

    pub fn discover_package_paths(&self) -> Result<Vec<PathBuf>> {
        let mut paths = Vec::new();
        collect_package_paths(self.root(), &mut paths)?;
        paths.sort();
        Ok(paths)
    }

    pub fn garbage_collect(
        &self,
        lock: &crate::plugin_lock::PluginsLock,
    ) -> Result<GarbageCollectResult> {
        let mut referenced = BTreeSet::new();
        for entry in lock.plugins.values() {
            if entry.source_kind != "filesystem" {
                continue;
            }
            referenced.insert(self.path_for(&entry.artifact_digest)?);
        }

        let mut removed_paths = Vec::new();
        let mut removed_bytes = 0u64;
        for path in self.discover_package_paths()? {
            if referenced.contains(&path) {
                continue;
            }

            let file_len = fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
            fs::remove_file(&path)
                .with_context(|| format!("failed to remove {}", path.display()))?;
            removed_bytes += file_len;
            removed_paths.push(path);
        }

        removed_paths.sort();
        Ok(GarbageCollectResult {
            removed_paths,
            removed_bytes,
        })
    }
}

impl StoredArtifact {
    pub fn from_inspected(path: PathBuf, inspected: &InspectedPackage) -> Self {
        Self {
            artifact_digest: inspected.header.digests.artifact.clone(),
            code_digest: inspected.header.digests.code.clone(),
            path,
            plugin_id: inspected.header.plugin.id.clone(),
            package_name: inspected.header.package.name.clone(),
            package_version: inspected.header.package.version.clone(),
            abi_version: inspected.header.plugin.abi_version.clone(),
        }
    }
}

fn default_store_root() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        PathBuf::from(xdg)
            .join("kasane")
            .join("plugins")
            .join("store")
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("kasane")
            .join("plugins")
            .join("store")
    } else {
        PathBuf::from("kasane-data").join("plugins").join("store")
    }
}

fn split_digest(digest: &str) -> Result<(&str, &str)> {
    let Some((algorithm, hex)) = digest.split_once(':') else {
        bail!("invalid artifact digest format: {digest}");
    };
    if algorithm.is_empty() || hex.is_empty() || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        bail!("invalid artifact digest format: {digest}");
    }
    Ok((algorithm, hex))
}

fn temp_store_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("artifact.kpk");
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id();
    path.with_file_name(format!(".{file_name}.{pid}.{stamp}.tmp"))
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
    use crate::plugin_lock::{LockedPluginEntry, PluginsLock};
    use kasane_plugin_package::manifest::PluginManifest;
    use kasane_plugin_package::package::{AssetInput, BuildInput};

    fn build_test_package(path: &Path) {
        let manifest = PluginManifest::parse(
            r#"
[plugin]
id = "demo_plugin"
abi_version = "0.25.0"

[handlers]
flags = ["contributor"]
"#,
        )
        .unwrap();

        let output = package::build_package(BuildInput {
            package_name: "example/demo-plugin".to_string(),
            package_version: "0.1.0".to_string(),
            component_entry: "plugin.wasm".to_string(),
            component: b"\0asmcomponent".to_vec(),
            manifest,
            assets: vec![AssetInput {
                name: "assets/icon.txt".to_string(),
                bytes: b"icon".to_vec(),
            }],
        })
        .unwrap();
        package::write_package(path, &output).unwrap();
    }

    #[test]
    fn path_for_uses_digest_sharding() {
        let store = PluginStore::new("/tmp/plugins-store");
        let path = store
            .path_for("sha256:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789")
            .unwrap();
        assert_eq!(
            path,
            PathBuf::from(
                "/tmp/plugins-store/sha256/ab/cd/abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789.kpk"
            )
        );
    }

    #[test]
    fn put_verified_package_stores_by_digest() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("demo.kpk");
        build_test_package(&source);

        let store = PluginStore::new(tmp.path().join("store"));
        let stored = store.put_verified_package(&source).unwrap();
        assert!(stored.path.exists());
        assert_eq!(stored.plugin_id, "demo_plugin");
        assert!(stored.path.starts_with(store.root()));
    }

    #[test]
    fn discover_package_paths_finds_nested_packages() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("demo.kpk");
        build_test_package(&source);

        let store = PluginStore::new(tmp.path().join("store"));
        let stored = store.put_verified_package(&source).unwrap();
        let paths = store.discover_package_paths().unwrap();
        assert_eq!(paths, vec![stored.path]);
    }

    #[test]
    fn garbage_collect_removes_unreferenced_filesystem_artifacts() {
        let tmp = tempfile::tempdir().unwrap();
        let source_a = tmp.path().join("demo-a.kpk");
        let source_b = tmp.path().join("demo-b.kpk");
        build_test_package(&source_a);
        build_test_package(&source_b);

        let store = PluginStore::new(tmp.path().join("store"));
        let stored_a = store.put_verified_package(&source_a).unwrap();

        // Change package identity so the second artifact gets a different digest.
        let manifest = PluginManifest::parse(
            r#"
[plugin]
id = "demo_plugin_b"
abi_version = "0.25.0"

[handlers]
flags = ["contributor"]
"#,
        )
        .unwrap();
        let output = package::build_package(BuildInput {
            package_name: "example/demo-plugin-b".to_string(),
            package_version: "0.1.0".to_string(),
            component_entry: "plugin.wasm".to_string(),
            component: b"\0asmcomponent".to_vec(),
            manifest,
            assets: Vec::new(),
        })
        .unwrap();
        package::write_package(&source_b, &output).unwrap();
        let stored_b = store.put_verified_package(&source_b).unwrap();

        let mut lock = PluginsLock::new();
        lock.plugins.insert(
            stored_a.plugin_id.clone(),
            LockedPluginEntry {
                plugin_id: stored_a.plugin_id.clone(),
                package: Some(stored_a.package_name.clone()),
                version: Some(stored_a.package_version.clone()),
                artifact_digest: stored_a.artifact_digest.clone(),
                code_digest: stored_a.code_digest.clone(),
                source_kind: "filesystem".to_string(),
                abi_version: Some(stored_a.abi_version.clone()),
            },
        );

        let gc = store.garbage_collect(&lock).unwrap();
        assert_eq!(gc.removed_paths, vec![stored_b.path.clone()]);
        assert!(stored_a.path.exists());
        assert!(!stored_b.path.exists());
    }
}
