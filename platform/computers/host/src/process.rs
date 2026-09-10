use anyhow::{Context, Result, ensure};
use nix::{
    sys::signal::{Signal, kill, killpg},
    unistd::Pid,
};
use std::{process::Stdio, time::Duration};
use tokio::process::{Child, Command};

pub fn command(binary: &str) -> Command {
    let mut command = Command::new(binary);
    command
        .env_clear()
        .env(
            "PATH",
            "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        )
        .env("container", "docker")
        .stdin(Stdio::null())
        .kill_on_drop(true);
    command
}
pub struct Process {
    child: Child,
    pid: Pid,
    name: &'static str,
}
impl Process {
    pub fn spawn(name: &'static str, command: &mut Command) -> Result<Self> {
        command.process_group(0);
        let child = command.spawn().with_context(|| format!("start {name}"))?;
        let pid = Pid::from_raw(i32::try_from(child.id().context("child PID")?)?);
        eprintln!("compute-host: started {name}");
        Ok(Self { child, pid, name })
    }
    pub fn check(&mut self) -> Result<()> {
        ensure!(
            self.child.try_wait()?.is_none(),
            "{} process exited",
            self.name
        );
        Ok(())
    }
    pub async fn stop(&mut self, seconds: u64) {
        eprintln!("compute-host: stopping {}", self.name);
        let _ = kill(self.pid, Signal::SIGTERM);
        if tokio::time::timeout(Duration::from_secs(seconds), self.child.wait())
            .await
            .is_err()
        {
            eprintln!("compute-host: {} exceeded its stop deadline", self.name);
            let _ = killpg(self.pid, Signal::SIGKILL);
            let _ = tokio::time::timeout(Duration::from_secs(2), self.child.wait()).await;
        }
        // Reap descendants of this owned process group even when its leader
        // exited early. Computer processes belong to the private daemon.
        let _ = killpg(self.pid, Signal::SIGKILL);
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = killpg(self.pid, Signal::SIGKILL);
    }
}

pub async fn finite(command: &mut Command, seconds: u64) -> Result<()> {
    let mut child = command.spawn()?;
    let status = tokio::time::timeout(Duration::from_secs(seconds), child.wait())
        .await
        .context("compute host command deadline")??;
    ensure!(status.success(), "compute host command failed");
    Ok(())
}
