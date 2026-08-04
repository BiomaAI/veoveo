use std::{path::PathBuf, process::ExitCode};

use anyhow::Result;
use clap::{Parser, Subcommand};

#[path = "../../smoke/src/bin/smoke/deployment.rs"]
mod deployment;

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
