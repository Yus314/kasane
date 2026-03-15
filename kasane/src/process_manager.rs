//! Process execution manager for plugin-spawned processes (Phase P-2).
//!
//! Uses tokio for async process management. Each spawned process gets a
//! management task that reads stdout/stderr and forwards events to the
//! event loop via `ProcessEventSink`.

use std::collections::HashMap;
use std::sync::Arc;

use kasane_core::plugin::{
    IoEvent, PluginId, ProcessDispatcher, ProcessEvent, ProcessEventSink, StdinMode,
};

const MAX_PROCESSES_PER_PLUGIN: usize = 4;
const MAX_PROCESSES_TOTAL: usize = 16;
const READ_BUF_SIZE: usize = 8192;

/// Handle for a running child process.
struct JobHandle {
    stdin_tx: Option<tokio::sync::mpsc::Sender<StdinCommand>>,
    abort_handle: tokio::task::AbortHandle,
}

enum StdinCommand {
    Write(Vec<u8>),
}

/// Manages plugin-spawned child processes using a tokio runtime.
pub struct ProcessManager {
    rt: tokio::runtime::Handle,
    jobs: HashMap<(PluginId, u64), JobHandle>,
    /// Count of jobs per plugin for limit enforcement.
    per_plugin_count: HashMap<PluginId, usize>,
    sink: Arc<dyn ProcessEventSink>,
}

impl ProcessManager {
    pub fn new(rt: tokio::runtime::Handle, sink: Arc<dyn ProcessEventSink>) -> Self {
        Self {
            rt,
            jobs: HashMap::new(),
            per_plugin_count: HashMap::new(),
            sink,
        }
    }

