//! Plugin manifest — static TOML declaration for plugin metadata.
//!
//! The manifest is the authoritative source for sandbox construction (capabilities,
//! authorities), plugin identity, and handler metadata. It is parsed before WASM
//! compilation, so plugins never participate in their own permission decisions.

use kasane_core::plugin::{PluginAuthorities, PluginCapabilities};
use kasane_core::state::DirtyFlags;
use serde::Deserialize;

/// Parsed plugin manifest from a `.toml` sidecar file.
#[derive(Clone, Debug, Deserialize)]
pub struct PluginManifest {
    pub plugin: PluginSection,
    #[serde(default)]
    pub capabilities: CapabilitiesSection,
    #[serde(default)]
    pub authorities: AuthoritiesSection,
    #[serde(default)]
    pub handlers: HandlersSection,
    #[serde(default)]
    pub view: ViewSection,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PluginSection {
    pub id: String,
    pub abi_version: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct CapabilitiesSection {
    #[serde(default)]
    pub wasi: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct AuthoritiesSection {
    #[serde(default)]
    pub host: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct HandlersSection {
    #[serde(default)]
    pub flags: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ViewSection {
    #[serde(default)]
    pub deps: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("ABI version mismatch: manifest declares {manifest}, host is {host}")]
    AbiMismatch { manifest: String, host: String },

    #[error("unknown capability name: {0}")]
    UnknownCapability(String),

    #[error("unknown authority name: {0}")]
    UnknownAuthority(String),

    #[error("unknown handler flag: {0}")]
    UnknownHandlerFlag(String),

    #[error("unknown view dep: {0}")]
    UnknownViewDep(String),

    #[error("duplicate entry in [{section}]: {name}")]
    DuplicateEntry { section: &'static str, name: String },

    #[error("multiple validation errors:\n{}", .0.iter().map(|e| format!("  - {e}")).collect::<Vec<_>>().join("\n"))]
    Multiple(Vec<ManifestError>),
}

/// Current host ABI version (from WIT package declaration).
pub const HOST_ABI_VERSION: &str = "0.22.0";

impl PluginManifest {
    /// Parse a manifest from a TOML string.
    pub fn parse(toml_str: &str) -> Result<Self, ManifestError> {
        Ok(toml::from_str(toml_str)?)
    }

    /// Validate the manifest against the host ABI version and check all names.
    ///
    /// Accumulates all errors and returns them together (except ABI mismatch,
    /// which is an early return since subsequent checks are meaningless).
    pub fn validate(&self) -> Result<(), ManifestError> {
        // ABI version check: must match major.minor (patch can differ).
        // Early return — subsequent validation is meaningless on ABI mismatch.
        if !abi_compatible(&self.plugin.abi_version, HOST_ABI_VERSION) {
            return Err(ManifestError::AbiMismatch {
                manifest: self.plugin.abi_version.clone(),
                host: HOST_ABI_VERSION.to_string(),
            });
        }

        let mut errors = Vec::new();

        // Validate capability names + check duplicates
        {
            let mut seen = std::collections::HashSet::new();
            for name in &self.capabilities.wasi {
                if !seen.insert(name.as_str()) {
                    errors.push(ManifestError::DuplicateEntry {
                        section: "capabilities.wasi",
                        name: name.clone(),
                    });
                } else if capability_from_name(name).is_none() {
                    errors.push(ManifestError::UnknownCapability(name.clone()));
                }
            }
        }

        // Validate authority names + check duplicates
        {
            let mut seen = std::collections::HashSet::new();
            for name in &self.authorities.host {
                if !seen.insert(name.as_str()) {
                    errors.push(ManifestError::DuplicateEntry {
                        section: "authorities.host",
                        name: name.clone(),
                    });
                } else if authority_from_name(name).is_none() {
                    errors.push(ManifestError::UnknownAuthority(name.clone()));
                }
            }
        }

        // Validate handler flag names + check duplicates
        {
            let mut seen = std::collections::HashSet::new();
            for name in &self.handlers.flags {
                if !seen.insert(name.as_str()) {
                    errors.push(ManifestError::DuplicateEntry {
                        section: "handlers.flags",
                        name: name.clone(),
                    });
                } else if handler_flag_bit(name).is_none() {
                    errors.push(ManifestError::UnknownHandlerFlag(name.clone()));
                }
            }
        }

        // Validate view dep names + check duplicates
        {
            let mut seen = std::collections::HashSet::new();
            for name in &self.view.deps {
                if !seen.insert(name.as_str()) {
                    errors.push(ManifestError::DuplicateEntry {
                        section: "view.deps",
                        name: name.clone(),
                    });
                } else if view_dep_bit(name).is_none() {
                    errors.push(ManifestError::UnknownViewDep(name.clone()));
                }
            }
        }

        match errors.len() {
            0 => Ok(()),
            1 => Err(errors.into_iter().next().unwrap()),
            _ => Err(ManifestError::Multiple(errors)),
        }
    }

    /// Convert handler flags to `PluginCapabilities` bitmask.
    ///
    /// Empty flags → `PluginCapabilities::empty()` (opt-in: plugin must declare
    /// which dispatch paths it participates in).
    pub fn plugin_capabilities(&self) -> PluginCapabilities {
        if self.handlers.flags.is_empty() {
            return PluginCapabilities::empty();
        }
        let mut bits: u32 = 0;
        for name in &self.handlers.flags {
            if let Some(bit) = handler_flag_bit(name) {
                bits |= bit;
            }
        }
        PluginCapabilities::from_bits_truncate(bits)
    }

    /// Convert view deps to `DirtyFlags` bitmask.
    ///
    /// Empty deps → `DirtyFlags::ALL` (conservative: notify on all changes).
    pub fn dirty_flags(&self) -> DirtyFlags {
        if self.view.deps.is_empty() {
            return DirtyFlags::ALL;
        }
        let mut bits: u16 = 0;
        for name in &self.view.deps {
            if let Some(bit) = view_dep_bit(name) {
                bits |= bit;
            }
        }
        DirtyFlags::from_bits_truncate(bits)
    }

    /// WASI capability names from the manifest.
    pub fn wasi_capabilities(&self) -> &[String] {
        &self.capabilities.wasi
    }

    /// Host authority names from the manifest.
    pub fn host_authorities(&self) -> &[String] {
        &self.authorities.host
    }
}

// ---------------------------------------------------------------------------
// ABI version compatibility
// ---------------------------------------------------------------------------

/// Check ABI compatibility: major.minor must match exactly.
fn abi_compatible(manifest_version: &str, host_version: &str) -> bool {
    let manifest_mm = major_minor(manifest_version);
    let host_mm = major_minor(host_version);
    manifest_mm == host_mm
}

fn major_minor(version: &str) -> Option<(&str, &str)> {
    let mut parts = version.split('.');
    let major = parts.next()?;
    let minor = parts.next()?;
    Some((major, minor))
}

// ---------------------------------------------------------------------------
// String → bitflag mappings
// ---------------------------------------------------------------------------

/// Map WASI capability name to the WIT Capability enum variant name.
/// Returns Some(name) if valid, None if unknown.
pub fn capability_from_name(name: &str) -> Option<&'static str> {
    match name {
        "filesystem" => Some("filesystem"),
        "environment" => Some("environment"),
        "monotonic-clock" => Some("monotonic-clock"),
        "process" => Some("process"),
        _ => None,
    }
}

/// Map host authority name to a valid authority.
pub fn authority_from_name(name: &str) -> Option<PluginAuthorities> {
    match name {
        "dynamic-surface" => Some(PluginAuthorities::DYNAMIC_SURFACE),
        "pty-process" => Some(PluginAuthorities::PTY_PROCESS),
        "workspace-management" => Some(PluginAuthorities::WORKSPACE),
        _ => None,
    }
}

/// Map handler flag name to its bit value.
fn handler_flag_bit(name: &str) -> Option<u32> {
    match name {
        "overlay" => Some(1 << 2),
        "menu-transform" => Some(1 << 5),
        "cursor-style" => Some(1 << 6),
        "input-handler" => Some(1 << 7),
        "surface-provider" => Some(1 << 11),
        "workspace-observer" => Some(1 << 12),
        "paint-hook" => Some(1 << 13),
        "contributor" => Some(1 << 14),
        "transformer" => Some(1 << 15),
        "annotator" => Some(1 << 16),
        "io-handler" => Some(1 << 17),
        "display-transform" => Some(1 << 18),
        "scroll-policy" => Some(1 << 19),
        "cell-decoration" => Some(1 << 20),
        "navigation-policy" => Some(1 << 21),
        "navigation-action" => Some(1 << 22),
        _ => None,
    }
}

/// Map view dep name to its bit value.
fn view_dep_bit(name: &str) -> Option<u16> {
    match name {
        "buffer-content" => Some(1 << 0),
        "status" => Some(1 << 1),
        "menu-structure" => Some(1 << 2),
        "menu-selection" => Some(1 << 3),
        "info" => Some(1 << 4),
        "options" => Some(1 << 5),
        "buffer-cursor" => Some(1 << 6),
        "plugin-state" => Some(1 << 7),
        "session" => Some(1 << 8),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL_MANIFEST: &str = r#"
[plugin]
id = "test_plugin"
abi_version = "0.22.0"
"#;

    const FULL_MANIFEST: &str = r#"
[plugin]
id = "fuzzy_finder"
abi_version = "0.22.0"

[capabilities]
wasi = ["filesystem", "process"]

[authorities]
host = ["pty-process"]

[handlers]
flags = ["overlay", "input-handler", "io-handler", "contributor"]

[view]
deps = ["buffer-content", "buffer-cursor", "menu-structure", "menu-selection"]
"#;

    #[test]
    fn parse_minimal_manifest() {
        let manifest = PluginManifest::parse(MINIMAL_MANIFEST).unwrap();
        assert_eq!(manifest.plugin.id, "test_plugin");
        assert_eq!(manifest.plugin.abi_version, "0.22.0");
        assert!(manifest.capabilities.wasi.is_empty());
        assert!(manifest.authorities.host.is_empty());
        assert!(manifest.handlers.flags.is_empty());
        assert!(manifest.view.deps.is_empty());
    }

    #[test]
    fn parse_full_manifest() {
        let manifest = PluginManifest::parse(FULL_MANIFEST).unwrap();
        assert_eq!(manifest.plugin.id, "fuzzy_finder");
        assert_eq!(manifest.capabilities.wasi, vec!["filesystem", "process"]);
        assert_eq!(manifest.authorities.host, vec!["pty-process"]);
        assert_eq!(
            manifest.handlers.flags,
            vec!["overlay", "input-handler", "io-handler", "contributor"]
        );
        assert_eq!(
            manifest.view.deps,
            vec![
                "buffer-content",
                "buffer-cursor",
                "menu-structure",
                "menu-selection"
            ]
        );
    }

    #[test]
    fn validate_passes_for_matching_abi() {
        let manifest = PluginManifest::parse(MINIMAL_MANIFEST).unwrap();
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn validate_passes_for_different_patch() {
        let toml = r#"
[plugin]
id = "test"
abi_version = "0.22.1"
"#;
        let manifest = PluginManifest::parse(toml).unwrap();
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn validate_fails_for_wrong_minor() {
        let toml = r#"
[plugin]
id = "test"
abi_version = "0.21.0"
"#;
        let manifest = PluginManifest::parse(toml).unwrap();
        let err = manifest.validate().unwrap_err();
        assert!(matches!(err, ManifestError::AbiMismatch { .. }));
    }

    #[test]
    fn validate_fails_for_unknown_capability() {
        let toml = r#"
[plugin]
id = "test"
abi_version = "0.22.0"

[capabilities]
wasi = ["filesystem", "teleportation"]
"#;
        let manifest = PluginManifest::parse(toml).unwrap();
        let err = manifest.validate().unwrap_err();
        assert!(matches!(err, ManifestError::UnknownCapability(name) if name == "teleportation"));
    }

    #[test]
    fn validate_fails_for_unknown_authority() {
        let toml = r#"
[plugin]
id = "test"
abi_version = "0.22.0"

[authorities]
host = ["root-access"]
"#;
        let manifest = PluginManifest::parse(toml).unwrap();
        let err = manifest.validate().unwrap_err();
        assert!(matches!(err, ManifestError::UnknownAuthority(name) if name == "root-access"));
    }

    #[test]
    fn validate_fails_for_unknown_handler_flag() {
        let toml = r#"
[plugin]
id = "test"
abi_version = "0.22.0"

[handlers]
flags = ["overlay", "time-travel"]
"#;
        let manifest = PluginManifest::parse(toml).unwrap();
        let err = manifest.validate().unwrap_err();
        assert!(matches!(err, ManifestError::UnknownHandlerFlag(name) if name == "time-travel"));
    }

    #[test]
    fn validate_fails_for_unknown_view_dep() {
        let toml = r#"
[plugin]
id = "test"
abi_version = "0.22.0"

[view]
deps = ["buffer-content", "magic-data"]
"#;
        let manifest = PluginManifest::parse(toml).unwrap();
        let err = manifest.validate().unwrap_err();
        assert!(matches!(err, ManifestError::UnknownViewDep(name) if name == "magic-data"));
    }

    #[test]
    fn validate_full_manifest() {
        let manifest = PluginManifest::parse(FULL_MANIFEST).unwrap();
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn plugin_capabilities_from_flags() {
        let manifest = PluginManifest::parse(FULL_MANIFEST).unwrap();
        let caps = manifest.plugin_capabilities();
        assert!(caps.contains(PluginCapabilities::OVERLAY));
        assert!(caps.contains(PluginCapabilities::INPUT_HANDLER));
        assert!(caps.contains(PluginCapabilities::IO_HANDLER));
        assert!(caps.contains(PluginCapabilities::CONTRIBUTOR));
        assert!(!caps.contains(PluginCapabilities::ANNOTATOR));
        assert!(!caps.contains(PluginCapabilities::TRANSFORMER));
    }

    #[test]
    fn empty_flags_returns_empty_capabilities() {
        let manifest = PluginManifest::parse(MINIMAL_MANIFEST).unwrap();
        assert_eq!(manifest.plugin_capabilities(), PluginCapabilities::empty());
    }

    #[test]
    fn dirty_flags_from_deps() {
        let manifest = PluginManifest::parse(FULL_MANIFEST).unwrap();
        let flags = manifest.dirty_flags();
        assert!(flags.contains(DirtyFlags::BUFFER_CONTENT));
        assert!(flags.contains(DirtyFlags::BUFFER_CURSOR));
        assert!(flags.contains(DirtyFlags::MENU_STRUCTURE));
        assert!(flags.contains(DirtyFlags::MENU_SELECTION));
        assert!(!flags.contains(DirtyFlags::STATUS));
        assert!(!flags.contains(DirtyFlags::INFO));
    }

    #[test]
    fn empty_deps_returns_all_dirty_flags() {
        let manifest = PluginManifest::parse(MINIMAL_MANIFEST).unwrap();
        assert_eq!(manifest.dirty_flags(), DirtyFlags::ALL);
    }

    #[test]
    fn toml_parse_error() {
        let err = PluginManifest::parse("not valid toml {{{{").unwrap_err();
        assert!(matches!(err, ManifestError::TomlParse(_)));
    }

    #[test]
    fn missing_required_fields() {
        let toml = r#"
[plugin]
id = "test"
"#;
        let err = PluginManifest::parse(toml).unwrap_err();
        assert!(matches!(err, ManifestError::TomlParse(_)));
    }

    #[test]
    fn validate_accumulates_multiple_errors() {
        let toml = r#"
[plugin]
id = "test"
abi_version = "0.22.0"

[capabilities]
wasi = ["teleportation"]

[handlers]
flags = ["time-travel"]

[view]
deps = ["magic-data"]
"#;
        let manifest = PluginManifest::parse(toml).unwrap();
        let err = manifest.validate().unwrap_err();
        match err {
            ManifestError::Multiple(errors) => {
                assert_eq!(errors.len(), 3);
                assert!(
                    errors
                        .iter()
                        .any(|e| matches!(e, ManifestError::UnknownCapability(_)))
                );
                assert!(
                    errors
                        .iter()
                        .any(|e| matches!(e, ManifestError::UnknownHandlerFlag(_)))
                );
                assert!(
                    errors
                        .iter()
                        .any(|e| matches!(e, ManifestError::UnknownViewDep(_)))
                );
            }
            _ => panic!("expected Multiple error, got: {err}"),
        }
    }

    #[test]
    fn validate_detects_duplicate_wasi_capability() {
        let toml = r#"
[plugin]
id = "test"
abi_version = "0.22.0"

[capabilities]
wasi = ["filesystem", "filesystem"]
"#;
        let manifest = PluginManifest::parse(toml).unwrap();
        let err = manifest.validate().unwrap_err();
        assert!(
            matches!(err, ManifestError::DuplicateEntry { section: "capabilities.wasi", ref name } if name == "filesystem")
        );
    }

    #[test]
    fn validate_detects_duplicate_authority() {
        let toml = r#"
[plugin]
id = "test"
abi_version = "0.22.0"

[authorities]
host = ["pty-process", "pty-process"]
"#;
        let manifest = PluginManifest::parse(toml).unwrap();
        let err = manifest.validate().unwrap_err();
        assert!(
            matches!(err, ManifestError::DuplicateEntry { section: "authorities.host", ref name } if name == "pty-process")
        );
    }

    #[test]
    fn validate_detects_duplicate_handler_flag() {
        let toml = r#"
[plugin]
id = "test"
abi_version = "0.22.0"

[handlers]
flags = ["overlay", "overlay"]
"#;
        let manifest = PluginManifest::parse(toml).unwrap();
        let err = manifest.validate().unwrap_err();
        assert!(
            matches!(err, ManifestError::DuplicateEntry { section: "handlers.flags", ref name } if name == "overlay")
        );
    }

    #[test]
    fn validate_detects_duplicate_view_dep() {
        let toml = r#"
[plugin]
id = "test"
abi_version = "0.22.0"

[view]
deps = ["status", "status"]
"#;
        let manifest = PluginManifest::parse(toml).unwrap();
        let err = manifest.validate().unwrap_err();
        assert!(
            matches!(err, ManifestError::DuplicateEntry { section: "view.deps", ref name } if name == "status")
        );
    }

    #[test]
    fn validate_single_error_not_wrapped_in_multiple() {
        let toml = r#"
[plugin]
id = "test"
abi_version = "0.22.0"

[handlers]
flags = ["time-travel"]
"#;
        let manifest = PluginManifest::parse(toml).unwrap();
        let err = manifest.validate().unwrap_err();
        assert!(matches!(err, ManifestError::UnknownHandlerFlag(_)));
    }
}
