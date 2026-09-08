use std::{path::PathBuf, process::ExitCode};

use anyhow::Result;
use clap::{Parser, Subcommand};

mod flux_cancellation;
mod gitops;
mod helm_config;

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
    /// Render and validate Helm, image packaging, and GitOps configuration.
    HelmConfig,
    /// Verify obsolete Flux health-check cancellation in a disposable namespace.
    GitopsCancelVerify(flux_cancellation::Args),
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
    /// Observe an exact GitOps revision through deployment readiness.
    GitopsConverge {
        /// Observe passively, or explicitly request Flux reconciliation.
        #[arg(long, value_enum, default_value_t = gitops::ReconciliationMode::Observe)]
        reconciliation: gitops::ReconciliationMode,
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
        Command::HelmConfig => helm_config::helm_config(),
        Command::GitopsCancelVerify(args) => flux_cancellation::verify(args),
        Command::ProfileValidate { profile } => veoveo_deploy_runtime::profile_validate(&profile),
        Command::ProfileRegistryUp { profile } => {
            veoveo_deploy_runtime::profile_registry_up(&profile)
        }
        Command::ProfileClusterUp { profile } => {
            veoveo_deploy_runtime::profile_cluster_up(&profile)
        }
        Command::ProfileClusterStop { profile } => {
            veoveo_deploy_runtime::profile_cluster_stop(&profile)
        }
        Command::ProfileClusterDelete { profile } => {
            veoveo_deploy_runtime::profile_cluster_delete(&profile)
        }
        Command::ProfileUp { profile, lock } => veoveo_deploy_runtime::profile_up(&profile, &lock),
        Command::ProfileGpuVerify { profile } => {
            veoveo_deploy_runtime::profile_gpu_verify(&profile)
        }
        Command::ProfileDown { profile } => veoveo_deploy_runtime::profile_down(&profile),
        Command::GitopsConverge {
            reconciliation,
            context,
            source,
            root,
            release,
            revision,
            deployment,
            timeout_seconds,
            evidence_output,
        } => gitops::converge(gitops::GitopsConvergeArgs {
            reconciliation,
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
