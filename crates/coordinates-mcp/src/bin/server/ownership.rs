use std::collections::{BTreeSet, HashMap};

use rmcp::{ErrorData as McpError, RoleServer, service::RequestContext};
use veoveo_mcp_contract::{
    DataLabelId, GatewayInternalIdentity, GatewayProfileId, PlaneCaller, PrincipalId, TenantId,
};

use super::app_state::AppState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TaskOwner {
    pub(super) task_id: String,
    pub(super) principal_id: PrincipalId,
    pub(super) profile: GatewayProfileId,
    pub(super) tenant: Option<TenantId>,
    pub(super) data_labels: BTreeSet<DataLabelId>,
}

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

pub(super) fn internal_caller(
    context: &RequestContext<RoleServer>,
) -> Result<PlaneCaller, McpError> {
    let parts = context
        .extensions
        .get::<axum::http::request::Parts>()
        .ok_or_else(|| McpError::invalid_request("authenticated HTTP context missing", None))?;
    let identity = parts
        .extensions
        .get::<GatewayInternalIdentity>()
        .cloned()
        .ok_or_else(|| McpError::invalid_request("gateway identity missing", None))?;
    let bearer = parts
        .extensions
        .get::<super::internal_auth::ForwardedBearer>()
        .map(|b| b.0.clone())
        .ok_or_else(|| McpError::invalid_request("forwarded bearer missing", None))?;
    Ok(caller_from(identity, bearer))
}

pub(super) fn caller_from(identity: GatewayInternalIdentity, bearer: String) -> PlaneCaller {
    let memberships = identity.principal.group_memberships();
    PlaneCaller {
        bearer_token: bearer,
        identity,
        memberships,
    }
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

pub(super) async fn optional_task_owner(state: &AppState, task_id: &str) -> Option<TaskOwner> {
    state.task_owners.read().await.get(task_id).cloned()
}

pub(super) async fn require_task_owner(
    state: &AppState,
    context: &RequestContext<RoleServer>,
    task_id: &str,
) -> Result<GatewayInternalIdentity, McpError> {
    let identity = internal_identity(context)?;
    let owner = optional_task_owner(state, task_id)
        .await
        .ok_or_else(|| McpError::invalid_request("task ownership record missing", None))?;
    if task_owner_allows(&owner, &identity) {
        Ok(identity)
    } else {
        Err(McpError::invalid_request(
            "coordinates task policy denied request",
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
