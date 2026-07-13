use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use surrealdb::types::{RecordId, RecordIdKey};
use uuid::Uuid;

use crate::{
    ArtifactBlobId, ArtifactBlobRecord, ArtifactGrantEdge, ArtifactGrantSubjectKind, ArtifactId,
    ArtifactOccurrenceRecord, ArtifactReleaseState, ArtifactWriteCapabilityId,
    ArtifactWriteCapabilityRecord, ArtifactWriteRedemptionId, ArtifactWriteRedemptionRecord,
    ArtifactWriteRedemptionState, AuditEventId, AuditEventRecord, AuditOutcome, GrantPermission,
    OpenObject, OutboxDraft, PlatformIdentity, PlatformStore, PrincipalId, PrincipalKind,
    PrincipalRecord, ShareLinkId, ShareLinkRecord, StoreError, TaskId, TenantId, TenantRecord,
};

use crate::identity::PLATFORM_ID_NAMESPACE;
use crate::store::primary_transaction_error;

#[derive(Clone, Debug)]
pub struct ArtifactOccurrenceDraft {
    pub artifact_id: ArtifactId,
    pub identity: PlatformIdentity,
    pub sha256: String,
    pub byte_len: i64,
    pub object_key: String,
    pub media_type: String,
    pub filename: Option<String>,
    pub classification: String,
    pub labels: Vec<String>,
    pub metadata: BTreeMap<String, serde_json::Value>,
    pub retention_expires_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug)]
pub struct ArtifactGrantDraft {
    pub artifact_id: ArtifactId,
    pub subject: RecordId,
    pub subject_kind: ArtifactGrantSubjectKind,
    pub subject_key: String,
    pub permission: GrantPermission,
    pub labels: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_by: PrincipalId,
}

#[derive(Clone, Debug)]
pub struct ArtifactAggregate {
    pub occurrence: ArtifactOccurrenceRecord,
    pub blob: ArtifactBlobRecord,
    pub tenant: TenantRecord,
    pub owner: PrincipalRecord,
    pub grants: Vec<ArtifactGrantEdge>,
}

