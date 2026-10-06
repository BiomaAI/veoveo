pub(crate) mod publication;
mod reads;
pub use reads::ArtifactReadScope;

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use surrealdb::types::{RecordId, RecordIdKey, SurrealValue};
use uuid::Uuid;

use crate::{
    ArtifactBlobId, ArtifactBlobRecord, ArtifactGrantEdge, ArtifactGrantSubjectKind, ArtifactId,
    ArtifactOccurrenceRecord, ArtifactReleaseState, ArtifactWriteCapabilityId,
    ArtifactWriteCapabilityRecord, ArtifactWriteRedemptionId, ArtifactWriteRedemptionRecord,
    ArtifactWriteRedemptionState, GrantPermission, InvocationAuthorityRecord, OpenObject,
    PlatformIdentity, PlatformStore, PrincipalId, PrincipalKind, ShareLinkId, ShareLinkRecord,
    StoreError, TenantId, TenantRecord, deterministic_principal_id, deterministic_work_context_id,
};
use veoveo_types::TaskId;

use crate::identity::PLATFORM_ID_NAMESPACE;
use crate::store::primary_transaction_error;

#[derive(Clone, Debug)]
pub struct ArtifactOccurrenceDraft {
    pub artifact_id: ArtifactId,
    pub identity: PlatformIdentity,
    pub authority: InvocationAuthorityRecord,
    pub owner: RecordId,
    pub initial_grants: Vec<ArtifactGrantDraft>,
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

#[derive(Clone, Debug, SurrealValue)]
pub struct ArtifactAggregate {
    pub occurrence: ArtifactOccurrenceRecord,
    pub blob: ArtifactBlobRecord,
    pub tenant: TenantRecord,
    pub grants: Vec<ArtifactGrantEdge>,
}

#[derive(Clone, Debug)]
pub struct ArtifactWriteCapabilityDraft {
    pub audit: crate::audit::AuditContextRecord,
    pub capability_id: ArtifactWriteCapabilityId,
    pub identity: PlatformIdentity,
    pub authority: InvocationAuthorityRecord,
    pub profile_key: String,
    pub server_key: String,
    pub task_id: veoveo_artifact_contract::ArtifactTaskId,
    pub actor_kind: PrincipalKind,
    pub actor_issuer: String,
    pub actor_subject: String,
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

impl PlatformStore {
    pub async fn create_artifact_occurrence(
        &self,
        draft: ArtifactOccurrenceDraft,
    ) -> Result<ArtifactAggregate, StoreError> {
        let artifact_id = draft.artifact_id;
        let tenant_id = draft.identity.tenant_id;
        let publication::PreparedPublication {
            blob,
            occurrence,
            grants,
        } = publication::prepare_publication(draft)?;
        // Equal concurrent writers keep the first object mapping. Retrying a
        // transaction conflict never rewrites the retained blob or its metadata.
        for attempt in 0..8_u32 {
            let mut response = self
                .db
                .query(include_str!(
                    "queries/artifacts/create_artifact_occurrence.surql"
                ))
                .bind(("blob", blob.id.clone()))
                .bind(("storage_usage", crate::artifact_storage_usage_id(tenant_id)))
                .bind(("blob_content", blob.clone()))
                .bind(("artifact", artifact_id.record_id()))
                .bind(("artifact_content", occurrence.clone()))
                .bind(("grants", grants.clone()))
                .await?;
            let Some(error) = primary_transaction_error(response.take_errors()) else {
                break;
            };
            if error.is_thrown() && error.message().contains("artifact_blob_integrity_conflict") {
                return Err(StoreError::ArtifactBlobIntegrityConflict);
            }
            if attempt < 7
                && (matches!(
                    error.query_details(),
                    Some(surrealdb::types::QueryError::TransactionConflict)
                ) || error.message().starts_with("Transaction conflict:")
                    || error
                        .message()
                        .contains("not executed due to a failed transaction"))
            {
                tokio::time::sleep(std::time::Duration::from_millis(1_u64 << attempt)).await;
                continue;
            }
            return Err(error.into());
        }
        self.artifact_aggregate(artifact_id)
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
            .query(include_str!("queries/artifacts/artifact_aggregate.surql"))
            .bind(("artifact", artifact_id.record_id()))
            .await?
            .check()?;
        let occurrence: Option<ArtifactOccurrenceRecord> = response.take(0)?;
        let Some(occurrence) = occurrence else {
            return Ok(None);
        };
        self.artifact_aggregate_from_occurrence(occurrence)
            .await
            .map(Some)
    }

