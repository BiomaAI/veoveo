use crate::{
    Computer, ComputerError, Result, api::ComputerPhase, authority_snapshot::AuthoritySnapshot,
};
use surrealdb::types::{SurrealValue, Value};
use uuid::Uuid;
use veoveo_mcp_contract::{
    GatewayAction, PolicyEffect, PolicyTarget, ResourceUri, ServerSlug, TraceId,
    WorkContextMembershipLevel,
};
use veoveo_platform_store::{OutboxDraft, deterministic_enterprise_id, deterministic_tenant_id};

pub(crate) fn require_attach(snapshot: &AuthoritySnapshot, computer: Uuid) -> Result<()> {
    snapshot.check_fresh()?;
    if !snapshot
        .membership
        .allows(WorkContextMembershipLevel::Contributor)
    {
        return Err(ComputerError::Forbidden);
    }
    let server = ServerSlug::new("computers").expect("static server");
    let trace = TraceId::new(Uuid::now_v7().to_string()).expect("UUID trace");
    let target = PolicyTarget::Resource {
        server,
        uri: ResourceUri::new(crate::api::computer_uri(computer)).expect("Computer URI"),
    };
    for action in [GatewayAction::ComputerAttach, GatewayAction::ResourcesRead] {
        if snapshot.decision(action, &target, &trace).effect != PolicyEffect::Allow {
            return Err(ComputerError::Forbidden);
        }
    }
    Ok(())
}
pub(crate) fn ready(computer: &Computer, provider: Uuid) -> Result<()> {
    if computer.provider_instance_id != provider
        || computer.phase != ComputerPhase::Ready
        || computer.active_operation.is_some()
        || computer
            .provider_resource_id
            .as_ref()
            .is_none_or(String::is_empty)
        || computer.process_id.as_ref().is_none_or(String::is_empty)
    {
        return Err(ComputerError::InvalidState);
    }
    Ok(())
}
pub(crate) fn bindings(snapshot: &AuthoritySnapshot) -> Vec<(&'static str, Value)> {
    vec![
        (
            "authority_revision",
            snapshot.revision_record.clone().into_value(),
        ),
        (
            "authority_sha256",
            snapshot.control_sha256.clone().into_value(),
        ),
        (
            "authority_revision_id",
            snapshot.control_revision.clone().into_value(),
        ),
        (
            "authority_enterprise",
            deterministic_enterprise_id().record_id().into_value(),
        ),
        ("authority_tenant", snapshot.tenant.clone().into_value()),
        ("authority_source", snapshot.source.clone().into_value()),
        ("authority_actor", snapshot.actor.clone().into_value()),
    ]
}
pub(crate) fn event(
    accepted: &crate::AcceptedAuthority,
    computer: Uuid,
    grant: Uuid,
    kind: &str,
) -> Result<OutboxDraft> {
    #[derive(serde::Serialize)]
    struct Payload<'a> {
        computer_id: Uuid,
        grant_id: Uuid,
        actor: &'a veoveo_mcp_contract::PrincipalId,
        authority: &'a veoveo_mcp_contract::InvocationAuthority,
    }
    Ok(OutboxDraft::now(
        Some(
            deterministic_tenant_id(accepted.invocation.tenant.as_str())
                .map_err(|_| ComputerError::InvalidInput)?
                .record_id(),
        ),
        "computer",
        computer.to_string(),
        kind,
        1,
        super::object(&Payload {
            computer_id: computer,
            grant_id: grant,
            actor: &accepted.actor.id,
            authority: &accepted.invocation,
        })?,
    ))
}
