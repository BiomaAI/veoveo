use rmcp::{ErrorData as McpError, RoleServer, service::RequestContext};
use veoveo_coordinates_mcp::state::CoordinateScope;
use veoveo_mcp_contract::{GatewayInternalIdentity, PlaneCaller, PrincipalKind};
use veoveo_platform_store::PrincipalKind as StorePrincipalKind;
use veoveo_task_runtime::TaskOwner;

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
        .map(|bearer| bearer.0.clone())
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

pub(super) fn runtime_owner(identity: &GatewayInternalIdentity) -> TaskOwner {
    TaskOwner {
        principal_key: identity.principal.id.to_string(),
        principal_kind: match identity.principal.kind {
            PrincipalKind::User => veoveo_task_runtime::PrincipalKind::User,
            PrincipalKind::Service => veoveo_task_runtime::PrincipalKind::Service,
        },
        issuer: identity.principal.issuer.to_string(),
        subject: identity.principal.subject.to_string(),
        profile: identity.profile.to_string(),
        tenant_key: identity.principal.tenant.as_ref().map(ToString::to_string),
        data_labels: identity
            .principal
            .data_labels
            .iter()
            .map(ToString::to_string)
            .collect(),
    }
}

pub(super) async fn coordinate_scope_from_identity(
    state: &AppState,
    identity: &GatewayInternalIdentity,
) -> Result<CoordinateScope, McpError> {
    coordinate_scope_from_runtime(state, &runtime_owner(identity)).await
}

pub(super) async fn coordinate_scope_from_runtime(
    state: &AppState,
    owner: &TaskOwner,
) -> Result<CoordinateScope, McpError> {
    let identity = state
        .tasks
        .platform_store()
        .ensure_identity(
            owner.tenant_key(),
            &owner.principal_key,
            &owner.issuer,
            &owner.subject,
            match owner.principal_kind {
                veoveo_task_runtime::PrincipalKind::User => StorePrincipalKind::User,
                veoveo_task_runtime::PrincipalKind::Service => StorePrincipalKind::Service,
            },
        )
        .await
        .map_err(|error| McpError::internal_error(error.to_string(), None))?;
    Ok(CoordinateScope {
        identity,
        data_labels: owner.data_labels.clone(),
    })
}

pub(super) async fn optional_task_owner(
    state: &AppState,
    task_id: &str,
) -> Result<Option<TaskOwner>, McpError> {
    state
        .tasks
        .owner(task_id)
        .await
        .map_err(|error| McpError::internal_error(error.to_string(), None))
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
            "coordinates task policy denied request",
            None,
        ))
    }
}

pub(super) fn task_owner_allows(owner: &TaskOwner, identity: &GatewayInternalIdentity) -> bool {
    let caller = runtime_owner(identity);
    owner.allows(
        &caller.principal_key,
        &caller.profile,
        caller.tenant_key.as_deref(),
        &caller.data_labels,
    )
}
