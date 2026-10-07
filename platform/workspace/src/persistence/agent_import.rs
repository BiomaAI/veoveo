//! Offline, installation-authorized import of existing chat bindings. This is
//! migration tooling, never an authoring HTTP endpoint or a runtime fallback.
use super::WorkspaceAgent;
use super::*;
use serde::{Deserialize, Serialize};
use surrealdb::types as surrealdb_types;
use surrealdb::types::{SurrealValue, Value};
use veoveo_agent_runtime::persistence::*;
use veoveo_platform_store::primary_transaction_error;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentChatImportMapping {
    pub key: String,
    pub source_digests: Vec<String>,
    pub target_digest: String,
}

/// Targeted export: identities and disclosure survive the digest conversion.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentChatImport {
    pub before: WorkspaceAgent,
    pub target_digest: String,
}

#[derive(Clone, Copy, SurrealValue)]
#[surreal(untagged)]
pub enum AgentChatImportDirection {
    #[surreal(value = "apply")]
    Apply,
    #[surreal(value = "restore")]
    Restore,
}

impl WorkspaceRepository {
    pub async fn plan_agent_chat_import(
        &self,
        authority: &AgentCatalogAuthority,
        mappings: &[AgentChatImportMapping],
    ) -> veoveo_agent_runtime::persistence::Result<Vec<AgentChatImport>> {
        if !authority.manage_context || mappings.is_empty() || mappings.len() > 64 {
            return Err(AgentManagementError::Forbidden);
        }
        let mut keys = std::collections::BTreeSet::new();
        for mapping in mappings {
            veoveo_agent_runtime::persistence::validate_agent_key(&mapping.key)?;
            veoveo_agent_runtime::persistence::validate_agent_digest(&mapping.target_digest)?;
            if !keys.insert(mapping.key.clone()) || mapping.source_digests.len() > 64 {
                return Err(AgentManagementError::Invalid("import mapping"));
            }
            for source in &mapping.source_digests {
                veoveo_agent_runtime::persistence::validate_agent_digest(source)?;
            }
            AgentRepository::new(self.store.clone())
                .agent_revision(authority, &mapping.key, &mapping.target_digest)
                .await?;
        }
        let rows: Vec<WorkspaceAgent> = self
            .agent_import_query(
                authority,
                keys.into_iter().collect::<Vec<_>>(),
                include_str!("queries/agent_import/plan_agent_chat_import.surql"),
            )
            .await?;
        if rows.len() > 10000 {
            return Err(AgentManagementError::Capacity);
        }
        rows.into_iter()
            .filter_map(|before| {
                let mapping = mappings
                    .iter()
                    .find(|m| m.key == before.definition)
                    .expect("query predicate");
                if before.definition_digest == mapping.target_digest {
                    return None;
                }
                Some(
                    if mapping.source_digests.contains(&before.definition_digest) {
                        Ok(AgentChatImport {
                            before,
                            target_digest: mapping.target_digest.clone(),
                        })
                    } else {
                        Err(AgentManagementError::Conflict)
                    },
                )
            })
            .collect()
    }

    /// The caller must stop every gateway writer before applying or restoring.
    /// Restore is only a pre-reopening migration recovery operation.
    pub async fn apply_agent_chat_import(
        &self,
        authority: &AgentCatalogAuthority,
        entries: &[AgentChatImport],
        direction: AgentChatImportDirection,
    ) -> veoveo_agent_runtime::persistence::Result<u32> {
        if !authority.manage_context || entries.len() > 10000 {
            return Err(AgentManagementError::Forbidden);
        }
        for entry in entries {
            veoveo_agent_runtime::persistence::validate_agent_digest(
                &entry.before.definition_digest,
            )?;
            veoveo_agent_runtime::persistence::validate_agent_digest(&entry.target_digest)?;
        }
        #[derive(Clone, SurrealValue)]
        struct Import {
            entries: Vec<AgentChatImport>,
            direction: AgentChatImportDirection,
        }
        self.agent_import_query(
            authority,
            Import {
                entries: entries.to_vec(),
                direction,
            },
            include_str!("queries/import.surql"),
        )
        .await
    }
    async fn agent_import_query<C: SurrealValue + Clone, R: SurrealValue>(
        &self,
        authority: &AgentCatalogAuthority,
        command: C,
        sql: &'static str,
    ) -> veoveo_agent_runtime::persistence::Result<R> {
        for attempt in 0..8 {
            let mut response = self
                .client()
                .query(include_str!("queries/agent_import_begin.surql"))
                .query(sql)
                .query(include_str!("queries/transaction_commit.surql"))
                .bind(("authority", authority.clone()))
                .bind(("command", command.clone()))
                .await
                .map_err(|_| AgentManagementError::Unavailable)?;
            if let Some(error) = primary_transaction_error(response.take_errors()) {
                if retryable_agent_transaction(&error) && attempt < 7 {
                    tokio::time::sleep(std::time::Duration::from_millis(1 << attempt)).await;
                    continue;
                }
                return Err(classify_agent_transaction(&error));
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