#[derive(Clone, Debug)]
pub struct ArtifactWriteCapabilityDraft {
    pub capability_id: ArtifactWriteCapabilityId,
    pub identity: PlatformIdentity,
    pub profile_key: String,
    pub server_key: String,
    pub task_id: String,
    pub owner_kind: PrincipalKind,
    pub owner_issuer: String,
    pub owner_subject: String,
    pub token_hash: String,
    pub labels: Vec<String>,
    pub max_artifact_count: i64,
    pub max_total_bytes: i64,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct ArtifactWriteReservation {
    pub capability: ArtifactWriteCapabilityRecord,
    pub redemption: ArtifactWriteRedemptionRecord,
    pub request_matches: bool,
}

#[derive(Clone, Debug)]
pub struct ArtifactShareLinkDraft {
    pub link_id: ShareLinkId,
    pub artifact_id: ArtifactId,
    pub identity: PlatformIdentity,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub max_downloads: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct PublicShareRedemption {
    pub link: ShareLinkRecord,
    pub artifact_id: ArtifactId,
}

#[derive(Clone, Debug)]
pub struct ArtifactAuditDraft {
    pub tenant: Option<TenantId>,
    pub actor: Option<PrincipalId>,
    pub action: String,
    pub resource_id: Option<String>,
    pub outcome: AuditOutcome,
    pub details: BTreeMap<String, serde_json::Value>,
}

impl PlatformStore {
    pub async fn create_artifact_occurrence(
        &self,
        draft: ArtifactOccurrenceDraft,
    ) -> Result<ArtifactAggregate, StoreError> {
        let blob_id = ArtifactBlobId::from_uuid(Uuid::new_v5(
            &PLATFORM_ID_NAMESPACE,
            format!("blob:{}:{}", draft.identity.tenant_key, draft.sha256).as_bytes(),
        ));
        let now = Utc::now();
        let blob = ArtifactBlobRecord {
            id: blob_id.record_id(),
            tenant: draft.identity.tenant_id.record_id(),
            sha256: draft.sha256,
            byte_len: draft.byte_len,
            object_key: draft.object_key,
            content_type: draft.media_type.clone(),
            encryption: OpenObject::default(),
            created_at: now,
        };
        let occurrence = ArtifactOccurrenceRecord {
            id: draft.artifact_id.record_id(),
            tenant: draft.identity.tenant_id.record_id(),
            blob: blob_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            task: None,
            filename: draft.filename,
            media_type: draft.media_type,
            classification: draft.classification,
            labels: draft.labels.clone(),
            metadata: OpenObject::new(draft.metadata),
            release_state: ArtifactReleaseState::Private,
            retention_expires_at: draft.retention_expires_at,
            created_at: now,
            updated_at: now,
            search_text: String::new(),
        };
        let grant_id = deterministic_relation_id(
            "artifact-grant",
            draft.artifact_id.to_string(),
            draft.identity.principal_id.to_string(),
        );
        let grant = ArtifactGrantEdge {
            id: grant_id.clone(),
            r#in: draft.artifact_id.record_id(),
            out: draft.identity.principal_id.record_id(),
            subject_kind: ArtifactGrantSubjectKind::User,
            subject_key: draft.identity.principal_key,
            permission: GrantPermission::Admin,
            labels: draft.labels,
            expires_at: draft.retention_expires_at,
            created_by: draft.identity.principal_id.record_id(),
            created_at: now,
        };
        let outbox = OutboxDraft::now(
            Some(draft.identity.tenant_id.record_id()),
            "artifact",
            draft.artifact_id.to_string(),
            "artifact.created",
            1,
            OpenObject::new(BTreeMap::from([(
                "artifact_id".into(),
                serde_json::json!(draft.artifact_id.to_string()),
            )])),
        );
        let mut response = self.db
            .query(
                "BEGIN TRANSACTION; UPSERT ONLY $blob CONTENT $blob_content RETURN NONE; CREATE ONLY $artifact CONTENT $artifact_content RETURN NONE; RELATE ONLY $artifact->$grant->$subject CONTENT $grant_content RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;",
            )
            .bind(("blob", blob_id.record_id()))
            .bind(("blob_content", blob.clone()))
            .bind(("artifact", draft.artifact_id.record_id()))
            .bind(("artifact_content", occurrence.clone()))
            .bind(("grant", grant_id))
            .bind(("subject", grant.out.clone()))
            .bind(("grant_content", grant.clone()))
            .bind(("outbox", outbox))
            .await?;
        let errors = response.take_errors();
        if let Some(error) = primary_transaction_error(errors) {
            return Err(error.into());
        }
        self.artifact_aggregate(draft.artifact_id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "artifact occurrence creation readback",
            })
    }

    pub async fn artifact_aggregate(
        &self,
        artifact_id: ArtifactId,
    ) -> Result<Option<ArtifactAggregate>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM ONLY $artifact;")
            .bind(("artifact", artifact_id.record_id()))
            .await?
            .check()?;
        let occurrence: Option<ArtifactOccurrenceRecord> = response.take(0)?;
        let Some(occurrence) = occurrence else {
            return Ok(None);
        };
        let mut response = self
            .db
            .query("SELECT * FROM ONLY $blob; SELECT * FROM ONLY $tenant; SELECT * FROM ONLY $owner; SELECT * FROM artifact_grant WHERE in = $artifact;")
            .bind(("blob", occurrence.blob.clone()))
            .bind(("tenant", occurrence.tenant.clone()))
            .bind(("owner", occurrence.owner.clone()))
            .bind(("artifact", artifact_id.record_id()))
            .await?
            .check()?;
        let blob =
            response
                .take::<Option<ArtifactBlobRecord>>(0)?
                .ok_or(StoreError::MissingRecord {
                    operation: "artifact blob lookup",
                })?;
        let tenant =
            response
                .take::<Option<TenantRecord>>(1)?
                .ok_or(StoreError::MissingRecord {
                    operation: "artifact tenant lookup",
                })?;
        let owner =
            response
                .take::<Option<PrincipalRecord>>(2)?
                .ok_or(StoreError::MissingRecord {
                    operation: "artifact owner lookup",
                })?;
        let grants = response.take(3)?;
        Ok(Some(ArtifactAggregate {
            occurrence,
            blob,
            tenant,
            owner,
            grants,
        }))
    }

