use rmcp::{ErrorData as McpError, RoleServer, service::RequestContext};
use veoveo_mcp_contract::{GatewayInternalIdentity, PlaneCaller};

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
    let bearer_token = parts
        .extensions
        .get::<super::auth::ForwardedBearer>()
        .map(|bearer| bearer.0.clone())
        .ok_or_else(|| McpError::invalid_request("forwarded bearer missing", None))?;
    Ok(plane_caller(identity, bearer_token))
}

pub(super) fn plane_caller(identity: GatewayInternalIdentity, bearer_token: String) -> PlaneCaller {
    let memberships = identity.principal.group_memberships();
    PlaneCaller {
        bearer_token,
        identity,
        memberships,
    }
}

pub(super) fn runtime_owner(identity: &GatewayInternalIdentity) -> veoveo_task_runtime::TaskOwner {
    veoveo_task_runtime::TaskOwner {
        principal_key: identity.principal.id.to_string(),
        principal_kind: match identity.principal.kind {
            veoveo_mcp_contract::PrincipalKind::User => veoveo_task_runtime::PrincipalKind::User,
            veoveo_mcp_contract::PrincipalKind::Service => {
                veoveo_task_runtime::PrincipalKind::Service
            }
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
