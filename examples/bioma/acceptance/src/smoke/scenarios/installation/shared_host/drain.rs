//! The installation-owned DuckDB process drain reuses the selected-container watch.
use anyhow::{Result, ensure};
use serde::Deserialize;
use std::time::Duration;
use veoveo_gateway_composition::smoke_support::InstalledTarget;
use veoveo_testing_support::{
    SmokeMcpClient,
    installed::restart::{
        DeploymentRestart, DrainProfile, DrainReceipt, SelectedDrainIdentity, SelectedDrainTarget,
    },
};
use veoveo_types::ResourceAddress;

const DUCKDB_DEPLOYMENT: &str = "duckdb-mcp";
const DRAIN_DEADLINE: Duration = Duration::from_secs(30);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct DrainInput {
    deployment: String,
    pod: String,
    container: String,
}

impl DrainInput {
    fn require_declared_server(&self, deployments: &[String]) -> Result<()> {
        ensure!(
            self.deployment == DUCKDB_DEPLOYMENT
                && deployments.iter().any(|name| name == &self.deployment),
            "shared-host drain requires the declared duckdb-mcp Deployment"
        );
        ensure!(
            self.container == DUCKDB_DEPLOYMENT,
            "shared-host drain requires the duckdb-mcp server container"
        );
        Ok(())
    }
}

pub(super) struct SelectedServerDrain {
    restart: DeploymentRestart,
    selected: SelectedDrainTarget,
}

impl SelectedServerDrain {
    pub(super) fn identity(&self) -> SelectedDrainIdentity {
        self.selected.identity()
    }

    /// Consume the admission once, after the caller has verified its completed Task.
    /// An ambiguous restart result cannot be retried through this value.
    pub(super) async fn restart(self) -> Result<DrainReceipt> {
        self.restart.restart_with_drain(&self.selected).await
    }
}

/// The caller uses its installation-selected public OAuth endpoint. Selection has
/// no mutation; the shared helper admits Pod/ReplicaSet/Deployment identities and
/// running readiness before establishing the watch and dispatching the later restart.
pub(super) async fn select(
    installation: &InstalledTarget,
    caller: &SmokeMcpClient,
    input: &DrainInput,
) -> Result<SelectedServerDrain> {
    input.require_declared_server(&installation.target.expected_deployments)?;
    let restart = DeploymentRestart::new(
        &installation.target,
        &input.deployment,
        DUCKDB_DEPLOYMENT,
        caller.peer().clone(),
        veoveo_duckdb_mcp::contract::DuckDbResource::Contract.to_uri()?,
    )?;
    let selected = restart
        .select_drain_target(
            &input.pod,
            DrainProfile::server(&input.container, DRAIN_DEADLINE)?,
        )
        .await?;
    Ok(SelectedServerDrain { restart, selected })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(deployment: &str, container: &str) -> DrainInput {
        DrainInput {
            deployment: deployment.into(),
            pod: "duckdb-mcp-abc-123".into(),
            container: container.into(),
        }
    }

    #[test]
    fn drain_requires_the_declared_duckdb_workload() {
        let selected = input(DUCKDB_DEPLOYMENT, DUCKDB_DEPLOYMENT);
        assert!(selected.require_declared_server(&[]).is_err());
        assert!(
            selected
                .require_declared_server(&["gateway".into()])
                .is_err()
        );
        assert!(
            selected
                .require_declared_server(&[DUCKDB_DEPLOYMENT.into()])
                .is_ok()
        );
        assert!(
            input("gateway", DUCKDB_DEPLOYMENT)
                .require_declared_server(&["gateway".into(), DUCKDB_DEPLOYMENT.into()])
                .is_err()
        );
    }

    #[test]
    fn a_ready_sidecar_cannot_substitute_for_the_server_process() {
        assert!(
            input(DUCKDB_DEPLOYMENT, "proxy")
                .require_declared_server(&[DUCKDB_DEPLOYMENT.into()])
                .is_err()
        );
    }
}