    /// Return occurrence ids connected to one of `subjects` by a live grant.
    /// The caller-facing artifact service remains responsible for evaluating
    /// tenant, clearance, group-role, and requested-level policy on each
    /// aggregate before exposing it.
    pub async fn artifact_ids_for_subjects(
        &self,
        tenant: TenantId,
        subjects: Vec<RecordId>,
        cursor: Option<ArtifactId>,
        limit: usize,
    ) -> Result<Vec<ArtifactId>, StoreError> {
        if subjects.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let query = if cursor.is_some() {
            "SELECT VALUE id FROM artifact_occurrence WHERE tenant = $tenant AND id < $cursor AND id IN (SELECT VALUE in FROM artifact_grant WHERE out IN $subjects AND (expires_at = NONE OR expires_at > time::now())) ORDER BY id DESC LIMIT $limit;"
        } else {
            "SELECT VALUE id FROM artifact_occurrence WHERE tenant = $tenant AND id IN (SELECT VALUE in FROM artifact_grant WHERE out IN $subjects AND (expires_at = NONE OR expires_at > time::now())) ORDER BY id DESC LIMIT $limit;"
        };
        let mut request = self
            .db
            .query(query)
            .bind(("tenant", tenant.record_id()))
            .bind(("subjects", subjects))
            .bind(("limit", i64::try_from(limit).unwrap_or(i64::MAX)));
        if let Some(cursor) = cursor {
            request = request.bind(("cursor", cursor.record_id()));
        }
        let mut response = request.await?.check()?;
        response
            .take::<Vec<RecordId>>(0)?
            .into_iter()
            .map(|record| record_uuid(&record).map(ArtifactId::from_uuid))
            .collect()
    }

