use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

const PLUGINS_LOCK_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginsLock {
    #[serde(default = "plugins_lock_version")]
    pub version: u32,
    #[serde(default)]
    pub plugins: BTreeMap<String, LockedPluginEntry>,
}

impl Default for PluginsLock {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockedPluginEntry {
    pub plugin_id: String,
    pub package: Option<String>,
    pub version: Option<String>,
    pub artifact_digest: String,
    pub code_digest: String,
    pub source_kind: String,
    pub abi_version: Option<String>,
}

impl PluginsLock {
    pub fn new() -> Self {
        Self {
            version: plugins_lock_version(),
            plugins: BTreeMap::new(),
        }
    }

    pub fn load() -> Result<Self> {
        Self::load_from_path(plugins_lock_path())
    }

    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Self::new()),
            Err(err) => {
                return Err(err).with_context(|| format!("failed to read {}", path.display()));
            }
        };
        let lock: Self = toml::from_str(&contents)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        lock.validate()
            .with_context(|| format!("failed to validate {}", path.display()))?;
        Ok(lock)
    }

    pub fn save(&self) -> Result<PathBuf> {
        let path = plugins_lock_path();
        self.save_to_path(&path)?;
        Ok(path)
    }

    pub fn save_to_path(&self, path: impl AsRef<Path>) -> Result<()> {
        self.validate()
            .context("failed to save invalid plugins lock")?;

        let path = path.as_ref();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let contents = toml::to_string_pretty(self).context("failed to serialize plugins lock")?;
        let temp_path = temp_lock_path(path);
        fs::write(&temp_path, contents)
            .with_context(|| format!("failed to write {}", temp_path.display()))?;
        fs::rename(&temp_path, path).with_context(|| {
            format!(
                "failed to atomically replace {} with {}",
                path.display(),
                temp_path.display()
            )
        })?;
        Ok(())
    }

    fn validate(&self) -> Result<()> {
        if self.version != plugins_lock_version() {
            bail!(
                "unsupported plugins.lock version {} (expected {})",
                self.version,
                plugins_lock_version()
            );
        }

        for (key, entry) in &self.plugins {
            if entry.plugin_id.is_empty() {
                bail!("plugin entry `{key}` has an empty plugin_id");
            }
            if key != &entry.plugin_id {
                bail!(
                    "plugin entry `{key}` does not match nested plugin_id `{}`",
                    entry.plugin_id
                );
            }
            if entry.artifact_digest.is_empty() {
                bail!("plugin entry `{key}` has an empty artifact_digest");
            }
            if entry.code_digest.is_empty() {
                bail!("plugin entry `{key}` has an empty code_digest");
            }
            if entry.source_kind.is_empty() {
                bail!("plugin entry `{key}` has an empty source_kind");
            }
        }

        Ok(())
    }
}

pub fn plugins_lock_path() -> PathBuf {
    plugins_lock_path_from_env(
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
        std::env::var_os("HOME").map(PathBuf::from),
    )
}

fn plugins_lock_version() -> u32 {
    PLUGINS_LOCK_VERSION
}

fn plugins_lock_path_from_env(xdg_config_home: Option<PathBuf>, home: Option<PathBuf>) -> PathBuf {
    if let Some(xdg) = xdg_config_home {
        xdg.join("kasane").join("plugins.lock")
    } else if let Some(home) = home {
        home.join(".config").join("kasane").join("plugins.lock")
    } else {
        PathBuf::from("plugins.lock")
    }
}

fn temp_lock_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("plugins.lock");
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id();
    path.with_file_name(format!(".{file_name}.{pid}.{stamp}.tmp"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugins_lock_path_prefers_xdg_config_home() {
        let path = plugins_lock_path_from_env(
            Some(PathBuf::from("/tmp/xdg-config")),
            Some(PathBuf::from("/tmp/home")),
        );
        assert_eq!(path, PathBuf::from("/tmp/xdg-config/kasane/plugins.lock"));
    }

    #[test]
    fn plugins_lock_path_falls_back_to_home() {
        let path = plugins_lock_path_from_env(None, Some(PathBuf::from("/tmp/home")));
        assert_eq!(path, PathBuf::from("/tmp/home/.config/kasane/plugins.lock"));
    }

    #[test]
    fn load_missing_plugins_lock_returns_empty_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("missing.lock");

        let lock = PluginsLock::load_from_path(&path).unwrap();
        assert_eq!(lock, PluginsLock::new());
    }

    #[test]
    fn save_and_load_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nested").join("plugins.lock");

        let mut lock = PluginsLock::new();
        lock.plugins.insert(
            "sel_badge".to_string(),
            LockedPluginEntry {
                plugin_id: "sel_badge".to_string(),
                package: Some("yus314/sel-badge".to_string()),
                version: Some("0.1.0".to_string()),
                artifact_digest: "sha256:artifact".to_string(),
                code_digest: "sha256:code".to_string(),
                source_kind: "filesystem".to_string(),
                abi_version: Some("0.25.0".to_string()),
            },
        );

        lock.save_to_path(&path).unwrap();
        let loaded = PluginsLock::load_from_path(&path).unwrap();
        assert_eq!(loaded, lock);
    }
}
