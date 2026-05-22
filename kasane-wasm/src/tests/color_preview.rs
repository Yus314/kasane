use super::*;

fn apply_color_preview_state_change(
    plugin: &mut crate::WasmPlugin,
    state: &AppState,
    dirty: DirtyFlags,
) {
    let effects = plugin.on_state_changed_effects(&AppView::new(state), dirty);
    assert!(effects.redraw.is_empty());
    assert!(effects.commands.is_empty());
    assert!(effects.scroll_plans.is_empty());
}

fn load_color_preview_plugin() -> crate::WasmPlugin {
    let loader = WasmPluginLoader::new().expect("failed to create loader");
    let bytes = crate::load_wasm_fixture("color-preview.wasm").expect("failed to load fixture");
    // ADR-052 chunk 4: color-preview now declares a `buffer` service
    // capability and exercises `open-buffer-view` in `handle_mouse`.
    // The manifest path is the only one that initializes the host's
    // `CapabilityBroker` from the declared services — the broker-less
    // `load(...)` path leaves the broker empty (default policy: deny
    // every service) which would block `open-buffer-view`.
    let manifest = load_fixture_manifest("color-preview.toml");
    loader
        .load_with_manifest(&bytes, &manifest, &crate::WasiCapabilityConfig::default())
        .map_err(|(_, e)| e)
        .expect("failed to load plugin with manifest")
}

#[test]
fn plugin_id() {
    let plugin = load_color_preview_plugin();
    assert_eq!(plugin.id().as_str(), "color_preview");
}

/// ADR-052 chunk E: a manifest that omits `[[capabilities.services]]`
/// but the WASM imports `host-capabilities` is rejected at load time
/// with the dedicated `UndeclaredCapabilityImport` error.
#[test]
fn rejects_capability_import_without_manifest_declaration() {
    let loader = WasmPluginLoader::new().expect("failed to create loader");
    let bytes = crate::load_wasm_fixture("color-preview.wasm").expect("failed to load fixture");
    // Hand-rolled manifest that matches the bundled color-preview's ID
    // and ABI but DROPS the `[[capabilities.services]] name = "buffer"`
    // declaration. The bundled color-preview.wasm imports
    // `kasane:plugin/host-capabilities`, so the load-time scan must
    // surface the mismatch.
    let toml = r#"
[plugin]
id = "color_preview"
abi_version = "6.5.0"

[handlers]
flags = ["overlay", "input-handler", "annotator"]

[view]
deps = ["buffer-content", "buffer-cursor"]
"#;
    let manifest = crate::manifest::PluginManifest::parse(toml).expect("manifest parses");
    manifest.validate().expect("manifest validates");
    let result =
        loader.load_with_manifest(&bytes, &manifest, &crate::WasiCapabilityConfig::default());
    match result {
        Err((_, crate::WasmPluginError::UndeclaredCapabilityImport { service })) => {
            assert_eq!(service, "buffer");
        }
        Err((_, e)) => panic!("expected UndeclaredCapabilityImport, got error: {e}"),
        Ok(_) => panic!("expected UndeclaredCapabilityImport, but load succeeded"),
    }
}

#[test]
fn detects_colors_in_line() {
    let mut plugin = load_color_preview_plugin();
    let state = make_state_with_lines(&["#ff0000"]);
    apply_color_preview_state_change(&mut plugin, &state, DirtyFlags::BUFFER);

    assert!(plugin.has_unified_display());
    let directives = plugin.unified_display(&AppView::new(&state));
    // Phase 10 exemplar: gutter swatch + inline_box slot per color.
    assert_eq!(directives.len(), 2);
    assert!(matches!(
        &directives[0],
        DisplayDirective::Gutter { line: 0, .. }
    ));
    assert!(matches!(
        &directives[1],
        DisplayDirective::InlineBox { line: 0, .. }
    ));
}

#[test]
fn no_decoration_without_colors() {
    let mut plugin = load_color_preview_plugin();
    let state = make_state_with_lines(&["no colors here"]);
    apply_color_preview_state_change(&mut plugin, &state, DirtyFlags::BUFFER);

    let directives = plugin.unified_display(&AppView::new(&state));
    assert!(directives.is_empty());
}

