use crate::{
    Computer, ComputerError, Result, api::ComputerPhase, authority_snapshot::AuthoritySnapshot,
};
use surrealdb::types::{SurrealValue, Value};
use uuid::Uuid;
use veoveo_gateway_contract::GatewayAction;
use veoveo_mcp_contract::{PolicyEffect, PolicyTarget, ServerSlug, TraceId};
use veoveo_platform_store::deterministic_enterprise_id;
use veoveo_types::ResourceUri;
use veoveo_types::WorkContextMembershipLevel;

pub(crate) fn require_attach(
    snapshot: &AuthoritySnapshot,
    computer: veoveo_computers_contract::ComputerId,
) -> Result<()> {
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
    let attach = snapshot
        .catalog
        .registry()
        .action_key::<veoveo_computers_contract::ComputerAction>()
        .and_then(|key| key.action(veoveo_computers_contract::ComputerAction::Attach))
        .map_err(|_| ComputerError::Unavailable)?;
    for action in [
        veoveo_gateway_contract::PolicyAction::from(attach),
        veoveo_gateway_contract::PolicyAction::from(GatewayAction::ResourcesRead),
    ] {
        if snapshot.decision(action, &target, &trace).effect != PolicyEffect::Allow {
            return Err(ComputerError::Forbidden);
        }
    }
    Ok(())
}
pub(crate) fn ready(computer: &Computer, provider: crate::api::ProviderInstanceId) -> Result<()> {
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
