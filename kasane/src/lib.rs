//! Kasane library: `kasane::run()` entry point, plugin registration, backend selection.

pub mod cli;
pub mod plugin_cmd;
pub mod process;
pub mod process_manager;

pub use kasane_core;

#[cfg(feature = "wasm-plugins")]
pub use kasane_wasm;

use std::sync::Arc;

use anyhow::{Context, Result};
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

    // Daemon mode: separate server (kak -d) and client (kak -c) processes.
    // In -c mode (connecting to an existing session), no daemon is spawned.
    let connect_mode = cli::is_connect_mode(&kak_args);

    let (session, kak_args, daemon) = if connect_mode {
        (session, kak_args, None)
    } else {
        let server_name = session.unwrap_or_else(|| format!("kasane-{}", std::process::id()));
        let (daemon_args, client_args) = cli::partition_kak_args(&kak_args);

        let mut daemon = process::spawn_kakoune_daemon(&server_name, &daemon_args)?;
        daemon
            .wait_ready(std::time::Duration::from_secs(5))
            .context("kakoune daemon failed to start")?;

        let mut primary_args = vec!["-c".to_string(), server_name.clone()];
        primary_args.extend(client_args);

        (Some(server_name), primary_args, Some(daemon))
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

    // Clean up the daemon before propagating errors.
    if let Some(mut d) = daemon {
        d.kill();
    }

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
    providers.push(Box::new(StaticPluginProvider::new([
        builtin_plugin(
            "builtin-pane-manager",
            "kasane.builtin.pane-manager",
            kasane_core::input::PaneManagerPlugin::new,
        ),
        builtin_plugin("builtin-input", "kasane.builtin.input", || {
            kasane_core::input::BuiltinInputPlugin
        }),
    ])));
    PluginManager::new(providers)
}

#[cfg(all(test, feature = "wasm-plugins"))]
mod tests {
    use super::*;

    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use anyhow::Result as AnyResult;
    use kasane_core::config::PluginsConfig;
    use kasane_core::event_loop::{
        reconcile_plugin_surfaces, register_builtin_surfaces, setup_plugin_surfaces,
    };
    use kasane_core::layout::SplitDirection;
    use kasane_core::plugin::{
        AppView, PaintHook, PluginBackend, PluginCapabilities, SessionReadyEffects,
    };
    use kasane_core::plugin::{
        PluginCollect, PluginDescriptor, PluginDiagnosticKind, PluginId, PluginManager,
        PluginProvider, PluginRank, PluginRevision, PluginRuntime, PluginSource,
        StaticPluginProvider, host_plugin, plugin_factory,
    };
    use kasane_core::state::{AppState, DirtyFlags};
    use kasane_core::surface::{Surface, SurfaceId, SurfaceRegistrationError, SurfaceRegistry};
    use kasane_core::test_support::TestSurfaceBuilder;
    use kasane_core::workspace::Placement;

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

    #[derive(Clone, Copy)]
    enum ReloadVariant {
        V1,
        V2,
    }

    impl ReloadVariant {
        fn revision(self) -> &'static str {
            match self {
                Self::V1 => "r1",
                Self::V2 => "r2",
            }
        }

