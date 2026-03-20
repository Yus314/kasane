//! Kasane library: `kasane::run()` entry point, plugin registration, backend selection.

pub mod cli;
pub mod plugin_cmd;
pub mod process;
pub mod process_manager;

pub use kasane_core;

#[cfg(feature = "wasm-plugins")]
pub use kasane_wasm;

use std::sync::Arc;

use anyhow::Result;
use kasane_core::config::Config;
use kasane_core::plugin::{
    PluginFactory, PluginManager, PluginProvider, ProcessDispatcher, ProcessEventSink,
    StaticPluginProvider, builtin_plugin,
};
use kasane_core::session::{SessionManager, SessionSpec};

use cli::UiMode;

fn setup_logging(config: &Config) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let log_dir = if let Some(ref file) = config.log.file {
        std::path::PathBuf::from(file)
    } else if let Ok(state_home) = std::env::var("XDG_STATE_HOME") {
        std::path::PathBuf::from(state_home).join("kasane")
    } else if let Ok(home) = std::env::var("HOME") {
        std::path::PathBuf::from(home)
            .join(".local")
            .join("state")
            .join("kasane")
    } else {
        return None;
    };

    let _ = std::fs::create_dir_all(&log_dir);

    let file_appender = tracing_appender::rolling::daily(&log_dir, "kasane.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = std::env::var("KASANE_LOG").unwrap_or_else(|_| config.log.level.clone());

    let subscriber = tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_env_filter(env_filter)
        .with_ansi(false)
        .finish();

    tracing::subscriber::set_global_default(subscriber).ok();

    Some(guard)
}

