//! Bounded request bodies and cancellation-safe release of shared payload budget.

use super::*;
use crate::store::multipart::VerifiedUploadPayload;

struct PartLease {
    database: platform::PlatformStore,
    fence: Option<platform::ArtifactUploadPartFence>,
}

impl Drop for PartLease {
    fn drop(&mut self) {
        if let Some(fence) = self.fence.take() {
            let database = self.database.clone();
            // Cancellation drops the body/storage future before releasing its
            // budget. A crashed runtime is recovered by the durable lease scan.
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(async move {
                    if let Err(error) = database.release_artifact_upload_part(&fence).await {
                        tracing::warn!(error = %error, "upload part lease release deferred to recovery");
                    }
                });
            }
        }
    }
}

impl UploadService {
    pub async fn put_part(
        &self,
        caller: &contract::VerifiedArtifactUploadIdentity,
        id: veoveo_artifact_contract::ArtifactUploadId,
        number: NonZeroU32,
        byte_len: u64,
        sha256: veoveo_artifact_contract::UploadSha256,
        stream: BlobStream,
    ) -> Result<veoveo_artifact_contract::UploadPartReceipt, UploadFault> {
        let (authority, row) = self.owned(caller, id).await?;
        view::layout(&row.layout)?.validate_part(number, byte_len)?;
        let multipart = row
            .multipart_id
            .as_deref()
            .ok_or(veoveo_artifact_contract::UploadErrorCode::Busy)?;
        let timeout = authority.policy.part_timeout_seconds.get();
        let owner = uuid::Uuid::now_v7();
        let claim = platform::ClaimArtifactUploadPart {
            upload_id: id.as_uuid(),
            part_number: number.get(),
            byte_len: i64::try_from(byte_len)
                .map_err(|_| veoveo_artifact_contract::UploadErrorCode::TooLarge)?,
            sha256: sha256.as_str().to_owned(),
            lease_owner: owner,
            lease_seconds: (timeout + 15) as u32,
            policy_digest: authority.policy_digest.clone(),
            quota_bytes: authority.policy.tenant_quota_bytes.get() as i64,
            max_inflight_bytes: authority.policy.max_inflight_bytes.get() as i64,
            max_tenant_inflight_parts: authority
                .policy
                .max_active_uploads_per_tenant
                .get()
                .saturating_mul(authority.policy.parallel_parts.get()),
        };
        let database = self.database.clone();
        // The claim can commit before its response reaches this request. Keep
        // that response and its exact lease guard together even when the caller
        // cancels while the database is settling the claim.
        let claimed = tokio::spawn(async move {
            let part = tokio::time::timeout(
                database.config().query_timeout(),
                database.claim_artifact_upload_part(claim),
            )
            .await
            .map_err(|_| UploadFault::unavailable())??;
            let fence = (part.state != platform::ArtifactUploadPartState::Accepted
                && part.lease_owner == Some(owner))
            .then(|| platform::ArtifactUploadPartFence {
                upload_id: id.as_uuid(),
                part_number: number.get(),
                lease_owner: owner,
                generation: part.generation,
            });
            Ok::<_, UploadFault>((part, PartLease { database, fence }))
        });
        let (part, mut guard) = claimed.await.map_err(|_| UploadFault::unavailable())??;
        if part.state == platform::ArtifactUploadPartState::Accepted {
            return view::part(&part);
        }
        if part.lease_owner != Some(owner) {
            return Err(veoveo_artifact_contract::UploadErrorCode::Busy.into());
        }
        let fence = guard
            .fence
            .as_ref()
            .ok_or_else(UploadFault::unavailable)?
            .clone();
        let accepted = tokio::time::timeout(Duration::from_secs(timeout), async {
            let payload = VerifiedUploadPayload::read(stream, byte_len, &sha256).await?;
            let content_id = self
                .objects
                .put_upload_part(&row.object_key, multipart, number, payload)
                .await?;
            self.database
                .accept_artifact_upload_part(&fence, &content_id, &authority.policy_digest)
                .await
                .map_err(UploadFault::from)
        })
        .await
        .map_err(|_| UploadFault::unavailable())??;
        guard.fence = None;
        view::part(&accepted)
    }
}
