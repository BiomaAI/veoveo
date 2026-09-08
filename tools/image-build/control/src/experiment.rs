//! A disposable second worker for export/import measurements on the current host.
use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, ensure};

use crate::{
    BUILDKIT_IMAGE, BUILDKIT_VERSION, BuilderLease, RESOURCES, base_configuration, buildx_command,
    output_text, resources,
};

pub struct ExperimentWorker<'a> {
    repository: PathBuf,
    name: String,
    container: String,
    volume: String,
    removed: bool,
    _lease: &'a BuilderLease,
}

impl<'a> ExperimentWorker<'a> {
    pub fn create(repository: &Path, lease: &'a BuilderLease) -> Result<Self> {
        RESOURCES.validate_host(repository)?;
        let name = format!(
            "veoveo-reuse-{}-{}",
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
            std::process::id()
        );
        let node = format!("{name}0");
        let container = format!("buildx_buildkit_{node}");
        let volume = format!("{container}_state");
        ensure!(
            listed(repository, "volume", &volume)?.is_empty(),
            "experiment state volume already exists"
        );
        let configuration = base_configuration(repository)?;
        let options = RESOURCES.driver_options();
        let mut command = buildx_command(repository)?;
        command.args([
            "create",
            "--name",
            &name,
            "--node",
            &node,
            "--driver",
            "docker-container",
            "--driver-opt",
            &format!("image={BUILDKIT_IMAGE}"),
            "--driver-opt",
            "network=host",
        ]);
        for option in &options {
            command.args(["--driver-opt", option]);
        }
        let status = command
            .arg("--buildkitd-config")
            .arg(&configuration.path)
            .status()?;
        ensure!(
            status.success(),
            "creating experiment worker failed: {status}"
        );
        let mut worker = Self {
            repository: repository.into(),
            name,
            container,
            volume,
            removed: false,
            _lease: lease,
        };
        let result = (|| {
            let status = buildx_command(repository)?
                .args(["inspect", "--bootstrap", &worker.name])
                .status()?;
            ensure!(
                status.success(),
                "bootstrapping experiment worker failed: {status}"
            );
            let version = output_text(
                "docker",
                ["exec", &worker.container, "buildkitd", "--version"],
                Some(repository),
            )?;
            ensure!(
                version
                    .split_whitespace()
                    .any(|part| part == BUILDKIT_VERSION),
                "experiment worker BuildKit version differs from the admitted pin"
            );
            resources::validate_container(repository, &worker.container)?;
            ensure!(
                worker.cache_records(None)? == 0,
                "second worker did not start with an empty BuildKit cache"
            );
            Ok(())
        })();
        if let Err(error) = result {
            worker.remove()?;
            return Err(error);
        }
        Ok(worker)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn identity(&self) -> Result<String> {
        let identity = output_text(
            "docker",
            [
                "exec",
                &self.container,
                "buildctl",
                "debug",
                "workers",
                "--format",
                "{{range .}}{{.ID}}{{end}}",
            ],
            Some(&self.repository),
        )?;
        ensure!(
            !identity.trim().is_empty(),
            "experiment worker has no identity"
        );
        Ok(identity.trim().into())
    }

    pub fn cache_records(&self, filter: Option<&str>) -> Result<u64> {
        let mut args = vec![
            "exec",
            &self.container,
            "buildctl",
            "du",
            "--format",
            "{{len .}}",
        ];
        if let Some(filter) = filter {
            args.extend(["--filter", filter]);
        }
        output_text("docker", args, Some(&self.repository))?
            .trim()
            .parse()
            .context("reading experiment cache record count")
    }

    pub fn cpu_snapshot(&self) -> Result<crate::CpuSnapshot> {
        resources::snapshot_for(&self.repository, &self.container)
    }

    pub fn remove(&mut self) -> Result<()> {
        if self.removed {
            return Ok(());
        }
        let status = buildx_command(&self.repository)?
            .args(["rm", &self.name])
            .status()?;
        ensure!(
            status.success(),
            "removing experiment worker failed: {status}"
        );
        ensure!(
            listed(&self.repository, "volume", &self.volume)?.is_empty(),
            "experiment state volume remains after removal"
        );
        let containers = output_text(
            "docker",
            [
                "ps",
                "--all",
                "--filter",
                &format!("name=^{}$", self.container),
                "--format",
                "{{.Names}}",
            ],
            Some(&self.repository),
        )?;
        ensure!(
            containers.trim().is_empty(),
            "experiment container remains after removal"
        );
        self.removed = true;
        Ok(())
    }
}

impl Drop for ExperimentWorker<'_> {
    fn drop(&mut self) {
        if let Err(error) = self.remove() {
            eprintln!("Removing experiment worker {}: {error:#}", self.name);
        }
    }
}

fn listed(repository: &Path, kind: &str, name: &str) -> Result<String> {
    Ok(output_text(
        "docker",
        [
            kind,
            "ls",
            "--filter",
            &format!("name=^{name}$"),
            "--format",
            "{{.Name}}",
        ],
        Some(repository),
    )?
    .trim()
    .into())
}
