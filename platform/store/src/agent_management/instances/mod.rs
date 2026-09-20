//! Admitted managed instances and durable reconciliation intent.
mod controller;
mod records;
mod validation;
pub use records::*;

use surrealdb::types::{RecordId, SurrealValue, ToSql};
use uuid::Uuid;

use super::{AgentCatalogAuthority, AgentManagementError, Result, agent_definition_record};
use crate::{PlatformStore, deterministic_principal_id};

pub fn managed_agent_record(tenant: &RecordId, key: &str) -> Result<RecordId> {
    super::validation::key(key)?;
    Ok(RecordId::new(
        "managed_agent",
        surrealdb::types::Uuid::from(Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!("managed-agent:{}:{key}", tenant.to_sql()).as_bytes(),
        )),
    ))
}

fn operation_record(authority: &AgentCatalogAuthority, request_id: Uuid) -> RecordId {
    RecordId::new(
        "managed_agent_operation",
        surrealdb::types::Uuid::from(Uuid::new_v5(
            &request_id,
            format!(
                "{}:{}",
                authority.tenant.to_sql(),
                authority.principal.to_sql()
            )
            .as_bytes(),
        )),
    )
}

impl PlatformStore {
    /// Recover an admitted mutation before revalidating an execution configuration
    /// that may have changed since the caller lost its first response.
    pub async fn replay_managed_agent(
        &self,
        authority: &AgentCatalogAuthority,
        key: &str,
        request_id: Uuid,
        expected_generation: Option<i64>,
        mutation: &ManagedAgentMutation,
    ) -> Result<Option<ManagedAgentOperation>> {
        if request_id.get_version_num() != 7 {
            return Err(AgentManagementError::Invalid("request"));
        }
        #[derive(Clone, SurrealValue)]
        struct Replay {
            instance: RecordId,
            operation: RecordId,
            fingerprint: String,
        }
        self.agent_query(authority, Replay {
            instance: managed_agent_record(&authority.tenant, key)?,
            operation: operation_record(authority, request_id),
            fingerprint: super::validation::hash(&(key, expected_generation, mutation))?,
        }, "IF $authority.membership = 'viewer' { THROW 'agent_forbidden'; }; LET $instance = SELECT * FROM ONLY $command.instance; IF $instance != NONE AND !fn::managed_agent_editor($authority, $instance) { THROW 'agent_not_found'; }; LET $receipt = SELECT * FROM ONLY $command.operation; IF $receipt = NONE { RETURN NONE; }; IF $receipt.fingerprint != $command.fingerprint OR $receipt.work_context != $authority.work_context { THROW 'agent_conflict'; }; RETURN $receipt;").await
    }

    pub async fn agent_management_head(&self, authority: &AgentCatalogAuthority) -> Result<i64> {
        self.agent_query(authority, false,
            "RETURN array::first(SELECT VALUE sequence FROM outbox_event WHERE tenant = $authority.tenant AND ((aggregate_type = 'agent_definition' AND $authority.work_context IN payload.affected_contexts) OR (aggregate_type = 'managed_agent' AND payload.work_context = $authority.work_context AND (payload.operation.instance.owner = $authority.principal OR $authority.manage_context))) ORDER BY sequence DESC LIMIT 1) ?? 0;"
        ).await
    }

    pub async fn managed_agent_registration(
        &self,
        client_id: &str,
    ) -> Result<Option<ManagedAgentRegistration>> {
        self.managed_query(client_id.to_owned(),
            "LET $instance = array::first(SELECT * FROM managed_agent WHERE identity.client_id = $command LIMIT 1); IF $instance = NONE { RETURN NONE; }; LET $revision = SELECT * FROM ONLY ($instance.active_revision ?? $instance.requested_revision); RETURN { instance: $instance, revision: $revision, tenant_key: $instance.tenant.slug, context_key: $instance.work_context.context_key, enabled: fn::managed_agent_enabled($instance.id) AND $instance.active_generation > 0 AND $instance.public_key != NONE };"
        ).await
    }

