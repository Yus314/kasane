//! Kakoune child process management: spawning, I/O piping, exit code propagation.

use std::cell::Cell;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use kasane_core::session::SessionSpec;

thread_local! {
    static LAST_KAK_EXIT_CODE: Cell<Option<i32>> = const { Cell::new(None) };
}

/// Writer half: wraps Kakoune's stdin as a `Write` impl.
pub struct KakouneWriter {
    stdin: ChildStdin,
}

impl Write for KakouneWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.stdin.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.stdin.flush()
    }
}

/// Reader half: reads JSON-RPC lines from Kakoune's stdout.
pub struct KakouneReader {
    stdout: BufReader<ChildStdout>,
}

impl std::io::Read for KakouneReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.stdout.read(buf)
    }
}

impl BufRead for KakouneReader {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        self.stdout.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.stdout.consume(amt);
    }
}

/// Handle to the Kakoune child process.
/// Waits for the child on drop.
pub struct KakouneChild {
    child: Child,
}

impl KakouneChild {
    pub fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        self.child.wait()
    }
}

impl Drop for KakouneChild {
    fn drop(&mut self) {
        let code = self.child.wait().ok().and_then(|status| {
            if status.success() {
                None
            } else {
                Some(status.code().unwrap_or(1))
            }
        });
        LAST_KAK_EXIT_CODE.with(|cell| cell.set(code));
    }
}

/// Returns the exit code of the last dropped KakouneChild, if it exited with failure.
pub fn last_kak_exit_code() -> Option<i32> {
    LAST_KAK_EXIT_CODE.with(|cell| cell.get())
}

/// Spawn a Kakoune process from a pre-configured Command and return split handles.
fn start_kakoune(mut cmd: Command) -> Result<(KakouneReader, KakouneWriter, KakouneChild)> {
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());

    let mut child = cmd.spawn().context("failed to spawn kak")?;

    let stdin = child.stdin.take().context("failed to get kak stdin")?;
    let stdout = child.stdout.take().context("failed to get kak stdout")?;

    Ok((
        KakouneReader {
            stdout: BufReader::new(stdout),
        },
        KakouneWriter { stdin },
        KakouneChild { child },
    ))
}

/// Spawn Kakoune and return split reader/writer handles.
pub fn spawn_kakoune(args: &[String]) -> Result<(KakouneReader, KakouneWriter, KakouneChild)> {
    spawn_kakoune_for_spec(&SessionSpec::primary(None, args.to_vec()))
}

/// Kakoune init command injected via `-e` to propagate buffer metadata through `ui_options`.
///
/// - `remove-hooks` + `hook -group`: idempotent (safe on `-c` reconnect)
/// - `WinDisplay`: fires on every buffer switch (`:edit`, `:buffer`)
/// - `set-option -add`: preserves existing ui_options (e.g. kak-lsp filetype)
/// - Quoted values: handles paths with spaces
const KASANE_INIT_COMMAND: &str = "\
    remove-hooks global kasane-meta; \
    hook -group kasane-meta global WinDisplay .* %{ \
        set-option -add window ui_options \
            \"kasane_buffile=%val{buffile}\" \
            \"kasane_bufname=%val{bufname}\" \
    }";

fn kak_command_argv(spec: &SessionSpec) -> Vec<String> {
    let mut argv = vec!["-ui".to_string(), "json".to_string()];
    // Kasane's -e comes first so user -e args run after (can override)
    argv.push("-e".to_string());
    argv.push(KASANE_INIT_COMMAND.to_string());
    argv.extend(spec.args.iter().cloned());
    argv
}

/// Spawn Kakoune for a specific managed session.
pub fn spawn_kakoune_for_spec(
    spec: &SessionSpec,
) -> Result<(KakouneReader, KakouneWriter, KakouneChild)> {
    let mut cmd = Command::new("kak");
    cmd.args(kak_command_argv(spec));
    start_kakoune(cmd)
}

/// Handle to a headless Kakoune daemon (`kak -d`).
///
/// Does **not** kill the daemon on drop — if kasane panics, the daemon
/// survives and the user can reconnect with `kasane -c <session>`.
/// Normal shutdown calls [`DaemonHandle::kill`] explicitly.
pub struct DaemonHandle {
    child: Child,
    session_name: String,
}

