pub mod animation;
mod app;
pub(crate) mod backend;
pub mod colors;
pub mod gpu;
pub(crate) mod input;

use std::sync::Arc;

use anyhow::Result;
use kasane_core::config::Config;
use kasane_core::plugin::{IoEvent, PluginId, ProcessEventSink};
use kasane_core::protocol::KakouneRequest;
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
    Kakoune(KakouneRequest),
    KakouneDied,
    PluginTimer(PluginId, TimerPayload),
    ProcessOutput(PluginId, IoEvent),
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
/// `spawn_kakoune`: closure that spawns/connects to Kakoune and returns (reader, writer, child).
/// `create_process_dispatcher`: factory that receives a `ProcessEventSink` and returns
///   a `ProcessDispatcher` for plugin-spawned processes.
pub fn run_gui<R, W, C>(
    config: Config,
    spawn_kakoune: impl FnOnce() -> Result<(R, W, C)>,
    register_plugins: impl FnOnce(&mut kasane_core::plugin::PluginRegistry),
    create_process_dispatcher: impl FnOnce(
        Arc<dyn ProcessEventSink>,
    ) -> Box<dyn kasane_core::plugin::ProcessDispatcher>,
) -> Result<()>
where
    R: std::io::BufRead + Send + 'static,
    W: std::io::Write + Send + 'static,
    C: Send + 'static,
{
    let event_loop = EventLoop::<GuiEvent>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();

    let (kak_reader, kak_writer, _kak_child) = spawn_kakoune()?;

    // Build plugin registry
    let mut registry = kasane_core::plugin::PluginRegistry::new();
    register_plugins(&mut registry);

    // Process dispatcher for plugin-spawned processes
    let process_sink: Arc<dyn ProcessEventSink> = Arc::new(GuiProcessEventSink(proxy.clone()));
    let process_dispatcher = create_process_dispatcher(process_sink);

    // Kakoune reader thread: forward JSON-RPC messages into the winit event loop
    let kak_proxy = proxy.clone();
    kasane_core::io::spawn_kak_reader(
        kak_reader,
        move |req| {
            if kak_proxy.send_event(GuiEvent::Kakoune(req)).is_err() {
                tracing::error!("[reader] event loop closed");
            }
        },
        {
            let died_proxy = proxy.clone();
            move || {
                let _ = died_proxy.send_event(GuiEvent::KakouneDied);
            }
        },
    );

    let mut app_handler = app::App::new(config, kak_writer, proxy, registry, process_dispatcher);
    event_loop.run_app(&mut app_handler)?;
    Ok(())
}
