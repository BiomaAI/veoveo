//! Offline, installation-authorized import of existing chat bindings. This is
//! migration tooling, never an authoring HTTP endpoint or a runtime fallback.
use super::*;
use crate::workspace::WorkspaceAgent;
use serde::{Deserialize, Serialize};
use surrealdb::types as surrealdb_types;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentChatImportMapping {
    pub key: String,
    pub source_digests: Vec<String>,
    pub target_digest: String,
}

/// Targeted export: identities and disclosure survive the digest conversion.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
#[serde(deny_unknown_fields)]
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

impl PlatformStore {
    pub async fn plan_agent_chat_import(
        &self,
        authority: &AgentCatalogAuthority,
        mappings: &[AgentChatImportMapping],
    ) -> Result<Vec<AgentChatImport>> {
        if !authority.manage_context || mappings.is_empty() || mappings.len() > 64 {
            return Err(AgentManagementError::Forbidden);
        }
        let mut keys = std::collections::BTreeSet::new();
        for mapping in mappings {
            validation::key(&mapping.key)?;
            validation::digest(&mapping.target_digest)?;
            if !keys.insert(mapping.key.clone()) || mapping.source_digests.len() > 64 {
                return Err(AgentManagementError::Invalid("import mapping"));
            }
            for source in &mapping.source_digests {
                validation::digest(source)?;
            }
            self.agent_revision(authority, &mapping.key, &mapping.target_digest)
                .await?;
        }
        let rows: Vec<WorkspaceAgent> = self.agent_query(authority, keys.into_iter().collect::<Vec<_>>(),
            "RETURN SELECT * FROM workspace_agent WHERE chat.tenant = $authority.tenant AND chat.work_context = $authority.work_context AND definition IN $command LIMIT 10001;"
        ).await?;
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
    ) -> Result<u32> {
        if !authority.manage_context || entries.len() > 10000 {
            return Err(AgentManagementError::Forbidden);
        }
        for entry in entries {
            validation::digest(&entry.before.definition_digest)?;
            validation::digest(&entry.target_digest)?;
        }
        #[derive(Clone, SurrealValue)]
        struct Import {
            entries: Vec<AgentChatImport>,
            direction: AgentChatImportDirection,
        }
        self.agent_query(
            authority,
            Import {
                entries: entries.to_vec(),
                direction,
            },
            include_str!("import.surql"),
        )
        .await
    }
}
