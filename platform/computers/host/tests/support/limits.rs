//! Checks run inside the owned host fixture after its retained-image upgrade.
use anyhow::{Context, Result, ensure};
use std::{fs, path::Path};

pub fn check(directory: &Path, after: bool) -> Result<()> {
    // OCI exec may join the originally configured cgroup namespace rather than
    // the one PID 1 created. Inspect from the launcher's actual namespace.
    nix::sched::setns(
        fs::File::open("/proc/1/ns/cgroup")?,
        nix::sched::CloneFlags::CLONE_NEWCGROUP,
    )?;
    const ROOT: &str = "/sys/fs/cgroup";
    ensure!(fs::read_to_string("/proc/1/cgroup")?.trim() == "0::/init");
    ensure!(fs::read_to_string(format!("{ROOT}/cpu.max"))?.trim() == "100000 100000");
    ensure!(fs::read_to_string(format!("{ROOT}/memory.max"))?.trim() == "6442450944");
    for name in ["memory.min", "memory.low"] {
        ensure!(fs::read_to_string(format!("{ROOT}/{name}"))?.trim() == "0");
    }
    let record: serde_json::Value =
        serde_json::from_slice(&fs::read(directory.join("record.json"))?)?;
    let container = record["writer"]["writer"]["containerId"]
        .as_str()
        .context("retained writer")?;
    ensure!(container.len() == 64 && container.bytes().all(|b| b.is_ascii_hexdigit()));
    let pid: u32 = super::command(
        "docker",
        &[
            "-H",
            "unix:///run/veoveo-computers/docker.sock",
            "inspect",
            "--format",
            "{{.State.Pid}}",
            container,
        ],
    )?
    .parse()?;
    ensure!(pid > 0);
    let relative = format!("/docker/{container}");
    ensure!(
        fs::read_to_string(format!("/proc/{pid}/cgroup"))?.trim() == format!("0::{relative}"),
        "retained guest escaped the compute host's cgroup root"
    );
    let guest = format!("{ROOT}{relative}");
    ensure!(fs::read_to_string(format!("{guest}/cpu.max"))?.trim() == "200000 100000");
    ensure!(fs::read_to_string(format!("{guest}/memory.max"))?.trim() == "2147483648");
    for name in ["memory.min", "memory.low"] {
        ensure!(fs::read_to_string(format!("{guest}/{name}"))?.trim() == "0");
    }
    let stat = fs::read_to_string(format!("{ROOT}/cpu.stat"))?;
    let throttled: u64 = stat
        .lines()
        .find_map(|line| line.strip_prefix("nr_throttled "))
        .context("aggregate throttling counter")?
        .parse()?;
    let baseline = directory.join("limits-before");
    if after {
        let before: u64 = fs::read_to_string(baseline)?.parse()?;
        ensure!(
            throttled > before,
            "guest workload did not hit the stricter host CPU ceiling"
        );
    } else {
        fs::write(baseline, throttled.to_string())?;
    }
    Ok(())
}
