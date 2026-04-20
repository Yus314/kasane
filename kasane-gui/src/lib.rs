pub mod animation;
mod app;
pub(crate) mod backend;
pub mod colors;
mod diagnostics_overlay;
pub mod gpu;
mod ime;
pub(crate) mod input;

use std::sync::Arc;
use std::sync::OnceLock;

use anyhow::Result;

/// Global session name for panic hook reconnect message.
static SESSION_NAME: OnceLock<String> = OnceLock::new();

/// Install a panic hook that shows session reconnect info.
fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_hook(info);
        kasane_core::event_loop::print_session_recovery_hint(
            SESSION_NAME.get().map(|s| s.as_str()),
        );
    }));
}
use std::time::Duration;

use kasane_core::config::Config;
use kasane_core::plugin::{IoEvent, PluginId, PluginManager, ProcessEventSink};
use kasane_core::protocol::KakouneRequest;
use kasane_core::session::{SessionId, SessionManager, SessionSpec};
use winit::event_loop::EventLoop;

/// Wrapper for plugin timer payloads (Any + Send, no Debug).
pub(crate) struct TimerPayload(pub Box<dyn std::any::Any + Send>);

impl std::fmt::Debug for TimerPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("TimerPayload").finish()
    }
}

/// Events injected into the winit event loop from background threads.
#[derive(Debug)]
pub(crate) enum GuiEvent {
    Kakoune(SessionId, KakouneRequest),
    KakouneDied(SessionId),
    PluginTimer(PluginId, TimerPayload),
    ProcessOutput(PluginId, IoEvent),
    DiagnosticOverlayExpire(u64),
    /// Background image decode completed.
    ImageLoaded(
        gpu::texture_cache::TextureKey,
        Result<gpu::texture_cache::DecodedImage, String>,
    ),
    /// kasane.kdl changed — reload config + widgets.
    FileReload,
    /// Plugin .reload sentinel changed — hot-reload WASM plugins.
    PluginReload,
}

/// EventSink that injects events into the winit event loop.
#[derive(Clone)]
pub(crate) struct GuiEventSink(pub(crate) winit::event_loop::EventLoopProxy<GuiEvent>);

impl kasane_core::event_loop::EventSink for GuiEventSink {
    fn send_kakoune(&self, session_id: SessionId, req: KakouneRequest) {
        let _ = self.0.send_event(GuiEvent::Kakoune(session_id, req));
    }
    fn send_died(&self, session_id: SessionId) {
        let _ = self.0.send_event(GuiEvent::KakouneDied(session_id));
    }
    fn send_timer(&self, target: PluginId, payload: Box<dyn std::any::Any + Send>) {
        let _ = self
            .0
            .send_event(GuiEvent::PluginTimer(target, TimerPayload(payload)));
    }
    fn send_diagnostic_expire(&self, generation: u64) {
        let _ = self
            .0
            .send_event(GuiEvent::DiagnosticOverlayExpire(generation));
    }
}

/// ProcessEventSink that injects process I/O events into the winit event loop.
struct GuiProcessEventSink(winit::event_loop::EventLoopProxy<GuiEvent>);

impl ProcessEventSink for GuiProcessEventSink {
    fn send_process_output(&self, plugin_id: PluginId, event: IoEvent) {
        let _ = self.0.send_event(GuiEvent::ProcessOutput(plugin_id, event));
    }
}

