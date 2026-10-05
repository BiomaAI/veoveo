use std::collections::BTreeMap;

use crate::contract::authoring as wire;
use crate::persistence::instances::*;
use surrealdb::types::{RecordId, SurrealValue, ToSql, Value};

use super::super::{AgentManagementState, Fault, authority::Admission, projection as common};

pub(super) fn desired(value: wire::InstanceDesired) -> ManagedAgentDesired {
    match value {
        wire::InstanceDesired::Running => ManagedAgentDesired::Running,
        wire::InstanceDesired::Paused => ManagedAgentDesired::Paused,
        wire::InstanceDesired::Archived => ManagedAgentDesired::Archived,
    }
}

pub(super) fn phase(value: ManagedAgentPhase) -> wire::InstancePhase {
    match value {
        ManagedAgentPhase::Queued => wire::InstancePhase::Queued,
        ManagedAgentPhase::Credentials => wire::InstancePhase::Credentials,
        ManagedAgentPhase::Storage => wire::InstancePhase::Storage,
        ManagedAgentPhase::Draining => wire::InstancePhase::Draining,
        ManagedAgentPhase::Workload => wire::InstancePhase::Workload,
        ManagedAgentPhase::Ready => wire::InstancePhase::Ready,
        ManagedAgentPhase::Paused => wire::InstancePhase::Paused,
        ManagedAgentPhase::Archived => wire::InstancePhase::Archived,
        ManagedAgentPhase::Failed => wire::InstancePhase::Failed,
        ManagedAgentPhase::Superseded => wire::InstancePhase::Superseded,
    }
}

pub(super) fn operation(
    key: &str,
    value: ManagedAgentOperation,
) -> Result<wire::LifecycleOperation, Fault> {
    Ok(wire::LifecycleOperation {
        id: common::uuid(&value.id)?,
        instance: wire::AgentManagedInstanceId::parse(key).map_err(|_| Fault::unavailable())?,
        generation: value.generation,
        phase: phase(value.phase),
        message: value.message,
        updated_at: value.updated_at,
    })
}

#[derive(SurrealValue)]
struct DefinitionIdentity {
    id: RecordId,
    key: String,
    tenant: RecordId,
}

pub(super) async fn instances(
    state: &AgentManagementState,
    actor: &Admission,
    values: Vec<ManagedAgentInstance>,
) -> Result<Vec<wire::ManagedInstance>, Fault> {
    if values.is_empty() {
        return Ok(Vec::new());
    }
    let definitions: Vec<_> = values.iter().map(|v| v.definition.clone()).collect();
    let revisions: Vec<_> = values
        .iter()
        .flat_map(|v| {
            std::iter::once(v.requested_revision.clone()).chain(v.active_revision.clone())
        })
        .collect();
    let mut response = state
        .store()
        .client()
        .query(include_str!(
            "../../../queries/gateway/http/instances/projection/instances/statement_1.surql"
        ))
        .bind(("definitions", definitions))
        .bind(("revisions", revisions))
        .await
        .map_err(|_| Fault::unavailable())?
        .check()
        .map_err(|_| Fault::unavailable())?;
    let definitions: Vec<DefinitionIdentity> =
        response.take(0).map_err(|_| Fault::unavailable())?;
    let revisions: Vec<Value> = response.take(1).map_err(|_| Fault::unavailable())?;
    let revisions = revisions
        .into_iter()
        .map(|row| crate::persistence::checked_revision(row).map_err(|_| Fault::unavailable()))
        .collect::<Result<Vec<_>, _>>()?;
    let definitions: BTreeMap<_, _> = definitions
        .into_iter()
        .map(|v| (v.id.to_sql(), v))
        .collect();
    let revisions: BTreeMap<_, _> = revisions.into_iter().map(|v| (v.id.to_sql(), v)).collect();
    values
        .into_iter()
        .map(|v| {
            let definition = definitions
                .get(&v.definition.to_sql())
                .ok_or_else(Fault::unavailable)?;
            let requested = revisions
                .get(&v.requested_revision.to_sql())
                .ok_or_else(Fault::unavailable)?;
            if definition.tenant != v.tenant || requested.definition != v.definition {
                return Err(Fault::unavailable());
            }
            let crate::persistence::AgentExecution::Managed { template, .. } = &requested.execution
            else {
                return Err(Fault::unavailable());
            };
            Ok(wire::ManagedInstance {
                id: wire::AgentManagedInstanceId::parse(v.key).map_err(|_| Fault::unavailable())?,
                name: v.name,
                definition: wire::AgentDefinitionId::parse(definition.key.clone())
                    .map_err(|_| Fault::unavailable())?,
                owner: common::uuid(&v.owner)?,
                work_context: common::context(
                    &actor.catalog,
                    &actor.subject.authority.tenant,
                    &v.work_context,
                )?,
                template: wire::AgentTemplateId::parse(template.clone())
                    .map_err(|_| Fault::unavailable())?,
                requested_revision: common::digest(&requested.digest)?,
                active_revision: v
                    .active_revision
                    .map(|id| {
                        revisions
                            .get(&id.to_sql())
                            .ok_or_else(Fault::unavailable)
                            .and_then(|r| {
                                if r.definition != v.definition {
                                    return Err(Fault::unavailable());
                                }
                                common::digest(&r.digest)
                            })
                    })
                    .transpose()?,
                generation: v.generation,
                active_generation: v.active_generation,
                desired: match v.desired {
                    ManagedAgentDesired::Running => wire::InstanceDesired::Running,
                    ManagedAgentDesired::Paused => wire::InstanceDesired::Paused,
                    ManagedAgentDesired::Archived => wire::InstanceDesired::Archived,
                },
                observed: phase(v.observed),
                client_id: v
                    .identity
                    .client_id
                    .parse()
                    .map_err(|_| Fault::unavailable())?,
                principal: common::uuid(&v.principal)?,
                storage_gib: v.resources.storage_gib,
                operation: common::uuid(&v.operation)?,
                updated_at: v.updated_at,
            })
        })
        .collect()
}
