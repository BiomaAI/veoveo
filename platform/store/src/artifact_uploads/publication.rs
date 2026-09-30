//! An upload receipt and governed occurrence become durable in one transaction.

use super::*;
use crate::audit::AuditTransactionWrite;
use crate::{
    ArtifactGrantDraft, ArtifactGrantSubjectKind, ArtifactId, ArtifactOccurrenceDraft,
    GrantPermission, PlatformIdentity, PrincipalId, TenantId,
};
use chrono::Utc;
use std::collections::BTreeMap;
use surrealdb::types::RecordId;
use veoveo_audit_contract::{
    ArtifactActivity, AuditDetail, AuditOutcome, AuditReason, AuditTarget,
};

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
        let audit = upload.audit.0.draft(
            AuditTarget::Artifact {
                artifact: veoveo_artifact_contract::ArtifactId::try_from(
                    crate::artifacts::record_uuid(&upload.artifact)?,
                )
                .map_err(|_| StoreError::AuditIntegrity)?,
            },
            AuditDetail::Artifact {
                activity: ArtifactActivity::Publish,
                requested: None,
                subject: None,
                release_state: None,
                related: None,
                bytes: Some(u64::try_from(byte_len).map_err(|_| StoreError::AuditIntegrity)?),
                window_start: None,
            },
            AuditOutcome::Succeeded,
            AuditReason::Accepted,
        )?;
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
                .bind(AuditTransactionWrite::new(audit.clone())?.into_binding())
                .bind((
                    "storage_usage",
                    RecordId::new("artifact_storage_usage", upload.tenant.key.clone()),
                ))
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
