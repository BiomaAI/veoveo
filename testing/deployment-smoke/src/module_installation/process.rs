//! File-backed output avoids pipe backpressure and descendant-held pipe waits.
use anyhow::{Context, Result, ensure};
use nix::{
    sys::signal::{Signal, killpg},
    sys::wait::{Id, WaitPidFlag, WaitStatus, waitid},
    unistd::Pid,
};
use std::os::unix::process::CommandExt;
use std::{
    io::{Read, Seek, SeekFrom},
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

struct OwnedChild {
    child: Child,
    settled: bool,
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.settled {
            return;
        }
        let _ = killpg(Pid::from_raw(self.child.id() as i32), Signal::SIGKILL);
        let _ = self.child.kill();
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(2) {
            match self.child.try_wait() {
                Ok(Some(_)) | Err(_) => return,
                Ok(None) => thread::sleep(Duration::from_millis(10)),
            }
        }
    }
}

fn captured(command: &mut Command, seconds: u64) -> Result<(ExitStatus, Vec<u8>, Vec<u8>)> {
    let mut stdout = tempfile::tempfile()?;
    let mut stderr = tempfile::tempfile()?;
    command.process_group(0);
    let child = command
        .stdin(Stdio::null())
        .stdout(stdout.try_clone()?)
        .stderr(stderr.try_clone()?)
        .spawn()
        .context("start fixture command")?;
    let mut owned = OwnedChild {
        child,
        settled: false,
    };
    let started = Instant::now();
    let status = loop {
        // Observe without reaping: the child's PID keeps the owned group identity
        // reserved until descendants are killed, avoiding a PID-reuse signal race.
        let pid = Pid::from_raw(owned.child.id() as i32);
        match waitid(
            Id::Pid(pid),
            WaitPidFlag::WEXITED | WaitPidFlag::WNOHANG | WaitPidFlag::WNOWAIT,
        ) {
            Ok(WaitStatus::Exited(_, _)) | Ok(WaitStatus::Signaled(_, _, _)) => {
                let _ = killpg(pid, Signal::SIGKILL);
                let status = owned
                    .child
                    .try_wait()?
                    .context("observed child settlement was lost")?;
                owned.settled = true;
                break status;
            }
            Ok(WaitStatus::StillAlive) if started.elapsed() < Duration::from_secs(seconds) => {
                thread::sleep(Duration::from_millis(30))
            }
            Ok(WaitStatus::StillAlive) => {
                anyhow::bail!("fixture command exceeded {seconds} seconds")
            }
            _ => anyhow::bail!("fixture command observation failed"),
        }
        ensure!(
            stdout.metadata()?.len() <= 2 * 1024 * 1024
                && stderr.metadata()?.len() <= 2 * 1024 * 1024,
            "fixture command output exceeded 2 MiB"
        );
    };
    ensure!(
        stdout.metadata()?.len() <= 2 * 1024 * 1024,
        "fixture output exceeded 2 MiB"
    );
    stdout.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    stdout.take(2 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 2 * 1024 * 1024,
        "fixture output exceeded 2 MiB"
    );
    ensure!(
        stderr.metadata()?.len() <= 2 * 1024 * 1024,
        "fixture stderr exceeded 2 MiB"
    );
    stderr.seek(SeekFrom::Start(0))?;
    let mut diagnostics = Vec::new();
    stderr
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut diagnostics)?;
    ensure!(
        diagnostics.len() <= 2 * 1024 * 1024,
        "fixture stderr exceeded 2 MiB"
    );
    Ok((status, bytes, diagnostics))
}
#[cfg(test)]
fn output(command: &mut Command, seconds: u64) -> Result<(bool, Vec<u8>)> {
    let (status, bytes, _) = captured(command, seconds)?;
    Ok((status.success(), bytes))
}
pub(super) fn checked(command: &mut Command, seconds: u64) -> Result<Vec<u8>> {
    checked_phase(command, seconds, "fixture command")
}
fn program(command: &Command) -> &str {
    match command.get_program().to_str() {
        Some("helm") => "helm",
        Some("kubectl") => "kubectl",
        Some("openssl") => "openssl",
        _ => "fixture subprocess",
    }
}
pub(super) fn checked_phase(command: &mut Command, seconds: u64, phase: &str) -> Result<Vec<u8>> {
    checked_with_diagnostics(command, seconds, phase, |_| {
        "stderr excluded to protect credentials".into()
    })
}
/// Only the fixture-owned Helm values boundary opts into redacted stderr.
pub(super) fn checked_redacted(
    command: &mut Command,
    seconds: u64,
    phase: &str,
    redact: impl FnOnce(&[u8]) -> String,
) -> Result<Vec<u8>> {
    ensure!(
        program(command) == "helm",
        "redacted command diagnostics require the fixture Helm boundary"
    );
    checked_with_diagnostics(command, seconds, phase, redact)
}
fn checked_with_diagnostics(
    command: &mut Command,
    seconds: u64,
    phase: &str,
    redact: impl FnOnce(&[u8]) -> String,
) -> Result<Vec<u8>> {
    let label = program(command).to_owned();
    let (status, bytes, stderr) =
        captured(command, seconds).with_context(|| format!("{phase}: {label}"))?;
    if !status.success() {
        anyhow::bail!("{phase}: {label} failed ({status}); {}", redact(&stderr));
    }
    Ok(bytes)
}

