use veoveo_mcp_contract::GatewayInternalIdentity;
use veoveo_task_runtime::TaskOwner;

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
