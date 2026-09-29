//! Recording projection receipt lifecycle and SQL download admission.
use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};
use veoveo_types::{DataLabelId, PolicyVersion};

use super::{typed_uuid_from_record, validate_sha256, validate_text};
use crate::{
    PlatformIdentity, PlatformStore, PrincipalId, RecordingId, RecordingProjectionReceiptId,
    RecordingProjectionReceiptRecord, RecordingProjectionState, RecordingReadGrantClass,
    RecordingReadGrantId, StoreError, TenantId, WorkContextId,
};

/// Current caller authority for redeeming one ready projection.
/// Identifiers remain typed until the query's driver bindings.
#[derive(Clone, Debug)]
pub struct RecordingProjectionReadScope {
    pub tenant_id: TenantId,
    pub actor_id: PrincipalId,
    pub work_context_id: WorkContextId,
    pub policy_revision: PolicyVersion,
    pub data_labels: BTreeSet<DataLabelId>,
}

#[derive(Clone, Debug)]
pub struct RecordingProjectionReceiptDraft {
    pub identity: PlatformIdentity,
    pub grant_id: RecordingReadGrantId,
    pub caller_idempotency_key: String,
    pub manifest_digest: String,
    pub query_digest: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct RecordingProjectionReceiptContent {
    tenant: RecordId,
    grant: RecordId,
    dataset: RecordId,
    recordings: Vec<RecordId>,
    actor: RecordId,
    work_context: RecordId,
    policy_revision: String,
    catalog_revision: String,
    caller_idempotency_key: String,
    manifest_digest: String,
    query_digest: String,
    state: RecordingProjectionState,
    result_byte_len: Option<i64>,
    result_sha256: Option<String>,
    failure_reason: Option<String>,
    expires_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl PlatformStore {
    /// Admit the receipt and its source/grant relationships before Rust decoding.
    pub async fn ready_recording_projection(
        &self,
        scope: &RecordingProjectionReadScope,
        recording_id: RecordingId,
        projection_id: RecordingProjectionReceiptId,
    ) -> Result<Option<RecordingProjectionReceiptRecord>, StoreError> {
        let mut response = self
            .db
            .query(
                "SELECT * FROM ONLY $projection
             WHERE tenant = $tenant AND actor = $actor
               AND work_context = $work_context AND policy_revision = $policy_revision
               AND recordings = [$recording] AND state = $ready
               AND expires_at > time::now()
               AND $recording.tenant = $tenant AND $clearance CONTAINSALL $recording.labels
               AND dataset = $recording.dataset AND dataset.tenant = $tenant
               AND grant.tenant = $tenant AND grant.actor = $actor
               AND grant.work_context = $work_context AND grant.policy_revision = $policy_revision
               AND grant.grant_class = $grant_class AND grant.dataset = dataset
               AND grant.recordings = recordings AND grant.catalog_revision = catalog_revision
               AND grant.expires_at >= expires_at AND grant.expires_at > time::now();",
            )
            .bind(("projection", projection_id.record_id()))
            .bind(("recording", recording_id.record_id()))
            .bind(("tenant", scope.tenant_id.record_id()))
            .bind(("actor", scope.actor_id.record_id()))
            .bind(("work_context", scope.work_context_id.record_id()))
            .bind(("policy_revision", scope.policy_revision.to_string()))
            .bind((
                "clearance",
                scope
                    .data_labels
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            ))
            .bind(("ready", RecordingProjectionState::Ready))
            .bind(("grant_class", RecordingReadGrantClass::AppProjection))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn reserve_recording_projection(
        &self,
        draft: RecordingProjectionReceiptDraft,
    ) -> Result<RecordingProjectionReceiptRecord, StoreError> {
        validate_text(
            "projection idempotency key",
            &draft.caller_idempotency_key,
            128,
        )?;
        validate_sha256("manifest_digest", &draft.manifest_digest)?;
        validate_sha256("query_digest", &draft.query_digest)?;
        let grant = self
            .recording_read_grant(draft.identity.tenant_id, draft.grant_id)
            .await?
            .ok_or(StoreError::RecordingReadGrantConflict {
                grant_id: draft.grant_id.to_string(),
            })?;
        if grant.actor != draft.identity.principal_id.record_id()
            || grant.grant_class != RecordingReadGrantClass::AppProjection
            || draft.expires_at > grant.expires_at
            || draft.expires_at <= Utc::now()
        {
            return Err(StoreError::RecordingReadGrantConflict {
                grant_id: draft.grant_id.to_string(),
            });
        }
        if let Some(existing) = self
            .recording_projection_by_idempotency_key(
                draft.identity.tenant_id,
                draft.identity.principal_id,
                &draft.caller_idempotency_key,
            )
            .await?
        {
            if existing.manifest_digest == draft.manifest_digest
                && existing.query_digest == draft.query_digest
            {
                return Ok(existing);
            }
            return Err(StoreError::RecordingProjectionConflict {
                projection_id: projection_id_from_record(&existing.id)?.to_string(),
            });
        }
        let id = RecordingProjectionReceiptId::new();
        let now = Utc::now();
        let content = RecordingProjectionReceiptContent {
            tenant: draft.identity.tenant_id.record_id(),
            grant: draft.grant_id.record_id(),
            dataset: grant.dataset,
            recordings: grant.recordings,
            actor: grant.actor,
            work_context: grant.work_context,
            policy_revision: grant.policy_revision,
            catalog_revision: grant.catalog_revision,
            caller_idempotency_key: draft.caller_idempotency_key.clone(),
            manifest_digest: draft.manifest_digest.clone(),
            query_digest: draft.query_digest.clone(),
            state: RecordingProjectionState::Reserved,
            result_byte_len: None,
            result_sha256: None,
            failure_reason: None,
            expires_at: draft.expires_at,
            created_at: now,
            updated_at: now,
        };
        let result = self
            .db
            .query("CREATE ONLY $projection CONTENT $content RETURN NONE;")
            .bind(("projection", id.record_id()))
            .bind(("content", content))
            .await
            .and_then(|response| response.check());
        if let Err(error) = result {
            if let Some(existing) = self
                .recording_projection_by_idempotency_key(
                    draft.identity.tenant_id,
                    draft.identity.principal_id,
                    &draft.caller_idempotency_key,
                )
                .await?
            {
                if existing.manifest_digest == draft.manifest_digest
                    && existing.query_digest == draft.query_digest
                {
                    return Ok(existing);
                }
                return Err(StoreError::RecordingProjectionConflict {
                    projection_id: projection_id_from_record(&existing.id)?.to_string(),
                });
            }
            return Err(error.into());
        }
        self.recording_projection_receipt(draft.identity.tenant_id, id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "recording projection reservation readback",
            })
    }

    pub async fn recording_projection_receipt(
        &self,
        tenant_id: TenantId,
        projection_id: RecordingProjectionReceiptId,
    ) -> Result<Option<RecordingProjectionReceiptRecord>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM ONLY $projection WHERE tenant = $tenant AND expires_at > time::now();")
            .bind(("projection", projection_id.record_id()))
            .bind(("tenant", tenant_id.record_id()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn begin_recording_projection(
        &self,
        identity: &PlatformIdentity,
        projection_id: RecordingProjectionReceiptId,
    ) -> Result<RecordingProjectionReceiptRecord, StoreError> {
        let existing = self
            .recording_projection_receipt(identity.tenant_id, projection_id)
            .await?
            .ok_or(StoreError::RecordingProjectionConflict {
                projection_id: projection_id.to_string(),
            })?;
        ensure_projection_actor(&existing, identity, projection_id)?;
        if existing.state == RecordingProjectionState::Materializing {
            return Ok(existing);
        }
        if existing.state != RecordingProjectionState::Reserved {
            return Err(StoreError::RecordingProjectionConflict {
                projection_id: projection_id.to_string(),
            });
        }
        self.db
            .query("LET $current = (SELECT * FROM ONLY $projection); IF $current.state != 'reserved' { THROW 'recording_projection_state_conflict'; }; UPDATE ONLY $projection SET state = 'materializing', updated_at = time::now() RETURN NONE;")
            .bind(("projection", projection_id.record_id()))
            .await?
            .check()?;
        self.recording_projection_receipt(identity.tenant_id, projection_id)
            .await?
            .ok_or(StoreError::RecordingProjectionConflict {
                projection_id: projection_id.to_string(),
            })
    }

    pub async fn complete_recording_projection(
        &self,
        identity: &PlatformIdentity,
        projection_id: RecordingProjectionReceiptId,
        result_byte_len: i64,
        result_sha256: &str,
    ) -> Result<RecordingProjectionReceiptRecord, StoreError> {
        if result_byte_len < 0 {
            return Err(StoreError::InvalidRecordingField {
                field: "projection result_byte_len",
                reason: "must be non-negative",
            });
        }
        validate_sha256("projection result_sha256", result_sha256)?;
        let existing = self
            .recording_projection_receipt(identity.tenant_id, projection_id)
            .await?
            .ok_or(StoreError::RecordingProjectionConflict {
                projection_id: projection_id.to_string(),
            })?;
        ensure_projection_actor(&existing, identity, projection_id)?;
        if existing.state == RecordingProjectionState::Ready
            && existing.result_byte_len == Some(result_byte_len)
            && existing.result_sha256.as_deref() == Some(result_sha256)
        {
            return Ok(existing);
        }
        if existing.state != RecordingProjectionState::Materializing {
            return Err(StoreError::RecordingProjectionConflict {
                projection_id: projection_id.to_string(),
            });
        }
        self.db
            .query("LET $current = (SELECT * FROM ONLY $projection); IF $current.state != 'materializing' { THROW 'recording_projection_state_conflict'; }; UPDATE ONLY $projection SET state = 'ready', result_byte_len = $byte_len, result_sha256 = $sha256, failure_reason = NONE, updated_at = time::now() RETURN NONE;")
            .bind(("projection", projection_id.record_id()))
            .bind(("byte_len", result_byte_len))
            .bind(("sha256", result_sha256.to_owned()))
            .await?
            .check()?;
        self.recording_projection_receipt(identity.tenant_id, projection_id)
            .await?
            .ok_or(StoreError::RecordingProjectionConflict {
                projection_id: projection_id.to_string(),
            })
    }

    pub async fn fail_recording_projection(
        &self,
        identity: &PlatformIdentity,
        projection_id: RecordingProjectionReceiptId,
        reason: &str,
    ) -> Result<RecordingProjectionReceiptRecord, StoreError> {
        self.finish_recording_projection(
            identity,
            projection_id,
            RecordingProjectionState::Failed,
            reason,
        )
        .await
    }

    pub async fn cancel_recording_projection(
        &self,
        identity: &PlatformIdentity,
        projection_id: RecordingProjectionReceiptId,
        reason: &str,
    ) -> Result<RecordingProjectionReceiptRecord, StoreError> {
        self.finish_recording_projection(
            identity,
            projection_id,
            RecordingProjectionState::Cancelled,
            reason,
        )
        .await
    }

    async fn finish_recording_projection(
        &self,
        identity: &PlatformIdentity,
        projection_id: RecordingProjectionReceiptId,
        target: RecordingProjectionState,
        reason: &str,
    ) -> Result<RecordingProjectionReceiptRecord, StoreError> {
        validate_text("projection failure_reason", reason, 2_048)?;
        if !matches!(
            target,
            RecordingProjectionState::Failed | RecordingProjectionState::Cancelled
        ) {
            return Err(StoreError::RecordingProjectionConflict {
                projection_id: projection_id.to_string(),
            });
        }
        let existing = self
            .recording_projection_receipt(identity.tenant_id, projection_id)
            .await?
            .ok_or(StoreError::RecordingProjectionConflict {
                projection_id: projection_id.to_string(),
            })?;
        ensure_projection_actor(&existing, identity, projection_id)?;
        if existing.state == target && existing.failure_reason.as_deref() == Some(reason) {
            return Ok(existing);
        }
        if !matches!(
            existing.state,
            RecordingProjectionState::Reserved | RecordingProjectionState::Materializing
        ) {
            return Err(StoreError::RecordingProjectionConflict {
                projection_id: projection_id.to_string(),
            });
        }
        self.db
            .query("LET $current = (SELECT * FROM ONLY $projection); IF $current.state NOT IN ['reserved', 'materializing'] { THROW 'recording_projection_state_conflict'; }; UPDATE ONLY $projection SET state = $state, failure_reason = $reason, updated_at = time::now() RETURN NONE;")
            .bind(("projection", projection_id.record_id()))
            .bind(("state", target))
            .bind(("reason", reason.to_owned()))
            .await?
            .check()?;
        self.recording_projection_receipt(identity.tenant_id, projection_id)
            .await?
            .ok_or(StoreError::RecordingProjectionConflict {
                projection_id: projection_id.to_string(),
            })
    }

    pub async fn recording_projection_by_idempotency_key(
        &self,
        tenant_id: TenantId,
        actor_id: crate::PrincipalId,
        idempotency_key: &str,
    ) -> Result<Option<RecordingProjectionReceiptRecord>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM recording_projection_receipt WHERE tenant = $tenant AND actor = $actor AND caller_idempotency_key = $key AND expires_at > time::now() LIMIT 1;")
            .bind(("tenant", tenant_id.record_id()))
            .bind(("actor", actor_id.record_id()))
            .bind(("key", idempotency_key.to_owned()))
            .await?
            .check()?;
        let records: Vec<RecordingProjectionReceiptRecord> = response.take(0)?;
        Ok(records.into_iter().next())
    }
}

fn ensure_projection_actor(
    receipt: &RecordingProjectionReceiptRecord,
    identity: &PlatformIdentity,
    projection_id: RecordingProjectionReceiptId,
) -> Result<(), StoreError> {
    if receipt.actor != identity.principal_id.record_id() {
        return Err(StoreError::RecordingProjectionConflict {
            projection_id: projection_id.to_string(),
        });
    }
    Ok(())
}

fn projection_id_from_record(
    record: &RecordId,
) -> Result<RecordingProjectionReceiptId, StoreError> {
    typed_uuid_from_record(record, RecordingProjectionReceiptId::TABLE)
        .map(RecordingProjectionReceiptId::from_uuid)
}
