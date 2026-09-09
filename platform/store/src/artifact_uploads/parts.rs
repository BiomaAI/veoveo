//! Part identity is immutable; request leases fence storage acknowledgements.

use super::*;
use chrono::{DateTime, TimeDelta, Utc};
use surrealdb::types::RecordId;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct ClaimArtifactUploadPart {
    pub upload_id: Uuid,
    pub part_number: u32,
    pub byte_len: i64,
    pub sha256: String,
    pub lease_owner: Uuid,
    pub lease_seconds: u32,
    pub policy_digest: String,
    pub quota_bytes: i64,
    pub max_inflight_bytes: i64,
    pub max_tenant_inflight_parts: u32,
}

#[derive(Clone, Debug)]
pub struct ArtifactUploadPartFence {
    pub upload_id: Uuid,
    pub part_number: u32,
    pub lease_owner: Uuid,
    pub generation: i64,
}

impl PlatformStore {
    pub async fn artifact_upload_parts(
        &self,
        upload_id: Uuid,
        after: u32,
        limit: u32,
        accepted_only: bool,
    ) -> Result<Vec<ArtifactUploadPartRecord>, StoreError> {
        if limit == 0 || limit > 10001 {
            return Err(StoreError::ArtifactUpload(
                ArtifactUploadRejection::Conflict,
            ));
        }
        let mut response = self.db.query("SELECT * FROM artifact_upload_part WHERE upload = $upload AND part_number > $after AND (!$accepted_only OR state = 'accepted') ORDER BY part_number ASC LIMIT $limit;")
            .bind(("upload", upload_record_id(upload_id))).bind(("after", i64::from(after)))
            .bind(("limit", i64::from(limit))).bind(("accepted_only", accepted_only)).await?.check()?;
        response.take(0).map_err(Into::into)
    }

    pub async fn claim_artifact_upload_part(
        &self,
        request: ClaimArtifactUploadPart,
    ) -> Result<ArtifactUploadPartRecord, StoreError> {
        if !(1..=10000).contains(&request.part_number)
            || request.byte_len < 0
            || request.byte_len > 5 * 1024 * 1024 * 1024
            || request.lease_seconds == 0
            || request.lease_seconds > 3660
            || request.max_inflight_bytes <= 0
            || request.max_tenant_inflight_parts == 0
            || request.quota_bytes <= 0
            || request.sha256.len() != 64
            || !request
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(StoreError::ArtifactUpload(
                ArtifactUploadRejection::Conflict,
            ));
        }
        let id = upload_part_record_id(request.upload_id, request.part_number);
        let upload = upload_record_id(request.upload_id);
        for attempt in 0..8_u32 {
            let now = Utc::now();
            let lease_until = now + TimeDelta::seconds(i64::from(request.lease_seconds));
            let mut response = self
                .db
                .query(include_str!("claim_part.surql"))
                .bind(("part", id.clone()))
                .bind(("upload", upload.clone()))
                .bind(("part_number", i64::from(request.part_number)))
                .bind(("byte_len", request.byte_len))
                .bind(("sha256", request.sha256.clone()))
                .bind(("lease_owner", request.lease_owner))
                .bind(("now", now))
                .bind(("lease_until", lease_until))
                .bind(("policy_digest", request.policy_digest.clone()))
                .bind(("quota", request.quota_bytes))
                .bind(("max_inflight_bytes", request.max_inflight_bytes))
                .bind(("tenant_parts", i64::from(request.max_tenant_inflight_parts)))
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
        self.upload_part(id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "upload part lease readback",
            })
    }

    pub async fn accept_artifact_upload_part(
        &self,
        fence: &ArtifactUploadPartFence,
        content_id: &str,
        policy_digest: &str,
    ) -> Result<ArtifactUploadPartRecord, StoreError> {
        if content_id.len() > 4096 {
            return Err(StoreError::ArtifactUpload(
                ArtifactUploadRejection::Conflict,
            ));
        }
        self.finish_upload_part(fence, Some(content_id), policy_digest)
            .await?;
        self.upload_part(upload_part_record_id(fence.upload_id, fence.part_number))
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "accepted upload part readback",
            })
    }

    pub async fn release_artifact_upload_part(
        &self,
        fence: &ArtifactUploadPartFence,
    ) -> Result<(), StoreError> {
        self.finish_upload_part(fence, None, "").await
    }

    async fn finish_upload_part(
        &self,
        fence: &ArtifactUploadPartFence,
        content_id: Option<&str>,
        policy_digest: &str,
    ) -> Result<(), StoreError> {
        for attempt in 0..8_u32 {
            let mut response = self
                .db
                .query(include_str!("finish_part.surql"))
                .bind((
                    "part",
                    upload_part_record_id(fence.upload_id, fence.part_number),
                ))
                .bind(("lease_owner", fence.lease_owner))
                .bind(("generation", fence.generation))
                .bind(("content_id", content_id.map(str::to_owned)))
                .bind(("policy_digest", policy_digest.to_owned()))
                .bind(("now", Utc::now()))
                .await?;
            let Some(error) = crate::store::primary_transaction_error(response.take_errors())
            else {
                return Ok(());
            };
            if attempt < 7 && retryable(&error) {
                tokio::time::sleep(std::time::Duration::from_millis(1_u64 << attempt)).await;
                continue;
            }
            return Err(upload_error(error));
        }
        unreachable!("bounded transaction loop returns on success or its last error")
    }

    async fn upload_part(
        &self,
        id: RecordId,
    ) -> Result<Option<ArtifactUploadPartRecord>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM ONLY $part;")
            .bind(("part", id))
            .await?
            .check()?;
        response.take(0).map_err(Into::into)
    }

    /// A crashed request holds budget only until its lease can be reclaimed.
    pub async fn expired_upload_part_leases(
        &self,
        now: DateTime<Utc>,
        limit: u32,
    ) -> Result<Vec<ArtifactUploadPartRecord>, StoreError> {
        let mut response = self.db.query("SELECT * FROM artifact_upload_part WHERE lease_owner != NONE AND lease_until <= $now LIMIT $limit;")
            .bind(("now", now)).bind(("limit", i64::from(limit.min(256)))).await?.check()?;
        response.take(0).map_err(Into::into)
    }
}
