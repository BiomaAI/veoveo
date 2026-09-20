//! The App discovers managed message targets; a target never grants vehicle control.
use std::sync::Arc;

use anyhow::Result;
use futures::StreamExt;
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::{GatewayInternalIdentity, SubscriptionHub};
use veoveo_platform_store::{
    PlatformStore, PlatformTable, ResourceChangeTable, deterministic_tenant_id,
    deterministic_work_context_id,
};

use crate::contract::SessionId;

pub(super) async fn targets(
    store: &PlatformStore,
    identity: &GatewayInternalIdentity,
    session: &SessionId,
) -> Result<Vec<String>> {
    let tenant = deterministic_tenant_id(identity.authority.tenant.as_str())?.record_id();
    let context = deterministic_work_context_id(
        identity.authority.tenant.as_str(),
        identity.authority.work_context.as_str(),
    )?
    .record_id();
    scoped_targets(store, tenant, context, session).await
}

async fn scoped_targets(
    store: &PlatformStore,
    tenant: surrealdb::types::RecordId,
    context: surrealdb::types::RecordId,
    session: &SessionId,
) -> Result<Vec<String>> {
    let mut response = store
        .client()
        .query(include_str!("agent_targets.surql"))
        .bind(("tenant", tenant))
        .bind(("context", context))
        .bind(("simulation_session", session.to_string()))
        .await?
        .check()?;
    Ok(response.take(2)?)
}

/// Shared projected LIVE sources recover with a fresh invalidation after a gap.
/// Only catalog metadata changes here; observation never wakes a model episode.
pub(super) async fn observe(
    store: PlatformStore,
    subscribers: Arc<SubscriptionHub>,
    shutdown: CancellationToken,
) {
    let mut changes = store.resource_changes(vec![
        ResourceChangeTable::ManagedAgent,
        ResourceChangeTable::AgentDefinition,
        ResourceChangeTable::UavVehicleControlGrant,
        PlatformTable::Principal.into(),
        PlatformTable::Tenant.into(),
        ResourceChangeTable::WorkContext,
    ]);
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            changed = changes.next() => {
                if changed.is_none() { break; }
                subscribers.notify_resource_list_changed().await;
            }
        }
    }
}

#[cfg(test)]
#[path = "agent_targets_tests.rs"]
mod tests;
