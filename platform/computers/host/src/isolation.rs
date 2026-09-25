//! Scope upstream dind initialization and every nested cgroup to this container.
use anyhow::{Context, Result, ensure};
use nix::{
    mount::{MsFlags, mount},
    sched::{CloneFlags, unshare},
    unistd::{geteuid, getpid},
};
use std::{fs, os::unix::process::CommandExt, path::Path, process::Command};

const ROOT: &str = "/sys/fs/cgroup";

// Runs as PID 1, before Tokio creates threads and before dind touches cgroups.
pub fn initialize(config: &Path) -> Result<()> {
    ensure!(
        geteuid().is_root() && getpid().as_raw() == 1,
        "computer-host init requires PID 1 in its privileged container"
    );
    unshare(CloneFlags::CLONE_NEWNS | CloneFlags::CLONE_NEWCGROUP)
        .context("create private mount and cgroup namespaces")?;
    // Break propagation before overmounting a possibly node-wide shared mount.
    // No node mount or cgroup is modified.
    mount(
        None::<&str>,
        "/",
        None::<&str>,
        MsFlags::MS_REC | MsFlags::MS_PRIVATE,
        None::<&str>,
    )?;
    mount(
        Some("cgroup2"),
        ROOT,
        Some("cgroup2"),
        MsFlags::MS_NOSUID | MsFlags::MS_NODEV | MsFlags::MS_NOEXEC,
        None::<&str>,
    )
    .context("mount cgroup v2 at the private namespace root")?;
    ensure!(
        fs::read_to_string("/proc/self/cgroup")?.trim() == "0::/",
        "compute host must own its cgroup namespace root"
    );
    verify_limits()?;
    // Upstream dind moves its process to /init and enables nested controllers.
    // Docker's default /docker parent now sits under our limits.
    Err(Command::new("/usr/local/bin/dind")
        .args([
            "/usr/local/bin/docker-init",
            "--",
            "/usr/local/bin/veoveo-computer-host",
            "run",
            "--config",
        ])
        .arg(config)
        .exec()
        .into())
}

pub fn verify_runtime() -> Result<()> {
    ensure!(
        fs::read_to_string("/proc/1/cgroup")?.trim() == "0::/init",
        "compute host requires its private cgroup bootstrap; launch with init"
    );
    verify_limits()
}

fn verify_limits() -> Result<()> {
    let cpu = fs::read_to_string(format!("{ROOT}/cpu.max"))?;
    let memory = fs::read_to_string(format!("{ROOT}/memory.max"))?;
    let cpu: Vec<_> = cpu.split_whitespace().collect();
    ensure!(
        cpu.len() == 2 && cpu.iter().all(|v| v.parse::<u64>().is_ok_and(|n| n > 0)),
        "set a finite CPU limit on the compute host container"
    );
    ensure!(
        memory.trim().parse::<u64>().is_ok_and(|n| n > 0),
        "set a finite memory limit on the compute host container"
    );
    Ok(())
}
