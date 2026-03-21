use std::path::PathBuf;

use super::*;

fn fuzzy_finder_wasi_config() -> crate::WasiCapabilityConfig {
    crate::WasiCapabilityConfig {
        data_base_dir: std::env::temp_dir().join("kasane_test_fuzzy_finder"),
        ..Default::default()
    }
}

fn load_fuzzy_finder_plugin() -> crate::WasmPlugin {
    let loader = WasmPluginLoader::new().expect("failed to create loader");
    let bytes = crate::load_wasm_fixture("fuzzy-finder.wasm").expect("failed to load fixture");
    let config = fuzzy_finder_wasi_config();
    loader.load(&bytes, &config).expect("failed to load plugin")
}

fn load_fuzzy_finder_plugin_with_config(config: &crate::WasiCapabilityConfig) -> crate::WasmPlugin {
    let loader = WasmPluginLoader::new().expect("failed to create loader");
    let bytes = crate::load_wasm_fixture("fuzzy-finder.wasm").expect("failed to load fixture");
    loader.load(&bytes, config).expect("failed to load plugin")
}

fn ctrl_p_event() -> KeyEvent {
    KeyEvent {
        key: Key::Char('p'),
        modifiers: Modifiers::CTRL,
    }
}

fn char_event(c: char) -> KeyEvent {
    KeyEvent {
        key: Key::Char(c),
        modifiers: Modifiers::empty(),
    }
}

fn key_event(key: Key) -> KeyEvent {
    KeyEvent {
        key,
        modifiers: Modifiers::empty(),
    }
}

fn apply_fuzzy_io_event(
    plugin: &mut crate::WasmPlugin,
    event: IoEvent,
    state: &AppState,
) -> kasane_core::plugin::RuntimeEffects {
    let effects = plugin.on_io_event_effects(&event, state);
    assert!(effects.redraw.is_empty());
    assert!(effects.scroll_plans.is_empty());
    effects
}

#[test]
fn plugin_id() {
    let plugin = load_fuzzy_finder_plugin();
    assert_eq!(plugin.id().0, "fuzzy_finder");
}

#[test]
fn requests_process_capability() {
    let plugin = load_fuzzy_finder_plugin();
    assert!(plugin.allows_process_spawn());
}

#[test]
fn process_denied_by_config() {
    use std::collections::HashMap;
    let mut config = fuzzy_finder_wasi_config();
    config.deny_capabilities =
        HashMap::from([("fuzzy_finder".to_string(), vec!["process".to_string()])]);
    let plugin = load_fuzzy_finder_plugin_with_config(&config);
    assert!(!plugin.allows_process_spawn());
}

#[test]
fn inactive_passes_keys() {
    let mut plugin = load_fuzzy_finder_plugin();
    let state = AppState::default();

    // Regular keys pass through when inactive
    let result = plugin.handle_key(&char_event('a'), &state);
    assert!(result.is_none());

    let result = plugin.handle_key(&key_event(Key::Enter), &state);
    assert!(result.is_none());
}

#[test]
fn ctrl_p_returns_spawn_command() {
    let mut plugin = load_fuzzy_finder_plugin();
    let state = AppState::default();

    let result = plugin.handle_key(&ctrl_p_event(), &state);
    assert!(result.is_some());
    let cmds = result.unwrap();

    // Should contain a SpawnProcess command (for fd)
    let has_spawn = cmds
        .iter()
        .any(|c| matches!(c, Command::SpawnProcess { .. }));
    assert!(has_spawn, "expected SpawnProcess in commands");
}

#[test]
fn consumes_keys_when_active() {
    let mut plugin = load_fuzzy_finder_plugin();
    let state = AppState::default();

    // Activate
    plugin.handle_key(&ctrl_p_event(), &state);

    // All keys should be consumed (Some, not None)
    let result = plugin.handle_key(&char_event('a'), &state);
    assert!(result.is_some());

    let result = plugin.handle_key(&key_event(Key::Escape), &state);
    assert!(result.is_some());
}

#[test]
fn escape_deactivates() {
    let mut plugin = load_fuzzy_finder_plugin();
    let state = AppState::default();

    // Activate
    plugin.handle_key(&ctrl_p_event(), &state);

    // Escape
    plugin.handle_key(&key_event(Key::Escape), &state);

    // Should be inactive again — keys pass through
    let result = plugin.handle_key(&char_event('a'), &state);
    assert!(result.is_none());
}

