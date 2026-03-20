//! WASI capability resolution for WASM plugins.
//!
//! Plugins declare their required capabilities via `requested-capabilities` (WIT export).
//! The host resolves these against user configuration (`deny_capabilities` in config.toml)
//! and builds a per-plugin `WasiCtx` with the appropriate grants.

use std::collections::HashMap;
use std::path::PathBuf;

use wasmtime_wasi::{DirPerms, FilePerms, WasiCtx, WasiCtxBuilder};

use crate::bindings::kasane::plugin::types::Capability;

/// Configuration for resolving WASI capabilities across all plugins.
#[derive(Clone)]
pub struct WasiCapabilityConfig {
    /// Base directory for per-plugin data directories.
    /// Each plugin gets `<data_base_dir>/<plugin_id>/data/`.
    pub data_base_dir: PathBuf,
    /// Current working directory to preopen for filesystem-capable plugins.
    pub cwd: PathBuf,
    /// Per-plugin capability denials. Key: plugin ID, Value: denied capability names.
    pub deny_capabilities: HashMap<String, Vec<String>>,
}

impl Default for WasiCapabilityConfig {
    fn default() -> Self {
        Self {
            data_base_dir: default_plugins_data_dir(),
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            deny_capabilities: HashMap::new(),
        }
    }
}

impl WasiCapabilityConfig {
    /// Create from a `PluginsConfig`.
    pub fn from_plugins_config(config: &kasane_core::config::PluginsConfig) -> Self {
        Self {
            data_base_dir: config.plugins_dir(),
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            deny_capabilities: config.deny_capabilities.clone(),
        }
    }
}

fn default_plugins_data_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        PathBuf::from(xdg).join("kasane").join("plugins")
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("kasane")
            .join("plugins")
    } else {
        PathBuf::from("kasane-data").join("plugins")
    }
}

fn capability_name(cap: &Capability) -> &'static str {
    match cap {
        Capability::Filesystem => "filesystem",
        Capability::Environment => "environment",
        Capability::MonotonicClock => "monotonic-clock",
        Capability::Process => "process",
    }
}

/// Check whether a specific capability is granted for a plugin.
///
/// Returns `true` if the capability is in the requested list and not denied by config.
pub fn is_capability_granted(
    plugin_id: &str,
    cap: &Capability,
    requested: &[Capability],
    config: &WasiCapabilityConfig,
) -> bool {
    let name = capability_name(cap);
    let is_requested = requested.iter().any(|r| capability_name(r) == name);
    if !is_requested {
        return false;
    }
    let denied = config
        .deny_capabilities
        .get(plugin_id)
        .map(|v| v.as_slice())
        .unwrap_or_default();
    !denied.iter().any(|d| d == name)
}

