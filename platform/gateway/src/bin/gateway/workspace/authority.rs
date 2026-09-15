use axum::http::StatusCode;
use veoveo_mcp_contract::{InvocationMode, PrincipalKind};
use veoveo_mcp_gateway::AuthenticatedSubject;
use veoveo_platform_store::{
    WorkContextMembershipLevel, deterministic_principal_id, deterministic_tenant_id,
    deterministic_work_context_id, workspace::WorkspaceAuthority,
};

use super::{WorkspaceState, fault};

pub(super) async fn admit(
    state: &WorkspaceState,
    subject: &AuthenticatedSubject,
) -> Result<WorkspaceAuthority, StatusCode> {
    if subject.principal.kind != PrincipalKind::User
        || subject.actor.id != subject.principal.id
        || subject.authority.provenance.mode() != InvocationMode::Direct
    {
        return Err(StatusCode::FORBIDDEN);
    }
    let tenant = subject.authority.tenant.as_str();
    let context = deterministic_work_context_id(tenant, subject.authority.work_context.as_str())
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let snapshot = state
        .store
        .workspace_context(context)
        .await
        .map_err(fault)?;
    let tenant_id = deterministic_tenant_id(tenant).map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if snapshot.context.tenant != tenant_id.record_id() {
        return Err(StatusCode::FORBIDDEN);
    }
    let membership = snapshot
        .context
        .memberships
        .iter()
        .filter(|rule| {
            rule.principals
                .iter()
                .any(|key| key == subject.principal.id.as_str())
                || rule.groups.iter().any(|key| {
                    subject
                        .principal
                        .groups
                        .iter()
                        .any(|group| group.as_str() == key)
                })
                || rule.roles.iter().any(|key| {
                    subject
                        .principal
                        .roles
                        .iter()
                        .any(|role| role.as_str() == key)
                })
                || rule
                    .oauth_clients
                    .iter()
                    .any(|key| key == subject.access_token.oauth_client_id.as_str())
        })
        .map(|rule| rule.level)
        .max_by_key(|level| match level {
            WorkContextMembershipLevel::Viewer => 0,
            WorkContextMembershipLevel::Contributor => 1,
            WorkContextMembershipLevel::Custodian => 2,
            WorkContextMembershipLevel::Owner => 3,
        })
        .ok_or(StatusCode::FORBIDDEN)?;
    let principal = deterministic_principal_id(tenant, subject.principal.id.as_str())
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(WorkspaceAuthority::new(
        tenant_id,
        context,
        principal,
        snapshot.digest,
        membership,
    ))
}