    /// Spawn a child process. Events are sent to the sink.
    fn spawn_process(
        &mut self,
        plugin_id: &PluginId,
        job_id: u64,
        program: &str,
        args: &[String],
        stdin_mode: StdinMode,
    ) {
        let key = (plugin_id.clone(), job_id);

        // Check limits
        if self.jobs.len() >= MAX_PROCESSES_TOTAL {
            self.sink.send_process_output(
                plugin_id.clone(),
                IoEvent::Process(ProcessEvent::SpawnFailed {
                    job_id,
                    error: format!("total process limit reached ({MAX_PROCESSES_TOTAL})"),
                }),
            );
            return;
        }

        let count = self.per_plugin_count.get(plugin_id).copied().unwrap_or(0);
        if count >= MAX_PROCESSES_PER_PLUGIN {
            self.sink.send_process_output(
                plugin_id.clone(),
                IoEvent::Process(ProcessEvent::SpawnFailed {
                    job_id,
                    error: format!("per-plugin process limit reached ({MAX_PROCESSES_PER_PLUGIN})"),
                }),
            );
            return;
        }

        // Duplicate check
        if self.jobs.contains_key(&key) {
            self.sink.send_process_output(
                plugin_id.clone(),
                IoEvent::Process(ProcessEvent::SpawnFailed {
                    job_id,
                    error: format!("job_id {job_id} already in use"),
                }),
            );
            return;
        }

        let mut cmd = tokio::process::Command::new(program);
        cmd.args(args);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        match stdin_mode {
            StdinMode::Piped => {
                cmd.stdin(std::process::Stdio::piped());
            }
            StdinMode::Null => {
                cmd.stdin(std::process::Stdio::null());
            }
        }

        let sink = self.sink.clone();
        let pid = plugin_id.clone();

        let (stdin_tx, stdin_rx) = if stdin_mode == StdinMode::Piped {
            let (tx, rx) = tokio::sync::mpsc::channel::<StdinCommand>(32);
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        let join_handle = self.rt.spawn(async move {
            let spawn_result = cmd.spawn();
            let mut child = match spawn_result {
                Ok(child) => child,
                Err(e) => {
                    sink.send_process_output(
                        pid,
                        IoEvent::Process(ProcessEvent::SpawnFailed {
                            job_id,
                            error: e.to_string(),
                        }),
                    );
                    return;
                }
            };

            let mut stdout = child.stdout.take();
            let mut stderr = child.stderr.take();
            let mut child_stdin = child.stdin.take();
            let mut stdin_rx = stdin_rx;

            let mut stdout_buf = vec![0u8; READ_BUF_SIZE];
            let mut stderr_buf = vec![0u8; READ_BUF_SIZE];
            let mut stdout_done = stdout.is_none();
            let mut stderr_done = stderr.is_none();

            loop {
                tokio::select! {
                    result = async {
                        if let Some(ref mut s) = stdout {
                            use tokio::io::AsyncReadExt;
                            s.read(&mut stdout_buf).await
                        } else {
                            std::future::pending().await
                        }
                    }, if !stdout_done => {
                        match result {
                            Ok(0) => {
                                stdout_done = true;
                                stdout = None;
                            }
                            Ok(n) => {
                                sink.send_process_output(
                                    pid.clone(),
                                    IoEvent::Process(ProcessEvent::Stdout {
                                        job_id,
                                        data: stdout_buf[..n].to_vec(),
                                    }),
                                );
                            }
                            Err(_) => {
                                stdout_done = true;
                                stdout = None;
                            }
                        }
                    }
                    result = async {
                        if let Some(ref mut s) = stderr {
                            use tokio::io::AsyncReadExt;
                            s.read(&mut stderr_buf).await
                        } else {
                            std::future::pending().await
                        }
                    }, if !stderr_done => {
                        match result {
                            Ok(0) => {
                                stderr_done = true;
                                stderr = None;
                            }
                            Ok(n) => {
                                sink.send_process_output(
                                    pid.clone(),
                                    IoEvent::Process(ProcessEvent::Stderr {
                                        job_id,
                                        data: stderr_buf[..n].to_vec(),
                                    }),
                                );
                            }
                            Err(_) => {
                                stderr_done = true;
                                stderr = None;
                            }
                        }
                    }
                    cmd = async {
                        if let Some(ref mut rx) = stdin_rx {
                            rx.recv().await
                        } else {
                            std::future::pending().await
                        }
                    } => {
                        match cmd {
                            Some(StdinCommand::Write(data)) => {
                                if let Some(ref mut stdin) = child_stdin {
                                    use tokio::io::AsyncWriteExt;
                                    let _ = stdin.write_all(&data).await;
                                }
                            }
                            None => {
                                // Sender dropped (stdin closed)
                                child_stdin = None;
                                stdin_rx = None;
                            }
                        }
                    }
                    status = child.wait(), if stdout_done && stderr_done => {
                        let exit_code = match status {
                            Ok(s) => s.code().unwrap_or(-1),
                            Err(_) => -1,
                        };
                        sink.send_process_output(
                            pid,
                            IoEvent::Process(ProcessEvent::Exited { job_id, exit_code }),
                        );
                        break;
                    }
                }
            }
        });

        let abort_handle = join_handle.abort_handle();

        self.jobs.insert(
            key,
            JobHandle {
                stdin_tx,
                abort_handle,
            },
        );
        *self.per_plugin_count.entry(plugin_id.clone()).or_insert(0) += 1;
    }

    fn write_to_process(&self, plugin_id: &PluginId, job_id: u64, data: &[u8]) {
        let key = (plugin_id.clone(), job_id);
        if let Some(handle) = self.jobs.get(&key)
            && let Some(ref tx) = handle.stdin_tx
        {
            let _ = tx.try_send(StdinCommand::Write(data.to_vec()));
        }
    }

    fn close_process_stdin(&mut self, plugin_id: &PluginId, job_id: u64) {
        let key = (plugin_id.clone(), job_id);
        if let Some(handle) = self.jobs.get_mut(&key) {
            handle.stdin_tx = None;
        }
    }

    fn kill_process(&mut self, plugin_id: &PluginId, job_id: u64) {
        let key = (plugin_id.clone(), job_id);
        if let Some(handle) = self.jobs.remove(&key) {
            handle.abort_handle.abort();
            if let Some(count) = self.per_plugin_count.get_mut(plugin_id) {
                *count = count.saturating_sub(1);
            }
        }
    }

    /// Shut down all running processes.
    pub fn shutdown(&mut self) {
        for ((plugin_id, _), handle) in self.jobs.drain() {
            handle.abort_handle.abort();
            if let Some(count) = self.per_plugin_count.get_mut(&plugin_id) {
                *count = count.saturating_sub(1);
            }
        }
        self.per_plugin_count.clear();
    }

    /// Remove finished jobs from tracking (called when Exited/SpawnFailed is delivered).
    pub fn remove_finished_job(&mut self, plugin_id: &PluginId, job_id: u64) {
        let key = (plugin_id.clone(), job_id);
        if self.jobs.remove(&key).is_some()
            && let Some(count) = self.per_plugin_count.get_mut(plugin_id)
        {
            *count = count.saturating_sub(1);
        }
    }
}

impl ProcessDispatcher for ProcessManager {
    fn spawn(
        &mut self,
        plugin_id: &PluginId,
        job_id: u64,
        program: &str,
        args: &[String],
        stdin_mode: StdinMode,
    ) {
        self.spawn_process(plugin_id, job_id, program, args, stdin_mode);
    }

