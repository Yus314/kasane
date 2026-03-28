//! Kasane authority resolution for WASM plugins.
//!
//! Plugins declare their required authorities via `requested-authorities` (WIT export).
//! The host resolves these against user configuration (`deny_authorities` in config.toml)
//! and stores the granted set on the adapted `PluginBackend`.

use kasane_core::plugin::PluginAuthorities;

use crate::bindings::kasane::plugin::types::PluginAuthority;
use crate::capability::WasiCapabilityConfig;

fn authority_name(authority: PluginAuthority) -> &'static str {
    match authority {
        PluginAuthority::DynamicSurface => "dynamic-surface",
        PluginAuthority::PtyProcess => "pty-process",
        PluginAuthority::WorkspaceManagement => "workspace-management",
    }
}

/// Resolve requested plugin authorities against user configuration.
pub fn resolve_authorities(
    plugin_id: &str,
    requested: &[PluginAuthority],
    config: &WasiCapabilityConfig,
) -> PluginAuthorities {
    let denied = config
        .deny_authorities
        .get(plugin_id)
        .map(|v| v.as_slice())
        .unwrap_or_default();

    let mut resolved = PluginAuthorities::empty();
    for authority in requested.iter().copied() {
        let name = authority_name(authority);
        if denied.iter().any(|d| d == name) {
            tracing::info!(
                plugin = plugin_id,
                authority = name,
                "plugin authority denied by configuration"
            );
            continue;
        }

        match authority {
            PluginAuthority::DynamicSurface => {
                resolved |= PluginAuthorities::DYNAMIC_SURFACE;
            }
            PluginAuthority::PtyProcess => {
                resolved |= PluginAuthorities::PTY_PROCESS;
            }
            PluginAuthority::WorkspaceManagement => {
                resolved |= PluginAuthorities::WORKSPACE;
            }
        }

        tracing::info!(
            plugin = plugin_id,
            authority = name,
            "granted plugin authority"
        );
    }

    resolved
}

/// Resolve authorities from manifest-declared string names.
///
/// Converts string names to `PluginAuthority` enum values, then delegates
/// to [`resolve_authorities`]. Unknown names are silently skipped (they
/// should have been caught by manifest validation).
pub fn resolve_authorities_from_manifest(
    plugin_id: &str,
    manifest_auths: &[String],
    config: &WasiCapabilityConfig,
) -> PluginAuthorities {
    let authorities: Vec<PluginAuthority> = manifest_auths
        .iter()
        .filter_map(|name| match name.as_str() {
            "dynamic-surface" => Some(PluginAuthority::DynamicSurface),
            "pty-process" => Some(PluginAuthority::PtyProcess),
            "workspace-management" => Some(PluginAuthority::WorkspaceManagement),
            _ => None,
        })
        .collect();
    resolve_authorities(plugin_id, &authorities, config)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn resolve_authorities_grants_requested_bits() {
        let config = WasiCapabilityConfig::default();
        let resolved = resolve_authorities(
            "surface_probe",
            &[PluginAuthority::DynamicSurface, PluginAuthority::PtyProcess],
            &config,
        );
        assert!(resolved.contains(PluginAuthorities::DYNAMIC_SURFACE));
        assert!(resolved.contains(PluginAuthorities::PTY_PROCESS));
    }

    #[test]
    fn resolve_authorities_honors_denials() {
        let config = WasiCapabilityConfig {
            deny_authorities: HashMap::from([(
                "surface_probe".to_string(),
                vec!["dynamic-surface".to_string()],
            )]),
            ..Default::default()
        };
        let resolved = resolve_authorities(
            "surface_probe",
            &[PluginAuthority::DynamicSurface, PluginAuthority::PtyProcess],
            &config,
        );
        assert!(!resolved.contains(PluginAuthorities::DYNAMIC_SURFACE));
        assert!(resolved.contains(PluginAuthorities::PTY_PROCESS));
    }
}
