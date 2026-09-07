use std::path::Path;

use anyhow::{Context, Result, ensure};

use crate::{BUILDER_CONTAINER, BUILDER_NAME, output_text};

/// Shared-host budget. Leave capacity for the live Kubernetes and GPU workloads.
pub const RESOURCES: BuilderResources = BuilderResources {
    cpu_period_us: 100_000,
    cpu_quota_us: 1_200_000,
    memory_bytes: 36 * 1024 * 1024 * 1024,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuilderResources {
    pub cpu_period_us: u64,
    pub cpu_quota_us: u64,
    pub memory_bytes: u64,
}

impl BuilderResources {
    pub(crate) fn driver_options(self) -> [String; 4] {
        [
            format!("cpu-period={}", self.cpu_period_us),
            format!("cpu-quota={}", self.cpu_quota_us),
            format!("memory={}", self.memory_bytes),
            format!("memory-swap={}", self.memory_bytes),
        ]
    }

    pub(crate) fn validate_host(self, repository: &Path) -> Result<()> {
        let output = output_text(
            "docker",
            ["info", "--format", "{{.NCPU}} {{.MemTotal}}"],
            Some(repository),
        )?;
        let values = output.split_whitespace().collect::<Vec<_>>();
        ensure!(values.len() == 2, "invalid Docker host resource inspection");
        let cpus: u64 = values[0].parse().context("reading Docker host CPU count")?;
        let memory: u64 = values[1].parse().context("reading Docker host memory")?;
        ensure!(
            cpus * self.cpu_period_us >= self.cpu_quota_us && memory >= self.memory_bytes,
            "Docker host has {cpus} CPUs and {memory} memory bytes; builder requires {self:?}"
        );
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ContainerLimits {
    cpu_period_us: u64,
    cpu_quota_us: u64,
    nano_cpus: u64,
    memory_bytes: u64,
    memory_swap_bytes: u64,
    cpuset: String,
}

impl ContainerLimits {
    fn parse(output: &str) -> Result<Self> {
        let fields = output.trim().split('|').collect::<Vec<_>>();
        ensure!(
            fields.len() == 6,
            "invalid Docker builder resource inspection"
        );
        Ok(Self {
            cpu_period_us: fields[0].parse().context("reading CPU period")?,
            cpu_quota_us: fields[1].parse().context("reading CPU quota")?,
            nano_cpus: fields[2].parse().context("reading NanoCPUs")?,
            memory_bytes: fields[3].parse().context("reading memory limit")?,
            memory_swap_bytes: fields[4].parse().context("reading swap limit")?,
            cpuset: fields[5].to_owned(),
        })
    }

    fn validate(&self, expected: BuilderResources) -> Result<()> {
        ensure!(
            self.cpu_period_us == expected.cpu_period_us
                && self.cpu_quota_us == expected.cpu_quota_us
                && self.nano_cpus == 0
                && self.memory_bytes == expected.memory_bytes
                && self.memory_swap_bytes == expected.memory_bytes
                && self.cpuset.is_empty(),
            "builder {BUILDER_NAME} resource drift: observed {self:?}, expected {expected:?}, no additional CPU pinning, and swap disabled; run `cargo xtask image builder reconfigure --confirm {BUILDER_NAME}`"
        );
        Ok(())
    }
}

/// Linux cgroup v2 counters. Subtract snapshots around a solve, not across restarts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuSnapshot {
    pub usage_us: u64,
    pub periods: u64,
    pub throttled_periods: u64,
    pub throttled_us: u64,
}

impl CpuSnapshot {
    pub fn checked_delta(self, before: Self) -> Option<Self> {
        Some(Self {
            usage_us: self.usage_us.checked_sub(before.usage_us)?,
            periods: self.periods.checked_sub(before.periods)?,
            throttled_periods: self
                .throttled_periods
                .checked_sub(before.throttled_periods)?,
            throttled_us: self.throttled_us.checked_sub(before.throttled_us)?,
        })
    }

    fn parse(output: &str) -> Result<Self> {
        let field = |name: &str| -> Result<u64> {
            output
                .lines()
                .find_map(|line| {
                    let (key, value) = line.split_once(' ')?;
                    (key == name).then_some(value)
                })
                .with_context(|| format!("cgroup CPU statistics have no {name}"))?
                .parse()
                .with_context(|| format!("invalid cgroup CPU statistic {name}"))
        };
        Ok(Self {
            usage_us: field("usage_usec")?,
            periods: field("nr_periods")?,
            throttled_periods: field("nr_throttled")?,
            throttled_us: field("throttled_usec")?,
        })
    }
}

pub fn cpu_snapshot(repository: &Path) -> Result<CpuSnapshot> {
    CpuSnapshot::parse(&output_text(
        "docker",
        ["exec", BUILDER_CONTAINER, "cat", "/sys/fs/cgroup/cpu.stat"],
        Some(repository),
    )?)
}

pub(crate) fn validate(repository: &Path) -> Result<()> {
    let output = output_text(
        "docker",
        [
            "inspect",
            BUILDER_CONTAINER,
            "--format",
            "{{with .HostConfig}}{{.CpuPeriod}}|{{.CpuQuota}}|{{.NanoCpus}}|{{.Memory}}|{{.MemorySwap}}|{{.CpusetCpus}}{{end}}",
        ],
        Some(repository),
    )?;
    ContainerLimits::parse(&output)?.validate(RESOURCES)
}

pub(crate) fn print_status(repository: &Path) -> Result<()> {
    println!(
        "Resource budget: {} CPUs, {} GiB RAM, swap disabled",
        RESOURCES.cpu_quota_us / RESOURCES.cpu_period_us,
        RESOURCES.memory_bytes / (1024 * 1024 * 1024)
    );
    let limits = output_text(
        "docker",
        [
            "exec",
            BUILDER_CONTAINER,
            "cat",
            "/sys/fs/cgroup/cpu.max",
            "/sys/fs/cgroup/memory.max",
        ],
        Some(repository),
    )?;
    println!(
        "Effective cgroup CPU quota/period and memory bytes: {}",
        limits.trim().replace('\n', "; ")
    );
    println!(
        "CPU counters since worker start: {:?}",
        cpu_snapshot(repository)?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_out_of_band_limits() {
        let valid = "100000|1200000|0|38654705664|38654705664|";
        ContainerLimits::parse(valid)
            .unwrap()
            .validate(RESOURCES)
            .unwrap();
        for invalid in [
            "0|0|4000000000|38654705664|38654705664|",
            "100000|400000|0|38654705664|38654705664|",
            "100000|1200000|0|38654705664|77309411328|",
            "100000|1200000|0|38654705664|38654705664|0-3",
        ] {
            assert!(
                ContainerLimits::parse(invalid)
                    .unwrap()
                    .validate(RESOURCES)
                    .is_err()
            );
        }
    }

    #[test]
    fn reports_cpu_deltas_and_rejects_counter_resets() {
        let before =
            CpuSnapshot::parse("usage_usec 40\nnr_periods 4\nnr_throttled 2\nthrottled_usec 20\n")
                .unwrap();
        let after = CpuSnapshot::parse(
            "usage_usec 90\nnr_periods 9\nnr_throttled 3\nthrottled_usec 27\nextra_stat 1\n",
        )
        .unwrap();
        assert_eq!(
            after.checked_delta(before),
            Some(CpuSnapshot {
                usage_us: 50,
                periods: 5,
                throttled_periods: 1,
                throttled_us: 7
            })
        );
        assert!(before.checked_delta(after).is_none());
        assert!(CpuSnapshot::parse("usage_usec 4").is_err());
    }
}