#[test]
fn overlay_on_color_line() {
    let mut plugin = load_color_preview_plugin();
    let mut state = make_state_with_lines(&["#3498db"]);
    state.observed.cursor_pos = kasane_core::protocol::Coord { line: 0, column: 0 };
    apply_color_preview_state_change(&mut plugin, &state, DirtyFlags::BUFFER);

    let ctx = default_overlay_ctx();
    let overlay = plugin.contribute_overlay_with_ctx(&AppView::new(&state), &ctx);
    assert!(overlay.is_some());
}

#[test]
fn no_overlay_on_plain_line() {
    let mut plugin = load_color_preview_plugin();
    let mut state = make_state_with_lines(&["no colors here", "#ff0000"]);
    state.observed.cursor_pos = kasane_core::protocol::Coord { line: 0, column: 0 };
    apply_color_preview_state_change(&mut plugin, &state, DirtyFlags::BUFFER);

    let ctx = default_overlay_ctx();
    assert!(
        plugin
            .contribute_overlay_with_ctx(&AppView::new(&state), &ctx)
            .is_none()
    );
}

#[test]
fn state_hash_changes() {
    let mut plugin = load_color_preview_plugin();
    let h1 = plugin.state_hash();

    let state = make_state_with_lines(&["#aabbcc"]);
    apply_color_preview_state_change(&mut plugin, &state, DirtyFlags::BUFFER);
    let h2 = plugin.state_hash();

    assert_ne!(h1, h2);
}

#[test]
fn skips_non_buffer_dirty() {
    let mut plugin = load_color_preview_plugin();
    let h1 = plugin.state_hash();

    let state = make_state_with_lines(&["#aabbcc"]);
    apply_color_preview_state_change(&mut plugin, &state, DirtyFlags::STATUS);
    let h2 = plugin.state_hash();

    assert_eq!(h1, h2);
}

#[test]
fn handle_mouse_increments() {
    use kasane_core::element::InteractiveId;
    use kasane_core::input::{Modifiers, MouseButton, MouseEvent, MouseEventKind};

    let mut plugin = load_color_preview_plugin();
    let mut state = make_state_with_lines(&["#100000"]);
    state.observed.cursor_pos = kasane_core::protocol::Coord { line: 0, column: 0 };
    apply_color_preview_state_change(&mut plugin, &state, DirtyFlags::BUFFER);

    // R up button: id = 2000 + 0*6 + 0 = 2000
    let event = MouseEvent {
        kind: MouseEventKind::Press(MouseButton::Left),
        line: 0,
        column: 0,
        modifiers: Modifiers::empty(),
    };
    let result = plugin.handle_mouse(
        &event,
        InteractiveId::framework(2000),
        &AppView::new(&state),
    );
    assert!(result.is_some());
    let cmds = result.unwrap();
    assert_eq!(cmds.len(), 1);
    // Should be a SendToKakoune command
    match &cmds[0] {
        kasane_core::plugin::Command::SendToKakoune(
            kasane_core::protocol::KasaneRequest::Keys(keys),
        ) => {
            let joined: String = keys.join("");
            assert!(joined.contains("#110000"), "Expected #110000 in: {joined}");
        }
        _ => panic!("Expected SendToKakoune Keys"),
    }
}

#[test]
fn handle_mouse_consumes_release() {
    use kasane_core::element::InteractiveId;
    use kasane_core::input::{Modifiers, MouseButton, MouseEvent, MouseEventKind};

    let mut plugin = load_color_preview_plugin();
    let mut state = make_state_with_lines(&["#ff0000"]);
    state.observed.cursor_pos = kasane_core::protocol::Coord { line: 0, column: 0 };
    apply_color_preview_state_change(&mut plugin, &state, DirtyFlags::BUFFER);

    let event = MouseEvent {
        kind: MouseEventKind::Release(MouseButton::Left),
        line: 0,
        column: 0,
        modifiers: Modifiers::empty(),
    };
    let result = plugin.handle_mouse(
        &event,
        InteractiveId::framework(2000),
        &AppView::new(&state),
    );
    assert!(result.is_some());
    assert!(result.unwrap().is_empty());
}
