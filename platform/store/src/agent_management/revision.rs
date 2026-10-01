//! Current authorized view revisions, independent of other tenants' writes.
use chrono::{DateTime, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use surrealdb::types::SurrealValue;
use veoveo_types::Sha256Digest;

use super::{AgentCatalogAuthority, AgentManagementError, Result};
use crate::PlatformStore;

#[derive(Serialize, SurrealValue)]
struct DefinitionRevision {
    key: String,
    revision: i64,
}

#[derive(Serialize, SurrealValue)]
struct InstanceRevision {
    key: String,
    generation: i64,
    dispatch_epoch: i64,
    updated_at: DateTime<Utc>,
}

#[derive(Serialize, SurrealValue)]
struct ManagementRevision {
    catalog: Vec<DefinitionRevision>,
    definitions: Vec<DefinitionRevision>,
    instances: Vec<InstanceRevision>,
}

fn digest(value: &impl Serialize) -> Result<Sha256Digest> {
    let bytes = serde_json::to_vec(value).map_err(|_| AgentManagementError::Unavailable)?;
    Ok(Sha256Digest::from_bytes(Sha256::digest(bytes).into()))
}

impl PlatformStore {
    /// Metadata admitted by the same SQL predicate as the public catalog.
    pub async fn agent_catalog_revision(
        &self,
        authority: &AgentCatalogAuthority,
    ) -> Result<Sha256Digest> {
        let rows: Vec<DefinitionRevision> = self.agent_query(authority, false,
            "RETURN SELECT key, revision FROM agent_definition WHERE tenant = $authority.tenant AND status = 'enabled' AND disabled = false AND $authority.work_context IN audience AND published != NONE ORDER BY key;"
        ).await?;
        digest(&rows)
    }

    /// Hash each ordered, currently admitted set. Removal changes the view even
    /// when the caller cannot read the removed record's old payload.
    pub async fn agent_management_revision(
        &self,
        authority: &AgentCatalogAuthority,
    ) -> Result<Sha256Digest> {
        let rows: ManagementRevision = self
            .agent_query(authority, false, include_str!("revision.surql"))
            .await?;
        digest(&rows)
    }
}
