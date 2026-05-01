//! Plugin configuration: enable/disable lists, capability denials, selection pinning.

use std::collections::HashMap;
use std::path::PathBuf;

/// Plugin configuration.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct PluginsConfig {
    /// Custom path to the plugins directory. Defaults to XDG_DATA_HOME/kasane/plugins/.
    pub path: Option<String>,
    /// Bundled plugin IDs to enable (opt-in). Bundled plugins are NOT loaded unless
    /// listed here, except for default-enabled plugins (e.g. "pane_manager").
    /// Available: "cursor_line", "color_preview", "sel_badge", "fuzzy_finder", "pane_manager".
    pub enabled: Vec<String>,
    /// Plugin IDs to disable (by plugin ID, e.g. "cursor_line").
    /// Applies to filesystem-discovered and user-registered plugins.
    pub disabled: Vec<String>,
    /// Per-plugin capability denials. Key: plugin ID, Value: list of denied capability names.
    /// Valid capability names: "filesystem", "environment", "monotonic-clock", "process".
    pub deny_capabilities: HashMap<String, Vec<String>>,
    /// Per-plugin authority denials. Key: plugin ID, Value: list of denied authority names.
    /// Valid authority names: "dynamic-surface", "pty-process".
    pub deny_authorities: HashMap<String, Vec<String>>,
    /// Per-plugin active-set selection policy.
    pub selection: HashMap<String, PluginSelection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PluginSelection {
    #[default]
    Auto,
    PinDigest {
        digest: String,
    },
    PinPackage {
        package: String,
        version: Option<String>,
    },
}

impl PluginsConfig {
    /// Check if a bundled plugin should be loaded (opt-in via `enabled` list).
    pub fn is_bundled_enabled(&self, id: &str) -> bool {
        self.enabled.iter().any(|s| s == id)
    }

    pub fn is_disabled(&self, id: &str) -> bool {
        self.disabled.iter().any(|s| s == id)
    }

    pub fn selection_for(&self, id: &str) -> PluginSelection {
        self.selection.get(id).cloned().unwrap_or_default()
    }

    /// Resolve the plugins directory path.
    pub fn plugins_dir(&self) -> PathBuf {
        if let Some(ref p) = self.path {
            PathBuf::from(p)
        } else {
            dirs_data_path().join("plugins")
        }
    }
}

fn dirs_data_path() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        PathBuf::from(xdg).join("kasane")
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("kasane")
    } else {
        PathBuf::from("kasane-data")
    }
}