/// Launch the GUI backend. Called from `kasane --ui gui`.
///
/// `session_manager`: managed Kakoune sessions. V1 consumes the active session only.
/// `create_process_dispatcher`: factory that receives a `ProcessEventSink` and returns
///   a `ProcessDispatcher` for plugin-spawned processes.
pub fn run_gui<R, W, C>(
    config: Config,
    mut session_manager: SessionManager<R, W, C>,
    spawn_session: fn(&SessionSpec) -> Result<(R, W, C)>,
    create_process_dispatcher: impl FnOnce(
        Arc<dyn ProcessEventSink>,
    ) -> Box<dyn kasane_core::plugin::ProcessDispatcher>,
    create_http_dispatcher: impl FnOnce(
        Arc<dyn ProcessEventSink>,
    ) -> Box<dyn kasane_core::plugin::HttpDispatcher>,
    plugin_manager: PluginManager,
) -> Result<()>
where
    R: std::io::BufRead + Send + 'static,
    W: std::io::Write + Send + 'static,
    C: Send + 'static,
{
    install_panic_hook();

    let event_loop = EventLoop::<GuiEvent>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();

    // Store session name for panic hook reconnect message
    if let Some(spec) = session_manager.active_spec()
        && let Some(ref name) = spec.session
    {
        let _ = SESSION_NAME.set(name.clone());
    }

    let active_session = session_manager
        .active_session_id()
        .ok_or_else(|| anyhow::anyhow!("missing primary session id"))?;
    let kak_reader = session_manager
        .take_active_reader()
        .map_err(|err| anyhow::anyhow!("failed to acquire primary session: {err:?}"))?;

    // Build plugin registry
    let registry = kasane_core::plugin::PluginRuntime::new();
    // Process dispatcher for plugin-spawned processes
    let process_sink: Arc<dyn ProcessEventSink> = Arc::new(GuiProcessEventSink(proxy.clone()));
    let process_dispatcher = create_process_dispatcher(Arc::clone(&process_sink));
    let http_dispatcher = create_http_dispatcher(process_sink);

    // Kakoune reader thread: forward JSON-RPC messages into the winit event loop
    let gui_sink = GuiEventSink(proxy.clone());
    kasane_core::event_loop::spawn_session_reader(active_session, kak_reader, gui_sink.clone());

    let (mut app_handler, widget_included_paths) = app::App::new(
        config.clone(),
        session_manager,
        spawn_session,
        proxy.clone(),
        plugin_manager,
        registry,
        process_dispatcher,
        http_dispatcher,
    )?;

    // Plugin hot-reload sentinel watcher thread (500ms polling)
    {
        let plugins_dir = config.plugins.plugins_dir();
        let reload_sentinel = plugins_dir.join(".reload");
        let proxy = proxy.clone();
        std::thread::spawn(move || {
            let mut last_modified = reload_sentinel.metadata().and_then(|m| m.modified()).ok();
            loop {
                std::thread::sleep(Duration::from_millis(500));
                let current = reload_sentinel.metadata().and_then(|m| m.modified()).ok();
                if current != last_modified && current.is_some() {
                    last_modified = current;
                    if proxy.send_event(GuiEvent::PluginReload).is_err() {
                        return;
                    }
                }
            }
        });
    }

    // Unified kasane.kdl hot-reload watcher (notify-based, 100ms debounce)
    let _config_watcher = {
        use notify::Watcher;

        let file_proxy = proxy.clone();
        let config_path = kasane_core::config::config_path();

        let (debounce_tx, debounce_rx) = std::sync::mpsc::channel::<()>();
        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res
                && (event.kind.is_modify() || event.kind.is_create())
            {
                let _ = debounce_tx.send(());
            }
        });

        match watcher {
            Ok(mut watcher) => {
                // Watch config file's parent directory.
                if let Some(parent) = config_path.parent() {
                    let _ = watcher.watch(parent, notify::RecursiveMode::NonRecursive);
                }

                // Also watch directories containing included widget files.
                let mut watched_dirs = std::collections::HashSet::new();
                for inc_path in &widget_included_paths {
                    if let Some(parent) = inc_path.parent()
                        && watched_dirs.insert(parent.to_path_buf())
                    {
                        let _ = watcher.watch(parent, notify::RecursiveMode::NonRecursive);
                    }
                }

                // Debounce thread: wait 100ms after last event before firing FileReload.
                std::thread::spawn(move || {
                    while debounce_rx.recv().is_ok() {
                        std::thread::sleep(Duration::from_millis(100));
                        while debounce_rx.try_recv().is_ok() {}
                        if file_proxy.send_event(GuiEvent::FileReload).is_err() {
                            return;
                        }
                    }
                });

                Some(watcher)
            }
            Err(e) => {
                tracing::warn!(
                    "failed to create config file watcher, falling back to polling: {e}"
                );
                // Fallback: 2-second polling thread.
                let file_proxy2 = proxy;
                let included_paths = widget_included_paths;
                std::thread::spawn(move || {
                    let mut last_modified = config_path.metadata().and_then(|m| m.modified()).ok();
                    let mut included_mtimes: Vec<Option<std::time::SystemTime>> = included_paths
                        .iter()
                        .map(|p| p.metadata().and_then(|m| m.modified()).ok())
                        .collect();
                    loop {
                        std::thread::sleep(Duration::from_secs(2));
                        let mut changed = false;
                        let current = config_path.metadata().and_then(|m| m.modified()).ok();
                        if current != last_modified {
                            last_modified = current;
                            changed = true;
                        }
                        for (i, path) in included_paths.iter().enumerate() {
                            let current = path.metadata().and_then(|m| m.modified()).ok();
                            if current != included_mtimes[i] {
                                included_mtimes[i] = current;
                                changed = true;
                            }
                        }
                        if changed && file_proxy2.send_event(GuiEvent::FileReload).is_err() {
                            return;
                        }
                    }
                });
                None
            }
        }
    };

    event_loop.run_app(&mut app_handler)?;
    Ok(())
}
