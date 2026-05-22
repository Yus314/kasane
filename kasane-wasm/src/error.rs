#[derive(Debug, thiserror::Error)]
pub enum WasmPluginError {
    #[error("engine initialization failed: {0}")]
    EngineInit(#[source] anyhow::Error),

    #[error("component loading failed: {0}")]
    ComponentLoad(#[source] anyhow::Error),

    #[error("instantiation failed: {0}")]
    Instantiate(#[source] anyhow::Error),

    /// Manifest declares a WIT package version the host cannot satisfy.
    /// Detected before wasmtime instantiation so the user sees a friendly
    /// message instead of the linker's "matching implementation not
    /// found" error.
    #[error(
        "plugin requires kasane:plugin@{required} but host provides @{host}; \
         rebuild with a compatible kasane-plugin-sdk"
    )]
    AbiVersionMismatch { required: String, host: String },

    #[error(
        "manifest-WASM ID mismatch: manifest declares `{manifest_id}`, WASM reports `{wasm_id}`"
    )]
    IdMismatch {
        manifest_id: String,
        wasm_id: String,
    },

    #[error("WASI context build failed: {0}")]
    WasiContext(#[source] anyhow::Error),

    #[error("unknown bundled plugin: `{0}`")]
    UnknownBundledPlugin(String),

    #[error("package error: {0}")]
    Package(#[from] kasane_plugin_package::package::PackageError),

    #[error("manifest error: {0}")]
    Manifest(#[from] kasane_plugin_package::manifest::ManifestError),

    /// ADR-052 chunk E: the WASM component imports a capability-resource
    /// interface (e.g. `kasane:plugin/host-capabilities`) but the
    /// plugin manifest does not declare the matching service. The
    /// broker would deny `open-*` at runtime; the load-time scan
    /// surfaces this as a manifest bug before instantiation.
    #[error(
        "plugin imports capability interface but manifest is missing \
         `[[capabilities.services]] name = \"{service}\"`; either \
         declare the capability or remove the import"
    )]
    UndeclaredCapabilityImport { service: String },

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

/// `wasmtime::Error` is a distinct type (not a re-export of `anyhow::Error`)
/// but is `Into<anyhow::Error>`; bridge it explicitly so `?` on wasmtime
/// calls works against `Result<_, WasmPluginError>`.
impl From<wasmtime::Error> for WasmPluginError {
    fn from(err: wasmtime::Error) -> Self {
        WasmPluginError::Other(err.into())
    }
}
