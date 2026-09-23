//! One-time Bioma definition consolidation. No gateway or manager route exposes it.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue, Value};
use veoveo_platform_store::{
    PlatformStore,
    agent_management::{
        AgentDefinition, AgentExecution, AgentRevision, agent_definition_record, instances::*,
    },
    deterministic_tenant_id, deterministic_work_context_id,
};

#[derive(Clone, Serialize, Deserialize, SurrealValue)]
#[serde(deny_unknown_fields)]
pub struct PilotRebinding {
    pub before: ManagedAgentInstance,
    pub after: ManagedAgentInstance,
    pub operation: ManagedAgentOperation,
}

/// Freeze the four paused instances. SQL independently checks drain and publication.
pub async fn prepare(store: &PlatformStore, config_map: &str) -> Result<Vec<PilotRebinding>> {
    let tenant = deterministic_tenant_id("bioma")?.record_id();
    let definition: AgentDefinition = store
        .client()
        .select(agent_definition_record(&tenant, "uav-pilot")?)
        .await?
        .context("publish the shared uav-pilot definition first")?;
    let revision: AgentRevision = store
        .client()
        .select(
            definition
                .published
                .context("shared definition is not published")?,
        )
        .await?
        .context("shared revision missing")?;
    let AgentExecution::Managed {
        parameters,
        resource_subscriptions,
        ..
    } = &revision.content.execution
    else {
        anyhow::bail!("shared revision must use managed execution");
    };
    ensure!(
        parameters.len() == 1
            && parameters.contains_key("session")
            && resource_subscriptions.len() == 2,
        "shared revision still contains vehicle-specific configuration"
    );
    let now: Vec<surrealdb::types::Datetime> = store
        .client()
        .query("RETURN [time::now()];")
        .await?
        .check()?
        .take(0)?;
    let now = now.into_iter().next().context("database clock")?.into();
    let mut result = Vec::new();
    for n in 1..=4 {
        let before: ManagedAgentInstance = store
            .client()
            .select(managed_agent_record(&tenant, &format!("uav-{n}-pilot"))?)
            .await?
            .context("retained pilot missing")?;
        let mut after = before.clone();
        after.definition = definition.id.clone();
        after.requested_revision = revision.id.clone();
        // Keep active_generation > 0: the manager must reject missing retained memory.
        after.active_revision = None;
        after.generation += 1;
        after.dispatch_epoch += 1;
        after.resources.template_config_map = config_map.to_owned();
        after.updated_at = now;
        let request = uuid::Uuid::now_v7();
        after.operation = RecordId::new(
            "managed_agent_operation",
            surrealdb::types::Uuid::from(request),
        );
        let operation = ManagedAgentOperation {
            id: after.operation.clone(),
            instance: after.id.clone(),
            tenant: after.tenant.clone(),
            work_context: after.work_context.clone(),
            actor: after.owner.clone(),
            request_id: request,
            fingerprint: format!("bioma-pilot-consolidation/v1:{}", after.key),
            generation: after.generation,
            phase: ManagedAgentPhase::Paused,
            message: Some(
                "Shared pilot definition selected after drain; resume through the management API."
                    .into(),
            ),
            lease_owner: None,
            lease_fence: 0,
            lease_expires_at: None,
            created_at: now,
            updated_at: now,
        };
        result.push(PilotRebinding {
            before,
            after,
            operation,
        });
    }
    validate(&result)?;
    Ok(result)
}

pub fn validate(entries: &[PilotRebinding]) -> Result<()> {
    ensure!(
        entries.len() == 4,
        "consolidation requires exactly four retained pilots"
    );
    let tenant = deterministic_tenant_id("bioma")?.record_id();
    let context = deterministic_work_context_id("bioma", "operations")?.record_id();
    let definition = agent_definition_record(&tenant, "uav-pilot")?;
    for (index, entry) in entries.iter().enumerate() {
        let before = &entry.before;
        let after = &entry.after;
        let key = format!("uav-{}-pilot", index + 1);
        ensure!(
            before.id == managed_agent_record(&tenant, &key)?
                && before.key == key
                && before.tenant == tenant
                && before.work_context == context
                && before.definition == agent_definition_record(&tenant, &key)?
                && before.desired == ManagedAgentDesired::Paused
                && before.observed == ManagedAgentPhase::Paused
                && before.active_generation > 0
                && before.public_key.is_some(),
            "pilot source identity or pause state differs"
        );
        let mut expected = before.clone();
        expected.definition = definition.clone();
        expected.requested_revision = after.requested_revision.clone();
        expected.active_revision = None;
        expected.generation += 1;
        expected.dispatch_epoch += 1;
        expected.resources.template_config_map = after.resources.template_config_map.clone();
        expected.operation = entry.operation.id.clone();
        expected.updated_at = after.updated_at;
        ensure!(
            after == &expected
                && after.requested_revision == entries[0].after.requested_revision
                && after.resources.template_config_map
                    == entries[0].after.resources.template_config_map
                && after
                    .resources
                    .template_config_map
                    .starts_with("uav-pilot-"),
            "consolidation attempted to change retained identity, memory, or grants"
        );
        let op = &entry.operation;
        ensure!(
            op.instance == after.id
                && op.tenant == tenant
                && op.work_context == context
                && op.actor == after.owner
                && op.generation == after.generation
                && op.phase == ManagedAgentPhase::Paused
                && op.lease_owner.is_none()
                && op.lease_expires_at.is_none()
                && op.id != before.operation,
            "invalid consolidation operation"
        );
    }
    Ok(())
}

pub async fn apply(store: &PlatformStore, entries: &[PilotRebinding]) -> Result<Vec<RecordId>> {
    validate(entries)?;
    let mut result = store
        .client()
        .query(include_str!("pilot_consolidation.surql"))
        .bind(("entries", stored_fields(entries.to_vec().into_value())))
        .await?
        .check()?;
    let last = result
        .num_statements()
        .checked_sub(2)
        .context("missing consolidation result")?;
    Ok(result.take(last)?)
}

fn stored_fields(value: Value) -> Value {
    match value {
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .filter_map(|(k, v)| (!matches!(v, Value::None)).then(|| (k, stored_fields(v))))
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.into_iter().map(stored_fields).collect()),
        value => value,
    }
}
