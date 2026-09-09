//! Durable upload sessions, immutable part descriptors, and shared storage accounting.

mod lifecycle;
mod model;
mod parts;
mod publication;
pub use lifecycle::*;
pub use model::*;
pub use parts::*;

use crate::{PlatformStore, StoreError};

impl PlatformStore {
    /// The gateway control-plane is an open document boundary. Its consumer
    /// deserializes only the typed profile fields it owns.
    pub async fn artifact_upload_profile(
        &self,
        profile: &str,
    ) -> Result<Option<crate::OpenObject>, StoreError> {
        let mut response = self.db.query("LET $active = SELECT * FROM ONLY gateway_control_active:current; SELECT VALUE document FROM gateway_control_object WHERE revision = $active.revision AND object_kind = 'profile' AND object_id = $profile LIMIT 1;")
            .bind(("profile", profile.to_owned())).await?.check()?;
        let mut documents: Vec<crate::OpenObject> = response.take(1)?;
        Ok(documents.pop())
    }

    pub async fn artifact_upload_storage_usage(
        &self,
        tenant: crate::TenantId,
    ) -> Result<Option<ArtifactStorageUsage>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM ONLY $usage;")
            .bind(("usage", artifact_storage_usage_id(tenant)))
            .await?
            .check()?;
        response.take(0).map_err(Into::into)
    }

    pub async fn artifact_upload_authority_version(
        &self,
        tenant_key: &str,
        context_key: &str,
        profile_key: &str,
    ) -> Result<Option<ArtifactUploadAuthorityVersion>, StoreError> {
        let mut response = self
            .db
            .query(include_str!("artifact_uploads/authority.surql"))
            .bind((
                "context",
                crate::deterministic_work_context_id(tenant_key, context_key)?.record_id(),
            ))
            .bind(("profile_key", profile_key.to_owned()))
            .await?
            .check()?;
        response.take(0).map_err(Into::into)
    }

    /// The shared tenant row serializes admission across replicas and includes
    /// committed storage and retained cleanup debt in the effective quota.
    pub async fn admit_artifact_upload(
        &self,
        content: ArtifactUploadRecord,
        quota: i64,
        active_limit: i64,
    ) -> Result<ArtifactUploadRecord, StoreError> {
        if quota <= 0
            || quota > 9_007_199_254_740_991
            || active_limit <= 0
            || content.state != ArtifactUploadState::Open
            || content.reserved_bytes < 0
            || content.reserved_bytes > content.layout.max_total_bytes
            || content.descriptor.byte_len.is_some_and(|size| {
                size < 0 || size != content.reserved_bytes || size > content.layout.max_total_bytes
            })
            || content.layout.part_bytes < 5 * 1024 * 1024
            || !(1..=10000).contains(&content.layout.max_parts)
            || content.layout.parallel_parts <= 0
            || content.layout.max_total_bytes <= 0
            || content.layout.max_total_bytes > 9_007_199_254_740_991
            || content.accepted_bytes != 0
            || content.accepted_part_count != 0
            || content.multipart_id.is_some()
            || content.generation != 0
            || content.manifest.is_some()
            || content.completed_at.is_some()
            || content.expires_at <= chrono::Utc::now()
            || content.expires_at > content.lifetime_ends_at
        {
            return Err(StoreError::ArtifactUpload(
                ArtifactUploadRejection::Conflict,
            ));
        }
        let usage =
            surrealdb::types::RecordId::new("artifact_storage_usage", content.tenant.key.clone());
        for attempt in 0..8_u32 {
            let mut response = self
                .db
                .query(include_str!("artifact_uploads/admit.surql"))
                .bind(("content", content.clone()))
                .bind(("usage", usage.clone()))
                .bind(("quota", quota))
                .bind(("active_limit", active_limit))
                .await?;
            let Some(error) = crate::store::primary_transaction_error(response.take_errors())
            else {
                break;
            };
            if attempt < 7 && retryable(&error) {
                tokio::time::sleep(std::time::Duration::from_millis(1_u64 << attempt)).await;
                continue;
            }
            return Err(upload_error(error));
        }
        let mut response = self.db.query("SELECT * FROM artifact_upload WHERE tenant = $tenant AND actor = $actor AND profile_key = $profile AND work_context = $context AND request_id = $request_id;")
            .bind(("tenant", content.tenant)).bind(("actor", content.actor))
            .bind(("profile", content.profile_key)).bind(("context", content.work_context))
            .bind(("request_id", content.request_id)).await?.check()?;
        let mut records: Vec<ArtifactUploadRecord> = response.take(0)?;
        records.pop().ok_or(StoreError::MissingRecord {
            operation: "artifact upload admission readback",
        })
    }

    /// Read identity before exposing any session state; callers enforce current authority.
    pub async fn artifact_upload(
        &self,
        id: uuid::Uuid,
    ) -> Result<Option<ArtifactUploadRecord>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM ONLY $upload;")
            .bind(("upload", upload_record_id(id)))
            .await?
            .check()?;
        response.take(0).map_err(Into::into)
    }
}

fn retryable(error: &surrealdb::Error) -> bool {
    matches!(
        error.query_details(),
        Some(surrealdb::types::QueryError::TransactionConflict)
    ) || error.message().starts_with("Transaction conflict:")
        || error
            .message()
            .contains("not executed due to a failed transaction")
}

fn upload_error(error: surrealdb::Error) -> StoreError {
    if error.is_thrown() {
        for (marker, reason) in [
            (
                "artifact_upload_conflict",
                ArtifactUploadRejection::Conflict,
            ),
            ("artifact_upload_denied", ArtifactUploadRejection::Denied),
            ("artifact_upload_quota", ArtifactUploadRejection::Quota),
            ("artifact_upload_busy", ArtifactUploadRejection::Busy),
            ("artifact_upload_expired", ArtifactUploadRejection::Expired),
            (
                "artifact_upload_integrity",
                ArtifactUploadRejection::Integrity,
            ),
        ] {
            if error.message().contains(marker) {
                return StoreError::ArtifactUpload(reason);
            }
        }
    }
    error.into()
}