/// Build a `WasiCtx` for a plugin based on its requested capabilities and user config.
///
/// Capabilities that appear in the deny list for this plugin are skipped.
/// Default `WasiCtxBuilder::new()` already provides clocks and RNG, so `monotonic-clock`
/// is a no-op grant (it serves as a declaration for auditability).
pub fn build_wasi_ctx(
    plugin_id: &str,
    requested: &[Capability],
    config: &WasiCapabilityConfig,
) -> anyhow::Result<WasiCtx> {
    let denied = config
        .deny_capabilities
        .get(plugin_id)
        .map(|v| v.as_slice())
        .unwrap_or_default();

    let mut builder = WasiCtxBuilder::new();

    for cap in requested {
        let name = capability_name(cap);
        if denied.iter().any(|d| d == name) {
            tracing::info!(
                plugin = plugin_id,
                capability = name,
                "WASI capability denied by configuration"
            );
            continue;
        }

        match cap {
            Capability::Filesystem => {
                let data_dir = config.data_base_dir.join(plugin_id).join("data");
                std::fs::create_dir_all(&data_dir)?;

                builder.preopened_dir(&data_dir, "data", DirPerms::all(), FilePerms::all())?;
                builder.preopened_dir(&config.cwd, ".", DirPerms::READ, FilePerms::READ)?;

                tracing::info!(
                    plugin = plugin_id,
                    data_dir = %data_dir.display(),
                    cwd = %config.cwd.display(),
                    "granted filesystem capability"
                );
            }
            Capability::Environment => {
                builder.inherit_env();
                tracing::info!(plugin = plugin_id, "granted environment capability");
            }
            Capability::MonotonicClock => {
                // Clocks are provided by default in WasiCtxBuilder::new().
                // This branch exists for auditability logging.
                tracing::info!(plugin = plugin_id, "granted monotonic-clock capability");
            }
            Capability::Process => {
                // Process execution is handled at the DeferredCommand level
                // (ProcessManager checks capability before spawning).
                // This branch exists for auditability logging.
                tracing::info!(plugin = plugin_id, "granted process capability");
            }
        }
    }

    Ok(builder.build())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_capabilities_produces_default_ctx() {
        let config = WasiCapabilityConfig::default();
        let ctx = build_wasi_ctx("test_plugin", &[], &config);
        assert!(ctx.is_ok());
    }

    #[test]
    fn denied_capability_is_skipped() {
        let config = WasiCapabilityConfig {
            deny_capabilities: HashMap::from([(
                "test_plugin".to_string(),
                vec!["filesystem".to_string()],
            )]),
            ..Default::default()
        };
        // Should succeed even though filesystem was requested — it's denied, not errored
        let ctx = build_wasi_ctx("test_plugin", &[Capability::Filesystem], &config);
        assert!(ctx.is_ok());
    }

    #[test]
    fn filesystem_creates_data_dir() {
        let tmp = std::env::temp_dir().join("kasane_test_cap_fs");
        let _ = std::fs::remove_dir_all(&tmp);

        let config = WasiCapabilityConfig {
            data_base_dir: tmp.clone(),
            cwd: std::env::current_dir().unwrap(),
            deny_capabilities: HashMap::new(),
        };

        let ctx = build_wasi_ctx("my_plugin", &[Capability::Filesystem], &config);
        assert!(ctx.is_ok());
        assert!(tmp.join("my_plugin").join("data").is_dir());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn monotonic_clock_is_noop() {
        let config = WasiCapabilityConfig::default();
        let ctx = build_wasi_ctx("clock_plugin", &[Capability::MonotonicClock], &config);
        assert!(ctx.is_ok());
    }

    #[test]
    fn deny_does_not_affect_other_plugins() {
        let config = WasiCapabilityConfig {
            deny_capabilities: HashMap::from([(
                "other_plugin".to_string(),
                vec!["environment".to_string()],
            )]),
            ..Default::default()
        };
        // "test_plugin" is not denied, so environment should be granted
        let ctx = build_wasi_ctx("test_plugin", &[Capability::Environment], &config);
        assert!(ctx.is_ok());
    }

    // --- Phase P-2: process capability grant/deny tests ---

    #[test]
    fn process_capability_granted_when_requested() {
        let config = WasiCapabilityConfig::default();
        assert!(is_capability_granted(
            "my_plugin",
            &Capability::Process,
            &[Capability::Process],
            &config,
        ));
    }

    #[test]
    fn process_capability_denied_when_not_requested() {
        let config = WasiCapabilityConfig::default();
        // Plugin did not request process capability
        assert!(!is_capability_granted(
            "my_plugin",
            &Capability::Process,
            &[], // no capabilities requested
            &config,
        ));
    }

    #[test]
    fn process_capability_denied_by_config() {
        let config = WasiCapabilityConfig {
            deny_capabilities: HashMap::from([(
                "my_plugin".to_string(),
                vec!["process".to_string()],
            )]),
            ..Default::default()
        };
        // Plugin requests process, but config denies it
        assert!(!is_capability_granted(
            "my_plugin",
            &Capability::Process,
            &[Capability::Process],
            &config,
        ));
    }

    #[test]
    fn process_capability_deny_does_not_affect_other_capabilities() {
        let config = WasiCapabilityConfig {
            deny_capabilities: HashMap::from([(
                "my_plugin".to_string(),
                vec!["process".to_string()],
            )]),
            ..Default::default()
        };
        // Filesystem is requested and not denied, should be granted
        assert!(is_capability_granted(
            "my_plugin",
            &Capability::Filesystem,
            &[Capability::Filesystem, Capability::Process],
            &config,
        ));
        // Process is denied
        assert!(!is_capability_granted(
            "my_plugin",
            &Capability::Process,
            &[Capability::Filesystem, Capability::Process],
            &config,
        ));
    }

    #[test]
    fn process_deny_for_other_plugin_does_not_affect_target() {
        let config = WasiCapabilityConfig {
            deny_capabilities: HashMap::from([(
                "other_plugin".to_string(),
                vec!["process".to_string()],
            )]),
            ..Default::default()
        };
        // "my_plugin" is not denied
        assert!(is_capability_granted(
            "my_plugin",
            &Capability::Process,
            &[Capability::Process],
            &config,
        ));
    }
}
