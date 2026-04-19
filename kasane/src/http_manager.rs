//! HTTP request manager for plugin-initiated HTTP calls (Phase 1A).
//!
//! Uses reqwest + tokio for async HTTP request management. Each request gets
//! a management task that executes the request and forwards events to the
//! event loop via `ProcessEventSink`.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;

use kasane_core::plugin::{
    HttpDispatcher, HttpEvent, HttpMethod, HttpRequestConfig, IoEvent, PluginId, ProcessEventSink,
    StreamingMode,
};

const MAX_HTTP_REQUESTS_PER_PLUGIN: usize = 8;
const MAX_HTTP_REQUESTS_TOTAL: usize = 32;

/// Handle for a running HTTP request.
struct RequestHandle {
    abort_handle: tokio::task::AbortHandle,
}

/// Manages plugin-initiated HTTP requests using a tokio runtime.
pub struct HttpManager {
    rt: tokio::runtime::Handle,
    client: reqwest::Client,
    jobs: HashMap<(PluginId, u64), RequestHandle>,
    per_plugin_count: HashMap<PluginId, usize>,
    sink: Arc<dyn ProcessEventSink>,
}

impl HttpManager {
    pub fn new(rt: tokio::runtime::Handle, sink: Arc<dyn ProcessEventSink>) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("kasane-plugin/0.1")
            .build()
            .expect("failed to build reqwest client");
        Self {
            rt,
            client,
            jobs: HashMap::new(),
            per_plugin_count: HashMap::new(),
            sink,
        }
    }

    fn send_error(&self, plugin_id: &PluginId, job_id: u64, error: String) {
        self.sink.send_process_output(
            plugin_id.clone(),
            IoEvent::Http(HttpEvent::Error { job_id, error }),
        );
    }

    fn decrement_plugin_count(&mut self, plugin_id: &PluginId) {
        if let Some(count) = self.per_plugin_count.get_mut(plugin_id) {
            *count = count.saturating_sub(1);
        }
    }

    fn start_request(&mut self, plugin_id: &PluginId, job_id: u64, config: HttpRequestConfig) {
        let key = (plugin_id.clone(), job_id);

        // Check total limit
        if self.jobs.len() >= MAX_HTTP_REQUESTS_TOTAL {
            self.send_error(
                plugin_id,
                job_id,
                format!("total HTTP request limit reached ({MAX_HTTP_REQUESTS_TOTAL})"),
            );
            return;
        }

        // Check per-plugin limit
        let count = self.per_plugin_count.get(plugin_id).copied().unwrap_or(0);
        if count >= MAX_HTTP_REQUESTS_PER_PLUGIN {
            self.send_error(
                plugin_id,
                job_id,
                format!("per-plugin HTTP request limit reached ({MAX_HTTP_REQUESTS_PER_PLUGIN})"),
            );
            return;
        }

        // Duplicate check
        if self.jobs.contains_key(&key) {
            self.send_error(plugin_id, job_id, format!("job_id {job_id} already in use"));
            return;
        }

        // URL security check
        if let Err(e) = validate_url(&config.url) {
            self.send_error(plugin_id, job_id, e);
            return;
        }

        let client = self.client.clone();
        let sink = self.sink.clone();
        let pid = plugin_id.clone();
        let streaming = config.streaming;

        let join_handle = self.rt.spawn(async move {
            // Build request
            let method = match config.method {
                HttpMethod::Get => reqwest::Method::GET,
                HttpMethod::Post => reqwest::Method::POST,
                HttpMethod::Put => reqwest::Method::PUT,
                HttpMethod::Delete => reqwest::Method::DELETE,
                HttpMethod::Patch => reqwest::Method::PATCH,
                HttpMethod::Head => reqwest::Method::HEAD,
            };

            let mut builder = client.request(method, &config.url);

            for (k, v) in &config.headers {
                builder = builder.header(k.as_str(), v.as_str());
            }

            if let Some(body) = config.body {
                builder = builder.body(body);
            }

            if config.timeout_ms > 0 {
                builder =
                    builder.timeout(std::time::Duration::from_millis(config.timeout_ms as u64));
            }

            // Execute request
            let response = match builder.send().await {
                Ok(resp) => resp,
                Err(e) => {
                    sink.send_process_output(
                        pid,
                        IoEvent::Http(HttpEvent::Error {
                            job_id,
                            error: e.to_string(),
                        }),
                    );
                    return;
                }
            };

            let status = response.status().as_u16();
            let headers: Vec<(String, String)> = response
                .headers()
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                .collect();

            match streaming {
                StreamingMode::Buffered => match response.bytes().await {
                    Ok(body) => {
                        sink.send_process_output(
                            pid,
                            IoEvent::Http(HttpEvent::Response {
                                job_id,
                                status,
                                headers,
                                body: body.to_vec(),
                            }),
                        );
                    }
                    Err(e) => {
                        sink.send_process_output(
                            pid,
                            IoEvent::Http(HttpEvent::Error {
                                job_id,
                                error: e.to_string(),
                            }),
                        );
                    }
                },
                StreamingMode::Chunked => {
                    use futures_util::StreamExt;

                    let idle_timeout = if config.idle_timeout_ms > 0 {
                        std::time::Duration::from_millis(config.idle_timeout_ms as u64)
                    } else {
                        std::time::Duration::from_secs(30)
                    };

                    let mut stream = response.bytes_stream();

                    loop {
                        let chunk = tokio::time::timeout(idle_timeout, stream.next()).await;
                        match chunk {
                            Ok(Some(Ok(data))) => {
                                sink.send_process_output(
                                    pid.clone(),
                                    IoEvent::Http(HttpEvent::Chunk {
                                        job_id,
                                        data: data.to_vec(),
                                    }),
                                );
                            }
                            Ok(Some(Err(e))) => {
                                sink.send_process_output(
                                    pid,
                                    IoEvent::Http(HttpEvent::Error {
                                        job_id,
                                        error: e.to_string(),
                                    }),
                                );
                                return;
                            }
                            Ok(None) => {
                                // Stream ended
                                sink.send_process_output(
                                    pid,
                                    IoEvent::Http(HttpEvent::StreamEnd { job_id }),
                                );
                                return;
                            }
                            Err(_) => {
                                // Idle timeout
                                sink.send_process_output(
                                    pid,
                                    IoEvent::Http(HttpEvent::Error {
                                        job_id,
                                        error: "idle timeout".to_string(),
                                    }),
                                );
                                return;
                            }
                        }
                    }
                }
            }
        });

        let abort_handle = join_handle.abort_handle();
        self.jobs.insert(key, RequestHandle { abort_handle });
        *self.per_plugin_count.entry(plugin_id.clone()).or_insert(0) += 1;
    }

    fn cancel_request(&mut self, plugin_id: &PluginId, job_id: u64) {
        let key = (plugin_id.clone(), job_id);
        if let Some(handle) = self.jobs.remove(&key) {
            handle.abort_handle.abort();
            self.decrement_plugin_count(plugin_id);
        }
    }

    /// Remove a completed request from tracking (called after event delivery).
    pub fn remove_finished_job(&mut self, plugin_id: &PluginId, job_id: u64) {
        let key = (plugin_id.clone(), job_id);
        if self.jobs.remove(&key).is_some() {
            self.decrement_plugin_count(plugin_id);
        }
    }

    /// Shut down all running requests.
    pub fn shutdown(&mut self) {
        for (_, handle) in self.jobs.drain() {
            handle.abort_handle.abort();
        }
        self.per_plugin_count.clear();
    }
}