#[test]
fn io_event_stdout_accumulation() {
    let mut plugin = load_fuzzy_finder_plugin();
    let state = AppState::default();

    // Activate (spawns fd with job_id=1)
    plugin.handle_key(&ctrl_p_event(), &state);
    let h1 = plugin.state_hash();

    // Simulate fd stdout in chunks
    apply_fuzzy_io_event(
        &mut plugin,
        IoEvent::Process(ProcessEvent::Stdout {
            job_id: 1,
            data: b"file1.rs\nfile2.rs\n".to_vec(),
        }),
        &state,
    );
    apply_fuzzy_io_event(
        &mut plugin,
        IoEvent::Process(ProcessEvent::Stdout {
            job_id: 1,
            data: b"file3.rs\n".to_vec(),
        }),
        &state,
    );

    // Simulate fd exit
    let effects = apply_fuzzy_io_event(
        &mut plugin,
        IoEvent::Process(ProcessEvent::Exited {
            job_id: 1,
            exit_code: 0,
        }),
        &state,
    );

    let h2 = plugin.state_hash();
    assert_ne!(h1, h2, "state_hash should change after receiving file list");

    // Should request redraw
    let has_redraw = effects
        .commands
        .iter()
        .any(|c| matches!(c, Command::RequestRedraw(_)));
    assert!(has_redraw, "expected RequestRedraw after fd exit");

    // Overlay should now show results
    let ctx = default_overlay_ctx();
    let overlay = plugin.contribute_overlay_with_ctx(&state, &ctx);
    assert!(
        overlay.is_some(),
        "overlay should be visible after file list received"
    );
}

#[test]
fn typed_io_event_effects_accumulation() {
    let mut plugin = load_fuzzy_finder_plugin();
    let state = AppState::default();

    plugin.handle_key(&ctrl_p_event(), &state);
    let h1 = plugin.state_hash();

    let effects = plugin.on_io_event_effects(
        &IoEvent::Process(ProcessEvent::Exited {
            job_id: 1,
            exit_code: 0,
        }),
        &state,
    );

    let h2 = plugin.state_hash();
    assert_ne!(
        h1, h2,
        "state_hash should change after typed io_event_effects"
    );
    assert!(effects.redraw.is_empty());
    assert!(
        effects
            .commands
            .iter()
            .any(|c| matches!(c, Command::RequestRedraw(_))),
        "expected RequestRedraw in typed runtime effects"
    );
    assert!(effects.scroll_plans.is_empty());
}

#[test]
fn overlay_uses_absolute_anchor() {
    let mut plugin = load_fuzzy_finder_plugin();
    let state = AppState::default();

    plugin.handle_key(&ctrl_p_event(), &state);
    apply_fuzzy_io_event(
        &mut plugin,
        IoEvent::Process(ProcessEvent::Stdout {
            job_id: 1,
            data: b"src/main.rs\nsrc/lib.rs\n".to_vec(),
        }),
        &state,
    );
    apply_fuzzy_io_event(
        &mut plugin,
        IoEvent::Process(ProcessEvent::Exited {
            job_id: 1,
            exit_code: 0,
        }),
        &state,
    );

    let overlay = plugin
        .contribute_overlay_with_ctx(&state, &default_overlay_ctx())
        .expect("expected overlay");
    match overlay.anchor {
        OverlayAnchor::Absolute { w, h, .. } => {
            assert!(w > 0);
            assert!(h > 0);
        }
        other => panic!("expected absolute overlay anchor, got {other:?}"),
    }
}

#[test]
fn spawn_failed_no_panic() {
    let mut plugin = load_fuzzy_finder_plugin();
    let state = AppState::default();

    // Activate
    plugin.handle_key(&ctrl_p_event(), &state);

    // fd fails → should try find fallback
    let effects = apply_fuzzy_io_event(
        &mut plugin,
        IoEvent::Process(ProcessEvent::SpawnFailed {
            job_id: 1,
            error: "not found".to_string(),
        }),
        &state,
    );

    // Should have spawned find as fallback
    let has_spawn = effects
        .commands
        .iter()
        .any(|c| matches!(c, Command::SpawnProcess { .. }));
    assert!(has_spawn, "expected find fallback SpawnProcess");

    // find also fails
    let effects = apply_fuzzy_io_event(
        &mut plugin,
        IoEvent::Process(ProcessEvent::SpawnFailed {
            job_id: 2,
            error: "not found".to_string(),
        }),
        &state,
    );

    // Should show error overlay without panicking
    let has_redraw = effects
        .commands
        .iter()
        .any(|c| matches!(c, Command::RequestRedraw(_)));
    assert!(has_redraw);

    let ctx = default_overlay_ctx();
    let overlay = plugin.contribute_overlay_with_ctx(&state, &ctx);
    assert!(overlay.is_some(), "overlay should show error");
}

