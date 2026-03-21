//! I/O event types for plugin process execution (Phase P-2).
//!
//! These types define the host-mediated I/O model where plugins request
//! process operations via `Command` variants, and receive results via
//! `IoEvent` delivered through `Plugin::on_io_event()`.

use super::PluginId;

/// I/O event delivered to plugins via `on_io_event()`.
#[derive(Debug, Clone)]
pub enum IoEvent {
    Process(ProcessEvent),
    // Future: Http(HttpResponse), FileWatch(FileWatchEvent)
}

/// Events from a spawned child process.
#[derive(Debug, Clone)]
pub enum ProcessEvent {
    /// Data received on the child's stdout.
    Stdout { job_id: u64, data: Vec<u8> },
    /// Data received on the child's stderr.
    Stderr { job_id: u64, data: Vec<u8> },
    /// The child process has exited.
    Exited { job_id: u64, exit_code: i32 },
    /// Process spawn failed (e.g., program not found).
    SpawnFailed { job_id: u64, error: String },
}

/// Whether stdin should be available for writing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdinMode {
    /// stdin is /dev/null — no writing possible.
    Null,
    /// stdin is piped — host can write data to the child.
    Piped,
    /// Spawn the process in a pseudo-terminal.
    /// The process receives stdin/stdout/stderr through the PTY.
    /// ANSI escape sequences are passed through in ProcessEvent::Stdout.
    Pty {
        /// Initial terminal dimensions.
        rows: u16,
        cols: u16,
    },
}

/// Abstraction for sending process events from ProcessManager to the event loop.
///
/// TUI and GUI backends implement this with their specific Event types.
pub trait ProcessEventSink: Send + Sync + 'static {
    fn send_process_output(&self, plugin_id: PluginId, event: IoEvent);
}

/// Abstraction for dispatching process commands from the event loop.
///
/// kasane-core uses this trait so it doesn't depend on tokio or ProcessManager directly.
pub trait ProcessDispatcher {
    fn spawn(
        &mut self,
        plugin_id: &PluginId,
        job_id: u64,
        program: &str,
        args: &[String],
        stdin_mode: StdinMode,
    );
    fn write(&mut self, plugin_id: &PluginId, job_id: u64, data: &[u8]);
    fn close_stdin(&mut self, plugin_id: &PluginId, job_id: u64);
    fn kill(&mut self, plugin_id: &PluginId, job_id: u64);
    /// Resize the PTY of a spawned process.
    /// No-op if the process was not spawned with `StdinMode::Pty`.
    fn resize_pty(&mut self, plugin_id: &PluginId, job_id: u64, rows: u16, cols: u16);
    /// Remove a finished job from tracking after its Exited or SpawnFailed event
    /// has been delivered. This frees the per-plugin process count slot.
    fn remove_finished_job(&mut self, plugin_id: &PluginId, job_id: u64);
}

/// No-op ProcessDispatcher for contexts where process execution is not available.
pub struct NullProcessDispatcher;

impl ProcessDispatcher for NullProcessDispatcher {
    fn spawn(
        &mut self,
        _plugin_id: &PluginId,
        _job_id: u64,
        _program: &str,
        _args: &[String],
        _stdin_mode: StdinMode,
    ) {
    }
    fn write(&mut self, _plugin_id: &PluginId, _job_id: u64, _data: &[u8]) {}
    fn close_stdin(&mut self, _plugin_id: &PluginId, _job_id: u64) {}
    fn kill(&mut self, _plugin_id: &PluginId, _job_id: u64) {}
    fn resize_pty(&mut self, _plugin_id: &PluginId, _job_id: u64, _rows: u16, _cols: u16) {}
    fn remove_finished_job(&mut self, _plugin_id: &PluginId, _job_id: u64) {}
}