    async fn artifact_aggregate_from_occurrence(
        &self,
        occurrence: ArtifactOccurrenceRecord,
    ) -> Result<ArtifactAggregate, StoreError> {
        let mut response = self
            .db
            .query(include_str!(
                "queries/artifacts/artifact_aggregate_from_occurrence.surql"
            ))
            .bind(("blob", occurrence.blob.clone()))
            .bind(("tenant", occurrence.tenant.clone()))
            .bind(("artifact", occurrence.id.clone()))
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
        let grants = response.take(2)?;
        Ok(ArtifactAggregate {
            occurrence,
            blob,
            tenant,
            grants,
        })
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

        let mut response = self
            .db
            .query(include_str!(
                "queries/artifacts/upsert_artifact_grant.surql"
            ))
            .bind(("record", id))
            .bind(("artifact", draft.artifact_id.record_id()))
            .bind(("subject", content.out.clone()))
            .bind(("content", content))
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

        self.db
            .query(include_str!(
                "queries/artifacts/remove_artifact_grant.surql"
            ))
            .bind(("record", id))
            .await?
            .check()?;
        Ok(())
    }

    pub async fn set_artifact_release_state(
        &self,
        artifact_id: ArtifactId,
        state: ArtifactReleaseState,
    ) -> Result<Option<ArtifactOccurrenceRecord>, StoreError> {
        let mut response = self
            .db
            .query(include_str!(
                "queries/artifacts/set_artifact_release_state.surql"
            ))
            .bind(("artifact", artifact_id.record_id()))
            .bind(("state", state))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn create_artifact_write_capability(
        &self,
        draft: ArtifactWriteCapabilityDraft,
    ) -> Result<ArtifactWriteCapabilityRecord, StoreError> {
        let record = ArtifactWriteCapabilityRecord {
            audit: draft.audit,
            id: draft.capability_id.record_id(),
            tenant: draft.identity.tenant_id.record_id(),
            actor: draft.identity.principal_id.record_id(),
            work_context: deterministic_work_context_id(
                &draft.identity.tenant_key,
                &draft.authority.context_key,
            )?
            .record_id(),
            authority: draft.authority,
            tenant_key: draft.identity.tenant_key,
            actor_key: draft.identity.principal_key,
            actor_kind: draft.actor_kind,
            actor_issuer: draft.actor_issuer,
            actor_subject: draft.actor_subject,
            profile_key: draft.profile_key,
            server_key: draft.server_key,
            task_id: draft.task_id.to_string(),
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
            .query(include_str!(
                "queries/artifacts/create_artifact_write_capability.surql"
            ))
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
    /// write. The quota increment and reservation record are atomic.
    /// Repeating an identical key returns the original reservation without
    /// incrementing counters again, including after capability expiry.
    #[allow(clippy::too_many_arguments)]
    pub async fn reserve_artifact_write_capability(
        &self,
        capability_id: ArtifactWriteCapabilityId,
        token_hash: &str,
        task_id: &veoveo_artifact_contract::ArtifactTaskId,
        idempotency_key: &str,
        request_hash: &str,
        byte_len: i64,
        requested_labels: &[String],
        proposed_artifact_id: ArtifactId,
    ) -> Result<ArtifactWriteReservation, StoreError> {
        let binding = ArtifactWriteBinding {
            capability_id,
            task_id,
            idempotency_key,
        };
        self.authenticate_artifact_write_capability(
            capability_id,
            token_hash,
            task_id,
            requested_labels,
        )
        .await?;
        let redemption_id = artifact_write_redemption_id(capability_id, idempotency_key);
        if let Some(reservation) = self.artifact_write_reservation(binding, token_hash).await? {
            let request_matches = reservation.redemption.request_hash == request_hash
                && reservation.redemption.byte_len == byte_len;
            if !request_matches
                && reservation.redemption.state == ArtifactWriteRedemptionState::Reserved
                && let Some(rebound) = self
                    .rebind_artifact_write_reservation(
                        &reservation,
                        binding,
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
            task: crate::task_record_id(TaskId::from_uuid(task_id.as_uuid())),
            task_id: task_id.to_string(),
            idempotency_key: idempotency_key.to_owned(),
            request_hash: request_hash.to_owned(),
            byte_len,
            artifact: proposed_artifact_id.record_id(),
            state: ArtifactWriteRedemptionState::Reserved,
            reserved_at: now,
            finalized_at: None,
        };

        let result = self
            .db
            .query(include_str!(
                "queries/artifacts/reserve_artifact_write_capability.surql"
            ))
            .bind(("capability", capability_id.record_id()))
            .bind(("token_hash", token_hash.to_owned()))
            .bind(("task_id", task_id.to_string()))
            .bind(("task", redemption.task.clone()))
            .bind(("requested_labels", requested_labels.to_vec()))
            .bind(("byte_len", byte_len))
            .bind(("now", now))
            .bind(("redemption", redemption_id.record_id()))
            .bind(("idempotency_key", idempotency_key.to_owned()))
            .bind(("request_hash", request_hash.to_owned()))
            .bind(("artifact", proposed_artifact_id.record_id()))
            .await
            .and_then(
                |mut response| match primary_transaction_error(response.take_errors()) {
                    Some(error) => Err(error),
                    None => Ok(()),
                },
            );
        if let Err(error) = result {
            if let Some(reservation) = self.artifact_write_reservation(binding, token_hash).await? {
                let request_matches = reservation.redemption.request_hash == request_hash
                    && reservation.redemption.byte_len == byte_len;
                if !request_matches
                    && reservation.redemption.state == ArtifactWriteRedemptionState::Reserved
                    && let Some(rebound) = self
                        .rebind_artifact_write_reservation(
                            &reservation,
                            binding,
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
        self.artifact_write_reservation(binding, token_hash)
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

        self.db
            .query(include_str!(
                "queries/artifacts/finalize_artifact_write_capability.surql"
            ))
            .bind(("redemption", redemption_id.record_id()))
            .bind(("artifact", artifact_id.record_id()))
            .bind(("now", now))
            .await?
            .check()?;
        let mut response = self
            .db
            .query(include_str!(
                "queries/artifacts/finalize_artifact_write_capability_2.surql"
            ))
            .bind(("redemption", redemption_id.record_id()))
            .await?
            .check()?;
        Ok(response
            .take::<Option<ArtifactWriteRedemptionRecord>>(0)?
            .is_some_and(|redemption| redemption.finalized_at == Some(now)))
    }

    async fn artifact_write_reservation(
        &self,
        binding: ArtifactWriteBinding<'_>,
        token_hash: &str,
    ) -> Result<Option<ArtifactWriteReservation>, StoreError> {
        let redemption_id =
            artifact_write_redemption_id(binding.capability_id, binding.idempotency_key);
        let mut response = self
            .db
            .query(include_str!(
                "queries/artifacts/artifact_write_reservation.surql"
            ))
            .bind(("redemption", redemption_id.record_id()))
            .bind(("capability", binding.capability_id.record_id()))
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
        let reservation = ArtifactWriteReservation {
            capability,
            redemption,
            request_matches: true,
        };
        validate_reservation_identity(&reservation, binding)?;
        Ok(Some(reservation))
    }

    async fn authenticate_artifact_write_capability(
        &self,
        capability_id: ArtifactWriteCapabilityId,
        token_hash: &str,
        task_id: &veoveo_artifact_contract::ArtifactTaskId,
        requested_labels: &[String],
    ) -> Result<ArtifactWriteCapabilityRecord, StoreError> {
        let mut response = self
            .db
            .query(include_str!(
                "queries/artifacts/authenticate_artifact_write_capability.surql"
            ))
            .bind(("capability", capability_id.record_id()))
            .bind(("token_hash", token_hash.to_owned()))
            .await?
            .check()?;
        let capability = response
            .take::<Option<ArtifactWriteCapabilityRecord>>(0)?
            .ok_or(StoreError::ArtifactWriteDenied)?;
        let task_matches = retained_artifact_task_id(&capability.task_id)? == *task_id;
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
        binding: ArtifactWriteBinding<'_>,
        token_hash: &str,
        request_hash: &str,
        byte_len: i64,
        requested_labels: &[String],
    ) -> Result<Option<ArtifactWriteReservation>, StoreError> {
        let result = self
            .db
            .query(include_str!(
                "queries/artifacts/rebind_artifact_write_reservation.surql"
            ))
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
            .await
            .and_then(
                |mut response| match primary_transaction_error(response.take_errors()) {
                    Some(error) => Err(error),
                    None => Ok(()),
                },
            );
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
        Ok(self
            .artifact_write_reservation(binding, token_hash)
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

        let mut response = self
            .db
            .query(include_str!(
                "queries/artifacts/create_artifact_share_link.surql"
            ))
            .bind(("record", draft.link_id.record_id()))
            .bind(("content", record))
            .await?;
        if let Some(error) = primary_transaction_error(response.take_errors()) {
            return Err(error.into());
        }
        let mut response = self
            .db
            .query(include_str!(
                "queries/artifacts/create_artifact_share_link_2.surql"
            ))
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
        let mut response = self
            .db
            .query(include_str!(
                "queries/artifacts/revoke_artifact_share_link.surql"
            ))
            .bind(("record", link_id.record_id()))
            .bind(("artifact", artifact_id.record_id()))
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
            .query(include_str!(
                "queries/artifacts/redeem_public_share_link.surql"
            ))
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

/// Current-format retained bindings are canonical. An older alias row requires the installation drain.
fn retained_artifact_task_id(
    value: &str,
) -> Result<veoveo_artifact_contract::ArtifactTaskId, StoreError> {
    let id = veoveo_artifact_contract::ArtifactTaskId::parse(value)
        .map_err(|_| StoreError::ArtifactWriteDenied)?;
    if id.to_string() != value {
        return Err(StoreError::ArtifactWriteDenied);
    }
    Ok(id)
}

#[derive(Clone, Copy)]
struct ArtifactWriteBinding<'a> {
    capability_id: ArtifactWriteCapabilityId,
    task_id: &'a veoveo_artifact_contract::ArtifactTaskId,
    idempotency_key: &'a str,
}

fn validate_reservation_identity(
    reservation: &ArtifactWriteReservation,
    binding: ArtifactWriteBinding<'_>,
) -> Result<(), StoreError> {
    let valid = reservation.capability.id == binding.capability_id.record_id()
        && reservation.redemption.id
            == artifact_write_redemption_id(binding.capability_id, binding.idempotency_key)
                .record_id()
        && reservation.redemption.capability == reservation.capability.id
        && retained_artifact_task_id(&reservation.capability.task_id)? == *binding.task_id
        && retained_artifact_task_id(&reservation.redemption.task_id)? == *binding.task_id
        && reservation.redemption.task
            == crate::task_record_id(TaskId::from_uuid(binding.task_id.as_uuid()))
        && reservation.redemption.idempotency_key == binding.idempotency_key;
    if valid {
        Ok(())
    } else {
        Err(StoreError::ArtifactWriteConflict {
            key: binding.idempotency_key.to_owned(),
        })
    }
}

pub(crate) fn record_uuid(record: &RecordId) -> Result<Uuid, StoreError> {
    match &record.key {
        RecordIdKey::Uuid(value) => Ok(**value),
        _ => Err(StoreError::MissingRecord {
            operation: "record UUID decoding",
        }),
    }
}