impl HttpDispatcher for HttpManager {
    fn request(&mut self, plugin_id: &PluginId, job_id: u64, config: HttpRequestConfig) {
        self.start_request(plugin_id, job_id, config);
    }

    fn cancel(&mut self, plugin_id: &PluginId, job_id: u64) {
        self.cancel_request(plugin_id, job_id);
    }
}

/// Validate that a URL is safe for plugin HTTP requests.
///
/// Rejects:
/// - Non-HTTP(S) schemes
/// - Localhost/loopback addresses
/// - Link-local addresses (169.254.x.x)
/// - Private network addresses (10.x.x.x, 172.16-31.x.x, 192.168.x.x)
fn validate_url(url: &str) -> Result<(), String> {
    let parsed = url::Url::parse(url).map_err(|e| format!("invalid URL: {e}"))?;

    match parsed.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(format!(
                "unsupported scheme: {scheme} (only http/https allowed)"
            ));
        }
    }

    if let Some(host) = parsed.host_str() {
        // Check for localhost aliases
        if host == "localhost" || host == "[::1]" {
            return Err("requests to localhost are not allowed".to_string());
        }

        // Parse as IP address and check ranges
        if let Ok(ip) = host.parse::<IpAddr>() {
            if ip.is_loopback() {
                return Err("requests to loopback addresses are not allowed".to_string());
            }
            match ip {
                IpAddr::V4(v4) => {
                    let octets = v4.octets();
                    // Link-local: 169.254.x.x
                    if octets[0] == 169 && octets[1] == 254 {
                        return Err("requests to link-local addresses are not allowed".to_string());
                    }
                    // Private: 10.x.x.x
                    if octets[0] == 10 {
                        return Err(
                            "requests to private network addresses are not allowed".to_string()
                        );
                    }
                    // Private: 172.16.0.0 - 172.31.255.255
                    if octets[0] == 172 && (16..=31).contains(&octets[1]) {
                        return Err(
                            "requests to private network addresses are not allowed".to_string()
                        );
                    }
                    // Private: 192.168.x.x
                    if octets[0] == 192 && octets[1] == 168 {
                        return Err(
                            "requests to private network addresses are not allowed".to_string()
                        );
                    }
                }
                IpAddr::V6(_) => {
                    // IPv6 loopback already handled above
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_url_allows_https() {
        assert!(validate_url("https://api.example.com/v1/data").is_ok());
    }

    #[test]
    fn validate_url_allows_http() {
        assert!(validate_url("http://api.example.com/v1/data").is_ok());
    }

    #[test]
    fn validate_url_rejects_localhost() {
        assert!(validate_url("http://localhost:8080/api").is_err());
        assert!(validate_url("http://127.0.0.1:8080/api").is_err());
        assert!(validate_url("http://[::1]:8080/api").is_err());
    }

    #[test]
    fn validate_url_rejects_private_networks() {
        assert!(validate_url("http://10.0.0.1/api").is_err());
        assert!(validate_url("http://172.16.0.1/api").is_err());
        assert!(validate_url("http://192.168.1.1/api").is_err());
    }

    #[test]
    fn validate_url_rejects_link_local() {
        assert!(validate_url("http://169.254.1.1/api").is_err());
    }

    #[test]
    fn validate_url_rejects_non_http_schemes() {
        assert!(validate_url("ftp://example.com/file").is_err());
        assert!(validate_url("file:///etc/passwd").is_err());
    }

    #[test]
    fn validate_url_rejects_invalid_urls() {
        assert!(validate_url("not a url").is_err());
    }

    #[test]
    fn per_plugin_limit_enforced() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let sink = Arc::new(TestSink::new());
        let mut mgr = HttpManager::new(rt.handle().clone(), sink.clone());
        let pid = PluginId("test".to_string());

        // Saturate the per-plugin limit
        for i in 0..MAX_HTTP_REQUESTS_PER_PLUGIN {
            // Use a URL that passes validation but won't actually connect
            mgr.start_request(
                &pid,
                i as u64,
                HttpRequestConfig {
                    url: "https://httpbin.org/delay/999".to_string(),
                    method: HttpMethod::Get,
                    headers: vec![],
                    body: None,
                    timeout_ms: 1000,
                    idle_timeout_ms: 0,
                    streaming: StreamingMode::Buffered,
                },
            );
        }

        // Next should fail
        mgr.start_request(
            &pid,
            MAX_HTTP_REQUESTS_PER_PLUGIN as u64,
            HttpRequestConfig {
                url: "https://httpbin.org/get".to_string(),
                method: HttpMethod::Get,
                headers: vec![],
                body: None,
                timeout_ms: 1000,
                idle_timeout_ms: 0,
                streaming: StreamingMode::Buffered,
            },
        );

        let events = sink.events();
        let has_limit_error = events.iter().any(|(_, e)| {
            matches!(
                e,
                IoEvent::Http(HttpEvent::Error { error, .. }) if error.contains("per-plugin")
            )
        });
        assert!(
            has_limit_error,
            "expected per-plugin limit error, got {events:?}"
        );

        mgr.shutdown();
    }

    #[test]
    fn duplicate_job_id_fails() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let sink = Arc::new(TestSink::new());
        let mut mgr = HttpManager::new(rt.handle().clone(), sink.clone());
        let pid = PluginId("test".to_string());

        let config = HttpRequestConfig {
            url: "https://httpbin.org/delay/999".to_string(),
            method: HttpMethod::Get,
            headers: vec![],
            body: None,
            timeout_ms: 1000,
            idle_timeout_ms: 0,
            streaming: StreamingMode::Buffered,
        };

        mgr.start_request(&pid, 1, config.clone());
        mgr.start_request(&pid, 1, config);

        let events = sink.events();
        let has_dup_error = events.iter().any(|(_, e)| {
            matches!(
                e,
                IoEvent::Http(HttpEvent::Error { error, .. }) if error.contains("already in use")
            )
        });
        assert!(
            has_dup_error,
            "expected duplicate job_id error, got {events:?}"
        );

        mgr.shutdown();
    }

    #[test]
    fn cancel_removes_job() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let sink = Arc::new(TestSink::new());
        let mut mgr = HttpManager::new(rt.handle().clone(), sink);
        let pid = PluginId("test".to_string());

        mgr.start_request(
            &pid,
            1,
            HttpRequestConfig {
                url: "https://httpbin.org/delay/999".to_string(),
                method: HttpMethod::Get,
                headers: vec![],
                body: None,
                timeout_ms: 30000,
                idle_timeout_ms: 0,
                streaming: StreamingMode::Buffered,
            },
        );

        assert!(mgr.jobs.contains_key(&(pid.clone(), 1)));
        mgr.cancel_request(&pid, 1);
        assert!(!mgr.jobs.contains_key(&(pid.clone(), 1)));
    }

    // Test sink for collecting events
    struct TestSink {
        events: std::sync::Mutex<Vec<(PluginId, IoEvent)>>,
    }

    impl TestSink {
        fn new() -> Self {
            Self {
                events: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn events(&self) -> Vec<(PluginId, IoEvent)> {
            self.events.lock().unwrap().clone()
        }
    }

    impl ProcessEventSink for TestSink {
        fn send_process_output(&self, plugin_id: PluginId, event: IoEvent) {
            self.events.lock().unwrap().push((plugin_id, event));
        }
    }
}
