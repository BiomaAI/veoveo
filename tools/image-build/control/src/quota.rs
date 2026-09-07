//! Temporary, exclusively leased CPU quotas for controlled compiler experiments.

use std::{path::Path, process::Command};

use anyhow::{Context, Result, ensure};

use crate::{BUILDER_CONTAINER, BuilderLease, RESOURCES, output_text, resources};

pub struct CpuQuotaLease<'a> {
    repository: &'a Path,
    _builder: &'a BuilderLease,
    changed: bool,
}

impl<'a> CpuQuotaLease<'a> {
    pub fn new(repository: &'a Path, builder: &'a BuilderLease) -> Result<Self> {
        resources::validate(repository)?;
        Ok(Self {
            repository,
            _builder: builder,
            changed: false,
        })
    }

    pub fn set(&mut self, cpus: u32) -> Result<()> {
        validate_count(cpus)?;
        // Docker can apply a change before a later verification fails.
        self.changed = true;
        update(self.repository, u64::from(cpus) * RESOURCES.cpu_period_us)
    }

    pub fn restore(mut self) -> Result<()> {
        self.restore_inner()
    }

    fn restore_inner(&mut self) -> Result<()> {
        if self.changed {
            update(self.repository, RESOURCES.cpu_quota_us)?;
            resources::validate(self.repository)?;
            self.changed = false;
        }
        Ok(())
    }
}

impl Drop for CpuQuotaLease<'_> {
    fn drop(&mut self) {
        if let Err(error) = self.restore_inner() {
            eprintln!(
                "CPU quota restoration failed: {error:#}; run `cargo xtask image builder reconfigure --confirm veoveo`"
            );
        }
    }
}

fn validate_count(cpus: u32) -> Result<()> {
    ensure!(
        cpus > 0 && u64::from(cpus) * RESOURCES.cpu_period_us <= RESOURCES.cpu_quota_us,
        "benchmark CPU count must be between 1 and the declared builder budget"
    );
    Ok(())
}

fn update(repository: &Path, quota: u64) -> Result<()> {
    let output = Command::new("docker")
        .current_dir(repository)
        .args([
            "update",
            "--cpu-period",
            &RESOURCES.cpu_period_us.to_string(),
            "--cpu-quota",
            &quota.to_string(),
            BUILDER_CONTAINER,
        ])
        .output()
        .context("updating benchmark CPU quota")?;
    ensure!(
        output.status.success(),
        "CPU quota update failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual = output_text(
        "docker",
        ["exec", BUILDER_CONTAINER, "cat", "/sys/fs/cgroup/cpu.max"],
        Some(repository),
    )?;
    ensure!(
        actual.trim() == format!("{quota} {}", RESOURCES.cpu_period_us),
        "effective CPU quota differs from requested quota: {actual}"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn experiments_cannot_exceed_the_shared_host_budget() {
        assert!(validate_count(0).is_err());
        assert!(validate_count(4).is_ok());
        assert!(validate_count(12).is_ok());
        assert!(validate_count(13).is_err());
        assert!(validate_count(u32::MAX).is_err());
    }
}
