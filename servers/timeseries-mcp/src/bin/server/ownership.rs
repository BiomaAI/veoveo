use veoveo_mcp_contract::GatewayInternalIdentity;
use veoveo_timeseries_mcp::state::TaskOwner;
use veoveo_types::TaskId;

pub(super) fn task_owner_from_identity(
    task_id: TaskId,
    identity: &GatewayInternalIdentity,
) -> TaskOwner {
    TaskOwner {
        task_id,
        principal_id: identity.actor.id.clone(),
        profile: identity.profile.clone(),
        tenant: identity.actor.tenant.clone(),
        data_labels: identity.actor.data_labels.clone(),
    }
}

pub(super) fn task_owner_from_runtime(
    task_id: TaskId,
    owner: &veoveo_task_runtime::TaskOwner,
) -> Result<TaskOwner, String> {
    Ok(TaskOwner {
        task_id,
        principal_id: veoveo_types::PrincipalId::parse(owner.principal_key.clone())
            .map_err(|error| error.to_string())?,
        profile: veoveo_mcp_contract::GatewayProfileId::parse(owner.profile.clone())
            .map_err(|error| error.to_string())?,
        tenant: owner
            .tenant_key
            .clone()
            .map(veoveo_types::TenantId::parse)
            .transpose()
            .map_err(|error| error.to_string())?,
        data_labels: owner
            .data_labels
            .iter()
            .cloned()
            .map(veoveo_types::DataLabelId::parse)
            .collect::<Result<_, _>>()
            .map_err(|error| error.to_string())?,
    })
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
