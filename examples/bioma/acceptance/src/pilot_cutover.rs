//! Explicit installation migration. Never mounted in a gateway or controller.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, RecordIdKey, SurrealValue, Value};
use veoveo_platform_store::{
    AgentRecord, PlatformStore, PrincipalKind, PrincipalRecord,
    agent_management::instances::{
        ManagedAgentDesired, ManagedAgentInstance, ManagedAgentOperation, ManagedAgentPhase,
        managed_agent_record,
    },
    deterministic_principal_id, deterministic_tenant_id, deterministic_work_context_id,
};

#[derive(Clone, Serialize, Deserialize, SurrealValue)]
#[serde(deny_unknown_fields)]
pub struct PilotAdoption {
    pub runtime: AgentRecord,
    pub principal: PrincipalRecord,
    pub instance: ManagedAgentInstance,
    pub operation: ManagedAgentOperation,
}

/// A frozen before-image is required. Old writers must already be stopped and
/// each retained PV and signing key transferred to the recorded destination.
pub fn validate(entries: &[PilotAdoption]) -> Result<()> {
    ensure!(
        entries.len() == 4,
        "the Bioma cutover requires exactly four pilots"
    );
    let tenant = deterministic_tenant_id("bioma")?.record_id();
    let context = deterministic_work_context_id("bioma", "operations")?.record_id();
    for (index, entry) in entries.iter().enumerate() {
        let key = format!("uav-{}-pilot", index + 1);
        let instance = &entry.instance;
        let RecordIdKey::Uuid(id) = &instance.id.key else {
            anyhow::bail!("managed instance must have a UUID record identity");
        };
        let workload = format!("agent-{}", uuid::Uuid::from(*id).simple());
        ensure!(
            instance.id == managed_agent_record(&tenant, &key)?
                && instance.tenant == tenant
                && instance.work_context == context
                && instance.key == key
                && instance.identity.client_id == key
                && instance.identity.issuer == "https://veoveo.bioma.ai/oauth"
                && instance.principal
                    == deterministic_principal_id(
                        "bioma",
                        &format!("https://veoveo.bioma.ai/oauth#{key}")
                    )?
                    .record_id()
                && instance.principal == entry.principal.id
                && entry.principal.tenant == tenant
                && entry.principal.enabled
                && entry.principal.kind == PrincipalKind::Service
                && entry.principal.issuer == instance.identity.issuer
                && entry.principal.subject == key
                && entry.runtime.tenant == tenant
                && entry.runtime.work_context == context
                && entry.runtime.agent_key == key,
            "pilot identity differs from its frozen source"
        );
        ensure!(
            instance.desired == ManagedAgentDesired::Paused
                && instance.observed == ManagedAgentPhase::Paused
                && instance.generation == 1
                && instance.active_generation == 1
                && instance.dispatch_epoch == 1
                && instance.active_revision.as_ref() == Some(&instance.requested_revision)
                && instance.public_key.is_some()
                && instance.resources.namespace == "veoveo-agents"
                && instance.resources.workload == workload
                && instance.resources.volume_claim == format!("{workload}-memory")
                && instance.resources.credential_secret == format!("{workload}-key")
                && instance.resources.storage_gib == 2,
            "adoption must retain identity and storage in a paused initial generation"
        );
        ensure!(
            entry.operation.id == instance.operation
                && entry.operation.instance == instance.id
                && entry.operation.tenant == tenant
                && entry.operation.work_context == context
                && entry.operation.actor == instance.owner
                && instance.deployed_by == instance.owner
                && entry.operation.generation == 1
                && entry.operation.phase == ManagedAgentPhase::Paused
                && entry.operation.lease_owner.is_none()
                && entry.operation.lease_expires_at.is_none(),
            "adoption operation differs from its instance"
        );
    }
    Ok(())
}

/// Insert only lifecycle ownership. Existing runtime, principals, grants, wakes,
/// Task ownership and memory are never rewritten by this transaction.
pub async fn adopt(store: &PlatformStore, entries: &[PilotAdoption]) -> Result<Vec<RecordId>> {
    validate(entries)?;
    let mut result = store
        .client()
        .query(include_str!("pilot_cutover.surql"))
        .bind(("entries", stored_fields(entries.to_vec().into_value())))
        .await?
        .check()?;
    let last = result
        .num_statements()
        .checked_sub(2)
        .ok_or_else(|| anyhow::anyhow!("missing adoption result"))?;
    Ok(result.take(last)?)
}

// SurrealDB omits NONE fields recursively. SDK-derived optional fields retain
// those keys in objects, so normalize their representation before exact comparison.
fn stored_fields(value: Value) -> Value {
    match value {
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .filter_map(|(key, value)| {
                    (!matches!(value, Value::None)).then(|| (key, stored_fields(value)))
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.into_iter().map(stored_fields).collect()),
        value => value,
    }
}
