use rmcp::{ErrorData as McpError, RoleServer, service::RequestContext};
use veoveo_mcp_contract::{GatewayInternalIdentity, PlaneCaller};
use veoveo_task_runtime::TaskOwner;

pub(super) fn internal_identity(
    context: &RequestContext<RoleServer>,
) -> Result<GatewayInternalIdentity, McpError> {
    let parts = context
        .extensions
        .get::<axum::http::request::Parts>()
        .ok_or_else(|| {
            McpError::invalid_request(veoveo_mcp_contract::GATEWAY_ROUTING_REQUIRED, None)
        })?;
    parts
        .extensions
        .get::<GatewayInternalIdentity>()
        .cloned()
        .ok_or_else(|| {
            McpError::invalid_request(veoveo_mcp_contract::GATEWAY_ROUTING_REQUIRED, None)
        })
}

pub(super) fn internal_caller(
    context: &RequestContext<RoleServer>,
) -> Result<PlaneCaller, McpError> {
    let parts = context
        .extensions
        .get::<axum::http::request::Parts>()
        .ok_or_else(|| {
            McpError::invalid_request(veoveo_mcp_contract::GATEWAY_ROUTING_REQUIRED, None)
        })?;
    let identity = parts
        .extensions
        .get::<GatewayInternalIdentity>()
        .cloned()
        .ok_or_else(|| {
            McpError::invalid_request(veoveo_mcp_contract::GATEWAY_ROUTING_REQUIRED, None)
        })?;
    let bearer = parts
        .extensions
        .get::<super::internal_auth::ForwardedBearer>()
        .map(|bearer| bearer.0.clone())
        .ok_or_else(|| {
            McpError::invalid_request(veoveo_mcp_contract::GATEWAY_ROUTING_REQUIRED, None)
        })?;
    Ok(caller_from(identity, bearer))
}

pub(super) fn caller_from(identity: GatewayInternalIdentity, bearer: String) -> PlaneCaller {
    let memberships = identity.actor.group_memberships();
    PlaneCaller {
        bearer_token: bearer,
        identity,
        memberships,
    }
}

pub(super) fn runtime_owner(identity: &GatewayInternalIdentity) -> TaskOwner {
    TaskOwner {
        principal_key: identity.actor.id.as_str().to_owned(),
        principal_kind: match identity.actor.kind {
            veoveo_mcp_contract::PrincipalKind::User => veoveo_task_runtime::PrincipalKind::User,
            veoveo_mcp_contract::PrincipalKind::Service => {
                veoveo_task_runtime::PrincipalKind::Service
            }
        },
        issuer: identity.actor.issuer.as_str().to_owned(),
        subject: identity.actor.subject.as_str().to_owned(),
        profile: identity.profile.as_str().to_owned(),
        tenant_key: identity
            .actor
            .tenant
            .as_ref()
            .map(|tenant| tenant.as_str().to_owned()),
        data_labels: identity
            .actor
            .data_labels
            .iter()
            .map(|label| label.as_str().to_owned())
            .collect(),
        authority: identity.authority.clone(),
    }
}
