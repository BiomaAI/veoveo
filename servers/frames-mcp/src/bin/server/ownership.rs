use rmcp::ErrorData as McpError;
use veoveo_frames_mcp::state::FrameScope;
use veoveo_mcp_contract::{GatewayInternalIdentity, PrincipalKind};
use veoveo_platform_store::PrincipalKind as StorePrincipalKind;
use veoveo_task_runtime::TaskOwner;

use super::app_state::AppState;

pub(super) fn runtime_owner(identity: &GatewayInternalIdentity) -> TaskOwner {
    TaskOwner {
        principal_key: identity.actor.id.to_string(),
        principal_kind: match identity.actor.kind {
            PrincipalKind::User => veoveo_task_runtime::PrincipalKind::User,
            PrincipalKind::Service => veoveo_task_runtime::PrincipalKind::Service,
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

pub(super) async fn frame_scope_from_identity(
    state: &AppState,
    identity: &GatewayInternalIdentity,
) -> Result<FrameScope, McpError> {
    let actor = &identity.actor;
    let resolved = state
        .tasks
        .platform_store()
        .ensure_identity(
            actor
                .tenant
                .as_ref()
                .map_or("installation", |tenant| tenant.as_str()),
            actor.id.as_str(),
            actor.issuer.as_str(),
            actor.subject.as_str(),
            match actor.kind {
                PrincipalKind::User => StorePrincipalKind::User,
                PrincipalKind::Service => StorePrincipalKind::Service,
            },
        )
        .await
        .map_err(|error| McpError::internal_error(error.to_string(), None))?;
    Ok(FrameScope::new(resolved, actor.data_labels.clone()))
}

pub(super) async fn frame_scope_from_runtime(
    state: &AppState,
    owner: &TaskOwner,
) -> Result<FrameScope, McpError> {
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
    Ok(FrameScope::new(
        identity,
        owner
            .data_labels
            .iter()
            .cloned()
            .map(veoveo_types::DataLabelId::new)
            .collect::<Result<_, _>>()
            .map_err(|error| McpError::internal_error(error.to_string(), None))?,
    ))
}

pub(super) fn operation_scope_from_identity(
    identity: &GatewayInternalIdentity,
) -> veoveo_frames_mcp::state::FrameOperationScope {
    veoveo_frames_mcp::state::FrameOperationScope::new(
        identity.actor.id.clone(),
        identity.actor.tenant.clone(),
        identity.profile.clone(),
        identity.actor.data_labels.clone(),
    )
}

pub(super) fn operation_scope_from_runtime(
    owner: &TaskOwner,
) -> anyhow::Result<veoveo_frames_mcp::state::FrameOperationScope> {
    Ok(veoveo_frames_mcp::state::FrameOperationScope::new(
        veoveo_types::PrincipalId::new(owner.principal_key.clone())?,
        owner
            .tenant_key
            .clone()
            .map(veoveo_types::TenantId::new)
            .transpose()?,
        veoveo_mcp_contract::GatewayProfileId::new(owner.profile.clone())?,
        owner
            .data_labels
            .iter()
            .cloned()
            .map(veoveo_types::DataLabelId::new)
            .collect::<Result<_, _>>()?,
    ))
}
