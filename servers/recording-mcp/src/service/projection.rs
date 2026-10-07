use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::contract::{
    CreateRecordingProjectionRequest, RecordingProjectionHandle, RecordingProjectionHandleBuilder,
    RecordingProjectionHandleSchema, RecordingProjectionResultMetadata,
};
use anyhow::{Context as _, Result, ensure};
use chrono::{DateTime, Utc};
use sha2::{Digest as _, Sha256};
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::{GatewayInternalIdentity, PlaneCaller};
use veoveo_platform_store::RecordId;
use veoveo_recording_store::{
    RecordingDatasetId, RecordingId, RecordingProjectionReceiptDraft, RecordingProjectionReceiptId,
    RecordingProjectionReceiptRecord, RecordingProjectionRequest, RecordingProjectionState,
    RecordingReadGrantClass, RecordingReadGrantId,
};
use veoveo_rrd::projection::{
    ArrowProjectionQuery, ArrowProjectionSummary, write_arrow_projection_cancelable,
};
use veoveo_types::Sha256Digest;

use super::{RecordingService, record_uuid};

mod scratch;
pub(super) use scratch::ProjectionRuntime;
use scratch::{
    MAX_METADATA_BYTES, ProjectionPaths, read_metadata, remove_projection_paths, verify_file,
    write_metadata,
};
pub use scratch::{ProjectionRuntimeLimits, ProjectionRuntimeStats};

pub struct ProjectionDownload {
    pub path: PathBuf,
    pub byte_len: u64,
    pub sha256: Sha256Digest,
}

impl RecordingService {
    pub fn projection_runtime_stats(&self) -> Result<Option<ProjectionRuntimeStats>> {
        self.projection_runtime
            .as_ref()
            .map(ProjectionRuntime::stats)
            .transpose()
    }

    pub(super) fn projection_runtime_readiness(&self) -> Result<()> {
        self.projection_runtime
            .as_ref()
            .context("recording projection runtime is not configured")?
            .readiness()
    }