#[test]
fn fzf_spawn_failed_shows_error() {
    let mut plugin = load_fuzzy_finder_plugin();
    let state = AppState::default();

    // Activate and provide file list
    plugin.handle_key(&ctrl_p_event(), &state);
    apply_fuzzy_io_event(
        &mut plugin,
        IoEvent::Process(ProcessEvent::Stdout {
            job_id: 1,
            data: b"file1.rs\n".to_vec(),
        }),
        &state,
    );
    apply_fuzzy_io_event(
        &mut plugin,
        IoEvent::Process(ProcessEvent::Exited {
            job_id: 1,
            exit_code: 0,
        }),
        &state,
    );

    // Type a character to trigger fzf
    plugin.handle_key(&char_event('f'), &state);

    // fzf spawn fails (job_id = 100 + 1 = 101)
    apply_fuzzy_io_event(
        &mut plugin,
        IoEvent::Process(ProcessEvent::SpawnFailed {
            job_id: 101,
            error: "fzf not installed".to_string(),
        }),
        &state,
    );

    // Should still show overlay (with error)
    let ctx = default_overlay_ctx();
    let overlay = plugin.contribute_overlay_with_ctx(&state, &ctx);
    assert!(overlay.is_some());
}

#[test]
fn enter_selects_file() {
    let mut plugin = load_fuzzy_finder_plugin();
    let state = AppState::default();

    // Activate and provide file list
    plugin.handle_key(&ctrl_p_event(), &state);
    apply_fuzzy_io_event(
        &mut plugin,
        IoEvent::Process(ProcessEvent::Stdout {
            job_id: 1,
            data: b"src/main.rs\nsrc/lib.rs\n".to_vec(),
        }),
        &state,
    );
    apply_fuzzy_io_event(
        &mut plugin,
        IoEvent::Process(ProcessEvent::Exited {
            job_id: 1,
            exit_code: 0,
        }),
        &state,
    );

    // Press Enter to select first file
    let cmds = plugin.handle_key(&key_event(Key::Enter), &state);
    assert!(cmds.is_some());
    let cmds = cmds.unwrap();

    // Should contain SendKeys with edit command
    let has_send_keys = cmds.iter().any(|c| {
        matches!(c, Command::SendToKakoune(kasane_core::protocol::KasaneRequest::Keys(keys)) if keys.iter().any(|k| k.contains("e") || k.contains("d") || k.contains("i") || k.contains("t")))
    });
    assert!(has_send_keys, "expected SendToKakoune with edit keys");

    // Should be inactive after Enter
    let result = plugin.handle_key(&char_event('a'), &state);
    assert!(result.is_none(), "should be inactive after Enter");
}

#[test]
fn up_down_navigation() {
    let mut plugin = load_fuzzy_finder_plugin();
    let state = AppState::default();

    // Activate and provide file list
    plugin.handle_key(&ctrl_p_event(), &state);
    apply_fuzzy_io_event(
        &mut plugin,
        IoEvent::Process(ProcessEvent::Stdout {
            job_id: 1,
            data: b"a.rs\nb.rs\nc.rs\n".to_vec(),
        }),
        &state,
    );
    apply_fuzzy_io_event(
        &mut plugin,
        IoEvent::Process(ProcessEvent::Exited {
            job_id: 1,
            exit_code: 0,
        }),
        &state,
    );

    let h1 = plugin.state_hash();

    // Down
    plugin.handle_key(&key_event(Key::Down), &state);
    let h2 = plugin.state_hash();
    assert_ne!(h1, h2, "state_hash should change on Down");

    // Down again
    plugin.handle_key(&key_event(Key::Down), &state);
    let h3 = plugin.state_hash();
    assert_ne!(h2, h3);

    // Up
    plugin.handle_key(&key_event(Key::Up), &state);
    let h4 = plugin.state_hash();
    assert_ne!(h3, h4);
}

#[test]
fn discover_loads_with_fixtures() {
    // When discover scans fixtures/, fuzzy-finder.wasm should load
    let config = PluginsConfig {
        auto_discover: true,
        path: Some(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures")
                .to_string_lossy()
                .into_owned(),
        ),
        disabled: vec![],
        ..Default::default()
    };
    let mut registry = PluginRuntime::new();
    crate::discover_and_register(&config, &mut registry);

    // Should now include fuzzy_finder among loaded plugins
    assert!(
        registry.plugin_count() >= 5,
        "expected at least 5 plugins (including fuzzy_finder)"
    );
}
