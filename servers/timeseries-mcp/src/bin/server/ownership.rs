use rmcp::{ErrorData as McpError, RoleServer, service::RequestContext};
use veoveo_mcp_contract::{GatewayInternalIdentity, PlaneCaller};
use veoveo_timeseries_mcp::state::TaskOwner;

use super::app_state::AppState;

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

/// Build the PlaneCaller for artifact-plane calls: the verified identity plus
/// the raw bearer to forward. Group memberships come from the signed identity
/// via Principal::group_memberships() (bare membership = Read).
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

/// Assemble a PlaneCaller from a verified identity and its raw bearer.
pub(super) fn caller_from(identity: GatewayInternalIdentity, bearer: String) -> PlaneCaller {
    let memberships = identity.actor.group_memberships();
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
        principal_id: identity.actor.id.clone(),
        profile: identity.profile.clone(),
        tenant: identity.actor.tenant.clone(),
        data_labels: identity.actor.data_labels.clone(),
    }
}

pub(super) fn task_owner_from_runtime(
    task_id: &str,
    owner: &veoveo_task_runtime::TaskOwner,
) -> Result<TaskOwner, String> {
    Ok(TaskOwner {
        task_id: task_id.to_owned(),
        principal_id: veoveo_mcp_contract::PrincipalId::new(owner.principal_key.clone())
            .map_err(|error| error.to_string())?,
        profile: veoveo_mcp_contract::GatewayProfileId::new(owner.profile.clone())
            .map_err(|error| error.to_string())?,
        tenant: owner
            .tenant_key
            .clone()
            .map(veoveo_mcp_contract::TenantId::new)
            .transpose()
            .map_err(|error| error.to_string())?,
        data_labels: owner
            .data_labels
            .iter()
            .cloned()
            .map(veoveo_mcp_contract::DataLabelId::new)
            .collect::<Result<_, _>>()
            .map_err(|error| error.to_string())?,
    })
}

pub(super) async fn optional_task_owner(
    state: &AppState,
    task_id: &str,
) -> Result<Option<TaskOwner>, McpError> {
    let Some(owner) = state
        .tasks
        .owner(task_id)
        .await
        .map_err(|err| McpError::internal_error(err.to_string(), None))?
    else {
        return Ok(None);
    };
    Ok(Some(TaskOwner {
        task_id: task_id.to_owned(),
        principal_id: veoveo_mcp_contract::PrincipalId::new(owner.principal_key)
            .map_err(|err| McpError::internal_error(err.to_string(), None))?,
        profile: veoveo_mcp_contract::GatewayProfileId::new(owner.profile)
            .map_err(|err| McpError::internal_error(err.to_string(), None))?,
        tenant: owner
            .tenant_key
            .map(veoveo_mcp_contract::TenantId::new)
            .transpose()
            .map_err(|err| McpError::internal_error(err.to_string(), None))?,
        data_labels: owner
            .data_labels
            .into_iter()
            .map(veoveo_mcp_contract::DataLabelId::new)
            .collect::<Result<_, _>>()
            .map_err(|err| McpError::internal_error(err.to_string(), None))?,
    }))
}

pub(super) fn runtime_owner(identity: &GatewayInternalIdentity) -> veoveo_task_runtime::TaskOwner {
    veoveo_task_runtime::TaskOwner {
        principal_key: identity.actor.id.to_string(),
        principal_kind: match identity.actor.kind {
            veoveo_mcp_contract::PrincipalKind::User => veoveo_task_runtime::PrincipalKind::User,
            veoveo_mcp_contract::PrincipalKind::Service => {
                veoveo_task_runtime::PrincipalKind::Service
            }
        },
        issuer: identity.actor.issuer.to_string(),
        subject: identity.actor.subject.to_string(),
        profile: identity.profile.to_string(),
        tenant_key: identity.actor.tenant.as_ref().map(ToString::to_string),
        data_labels: identity
            .actor
            .data_labels
            .iter()
            .map(ToString::to_string)
            .collect(),
        authority: identity.authority.clone(),
    }
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
            "timeseries task policy denied request",
            None,
        ))
    }
}

pub(super) fn task_owner_allows(owner: &TaskOwner, identity: &GatewayInternalIdentity) -> bool {
    owner.principal_id == identity.actor.id
        && owner.profile == identity.profile
        && owner.tenant == identity.actor.tenant
        && owner.data_labels.is_subset(&identity.actor.data_labels)
}