    fn write(&mut self, plugin_id: &PluginId, job_id: u64, data: &[u8]) {
        self.write_to_process(plugin_id, job_id, data);
    }

    fn close_stdin(&mut self, plugin_id: &PluginId, job_id: u64) {
        self.close_process_stdin(plugin_id, job_id);
    }

    fn kill(&mut self, plugin_id: &PluginId, job_id: u64) {
        self.kill_process(plugin_id, job_id);
    }

    fn remove_finished_job(&mut self, plugin_id: &PluginId, job_id: u64) {
        ProcessManager::remove_finished_job(self, plugin_id, job_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct TestSink {
        events: Arc<Mutex<Vec<(PluginId, IoEvent)>>>,
    }

    impl TestSink {
        fn new() -> Self {
            Self {
                events: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn events(&self) -> Vec<(PluginId, IoEvent)> {
            self.events.lock().unwrap().clone()
        }

        fn wait_for_events(&self, count: usize, timeout_ms: u64) -> Vec<(PluginId, IoEvent)> {
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
            loop {
                let events = self.events.lock().unwrap();
                if events.len() >= count || std::time::Instant::now() >= deadline {
                    return events.clone();
                }
                drop(events);
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    }

    impl kasane_core::plugin::ProcessEventSink for TestSink {
        fn send_process_output(&self, plugin_id: PluginId, event: IoEvent) {
            self.events.lock().unwrap().push((plugin_id, event));
        }
    }

    fn test_plugin_id() -> PluginId {
        PluginId("test_plugin".to_string())
    }

    fn create_manager(sink: TestSink) -> (tokio::runtime::Runtime, ProcessManager) {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let mgr = ProcessManager::new(rt.handle().clone(), Arc::new(sink));
        (rt, mgr)
    }

    #[test]
    fn spawn_echo_receives_stdout() {
        let sink = TestSink::new();
        let (_rt, mut mgr) = create_manager(sink.clone());

        mgr.spawn_process(
            &test_plugin_id(),
            1,
            "echo",
            &["hello".into()],
            StdinMode::Null,
        );

        let events = sink.wait_for_events(2, 3000);
        let has_stdout = events.iter().any(|(_, e)| matches!(e, IoEvent::Process(ProcessEvent::Stdout { data, .. }) if data == b"hello\n"));
        let has_exit = events.iter().any(|(_, e)| {
            matches!(
                e,
                IoEvent::Process(ProcessEvent::Exited { exit_code: 0, .. })
            )
        });
        assert!(has_stdout, "expected stdout event, got {events:?}");
        assert!(has_exit, "expected exit event, got {events:?}");
    }

    #[test]
    fn spawn_stderr_receives_stderr() {
        let sink = TestSink::new();
        let (_rt, mut mgr) = create_manager(sink.clone());

        mgr.spawn_process(
            &test_plugin_id(),
            1,
            "sh",
            &["-c".into(), "echo err >&2".into()],
            StdinMode::Null,
        );

        let events = sink.wait_for_events(2, 3000);
        let has_stderr = events.iter().any(|(_, e)| matches!(e, IoEvent::Process(ProcessEvent::Stderr { data, .. }) if data == b"err\n"));
        assert!(has_stderr, "expected stderr event, got {events:?}");
    }

    #[test]
    fn spawn_nonexistent_program_fails() {
        let sink = TestSink::new();
        let (_rt, mut mgr) = create_manager(sink.clone());

        mgr.spawn_process(
            &test_plugin_id(),
            1,
            "nonexistent_program_kasane_test",
            &[],
            StdinMode::Null,
        );

        let events = sink.wait_for_events(1, 3000);
        let has_failed = events
            .iter()
            .any(|(_, e)| matches!(e, IoEvent::Process(ProcessEvent::SpawnFailed { .. })));
        assert!(has_failed, "expected SpawnFailed event, got {events:?}");
    }

    #[test]
    fn piped_stdin_write_and_close() {
        let sink = TestSink::new();
        let (_rt, mut mgr) = create_manager(sink.clone());

        mgr.spawn_process(&test_plugin_id(), 1, "cat", &[], StdinMode::Piped);

        // Give process time to start
        std::thread::sleep(std::time::Duration::from_millis(100));

        mgr.write_to_process(&test_plugin_id(), 1, b"hello from stdin");
        mgr.close_process_stdin(&test_plugin_id(), 1);

        let events = sink.wait_for_events(2, 3000);
        let has_stdout = events.iter().any(|(_, e)| matches!(e, IoEvent::Process(ProcessEvent::Stdout { data, .. }) if data == b"hello from stdin"));
        let has_exit = events.iter().any(|(_, e)| {
            matches!(
                e,
                IoEvent::Process(ProcessEvent::Exited { exit_code: 0, .. })
            )
        });
        assert!(has_stdout, "expected stdout event, got {events:?}");
        assert!(has_exit, "expected exit event, got {events:?}");
    }

    #[test]
    fn kill_process_terminates() {
        let sink = TestSink::new();
        let (_rt, mut mgr) = create_manager(sink.clone());

        mgr.spawn_process(
            &test_plugin_id(),
            1,
            "sleep",
            &["60".into()],
            StdinMode::Null,
        );

        // Give process time to start
        std::thread::sleep(std::time::Duration::from_millis(100));

        mgr.kill_process(&test_plugin_id(), 1);

        // The task was aborted, so the job is removed
        assert!(!mgr.jobs.contains_key(&(test_plugin_id(), 1)));
    }

    #[test]
    fn per_plugin_limit_enforced() {
        let sink = TestSink::new();
        let (_rt, mut mgr) = create_manager(sink.clone());

        // Spawn MAX_PROCESSES_PER_PLUGIN processes
        for i in 0..MAX_PROCESSES_PER_PLUGIN {
            mgr.spawn_process(
                &test_plugin_id(),
                i as u64,
                "sleep",
                &["60".into()],
                StdinMode::Null,
            );
        }

        // The next spawn should fail
        mgr.spawn_process(
            &test_plugin_id(),
            MAX_PROCESSES_PER_PLUGIN as u64,
            "sleep",
            &["60".into()],
            StdinMode::Null,
        );

        let events = sink.events();
        let has_limit_error = events.iter().any(|(_, e)| {
            matches!(e, IoEvent::Process(ProcessEvent::SpawnFailed { error, .. }) if error.contains("per-plugin"))
        });
        assert!(
            has_limit_error,
            "expected per-plugin limit error, got {events:?}"
        );

        mgr.shutdown();
    }

    #[test]
    fn exit_code_nonzero() {
        let sink = TestSink::new();
        let (_rt, mut mgr) = create_manager(sink.clone());

        mgr.spawn_process(
            &test_plugin_id(),
            1,
            "sh",
            &["-c".into(), "exit 42".into()],
            StdinMode::Null,
        );

        let events = sink.wait_for_events(1, 3000);
        let has_exit = events.iter().any(|(_, e)| {
            matches!(
                e,
                IoEvent::Process(ProcessEvent::Exited { exit_code: 42, .. })
            )
        });
        assert!(has_exit, "expected exit code 42, got {events:?}");
    }

    #[test]
    fn duplicate_job_id_fails() {
        let sink = TestSink::new();
        let (_rt, mut mgr) = create_manager(sink.clone());

        mgr.spawn_process(
            &test_plugin_id(),
            1,
            "sleep",
            &["60".into()],
            StdinMode::Null,
        );
        mgr.spawn_process(
            &test_plugin_id(),
            1,
            "echo",
            &["dup".into()],
            StdinMode::Null,
        );

        let events = sink.events();
        let has_dup_error = events.iter().any(|(_, e)| {
            matches!(e, IoEvent::Process(ProcessEvent::SpawnFailed { error, .. }) if error.contains("already in use"))
        });
        assert!(
            has_dup_error,
            "expected duplicate job_id error, got {events:?}"
        );

        mgr.shutdown();
    }
}