impl DaemonHandle {
    /// Poll until the daemon's session appears in `kak -l` output.
    pub fn wait_ready(&mut self, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        loop {
            let output = Command::new("kak").arg("-l").output()?;
            let listing = String::from_utf8_lossy(&output.stdout);
            if listing.lines().any(|line| line.trim() == self.session_name) {
                return Ok(());
            }
            if let Some(status) = self.child.try_wait()? {
                bail!("kakoune daemon exited immediately ({status})");
            }
            if Instant::now() >= deadline {
                bail!(
                    "timeout waiting for kakoune daemon session '{}'",
                    self.session_name
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Gracefully shut down the daemon, falling back to kill.
    pub fn kill(&mut self) {
        // Try graceful shutdown via `kak -p <session> <<< kill-session`
        let _ = Command::new("kak")
            .arg("-p")
            .arg(&self.session_name)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .and_then(|mut p| {
                if let Some(ref mut stdin) = p.stdin {
                    let _ = stdin.write_all(b"kill-session\n");
                }
                p.wait()
            });

        // Fallback: forcibly kill and reap
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    pub fn session_name(&self) -> &str {
        &self.session_name
    }
}

/// Spawn a headless Kakoune daemon: `kak -d -s <session_name> [daemon_args...]`.
pub fn spawn_kakoune_daemon(session_name: &str, daemon_args: &[String]) -> Result<DaemonHandle> {
    let mut cmd = Command::new("kak");
    cmd.arg("-d")
        .arg("-s")
        .arg(session_name)
        .args(daemon_args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());

    let child = cmd.spawn().context("failed to spawn kakoune daemon")?;

    Ok(DaemonHandle {
        child,
        session_name: session_name.to_string(),
    })
}

/// Replace the current process with kak, passing through all arguments.
/// This function never returns on success.
pub fn exec_kak(args: &[String]) -> ! {
    use std::os::unix::process::CommandExt;

    let mut cmd = Command::new("kak");
    cmd.args(args);
    let err = cmd.exec();
    eprintln!("error: failed to exec kak: {err}");
    std::process::exit(1);
}

/// Get the kak version string for display.
pub fn get_kak_version() -> String {
    match Command::new("kak").arg("-version").output() {
        Ok(output) => String::from_utf8_lossy(&output.stdout).trim().to_string(),
        Err(_) => "kak not found".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_session_spec_does_not_add_connect_flag() {
        let spec = SessionSpec::primary(None, vec!["file.txt".to_string()]);
        let argv = kak_command_argv(&spec);

        assert_eq!(argv[0], "-ui");
        assert_eq!(argv[1], "json");
        assert_eq!(argv[2], "-e");
        assert_eq!(argv[3], KASANE_INIT_COMMAND);
        assert_eq!(argv[4], "file.txt");
        assert_eq!(argv.len(), 5);
    }

    #[test]
    fn connect_flag_from_args_is_preserved() {
        let spec = SessionSpec::primary(
            Some("project".to_string()),
            vec![
                "-c".to_string(),
                "project".to_string(),
                "file.txt".to_string(),
            ],
        );
        let argv = kak_command_argv(&spec);

        assert_eq!(&argv[0..4], &["-ui", "json", "-e", KASANE_INIT_COMMAND]);
        assert_eq!(&argv[4..], &["-c", "project", "file.txt"]);
    }

    #[test]
    fn named_session_flag_from_args_is_preserved() {
        let spec = SessionSpec::primary(
            Some("myses".to_string()),
            vec![
                "-s".to_string(),
                "myses".to_string(),
                "file.txt".to_string(),
            ],
        );
        let argv = kak_command_argv(&spec);

        assert_eq!(&argv[0..4], &["-ui", "json", "-e", KASANE_INIT_COMMAND]);
        assert_eq!(&argv[4..], &["-s", "myses", "file.txt"]);
    }

    #[test]
    fn plain_file_open_has_no_session_flags() {
        let spec = SessionSpec::primary(None, vec!["file.txt".to_string()]);
        let argv = kak_command_argv(&spec);

        assert_eq!(&argv[0..4], &["-ui", "json", "-e", KASANE_INIT_COMMAND]);
        assert_eq!(&argv[4..], &["file.txt"]);
    }

    #[test]
    fn kasane_init_command_comes_before_user_args() {
        let spec = SessionSpec::primary(
            None,
            vec![
                "-e".to_string(),
                "colorscheme gruvbox".to_string(),
                "file.txt".to_string(),
            ],
        );
        let argv = kak_command_argv(&spec);

        // Kasane's -e is at index 2-3, user's -e is at index 4-5
        assert_eq!(argv[2], "-e");
        assert_eq!(argv[3], KASANE_INIT_COMMAND);
        assert_eq!(argv[4], "-e");
        assert_eq!(argv[5], "colorscheme gruvbox");
        assert_eq!(argv[6], "file.txt");
    }
}
