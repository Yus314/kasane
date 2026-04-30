//! Plugin manifest — static TOML declaration for plugin metadata.
//!
//! This schema is the source-of-truth input for package building. It remains
//! separate from the runtime package header, which is generated from this
//! manifest in canonical form.

use std::collections::HashMap;

use compact_str::CompactString;
use kasane_core::plugin::{CapabilityDescriptor, PluginAuthorities, PluginCapabilities};
use kasane_core::state::DirtyFlags;
use kasane_plugin_model::{ExtensionPointId, SettingValue, TopicId, TransformTarget};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PluginSection {
    pub id: String,
    pub abi_version: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct CapabilitiesSection {
    #[serde(default)]
    pub wasi: Vec<String>,
    /// Additional environment variables to expose beyond the safe default set.
    ///
    /// Only effective when `wasi` includes `"environment"`. Each entry is a
    /// variable name (e.g. `"RUST_LOG"`). The host resolves values from the
    /// process environment at load time.
    #[serde(default)]
    pub env_vars: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct AuthoritiesSection {
    #[serde(default)]
    pub host: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
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

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct ViewSection {
    #[serde(default)]
    pub deps: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
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

pub const CURRENT_MANIFEST_VERSION: u32 = 2;
pub const HOST_ABI_VERSION: &str = "2.0.0";

impl PluginManifest {
    pub fn parse(toml_str: &str) -> Result<Self, ManifestError> {
        Ok(toml::from_str(toml_str)?)
    }

    pub fn validate(&self) -> Result<(), ManifestError> {
        if !abi_compatible(&self.plugin.abi_version, HOST_ABI_VERSION) {
            return Err(ManifestError::AbiMismatch {
                manifest: self.plugin.abi_version.clone(),
                host: HOST_ABI_VERSION.to_string(),
            });
        }

        let mut errors = Vec::new();

        if let Some(v) = self.manifest_version
            && v > CURRENT_MANIFEST_VERSION
        {
            errors.push(ManifestError::UnsupportedManifestVersion {
                found: v,
                max_supported: CURRENT_MANIFEST_VERSION,
            });
        }

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

    pub fn wasi_capabilities(&self) -> &[String] {
        &self.capabilities.wasi
    }

    pub fn host_authorities(&self) -> &[String] {
        &self.authorities.host
    }

    pub fn resolve_setting_defaults(&self) -> HashMap<String, SettingValue> {
        self.settings
            .iter()
            .filter_map(|(key, schema)| {
                toml_value_to_setting(&schema.setting_type, &schema.default)
                    .map(|v| (key.clone(), v))
            })
            .collect()
    }

    pub fn validate_config_settings(
        &self,
        config: &HashMap<String, SettingValue>,
    ) -> (HashMap<String, SettingValue>, Vec<String>) {
        let mut valid = HashMap::new();
        let mut warnings = Vec::new();

        for (key, value) in config {
            let Some(schema) = self.settings.get(key) else {
                warnings.push(format!("unknown setting key `{key}`"));
                continue;
            };
            if setting_type_matches(&schema.setting_type, value) {
                valid.insert(key.clone(), value.clone());
            } else {
                warnings.push(format!(
                    "setting `{key}`: expected {}, got {}",
                    schema.setting_type,
                    setting_value_type_name(value)
                ));
            }
        }

        for (key, default) in self.resolve_setting_defaults() {
            valid.entry(key).or_insert(default);
        }

        (valid, warnings)
    }

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

pub fn abi_compatible(manifest_version: &str, host_version: &str) -> bool {
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

pub fn capability_from_name(name: &str) -> Option<&'static str> {
    match name {
        "filesystem" => Some("filesystem"),
        "environment" => Some("environment"),
        "monotonic-clock" => Some("monotonic-clock"),
        "process" => Some("process"),
        _ => None,
    }
}

pub fn authority_from_name(name: &str) -> Option<PluginAuthorities> {
    match name {
        "dynamic-surface" => Some(PluginAuthorities::DYNAMIC_SURFACE),
        "pty-process" => Some(PluginAuthorities::PTY_PROCESS),
        "workspace-management" => Some(PluginAuthorities::WORKSPACE),
        _ => None,
    }
}

pub fn handler_flag_bit(name: &str) -> Option<u32> {
    match name {
        "overlay" => Some(1 << 2),
        "menu-transform" => Some(1 << 5),
        "cursor-style" => Some(1 << 6),
        "input-handler" => Some(1 << 7),
        "surface-provider" => Some(1 << 11),
        "workspace-observer" => Some(1 << 12),
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

fn setting_type_matches(expected_type: &str, value: &SettingValue) -> bool {
    matches!(
        (expected_type, value),
        ("bool", SettingValue::Bool(_))
            | ("integer", SettingValue::Integer(_))
            | ("float", SettingValue::Float(_))
            | ("string", SettingValue::Str(_))
    )
}

fn setting_value_type_name(value: &SettingValue) -> &'static str {
    match value {
        SettingValue::Bool(_) => "bool",
        SettingValue::Integer(_) => "integer",
        SettingValue::Float(_) => "float",
        SettingValue::Str(_) => "string",
    }
}

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
abi_version = "2.0.0"
"#;

    #[test]
    fn parse_and_validate_minimal_manifest() {
        let manifest = PluginManifest::parse(MINIMAL_MANIFEST).unwrap();
        assert_eq!(manifest.plugin.id, "test_plugin");
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn validate_accumulates_multiple_errors() {
        let toml = r#"
[plugin]
id = "test"
abi_version = "2.0.0"

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
            ManifestError::Multiple(errors) => assert_eq!(errors.len(), 3),
            _ => panic!("expected multiple validation errors, got: {err}"),
        }
    }
}
