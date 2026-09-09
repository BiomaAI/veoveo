//! Service-owned recovery: bounded workers, lease renewal, and physical cleanup.

use super::*;

const WORK_LEASE_SECONDS: u32 = 120;
const CONTROL_TIMEOUT: Duration = Duration::from_secs(30);

impl UploadService {
    /// The binary owns this task. Dropping a replica cannot discard durable work.
    pub async fn run_recovery(self) {
        let mut tick = tokio::time::interval(Duration::from_secs(15));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! { _ = tick.tick() => {}, _ = self.wake.notified() => {} }
            if let Err(error) = self.recovery_pass().await {
                tracing::warn!(?error, "artifact upload recovery pass will retry");
            }
        }
    }

    async fn recovery_pass(&self) -> Result<(), UploadFault> {
        loop {
            let parts = self
                .database
                .expired_upload_part_leases(Utc::now(), 256)
                .await?;
            let more = parts.len() == 256;
            for part in parts {
                if let Some(owner) = part.lease_owner {
                    self.database
                        .release_artifact_upload_part(&platform::ArtifactUploadPartFence {
                            upload_id: view::uuid(&part.upload)?,
                            part_number: u32::try_from(part.part_number)
                                .map_err(|_| UploadFault::unavailable())?,
                            lease_owner: owner,
                            generation: part.generation,
                        })
                        .await?;
                }
            }
            if !more {
                break;
            }
        }
        let mut after = None;
        loop {
            let rows = self
                .database
                .artifact_upload_recovery_page(after, 64)
                .await?;
            let more = rows.len() == 64;
            after = rows.last().map(|row| row.id.clone());
            for row in rows {
                if row.lease_owner.is_some()
                    && row.lease_until.is_some_and(|until| until > Utc::now())
                {
                    continue;
                }
                if row.state == platform::ArtifactUploadState::Open
                    && row.multipart_id.is_some()
                    && row.expires_at > Utc::now()
                    && self.authority_current(&row).await?
                {
                    continue;
                }
                // Open sessions are inspected for expiry and revocation as well
                // as initialization, without querying a job provider's status.
                let Ok(permit) = self.workers.clone().try_acquire_owned() else {
                    continue;
                };
                let service = self.clone();
                tokio::spawn(async move {
                    let _permit = permit;
                    if let Err(error) = service.recover(row).await {
                        tracing::debug!(?error, "upload work remains in durable recovery queue");
                    }
                });
            }
            if !more {
                return Ok(());
            }
        }
    }

    pub(super) async fn initialize(&self, id: uuid::Uuid) -> Result<(), UploadFault> {
        let owner = uuid::Uuid::now_v7();
        let row = self
            .database
            .claim_artifact_upload_work(id, owner, WORK_LEASE_SECONDS)
            .await?;
        let fence = platform::ArtifactUploadFence {
            upload_id: id,
            owner,
            generation: row.generation,
        };
        let result = self.initialize_fenced(&row, &fence).await;
        let _ = self.database.release_artifact_upload_work(&fence).await;
        result
    }

    async fn initialize_fenced(
        &self,
        row: &platform::ArtifactUploadRecord,
        fence: &platform::ArtifactUploadFence,
    ) -> Result<(), UploadFault> {
        if row.multipart_id.is_some() {
            return Ok(());
        }
        let multipart = tokio::time::timeout(
            CONTROL_TIMEOUT,
            self.objects
                .create_upload(&row.object_key, &row.descriptor.mime_type),
        )
        .await
        .map_err(|_| UploadFault::unavailable())??;
        if let Err(error) = self
            .database
            .initialize_artifact_upload(fence, &multipart)
            .await
        {
            // An acknowledged provider handle is never abandoned on a failed
            // ledger write. TODO: enumerate session-prefixed S3 multipart handles
            // to recover a crash or lost create acknowledgement before activation.
            let _ = tokio::time::timeout(
                CONTROL_TIMEOUT,
                self.objects.abort_upload(&row.object_key, &multipart),
            )
            .await;
            return Err(error.into());
        }
        Ok(())
    }

    async fn recover(&self, previous: platform::ArtifactUploadRecord) -> Result<(), UploadFault> {
        let id = view::uuid(&previous.id)?;
        if previous.state == platform::ArtifactUploadState::Open
            && previous.multipart_id.is_some()
            && previous.expires_at > Utc::now()
            && self.authority_current(&previous).await?
        {
            return Ok(());
        }
        let owner = uuid::Uuid::now_v7();
        let row = self
            .database
            .claim_artifact_upload_work(id, owner, WORK_LEASE_SECONDS)
            .await?;
        if row.lease_owner != Some(owner) {
            return Err(contract::UploadErrorCode::Busy.into());
        }
        let fence = platform::ArtifactUploadFence {
            upload_id: id,
            owner,
            generation: row.generation,
        };
        let result = self.drive_work(&row, &fence).await;
        if let Err(fault) = &result {
            let failure = match fault.0 {
                contract::UploadErrorCode::Integrity => {
                    Some(platform::ArtifactUploadFailure::Integrity)
                }
                contract::UploadErrorCode::Denied => {
                    Some(platform::ArtifactUploadFailure::AuthorityChanged)
                }
                contract::UploadErrorCode::Expired => {
                    Some(platform::ArtifactUploadFailure::Expired)
                }
                _ => None,
            };
            if let Some(failure) = failure {
                let _ = self.database.fail_artifact_upload(&fence, failure).await;
            }
        }
        let _ = self.database.release_artifact_upload_work(&fence).await;
        result
    }

    async fn drive_work(
        &self,
        row: &platform::ArtifactUploadRecord,
        fence: &platform::ArtifactUploadFence,
    ) -> Result<(), UploadFault> {
        let operation = self.process_fenced(row, fence);
        tokio::pin!(operation);
        let mut tick = tokio::time::interval(Duration::from_secs(30));
        tick.tick().await;
        loop {
            tokio::select! {
                result = &mut operation => return result,
                _ = tick.tick() => {
                    let current = self.database.renew_artifact_upload_work(fence, WORK_LEASE_SECONDS).await?;
                    if matches!(row.state, platform::ArtifactUploadState::Open | platform::ArtifactUploadState::Finalizing | platform::ArtifactUploadState::Verifying) {
                        if matches!(current.state, platform::ArtifactUploadState::Cancelled | platform::ArtifactUploadState::Expired | platform::ArtifactUploadState::Failed) { return Err(contract::UploadErrorCode::Conflict.into()); }
                        if current.expires_at <= Utc::now() { return Err(contract::UploadErrorCode::Expired.into()); }
                        if !self.authority_current(&current).await? { return Err(contract::UploadErrorCode::Denied.into()); }
                    }
                }
            }
        }
    }

    async fn authority_current(
        &self,
        row: &platform::ArtifactUploadRecord,
    ) -> Result<bool, UploadFault> {
        let current = self
            .database
            .artifact_upload_authority_version(
                &row.tenant_key,
                &row.authority.context_key,
                &row.profile_key,
            )
            .await?;
        Ok(current.is_some_and(|current| {
            current.context_digest == row.context_digest
                && current.profile_policy_digest.as_ref() == Some(&row.profile_policy_digest)
        }))
    }

    async fn process_fenced(
        &self,
        row: &platform::ArtifactUploadRecord,
        fence: &platform::ArtifactUploadFence,
    ) -> Result<(), UploadFault> {
        use platform::ArtifactUploadState::*;
        if matches!(row.state, Open | Finalizing | Verifying) {
            if row.expires_at <= Utc::now() {
                return Err(contract::UploadErrorCode::Expired.into());
            }
            if !self.authority_current(row).await? {
                return Err(contract::UploadErrorCode::Denied.into());
            }
        }
        match row.state {
            Open => self.initialize_fenced(row, fence).await,
            Finalizing | Verifying => {
                let manifest = row.manifest.as_ref().ok_or_else(UploadFault::unavailable)?;
                let multipart = row
                    .multipart_id
                    .as_deref()
                    .ok_or_else(UploadFault::unavailable)?;
                if row.state == Finalizing {
                    let parts = self
                        .database
                        .artifact_upload_parts(fence.upload_id, 0, 10001, true)
                        .await?;
                    let content_ids = parts
                        .into_iter()
                        .map(|part| part.content_id.ok_or_else(UploadFault::unavailable))
                        .collect::<Result<Vec<_>, _>>()?;
                    tokio::time::timeout(
                        CONTROL_TIMEOUT,
                        self.objects.complete_upload(
                            &row.object_key,
                            multipart,
                            content_ids,
                            manifest.byte_len as u64,
                        ),
                    )
                    .await
                    .map_err(|_| UploadFault::unavailable())??;
                    self.database.verify_artifact_upload(fence).await?;
                }
                let expected_sha = manifest
                    .sha256
                    .clone()
                    .map(contract::UploadSha256::parse)
                    .transpose()
                    .map_err(|_| UploadFault::unavailable())?;
                let verified = self
                    .objects
                    .verify_upload(
                        &row.object_key,
                        manifest.byte_len as u64,
                        expected_sha.as_ref(),
                    )
                    .await?;
                self.database
                    .publish_artifact_upload(fence, &verified.sha256, verified.byte_len as i64)
                    .await?;
                self.wake.notify_one();
                Ok(())
            }
            Completed | Cancelled | Expired | Failed => {
                if !self.database.artifact_upload_cleanup_ready(fence).await? {
                    return Ok(());
                }
                if let Some(multipart) = &row.multipart_id {
                    tokio::time::timeout(
                        CONTROL_TIMEOUT,
                        self.objects.abort_upload(&row.object_key, multipart),
                    )
                    .await
                    .map_err(|_| UploadFault::unavailable())??;
                }
                tokio::time::timeout(CONTROL_TIMEOUT, self.objects.delete(&row.object_key))
                    .await
                    .map_err(|_| UploadFault::unavailable())??;
                self.database.finish_artifact_upload_cleanup(fence).await?;
                Ok(())
            }
        }
    }
}
