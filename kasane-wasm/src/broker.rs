//! ADR-052 `CapabilityBroker` — authority gate for WIT capability
//! resource acquisition.
//!
//! The broker enforces manifest-declared service capability bounds at
//! `open-*` time. A plugin that has not declared a service in its
//! `[[capabilities.services]]` manifest section cannot acquire the
//! corresponding handle: the host returns `open-error::denied` before
//! the resource is ever pushed to the table.
//!
//! Per ADR-052 §Rationale item 2 the check happens at *acquisition*,
//! not on every method call — a plugin that opens 1000 handles pays
//! the broker cost once. The hot path (`get-lines-text` and friends)
//! sees only a table-lookup overhead.
//!
//! Chunks 4+ extend the broker with ADR-056 attenuation: an
//! attenuated handle carries the predicate, and `open-*` succeeds
//! only when the requested scope is within both the manifest-declared
//! bound and any active attenuation.

use std::collections::HashSet;

use kasane_plugin_package::manifest::PluginManifest;

/// Per-plugin authority gate for ADR-052 service capability handles.
///
/// Constructed once per plugin instance from the validated manifest
/// and carried on `HostState`. Every `open-*` host function consults
/// the broker before allocating a handle.
#[derive(Debug, Default, Clone)]
pub struct CapabilityBroker {
    declared_services: HashSet<String>,
}

impl CapabilityBroker {
    /// Build a broker from a validated plugin manifest. The manifest's
    /// `[[capabilities.services]]` declarations form the upper bound
    /// of authority — services not listed here cannot be acquired.
    pub fn from_manifest(manifest: &PluginManifest) -> Self {
        let declared_services = manifest
            .service_capabilities()
            .iter()
            .map(|s| s.name.clone())
            .collect();
        Self { declared_services }
    }

    /// Returns `true` iff the plugin's manifest declared the named
    /// service capability and acquisition should be permitted.
    pub fn allows_service(&self, name: &str) -> bool {
        self.declared_services.contains(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kasane_plugin_package::manifest::{
        CapabilitiesSection, PluginManifest, PluginSection, ServiceDeclaration,
    };

    fn manifest_with_services(services: Vec<&str>) -> PluginManifest {
        PluginManifest {
            manifest_version: None,
            plugin: PluginSection {
                id: "test".into(),
                abi_version: "6.5.0".into(),
            },
            capabilities: CapabilitiesSection {
                wasi: Vec::new(),
                env_vars: Vec::new(),
                services: services
                    .into_iter()
                    .map(|n| ServiceDeclaration { name: n.into() })
                    .collect(),
            },
            authorities: Default::default(),
            handlers: Default::default(),
            view: Default::default(),
            settings: Default::default(),
        }
    }

    #[test]
    fn declared_service_is_allowed() {
        let m = manifest_with_services(vec!["buffer"]);
        let broker = CapabilityBroker::from_manifest(&m);
        assert!(broker.allows_service("buffer"));
    }

    #[test]
    fn undeclared_service_is_denied() {
        let m = manifest_with_services(vec![]);
        let broker = CapabilityBroker::from_manifest(&m);
        assert!(!broker.allows_service("buffer"));
    }

    #[test]
    fn declared_service_does_not_grant_other_services() {
        let m = manifest_with_services(vec!["buffer"]);
        let broker = CapabilityBroker::from_manifest(&m);
        assert!(!broker.allows_service("display"));
        assert!(!broker.allows_service("git"));
    }
}