/// Run kasane with plugins collected from a provider.
pub fn run(provider: impl PluginProvider + 'static) {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let action = match cli::parse_cli_args(&args) {
        Ok(action) => action,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    match action {
        cli::CliAction::ShowVersion => {
            println!("kasane {}", env!("CARGO_PKG_VERSION"));
            println!("{}", process::get_kak_version());
        }
        cli::CliAction::ShowHelp => {
            cli::print_help();
        }
        cli::CliAction::DelegateToKak(args) => {
            process::exec_kak(&args);
        }
        cli::CliAction::Plugin(subcmd) => {
            if let Err(e) = plugin_cmd::execute(subcmd) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        cli::CliAction::RunKasane {
            session,
            ui_mode,
            kak_args,
        } => {
            if let Err(e) = run_inner(session, ui_mode, kak_args, provider) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
    }
}

/// Run kasane with a fixed set of host plugin factories.
pub fn run_with_factories(factories: impl IntoIterator<Item = Arc<dyn PluginFactory>>) {
    run(StaticPluginProvider::new(factories));
}

/// Run kasane without additional host plugins.
pub fn run_without_plugins() {
    run(StaticPluginProvider::new(
        Vec::<Arc<dyn PluginFactory>>::new(),
    ));
}

fn run_inner(
    session: Option<String>,
    ui_mode: Option<UiMode>,
    kak_args: Vec<String>,
    provider: impl PluginProvider + 'static,
) -> Result<()> {
    let config = Config::load();
    let _guard = setup_logging(&config);

    if let Some(ref s) = session {
        tracing::info!("session: {s}");
    }

    let resolved_ui = match ui_mode {
        Some(m) => m,
        None => match config.ui.backend.as_str() {
            "gui" => UiMode::Gui,
            _ => UiMode::Tui,
        },
    };

    let plugin_manager = build_plugin_manager(config.plugins.clone(), provider);

    // Build tokio runtime for async process management (Phase P-2).
    // The runtime must outlive run_tui/run_gui which are blocking calls.
    let tokio_rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let make_dispatcher = |sink: Arc<dyn ProcessEventSink>| -> Box<dyn ProcessDispatcher> {
        Box::new(process_manager::ProcessManager::new(
            tokio_rt.handle().clone(),
            sink,
        ))
    };

    let mut session_manager = SessionManager::new();
    let primary_session = SessionSpec::primary(session, kak_args);
    let (reader, writer, child) = process::spawn_kakoune_for_spec(&primary_session)?;
    session_manager
        .insert(primary_session, reader, writer, child)
        .expect("primary session key should be unique");

    let result = match resolved_ui {
        UiMode::Tui => kasane_tui::run_tui(
            config,
            session_manager,
            process::spawn_kakoune_for_spec,
            make_dispatcher,
            plugin_manager,
        ),
        #[cfg(feature = "gui")]
        UiMode::Gui => kasane_gui::run_gui(
            config,
            session_manager,
            process::spawn_kakoune_for_spec,
            make_dispatcher,
            plugin_manager,
        ),
        #[cfg(not(feature = "gui"))]
        UiMode::Gui => {
            let _ = make_dispatcher;
            let _ = plugin_manager;
            eprintln!("GUI support not compiled. Rebuild with: cargo build --features gui");
            std::process::exit(1);
        }
    };

    result?;

    // Propagate Kakoune's exit code for EDITOR= use case
    if let Some(code) = process::last_kak_exit_code() {
        std::process::exit(code);
    }

    Ok(())
}

fn build_plugin_manager(
    plugins_config: kasane_core::config::PluginsConfig,
    provider: impl PluginProvider + 'static,
) -> PluginManager {
    let mut providers: Vec<Box<dyn PluginProvider>> = Vec::new();
    #[cfg(feature = "wasm-plugins")]
    {
        providers.push(Box::new(kasane_wasm::WasmPluginProvider::new(
            plugins_config.clone(),
        )));
    }
    #[cfg(not(feature = "wasm-plugins"))]
    {
        let _ = plugins_config;
    }
    providers.push(Box::new(provider));
    providers.push(Box::new(StaticPluginProvider::new([builtin_plugin(
        "builtin-input",
        "kasane.builtin.input",
        || kasane_core::input::BuiltinInputPlugin,
    )])));
    PluginManager::new(providers)
}

#[cfg(all(test, feature = "wasm-plugins"))]
mod tests {
    use super::*;

    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use kasane_core::config::PluginsConfig;
    use kasane_core::plugin::PluginBackend;
    use kasane_core::plugin::{
        PluginId, PluginManager, PluginProvider, PluginRegistry, PluginSource,
        StaticPluginProvider, host_plugin,
    };
    use kasane_core::state::AppState;

    struct TempPluginDir {
        path: PathBuf,
    }

    impl TempPluginDir {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock before unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "kasane-plugin-reload-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("failed to create temp plugin dir");
            Self { path }
        }

        fn copy_fixture(&self, fixture_name: &str) {
            let src = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../kasane-wasm/fixtures")
                .join(fixture_name);
            let dst = self.path.join(fixture_name);
            fs::copy(src, dst).expect("failed to copy fixture");
        }

        fn remove(&self, file_name: &str) {
            fs::remove_file(self.path.join(file_name)).expect("failed to remove fixture");
        }

        fn config(&self) -> PluginsConfig {
            PluginsConfig {
                auto_discover: true,
                path: Some(self.path.to_string_lossy().into_owned()),
                disabled: vec![],
                ..Default::default()
            }
        }
    }

    impl Drop for TempPluginDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn wasm_manager(config: &PluginsConfig) -> PluginManager {
        PluginManager::new(vec![Box::new(kasane_wasm::WasmPluginProvider::new(
            config.clone(),
        ))])
    }

    struct CursorLineOverridePlugin;

    impl PluginBackend for CursorLineOverridePlugin {
        fn id(&self) -> PluginId {
            PluginId("cursor_line".to_string())
        }
    }

    fn full_manager(
        config: &PluginsConfig,
        provider: impl PluginProvider + 'static,
    ) -> PluginManager {
        build_plugin_manager(config.clone(), provider)
    }

    #[test]
    fn plugin_manager_reload_skips_unchanged_plugins() {
        let dir = TempPluginDir::new();
        dir.copy_fixture("cursor-line.wasm");

        let config = dir.config();
        let mut registry = PluginRegistry::new();
        let mut manager = wasm_manager(&config);
        manager.register_initial_winners(&mut registry).unwrap();

        let result = manager
            .reload(&mut registry, &AppState::default(), false)
            .unwrap();
        assert!(result.winner_changed.is_empty());
        assert_eq!(registry.plugin_count(), 1);
    }

    #[test]
    fn plugin_manager_reload_unloads_removed_plugins() {
        let dir = TempPluginDir::new();
        dir.copy_fixture("cursor-line.wasm");

        let config = dir.config();
        let mut registry = PluginRegistry::new();
        let mut manager = wasm_manager(&config);
        manager.register_initial_winners(&mut registry).unwrap();

        dir.remove("cursor-line.wasm");

        let result = manager
            .reload(&mut registry, &AppState::default(), false)
            .unwrap();
        assert!(result.ready_targets.is_empty());
        assert_eq!(registry.plugin_count(), 0);
        assert!(!registry.contains_plugin(&PluginId("cursor_line".to_string())));
    }

    #[test]
    fn plugin_manager_reload_applies_added_plugins_only() {
        let dir = TempPluginDir::new();
        dir.copy_fixture("cursor-line.wasm");

        let config = dir.config();
        let mut registry = PluginRegistry::new();
        let mut manager = wasm_manager(&config);
        manager.register_initial_winners(&mut registry).unwrap();

        dir.copy_fixture("smooth-scroll.wasm");

        let result = manager
            .reload(&mut registry, &AppState::default(), false)
            .unwrap();
        assert_eq!(
            result.winner_changed,
            vec![PluginId("smooth_scroll".to_string())]
        );
        assert_eq!(registry.plugin_count(), 2);
        assert!(registry.contains_plugin(&PluginId("smooth_scroll".to_string())));
    }

    #[test]
    fn host_override_survives_wasm_removal() {
        let dir = TempPluginDir::new();
        dir.copy_fixture("cursor-line.wasm");

        let config = dir.config();
        let mut registry = PluginRegistry::new();
        let mut manager = full_manager(
            &config,
            StaticPluginProvider::new([host_plugin("cursor_line", || CursorLineOverridePlugin)]),
        );
        manager.register_initial_winners(&mut registry).unwrap();
        assert!(matches!(
            manager
                .snapshot()
                .winner(&PluginId("cursor_line".to_string()))
                .map(|winner| &winner.source),
            Some(PluginSource::Host { .. })
        ));

        dir.remove("cursor-line.wasm");

        let result = manager
            .reload(&mut registry, &AppState::default(), false)
            .unwrap();
        assert!(
            !result
                .winner_changed
                .contains(&PluginId("cursor_line".to_string()))
        );
        assert!(matches!(
            manager
                .snapshot()
                .winner(&PluginId("cursor_line".to_string()))
                .map(|winner| &winner.source),
            Some(PluginSource::Host { .. })
        ));
    }

    #[test]
    fn host_override_blocks_later_wasm_addition() {
        let dir = TempPluginDir::new();
        let config = dir.config();
        let mut registry = PluginRegistry::new();
        let mut manager = full_manager(
            &config,
            StaticPluginProvider::new([host_plugin("cursor_line", || CursorLineOverridePlugin)]),
        );
        manager.register_initial_winners(&mut registry).unwrap();

        dir.copy_fixture("cursor-line.wasm");

        let result = manager
            .reload(&mut registry, &AppState::default(), false)
            .unwrap();
        assert!(
            !result
                .winner_changed
                .contains(&PluginId("cursor_line".to_string()))
        );
        assert!(matches!(
            manager
                .snapshot()
                .winner(&PluginId("cursor_line".to_string()))
                .map(|winner| &winner.source),
            Some(PluginSource::Host { .. })
        ));
    }
}