    pub async fn upsert_artifact_grant(&self, draft: ArtifactGrantDraft) -> Result<(), StoreError> {
        let id = deterministic_relation_id(
            "artifact-grant",
            draft.artifact_id.to_string(),
            format!("{:?}:{}", draft.subject_kind, draft.subject_key),
        );
        let content = ArtifactGrantEdge {
            id: id.clone(),
            r#in: draft.artifact_id.record_id(),
            out: draft.subject,
            subject_kind: draft.subject_kind,
            subject_key: draft.subject_key,
            permission: draft.permission,
            labels: draft.labels,
            expires_at: draft.expires_at,
            created_by: draft.created_by.record_id(),
            created_at: Utc::now(),
        };
        let outbox = OutboxDraft::now(
            None,
            "artifact",
            draft.artifact_id.to_string(),
            "artifact.grant.updated",
            1,
            OpenObject::new(BTreeMap::from([
                (
                    "subject_key".into(),
                    serde_json::json!(&content.subject_key),
                ),
                (
                    "permission".into(),
                    serde_json::to_value(content.permission).unwrap_or_default(),
                ),
            ])),
        );
        let mut response = self.db
            .query("BEGIN TRANSACTION; DELETE $record RETURN NONE; RELATE ONLY $artifact->$record->$subject CONTENT $content RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("record", id))
            .bind(("artifact", draft.artifact_id.record_id()))
            .bind(("subject", content.out.clone()))
            .bind(("content", content))
            .bind(("outbox", outbox))
            .await?;
        if let Some(error) = primary_transaction_error(response.take_errors()) {
            return Err(error.into());
        }
        Ok(())
    }

    pub async fn remove_artifact_grant(
        &self,
        artifact_id: ArtifactId,
        subject_kind: ArtifactGrantSubjectKind,
        subject_key: &str,
    ) -> Result<(), StoreError> {
        let id = deterministic_relation_id(
            "artifact-grant",
            artifact_id.to_string(),
            format!("{subject_kind:?}:{subject_key}"),
        );
        let outbox = OutboxDraft::now(
            None,
            "artifact",
            artifact_id.to_string(),
            "artifact.grant.removed",
            1,
            OpenObject::new(BTreeMap::from([(
                "subject_key".into(),
                serde_json::json!(subject_key),
            )])),
        );
        self.db
            .query("BEGIN TRANSACTION; LET $removed = (DELETE ONLY $record RETURN BEFORE); IF $removed != NONE { CREATE outbox_event CONTENT $outbox RETURN NONE; }; COMMIT TRANSACTION;")
            .bind(("record", id))
            .bind(("outbox", outbox))
            .await?
            .check()?;
        Ok(())
    }

    pub async fn set_artifact_release_state(
        &self,
        artifact_id: ArtifactId,
        state: ArtifactReleaseState,
    ) -> Result<Option<ArtifactOccurrenceRecord>, StoreError> {
        let outbox = OutboxDraft::now(
            None,
            "artifact",
            artifact_id.to_string(),
            "artifact.release_state.changed",
            1,
            OpenObject::new(BTreeMap::from([(
                "release_state".into(),
                serde_json::to_value(state).unwrap_or_default(),
            )])),
        );
        let mut response = self
            .db
            .query("BEGIN TRANSACTION; LET $updated = (UPDATE ONLY $artifact SET release_state = $state, updated_at = time::now() RETURN AFTER); IF $updated != NONE { CREATE outbox_event CONTENT $outbox RETURN NONE; }; RETURN $updated; COMMIT TRANSACTION;")
            .bind(("artifact", artifact_id.record_id()))
            .bind(("state", state))
            .bind(("outbox", outbox))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn create_artifact_write_capability(
        &self,
        draft: ArtifactWriteCapabilityDraft,
    ) -> Result<ArtifactWriteCapabilityRecord, StoreError> {
        let record = ArtifactWriteCapabilityRecord {
            id: draft.capability_id.record_id(),
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            tenant_key: draft.identity.tenant_key,
            owner_key: draft.identity.principal_key,
            owner_kind: draft.owner_kind,
            owner_issuer: draft.owner_issuer,
            owner_subject: draft.owner_subject,
            profile_key: draft.profile_key,
            server_key: draft.server_key,
            task_id: draft.task_id,
            token_hash: draft.token_hash,
            labels: draft.labels,
            max_artifact_count: draft.max_artifact_count,
            max_total_bytes: draft.max_total_bytes,
            used_artifact_count: 0,
            used_total_bytes: 0,
            expires_at: draft.expires_at,
            revoked_at: None,
            created_at: Utc::now(),
        };
        let mut response = self
            .db
            .query("CREATE ONLY $record CONTENT $content RETURN AFTER;")
            .bind(("record", draft.capability_id.record_id()))
            .bind(("content", record))
            .await?
            .check()?;
        response
            .take::<Option<ArtifactWriteCapabilityRecord>>(0)?
            .ok_or(StoreError::MissingRecord {
                operation: "artifact capability creation",
            })
    }

    /// Reserve quota and one occurrence identity for a retryable capability
    /// write. The quota increment and reservation/outbox record are atomic.
    /// Repeating an identical key returns the original reservation without
    /// incrementing counters again, including after capability expiry.
    #[allow(clippy::too_many_arguments)]
    pub async fn reserve_artifact_write_capability(
        &self,
        capability_id: ArtifactWriteCapabilityId,
        token_hash: &str,
        task_id: &str,
        idempotency_key: &str,
        request_hash: &str,
        byte_len: i64,
        requested_labels: &[String],
        proposed_artifact_id: ArtifactId,
    ) -> Result<ArtifactWriteReservation, StoreError> {
        self.authenticate_artifact_write_capability(
            capability_id,
            token_hash,
            task_id,
            requested_labels,
        )
        .await?;
        let redemption_id = artifact_write_redemption_id(capability_id, idempotency_key);
        if let Some(reservation) = self
            .artifact_write_reservation(capability_id, token_hash, redemption_id)
            .await?
        {
            validate_reservation_identity(&reservation, task_id, idempotency_key)?;
            let request_matches = reservation.redemption.request_hash == request_hash
                && reservation.redemption.byte_len == byte_len;
            if !request_matches
                && reservation.redemption.state == ArtifactWriteRedemptionState::Reserved
                && let Some(rebound) = self
                    .rebind_artifact_write_reservation(
                        &reservation,
                        token_hash,
                        request_hash,
                        byte_len,
                        requested_labels,
                    )
                    .await?
            {
                return Ok(rebound);
            }
            return Ok(ArtifactWriteReservation {
                request_matches,
                ..reservation
            });
        }

        let now = Utc::now();
        let redemption = ArtifactWriteRedemptionRecord {
            id: redemption_id.record_id(),
            capability: capability_id.record_id(),
            tenant: RecordId::new("tenant", "placeholder"),
            task: task_id
                .parse::<TaskId>()
                .map_err(|_| StoreError::ArtifactWriteConflict {
                    key: idempotency_key.to_owned(),
                })?
                .record_id(),
            task_id: task_id.to_owned(),
            idempotency_key: idempotency_key.to_owned(),
            request_hash: request_hash.to_owned(),
            byte_len,
            artifact: proposed_artifact_id.record_id(),
            state: ArtifactWriteRedemptionState::Reserved,
            reserved_at: now,
            finalized_at: None,
        };
        let outbox = OutboxDraft::now(
            None,
            "artifact_write_redemption",
            redemption_id.to_string(),
            "artifact.write.reserved",
            1,
            OpenObject::new(BTreeMap::from([
                ("task_id".into(), serde_json::json!(task_id)),
                (
                    "artifact_id".into(),
                    serde_json::json!(proposed_artifact_id.to_string()),
                ),
            ])),
        );
        let result = self
            .db
            .query(
                "BEGIN TRANSACTION; LET $capability_rows = (UPDATE $capability SET used_artifact_count += 1, used_total_bytes += $byte_len WHERE token_hash = $token_hash AND task_id = $task_id AND labels CONTAINSALL $requested_labels AND revoked_at = NONE AND expires_at > $now AND used_artifact_count < max_artifact_count AND used_total_bytes + $byte_len <= max_total_bytes RETURN AFTER); LET $capability_row = array::first($capability_rows); IF $capability_row = NONE { THROW 'artifact write capability denied'; }; CREATE ONLY $redemption CONTENT { capability: $capability, tenant: $capability_row.tenant, task: $task, task_id: $task_id, idempotency_key: $idempotency_key, request_hash: $request_hash, byte_len: $byte_len, artifact: $artifact, state: 'reserved', reserved_at: $now, finalized_at: NONE } RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;",
            )
            .bind(("capability", capability_id.record_id()))
            .bind(("token_hash", token_hash.to_owned()))
            .bind(("task_id", task_id.to_owned()))
            .bind(("task", redemption.task.clone()))
            .bind(("requested_labels", requested_labels.to_vec()))
            .bind(("byte_len", byte_len))
            .bind(("now", now))
            .bind(("redemption", redemption_id.record_id()))
            .bind(("idempotency_key", idempotency_key.to_owned()))
            .bind(("request_hash", request_hash.to_owned()))
            .bind(("artifact", proposed_artifact_id.record_id()))
            .bind(("outbox", outbox))
            .await
            .and_then(|mut response| match primary_transaction_error(response.take_errors()) {
                Some(error) => Err(error),
                None => Ok(()),
            });
        if let Err(error) = result {
            if let Some(reservation) = self
                .artifact_write_reservation(capability_id, token_hash, redemption_id)
                .await?
            {
                validate_reservation_identity(&reservation, task_id, idempotency_key)?;
                let request_matches = reservation.redemption.request_hash == request_hash
                    && reservation.redemption.byte_len == byte_len;
                if !request_matches
                    && reservation.redemption.state == ArtifactWriteRedemptionState::Reserved
                    && let Some(rebound) = self
                        .rebind_artifact_write_reservation(
                            &reservation,
                            token_hash,
                            request_hash,
                            byte_len,
                            requested_labels,
                        )
                        .await?
                {
                    return Ok(rebound);
                }
                return Ok(ArtifactWriteReservation {
                    request_matches,
                    ..reservation
                });
            }
            if error
                .to_string()
                .contains("artifact write capability denied")
            {
                return Err(StoreError::ArtifactWriteDenied);
            }
            return Err(error.into());
        }
        self.artifact_write_reservation(capability_id, token_hash, redemption_id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "artifact write reservation readback",
            })
    }

    /// Finalize a reserved write after its occurrence is durable. Returns
    /// `true` only for the first successful finalization.
    pub async fn finalize_artifact_write_capability(
        &self,
        redemption_id: ArtifactWriteRedemptionId,
        artifact_id: ArtifactId,
    ) -> Result<bool, StoreError> {
        let now = Utc::now();
        let outbox = OutboxDraft::now(
            None,
            "artifact_write_redemption",
            redemption_id.to_string(),
            "artifact.write.finalized",
            1,
            OpenObject::new(BTreeMap::from([(
                "artifact_id".into(),
                serde_json::json!(artifact_id.to_string()),
            )])),
        );
        self
            .db
            .query("BEGIN TRANSACTION; LET $updated_rows = (UPDATE $redemption SET state = 'finalized', finalized_at = $now WHERE state = 'reserved' AND artifact = $artifact RETURN AFTER); LET $updated = array::first($updated_rows); IF $updated != NONE { UPDATE ONLY $artifact SET task = $updated.task RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; }; COMMIT TRANSACTION;")
            .bind(("redemption", redemption_id.record_id()))
            .bind(("artifact", artifact_id.record_id()))
            .bind(("now", now))
            .bind(("outbox", outbox))
            .await?
            .check()?;
        let mut response = self
            .db
            .query("SELECT * FROM ONLY $redemption;")
            .bind(("redemption", redemption_id.record_id()))
            .await?
            .check()?;
        Ok(response
            .take::<Option<ArtifactWriteRedemptionRecord>>(0)?
            .is_some_and(|redemption| redemption.finalized_at == Some(now)))
    }

    async fn artifact_write_reservation(
        &self,
        capability_id: ArtifactWriteCapabilityId,
        token_hash: &str,
        redemption_id: ArtifactWriteRedemptionId,
    ) -> Result<Option<ArtifactWriteReservation>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM ONLY $redemption; SELECT * FROM ONLY $capability WHERE token_hash = $token_hash;")
            .bind(("redemption", redemption_id.record_id()))
            .bind(("capability", capability_id.record_id()))
            .bind(("token_hash", token_hash.to_owned()))
            .await?
            .check()?;
        let redemption: Option<ArtifactWriteRedemptionRecord> = response.take(0)?;
        let Some(redemption) = redemption else {
            return Ok(None);
        };
        let capability = response
            .take::<Option<ArtifactWriteCapabilityRecord>>(1)?
            .ok_or(StoreError::ArtifactWriteDenied)?;
        Ok(Some(ArtifactWriteReservation {
            capability,
            redemption,
            request_matches: true,
        }))
    }

