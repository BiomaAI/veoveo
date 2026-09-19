//! Governed agent authoring. HTTP authorization is caller-owned; the store fences
//! identity/context changes and commits revisions, replay receipts and events together.
mod records;
mod validation;
pub use records::*;

use std::time::Duration;

use surrealdb::types::{RecordId, SurrealValue, ToSql, Value};
use uuid::Uuid;

use crate::{PlatformStore, store::primary_transaction_error};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AgentManagementError {
    #[error("invalid agent field: {0}")]
    Invalid(&'static str),
    #[error("agent definition or revision is not available")]
    NotFound,
    #[error("agent operation is not permitted")]
    Forbidden,
    #[error("agent request conflicts with current state")]
    Conflict,
    #[error("agent definition capacity is exhausted")]
    Capacity,
    #[error("agent management persistence is unavailable")]
    Unavailable,
}
pub type Result<T> = std::result::Result<T, AgentManagementError>;

#[derive(Clone, SurrealValue)]
struct Page {
    after: String,
    limit: u32,
}

#[derive(Clone, SurrealValue)]
struct RevisionQuery {
    definition: RecordId,
    digest: String,
}

pub fn agent_definition_record(tenant: &RecordId, key: &str) -> Result<RecordId> {
    validation::key(key)?;
    let id = Uuid::new_v5(
        &Uuid::NAMESPACE_OID,
        format!("agent-definition:{}:{key}", tenant.to_sql()).as_bytes(),
    );
    Ok(RecordId::new(
        "agent_definition",
        surrealdb::types::Uuid::from(id),
    ))
}

impl PlatformStore {
    /// Authorized invalidation head, scoped to contexts affected by a committed mutation.
    pub async fn agent_catalog_head(&self, authority: &AgentCatalogAuthority) -> Result<i64> {
        self.agent_query(authority, false,
            "RETURN array::first(SELECT VALUE sequence FROM outbox_event WHERE tenant = $authority.tenant AND aggregate_type = 'agent_definition' AND $authority.work_context IN payload.affected_contexts ORDER BY sequence DESC LIMIT 1) ?? 0;"
        ).await
    }

    pub async fn mutate_agent_definition(
        &self,
        authority: &AgentCatalogAuthority,
        key: &str,
        request_id: Uuid,
        expected_revision: Option<i64>,
        mutation: AgentDefinitionMutation,
    ) -> Result<AgentDefinition> {
        let command = mutation_command(authority, key, request_id, expected_revision, mutation)?;
        self.agent_query(authority, command, include_str!("mutate.surql"))
            .await
    }

    /// Read an authorized receipt before repeating expensive external validation.
    pub async fn replay_agent_definition(
        &self,
        authority: &AgentCatalogAuthority,
        key: &str,
        request_id: Uuid,
        expected_revision: Option<i64>,
        mutation: AgentDefinitionMutation,
    ) -> Result<Option<AgentDefinition>> {
        let command = mutation_command(authority, key, request_id, expected_revision, mutation)?;
        self.agent_query(authority, command,
            "IF $authority.membership = 'viewer' { THROW 'agent_forbidden'; }; LET $definition = SELECT * FROM ONLY $command.definition; IF $definition != NONE AND !fn::agent_definition_editor($authority, $definition) { THROW 'agent_not_found'; }; LET $receipt = SELECT * FROM ONLY $command.receipt; IF $receipt = NONE { RETURN NONE; }; IF $receipt.fingerprint != $command.fingerprint OR $receipt.work_context != $authority.work_context { THROW 'agent_conflict'; }; RETURN $receipt.result;"
        ).await
    }

    /// Private content access also works for disabled and archived definitions.
    pub async fn agent_authored_revision(
        &self,
        authority: &AgentCatalogAuthority,
        key: &str,
        digest: &str,
    ) -> Result<AgentRevision> {
        validation::digest(digest)?;
        self.agent_query(authority, RevisionQuery { definition: agent_definition_record(&authority.tenant, key)?, digest: digest.to_owned() },
            "LET $definition = SELECT * FROM ONLY $command.definition; IF !fn::agent_definition_editor($authority, $definition) { THROW 'agent_not_found'; }; LET $revision = array::first(SELECT * FROM agent_definition_revision WHERE definition = $definition.id AND digest = $command.digest LIMIT 1); IF $revision = NONE { THROW 'agent_not_found'; }; RETURN $revision;"
        ).await
    }

    /// Private authoring access, constrained to the current home context and owner.
    pub async fn agent_definition(
        &self,
        authority: &AgentCatalogAuthority,
        key: &str,
    ) -> Result<AgentDefinition> {
        self.agent_query(authority, agent_definition_record(&authority.tenant, key)?,
            "LET $definition = SELECT * FROM ONLY $command; IF !fn::agent_definition_editor($authority, $definition) { THROW 'agent_not_found'; }; RETURN $definition;"
        ).await
    }

    pub async fn agent_definitions(
        &self,
        authority: &AgentCatalogAuthority,
        after: Option<&str>,
        limit: u32,
    ) -> Result<Vec<AgentDefinition>> {
        let page = page(after, limit)?;
        self.agent_query(authority, page,
            "RETURN SELECT * FROM agent_definition WHERE tenant = $authority.tenant AND work_context = $authority.work_context AND key > $command.after AND (owner = $authority.principal OR $authority.manage_context) ORDER BY key LIMIT $command.limit;"
        ).await
    }

    pub async fn agent_catalog(
        &self,
        authority: &AgentCatalogAuthority,
        after: Option<&str>,
        limit: u32,
    ) -> Result<Vec<AgentCatalogEntry>> {
        self.agent_query(
            authority,
            page(after, limit)?,
            include_str!("catalog.surql"),
        )
        .await
    }

    /// Resolve an explicitly admitted revision. Archive preserves existing use;
    /// disable, audience removal and current context revocation close execution.
    pub async fn agent_revision(
        &self,
        authority: &AgentCatalogAuthority,
        key: &str,
        digest: &str,
    ) -> Result<AgentRevision> {
        validation::digest(digest)?;
        self.agent_query(authority, RevisionQuery { definition: agent_definition_record(&authority.tenant, key)?, digest: digest.to_owned() },
            "LET $definition = SELECT * FROM ONLY $command.definition; IF $definition = NONE OR $definition.tenant != $authority.tenant OR $definition.disabled OR $authority.work_context NOT IN $definition.audience { THROW 'agent_not_found'; }; LET $revision = array::first(SELECT * FROM agent_definition_revision WHERE definition = $definition.id AND digest = $command.digest LIMIT 1); IF $revision = NONE { THROW 'agent_not_found'; }; RETURN $revision;"
        ).await
    }

    pub async fn agent_revision_history(
        &self,
        authority: &AgentCatalogAuthority,
        key: &str,
        after: Option<&str>,
        limit: u32,
    ) -> Result<Vec<AgentRevision>> {
        if !(1..=100).contains(&limit) {
            return Err(AgentManagementError::Invalid("page"));
        }
        if let Some(digest) = after {
            validation::digest(digest)?;
        }
        #[derive(Clone, SurrealValue)]
        struct History {
            definition: RecordId,
            after: Option<String>,
            limit: u32,
        }
        self.agent_query(authority, History { definition: agent_definition_record(&authority.tenant, key)?, after: after.map(str::to_owned), limit },
            "LET $definition = SELECT * FROM ONLY $command.definition; IF !fn::agent_definition_editor($authority, $definition) { THROW 'agent_not_found'; }; LET $cursor = array::first(SELECT * FROM agent_definition_revision WHERE definition = $definition.id AND digest = $command.after LIMIT 1); IF $command.after != NONE AND $cursor = NONE { THROW 'agent_not_found'; }; RETURN SELECT * FROM agent_definition_revision WHERE definition = $definition.id AND ($cursor = NONE OR created_at < $cursor.created_at OR (created_at = $cursor.created_at AND digest < $cursor.digest)) ORDER BY created_at DESC, digest DESC LIMIT $command.limit;"
        ).await
    }

    async fn agent_query<C: SurrealValue + Clone, R: SurrealValue>(
        &self,
        authority: &AgentCatalogAuthority,
        command: C,
        sql: &'static str,
    ) -> Result<R> {
        let query = format!(
            "BEGIN TRANSACTION; IF !fn::agent_catalog_authority($authority) {{ THROW 'agent_forbidden'; }}; {sql} COMMIT TRANSACTION;"
        );
        for attempt in 0..8 {
            let mut response = self
                .client()
                .query(query.clone())
                .bind(("authority", authority.clone()))
                .bind(("command", command.clone()))
                .await
                .map_err(|_| AgentManagementError::Unavailable)?;
            if let Some(error) = primary_transaction_error(response.take_errors()) {
                if retryable(&error) && attempt < 7 {
                    tokio::time::sleep(Duration::from_millis(1 << attempt)).await;
                    continue;
                }
                return Err(classify(&error));
            }
            let last = response
                .num_statements()
                .checked_sub(2)
                .ok_or(AgentManagementError::Unavailable)?;
            let value: Value = response
                .take(last)
                .map_err(|_| AgentManagementError::Unavailable)?;
            return R::from_value(value).map_err(|_| AgentManagementError::Unavailable);
        }
        Err(AgentManagementError::Unavailable)
    }
}

fn mutation_command(
    authority: &AgentCatalogAuthority,
    key: &str,
    request_id: Uuid,
    expected_revision: Option<i64>,
    mutation: AgentDefinitionMutation,
) -> Result<MutationCommand> {
    validation::key(key)?;
    validation::mutation(&mutation)?;
    if request_id.get_version_num() != 7
        || expected_revision.is_some_and(|r| r < 1)
        || !(1..=10_000).contains(&authority.definition_limit)
    {
        return Err(AgentManagementError::Invalid("request"));
    }
    if matches!(mutation, AgentDefinitionMutation::Create { .. }) != expected_revision.is_none() {
        return Err(AgentManagementError::Invalid("expected revision"));
    }
    // Context digests are current admission evidence, not client mutation input.
    // Policy refresh must not change the identity of an otherwise identical retry.
    let mut canonical = mutation.clone();
    if let AgentDefinitionMutation::Publish { audience, .. } = &mut canonical {
        for target in audience {
            target.context_digest.clear();
        }
    }
    let fingerprint = validation::hash(&(key, expected_revision, &canonical))?;
    let content_digest = match &mutation {
        AgentDefinitionMutation::Create { content, .. }
        | AgentDefinitionMutation::Draft { content } => Some(content.digest()?),
        _ => None,
    };
    let receipt_id = Uuid::new_v5(
        &request_id,
        format!(
            "{}:{}",
            authority.tenant.to_sql(),
            authority.principal.to_sql()
        )
        .as_bytes(),
    );
    Ok(MutationCommand {
        definition: agent_definition_record(&authority.tenant, key)?,
        receipt: RecordId::new(
            "agent_definition_receipt",
            surrealdb::types::Uuid::from(receipt_id),
        ),
        request_id,
        fingerprint,
        key: key.to_owned(),
        expected_revision,
        content_digest,
        mutation,
    })
}

fn page(after: Option<&str>, limit: u32) -> Result<Page> {
    if !(1..=200).contains(&limit) {
        return Err(AgentManagementError::Invalid("page"));
    }
    if let Some(after) = after {
        validation::key(after)?;
    }
    Ok(Page {
        after: after.unwrap_or("").to_owned(),
        limit,
    })
}

fn retryable(error: &surrealdb::Error) -> bool {
    matches!(
        error.query_details(),
        Some(surrealdb::types::QueryError::TransactionConflict)
    ) || error.message().starts_with("Transaction conflict:")
}
fn classify(error: &surrealdb::Error) -> AgentManagementError {
    let message = error.message();
    if message.contains("agent_not_found") {
        AgentManagementError::NotFound
    } else if message.contains("agent_forbidden") {
        AgentManagementError::Forbidden
    } else if message.contains("agent_capacity") {
        AgentManagementError::Capacity
    } else if message.contains("agent_conflict") || retryable(error) {
        AgentManagementError::Conflict
    } else {
        AgentManagementError::Unavailable
    }
}
