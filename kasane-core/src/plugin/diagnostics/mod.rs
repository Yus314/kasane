use std::borrow::Cow;

use crate::surface::SurfaceRegistrationError;

use super::{PluginDescriptor, PluginId};

mod painter;
mod scoring;
mod types;

#[cfg(test)]
mod tests;

pub use painter::*;
pub use scoring::diagnostic_overlay_lines;
pub use types::*;

/// Maximum visible lines in the overlay under normal conditions.
pub const DEFAULT_PLUGIN_DIAGNOSTIC_OVERLAY_LINES: usize = 3;
pub const PLUGIN_DIAGNOSTIC_OVERLAY_TITLE: &str = "plugin diagnostics";
pub const PLUGIN_ACTIVATION_OVERLAY_TITLE: &str = "plugin activation";
pub const PLUGIN_DISCOVERY_OVERLAY_TITLE: &str = "plugin discovery";
/// Expanded line limit when any provider error is present.
pub(super) const ERROR_PLUGIN_DIAGNOSTIC_OVERLAY_LINES: usize = 4;
/// Expanded line limit when a direct plugin activation error is present.
pub(super) const PLUGIN_ERROR_PLUGIN_DIAGNOSTIC_OVERLAY_LINES: usize = 5;
pub(super) const MIN_PLUGIN_DIAGNOSTIC_OVERLAY_COLS: u16 = 8;
pub(super) const MIN_PLUGIN_DIAGNOSTIC_OVERLAY_ROWS: u16 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginDiagnosticSeverity {
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderArtifactStage {
    Manifest,
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
    RuntimeError {
        method: String,
    },
    /// A configuration/validation error for a named item (e.g. widget parse error).
    /// Non-fatal: the item is skipped but the system continues.
    ConfigError {
        key: String,
    },
    /// A plugin contribution carried a primitive whose kind exceeds the
    /// active GPU backend's `BackendCapabilities` and the backend's
    /// `degradation_policy` was `Reject`. The contribution was dropped
    /// for the frame; subsequent frames continue dispatching the
    /// plugin normally — this is *not* a quarantine event (see
    /// ADR-033 for plugin failure semantics, which is a distinct
    /// contract).
    ///
    /// Emitted at most once per `(plugin_id, primitive_kind)` per
    /// session — the per-frame rejection check deduplicates so a
    /// plugin emitting the same path on every frame produces a single
    /// diagnostic, not one per frame. Per ADR-032 §Decision item 3.
    BackendCapabilityRejected {
        /// Short identifier for the rejected primitive ("path",
        /// "gradient", "blur", "DrawCanvas", etc.). Static-string-shaped
        /// to keep the diagnostic-emission path allocation-free.
        primitive_kind: &'static str,
        /// Backend that rejected (`"WgpuBackend"`, `"VelloBackend"`,
        /// future backends). Static-string-shaped for the same reason.
        backend: &'static str,
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

    pub fn runtime_error(
        plugin_id: PluginId,
        method: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            target: PluginDiagnosticTarget::Plugin(plugin_id),
            message: message.into(),
            kind: PluginDiagnosticKind::RuntimeError {
                method: method.into(),
            },
            previous: None,
            attempted: None,
        }
    }

    pub fn config_error(
        plugin_id: PluginId,
        key: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            target: PluginDiagnosticTarget::Plugin(plugin_id),
            message: message.into(),
            kind: PluginDiagnosticKind::ConfigError { key: key.into() },
            previous: None,
            attempted: None,
        }
    }

    /// Construct a `BackendCapabilityRejected` diagnostic. Used by the
    /// GPU backend layer when a plugin contribution carrying a
    /// primitive that exceeds `BackendCapabilities` is dropped under
    /// `DegradationPolicy::Reject`. See ADR-032 §Decision item 3.
    ///
    /// The caller is responsible for deduplicating
    /// `(plugin_id, primitive_kind)` across frames; this constructor
    /// simply builds the diagnostic value.
    pub fn backend_capability_rejected(
        plugin_id: PluginId,
        primitive_kind: &'static str,
        backend: &'static str,
    ) -> Self {
        Self {
            target: PluginDiagnosticTarget::Plugin(plugin_id),
            message: format!("{backend} cannot render {primitive_kind} (contribution dropped)"),
            kind: PluginDiagnosticKind::BackendCapabilityRejected {
                primitive_kind,
                backend,
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
            | PluginDiagnosticKind::ProviderCollectFailed
            | PluginDiagnosticKind::RuntimeError { .. } => PluginDiagnosticSeverity::Error,
            PluginDiagnosticKind::ProviderArtifactFailed { .. }
            | PluginDiagnosticKind::ConfigError { .. }
            | PluginDiagnosticKind::BackendCapabilityRejected { .. } => {
                PluginDiagnosticSeverity::Warning
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginDiagnosticOverlayLine {
    pub severity: PluginDiagnosticSeverity,
    pub tag_kind: PluginDiagnosticOverlayTagKind,
    pub text: String,
    pub repeat_count: usize,
}

impl PluginDiagnosticOverlayLine {
    pub fn display_text(&self) -> Cow<'_, str> {
        if self.repeat_count <= 1 {
            Cow::Borrowed(self.text.as_str())
        } else {
            Cow::Owned(format!("{} x{}", self.text, self.repeat_count))
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginDiagnosticOverlayTagKind {
    Activation,
    Discovery,
    ArtifactManifest,
    ArtifactRead,
    ArtifactLoad,
    ArtifactInstantiate,
    Runtime,
    Config,
}

pub fn summarize_plugin_diagnostic(diagnostic: &PluginDiagnostic) -> String {
    let target = diagnostic
        .plugin_id()
        .map(|id| id.0.as_str())
        .or_else(|| diagnostic.provider_name())
        .unwrap_or("unknown");

    match &diagnostic.kind {
        PluginDiagnosticKind::SurfaceRegistrationFailed { .. } => {
            format!("{target}: surface registration failed")
        }
        PluginDiagnosticKind::InstantiationFailed => {
            format!("{target}: {}", diagnostic.message)
        }
        PluginDiagnosticKind::ProviderCollectFailed => {
            format!("{target}: {}", diagnostic.message)
        }
        PluginDiagnosticKind::ProviderArtifactFailed { artifact, stage } => {
            format!(
                "{target}: {} {}",
                provider_artifact_stage_summary_label(*stage),
                provider_artifact_summary_name(artifact)
            )
        }
        PluginDiagnosticKind::RuntimeError { method } => {
            format!("{target}.{method}: {}", diagnostic.message)
        }
        PluginDiagnosticKind::ConfigError { key } => {
            format!("{key}: {}", diagnostic.message)
        }
        PluginDiagnosticKind::BackendCapabilityRejected {
            primitive_kind,
            backend,
        } => {
            format!("{target}: {backend} rejected {primitive_kind}")
        }
    }
}

pub fn provider_artifact_stage_label(stage: ProviderArtifactStage) -> &'static str {
    match stage {
        ProviderArtifactStage::Manifest => "manifest",
        ProviderArtifactStage::Read => "read",
        ProviderArtifactStage::Load => "load",
        ProviderArtifactStage::Instantiate => "instantiate",
    }
}

fn provider_artifact_stage_summary_label(stage: ProviderArtifactStage) -> &'static str {
    match stage {
        ProviderArtifactStage::Manifest => "manifest",
        ProviderArtifactStage::Read => "read",
        ProviderArtifactStage::Load => "load",
        ProviderArtifactStage::Instantiate => "init",
    }
}

fn provider_artifact_summary_name(artifact: &str) -> &str {
    std::path::Path::new(artifact)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(artifact)
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
            PluginDiagnosticKind::RuntimeError { ref method } => {
                tracing::error!(
                    plugin_id = plugin_id.unwrap_or("none"),
                    kind = "runtime_error",
                    method = %method,
                    message = %diagnostic.message,
                    "plugin runtime error"
                );
            }
            PluginDiagnosticKind::ConfigError { ref key } => {
                tracing::warn!(
                    plugin_id = plugin_id.unwrap_or("none"),
                    kind = "config_error",
                    key = %key,
                    message = %diagnostic.message,
                    "plugin config error"
                );
            }
            PluginDiagnosticKind::BackendCapabilityRejected {
                primitive_kind,
                backend,
            } => {
                tracing::warn!(
                    plugin_id = plugin_id.unwrap_or("none"),
                    kind = "backend_capability_rejected",
                    primitive_kind = %primitive_kind,
                    backend = %backend,
                    message = %diagnostic.message,
                    "plugin contribution dropped"
                );
            }
        }
    }
}