    async fn authenticate_artifact_write_capability(
        &self,
        capability_id: ArtifactWriteCapabilityId,
        token_hash: &str,
        task_id: &str,
        requested_labels: &[String],
    ) -> Result<ArtifactWriteCapabilityRecord, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM ONLY $capability WHERE token_hash = $token_hash;")
            .bind(("capability", capability_id.record_id()))
            .bind(("token_hash", token_hash.to_owned()))
            .await?
            .check()?;
        let capability = response
            .take::<Option<ArtifactWriteCapabilityRecord>>(0)?
            .ok_or(StoreError::ArtifactWriteDenied)?;
        let task_matches = capability.task_id == task_id;
        let labels_allowed = requested_labels
            .iter()
            .all(|label| capability.labels.contains(label));
        if !task_matches || !labels_allowed {
            return Err(StoreError::ArtifactWriteDenied);
        }
        Ok(capability)
    }

    async fn rebind_artifact_write_reservation(
        &self,
        reservation: &ArtifactWriteReservation,
        token_hash: &str,
        request_hash: &str,
        byte_len: i64,
        requested_labels: &[String],
    ) -> Result<Option<ArtifactWriteReservation>, StoreError> {
        let outbox = OutboxDraft::now(
            Some(reservation.redemption.tenant.clone()),
            "artifact_write_redemption",
            record_uuid(&reservation.redemption.id)?.to_string(),
            "artifact.write.reservation_rebound",
            1,
            OpenObject::new(BTreeMap::from([(
                "artifact_id".into(),
                serde_json::json!(record_uuid(&reservation.redemption.artifact)?.to_string()),
            )])),
        );
        let result = self
            .db
            .query("BEGIN TRANSACTION; LET $current_rows = (SELECT * FROM $redemption WHERE state = 'reserved' AND request_hash = $expected_hash AND byte_len = $expected_bytes); LET $current = array::first($current_rows); IF $current = NONE { THROW 'artifact write reservation changed'; }; LET $occurrences = (SELECT * FROM $artifact); IF array::len($occurrences) > 0 { THROW 'artifact write occurrence already staged'; }; LET $capability_rows = (UPDATE $capability SET used_total_bytes = used_total_bytes - $expected_bytes + $byte_len WHERE token_hash = $token_hash AND task_id = $task_id AND labels CONTAINSALL $requested_labels AND revoked_at = NONE AND used_total_bytes - $expected_bytes + $byte_len <= max_total_bytes RETURN AFTER); LET $capability_row = array::first($capability_rows); IF $capability_row = NONE { THROW 'artifact write capability denied'; }; UPDATE ONLY $redemption SET request_hash = $request_hash, byte_len = $byte_len RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("redemption", reservation.redemption.id.clone()))
            .bind(("expected_hash", reservation.redemption.request_hash.clone()))
            .bind(("expected_bytes", reservation.redemption.byte_len))
            .bind(("artifact", reservation.redemption.artifact.clone()))
            .bind(("capability", reservation.capability.id.clone()))
            .bind(("token_hash", token_hash.to_owned()))
            .bind(("task_id", reservation.redemption.task_id.clone()))
            .bind(("requested_labels", requested_labels.to_vec()))
            .bind(("request_hash", request_hash.to_owned()))
            .bind(("byte_len", byte_len))
            .bind(("outbox", outbox))
            .await
            .and_then(|mut response| match primary_transaction_error(response.take_errors()) {
                Some(error) => Err(error),
                None => Ok(()),
            });
        if let Err(error) = result {
            let message = error.to_string();
            if message.contains("artifact write capability denied") {
                return Err(StoreError::ArtifactWriteDenied);
            }
            if message.contains("artifact write occurrence already staged")
                || message.contains("artifact write reservation changed")
            {
                return Ok(None);
            }
            return Err(error.into());
        }
        let redemption_id =
            ArtifactWriteRedemptionId::from_uuid(record_uuid(&reservation.redemption.id)?);
        Ok(self
            .artifact_write_reservation(
                ArtifactWriteCapabilityId::from_uuid(record_uuid(&reservation.capability.id)?),
                token_hash,
                redemption_id,
            )
            .await?
            .map(|reservation| ArtifactWriteReservation {
                request_matches: true,
                ..reservation
            }))
    }

    pub async fn create_artifact_share_link(
        &self,
        draft: ArtifactShareLinkDraft,
    ) -> Result<ShareLinkRecord, StoreError> {
        let record = ShareLinkRecord {
            id: draft.link_id.record_id(),
            tenant: draft.identity.tenant_id.record_id(),
            artifact: draft.artifact_id.record_id(),
            created_by: draft.identity.principal_id.record_id(),
            token_hash: draft.token_hash,
            permission: GrantPermission::Read,
            expires_at: draft.expires_at,
            max_downloads: draft.max_downloads,
            download_count: 0,
            revoked_at: None,
            created_at: Utc::now(),
        };
        let outbox = OutboxDraft::now(
            Some(draft.identity.tenant_id.record_id()),
            "artifact",
            draft.artifact_id.to_string(),
            "artifact.share_link.created",
            1,
            OpenObject::new(BTreeMap::from([(
                "link_id".into(),
                serde_json::json!(draft.link_id.to_string()),
            )])),
        );
        let mut response = self
            .db
            .query("BEGIN TRANSACTION; CREATE ONLY $record CONTENT $content RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("record", draft.link_id.record_id()))
            .bind(("content", record))
            .bind(("outbox", outbox))
            .await?;
        if let Some(error) = primary_transaction_error(response.take_errors()) {
            return Err(error.into());
        }
        let mut response = self
            .db
            .query("SELECT * FROM ONLY $record;")
            .bind(("record", draft.link_id.record_id()))
            .await?
            .check()?;
        response
            .take::<Option<ShareLinkRecord>>(0)?
            .ok_or(StoreError::MissingRecord {
                operation: "artifact share link creation",
            })
    }

    pub async fn revoke_artifact_share_link(
        &self,
        link_id: ShareLinkId,
        artifact_id: ArtifactId,
    ) -> Result<bool, StoreError> {
        let outbox = OutboxDraft::now(
            None,
            "artifact",
            artifact_id.to_string(),
            "artifact.share_link.revoked",
            1,
            OpenObject::new(BTreeMap::from([(
                "link_id".into(),
                serde_json::json!(link_id.to_string()),
            )])),
        );
        let mut response = self
            .db
            .query("BEGIN TRANSACTION; LET $updated = (UPDATE ONLY $record SET revoked_at = time::now() WHERE artifact = $artifact AND revoked_at = NONE RETURN AFTER); IF $updated != NONE { CREATE outbox_event CONTENT $outbox RETURN NONE; }; RETURN $updated; COMMIT TRANSACTION;")
            .bind(("record", link_id.record_id()))
            .bind(("artifact", artifact_id.record_id()))
            .bind(("outbox", outbox))
            .await?
            .check()?;
        Ok(response.take::<Option<ShareLinkRecord>>(0)?.is_some())
    }

    pub async fn redeem_public_share_link(
        &self,
        token_hash: &str,
    ) -> Result<Option<PublicShareRedemption>, StoreError> {
        let mut response = self
            .db
            .query("UPDATE share_link SET download_count += 1 WHERE token_hash = $token_hash AND permission = 'read' AND revoked_at = NONE AND expires_at > time::now() AND (max_downloads = NONE OR download_count < max_downloads) AND artifact.release_state IN ['releasable', 'released'] AND (artifact.retention_expires_at = NONE OR artifact.retention_expires_at > time::now()) RETURN AFTER;")
            .bind(("token_hash", token_hash.to_string()))
            .await?
            .check()?;
        let records: Vec<ShareLinkRecord> = response.take(0)?;
        let Some(link) = records.into_iter().next() else {
            return Ok(None);
        };
        let artifact_id = record_uuid(&link.artifact).map(ArtifactId::from_uuid)?;
        Ok(Some(PublicShareRedemption { link, artifact_id }))
    }

    pub async fn append_artifact_audit(&self, draft: ArtifactAuditDraft) -> Result<(), StoreError> {
        let id = AuditEventId::new();
        let record = AuditEventRecord {
            id: id.record_id(),
            tenant: draft.tenant.map(TenantId::record_id),
            actor: draft.actor.map(PrincipalId::record_id),
            action: draft.action.clone(),
            resource_type: "artifact".into(),
            resource_id: draft.resource_id,
            outcome: draft.outcome,
            request_id: None,
            trace_id: None,
            source_ip: None,
            details: OpenObject::new(draft.details),
            occurred_at: Utc::now(),
            search_text: draft.action,
        };
        let outbox = OutboxDraft::now(
            record.tenant.clone(),
            "audit",
            id.to_string(),
            "audit.recorded",
            1,
            OpenObject::new(BTreeMap::from([
                ("action".into(), serde_json::json!(&record.action)),
                ("resource_id".into(), serde_json::json!(&record.resource_id)),
            ])),
        );
        self.db
            .query("BEGIN TRANSACTION; CREATE ONLY $record CONTENT $content RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("record", id.record_id()))
            .bind(("content", record))
            .bind(("outbox", outbox))
            .await?
            .check()?;
        Ok(())
    }
}

fn deterministic_relation_id(prefix: &str, left: String, right: String) -> RecordId {
    let id = Uuid::new_v5(
        &PLATFORM_ID_NAMESPACE,
        format!("{prefix}:{left}:{right}").as_bytes(),
    );
    RecordId::new("artifact_grant", surrealdb::types::Uuid::from(id))
}

fn artifact_write_redemption_id(
    capability_id: ArtifactWriteCapabilityId,
    idempotency_key: &str,
) -> ArtifactWriteRedemptionId {
    ArtifactWriteRedemptionId::from_uuid(Uuid::new_v5(
        &PLATFORM_ID_NAMESPACE,
        format!("artifact-write-redemption:{capability_id}:{idempotency_key}").as_bytes(),
    ))
}

fn validate_reservation_identity(
    reservation: &ArtifactWriteReservation,
    task_id: &str,
    idempotency_key: &str,
) -> Result<(), StoreError> {
    let valid = reservation.redemption.capability == reservation.capability.id
        && reservation.redemption.task_id == task_id
        && reservation.redemption.idempotency_key == idempotency_key;
    if valid {
        Ok(())
    } else {
        Err(StoreError::ArtifactWriteConflict {
            key: idempotency_key.to_owned(),
        })
    }
}

fn record_uuid(record: &RecordId) -> Result<Uuid, StoreError> {
    match &record.key {
        RecordIdKey::Uuid(value) => Ok(**value),
        _ => Err(StoreError::MissingRecord {
            operation: "record UUID decoding",
        }),
    }
}