    pub async fn mutate_managed_agent(
        &self,
        authority: &AgentCatalogAuthority,
        key: &str,
        request_id: Uuid,
        expected_generation: Option<i64>,
        mutation: ManagedAgentMutation,
        limits: ManagedAgentLimits,
    ) -> Result<ManagedAgentOperation> {
        validation::mutation(&mutation, limits)?;
        if request_id.get_version_num() != 7
            || expected_generation.is_some_and(|g| g < 1)
            || matches!(mutation, ManagedAgentMutation::Provision { .. })
                != expected_generation.is_none()
        {
            return Err(AgentManagementError::Invalid("request"));
        }
        let (definition, principal) = if let ManagedAgentMutation::Provision { plan } = &mutation {
            // The actual slug is a server-owned field of the current tenant.
            let slug: String = self
                .agent_query(authority, false, "RETURN $authority.tenant.slug;")
                .await?;
            let principal = deterministic_principal_id(
                &slug,
                &format!("{}#{}", plan.identity.issuer, plan.identity.client_id),
            )
            .map_err(|_| AgentManagementError::Invalid("identity"))?
            .record_id();
            (
                Some(agent_definition_record(
                    &authority.tenant,
                    &plan.definition_key,
                )?),
                Some(principal),
            )
        } else {
            (None, None)
        };
        let command = InstanceCommand {
            instance: managed_agent_record(&authority.tenant, key)?,
            operation: operation_record(authority, request_id),
            fingerprint: super::validation::hash(&(key, expected_generation, &mutation))?,
            request_id,
            key: key.to_owned(),
            expected_generation,
            mutation,
            definition,
            principal,
            limits,
        };
        self.agent_query(authority, command, include_str!("mutate.surql"))
            .await
    }

    pub async fn managed_agent(
        &self,
        authority: &AgentCatalogAuthority,
        key: &str,
    ) -> Result<ManagedAgentInstance> {
        self.agent_query(authority, managed_agent_record(&authority.tenant, key)?,
            "LET $instance = SELECT * FROM ONLY $command; IF !fn::managed_agent_editor($authority, $instance) { THROW 'agent_not_found'; }; RETURN $instance;"
        ).await
    }

    pub async fn managed_agents(
        &self,
        authority: &AgentCatalogAuthority,
        after: Option<&str>,
        limit: u32,
    ) -> Result<Vec<ManagedAgentInstance>> {
        self.agent_query(authority, super::page(after, limit)?,
            "RETURN SELECT * FROM managed_agent WHERE tenant = $authority.tenant AND work_context = $authority.work_context AND key > $command.after AND (owner = $authority.principal OR $authority.manage_context) ORDER BY key LIMIT $command.limit;"
        ).await
    }

    pub async fn managed_agent_operation(
        &self,
        authority: &AgentCatalogAuthority,
        id: RecordId,
    ) -> Result<ManagedAgentOperation> {
        if id.table.as_str() != "managed_agent_operation" {
            return Err(AgentManagementError::Invalid("operation"));
        }
        self.agent_query(authority, id,
            "LET $operation = SELECT * FROM ONLY $command; IF $operation = NONE OR !fn::managed_agent_editor($authority, $operation.instance.*) { THROW 'agent_not_found'; }; RETURN $operation;"
        ).await
    }

    /// Internal effective-identity lookup. Authentication still verifies assertion,
    /// resource, scopes and the installation's current template ceiling.
    pub async fn managed_agent_client(
        &self,
        client_id: &str,
    ) -> Result<Option<ManagedAgentInstance>> {
        self.managed_query(client_id.to_owned(),
            "RETURN array::first(SELECT * FROM managed_agent WHERE identity.client_id = $command AND public_key != NONE AND active_generation > 0 AND fn::managed_agent_enabled(id) LIMIT 1);"
        ).await
    }

    /// Runtime fence for a previously admitted episode. Pause drains it; archive,
    /// disable, identity revocation and stop prevent further dispatch immediately.
    pub async fn managed_agent_dispatch(
        &self,
        instance: RecordId,
        generation: i64,
        epoch: i64,
    ) -> Result<bool> {
        #[derive(Clone, SurrealValue)]
        struct Dispatch {
            instance: RecordId,
            generation: i64,
            epoch: i64,
        }
        self.managed_query(Dispatch { instance, generation, epoch },
            "LET $instance = SELECT * FROM ONLY $command.instance; RETURN fn::managed_agent_enabled($command.instance) AND $instance.active_generation > 0 AND $instance.active_revision != NONE AND $instance.active_generation = $command.generation AND $instance.dispatch_epoch = $command.epoch;"
        ).await
    }

    /// New episodes wait for convergence. An update or pause does not invalidate
    /// the already-admitted episode's separate dispatch fence.
    pub async fn managed_agent_episode_admission(
        &self,
        instance: RecordId,
        generation: i64,
    ) -> Result<Option<i64>> {
        #[derive(Clone, SurrealValue)]
        struct Admission {
            instance: RecordId,
            generation: i64,
        }
        self.managed_query(Admission { instance, generation },
            "LET $instance = SELECT * FROM ONLY $command.instance; IF !fn::managed_agent_enabled($command.instance) OR $instance.desired != 'running' OR $instance.active_generation < 1 OR $instance.active_generation != $command.generation OR $instance.generation != $command.generation OR $instance.observed NOT IN ['workload', 'ready'] { RETURN NONE; }; RETURN $instance.dispatch_epoch;"
        ).await
    }
}