    pub async fn create_projection(
        &self,
        identity: &GatewayInternalIdentity,
        artifact_caller: &PlaneCaller,
        request: CreateRecordingProjectionRequest,
        cancellation: CancellationToken,
    ) -> Result<RecordingProjectionHandle> {
        let runtime = self
            .projection_runtime
            .as_ref()
            .context("recording projection runtime is not configured")?;
        ensure!(
            (1..=runtime.maximum_deadline_ms()).contains(&request.deadline_ms),
            "recording projection deadline exceeds the configured maximum"
        );
        let query = ArrowProjectionQuery::new(request.query.clone())?;
        let _permit = runtime.try_acquire()?;
        let dataset_id = RecordingDatasetId::from_uuid(request.dataset_id.as_uuid());
        let recording_id = RecordingId::from_uuid(request.recording_id.as_uuid());
        let plan = self
            .playback_plan(
                identity,
                Some(artifact_caller),
                recording_id,
                super::PlaybackArchiveSelection::Complete,
            )
            .await?
            .context("recording projection source is not visible")?;
        ensure!(
            plan.dataset_id == dataset_id,
            "recording does not belong to the requested dataset"
        );
        ensure!(
            !plan.archive_layers.is_empty(),
            "recording has no committed immutable layers"
        );
        let platform_identity = self.platform_identity(identity).await?;
        let query_digest = projection_query_digest(&request)?;
        let manifest_digest = projection_manifest_digest(&plan);
        let scope = super::grants::recording_access_scope(identity, &platform_identity)?;
        let request_identity = RecordingProjectionRequest::new(
            dataset_id,
            recording_id,
            request.idempotency_key.clone(),
            manifest_digest,
            query_digest.clone(),
        )?;
        let existing = self
            .recordings
            .recording_projection_by_idempotency_key(&scope, &request_identity)
            .await?;
        let receipt = if let Some(existing) = existing {
            existing
        } else {
            let grant = self
                .issue_read_grant(
                    identity,
                    dataset_id,
                    RecordingReadGrantClass::AppProjection,
                    vec![recording_id],
                    plan.catalog_revision.clone(),
                    None,
                )
                .await?;
            let grant_id = RecordingReadGrantId::from_uuid(record_uuid(
                &grant.id,
                RecordingReadGrantId::TABLE,
            )?);
            self.recordings
                .reserve_recording_projection(RecordingProjectionReceiptDraft {
                    scope: scope.clone(),
                    request: request_identity,
                    grant_id,
                    expires_at: grant.expires_at,
                })
                .await?
        };
        let projection_id = projection_id(&receipt.id)?;
        let paths = runtime.paths(projection_id);
        if receipt.state == RecordingProjectionState::Ready {
            return read_handle(&paths, &receipt, &request, HandleReadMode::Ready);
        }
        if receipt.state == RecordingProjectionState::Materializing
            && paths.final_arrow.is_file()
            && paths.final_metadata.is_file()
        {
            let handle = read_handle(&paths, &receipt, &request, HandleReadMode::Recovering)?;
            self.recordings
                .complete_recording_projection(
                    &scope,
                    recording_id,
                    projection_id,
                    handle.result.byte_len.get(),
                    &handle.result.payload_sha256,
                )
                .await?;
            return Ok(handle);
        }
        ensure!(
            receipt.state == RecordingProjectionState::Reserved
                || receipt.state == RecordingProjectionState::Materializing,
            "recording projection is not retryable"
        );
        let reservation = runtime.reserve(
            projection_id,
            request.query.maximum_bytes + MAX_METADATA_BYTES,
        )?;
        self.recordings
            .begin_recording_projection(&scope, recording_id, projection_id)
            .await?;
        remove_projection_paths(&paths, false)?;
        let layer_paths = plan
            .archive_layers
            .iter()
            .map(|layer| layer.cached.path().to_path_buf())
            .collect::<Vec<_>>();
        let partial_arrow = paths.partial_arrow.clone();
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let mut worker = tokio::task::spawn_blocking(move || {
            write_arrow_projection_cancelable(
                &layer_paths,
                &query,
                &partial_arrow,
                worker_cancelled,
            )
        });
        let deadline = tokio::time::sleep(Duration::from_millis(request.deadline_ms));
        tokio::pin!(deadline);
        let (worker_result, terminal) = tokio::select! {
            result = &mut worker => (Some(result), None),
            () = cancellation.cancelled() => (None, Some(ProjectionTerminal::Cancelled)),
            () = &mut deadline => (None, Some(ProjectionTerminal::Deadline)),
        };
        let worker_result = if let Some(result) = worker_result {
            result
        } else {
            cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
            worker.await
        };
        if let Some(terminal) = terminal {
            remove_projection_paths(&paths, true)?;
            match terminal {
                ProjectionTerminal::Cancelled => {
                    self.recordings
                        .cancel_recording_projection(
                            &scope,
                            recording_id,
                            projection_id,
                            "cancelled",
                        )
                        .await?;
                    anyhow::bail!("recording projection was cancelled");
                }
                ProjectionTerminal::Deadline => {
                    self.recordings
                        .fail_recording_projection(
                            &scope,
                            recording_id,
                            projection_id,
                            "deadline_exceeded",
                        )
                        .await?;
                    anyhow::bail!("recording projection deadline exceeded");
                }
            }
        }
        let summary = match worker_result {
            Ok(Ok(summary)) => summary,
            Ok(Err(error)) => {
                remove_projection_paths(&paths, true)?;
                self.recordings
                    .fail_recording_projection(
                        &scope,
                        recording_id,
                        projection_id,
                        "materialization_failed",
                    )
                    .await?;
                return Err(error);
            }
            Err(error) => {
                remove_projection_paths(&paths, true)?;
                self.recordings
                    .fail_recording_projection(&scope, recording_id, projection_id, "worker_failed")
                    .await?;
                return Err(error.into());
            }
        };
        let handle = projection_handle(
            &request,
            projection_id,
            plan.catalog_revision,
            query_digest,
            receipt.expires_at,
            summary,
        );
        let handle = match handle {
            Ok(handle) => handle,
            Err(error) => {
                remove_projection_paths(&paths, true)?;
                self.recordings
                    .fail_recording_projection(
                        &scope,
                        recording_id,
                        projection_id,
                        "invalid_result",
                    )
                    .await?;
                return Err(error);
            }
        };
        write_metadata(&paths.partial_metadata, &handle)?;
        std::fs::rename(&paths.partial_arrow, &paths.final_arrow)?;
        std::fs::rename(&paths.partial_metadata, &paths.final_metadata)?;
        runtime.sync()?;
        reservation.commit(
            handle.result.byte_len.get() + paths.final_metadata.metadata()?.len(),
            handle.expires_at,
        )?;
        self.recordings
            .complete_recording_projection(
                &scope,
                recording_id,
                projection_id,
                handle.result.byte_len.get(),
                &handle.result.payload_sha256,
            )
            .await?;
        Ok(handle)
    }

