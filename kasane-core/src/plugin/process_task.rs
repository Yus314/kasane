//! Declarative process I/O model for plugins.
//!
//! [`ProcessTaskSpec`] describes a process to spawn (with optional fallback chain).
//! The framework manages job ID allocation, stdout buffering, fallback on spawn
//! failure, and state machine transitions. Plugin authors receive a
//! [`ProcessTaskResult`] when a task completes, fails, or produces streaming output.
//!
//! # Example (Native Plugin)
//!
//! ```ignore
//! r.on_process_task(
//!     "file_list",
//!     ProcessTaskSpec::new("fd", &["--type", "f"])
//!         .fallback(ProcessTaskSpec::new("find", &[".", "-type", "f"])),
//!     |state, result| match result {
//!         ProcessTaskResult::Completed { stdout, .. } => {
//!             let files: Vec<String> = String::from_utf8_lossy(&stdout)
//!                 .lines()
//!                 .map(String::from)
//!                 .collect();
//!             (MyState { files, ..state.clone() }, Effects::none())
//!         }
//!         ProcessTaskResult::Failed(msg) => {
//!             (MyState { error: Some(msg.clone()), ..state.clone() }, Effects::none())
//!         }
//!         _ => (state.clone(), Effects::none()),
//!     },
//! );
//! ```

use super::io::{ProcessEvent, StdinMode};
use super::{AppView, Command, Effects, PluginState};

// =============================================================================
// Public types
// =============================================================================

/// Specification for a process to spawn.
///
/// Supports a fallback chain: if the primary program fails to spawn,
/// the framework automatically tries the fallback.
#[derive(Debug, Clone)]
pub struct ProcessTaskSpec {
    /// Program name or path.
    pub program: String,
    /// Command-line arguments.
    pub args: Vec<String>,
    /// How stdin should be handled.
    pub stdin_mode: StdinMode,
    /// Optional fallback if this program fails to spawn.
    pub fallback: Option<Box<ProcessTaskSpec>>,
}

impl ProcessTaskSpec {
    /// Create a new process task spec with null stdin.
    pub fn new(program: impl Into<String>, args: &[impl AsRef<str>]) -> Self {
        Self {
            program: program.into(),
            args: args.iter().map(|a| a.as_ref().to_string()).collect(),
            stdin_mode: StdinMode::Null,
            fallback: None,
        }
    }

    /// Set the stdin mode.
    pub fn stdin(mut self, mode: StdinMode) -> Self {
        self.stdin_mode = mode;
        self
    }

    /// Add a fallback process to try if this one fails to spawn.
    pub fn fallback(mut self, spec: ProcessTaskSpec) -> Self {
        self.fallback = Some(Box::new(spec));
        self
    }
}

/// Result delivered to a process task handler.
#[derive(Debug, Clone)]
pub enum ProcessTaskResult {
    /// Process exited normally. Contains accumulated stdout and exit code.
    Completed { stdout: Vec<u8>, exit_code: i32 },
    /// Streaming stdout chunk (for tasks that need incremental processing).
    Stdout(Vec<u8>),
    /// All attempts (including fallbacks) failed.
    Failed(String),
}

// =============================================================================
// Framework-internal types
// =============================================================================

