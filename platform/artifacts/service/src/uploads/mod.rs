//! Resumable upload orchestration; Store owns durable state, S3 owns file bytes.

mod authority;
mod error;
mod recovery;
mod transfer;
mod view;

use chrono::{TimeDelta, Utc};
use std::{num::NonZeroU32, sync::Arc, time::Duration};
use tokio::sync::{Notify, Semaphore};
use veoveo_mcp_contract as contract;
use veoveo_platform_store as platform;

use crate::store::{ArtifactObjectStore, BlobStore, BlobStream};
pub use error::UploadFault;

#[derive(Clone)]
pub struct UploadService {
    database: platform::PlatformStore,
    objects: ArtifactObjectStore,
    wake: Arc<Notify>,
    workers: Arc<Semaphore>,
}

impl UploadService {
    pub fn new(database: platform::PlatformStore, objects: ArtifactObjectStore) -> Self {
        Self {
            database,
            objects,
            wake: Arc::new(Notify::new()),
            workers: Arc::new(Semaphore::new(2)),
        }
    }

    pub async fn create(
        &self,
        caller: &contract::VerifiedArtifactUploadIdentity,
        request_id: contract::ArtifactUploadRequestId,
        descriptor: contract::CreateArtifactUpload,
    ) -> Result<(contract::ArtifactUploadSession, bool), UploadFault> {
        let authority = self.authorize(caller).await?;
        let layout = authority.policy.admit(&descriptor)?;
        let row = authority.admission(caller, request_id, descriptor, &layout)?;
        self.database
            .ensure_identity(
                &row.tenant_key,
                &row.actor_key,
                &row.actor_issuer,
                &row.actor_subject,
                row.actor_kind,
            )
            .await?;
        let new_id = row.id.clone();
        let row = self
            .database
            .admit_artifact_upload(
                row,
                authority.policy.tenant_quota_bytes.get() as i64,
                i64::from(authority.policy.max_active_uploads_per_tenant.get()),
            )
            .await?;
        let created = row.id == new_id;
        let id = view::uuid(&row.id)?;
        if row.state == platform::ArtifactUploadState::Open && row.multipart_id.is_none() {
            self.initialize(id).await?;
        }
        Ok((
            self.status(
                caller,
                contract::ArtifactUploadId::parse(id.to_string())
                    .map_err(|_| UploadFault::unavailable())?,
                0,
            )
            .await?,
            created,
        ))
    }

    pub async fn status(
        &self,
        caller: &contract::VerifiedArtifactUploadIdentity,
        id: contract::ArtifactUploadId,
        after: u32,
    ) -> Result<contract::ArtifactUploadSession, UploadFault> {
        let (_, row) = self.owned(caller, id).await?;
        let parts = self
            .database
            .artifact_upload_parts(
                id.as_uuid(),
                after,
                (contract::UPLOAD_PART_PAGE_LIMIT + 1) as u32,
                true,
            )
            .await?;
        view::session(row, parts)
    }

    pub async fn complete(
        &self,
        caller: &contract::VerifiedArtifactUploadIdentity,
        id: contract::ArtifactUploadId,
        mut manifest: contract::CompleteArtifactUpload,
    ) -> Result<contract::ArtifactUploadSession, UploadFault> {
        let (_, row) = self.owned(caller, id).await?;
        if manifest.sha256.is_none() {
            manifest.sha256 = row
                .descriptor
                .sha256
                .clone()
                .map(contract::UploadSha256::parse)
                .transpose()
                .map_err(|_| UploadFault::unavailable())?;
        }
        let parts = self
            .database
            .artifact_upload_parts(id.as_uuid(), 0, 10001, true)
            .await?;
        let receipts = parts
            .iter()
            .map(view::part)
            .collect::<Result<Vec<_>, _>>()?;
        view::layout(&row.layout)?.validate_manifest(&manifest, &receipts)?;
        self.database
            .freeze_artifact_upload(
                id.as_uuid(),
                platform::ArtifactUploadManifest {
                    byte_len: i64::try_from(manifest.byte_len)
                        .map_err(|_| contract::UploadErrorCode::TooLarge)?,
                    part_count: i64::from(manifest.part_count.get()),
                    sha256: manifest.sha256.map(Into::into),
                },
                &row.policy_digest,
            )
            .await?;
        self.wake.notify_one();
        self.status(caller, id, 0).await
    }

    pub async fn cancel(
        &self,
        caller: &contract::VerifiedArtifactUploadIdentity,
        id: contract::ArtifactUploadId,
    ) -> Result<(), UploadFault> {
        // Ownership/current authority are checked even on an idempotent cancellation.
        self.owned(caller, id).await?;
        self.database.cancel_artifact_upload(id.as_uuid()).await?;
        self.wake.notify_one();
        Ok(())
    }
}
