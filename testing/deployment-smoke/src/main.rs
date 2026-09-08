use std::{path::PathBuf, process::ExitCode};

use anyhow::Result;
use clap::{Parser, Subcommand};

mod component_scope;
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
    /// Verify selected deployment writes with independent Git and OCI fixtures.
    ComponentScopeVerify(component_scope::Args),
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
    /// Apply exact components and their dependencies from one deployment lock.
    ProfileUp {
        #[arg(long)]
        profile: PathBuf,
        #[arg(long)]
        lock: PathBuf,
        #[arg(
            long,
            required_unless_present = "all_components",
            conflicts_with = "all_components"
        )]
        component: Vec<String>,
        #[arg(long)]
        all_components: bool,
        #[arg(long)]
        receipt_output: PathBuf,
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
        Command::ComponentScopeVerify(args) => component_scope::verify(args),
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
        Command::ProfileUp {
            profile,
            lock,
            component,
            all_components,
            receipt_output,
        } => {
            use veoveo_deploy_contract::components::{ComponentId, ComponentSelection};
            anyhow::ensure!(
                !receipt_output.exists(),
                "installation receipt already exists"
            );
            let parent = receipt_output
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| std::path::Path::new("."));
            std::fs::create_dir_all(parent)?;
            let temporary = tempfile::NamedTempFile::new_in(parent)?;
            let selection = if all_components {
                ComponentSelection::All
            } else {
                let ids = component
                    .iter()
                    .cloned()
                    .map(ComponentId::try_from)
                    .collect::<Result<std::collections::BTreeSet<_>>>()?;
                anyhow::ensure!(
                    ids.len() == component.len(),
                    "duplicate component selection"
                );
                ComponentSelection::Exact(ids)
            };
            let receipt = veoveo_deploy_runtime::profile_up(&profile, &lock, &selection)?;
            serde_json::to_writer_pretty(temporary.as_file(), &receipt)?;
            temporary.as_file().sync_all()?;
            temporary.persist_noclobber(receipt_output)?;
            Ok(())
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_up_requires_explicit_selection_and_receipt() {
        let base = [
            "deployment-smoke",
            "profile-up",
            "--profile",
            "profile.json",
            "--lock",
            "lock.json",
        ];
        let parse = |extra: &[&str]| {
            Args::try_parse_from(base.iter().copied().chain(extra.iter().copied()))
        };
        assert!(parse(&[]).is_err());
        assert!(parse(&["--all-components"]).is_err());
        assert!(parse(&["--receipt-output", "receipt.json"]).is_err());
        assert!(parse(&["--receipt-output", "receipt.json", "--all-components"]).is_ok());
        assert!(
            parse(&[
                "--receipt-output",
                "receipt.json",
                "--component",
                "platform",
                "--component",
                "extension"
            ])
            .is_ok()
        );
        assert!(
            parse(&[
                "--receipt-output",
                "receipt.json",
                "--all-components",
                "--component",
                "platform"
            ])
            .is_err()
        );
    }
}
