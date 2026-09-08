//! Fenced initialization, manifest sealing, and terminal cleanup accounting.

use super::*;
use chrono::{TimeDelta, Utc};
use surrealdb::types::RecordId;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct ArtifactUploadFence {
    pub upload_id: Uuid,
    pub owner: Uuid,
    pub generation: i64,
}

/// One typed binding set for the private lifecycle transactions. SQL is selected
/// by the method, never supplied by a public caller.
#[derive(Default)]
struct Mutation<'a> {
    owner: Option<Uuid>,
    generation: i64,
    seconds: u32,
    operation: &'static str,
    multipart_id: Option<&'a str>,
    manifest: Option<ArtifactUploadManifest>,
    policy_digest: Option<&'a str>,
    terminal: Option<ArtifactUploadState>,
    failure: Option<ArtifactUploadFailure>,
}

impl Mutation<'_> {
    fn fenced(fence: &ArtifactUploadFence) -> Self {
        Self {
            owner: Some(fence.owner),
            generation: fence.generation,
            ..Self::default()
        }
    }
}

impl PlatformStore {
    pub async fn claim_artifact_upload_work(
        &self,
        upload_id: Uuid,
        owner: Uuid,
        seconds: u32,
    ) -> Result<ArtifactUploadRecord, StoreError> {
        check_lease_seconds(seconds)?;
        self.mutate_upload(
            include_str!("lease.surql"),
            upload_id,
            Mutation {
                owner: Some(owner),
                seconds,
                operation: "claim",
                ..Mutation::default()
            },
        )
        .await
    }

    pub async fn renew_artifact_upload_work(
        &self,
        fence: &ArtifactUploadFence,
        seconds: u32,
    ) -> Result<ArtifactUploadRecord, StoreError> {
        check_lease_seconds(seconds)?;
        self.mutate_upload(
            include_str!("lease.surql"),
            fence.upload_id,
            Mutation {
                seconds,
                operation: "renew",
                ..Mutation::fenced(fence)
            },
        )
        .await
    }

    pub async fn release_artifact_upload_work(
        &self,
        fence: &ArtifactUploadFence,
    ) -> Result<(), StoreError> {
        self.mutate_upload(
            include_str!("lease.surql"),
            fence.upload_id,
            Mutation {
                operation: "release",
                ..Mutation::fenced(fence)
            },
        )
        .await
        .map(|_| ())
    }

    pub async fn initialize_artifact_upload(
        &self,
        fence: &ArtifactUploadFence,
        multipart_id: &str,
    ) -> Result<ArtifactUploadRecord, StoreError> {
        if multipart_id.is_empty() || multipart_id.len() > 4096 {
            return Err(conflict());
        }
        self.mutate_upload(
            include_str!("lease.surql"),
            fence.upload_id,
            Mutation {
                operation: "initialize",
                multipart_id: Some(multipart_id),
                ..Mutation::fenced(fence)
            },
        )
        .await
    }

    pub async fn freeze_artifact_upload(
        &self,
        upload_id: Uuid,
        manifest: ArtifactUploadManifest,
        policy_digest: &str,
    ) -> Result<ArtifactUploadRecord, StoreError> {
        if manifest.byte_len < 0
            || manifest.byte_len > 9_007_199_254_740_991
            || !(1..=10000).contains(&manifest.part_count)
            || manifest
                .sha256
                .as_deref()
                .is_some_and(|sha| !valid_digest(sha))
        {
            return Err(conflict());
        }
        self.mutate_upload(
            include_str!("freeze.surql"),
            upload_id,
            Mutation {
                manifest: Some(manifest),
                policy_digest: Some(policy_digest),
                ..Mutation::default()
            },
        )
        .await
    }

    pub async fn verify_artifact_upload(
        &self,
        fence: &ArtifactUploadFence,
    ) -> Result<ArtifactUploadRecord, StoreError> {
        self.mutate_upload(
            include_str!("lease.surql"),
            fence.upload_id,
            Mutation {
                operation: "verify",
                ..Mutation::fenced(fence)
            },
        )
        .await
    }

    /// Cancellation may win against a finalizer, but does not release the
    /// finalizer's or part requests' leases while they could still touch storage.
    pub async fn cancel_artifact_upload(
        &self,
        upload_id: Uuid,
    ) -> Result<ArtifactUploadRecord, StoreError> {
        self.mutate_upload(
            include_str!("terminate.surql"),
            upload_id,
            Mutation {
                terminal: Some(ArtifactUploadState::Cancelled),
                ..Mutation::default()
            },
        )
        .await
    }

