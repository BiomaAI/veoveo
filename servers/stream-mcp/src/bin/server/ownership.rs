use std::collections::BTreeSet;

use veoveo_mcp_contract::{GatewayInternalIdentity, PrincipalKind, TokenIssuer, TokenSubject};
use veoveo_recording_reader::RecordingReadAuthority;
use veoveo_task_runtime::TaskOwner;
use veoveo_types::{DataLabelId, PrincipalId, TenantId};

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
