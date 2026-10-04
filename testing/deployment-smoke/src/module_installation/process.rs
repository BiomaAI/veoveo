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
    process::{Child, Command, Stdio},
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

pub(super) fn output(command: &mut Command, seconds: u64) -> Result<(bool, Vec<u8>)> {
    let mut stdout = tempfile::tempfile()?;
    let stderr = tempfile::tempfile()?;
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
    let success = loop {
        // Observe without reaping: the child's PID keeps the owned group identity
        // reserved until descendants are killed, avoiding a PID-reuse signal race.
        let pid = Pid::from_raw(owned.child.id() as i32);
        match waitid(
            Id::Pid(pid),
            WaitPidFlag::WEXITED | WaitPidFlag::WNOHANG | WaitPidFlag::WNOWAIT,
        ) {
            Ok(WaitStatus::Exited(_, code)) => {
                let _ = killpg(pid, Signal::SIGKILL);
                ensure!(
                    owned.child.try_wait()?.is_some(),
                    "observed child settlement was lost"
                );
                owned.settled = true;
                break code == 0;
            }
            Ok(WaitStatus::Signaled(_, _, _)) => {
                let _ = killpg(pid, Signal::SIGKILL);
                ensure!(
                    owned.child.try_wait()?.is_some(),
                    "observed child settlement was lost"
                );
                owned.settled = true;
                break false;
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
    Ok((success, bytes))
}
pub(super) fn checked(command: &mut Command, seconds: u64) -> Result<Vec<u8>> {
    let (success, bytes) = output(command, seconds)?;
    ensure!(
        success,
        "fixture command failed; child output is excluded to protect credential diagnostics"
    );
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
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
