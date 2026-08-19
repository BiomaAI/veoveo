use std::{path::PathBuf, process::ExitCode};

use anyhow::Result;
use clap::{Parser, Subcommand};

#[path = "../../smoke/src/bin/smoke/deployment.rs"]
mod deployment;
mod gitops;

#[derive(Debug, Parser)]
#[command(
    name = "deployment-smoke",
    about = "Focused VeoVeo deployment-profile harness"
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Validate one typed deployment profile and every selected build and Helm surface.
    ProfileValidate {
        #[arg(long)]
        profile: PathBuf,
    },
    /// Start the standalone local registry selected by a deployment profile.
    ProfileRegistryUp {
        #[arg(long)]
        profile: PathBuf,
    },
    /// Create or start the local k3d cluster selected by a deployment profile.
    ProfileClusterUp {
        #[arg(long)]
        profile: PathBuf,
    },
    /// Stop the local k3d cluster selected by a deployment profile.
    ProfileClusterStop {
        #[arg(long)]
        profile: PathBuf,
    },
    /// Delete the local k3d cluster selected by a deployment profile.
    ProfileClusterDelete {
        #[arg(long)]
        profile: PathBuf,
    },
    /// Apply a profile's resources and independently resolved Helm releases.
    ProfileUp {
        #[arg(long)]
        profile: PathBuf,
        #[arg(long)]
        lock: PathBuf,
    },
    /// Verify the live GPU placement selected by a deployment profile without mutating it.
    ProfileGpuVerify {
        #[arg(long)]
        profile: PathBuf,
    },
    /// Uninstall every Helm release selected by a deployment profile.
    ProfileDown {
        #[arg(long)]
        profile: PathBuf,
    },
    /// Refresh and observe an exact GitOps revision through deployment readiness.
    GitopsConverge {
        #[arg(long)]
        context: String,
        #[arg(long)]
        source: String,
        #[arg(long)]
        root: String,
        #[arg(long, required = true)]
        release: Vec<String>,
        #[arg(long)]
        revision: String,
        #[arg(long, required = true, value_name = "NAMESPACE/NAME")]
        deployment: Vec<String>,
        #[arg(long, default_value_t = 300)]
        timeout_seconds: u64,
        #[arg(long)]
        evidence_output: PathBuf,
    },
}

fn run() -> Result<()> {
    match Args::parse().command {
        Command::ProfileValidate { profile } => deployment::profile_validate(&profile),
        Command::ProfileRegistryUp { profile } => deployment::profile_registry_up(&profile),
        Command::ProfileClusterUp { profile } => deployment::profile_cluster_up(&profile),
        Command::ProfileClusterStop { profile } => deployment::profile_cluster_stop(&profile),
        Command::ProfileClusterDelete { profile } => deployment::profile_cluster_delete(&profile),
        Command::ProfileUp { profile, lock } => deployment::profile_up(&profile, &lock),
        Command::ProfileGpuVerify { profile } => deployment::profile_gpu_verify(&profile),
        Command::ProfileDown { profile } => deployment::profile_down(&profile),
        Command::GitopsConverge {
            context,
            source,
            root,
            release,
            revision,
            deployment,
            timeout_seconds,
            evidence_output,
        } => gitops::converge(gitops::GitopsConvergeArgs {
            context,
            source,
            root,
            releases: release,
            revision,
            deployments: deployment,
            timeout: std::time::Duration::from_secs(timeout_seconds),
            evidence_output,
        }),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error:#}");
            ExitCode::FAILURE
        }
    }
}
