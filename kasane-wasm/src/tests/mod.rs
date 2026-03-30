use kasane_core::config::PluginsConfig;
use kasane_core::element::{Direction, Element, OverlayAnchor};
use kasane_core::input::{Key, KeyEvent, Modifiers, MouseButton, MouseEvent, MouseEventKind};
use kasane_core::layout::Rect;
use kasane_core::plugin::{
    AnnotateContext, AppView, Command, ContribSizeHint, ContributeContext, Contribution, IoEvent,
    OverlayContext, PluginBackend, PluginId, PluginRuntime, ProcessEvent, SlotId,
};
use kasane_core::protocol::Color;
use kasane_core::state::{AppState, DirtyFlags};
use kasane_core::surface::{
    ResolvedSlotContentKind, SlotKind, SurfaceEvent, SurfacePlacementRequest, SurfaceRegistry,
    ViewContext,
};
use kasane_core::workspace::DockPosition;
use kasane_core::workspace::Workspace;

use crate::WasmPluginLoader;

mod bulk_buffer;
mod color_preview;
mod cursor_line;
mod discovery;
mod fuzzy_finder;
mod prompt_highlight;
mod session_ui;
mod smooth_scroll;
mod surface_probe;

fn load_cursor_line_plugin() -> crate::WasmPlugin {
    let loader = WasmPluginLoader::new().expect("failed to create loader");
    let bytes = crate::load_wasm_fixture("cursor-line.wasm").expect("failed to load fixture");
    loader
        .load(&bytes, &crate::WasiCapabilityConfig::default())
        .expect("failed to load plugin")
}

fn load_prompt_highlight_plugin() -> crate::WasmPlugin {
    let loader = WasmPluginLoader::new().expect("failed to create loader");
    let bytes = crate::load_wasm_fixture("prompt-highlight.wasm").expect("failed to load fixture");
    loader
        .load(&bytes, &crate::WasiCapabilityConfig::default())
        .expect("failed to load plugin")
}

fn load_session_ui_plugin() -> crate::WasmPlugin {
    let loader = WasmPluginLoader::new().expect("failed to create loader");
    let bytes = crate::load_wasm_fixture("session-ui.wasm").expect("failed to load fixture");
    loader
        .load(&bytes, &crate::WasiCapabilityConfig::default())
        .expect("failed to load plugin")
}

fn load_surface_probe_plugin() -> crate::WasmPlugin {
    load_surface_probe_plugin_with_config(&crate::WasiCapabilityConfig::default())
}

fn load_surface_probe_plugin_with_config(
    config: &crate::WasiCapabilityConfig,
) -> crate::WasmPlugin {
    let loader = WasmPluginLoader::new().expect("failed to create loader");
    let bytes = crate::load_wasm_fixture("surface-probe.wasm").expect("failed to load fixture");
    loader.load(&bytes, config).expect("failed to load plugin")
}

fn load_smooth_scroll_plugin() -> crate::WasmPlugin {
    let loader = WasmPluginLoader::new().expect("failed to create loader");
    let bytes = crate::load_wasm_fixture("smooth-scroll.wasm").expect("failed to load fixture");
    loader
        .load(&bytes, &crate::WasiCapabilityConfig::default())
        .expect("failed to load plugin")
}

fn load_fixture_manifest(name: &str) -> crate::manifest::PluginManifest {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name);
    let toml_str = std::fs::read_to_string(path).expect("failed to read fixture manifest");
    let manifest = crate::manifest::PluginManifest::parse(&toml_str).expect("failed to parse");
    manifest.validate().expect("manifest validation failed");
    manifest
}

fn load_cursor_line_with_manifest() -> crate::WasmPlugin {
    let loader = WasmPluginLoader::new().expect("failed to create loader");
    let bytes = crate::load_wasm_fixture("cursor-line.wasm").expect("failed to load fixture");
    let manifest = load_fixture_manifest("cursor-line.toml");
    loader
        .load_with_manifest(&bytes, &manifest, &crate::WasiCapabilityConfig::default())
        .map_err(|(_, e)| e)
        .expect("failed to load plugin with manifest")
}

fn load_prompt_highlight_with_manifest() -> crate::WasmPlugin {
    let loader = WasmPluginLoader::new().expect("failed to create loader");
    let bytes = crate::load_wasm_fixture("prompt-highlight.wasm").expect("failed to load fixture");
    let manifest = load_fixture_manifest("prompt-highlight.toml");
    loader
        .load_with_manifest(&bytes, &manifest, &crate::WasiCapabilityConfig::default())
        .map_err(|(_, e)| e)
        .expect("failed to load plugin with manifest")
}

fn load_fuzzy_finder_with_manifest() -> crate::WasmPlugin {
    let loader = WasmPluginLoader::new().expect("failed to create loader");
    let bytes = crate::load_wasm_fixture("fuzzy-finder.wasm").expect("failed to load fixture");
    let manifest = load_fixture_manifest("fuzzy-finder.toml");
    loader
        .load_with_manifest(&bytes, &manifest, &crate::WasiCapabilityConfig::default())
        .map_err(|(_, e)| e)
        .expect("failed to load plugin with manifest")
}

fn default_annotate_ctx() -> AnnotateContext {
    AnnotateContext {
        line_width: 80,
        gutter_width: 0,
        display_map: None,
        pane_surface_id: None,
        pane_focused: true,
    }
}

fn default_contribute_ctx(state: &AppState) -> ContributeContext {
    ContributeContext::new(&AppView::new(state), None)
}

