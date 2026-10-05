//! Admitted managed instances and durable reconciliation intent.
mod controller;
mod records;
mod validation;
pub use records::*;

use surrealdb::types::{RecordId, SurrealValue, ToSql};
use uuid::Uuid;

use super::{AgentCatalogAuthority, AgentManagementError, Result, agent_definition_record};
use crate::persistence::AgentRepository;
use veoveo_platform_store::deterministic_principal_id;

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

impl AgentRepository {
    /// Dispatch belongs to the admitted generation and its current scheduler
    /// lease. A replacement pod in the same generation cannot share dispatch.
    pub async fn managed_agent_kernel_dispatch(
        &self,
        instance: RecordId,
        generation: i64,
        epoch: i64,
        owner: Uuid,
        fence: i64,
    ) -> Result<bool> {
        #[derive(Clone, SurrealValue)]
        struct Dispatch {
            instance: RecordId,
            generation: i64,
            epoch: i64,
            owner: String,
            fence: i64,
        }
        self.managed_query(
            Dispatch {
                instance,
                generation,
                epoch,
                owner: owner.to_string(),
                fence,
            },
            include_str!("../queries/instances/mod/managed_agent_kernel_dispatch.surql"),
        )
        .await
    }

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
        self.agent_query(
            authority,
            Replay {
                instance: managed_agent_record(&authority.tenant, key)?,
                operation: operation_record(authority, request_id),
                fingerprint: super::validation::hash(&(key, expected_generation, mutation))?,
            },
            include_str!("../queries/instances/mod/replay_managed_agent.surql"),
        )
        .await
    }

    pub async fn managed_agent_registration(
        &self,
        client_id: &str,
    ) -> Result<Option<ManagedAgentRegistration>> {
        self.managed_query(
            client_id.to_owned(),
            include_str!("../queries/instances/mod/managed_agent_registration.surql"),
        )
        .await
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
                .agent_query(
                    authority,
                    false,
                    include_str!("../queries/instances/mod/mutate_managed_agent.surql"),
                )
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
        self.agent_query(
            authority,
            command,
            include_str!("../queries/instances/mutate.surql"),
        )
        .await
    }

    pub async fn managed_agent(
        &self,
        authority: &AgentCatalogAuthority,
        key: &str,
    ) -> Result<ManagedAgentInstance> {
        self.agent_query(
            authority,
            managed_agent_record(&authority.tenant, key)?,
            include_str!("../queries/instances/mod/managed_agent.surql"),
        )
        .await
    }

    pub async fn managed_agents(
        &self,
        authority: &AgentCatalogAuthority,
        after: Option<&str>,
        limit: u32,
    ) -> Result<Vec<ManagedAgentInstance>> {
        self.agent_query(
            authority,
            super::page(after, limit)?,
            include_str!("../queries/instances/mod/managed_agents.surql"),
        )
        .await
    }

    pub async fn managed_agent_operation(
        &self,
        authority: &AgentCatalogAuthority,
        id: RecordId,
    ) -> Result<ManagedAgentOperation> {
        if id.table.as_str() != "managed_agent_operation" {
            return Err(AgentManagementError::Invalid("operation"));
        }
        self.agent_query(
            authority,
            id,
            include_str!("../queries/instances/mod/managed_agent_operation.surql"),
        )
        .await
    }

    /// Internal effective-identity lookup. Authentication still verifies assertion,
    /// resource, scopes and the installation's current template ceiling.
    pub async fn managed_agent_client(
        &self,
        client_id: &str,
    ) -> Result<Option<ManagedAgentInstance>> {
        self.managed_query(
            client_id.to_owned(),
            include_str!("../queries/instances/mod/managed_agent_client.surql"),
        )
        .await
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
        self.managed_query(
            Dispatch {
                instance,
                generation,
                epoch,
            },
            include_str!("../queries/instances/mod/managed_agent_dispatch.surql"),
        )
        .await
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
        self.managed_query(
            Admission {
                instance,
                generation,
            },
            include_str!("../queries/instances/mod/managed_agent_episode_admission.surql"),
        )
        .await
    }
}
