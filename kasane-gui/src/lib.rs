mod app;
pub mod backend;
mod colors;
pub mod gpu;
pub mod input;

use anyhow::Result;
use kasane_core::config::Config;
use kasane_core::protocol::KakouneRequest;
use winit::event_loop::EventLoop;

/// Events injected into the winit event loop from background threads.
#[derive(Debug)]
pub enum GuiEvent {
    Kakoune(KakouneRequest),
    KakouneDied,
}

/// Launch the GUI backend. Called from `kasane --ui gui`.
///
/// `session`: optional Kakoune session name (`-c <session>`).
/// `kak_args`: remaining arguments forwarded to `kak`.
/// `spawn_kakoune`: closure that spawns/connects to Kakoune and returns (reader, writer, child).
pub fn run_gui<R, W, C>(
    config: Config,
    spawn_kakoune: impl FnOnce() -> Result<(R, W, C)>,
) -> Result<()>
where
    R: std::io::BufRead + Send + 'static,
    W: std::io::Write + Send + 'static,
    C: Send + 'static,
{
    let event_loop = EventLoop::<GuiEvent>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();

    let (mut kak_reader, kak_writer, _kak_child) = spawn_kakoune()?;

    // Kakoune reader thread: forward JSON-RPC messages into the winit event loop
    let kak_proxy = proxy.clone();
    std::thread::spawn(move || {
        tracing::info!("[reader] kakoune reader thread started");
        let mut buf = String::new();
        loop {
            buf.clear();
            match read_line(&mut kak_reader, &mut buf) {
                Ok(0) => {
                    tracing::info!("[reader] EOF from kakoune");
                    let _ = kak_proxy.send_event(GuiEvent::KakouneDied);
                    return;
                }
                Ok(n) => {
                    let trimmed = buf.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    tracing::debug!("[reader] got {n} bytes from kakoune");
                    let mut bytes = trimmed.as_bytes().to_vec();
                    match kasane_core::protocol::parse_request(&mut bytes) {
                        Ok(req) => {
                            tracing::debug!("[reader] parsed request, sending to event loop");
                            if kak_proxy.send_event(GuiEvent::Kakoune(req)).is_err() {
                                tracing::error!("[reader] event loop closed");
                                return;
                            }
                        }
                        Err(e) => {
                            tracing::warn!("failed to parse kak message: {e}");
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("kak stdout read error: {e}");
                    let _ = kak_proxy.send_event(GuiEvent::KakouneDied);
                    return;
                }
            }
        }
    });

    let mut app_handler = app::App::new(config, kak_writer);
    event_loop.run_app(&mut app_handler)?;
    Ok(())
}

fn read_line(reader: &mut impl std::io::BufRead, buf: &mut String) -> std::io::Result<usize> {
    reader.read_line(buf)
}
