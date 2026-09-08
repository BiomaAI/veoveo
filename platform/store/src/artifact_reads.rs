//! Durable task read delegation and atomic distinct-occurrence accounting.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;

use crate::{
    ArtifactId, ArtifactReadCapabilityId, GrantPermission, InvocationAuthorityRecord,
    PlatformIdentity, PlatformStore, PrincipalKind, StoreError, deterministic_work_context_id,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct ArtifactReadContextVersion {
    pub policy_revision: String,
    pub digest: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct ArtifactReadMembership {
    pub group_key: String,
    pub permission: GrantPermission,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct ArtifactReadAdmission {
    pub artifact: RecordId,
    pub byte_len: i64,
}

#[derive(Clone, Debug)]
pub struct ArtifactReadCapabilityDraft {
    pub capability_id: ArtifactReadCapabilityId,
    pub identity: PlatformIdentity,
    pub authority: InvocationAuthorityRecord,
    pub context_digest: String,
    pub actor_kind: PrincipalKind,
    pub actor_issuer: String,
    pub actor_subject: String,
    pub profile_key: String,
    pub server_key: String,
    pub task_id: Uuid,
    pub token_hash: String,
    pub labels: Vec<String>,
    pub memberships: Vec<ArtifactReadMembership>,
    pub max_artifact_count: i64,
    pub max_total_bytes: i64,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct ArtifactReadCapabilityRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub actor: RecordId,
    pub work_context: RecordId,
    pub authority: InvocationAuthorityRecord,
    pub context_digest: String,
    pub tenant_key: String,
    pub actor_key: String,
    pub actor_kind: PrincipalKind,
    pub actor_issuer: String,
    pub actor_subject: String,
    pub profile_key: String,
    pub server_key: String,
    pub task_id: Uuid,
    pub token_hash: String,
    pub labels: Vec<String>,
    pub memberships: Vec<ArtifactReadMembership>,
    pub max_artifact_count: i64,
    pub max_total_bytes: i64,
    pub used_total_bytes: i64,
    pub admitted_artifacts: Vec<ArtifactReadAdmission>,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl PlatformStore {
    pub async fn artifact_read_context_version(
        &self,
        tenant_key: &str,
        context_key: &str,
    ) -> Result<Option<ArtifactReadContextVersion>, StoreError> {
        let mut response = self
            .db
            .query(include_str!("artifact_reads/context.surql"))
            .bind((
                "context",
                deterministic_work_context_id(tenant_key, context_key)?.record_id(),
            ))
            .await?
            .check()?;
        response.take(0).map_err(Into::into)
    }

    pub async fn create_artifact_read_capability(
        &self,
        draft: ArtifactReadCapabilityDraft,
    ) -> Result<(), StoreError> {
        let record = ArtifactReadCapabilityRecord {
            id: draft.capability_id.record_id(),
            tenant: draft.identity.tenant_id.record_id(),
            actor: draft.identity.principal_id.record_id(),
            work_context: deterministic_work_context_id(
                &draft.identity.tenant_key,
                &draft.authority.context_key,
            )?
            .record_id(),
            authority: draft.authority,
            context_digest: draft.context_digest,
            tenant_key: draft.identity.tenant_key,
            actor_key: draft.identity.principal_key,
            actor_kind: draft.actor_kind,
            actor_issuer: draft.actor_issuer,
            actor_subject: draft.actor_subject,
            profile_key: draft.profile_key,
            server_key: draft.server_key,
            task_id: draft.task_id,
            token_hash: draft.token_hash,
            labels: draft.labels,
            memberships: draft.memberships,
            max_artifact_count: draft.max_artifact_count,
            max_total_bytes: draft.max_total_bytes,
            used_total_bytes: 0,
            admitted_artifacts: Vec::new(),
            expires_at: draft.expires_at,
            revoked_at: None,
            created_at: Utc::now(),
        };
        let mut response = self
            .db
            .query(include_str!("artifact_reads/create.surql"))
            .bind(("record", record.id.clone()))
            .bind(("content", record))
            .await?;
        if let Some(error) = crate::store::primary_transaction_error(response.take_errors()) {
            return Err(error.into());
        }
        Ok(())
    }

    /// Both retries and first reads require a currently live delegation.
    pub async fn artifact_read_capability(
        &self,
        capability: ArtifactReadCapabilityId,
        token_hash: &str,
        task_id: Uuid,
    ) -> Result<Option<ArtifactReadCapabilityRecord>, StoreError> {
        let mut response = self
            .db
            .query(include_str!("artifact_reads/authorize.surql"))
            .bind(("capability", capability.record_id()))
            .bind(("token_hash", token_hash.to_owned()))
            .bind(("task_id", task_id))
            .await?
            .check()?;
        response.take(0).map_err(Into::into)
    }

    pub async fn admit_artifact_read(
        &self,
        capability: ArtifactReadCapabilityId,
        token_hash: &str,
        task_id: Uuid,
        artifact: ArtifactId,
        byte_len: i64,
    ) -> Result<bool, StoreError> {
        if byte_len < 0 {
            return Ok(false);
        }
        let mut response = self
            .db
            .query(include_str!("artifact_reads/admit.surql"))
            .bind(("capability", capability.record_id()))
            .bind(("token_hash", token_hash.to_owned()))
            .bind(("task_id", task_id))
            .bind(("artifact", artifact.record_id()))
            .bind(("byte_len", byte_len))
            .await?
            .check()?;
        let updated: Vec<ArtifactReadCapabilityRecord> = response.take(0)?;
        if !updated.is_empty() {
            return Ok(true);
        }
        // A preceding or concurrent retry can already own the same reservation.
        // Re-read expiry, revocation and context even for a repeated occurrence.
        let current = self
            .artifact_read_capability(capability, token_hash, task_id)
            .await?;
        Ok(current.is_some_and(|cap| {
            cap.admitted_artifacts
                .iter()
                .any(|entry| entry.artifact == artifact.record_id() && entry.byte_len == byte_len)
        }))
    }

    pub async fn revoke_artifact_read_capability(
        &self,
        capability: ArtifactReadCapabilityId,
        identity: &PlatformIdentity,
    ) -> Result<bool, StoreError> {
        let mut response = self.db.query("UPDATE $capability SET revoked_at = time::now() WHERE tenant = $tenant AND actor = $actor RETURN AFTER;")
            .bind(("capability", capability.record_id()))
            .bind(("tenant", identity.tenant_id.record_id())).bind(("actor", identity.principal_id.record_id()))
            .await?.check()?;
        let revoked: Vec<ArtifactReadCapabilityRecord> = response.take(0)?;
        Ok(!revoked.is_empty())
    }
}
