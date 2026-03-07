use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use anyhow::{Context, Result};

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

impl Drop for KakouneChild {
    fn drop(&mut self) {
        let _ = self.child.wait();
    }
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
    let mut cmd = Command::new("kak");
    cmd.arg("-ui").arg("json");
    cmd.args(args);
    start_kakoune(cmd)
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