    pub async fn fail_artifact_upload(
        &self,
        fence: &ArtifactUploadFence,
        failure: ArtifactUploadFailure,
    ) -> Result<ArtifactUploadRecord, StoreError> {
        let terminal = if failure == ArtifactUploadFailure::Expired {
            ArtifactUploadState::Expired
        } else {
            ArtifactUploadState::Failed
        };
        self.mutate_upload(
            include_str!("terminate.surql"),
            fence.upload_id,
            Mutation {
                terminal: Some(terminal),
                failure: Some(failure),
                ..Mutation::fenced(fence)
            },
        )
        .await
    }

    /// Call before deleting the unique session object. Terminal state prevents a
    /// concurrent publication; live request leases defer cleanup until work stops.
    pub async fn artifact_upload_cleanup_ready(
        &self,
        fence: &ArtifactUploadFence,
    ) -> Result<bool, StoreError> {
        let mut response = self.db.query("LET $item = SELECT * FROM ONLY $upload; LET $parts = SELECT VALUE id FROM artifact_upload_part WHERE upload = $upload AND lease_owner != NONE LIMIT 1; LET $blobs = SELECT VALUE id FROM artifact_blob WHERE tenant = $item.tenant AND object_key = $item.object_key LIMIT 1; RETURN $item.cleanup_pending AND $item.state IN ['completed', 'cancelled', 'expired', 'failed'] AND $item.lease_owner = $owner AND $item.generation = $generation AND $item.lease_until > $now AND array::len($parts) = 0 AND array::len($blobs) = 0;")
            .bind(("upload", upload_record_id(fence.upload_id))).bind(("owner", fence.owner))
            .bind(("generation", fence.generation)).bind(("now", Utc::now())).await?.check()?;
        Ok(response.take::<Option<bool>>(3)?.unwrap_or(false))
    }

    /// Physical deletion must succeed before releasing retained cleanup debt.
    pub async fn finish_artifact_upload_cleanup(
        &self,
        fence: &ArtifactUploadFence,
    ) -> Result<ArtifactUploadRecord, StoreError> {
        self.mutate_upload(
            include_str!("cleanup.surql"),
            fence.upload_id,
            Mutation::fenced(fence),
        )
        .await
    }

    pub async fn artifact_upload_recovery_page(
        &self,
        after: Option<RecordId>,
        limit: u32,
    ) -> Result<Vec<ArtifactUploadRecord>, StoreError> {
        let mut response = self.db.query("SELECT * FROM artifact_upload WHERE (state IN ['open', 'finalizing', 'verifying'] OR cleanup_pending) AND ($after = NONE OR id > $after) ORDER BY id ASC LIMIT $limit;")
            .bind(("after", after)).bind(("limit", i64::from(limit.clamp(1, 256)))).await?.check()?;
        response.take(0).map_err(Into::into)
    }

    pub(super) async fn required_upload(
        &self,
        id: Uuid,
    ) -> Result<ArtifactUploadRecord, StoreError> {
        self.artifact_upload(id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "upload lifecycle readback",
            })
    }

    async fn mutate_upload(
        &self,
        sql: &'static str,
        id: Uuid,
        mutation: Mutation<'_>,
    ) -> Result<ArtifactUploadRecord, StoreError> {
        for attempt in 0..8_u32 {
            let now = Utc::now();
            let mut response = self
                .db
                .query(sql)
                .bind(("upload", upload_record_id(id)))
                .bind(("now", now))
                .bind((
                    "until",
                    now + TimeDelta::seconds(i64::from(mutation.seconds)),
                ))
                .bind(("owner", mutation.owner))
                .bind(("generation", mutation.generation))
                .bind(("operation", mutation.operation.to_owned()))
                .bind(("multipart_id", mutation.multipart_id.map(str::to_owned)))
                .bind(("manifest", mutation.manifest.clone()))
                .bind(("policy_digest", mutation.policy_digest.map(str::to_owned)))
                .bind(("terminal", mutation.terminal))
                .bind(("failure", mutation.failure))
                .await?;
            let Some(error) = crate::store::primary_transaction_error(response.take_errors())
            else {
                return self.required_upload(id).await;
            };
            if attempt < 7 && retryable(&error) {
                tokio::time::sleep(std::time::Duration::from_millis(1_u64 << attempt)).await;
                continue;
            }
            return Err(upload_error(error));
        }
        unreachable!("bounded transaction retry")
    }
}

pub(super) fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn conflict() -> StoreError {
    StoreError::ArtifactUpload(ArtifactUploadRejection::Conflict)
}
fn check_lease_seconds(seconds: u32) -> Result<(), StoreError> {
    if (1..=3600).contains(&seconds) {
        Ok(())
    } else {
        Err(conflict())
    }
}
