use kasane_core::config::PluginsConfig;
use kasane_core::element::{Direction, Element, OverlayAnchor};
use kasane_core::input::{Key, KeyEvent, Modifiers, MouseButton, MouseEvent, MouseEventKind};
use kasane_core::layout::Rect;
use kasane_core::plugin::{
    AnnotateContext, Command, ContribSizeHint, ContributeContext, Contribution, IoEvent,
    OverlayContext, PluginBackend, PluginId, PluginRegistry, ProcessEvent, SlotId,
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
    let loader = WasmPluginLoader::new().expect("failed to create loader");
    let bytes = crate::load_wasm_fixture("surface-probe.wasm").expect("failed to load fixture");
    loader
        .load(&bytes, &crate::WasiCapabilityConfig::default())
        .expect("failed to load plugin")
}

fn load_smooth_scroll_plugin() -> crate::WasmPlugin {
    let loader = WasmPluginLoader::new().expect("failed to create loader");
    let bytes = crate::load_wasm_fixture("smooth-scroll.wasm").expect("failed to load fixture");
    loader
        .load(&bytes, &crate::WasiCapabilityConfig::default())
        .expect("failed to load plugin")
}

fn default_annotate_ctx() -> AnnotateContext {
    AnnotateContext {
        line_width: 80,
        gutter_width: 0,
        display_map: None,
    }
}

fn default_contribute_ctx(state: &AppState) -> ContributeContext {
    ContributeContext::new(state, None)
}

fn default_overlay_ctx() -> OverlayContext {
    OverlayContext {
        screen_cols: 80,
        screen_rows: 24,
        menu_rect: None,
        existing_overlays: vec![],
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
        _state: &AppState,
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
