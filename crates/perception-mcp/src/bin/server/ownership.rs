use std::collections::BTreeSet;

use rmcp::{ErrorData as McpError, RoleServer, service::RequestContext};
use veoveo_mcp_contract::{
    DataLabelId, GatewayInternalIdentity, PlaneCaller, PrincipalId, PrincipalKind, TenantId,
    TokenIssuer, TokenSubject,
};
use veoveo_recording_mcp::RecordingReadAuthority;
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

pub(super) fn recording_authority_from_identity(
    identity: &GatewayInternalIdentity,
) -> RecordingReadAuthority {
    RecordingReadAuthority::from_gateway(identity)
}

pub(super) fn recording_authority_from_runtime(
    owner: &TaskOwner,
) -> Result<RecordingReadAuthority, String> {
    Ok(RecordingReadAuthority::new(
        PrincipalId::new(owner.principal_key.clone()).map_err(|error| error.to_string())?,
        match owner.principal_kind {
            veoveo_task_runtime::PrincipalKind::User => PrincipalKind::User,
            veoveo_task_runtime::PrincipalKind::Service => PrincipalKind::Service,
        },
        TokenIssuer::new(owner.issuer.clone()).map_err(|error| error.to_string())?,
        TokenSubject::new(owner.subject.clone()).map_err(|error| error.to_string())?,
        owner
            .tenant_key
            .clone()
            .map(TenantId::new)
            .transpose()
            .map_err(|error| error.to_string())?,
        owner
            .data_labels
            .iter()
            .cloned()
            .map(DataLabelId::new)
            .collect::<Result<BTreeSet<_>, _>>()
            .map_err(|error| error.to_string())?,
    ))
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

pub(super) async fn require_task_owner(
    state: &AppState,
    context: &RequestContext<RoleServer>,
    task_id: &str,
) -> Result<GatewayInternalIdentity, McpError> {
    let identity = internal_identity(context)?;
    let owner = state
        .tasks
        .owner(task_id)
        .await
        .map_err(|error| McpError::internal_error(error.to_string(), None))?
        .ok_or_else(|| McpError::resource_not_found("analysis not found", None))?;
    if task_owner_allows(&owner, &identity) {
        Ok(identity)
    } else {
        Err(McpError::resource_not_found("analysis not found", None))
    }
}