        fn hook_id(self) -> &'static str {
            match self {
                Self::V1 => "hook-a",
                Self::V2 => "hook-b",
            }
        }

        fn ready_redraw(self) -> DirtyFlags {
            match self {
                Self::V1 => DirtyFlags::STATUS,
                Self::V2 => DirtyFlags::BUFFER,
            }
        }

        fn bootstrap_redraw(self) -> DirtyFlags {
            match self {
                Self::V1 => DirtyFlags::BUFFER_CURSOR,
                Self::V2 => DirtyFlags::ALL,
            }
        }
    }

    struct ReloadHook {
        id: &'static str,
    }

    impl PaintHook for ReloadHook {
        fn id(&self) -> &str {
            self.id
        }

        fn deps(&self) -> DirtyFlags {
            DirtyFlags::ALL
        }

        fn apply(
            &self,
            _grid: &mut kasane_core::render::CellGrid,
            _region: &kasane_core::layout::Rect,
            _state: &AppState,
        ) {
        }
    }

    struct ReloadChainPlugin {
        variant: ReloadVariant,
    }

    impl PluginBackend for ReloadChainPlugin {
        fn id(&self) -> PluginId {
            PluginId("reload_owner".to_string())
        }

        fn capabilities(&self) -> PluginCapabilities {
            PluginCapabilities::PAINT_HOOK
        }

        fn on_init_effects(
            &mut self,
            _state: &AppView<'_>,
        ) -> kasane_core::plugin::BootstrapEffects {
            kasane_core::plugin::BootstrapEffects {
                redraw: self.variant.bootstrap_redraw(),
            }
        }

        fn on_active_session_ready_effects(&mut self, _state: &AppView<'_>) -> SessionReadyEffects {
            SessionReadyEffects {
                redraw: self.variant.ready_redraw(),
                commands: vec![],
                scroll_plans: vec![],
            }
        }

        fn surfaces(&mut self) -> Vec<Box<dyn Surface>> {
            vec![TestSurfaceBuilder::new(SurfaceId(200)).build()]
        }

        fn workspace_request(&self) -> Option<Placement> {
            Some(Placement::SplitFocused {
                direction: SplitDirection::Vertical,
                ratio: 0.4,
            })
        }

        fn paint_hooks(&self) -> Vec<Box<dyn PaintHook>> {
            vec![Box::new(ReloadHook {
                id: self.variant.hook_id(),
            })]
        }
    }

    #[derive(Clone)]
    struct ReloadChainProvider {
        variant: Arc<std::sync::Mutex<ReloadVariant>>,
    }

    impl ReloadChainProvider {
        fn new(initial: ReloadVariant) -> Self {
            Self {
                variant: Arc::new(std::sync::Mutex::new(initial)),
            }
        }

        fn set_variant(&self, variant: ReloadVariant) {
            *self.variant.lock().expect("poisoned reload variant") = variant;
        }
    }

    impl PluginProvider for ReloadChainProvider {
        fn collect(&self) -> AnyResult<PluginCollect> {
            let variant = *self.variant.lock().expect("poisoned reload variant");
            let descriptor = PluginDescriptor {
                id: PluginId("reload_owner".to_string()),
                source: PluginSource::Host {
                    provider: "reload-test".to_string(),
                },
                revision: PluginRevision(variant.revision().to_string()),
                rank: PluginRank::HOST,
            };
            Ok(PluginCollect {
                factories: vec![plugin_factory(descriptor, move || {
                    Ok(Box::new(ReloadChainPlugin { variant }))
                })],
                diagnostics: vec![],
            })
        }
    }

    #[derive(Clone, Copy)]
    enum DiagnosticVariant {
        Valid,
        Invalid,
    }

    struct DiagnosticSurfacePlugin {
        variant: DiagnosticVariant,
    }

    impl PluginBackend for DiagnosticSurfacePlugin {
        fn id(&self) -> PluginId {
            PluginId("diagnostic_owner".to_string())
        }

        fn surfaces(&mut self) -> Vec<Box<dyn Surface>> {
            match self.variant {
                DiagnosticVariant::Valid => vec![TestSurfaceBuilder::new(SurfaceId(201)).build()],
                DiagnosticVariant::Invalid => {
                    vec![TestSurfaceBuilder::new(SurfaceId::BUFFER).build()]
                }
            }
        }
    }

    #[derive(Clone)]
    struct DiagnosticProvider {
        variant: Arc<std::sync::Mutex<DiagnosticVariant>>,
    }

    impl DiagnosticProvider {
        fn new(initial: DiagnosticVariant) -> Self {
            Self {
                variant: Arc::new(std::sync::Mutex::new(initial)),
            }
        }

        fn set_variant(&self, variant: DiagnosticVariant) {
            *self.variant.lock().expect("poisoned diagnostic variant") = variant;
        }
    }

    impl PluginProvider for DiagnosticProvider {
        fn collect(&self) -> AnyResult<PluginCollect> {
            let variant = *self.variant.lock().expect("poisoned diagnostic variant");
            let revision = match variant {
                DiagnosticVariant::Valid => "valid",
                DiagnosticVariant::Invalid => "invalid",
            };
            let descriptor = PluginDescriptor {
                id: PluginId("diagnostic_owner".to_string()),
                source: PluginSource::Host {
                    provider: "diagnostic-test".to_string(),
                },
                revision: PluginRevision(revision.to_string()),
                rank: PluginRank::HOST,
            };
            Ok(PluginCollect {
                factories: vec![plugin_factory(descriptor, move || {
                    Ok(Box::new(DiagnosticSurfacePlugin { variant }))
                })],
                diagnostics: vec![],
            })
        }
    }

    fn commit_initial_winners(manager: &mut PluginManager, registry: &mut PluginRuntime) {
        let _ = manager.initialize(registry, |_, _| vec![]).unwrap();
    }

    #[test]
    fn plugin_manager_reload_skips_unchanged_plugins() {
        let dir = TempPluginDir::new();
        dir.copy_fixture("cursor-line.wasm");

        let config = dir.config();
        let mut registry = PluginRuntime::new();
        let mut manager = wasm_manager(&config);
        commit_initial_winners(&mut manager, &mut registry);

        let state = AppState::default();
        let result = manager
            .reload(&mut registry, &AppView::new(&state), |_, _| vec![])
            .unwrap();
        assert!(result.deltas.is_empty());
        assert_eq!(registry.plugin_count(), 1);
    }

    #[test]
    fn plugin_manager_reload_unloads_removed_plugins() {
        let dir = TempPluginDir::new();
        dir.copy_fixture("cursor-line.wasm");

        let config = dir.config();
        let mut registry = PluginRuntime::new();
        let mut manager = wasm_manager(&config);
        commit_initial_winners(&mut manager, &mut registry);

        dir.remove("cursor-line.wasm");

        let state = AppState::default();
        let result = manager
            .reload(&mut registry, &AppView::new(&state), |_, _| vec![])
            .unwrap();
        assert_eq!(result.deltas.len(), 1);
        assert!(result.deltas[0].is_removed());
        assert_eq!(result.ready_targets().count(), 0);
        assert_eq!(registry.plugin_count(), 0);
        assert!(!registry.contains_plugin(&PluginId("cursor_line".to_string())));
    }

    #[test]
    fn plugin_manager_reload_applies_added_plugins_only() {
        let dir = TempPluginDir::new();
        dir.copy_fixture("cursor-line.wasm");

        let config = dir.config();
        let mut registry = PluginRuntime::new();
        let mut manager = wasm_manager(&config);
        commit_initial_winners(&mut manager, &mut registry);

        dir.copy_fixture("smooth-scroll.wasm");

        let state = AppState::default();
        let result = manager
            .reload(&mut registry, &AppView::new(&state), |_, _| vec![])
            .unwrap();
        assert_eq!(result.deltas.len(), 1);
        assert!(result.deltas[0].is_added());
        assert_eq!(result.deltas[0].id, PluginId("smooth_scroll".to_string()));
        assert_eq!(registry.plugin_count(), 2);
        assert!(registry.contains_plugin(&PluginId("smooth_scroll".to_string())));
    }

    #[test]
    fn host_override_survives_wasm_removal() {
        let dir = TempPluginDir::new();
        dir.copy_fixture("cursor-line.wasm");

        let config = dir.config();
        let mut registry = PluginRuntime::new();
        let mut manager = full_manager(
            &config,
            StaticPluginProvider::new([host_plugin("cursor_line", || CursorLineOverridePlugin)]),
        );
        commit_initial_winners(&mut manager, &mut registry);
        assert!(matches!(
            manager
                .snapshot()
                .winner(&PluginId("cursor_line".to_string()))
                .map(|winner| &winner.source),
            Some(PluginSource::Host { .. })
        ));

        dir.remove("cursor-line.wasm");

        let state = AppState::default();
        let result = manager
            .reload(&mut registry, &AppView::new(&state), |_, _| vec![])
            .unwrap();
        assert!(
            result
                .deltas
                .iter()
                .all(|delta| delta.id != PluginId("cursor_line".to_string()))
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
        let mut registry = PluginRuntime::new();
        let mut manager = full_manager(
            &config,
            StaticPluginProvider::new([host_plugin("cursor_line", || CursorLineOverridePlugin)]),
        );
        commit_initial_winners(&mut manager, &mut registry);

        dir.copy_fixture("cursor-line.wasm");

        let state = AppState::default();
        let result = manager
            .reload(&mut registry, &AppView::new(&state), |_, _| vec![])
            .unwrap();
        assert!(
            result
                .deltas
                .iter()
                .all(|delta| delta.id != PluginId("cursor_line".to_string()))
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
    fn reload_reconciles_surface_paint_hook_and_ready_chain() {
        let state = AppState::default();
        let provider = ReloadChainProvider::new(ReloadVariant::V1);
        let mut manager = PluginManager::new(vec![Box::new(provider.clone())]);
        let mut registry = PluginRuntime::new();
        let mut surface_registry = SurfaceRegistry::new();
        register_builtin_surfaces(&mut surface_registry);

        let _ = manager
            .initialize(&mut registry, |_, registry| {
                let disabled = setup_plugin_surfaces(registry, &mut surface_registry, &state);
                assert!(disabled.is_empty());
                disabled
            })
            .unwrap();

        assert!(surface_registry.get(SurfaceId(200)).is_some());
        let hooks = registry.collect_paint_hooks_for_owner(&PluginId("reload_owner".to_string()));
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0].id(), "hook-a");

        provider.set_variant(ReloadVariant::V2);

        let result = manager
            .reload(&mut registry, &AppView::new(&state), |result, registry| {
                let disabled = reconcile_plugin_surfaces(
                    registry,
                    &mut surface_registry,
                    &state,
                    &result.deltas,
                );
                assert!(disabled.is_empty());
                disabled
            })
            .unwrap();

        assert_eq!(
            result.bootstrap.redraw,
            ReloadVariant::V2.bootstrap_redraw()
        );
        assert_eq!(result.deltas.len(), 1);
        assert!(result.deltas[0].is_replaced());
        assert_eq!(
            result.ready_targets().cloned().collect::<Vec<_>>(),
            vec![PluginId("reload_owner".to_string())]
        );
        assert_eq!(
            surface_registry
                .workspace()
                .root()
                .collect_ids()
                .into_iter()
                .filter(|surface_id| *surface_id == SurfaceId(200))
                .count(),
            1
        );

        let hooks = registry.collect_paint_hooks_for_owner(&PluginId("reload_owner".to_string()));
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0].id(), "hook-b");

        let ready_batch = registry.notify_plugin_active_session_ready_batch(
            &PluginId("reload_owner".to_string()),
            &AppView::new(&state),
        );
        assert_eq!(ready_batch.effects.redraw, ReloadVariant::V2.ready_redraw());
    }

    #[test]
    fn initialize_reports_surface_activation_diagnostic() {
        let state = AppState::default();
        let mut manager = PluginManager::new(vec![Box::new(DiagnosticProvider::new(
            DiagnosticVariant::Invalid,
        ))]);
        let mut registry = PluginRuntime::new();
        let mut surface_registry = SurfaceRegistry::new();
        register_builtin_surfaces(&mut surface_registry);

        let result = manager
            .initialize(&mut registry, |_, registry| {
                setup_plugin_surfaces(registry, &mut surface_registry, &state)
            })
            .unwrap();

        assert!(result.deltas.is_empty());
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(
            result.diagnostics[0].plugin_id(),
            Some(&PluginId("diagnostic_owner".to_string()))
        );
        assert!(matches!(
            result.diagnostics[0].kind,
            PluginDiagnosticKind::SurfaceRegistrationFailed {
                reason: SurfaceRegistrationError::DuplicateSurfaceId { .. }
            }
        ));
        assert!(result.diagnostics[0].previous.is_none());
        assert_eq!(
            result.diagnostics[0]
                .attempted
                .as_ref()
                .map(|descriptor| descriptor.revision.0.as_str()),
            Some("invalid")
        );
        assert!(
            manager
                .snapshot()
                .winner(&PluginId("diagnostic_owner".to_string()))
                .is_none()
        );
        assert!(!registry.contains_plugin(&PluginId("diagnostic_owner".to_string())));
    }

    #[test]
    fn reload_reports_surface_activation_diagnostic_and_removes_winner() {
        let state = AppState::default();
        let provider = DiagnosticProvider::new(DiagnosticVariant::Valid);
        let mut manager = PluginManager::new(vec![Box::new(provider.clone())]);
        let mut registry = PluginRuntime::new();
        let mut surface_registry = SurfaceRegistry::new();
        register_builtin_surfaces(&mut surface_registry);

        let initial = manager
            .initialize(&mut registry, |_, registry| {
                let diagnostics = setup_plugin_surfaces(registry, &mut surface_registry, &state);
                assert!(diagnostics.is_empty());
                diagnostics
            })
            .unwrap();
        assert!(initial.diagnostics.is_empty());
        assert!(surface_registry.get(SurfaceId(201)).is_some());

        provider.set_variant(DiagnosticVariant::Invalid);

        let result = manager
            .reload(&mut registry, &AppView::new(&state), |result, registry| {
                reconcile_plugin_surfaces(registry, &mut surface_registry, &state, &result.deltas)
            })
            .unwrap();

        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(
            result.diagnostics[0].plugin_id(),
            Some(&PluginId("diagnostic_owner".to_string()))
        );
        assert!(matches!(
            result.diagnostics[0].kind,
            PluginDiagnosticKind::SurfaceRegistrationFailed {
                reason: SurfaceRegistrationError::DuplicateSurfaceId { .. }
            }
        ));
        assert_eq!(
            result.diagnostics[0]
                .previous
                .as_ref()
                .map(|descriptor| descriptor.revision.0.as_str()),
            Some("valid")
        );
        assert_eq!(
            result.diagnostics[0]
                .attempted
                .as_ref()
                .map(|descriptor| descriptor.revision.0.as_str()),
            Some("invalid")
        );
        assert_eq!(result.deltas.len(), 1);
        assert!(result.deltas[0].is_removed());
        assert!(
            manager
                .snapshot()
                .winner(&PluginId("diagnostic_owner".to_string()))
                .is_none()
        );
        assert!(!registry.contains_plugin(&PluginId("diagnostic_owner".to_string())));
        assert!(surface_registry.get(SurfaceId(201)).is_none());
        assert!(
            !surface_registry
                .workspace()
                .root()
                .collect_ids()
                .contains(&SurfaceId(201))
        );
    }
}
