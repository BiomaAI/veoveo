use std::collections::HashMap;

use rmcp::{ErrorData as McpError, RoleServer, service::RequestContext};
use veoveo_mcp_contract::GatewayInternalIdentity;
use veoveo_optimization_mcp::state::{ArtifactOwner, TaskOwner};

use super::app_state::AppState;

pub(super) type TaskOwnerMap = HashMap<String, TaskOwner>;

pub(super) fn internal_identity(
    context: &RequestContext<RoleServer>,
) -> Result<GatewayInternalIdentity, McpError> {
    let parts = context
        .extensions
        .get::<axum::http::request::Parts>()
        .ok_or_else(|| McpError::invalid_request("authenticated HTTP context missing", None))?;
    parts
        .extensions
        .get::<GatewayInternalIdentity>()
        .cloned()
        .ok_or_else(|| McpError::invalid_request("gateway identity missing", None))
}

pub(super) fn task_owner_from_identity(
    task_id: &str,
    identity: &GatewayInternalIdentity,
) -> TaskOwner {
    TaskOwner {
        task_id: task_id.to_string(),
        principal_id: identity.principal.id.clone(),
        profile: identity.profile.clone(),
        tenant: identity.principal.tenant.clone(),
        data_labels: identity.principal.data_labels.clone(),
    }
}

pub(super) async fn optional_task_owner(
    state: &AppState,
    task_id: &str,
) -> Result<Option<TaskOwner>, McpError> {
    if let Some(owner) = state.task_owners.read().await.get(task_id).cloned() {
        return Ok(Some(owner));
    }
    let Some(owner) = state
        .durable
        .task_owner(task_id)
        .map_err(|err| McpError::internal_error(err.to_string(), None))?
    else {
        return Ok(None);
    };
    state
        .task_owners
        .write()
        .await
        .insert(task_id.to_string(), owner.clone());
    Ok(Some(owner))
}

pub(super) async fn require_task_owner(
    state: &AppState,
    context: &RequestContext<RoleServer>,
    task_id: &str,
) -> Result<GatewayInternalIdentity, McpError> {
    let identity = internal_identity(context)?;
    let owner = optional_task_owner(state, task_id)
        .await?
        .ok_or_else(|| McpError::invalid_request("task ownership record missing", None))?;
    if task_owner_allows(&owner, &identity) {
        Ok(identity)
    } else {
        Err(McpError::invalid_request(
            "optimization task policy denied request",
            None,
        ))
    }
}

pub(super) fn artifact_owner_from_task(sha256: &str, owner: &TaskOwner) -> ArtifactOwner {
    ArtifactOwner {
        sha256: sha256.to_string(),
        task_id: owner.task_id.clone(),
        principal_id: owner.principal_id.clone(),
        profile: owner.profile.clone(),
        tenant: owner.tenant.clone(),
        data_labels: owner.data_labels.clone(),
    }
}

pub(super) fn artifact_owned_by(
    state: &AppState,
    sha256: &str,
    identity: &GatewayInternalIdentity,
) -> Result<(), McpError> {
    let owners = state
        .durable
        .artifact_owners(sha256)
        .map_err(|err| McpError::internal_error(err.to_string(), None))?;
    if owners
        .iter()
        .any(|owner| artifact_owner_allows_identity(owner, identity))
    {
        Ok(())
    } else {
        Err(McpError::invalid_request(
            "optimization artifact policy denied request",
            None,
        ))
    }
}

pub(super) fn task_owner_allows(owner: &TaskOwner, identity: &GatewayInternalIdentity) -> bool {
    owner.principal_id == identity.principal.id
        && owner.profile == identity.profile
        && owner.tenant == identity.principal.tenant
        && owner.data_labels.is_subset(&identity.principal.data_labels)
}

fn artifact_owner_allows_identity(
    owner: &ArtifactOwner,
    identity: &GatewayInternalIdentity,
) -> bool {
    owner.principal_id == identity.principal.id
        && owner.profile == identity.profile
        && owner.tenant == identity.principal.tenant
        && owner.data_labels.is_subset(&identity.principal.data_labels)
}
