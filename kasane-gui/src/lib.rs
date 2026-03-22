pub mod animation;
mod app;
pub(crate) mod backend;
pub mod colors;
mod diagnostics_overlay;
pub mod gpu;
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
}

fn spawn_session_reader<R>(
    session_id: SessionId,
    reader: R,
    proxy: winit::event_loop::EventLoopProxy<GuiEvent>,
) where
    R: std::io::BufRead + Send + 'static,
{
    let died_proxy = proxy.clone();
    kasane_core::io::spawn_kak_reader(
        reader,
        move |req| {
            if proxy
                .send_event(GuiEvent::Kakoune(session_id, req))
                .is_err()
            {
                tracing::error!("[reader] event loop closed");
            }
        },
        move || {
            let _ = died_proxy.send_event(GuiEvent::KakouneDied(session_id));
        },
    );
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
    mut plugin_manager: PluginManager,
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
    let process_dispatcher = create_process_dispatcher(process_sink);

    // Kakoune reader thread: forward JSON-RPC messages into the winit event loop
    spawn_session_reader(active_session, kak_reader, proxy.clone());

    let mut app_handler = app::App::new(
        config,
        session_manager,
        spawn_session,
        proxy,
        &mut plugin_manager,
        registry,
        process_dispatcher,
    )?;
    event_loop.run_app(&mut app_handler)?;
    Ok(())
}