    pub async fn projection_download(
        &self,
        identity: &GatewayInternalIdentity,
        recording_id: RecordingId,
        projection_id: RecordingProjectionReceiptId,
    ) -> Result<Option<ProjectionDownload>> {
        let runtime = self
            .projection_runtime
            .as_ref()
            .context("recording projection runtime is not configured")?;
        let platform_identity = self.platform_identity(identity).await?;
        let scope = super::grants::recording_access_scope(identity, &platform_identity)?;
        let Some(receipt) = self
            .recordings
            .ready_recording_projection(&scope, recording_id, projection_id)
            .await?
        else {
            return Ok(None);
        };
        let byte_len = u64::try_from(
            receipt
                .result_byte_len
                .context("ready projection has no length")?,
        )?;
        let sha256 = Sha256Digest::from_hex(
            receipt
                .result_sha256
                .as_ref()
                .context("ready projection has no digest")?,
        )?;
        let path = runtime.paths(projection_id).final_arrow;
        let validation_path = path.clone();
        let expected_sha256 = sha256.clone();
        tokio::task::spawn_blocking(move || {
            verify_file(&validation_path, byte_len, &expected_sha256)
        })
        .await??;
        Ok(Some(ProjectionDownload {
            path,
            byte_len,
            sha256,
        }))
    }
}

fn projection_handle(
    request: &CreateRecordingProjectionRequest,
    projection_id: RecordingProjectionReceiptId,
    catalog_revision: String,
    query_digest: Sha256Digest,
    expires_at: DateTime<Utc>,
    summary: ArrowProjectionSummary,
) -> Result<RecordingProjectionHandle> {
    Ok(RecordingProjectionHandleBuilder {
        schema: RecordingProjectionHandleSchema::V2,
        projection_id: crate::contract::RecordingProjectionId::try_from(projection_id.as_uuid())?,
        dataset_id: request.dataset_id,
        recording_id: request.recording_id,
        result: RecordingProjectionResultMetadata {
            catalog_revision,
            query_digest,
            timeline: request.query.timeline.clone(),
            sample_grid: request.query.sampling.sample_grid(),
            units: request.units.clone(),
            coordinate_frame_refs: request.coordinate_frame_refs.clone(),
            omitted_sample_count: summary.omitted_sample_count,
            row_count: summary.row_count,
            arrow_schema_sha256: summary.schema_sha256,
            byte_len: summary.byte_len,
            payload_sha256: summary.sha256,
        },
        expires_at,
    }
    .build_for(request)?)
}

enum ProjectionTerminal {
    Cancelled,
    Deadline,
}

fn projection_query_digest(request: &CreateRecordingProjectionRequest) -> Result<Sha256Digest> {
    Ok(Sha256Digest::from_bytes(
        Sha256::digest(serde_json::to_vec(&request.query_identity())?).into(),
    ))
}

fn projection_manifest_digest(plan: &crate::RecordingPlaybackPlan) -> Sha256Digest {
    let mut digest = Sha256::new();
    digest.update(plan.dataset_id.as_uuid().as_bytes());
    digest.update(plan.recording_id.as_uuid().as_bytes());
    for layer in &plan.archive_layers {
        digest.update(layer.layer_id.as_uuid().as_bytes());
        digest.update(layer.sha256.hex().as_bytes());
    }
    Sha256Digest::from_bytes(digest.finalize().into())
}

fn projection_id(record: &RecordId) -> Result<RecordingProjectionReceiptId> {
    Ok(RecordingProjectionReceiptId::from_uuid(record_uuid(
        record,
        RecordingProjectionReceiptId::TABLE,
    )?))
}
enum HandleReadMode {
    Ready,
    Recovering,
}

fn read_handle(
    paths: &ProjectionPaths,
    receipt: &RecordingProjectionReceiptRecord,
    request: &CreateRecordingProjectionRequest,
    mode: HandleReadMode,
) -> Result<RecordingProjectionHandle> {
    let expected = match mode {
        HandleReadMode::Ready => RecordingProjectionState::Ready,
        HandleReadMode::Recovering => RecordingProjectionState::Materializing,
    };
    ensure!(
        receipt.state == expected,
        "projection receipt has an unexpected state"
    );
    let handle = read_metadata(&paths.final_metadata)?;
    handle.validate_request(request)?;
    ensure!(
        handle.projection_id.as_uuid() == projection_id(&receipt.id)?.as_uuid()
            && handle.result.catalog_revision == receipt.catalog_revision
            && handle.result.query_digest == Sha256Digest::from_hex(&receipt.query_digest)?
            && handle.result.query_digest == projection_query_digest(request)?
            && handle.expires_at == receipt.expires_at,
        "projection metadata does not match its receipt"
    );
    verify_file(
        &paths.final_arrow,
        handle.result.byte_len.get(),
        &handle.result.payload_sha256,
    )?;
    if matches!(mode, HandleReadMode::Ready) {
        ensure!(
            receipt.result_byte_len == Some(i64::try_from(handle.result.byte_len.get())?)
                && receipt.result_sha256.as_deref() == Some(handle.result.payload_sha256.hex()),
            "projection result does not match its ready receipt"
        );
    }
    Ok(handle)
}

#[cfg(test)]
#[path = "projection/tests.rs"]
mod tests;
