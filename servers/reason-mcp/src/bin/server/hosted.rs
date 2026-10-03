//! The hosted Reason server, shared by the binary and the hosted tests.
use std::sync::Arc;

use veoveo_mcp_contract::{
    GatewayInternalTrustBundle, PublicDeployment,
    hosting::{Hosted, HostedServer},
};
use veoveo_task_runtime::DurableTasks;

use super::{AppState, ReasonListener, ReasonMcp, ReasonTaskService};

/// Builds the hosted Reason server for `deployment`, trusting `trust`.
pub(super) fn server(
    state: Arc<AppState>,
    deployment: &PublicDeployment,
    allow_loopback_hosts: bool,
    allowed_hosts: Vec<String>,
    trust: GatewayInternalTrustBundle,
) -> anyhow::Result<HostedServer> {
    let readiness_state = state.clone();
    Ok(HostedServer::for_domain::<ReasonMcp>()
        .deployment(deployment, allow_loopback_hosts)?
        .allowed_hosts(allowed_hosts)
        .internal_trust(trust)?
        .handler(move || {
            Hosted::new(ReasonMcp::new(state.clone())).with_tasks(DurableTasks::with_listener(
                ReasonTaskService::new(state.clone()),
                ReasonListener {
                    state: state.clone(),
                },
            ))
        })
        .readiness(move || {
            let state = readiness_state.clone();
            async move { ready(&state).await }
        })
        .build())
}

/// Ready while the recording cache, Store and reasoning runner are.
async fn ready(state: &AppState) -> bool {
    if let Err(error) = state.recordings.readiness() {
        tracing::warn!("recording cache readiness failure: {error}");
        return false;
    }
    if let Err(error) = state.tasks.platform_store().healthcheck().await {
        tracing::warn!("reason readiness database failure: {error}");
        return false;
    }
    if let Err(error) = state.executor.readiness() {
        tracing::warn!("reason readiness runner failure: {error}");
        return false;
    }
    true
}
