use crate::surface::SurfaceRegistrationError;

use super::{PluginDescriptor, PluginId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginDiagnosticSeverity {
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderArtifactStage {
    Read,
    Load,
    Instantiate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginDiagnosticTarget {
    Plugin(PluginId),
    Provider(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginDiagnosticKind {
    SurfaceRegistrationFailed {
        reason: SurfaceRegistrationError,
    },
    InstantiationFailed,
    ProviderCollectFailed,
    ProviderArtifactFailed {
        artifact: String,
        stage: ProviderArtifactStage,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginDiagnostic {
    pub target: PluginDiagnosticTarget,
    pub kind: PluginDiagnosticKind,
    pub message: String,
    pub previous: Option<PluginDescriptor>,
    pub attempted: Option<PluginDescriptor>,
}

impl PluginDiagnostic {
    pub fn surface_registration_failed(
        plugin_id: PluginId,
        reason: SurfaceRegistrationError,
    ) -> Self {
        Self {
            target: PluginDiagnosticTarget::Plugin(plugin_id),
            message: format!("{reason:?}"),
            kind: PluginDiagnosticKind::SurfaceRegistrationFailed { reason },
            previous: None,
            attempted: None,
        }
    }

    pub fn instantiation_failed(plugin_id: PluginId, message: impl Into<String>) -> Self {
        Self {
            target: PluginDiagnosticTarget::Plugin(plugin_id),
            message: message.into(),
            kind: PluginDiagnosticKind::InstantiationFailed,
            previous: None,
            attempted: None,
        }
    }

    pub fn provider_collect_failed(
        provider: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            target: PluginDiagnosticTarget::Provider(provider.into()),
            message: message.into(),
            kind: PluginDiagnosticKind::ProviderCollectFailed,
            previous: None,
            attempted: None,
        }
    }

    pub fn provider_artifact_failed(
        provider: impl Into<String>,
        artifact: impl Into<String>,
        stage: ProviderArtifactStage,
        message: impl Into<String>,
    ) -> Self {
        Self {
            target: PluginDiagnosticTarget::Provider(provider.into()),
            message: message.into(),
            kind: PluginDiagnosticKind::ProviderArtifactFailed {
                artifact: artifact.into(),
                stage,
            },
            previous: None,
            attempted: None,
        }
    }

    pub fn plugin_id(&self) -> Option<&PluginId> {
        match &self.target {
            PluginDiagnosticTarget::Plugin(plugin_id) => Some(plugin_id),
            PluginDiagnosticTarget::Provider(_) => None,
        }
    }

    pub fn provider_name(&self) -> Option<&str> {
        match &self.target {
            PluginDiagnosticTarget::Plugin(_) => None,
            PluginDiagnosticTarget::Provider(provider) => Some(provider.as_str()),
        }
    }

    pub fn severity(&self) -> PluginDiagnosticSeverity {
        match self.kind {
            PluginDiagnosticKind::SurfaceRegistrationFailed { .. }
            | PluginDiagnosticKind::InstantiationFailed
            | PluginDiagnosticKind::ProviderCollectFailed => PluginDiagnosticSeverity::Error,
            PluginDiagnosticKind::ProviderArtifactFailed { .. } => {
                PluginDiagnosticSeverity::Warning
            }
        }
    }
}

pub fn report_plugin_diagnostics(diagnostics: &[PluginDiagnostic]) {
    for diagnostic in diagnostics {
        let plugin_id = diagnostic.plugin_id().map(|plugin_id| plugin_id.0.as_str());
        let provider = diagnostic.provider_name();
        let severity = diagnostic.severity();
        match diagnostic.kind {
            PluginDiagnosticKind::SurfaceRegistrationFailed { ref reason } => {
                match severity {
                    PluginDiagnosticSeverity::Warning => tracing::warn!(
                        plugin_id = plugin_id.unwrap_or("none"),
                        provider = provider.unwrap_or("none"),
                        kind = "surface_registration_failed",
                        reason = ?reason,
                        message = %diagnostic.message,
                        previous_source = ?diagnostic.previous.as_ref().map(|descriptor| &descriptor.source),
                        previous_revision = diagnostic.previous.as_ref().map(|descriptor| descriptor.revision.0.as_str()).unwrap_or("none"),
                        attempted_source = ?diagnostic.attempted.as_ref().map(|descriptor| &descriptor.source),
                        attempted_revision = diagnostic.attempted.as_ref().map(|descriptor| descriptor.revision.0.as_str()).unwrap_or("none"),
                        "plugin activation failed"
                    ),
                    PluginDiagnosticSeverity::Error => tracing::error!(
                        plugin_id = plugin_id.unwrap_or("none"),
                        provider = provider.unwrap_or("none"),
                        kind = "surface_registration_failed",
                        reason = ?reason,
                        message = %diagnostic.message,
                        previous_source = ?diagnostic.previous.as_ref().map(|descriptor| &descriptor.source),
                        previous_revision = diagnostic.previous.as_ref().map(|descriptor| descriptor.revision.0.as_str()).unwrap_or("none"),
                        attempted_source = ?diagnostic.attempted.as_ref().map(|descriptor| &descriptor.source),
                        attempted_revision = diagnostic.attempted.as_ref().map(|descriptor| descriptor.revision.0.as_str()).unwrap_or("none"),
                        "plugin activation failed"
                    ),
                };
            }
            PluginDiagnosticKind::InstantiationFailed => {
                match severity {
                    PluginDiagnosticSeverity::Warning => tracing::warn!(
                        plugin_id = plugin_id.unwrap_or("none"),
                        provider = provider.unwrap_or("none"),
                        kind = "instantiation_failed",
                        message = %diagnostic.message,
                        previous_source = ?diagnostic.previous.as_ref().map(|descriptor| &descriptor.source),
                        previous_revision = diagnostic.previous.as_ref().map(|descriptor| descriptor.revision.0.as_str()).unwrap_or("none"),
                        attempted_source = ?diagnostic.attempted.as_ref().map(|descriptor| &descriptor.source),
                        attempted_revision = diagnostic.attempted.as_ref().map(|descriptor| descriptor.revision.0.as_str()).unwrap_or("none"),
                        "plugin activation failed"
                    ),
                    PluginDiagnosticSeverity::Error => tracing::error!(
                        plugin_id = plugin_id.unwrap_or("none"),
                        provider = provider.unwrap_or("none"),
                        kind = "instantiation_failed",
                        message = %diagnostic.message,
                        previous_source = ?diagnostic.previous.as_ref().map(|descriptor| &descriptor.source),
                        previous_revision = diagnostic.previous.as_ref().map(|descriptor| descriptor.revision.0.as_str()).unwrap_or("none"),
                        attempted_source = ?diagnostic.attempted.as_ref().map(|descriptor| &descriptor.source),
                        attempted_revision = diagnostic.attempted.as_ref().map(|descriptor| descriptor.revision.0.as_str()).unwrap_or("none"),
                        "plugin activation failed"
                    ),
                };
            }
            PluginDiagnosticKind::ProviderCollectFailed => {
                match severity {
                    PluginDiagnosticSeverity::Warning => tracing::warn!(
                        plugin_id = plugin_id.unwrap_or("none"),
                        provider = provider.unwrap_or("none"),
                        kind = "provider_collect_failed",
                        message = %diagnostic.message,
                        "plugin discovery failed"
                    ),
                    PluginDiagnosticSeverity::Error => tracing::error!(
                        plugin_id = plugin_id.unwrap_or("none"),
                        provider = provider.unwrap_or("none"),
                        kind = "provider_collect_failed",
                        message = %diagnostic.message,
                        "plugin discovery failed"
                    ),
                };
            }
            PluginDiagnosticKind::ProviderArtifactFailed {
                ref artifact,
                stage,
            } => {
                match severity {
                    PluginDiagnosticSeverity::Warning => tracing::warn!(
                        plugin_id = plugin_id.unwrap_or("none"),
                        provider = provider.unwrap_or("none"),
                        kind = "provider_artifact_failed",
                        artifact = %artifact,
                        stage = ?stage,
                        message = %diagnostic.message,
                        "plugin artifact preparation failed"
                    ),
                    PluginDiagnosticSeverity::Error => tracing::error!(
                        plugin_id = plugin_id.unwrap_or("none"),
                        provider = provider.unwrap_or("none"),
                        kind = "provider_artifact_failed",
                        artifact = %artifact,
                        stage = ?stage,
                        message = %diagnostic.message,
                        "plugin artifact preparation failed"
                    ),
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_artifact_failures_are_warnings() {
        let diagnostic = PluginDiagnostic::provider_artifact_failed(
            "test-provider",
            "broken.wasm",
            ProviderArtifactStage::Load,
            "bad artifact",
        );
        assert_eq!(diagnostic.severity(), PluginDiagnosticSeverity::Warning);
    }

    #[test]
    fn winner_activation_failures_are_errors() {
        let diagnostic =
            PluginDiagnostic::instantiation_failed(PluginId("test.plugin".to_string()), "boom");
        assert_eq!(diagnostic.severity(), PluginDiagnosticSeverity::Error);
    }

    #[test]
    fn provider_collect_failures_are_errors() {
        let diagnostic = PluginDiagnostic::provider_collect_failed("test-provider", "boom");
        assert_eq!(diagnostic.severity(), PluginDiagnosticSeverity::Error);
    }
}
