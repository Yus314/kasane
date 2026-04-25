#[derive(Debug, thiserror::Error)]
pub enum WasmPluginError {
    #[error("engine initialization failed: {0}")]
    EngineInit(#[source] anyhow::Error),

    #[error("component loading failed: {0}")]
    ComponentLoad(#[source] anyhow::Error),

    #[error("instantiation failed: {0}")]
    Instantiate(#[source] anyhow::Error),

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

    #[error("{0}")]
    Other(#[source] anyhow::Error),
}