/// A fixture-owned watch or port-forward. Output is file-backed and never logged.
pub(super) struct Background {
    owned: OwnedChild,
    stdout: std::fs::File,
    stderr: std::fs::File,
    started: Instant,
    seconds: u64,
    offset: u64,
}
impl Background {
    pub fn start(command: &mut Command, seconds: u64) -> Result<Self> {
        let stdout = tempfile::tempfile()?;
        let stderr = tempfile::tempfile()?;
        command.process_group(0);
        let child = command
            .stdin(Stdio::null())
            .stdout(stdout.try_clone()?)
            .stderr(stderr.try_clone()?)
            .spawn()
            .context("start owned fixture observer")?;
        Ok(Self {
            owned: OwnedChild {
                child,
                settled: false,
            },
            stdout,
            stderr,
            started: Instant::now(),
            seconds,
            offset: 0,
        })
    }
    pub fn read(&mut self) -> Result<Vec<u8>> {
        ensure!(
            self.started.elapsed() < Duration::from_secs(self.seconds),
            "fixture observer deadline exceeded"
        );
        ensure!(
            self.stdout.metadata()?.len() <= 2 * 1024 * 1024
                && self.stderr.metadata()?.len() <= 2 * 1024 * 1024,
            "fixture observer output exceeded 2 MiB"
        );
        ensure!(
            matches!(
                waitid(
                    Id::Pid(Pid::from_raw(self.owned.child.id() as i32)),
                    WaitPidFlag::WEXITED | WaitPidFlag::WNOHANG | WaitPidFlag::WNOWAIT
                )?,
                WaitStatus::StillAlive
            ),
            "fixture watch/forward ended; an observation gap cannot prove drain"
        );
        // pread keeps the child's shared open-file write offset untouched.
        use std::os::unix::fs::FileExt;
        let mut bytes = vec![0; (self.stdout.metadata()?.len() - self.offset) as usize];
        self.stdout.read_exact_at(&mut bytes, self.offset)?;
        self.offset += bytes.len() as u64;
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generic_command_diagnostics_suppress_child_secrets() {
        let error = checked_phase(
            Command::new("sh").args(["-c", "echo secret=password-123 >&2; exit 7"]),
            2,
            "native diagnostic",
        )
        .unwrap_err();
        let diagnostic = format!("{error:#}");
        assert!(diagnostic.contains("native diagnostic"));
        assert!(diagnostic.contains("exit status: 7"));
        assert!(!diagnostic.contains("password-123"));
    }
    #[test]
    fn command_deadline_and_failure_are_distinct() {
        assert!(!output(Command::new("false").arg(""), 1).unwrap().0);
        let start = Instant::now();
        assert!(output(Command::new("sleep").arg("5"), 1).is_err());
        assert!(start.elapsed() < Duration::from_secs(4));
    }
    #[test]
    fn deadline_kills_owned_descendants() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("descendant.pid");
        let mut command = Command::new("sh");
        command
            .args(["-c", "sleep 30 & echo $! > \"$1\"; wait", "fixture"])
            .arg(&path);
        assert!(output(&mut command, 1).is_err());
        let pid = std::fs::read_to_string(&path).unwrap().trim().to_owned();
        assert_stopped(&pid);
    }

    #[test]
    fn parent_success_retires_background_descendants_before_output() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("descendant.pid");
        let mut command = Command::new("sh");
        command
            .args(["-c", "sleep 30 & echo $! > \"$1\"; exit 0", "fixture"])
            .arg(&path);
        assert!(output(&mut command, 1).unwrap().0);
        let pid = std::fs::read_to_string(path).unwrap().trim().to_owned();
        assert_stopped(&pid);
    }
    fn assert_stopped(pid: &str) {
        let start = Instant::now();
        loop {
            let Ok(state) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
                return;
            };
            if state.split_whitespace().nth(2) == Some("Z") {
                return;
            }
            assert!(
                start.elapsed() < Duration::from_secs(2),
                "owned descendant still executes after group kill"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}
