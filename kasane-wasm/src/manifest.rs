//! Plugin manifest — static TOML declaration for plugin metadata.
//!
//! The manifest is the authoritative source for sandbox construction (capabilities,
//! authorities), plugin identity, and handler metadata. It is parsed before WASM
//! compilation, so plugins never participate in their own permission decisions.

use std::collections::HashMap;

use compact_str::CompactString;
use kasane_core::plugin::{CapabilityDescriptor, PluginAuthorities, PluginCapabilities};
use kasane_core::state::DirtyFlags;
use kasane_plugin_model::{ExtensionPointId, SettingValue, TopicId, TransformTarget};
use serde::Deserialize;

/// Parsed plugin manifest from a `.toml` sidecar file.
#[derive(Clone, Debug, Deserialize)]
pub struct PluginManifest {
    #[serde(default)]
    pub manifest_version: Option<u32>,
    pub plugin: PluginSection,
    #[serde(default)]
    pub capabilities: CapabilitiesSection,
    #[serde(default)]
    pub authorities: AuthoritiesSection,
    #[serde(default)]
    pub handlers: HandlersSection,
    #[serde(default)]
    pub view: ViewSection,
    #[serde(default)]
    pub settings: HashMap<String, SettingSchema>,
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
    #[serde(default)]
    pub transform_targets: Vec<String>,
    #[serde(default)]
    pub publish_topics: Vec<String>,
    #[serde(default)]
    pub subscribe_topics: Vec<String>,
    #[serde(default)]
    pub extensions_defined: Vec<String>,
    #[serde(default)]
    pub extensions_consumed: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ViewSection {
    #[serde(default)]
    pub deps: Vec<String>,
}

/// Schema for a single plugin setting declared in the manifest.
#[derive(Clone, Debug, Deserialize)]
pub struct SettingSchema {
    #[serde(rename = "type")]
    pub setting_type: String,
    pub default: toml::Value,
    #[serde(default)]
    pub description: Option<String>,
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

    #[error("unsupported manifest_version: {found} (max supported: {max_supported})")]
    UnsupportedManifestVersion { found: u32, max_supported: u32 },

    #[error(
        "invalid setting type for key `{key}`: `{found}` (expected bool, integer, float, or string)"
    )]
    InvalidSettingType { key: String, found: String },

    #[error("invalid default value for setting `{key}`: expected {expected_type}")]
    InvalidSettingDefault { key: String, expected_type: String },

    #[error("multiple validation errors:\n{}", .0.iter().map(|e| format!("  - {e}")).collect::<Vec<_>>().join("\n"))]
    Multiple(Vec<ManifestError>),
}

/// Maximum supported manifest schema version.
pub const CURRENT_MANIFEST_VERSION: u32 = 2;