fn default_overlay_ctx() -> OverlayContext {
    OverlayContext {
        screen_cols: 80,
        screen_rows: 24,
        menu_rect: None,
        existing_overlays: vec![],
        focused_surface_id: None,
    }
}

fn default_surface_rect() -> Rect {
    Rect {
        x: 2,
        y: 3,
        w: 30,
        h: 8,
    }
}

struct SurfaceProbeContributor;

impl PluginBackend for SurfaceProbeContributor {
    fn id(&self) -> PluginId {
        PluginId("surface_probe_contributor".to_string())
    }

    fn contribute_to(
        &self,
        region: &SlotId,
        _state: &AppView<'_>,
        ctx: &ContributeContext,
    ) -> Option<Contribution> {
        if region.as_str() != "surface_probe.sidebar.top" {
            return None;
        }

        Some(Contribution {
            element: Element::text(
                format!(
                    "slot-fill:{}x{}",
                    ctx.min_width,
                    ctx.max_width.unwrap_or_default()
                ),
                Default::default(),
            ),
            priority: 0,
            size_hint: ContribSizeHint::Auto,
        })
    }
}

fn make_state_with_lines(lines: &[&str]) -> AppState {
    use kasane_core::protocol::{Atom, Face};
    let mut state = AppState::default();
    state.lines = lines
        .iter()
        .map(|s| {
            vec![Atom {
                face: Face::default(),
                contents: (*s).into(),
            }]
        })
        .collect();
    state.lines_dirty = vec![true; lines.len()];
    state
}

// ---- Manifest-path test variants ----

#[test]
fn cursor_line_with_manifest_id() {
    let plugin = load_cursor_line_with_manifest();
    assert_eq!(plugin.id().0, "cursor_line");
}

#[test]
fn prompt_highlight_with_manifest_id() {
    let plugin = load_prompt_highlight_with_manifest();
    assert_eq!(plugin.id().0, "prompt_highlight");
}

#[test]
fn fuzzy_finder_with_manifest_id() {
    let plugin = load_fuzzy_finder_with_manifest();
    assert_eq!(plugin.id().0, "fuzzy_finder");
}

#[test]
fn manifest_wasm_id_mismatch_detected() {
    let loader = WasmPluginLoader::new().expect("failed to create loader");
    let bytes = crate::load_wasm_fixture("cursor-line.wasm").expect("failed to load fixture");
    // Use a manifest with a different plugin ID
    let toml = r#"
[plugin]
id = "wrong_id"
abi_version = "0.23.0"

[handlers]
flags = ["annotator"]
"#;
    let manifest = crate::manifest::PluginManifest::parse(toml).unwrap();
    let result =
        loader.load_with_manifest(&bytes, &manifest, &crate::WasiCapabilityConfig::default());
    let err_msg = match result {
        Err((_, e)) => e.to_string(),
        Ok(_) => panic!("expected error for ID mismatch"),
    };
    assert!(
        err_msg.contains("mismatch"),
        "expected mismatch error, got: {err_msg}"
    );
}

#[test]
fn fingerprint_includes_manifest_mtime() {
    let fp1 = crate::WasmPluginFingerprint::Filesystem {
        len: 100,
        modified_ns: Some(1000),
        manifest_modified_ns: Some(2000),
    };
    let fp2 = crate::WasmPluginFingerprint::Filesystem {
        len: 100,
        modified_ns: Some(1000),
        manifest_modified_ns: Some(3000),
    };
    assert_ne!(fp1, fp2);
}

#[test]
fn fingerprint_same_when_all_fields_match() {
    let fp1 = crate::WasmPluginFingerprint::Filesystem {
        len: 100,
        modified_ns: Some(1000),
        manifest_modified_ns: Some(2000),
    };
    let fp2 = crate::WasmPluginFingerprint::Filesystem {
        len: 100,
        modified_ns: Some(1000),
        manifest_modified_ns: Some(2000),
    };
    assert_eq!(fp1, fp2);
}

#[test]
fn second_loader_hits_cache() {
    let tmp = tempfile::tempdir().unwrap();
    let bytes = crate::load_wasm_fixture("cursor-line.wasm").unwrap();
    let wasi = crate::WasiCapabilityConfig::default();

    // First loader: compiles and caches
    let loader1 = WasmPluginLoader::new_with_cache_base(tmp.path()).expect("loader1");
    let plugin1 = loader1.load(&bytes, &wasi).expect("load1");

    // Second loader: new Engine, should hit cache
    let loader2 = WasmPluginLoader::new_with_cache_base(tmp.path()).expect("loader2");
    let plugin2 = loader2.load(&bytes, &wasi).expect("load2");

    assert_eq!(plugin1.id(), plugin2.id());
}

#[test]
fn factory_create_hits_cache() {
    let tmp = tempfile::tempdir().unwrap();
    let bytes = crate::load_wasm_fixture("cursor-line.wasm").unwrap();
    let manifest = load_fixture_manifest("cursor-line.toml");
    let wasi = crate::WasiCapabilityConfig::default();

    // Simulate collect path: load with manifest (populates cache)
    let loader1 = WasmPluginLoader::new_with_cache_base(tmp.path()).expect("loader1");
    let plugin1 = loader1
        .load_with_manifest(&bytes, &manifest, &wasi)
        .map_err(|(_, e)| e)
        .expect("collect load");

    // Simulate factory.create() path: new loader, same wasm bytes
    let loader2 = WasmPluginLoader::new_with_cache_base(tmp.path()).expect("loader2");
    let plugin2 = loader2
        .load_with_manifest(&bytes, &manifest, &wasi)
        .map_err(|(_, e)| e)
        .expect("factory load");

    assert_eq!(plugin1.id(), plugin2.id());
}