/// Type-erased process task handler.
pub(crate) type ErasedProcessTaskHandler = Box<
    dyn Fn(&dyn PluginState, &ProcessTaskResult, &AppView<'_>) -> (Box<dyn PluginState>, Effects)
        + Send
        + Sync,
>;

/// A registered process task (stored in HandlerTable).
pub(crate) struct ProcessTaskEntry {
    pub(crate) name: &'static str,
    pub(crate) spec: ProcessTaskSpec,
    pub(crate) handler: ErasedProcessTaskHandler,
    /// If true, deliver `Stdout` chunks incrementally instead of accumulating.
    pub(crate) streaming: bool,
}

/// Framework-managed state for a running process task.
pub(crate) struct ProcessTaskHandle {
    /// The registered task name.
    pub(crate) name: &'static str,
    /// Current job ID assigned by the framework.
    pub(crate) job_id: u64,
    /// Accumulated stdout (when not streaming).
    pub(crate) stdout_buf: Vec<u8>,
    /// Remaining fallback chain (specs to try on SpawnFailed).
    pub(crate) fallbacks: Vec<ProcessTaskSpec>,
}

impl ProcessTaskHandle {
    /// Create a new handle for a task that was just spawned.
    pub(crate) fn new(name: &'static str, job_id: u64, fallbacks: Vec<ProcessTaskSpec>) -> Self {
        Self {
            name,
            job_id,
            stdout_buf: Vec::new(),
            fallbacks,
        }
    }

    /// Feed a process event. Returns the result if the task reached a terminal state,
    /// or a fallback spec to try next on spawn failure.
    pub(crate) fn feed(&mut self, event: &ProcessEvent, streaming: bool) -> ProcessTaskFeedResult {
        match event {
            ProcessEvent::Stdout { job_id, data } if *job_id == self.job_id => {
                if streaming {
                    ProcessTaskFeedResult::Deliver(ProcessTaskResult::Stdout(data.clone()))
                } else {
                    self.stdout_buf.extend_from_slice(data);
                    ProcessTaskFeedResult::Pending
                }
            }
            ProcessEvent::Stderr { job_id, .. } if *job_id == self.job_id => {
                // Stderr is ignored by default in process tasks.
                ProcessTaskFeedResult::Pending
            }
            ProcessEvent::Exited { job_id, exit_code } if *job_id == self.job_id => {
                ProcessTaskFeedResult::Deliver(ProcessTaskResult::Completed {
                    stdout: std::mem::take(&mut self.stdout_buf),
                    exit_code: *exit_code,
                })
            }
            ProcessEvent::SpawnFailed { job_id, error } if *job_id == self.job_id => {
                if let Some(fallback) = self.fallbacks.pop() {
                    ProcessTaskFeedResult::TryFallback(fallback)
                } else {
                    ProcessTaskFeedResult::Deliver(ProcessTaskResult::Failed(error.clone()))
                }
            }
            _ => ProcessTaskFeedResult::Ignored,
        }
    }
}

/// Result of feeding an event to a process task handle.
pub(crate) enum ProcessTaskFeedResult {
    /// Event was consumed, task is still running.
    Pending,
    /// Task produced a result to deliver to the handler.
    Deliver(ProcessTaskResult),
    /// Primary spawn failed, try this fallback spec.
    TryFallback(ProcessTaskSpec),
    /// Event was not for this task.
    Ignored,
}

/// Collect fallback chain from a spec into a Vec (reversed for pop()).
pub(crate) fn collect_fallbacks(spec: &ProcessTaskSpec) -> Vec<ProcessTaskSpec> {
    let mut fallbacks = Vec::new();
    let mut current = spec.fallback.as_deref();
    while let Some(fb) = current {
        fallbacks.push(fb.clone());
        current = fb.fallback.as_deref();
    }
    fallbacks.reverse(); // So pop() gives first fallback
    fallbacks
}

/// Generate the initial spawn command for a process task.
pub(crate) fn spawn_command(spec: &ProcessTaskSpec, job_id: u64) -> Command {
    Command::SpawnProcess {
        job_id,
        program: spec.program.clone(),
        args: spec.args.clone(),
        stdin_mode: spec.stdin_mode,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_builder() {
        let spec = ProcessTaskSpec::new("fd", &["--type", "f"])
            .stdin(StdinMode::Piped)
            .fallback(ProcessTaskSpec::new("find", &[".", "-type", "f"]));

        assert_eq!(spec.program, "fd");
        assert_eq!(spec.args, vec!["--type", "f"]);
        assert_eq!(spec.stdin_mode, StdinMode::Piped);
        assert!(spec.fallback.is_some());

        let fb = spec.fallback.as_ref().unwrap();
        assert_eq!(fb.program, "find");
        assert_eq!(fb.args, vec![".", "-type", "f"]);
    }

    #[test]
    fn collect_fallbacks_chain() {
        let spec = ProcessTaskSpec::new("a", &[] as &[&str]).fallback(
            ProcessTaskSpec::new("b", &[] as &[&str])
                .fallback(ProcessTaskSpec::new("c", &[] as &[&str])),
        );

        let fbs = collect_fallbacks(&spec);
        assert_eq!(fbs.len(), 2);
        assert_eq!(fbs[0].program, "c"); // reversed: pop gives "b" first
        assert_eq!(fbs[1].program, "b");
    }

    #[test]
    fn handle_stdout_accumulation() {
        let mut handle = ProcessTaskHandle::new("test", 42, vec![]);
        let result = handle.feed(
            &ProcessEvent::Stdout {
                job_id: 42,
                data: b"hello ".to_vec(),
            },
            false,
        );
        assert!(matches!(result, ProcessTaskFeedResult::Pending));

        let result = handle.feed(
            &ProcessEvent::Stdout {
                job_id: 42,
                data: b"world".to_vec(),
            },
            false,
        );
        assert!(matches!(result, ProcessTaskFeedResult::Pending));

        let result = handle.feed(
            &ProcessEvent::Exited {
                job_id: 42,
                exit_code: 0,
            },
            false,
        );
        match result {
            ProcessTaskFeedResult::Deliver(ProcessTaskResult::Completed { stdout, exit_code }) => {
                assert_eq!(stdout, b"hello world");
                assert_eq!(exit_code, 0);
            }
            _ => panic!("expected Completed"),
        }
    }

    #[test]
    fn handle_streaming_mode() {
        let mut handle = ProcessTaskHandle::new("test", 42, vec![]);
        let result = handle.feed(
            &ProcessEvent::Stdout {
                job_id: 42,
                data: b"chunk".to_vec(),
            },
            true,
        );
        match result {
            ProcessTaskFeedResult::Deliver(ProcessTaskResult::Stdout(data)) => {
                assert_eq!(data, b"chunk");
            }
            _ => panic!("expected streaming Stdout"),
        }
    }

    #[test]
    fn handle_fallback_on_spawn_failure() {
        let fallback = ProcessTaskSpec::new("find", &[".", "-type", "f"]);
        let mut handle = ProcessTaskHandle::new("test", 42, vec![fallback.clone()]);

        let result = handle.feed(
            &ProcessEvent::SpawnFailed {
                job_id: 42,
                error: "not found".to_string(),
            },
            false,
        );
        match result {
            ProcessTaskFeedResult::TryFallback(spec) => {
                assert_eq!(spec.program, "find");
            }
            _ => panic!("expected TryFallback"),
        }
    }

    #[test]
    fn handle_final_failure() {
        let mut handle = ProcessTaskHandle::new("test", 42, vec![]);

        let result = handle.feed(
            &ProcessEvent::SpawnFailed {
                job_id: 42,
                error: "not found".to_string(),
            },
            false,
        );
        match result {
            ProcessTaskFeedResult::Deliver(ProcessTaskResult::Failed(msg)) => {
                assert_eq!(msg, "not found");
            }
            _ => panic!("expected Failed"),
        }
    }

    #[test]
    fn handle_ignores_other_job_ids() {
        let mut handle = ProcessTaskHandle::new("test", 42, vec![]);
        let result = handle.feed(
            &ProcessEvent::Stdout {
                job_id: 99,
                data: b"other".to_vec(),
            },
            false,
        );
        assert!(matches!(result, ProcessTaskFeedResult::Ignored));
    }
}