/// Current host ABI version (from WIT package declaration).
pub const HOST_ABI_VERSION: &str = "0.25.0";

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

        // Validate manifest_version
        if let Some(v) = self.manifest_version
            && v > CURRENT_MANIFEST_VERSION
        {
            errors.push(ManifestError::UnsupportedManifestVersion {
                found: v,
                max_supported: CURRENT_MANIFEST_VERSION,
            });
        }

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

        // Validate handler metadata fields (duplicates only — values are free-form strings)
        macro_rules! check_duplicates {
            ($field:expr, $section:expr) => {{
                let mut seen = std::collections::HashSet::new();
                for name in $field {
                    if !seen.insert(name.as_str()) {
                        errors.push(ManifestError::DuplicateEntry {
                            section: $section,
                            name: name.clone(),
                        });
                    }
                }
            }};
        }
        check_duplicates!(
            &self.handlers.transform_targets,
            "handlers.transform_targets"
        );
        check_duplicates!(&self.handlers.publish_topics, "handlers.publish_topics");
        check_duplicates!(&self.handlers.subscribe_topics, "handlers.subscribe_topics");
        check_duplicates!(
            &self.handlers.extensions_defined,
            "handlers.extensions_defined"
        );
        check_duplicates!(
            &self.handlers.extensions_consumed,
            "handlers.extensions_consumed"
        );

        // Validate settings section
        for (key, schema) in &self.settings {
            if !matches!(
                schema.setting_type.as_str(),
                "bool" | "integer" | "float" | "string"
            ) {
                errors.push(ManifestError::InvalidSettingType {
                    key: key.clone(),
                    found: schema.setting_type.clone(),
                });
                continue;
            }
            let type_ok = match schema.setting_type.as_str() {
                "bool" => schema.default.is_bool(),
                "integer" => schema.default.is_integer(),
                "float" => schema.default.is_float(),
                "string" => schema.default.is_str(),
                _ => false,
            };
            if !type_ok {
                errors.push(ManifestError::InvalidSettingDefault {
                    key: key.clone(),
                    expected_type: schema.setting_type.clone(),
                });
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

    /// Convert manifest setting defaults to a map of `SettingValue`.
    pub fn resolve_setting_defaults(&self) -> HashMap<String, SettingValue> {
        self.settings
            .iter()
            .filter_map(|(key, schema)| {
                toml_value_to_setting(&schema.setting_type, &schema.default)
                    .map(|v| (key.clone(), v))
            })
            .collect()
    }

    /// Validate config.toml values against the manifest schema.
    ///
    /// Returns `(valid_settings, warnings)`. Unknown keys and type mismatches
    /// produce warnings but do not fail — the default is used instead.
    pub fn validate_config_settings(
        &self,
        config: &toml::Table,
    ) -> (HashMap<String, SettingValue>, Vec<String>) {
        let mut valid = HashMap::new();
        let mut warnings = Vec::new();

        for (key, value) in config {
            let Some(schema) = self.settings.get(key) else {
                warnings.push(format!("unknown setting key `{key}`"));
                continue;
            };
            match toml_value_to_setting(&schema.setting_type, value) {
                Some(sv) => {
                    valid.insert(key.clone(), sv);
                }
                None => {
                    warnings.push(format!(
                        "setting `{key}`: expected {}, got {}",
                        schema.setting_type,
                        toml_type_name(value)
                    ));
                }
            }
        }

        (valid, warnings)
    }

    /// Build a [`CapabilityDescriptor`] from manifest metadata.
    pub fn capability_descriptor(&self) -> CapabilityDescriptor {
        CapabilityDescriptor {
            transform_targets: self
                .handlers
                .transform_targets
                .iter()
                .map(|s| TransformTarget::new(s.clone()))
                .collect(),
            contribution_slots: Vec::new(),
            annotation_scopes: Vec::new(),
            publish_topics: self
                .handlers
                .publish_topics
                .iter()
                .map(|s| TopicId::new(s.clone()))
                .collect(),
            subscribe_topics: self
                .handlers
                .subscribe_topics
                .iter()
                .map(|s| TopicId::new(s.clone()))
                .collect(),
            extensions_defined: self
                .handlers
                .extensions_defined
                .iter()
                .map(|s| ExtensionPointId::new(s.clone()))
                .collect(),
            extensions_consumed: self
                .handlers
                .extensions_consumed
                .iter()
                .map(|s| ExtensionPointId::new(s.clone()))
                .collect(),
        }
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
pub fn handler_flag_bit(name: &str) -> Option<u32> {
    match name {
        "overlay" => Some(1 << 2),
        "menu-transform" => Some(1 << 5),
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
        "navigation-policy" => Some(1 << 21),
        "navigation-action" => Some(1 << 22),
        _ => None,
    }
}

/// Convert a TOML value to a `SettingValue` given the expected type.
fn toml_value_to_setting(setting_type: &str, value: &toml::Value) -> Option<SettingValue> {
    match setting_type {
        "bool" => value.as_bool().map(SettingValue::Bool),
        "integer" => value.as_integer().map(SettingValue::Integer),
        "float" => value.as_float().map(SettingValue::Float),
        "string" => value
            .as_str()
            .map(|s| SettingValue::Str(CompactString::new(s))),
        _ => None,
    }
}

/// Return a human-readable type name for a TOML value.
fn toml_type_name(value: &toml::Value) -> &'static str {
    match value {
        toml::Value::Boolean(_) => "bool",
        toml::Value::Integer(_) => "integer",
        toml::Value::Float(_) => "float",
        toml::Value::String(_) => "string",
        toml::Value::Array(_) => "array",
        toml::Value::Table(_) => "table",
        toml::Value::Datetime(_) => "datetime",
    }
}

/// Map view dep name to its bit value.
pub fn view_dep_bit(name: &str) -> Option<u16> {
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
        "settings" => Some(1 << 9),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL_MANIFEST: &str = r#"
[plugin]
id = "test_plugin"
abi_version = "0.25.0"
"#;

    const FULL_MANIFEST: &str = r#"
[plugin]
id = "fuzzy_finder"
abi_version = "0.25.0"

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
        assert_eq!(manifest.plugin.abi_version, "0.25.0");
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
abi_version = "0.25.1"
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
abi_version = "0.25.0"

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
abi_version = "0.25.0"

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
abi_version = "0.25.0"

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
abi_version = "0.25.0"

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

    // --- Error accumulation tests ---

    #[test]
    fn validate_accumulates_multiple_errors() {
        let toml = r#"
[plugin]
id = "test"
abi_version = "0.25.0"

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
    fn validate_single_error_not_wrapped_in_multiple() {
        let toml = r#"
[plugin]
id = "test"
abi_version = "0.25.0"

[handlers]
flags = ["time-travel"]
"#;
        let manifest = PluginManifest::parse(toml).unwrap();
        let err = manifest.validate().unwrap_err();
        assert!(matches!(err, ManifestError::UnknownHandlerFlag(_)));
    }

    // --- Duplicate detection tests ---

    #[test]
    fn validate_detects_duplicate_wasi_capability() {
        let toml = r#"
[plugin]
id = "test"
abi_version = "0.25.0"

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
abi_version = "0.25.0"

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
abi_version = "0.25.0"

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
abi_version = "0.25.0"

[view]
deps = ["status", "status"]
"#;
        let manifest = PluginManifest::parse(toml).unwrap();
        let err = manifest.validate().unwrap_err();
        assert!(
            matches!(err, ManifestError::DuplicateEntry { section: "view.deps", ref name } if name == "status")
        );
    }

    // --- Manifest version tests ---

    #[test]
    fn manifest_version_1_is_valid() {
        let toml = r#"
manifest_version = 1

[plugin]
id = "test"
abi_version = "0.25.0"
"#;
        let manifest = PluginManifest::parse(toml).unwrap();
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn manifest_version_99_is_rejected() {
        let toml = r#"
manifest_version = 99

[plugin]
id = "test"
abi_version = "0.25.0"
"#;
        let manifest = PluginManifest::parse(toml).unwrap();
        let err = manifest.validate().unwrap_err();
        assert!(matches!(
            err,
            ManifestError::UnsupportedManifestVersion {
                found: 99,
                max_supported: 2
            }
        ));
    }

    #[test]
    fn manifest_version_absent_is_valid() {
        let manifest = PluginManifest::parse(MINIMAL_MANIFEST).unwrap();
        assert_eq!(manifest.manifest_version, None);
        assert!(manifest.validate().is_ok());
    }

    // --- Bit sync verification tests (P3.6) ---

    #[test]
    fn handler_flag_bits_match_plugin_capabilities() {
        assert_eq!(
            handler_flag_bit("overlay"),
            Some(PluginCapabilities::OVERLAY.bits())
        );
        assert_eq!(
            handler_flag_bit("menu-transform"),
            Some(PluginCapabilities::MENU_TRANSFORM.bits())
        );
        assert_eq!(
            handler_flag_bit("input-handler"),
            Some(PluginCapabilities::INPUT_HANDLER.bits())
        );
        assert_eq!(
            handler_flag_bit("surface-provider"),
            Some(PluginCapabilities::SURFACE_PROVIDER.bits())
        );
        assert_eq!(
            handler_flag_bit("workspace-observer"),
            Some(PluginCapabilities::WORKSPACE_OBSERVER.bits())
        );
        assert_eq!(
            handler_flag_bit("paint-hook"),
            Some(PluginCapabilities::PAINT_HOOK.bits())
        );
        assert_eq!(
            handler_flag_bit("contributor"),
            Some(PluginCapabilities::CONTRIBUTOR.bits())
        );
        assert_eq!(
            handler_flag_bit("transformer"),
            Some(PluginCapabilities::TRANSFORMER.bits())
        );
        assert_eq!(
            handler_flag_bit("annotator"),
            Some(PluginCapabilities::ANNOTATOR.bits())
        );
        assert_eq!(
            handler_flag_bit("io-handler"),
            Some(PluginCapabilities::IO_HANDLER.bits())
        );
        assert_eq!(
            handler_flag_bit("display-transform"),
            Some(PluginCapabilities::DISPLAY_TRANSFORM.bits())
        );
        assert_eq!(
            handler_flag_bit("scroll-policy"),
            Some(PluginCapabilities::SCROLL_POLICY.bits())
        );
        assert_eq!(
            handler_flag_bit("navigation-policy"),
            Some(PluginCapabilities::NAVIGATION_POLICY.bits())
        );
        assert_eq!(
            handler_flag_bit("navigation-action"),
            Some(PluginCapabilities::NAVIGATION_ACTION.bits())
        );
    }

    // --- New handlers section fields tests ---

    #[test]
    fn parse_manifest_with_new_handler_fields() {
        let toml = r#"
[plugin]
id = "ext_plugin"
abi_version = "0.25.0"

[handlers]
flags = ["transformer"]
transform_targets = ["kasane.buffer", "kasane.menu"]
publish_topics = ["cursor.line", "cursor.col"]
subscribe_topics = ["theme.changed"]
extensions_defined = ["myplugin.status-items"]
extensions_consumed = ["other.ext"]
"#;
        let manifest = PluginManifest::parse(toml).unwrap();
        assert_eq!(
            manifest.handlers.transform_targets,
            vec!["kasane.buffer", "kasane.menu"]
        );
        assert_eq!(
            manifest.handlers.publish_topics,
            vec!["cursor.line", "cursor.col"]
        );
        assert_eq!(manifest.handlers.subscribe_topics, vec!["theme.changed"]);
        assert_eq!(
            manifest.handlers.extensions_defined,
            vec!["myplugin.status-items"]
        );
        assert_eq!(manifest.handlers.extensions_consumed, vec!["other.ext"]);
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn capability_descriptor_from_manifest() {
        let toml = r#"
[plugin]
id = "desc_test"
abi_version = "0.25.0"

[handlers]
transform_targets = ["kasane.buffer"]
publish_topics = ["my.topic"]
subscribe_topics = ["other.topic"]
extensions_defined = ["my.ext"]
extensions_consumed = ["other.ext"]
"#;
        let manifest = PluginManifest::parse(toml).unwrap();
        let desc = manifest.capability_descriptor();
        assert_eq!(desc.transform_targets.len(), 1);
        assert_eq!(desc.transform_targets[0].as_str(), "kasane.buffer");
        assert_eq!(desc.publish_topics.len(), 1);
        assert_eq!(desc.publish_topics[0].as_str(), "my.topic");
        assert_eq!(desc.subscribe_topics.len(), 1);
        assert_eq!(desc.subscribe_topics[0].as_str(), "other.topic");
        assert_eq!(desc.extensions_defined.len(), 1);
        assert_eq!(desc.extensions_defined[0].as_str(), "my.ext");
        assert_eq!(desc.extensions_consumed.len(), 1);
        assert_eq!(desc.extensions_consumed[0].as_str(), "other.ext");
    }

    #[test]
    fn capability_descriptor_empty_for_minimal_manifest() {
        let manifest = PluginManifest::parse(MINIMAL_MANIFEST).unwrap();
        let desc = manifest.capability_descriptor();
        assert!(desc.transform_targets.is_empty());
        assert!(desc.publish_topics.is_empty());
        assert!(desc.subscribe_topics.is_empty());
        assert!(desc.extensions_defined.is_empty());
        assert!(desc.extensions_consumed.is_empty());
    }

    #[test]
    fn validate_detects_duplicate_transform_targets() {
        let toml = r#"
[plugin]
id = "test"
abi_version = "0.25.0"

[handlers]
transform_targets = ["kasane.buffer", "kasane.buffer"]
"#;
        let manifest = PluginManifest::parse(toml).unwrap();
        let err = manifest.validate().unwrap_err();
        assert!(
            matches!(err, ManifestError::DuplicateEntry { section: "handlers.transform_targets", ref name } if name == "kasane.buffer")
        );
    }

    #[test]
    fn validate_detects_duplicate_publish_topics() {
        let toml = r#"
[plugin]
id = "test"
abi_version = "0.25.0"

[handlers]
publish_topics = ["my.topic", "my.topic"]
"#;
        let manifest = PluginManifest::parse(toml).unwrap();
        let err = manifest.validate().unwrap_err();
        assert!(
            matches!(err, ManifestError::DuplicateEntry { section: "handlers.publish_topics", ref name } if name == "my.topic")
        );
    }

    #[test]
    fn validate_detects_duplicate_subscribe_topics() {
        let toml = r#"
[plugin]
id = "test"
abi_version = "0.25.0"

[handlers]
subscribe_topics = ["t", "t"]
"#;
        let manifest = PluginManifest::parse(toml).unwrap();
        let err = manifest.validate().unwrap_err();
        assert!(
            matches!(err, ManifestError::DuplicateEntry { section: "handlers.subscribe_topics", ref name } if name == "t")
        );
    }

    #[test]
    fn validate_detects_duplicate_extensions_defined() {
        let toml = r#"
[plugin]
id = "test"
abi_version = "0.25.0"

[handlers]
extensions_defined = ["my.ext", "my.ext"]
"#;
        let manifest = PluginManifest::parse(toml).unwrap();
        let err = manifest.validate().unwrap_err();
        assert!(
            matches!(err, ManifestError::DuplicateEntry { section: "handlers.extensions_defined", ref name } if name == "my.ext")
        );
    }

    #[test]
    fn validate_detects_duplicate_extensions_consumed() {
        let toml = r#"
[plugin]
id = "test"
abi_version = "0.25.0"

[handlers]
extensions_consumed = ["x", "x"]
"#;
        let manifest = PluginManifest::parse(toml).unwrap();
        let err = manifest.validate().unwrap_err();
        assert!(
            matches!(err, ManifestError::DuplicateEntry { section: "handlers.extensions_consumed", ref name } if name == "x")
        );
    }

    #[test]
    fn view_dep_bits_match_dirty_flags() {
        assert_eq!(
            view_dep_bit("buffer-content"),
            Some(DirtyFlags::BUFFER_CONTENT.bits())
        );
        assert_eq!(view_dep_bit("status"), Some(DirtyFlags::STATUS.bits()));
        assert_eq!(
            view_dep_bit("menu-structure"),
            Some(DirtyFlags::MENU_STRUCTURE.bits())
        );
        assert_eq!(
            view_dep_bit("menu-selection"),
            Some(DirtyFlags::MENU_SELECTION.bits())
        );
        assert_eq!(view_dep_bit("info"), Some(DirtyFlags::INFO.bits()));
        assert_eq!(view_dep_bit("options"), Some(DirtyFlags::OPTIONS.bits()));
        assert_eq!(
            view_dep_bit("buffer-cursor"),
            Some(DirtyFlags::BUFFER_CURSOR.bits())
        );
        assert_eq!(
            view_dep_bit("plugin-state"),
            Some(DirtyFlags::PLUGIN_STATE.bits())
        );
        assert_eq!(view_dep_bit("session"), Some(DirtyFlags::SESSION.bits()));
    }
}
