use std::io::Write;

use crate::protocol::{KakouneRequest, KasaneRequest};

/// Send a KasaneRequest as JSON to the Kakoune process.
pub fn send_request(writer: &mut (impl Write + ?Sized), req: &KasaneRequest) {
    let json = req.to_json();
    tracing::debug!(%json, "send_request");
    let _ = writeln!(writer, "{}", json);
    let _ = writer.flush();
}

/// Send the initial resize request to Kakoune (once only).
///
/// Sets `*sent = true` after the first call so subsequent calls are no-ops.
pub fn send_initial_resize(writer: &mut impl Write, sent: &mut bool, rows: u16, cols: u16) {
    if *sent {
        return;
    }
    *sent = true;
    send_request(
        writer,
        &KasaneRequest::Resize {
            rows: rows.saturating_sub(1),
            cols,
        },
    );
}

/// Spawn a background thread that reads JSON-RPC messages from a Kakoune process.
///
/// Reads lines from `reader`, parses each as a `KakouneRequest`, and calls
/// `on_request` for each parsed message. When the reader reaches EOF or an
/// I/O error occurs, `on_died` is called before the thread exits.
pub fn spawn_kak_reader<R, F, D>(mut reader: R, on_request: F, on_died: D)
where
    R: std::io::BufRead + Send + 'static,
    F: Fn(KakouneRequest) + Send + 'static,
    D: Fn() + Send + 'static,
{
    std::thread::spawn(move || {
        let mut buf = String::new();
        loop {
            buf.clear();
            match reader.read_line(&mut buf) {
                Ok(0) => {
                    tracing::info!("[reader] EOF from kakoune");
                    on_died();
                    return;
                }
                Ok(_) => {
                    let trimmed = buf.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let mut bytes = trimmed.as_bytes().to_vec();
                    match crate::protocol::parse_request(&mut bytes) {
                        Ok(req) => on_request(req),
                        Err(e) => tracing::warn!("failed to parse kak message: {e}"),
                    }
                }
                Err(e) => {
                    tracing::error!("kak stdout read error: {e}");
                    on_died();
                    return;
                }
            }
        }
    });
}
