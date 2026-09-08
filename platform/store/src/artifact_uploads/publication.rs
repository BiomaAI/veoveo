//! An upload receipt and governed occurrence become durable in one transaction.

use super::*;
use crate::{
    ArtifactGrantDraft, ArtifactGrantSubjectKind, ArtifactId, ArtifactOccurrenceDraft,
    AuditEventId, AuditEventRecord, AuditOutcome, GrantPermission, OpenObject, OutboxDraft,
    PlatformIdentity, PrincipalId, TenantId,
};
use chrono::Utc;
use std::collections::BTreeMap;
use surrealdb::types::RecordId;

impl PlatformStore {
    pub async fn publish_artifact_upload(
        &self,
        fence: &ArtifactUploadFence,
        sha256: &str,
        byte_len: i64,
    ) -> Result<ArtifactUploadRecord, StoreError> {
        if !lifecycle::valid_digest(sha256) || byte_len < 0 {
            return Err(StoreError::ArtifactUpload(
                ArtifactUploadRejection::Integrity,
            ));
        }
        let upload = self.required_upload(fence.upload_id).await?;
        let draft = upload_publication(&upload, sha256, byte_len)?;
        let publication = crate::artifacts::publication::prepare_publication(draft)?;
        let audit_id = AuditEventId::new();
        let audit = AuditEventRecord {
            id: audit_id.record_id(),
            tenant: Some(upload.tenant.clone()),
            actor: Some(upload.actor.clone()),
            action: "artifact.upload.completed".into(),
            resource_type: "artifact".into(),
            resource_id: Some(crate::artifacts::record_uuid(&upload.artifact)?.to_string()),
            outcome: AuditOutcome::Succeeded,
            request_id: Some(upload.request_id.to_string()),
            trace_id: None,
            source_ip: None,
            details: OpenObject::new(BTreeMap::from([
                (
                    "upload_id".into(),
                    serde_json::json!(fence.upload_id.to_string()),
                ),
                ("byte_len".into(), serde_json::json!(byte_len)),
            ])),
            occurred_at: Utc::now(),
            search_text: "artifact.upload.completed".into(),
        };
        let audit_outbox = OutboxDraft::now(
            Some(upload.tenant.clone()),
            "audit",
            audit_id.to_string(),
            "audit.recorded",
            1,
            OpenObject::new(BTreeMap::from([
                ("action".into(), serde_json::json!(&audit.action)),
                ("resource_id".into(), serde_json::json!(&audit.resource_id)),
            ])),
        );
        for attempt in 0..8_u32 {
            let mut response = self
                .db
                .query(concat!(
                    include_str!("publish_begin.surql"),
                    include_str!("../artifacts/register.surql"),
                    include_str!("publish_finish.surql")
                ))
                .bind(("upload", upload.id.clone()))
                .bind(("owner", fence.owner))
                .bind(("generation", fence.generation))
                .bind(("now", Utc::now()))
                .bind(("blob", publication.blob.id.clone()))
                .bind(("blob_content", publication.blob.clone()))
                .bind(("artifact", upload.artifact.clone()))
                .bind(("artifact_content", publication.occurrence.clone()))
                .bind(("grants", publication.grants.clone()))
                .bind(("outbox", publication.outbox.clone()))
                .bind((
                    "storage_usage",
                    RecordId::new("artifact_storage_usage", upload.tenant.key.clone()),
                ))
                .bind(("audit", audit.clone()))
                .bind(("audit_outbox", audit_outbox.clone()))
                .await?;
            let Some(error) = crate::store::primary_transaction_error(response.take_errors())
            else {
                return self.required_upload(fence.upload_id).await;
            };
            if error.is_thrown() && error.message().contains("artifact_blob_integrity_conflict") {
                return Err(StoreError::ArtifactBlobIntegrityConflict);
            }
            if attempt < 7 && retryable(&error) {
                tokio::time::sleep(std::time::Duration::from_millis(1_u64 << attempt)).await;
                continue;
            }
            return Err(upload_error(error));
        }
        unreachable!("bounded transaction retry")
    }
}

/// Ownership and grants come exclusively from the admitted Work Context.
fn upload_publication(
    upload: &ArtifactUploadRecord,
    sha256: &str,
    byte_len: i64,
) -> Result<ArtifactOccurrenceDraft, StoreError> {
    let artifact_id = ArtifactId::from_uuid(crate::artifacts::record_uuid(&upload.artifact)?);
    let principal_id = PrincipalId::from_uuid(crate::artifacts::record_uuid(&upload.actor)?);
    let identity = PlatformIdentity {
        tenant_id: TenantId::from_uuid(crate::artifacts::record_uuid(&upload.tenant)?),
        principal_id,
        tenant_key: upload.tenant_key.clone(),
        principal_key: upload.actor_key.clone(),
    };
    let authority = &upload.authority;
    let subject = |kind, key: &str| match kind {
        ArtifactGrantSubjectKind::Principal => {
            crate::deterministic_principal_id(&upload.tenant_key, key).map(|id| id.record_id())
        }
        ArtifactGrantSubjectKind::Group => {
            crate::deterministic_group_id(&upload.tenant_key, key).map(|id| id.record_id())
        }
    };
    let owner = subject(authority.owner_kind, &authority.owner_key)?;
    let mut grants = vec![ArtifactGrantDraft {
        artifact_id,
        subject: owner.clone(),
        subject_kind: authority.owner_kind,
        subject_key: authority.owner_key.clone(),
        permission: GrantPermission::Admin,
        labels: authority.data_labels.clone(),
        expires_at: None,
        created_by: principal_id,
    }];
    for initial in &authority.initial_grants {
        if let Some(existing) = grants.iter_mut().find(|grant| {
            grant.subject_kind == initial.subject_kind && grant.subject_key == initial.subject_key
        }) {
            existing.permission = strongest(existing.permission, initial.permission);
        } else {
            grants.push(ArtifactGrantDraft {
                artifact_id,
                subject: subject(initial.subject_kind, &initial.subject_key)?,
                subject_kind: initial.subject_kind,
                subject_key: initial.subject_key.clone(),
                permission: initial.permission,
                labels: authority.data_labels.clone(),
                expires_at: None,
                created_by: principal_id,
            });
        }
    }
    Ok(ArtifactOccurrenceDraft {
        artifact_id,
        identity,
        authority: authority.clone(),
        owner,
        initial_grants: grants,
        sha256: sha256.to_owned(),
        byte_len,
        object_key: upload.object_key.clone(),
        media_type: upload.descriptor.mime_type.clone(),
        filename: Some(upload.descriptor.filename.clone()),
        classification: authority.classification.clone().unwrap_or_default(),
        labels: authority.data_labels.clone(),
        metadata: BTreeMap::new(),
        retention_expires_at: None,
    })
}

fn strongest(left: GrantPermission, right: GrantPermission) -> GrantPermission {
    use GrantPermission::*;
    match (left, right) {
        (Admin, _) | (_, Admin) => Admin,
        (Write, _) | (_, Write) => Write,
        _ => Read,
    }
}
