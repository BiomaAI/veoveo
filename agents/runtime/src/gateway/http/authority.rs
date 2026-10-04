use crate::contract::AgentAction;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{http::StatusCode, response::IntoResponse};
use chrono::Utc;
use veoveo_mcp_contract::{GatewayProfile, GatewayProfileId, PolicyEffect, PolicyTarget, TraceId};
use veoveo_mcp_gateway::{AuthenticatedSubject, GatewayCatalog, PolicyRequest};
use veoveo_platform_store::{
    WorkContextMembershipLevel, agent_management::AgentCatalogAuthority,
    deterministic_principal_id, deterministic_tenant_id, deterministic_work_context_id,
};
use veoveo_types::WorkContextId;

use super::{AgentManagementState, Fault};
use veoveo_mcp_gateway::http::action_audit::{AdminAuthorizationRequest, authorize_gateway_action};

pub(super) struct Admission {
    pub catalog: Arc<GatewayCatalog>,
    pub profile: GatewayProfile,
    pub subject: AuthenticatedSubject,
    pub authority: AgentCatalogAuthority,
    pub started: Instant,
    pub action: AgentAction,
}

pub(super) fn policy_action(
    catalog: &GatewayCatalog,
    action: AgentAction,
) -> Result<veoveo_gateway_contract::PolicyAction, veoveo_types::ExtensionError> {
    Ok(catalog
        .registry()
        .action_key::<AgentAction>()?
        .action(action)?
        .into())
}

pub(super) fn allowed(
    catalog: &GatewayCatalog,
    profile: &GatewayProfileId,
    subject: &AuthenticatedSubject,
    action: AgentAction,
) -> bool {
    let Ok(action) = policy_action(catalog, action) else {
        return false;
    };
    catalog
        .decide(PolicyRequest {
            principal: &subject.principal,
            profile,
            action,
            target: &PolicyTarget::Gateway,
            trace_id: &TraceId::new("agent-management-authority").expect("fixed trace identifier"),
        })
        .effect
        == PolicyEffect::Allow
}

pub(super) async fn admit(
    state: &AgentManagementState,
    profile: String,
    subject: AuthenticatedSubject,
    action: AgentAction,
) -> Result<Admission, Fault> {
    let profile =
        GatewayProfileId::new(profile).map_err(|_| Fault::status(StatusCode::NOT_FOUND))?;
    let started = Instant::now();
    let audit_target = veoveo_mcp_contract::audit::AuditTarget::WorkContext {
        tenant: subject.authority.tenant.clone(),
        context: subject.authority.work_context.clone(),
    };
    let current = state.catalog.current();
    let policy_action = policy_action(&current, action).map_err(|_| Fault::unavailable())?;
    let (catalog, profile, subject) = authorize_gateway_action(
        &state.gateway,
        current,
        &profile,
        subject,
        AdminAuthorizationRequest {
            audit_target: Some(audit_target),
            action: policy_action,
            target: PolicyTarget::Gateway,
            operation: crate::gateway::http::audit::agent_management_operation(action)
                .map_err(|_| Fault::unavailable())?,
            started_at: started,
        },
    )
    .await
    .map_err(Fault::Response)?;
    live_session(state, &profile, &subject).await?;
    let authority = context(state, &subject, &subject.authority.work_context).await?;
    Ok(Admission {
        catalog,
        profile,
        subject,
        authority,
        started,
        action,
    })
}

pub(super) async fn live_session(
    state: &AgentManagementState,
    profile: &GatewayProfile,
    subject: &AuthenticatedSubject,
) -> Result<(), Fault> {
    tokio::time::timeout(Duration::from_secs(5), async {
        if subject.access_token.expires_at <= Utc::now()
            || !state
                .gateway
                .access_token_session_valid(
                    &profile.id,
                    &profile.authorization_server,
                    &subject.access_token,
                    &subject.principal,
                )
                .await
                .map_err(|_| Fault::unavailable())?
        {
            return Err(Fault::status(StatusCode::UNAUTHORIZED));
        }
        if let Some(jwt) = &subject.access_token.jwt_id
            && state
                .gateway
                .jwt_revocation(&profile.id, &subject.access_token.issuer, jwt, Utc::now())
                .await
                .map_err(|_| Fault::unavailable())?
                .is_some()
        {
            return Err(Fault::status(StatusCode::UNAUTHORIZED));
        }
        Ok(())
    })
    .await
    .map_err(|_| Fault::unavailable())?
}

pub(crate) async fn context(
    state: &AgentManagementState,
    subject: &AuthenticatedSubject,
    context: &WorkContextId,
) -> Result<AgentCatalogAuthority, Fault> {
    let tenant = subject.authority.tenant.as_str();
    let context = deterministic_work_context_id(tenant, context.as_str())
        .map_err(|_| Fault::unavailable())?;
    let snapshot = tokio::time::timeout(
        Duration::from_secs(5),
        state.store().workspace_context(context),
    )
    .await
    .map_err(|_| Fault::unavailable())?
    .map_err(|_| Fault::unavailable())?;
    let tenant_id = deterministic_tenant_id(tenant).map_err(|_| Fault::unavailable())?;
    if snapshot.context.tenant != tenant_id.record_id() {
        return Err(Fault::status(StatusCode::FORBIDDEN));
    }
    let membership = snapshot
        .context
        .memberships
        .iter()
        .filter(|rule| {
            rule.principals
                .iter()
                .any(|id| id == subject.principal.id.as_str())
                || rule
                    .groups
                    .iter()
                    .any(|id| subject.principal.groups.iter().any(|g| g.as_str() == id))
                || rule
                    .roles
                    .iter()
                    .any(|id| subject.principal.roles.iter().any(|r| r.as_str() == id))
                || rule
                    .oauth_clients
                    .iter()
                    .any(|id| subject.access_token.oauth_client_id.as_str() == id)
        })
        .map(|rule| rule.level)
        .max_by_key(|level| match level {
            WorkContextMembershipLevel::Viewer => 0,
            WorkContextMembershipLevel::Contributor => 1,
            WorkContextMembershipLevel::Custodian => 2,
            WorkContextMembershipLevel::Owner => 3,
        })
        .ok_or_else(|| Fault::status(StatusCode::FORBIDDEN))?;
    let manage = matches!(
        membership,
        WorkContextMembershipLevel::Custodian | WorkContextMembershipLevel::Owner
    );
    Ok(AgentCatalogAuthority::new(
        tenant_id,
        context,
        deterministic_principal_id(tenant, subject.principal.id.as_str())
            .map_err(|_| Fault::unavailable())?,
        snapshot.digest,
        membership,
        manage,
        state.definition_limit,
    ))
}

impl IntoResponse for Fault {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::Response(response) => *response,
            Self::Status(status) => status.into_response(),
            Self::Validation(validation) => {
                (StatusCode::UNPROCESSABLE_ENTITY, axum::Json(validation)).into_response()
            }
        }
    }
}
