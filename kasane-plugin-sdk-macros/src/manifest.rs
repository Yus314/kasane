// ---------------------------------------------------------------------------
// Compile-time manifest types (subset of kasane-wasm's PluginManifest)
// ---------------------------------------------------------------------------

use serde::Deserialize;

/// Manifest schema for compile-time validation in `define_plugin!`.
#[derive(Debug, Deserialize)]
pub(crate) struct CompileTimeManifest {
    pub(crate) plugin: ManifestPlugin,
    #[serde(default)]
    pub(crate) capabilities: ManifestCapabilities,
    #[serde(default)]
    pub(crate) authorities: ManifestAuthorities,
    #[serde(default)]
    pub(crate) view: ManifestView,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ManifestPlugin {
    pub(crate) id: String,
    #[allow(dead_code)]
    pub(crate) abi_version: String,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct ManifestCapabilities {
    #[serde(default)]
    pub(crate) wasi: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct ManifestAuthorities {
    #[serde(default)]
    pub(crate) host: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct ManifestView {
    #[serde(default)]
    pub(crate) deps: Vec<String>,
}

/// Map WASI capability name to the WIT enum variant path for codegen.
pub(crate) fn wasi_capability_variant(name: &str) -> Option<&'static str> {
    match name {
        "filesystem" => Some("Capability::Filesystem"),
        "environment" => Some("Capability::Environment"),
        "monotonic-clock" => Some("Capability::MonotonicClock"),
        "process" => Some("Capability::Process"),
        _ => None,
    }
}

/// Map host authority name to the WIT enum variant path for codegen.
pub(crate) fn host_authority_variant(name: &str) -> Option<&'static str> {
    match name {
        "dynamic-surface" => Some("PluginAuthority::DynamicSurface"),
        "pty-process" => Some("PluginAuthority::PtyProcess"),
        "workspace-management" => Some("PluginAuthority::WorkspaceManagement"),
        _ => None,
    }
}

/// Map view dep name to its bit value.
// NOTE: kasane-wasm/src/manifest.rs has a `view_dep_bit` function that must stay in sync.
// proc-macro crates cannot share code with runtime crates.
pub(crate) fn compile_time_view_dep_bit(name: &str) -> Option<u16> {
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

/// Resolved manifest data from compile-time TOML parsing.
pub(crate) struct ManifestDef {
    /// The plugin ID from `[plugin].id`.
    pub(crate) id: String,
    /// Capability variant tokens (e.g. `Capability::Process`).
    pub(crate) capability_variants: Vec<proc_macro2::TokenStream>,
    /// Authority variant tokens (e.g. `PluginAuthority::PtyProcess`).
    pub(crate) authority_variants: Vec<proc_macro2::TokenStream>,
    /// Pre-computed view_deps bitmask.
    pub(crate) view_deps_mask: u16,
    /// Whether view.deps was non-empty (use mask) vs empty (use ALL default).
    pub(crate) has_view_deps: bool,
}

/// Read and parse a manifest TOML file at compile time.
///
/// The path is resolved relative to `CARGO_MANIFEST_DIR` (the consuming crate's root).
pub(crate) fn parse_manifest_at_compile_time(path_lit: &syn::LitStr) -> syn::Result<ManifestDef> {
    let rel_path = path_lit.value();
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").map_err(|_| {
        syn::Error::new(
            path_lit.span(),
            "CARGO_MANIFEST_DIR not set — cannot resolve manifest path",
        )
    })?;
    let full_path = std::path::Path::new(&manifest_dir).join(&rel_path);

    let toml_str = std::fs::read_to_string(&full_path).map_err(|e| {
        syn::Error::new(
            path_lit.span(),
            format!("failed to read manifest at {}: {e}", full_path.display()),
        )
    })?;

    let manifest: CompileTimeManifest = toml::from_str(&toml_str).map_err(|e| {
        syn::Error::new(
            path_lit.span(),
            format!("failed to parse manifest TOML: {e}"),
        )
    })?;

    // Convert capability names to WIT variant tokens
    let mut capability_variants = Vec::new();
    for name in &manifest.capabilities.wasi {
        let variant = wasi_capability_variant(name).ok_or_else(|| {
            syn::Error::new(
                path_lit.span(),
                format!("unknown WASI capability in manifest: `{name}`"),
            )
        })?;
        let tokens: proc_macro2::TokenStream = variant.parse().unwrap();
        capability_variants.push(tokens);
    }

    // Convert authority names to WIT variant tokens
    let mut authority_variants = Vec::new();
    for name in &manifest.authorities.host {
        let variant = host_authority_variant(name).ok_or_else(|| {
            syn::Error::new(
                path_lit.span(),
                format!("unknown host authority in manifest: `{name}`"),
            )
        })?;
        let tokens: proc_macro2::TokenStream = variant.parse().unwrap();
        authority_variants.push(tokens);
    }

    // Compute view_deps bitmask
    let has_view_deps = !manifest.view.deps.is_empty();
    let mut view_deps_mask: u16 = 0;
    for name in &manifest.view.deps {
        let bit = compile_time_view_dep_bit(name).ok_or_else(|| {
            syn::Error::new(
                path_lit.span(),
                format!("unknown view dep in manifest: `{name}`"),
            )
        })?;
        view_deps_mask |= bit;
    }

    Ok(ManifestDef {
        id: manifest.plugin.id,
        capability_variants,
        authority_variants,
        view_deps_mask,
        has_view_deps,
    })
}
