use std::collections::BTreeSet;
use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use chrono::Utc;
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_mcp_contract::{
    ArtifactPlane, DataLabelId, GatewayInternalIdentity, PlaneCaller, PutArtifactRequest,
};
use veoveo_platform_store::{
    ArtifactId as PlatformArtifactId, PlatformIdentity, PlatformStore, PrincipalKind, RecordId,
    RecordIdKey, RecordingId, RecordingRecord, RecordingSeal, RecordingState, SegmentId,
    SegmentRecord, SegmentSealBinding, SegmentState,
};
use veoveo_recording_hub::{inspect_segment, query_segments};

use crate::contract::{
    ManifestSegment, PlaybackManifest, PlaybackSegment, QueryRecordingOutput,
    QueryRecordingRequest, RecordingManifest, RecordingView, SealRecordingOutput, SegmentView,
};

mod read;
pub use read::{RecordingReadAuthority, RecordingReadPlan, RecordingReadSegment};

const MAX_QUERY_ROWS: u64 = 10_000;
const MAX_SEGMENTS: u32 = 10_000;
const RRD_MIME: &str = "application/vnd.rerun.rrd";
const MANIFEST_MIME: &str = "application/vnd.veoveo.recording-manifest+json";

#[derive(Clone)]
pub struct RecordingService {
    store: PlatformStore,
    artifacts: HttpArtifactPlane,
    spool_root: PathBuf,
}

impl RecordingService {
    pub fn new(
        store: PlatformStore,
        artifacts: HttpArtifactPlane,
        spool_root: PathBuf,
    ) -> Result<Self> {
        ensure!(
            spool_root.is_absolute(),
            "recording spool root must be absolute"
        );
        let spool_root = spool_root
            .canonicalize()
            .with_context(|| format!("canonicalizing spool root {}", spool_root.display()))?;
        Ok(Self {
            store,
            artifacts,
            spool_root,
        })
    }

    pub fn platform_store(&self) -> &PlatformStore {
        &self.store
    }

    pub async fn platform_identity(
        &self,
        identity: &GatewayInternalIdentity,
    ) -> Result<PlatformIdentity> {
        let tenant_key = identity
            .principal
            .tenant
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "installation".to_owned());
        Ok(self
            .store
            .ensure_identity(
                &tenant_key,
                identity.principal.id.as_str(),
                identity.principal.issuer.as_str(),
                identity.principal.subject.as_str(),
                match identity.principal.kind {
                    veoveo_mcp_contract::PrincipalKind::User => PrincipalKind::User,
                    veoveo_mcp_contract::PrincipalKind::Service => PrincipalKind::Service,
                },
            )
            .await?)
    }

    pub async fn list_visible(
        &self,
        identity: &GatewayInternalIdentity,
    ) -> Result<Vec<RecordingView>> {
        let platform_identity = self.platform_identity(identity).await?;
        let mut views = Vec::new();
        for recording in self
            .store
            .list_recordings(platform_identity.tenant_id, 500)
            .await?
        {
            if visible(&recording, identity) {
                views.push(self.view(platform_identity.tenant_id, recording).await?);
            }
        }
        Ok(views)
    }

    pub async fn visible_recording(
        &self,
        identity: &GatewayInternalIdentity,
        recording_id: RecordingId,
    ) -> Result<Option<(PlatformIdentity, RecordingRecord)>> {
        let platform_identity = self.platform_identity(identity).await?;
        let recording = self
            .store
            .recording(platform_identity.tenant_id, recording_id)
            .await?;
        Ok(recording
            .filter(|recording| visible(recording, identity))
            .map(|recording| (platform_identity, recording)))
    }

    pub async fn segment_views(
        &self,
        identity: &GatewayInternalIdentity,
        recording_id: RecordingId,
    ) -> Result<Option<Vec<SegmentView>>> {
        let Some((platform_identity, _)) = self.visible_recording(identity, recording_id).await?
        else {
            return Ok(None);
        };
        let segments = self
            .store
            .recording_segments(platform_identity.tenant_id, recording_id, MAX_SEGMENTS)
            .await?;
        Ok(Some(
            segments
                .iter()
                .map(segment_view)
                .collect::<Result<Vec<_>>>()?,
        ))
    }

    pub async fn recording_view(
        &self,
        identity: &GatewayInternalIdentity,
        recording_id: RecordingId,
    ) -> Result<Option<RecordingView>> {
        let Some((platform_identity, recording)) =
            self.visible_recording(identity, recording_id).await?
        else {
            return Ok(None);
        };
        Ok(Some(
            self.view(platform_identity.tenant_id, recording).await?,
        ))
    }

    pub async fn playback_manifest(
        &self,
        identity: &GatewayInternalIdentity,
        recording_id: RecordingId,
    ) -> Result<Option<PlaybackManifest>> {
        let authority = RecordingReadAuthority::from_gateway(identity);
        let Some(plan) = self.read_plan(&authority, recording_id).await? else {
            return Ok(None);
        };
        let Some((_, recording)) = self.visible_recording(identity, recording_id).await? else {
            return Ok(None);
        };
        let segments = plan
            .segments
            .into_iter()
            .filter(|segment| matches!(segment.state, SegmentState::Frozen | SegmentState::Sealed))
            .map(|segment| {
                Ok(PlaybackSegment {
                    segment_id: segment.segment_id.to_string(),
                    ordinal: segment.ordinal,
                    byte_len: segment.byte_len,
                    sha256: segment
                        .sha256
                        .context("playback segment is missing sha256")?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Some(PlaybackManifest {
            recording_id: recording_id.to_string(),
            application_id: plan.application_id,
            recording_key: plan.recording_key,
            state: recording_state(plan.state).to_owned(),
            started_at: recording.started_at.to_rfc3339(),
            ended_at: recording.ended_at.map(|value| value.to_rfc3339()),
            segments,
        }))
    }

    pub async fn playback_segment_path(
        &self,
        identity: &GatewayInternalIdentity,
        recording_id: RecordingId,
        segment_id: SegmentId,
    ) -> Result<Option<PathBuf>> {
        let authority = RecordingReadAuthority::from_gateway(identity);
        let Some(plan) = self.read_plan(&authority, recording_id).await? else {
            return Ok(None);
        };
        Ok(plan
            .segments
            .into_iter()
            .find(|segment| {
                segment.segment_id == segment_id
                    && matches!(segment.state, SegmentState::Frozen | SegmentState::Sealed)
            })
            .map(|segment| segment.path))
    }

    pub async fn query(
        &self,
        identity: &GatewayInternalIdentity,
        request: QueryRecordingRequest,
    ) -> Result<QueryRecordingOutput> {
        ensure!(
            request.max_rows > 0 && request.max_rows <= MAX_QUERY_ROWS,
            "max_rows must be in 1..={MAX_QUERY_ROWS}"
        );
        ensure!(
            !request.timeline.trim().is_empty() && request.timeline.len() <= 256,
            "timeline must be 1-256 characters"
        );
        ensure!(
            !request.entities.trim().is_empty() && request.entities.len() <= 4_096,
            "entities must be 1-4096 characters"
        );
        let recording_id = parse_recording_id(&request.recording_id)?;
        let Some((platform_identity, _)) = self.visible_recording(identity, recording_id).await?
        else {
            anyhow::bail!("recording not found");
        };
        let segments = self
            .store
            .recording_segments(platform_identity.tenant_id, recording_id, MAX_SEGMENTS)
            .await?;
        let paths = segments
            .iter()
            .map(|segment| self.segment_path(&segment.relative_path))
            .collect::<Result<Vec<_>>>()?;
        let entities = request.entities.clone();
        let timeline = request.timeline.clone();
        let max_rows = request.max_rows;
        let result = tokio::task::spawn_blocking(move || {
            query_segments(&paths, &entities, &timeline, max_rows)
        })
        .await
        .context("recording query worker panicked")??;
        Ok(QueryRecordingOutput {
            recording_id: recording_id.to_string(),
            timeline: request.timeline,
            rows: result.rows,
            rows_by_recording: result.rows_by_recording,
        })
    }

    pub async fn seal(
        &self,
        identity: &GatewayInternalIdentity,
        caller: &PlaneCaller,
        recording_id: RecordingId,
    ) -> Result<SealRecordingOutput> {
        ensure_seal_scope(identity)?;
        let Some((platform_identity, recording)) =
            self.visible_recording(identity, recording_id).await?
        else {
            anyhow::bail!("recording not found");
        };
        if recording.state == RecordingState::Sealed {
            return self.sealed_output(&platform_identity, recording).await;
        }
        ensure!(
            matches!(
                recording.state,
                RecordingState::Ready | RecordingState::Interrupted | RecordingState::Sealing
            ),
            "recording is not sealable from state {}",
            recording_state(recording.state)
        );
        let segments = self
            .store
            .recording_segments(platform_identity.tenant_id, recording_id, MAX_SEGMENTS)
            .await?;
        ensure!(!segments.is_empty(), "recording has no segments");
        for segment in &segments {
            ensure!(
                segment.state == SegmentState::Frozen,
                "segment {} is not frozen",
                record_uuid(&segment.id, "segment")?
            );
            self.validate_segment(segment).await?;
        }

        self.store
            .begin_recording_seal(&platform_identity, recording_id, None)
            .await?;
        let mut manifest_segments = Vec::with_capacity(segments.len());
        let mut bindings = Vec::with_capacity(segments.len());
        for segment in &segments {
            let segment_id = SegmentId::from_uuid(record_uuid(&segment.id, "segment")?);
            let (artifact_id, artifact_uri) = if let Some(artifact) = &segment.artifact {
                let id =
                    PlatformArtifactId::from_uuid(record_uuid(artifact, "artifact_occurrence")?);
                (id, artifact_uri(id))
            } else {
                let path = self.segment_path(&segment.relative_path)?;
                let bytes = tokio::fs::read(&path)
                    .await
                    .with_context(|| format!("reading frozen segment {}", path.display()))?;
                let metadata = self
                    .artifacts
                    .put(
                        caller,
                        PutArtifactRequest {
                            mime_type: Some(RRD_MIME.to_owned()),
                            filename: path
                                .file_name()
                                .and_then(|value| value.to_str())
                                .map(str::to_owned),
                            classification: artifact_classification(&recording.classification)?,
                            data_labels: labels(&recording.labels)?,
                            retention_expires_at: None,
                            metadata: serde_json::json!({
                                "provenance": {
                                    "kind": "recording_segment",
                                    "recording_id": recording_id,
                                    "segment_id": segment_id,
                                    "ordinal": segment.ordinal,
                                    "sha256": segment.sha256,
                                    "application_id": recording.application_id,
                                    "recording_key": recording.recording_key,
                                }
                            }),
                        },
                        bytes,
                    )
                    .await?;
                let artifact_id = PlatformArtifactId::from_uuid(metadata.artifact_id.as_uuid());
                self.store
                    .stage_segment_artifact(
                        &platform_identity,
                        recording_id,
                        segment_id,
                        artifact_id,
                    )
                    .await?;
                (artifact_id, metadata.artifact_uri)
            };
            bindings.push(SegmentSealBinding {
                segment_id,
                artifact_id,
            });
            manifest_segments.push(ManifestSegment {
                segment_id: segment_id.to_string(),
                ordinal: segment.ordinal,
                byte_len: segment.byte_len,
                sha256: segment
                    .sha256
                    .clone()
                    .context("frozen segment is missing sha256")?,
                artifact_uri,
            });
        }

        let sealed_at = Utc::now();
        let current = self
            .store
            .recording(platform_identity.tenant_id, recording_id)
            .await?
            .context("recording disappeared while sealing")?;
        let manifest_artifact_id = if let Some(record) = current.manifest_artifact {
            PlatformArtifactId::from_uuid(record_uuid(&record, "artifact_occurrence")?)
        } else {
            let recording_view = self.view(platform_identity.tenant_id, current).await?;
            let manifest = RecordingManifest {
                schema: "veoveo.recording-manifest/v1".to_owned(),
                recording: recording_view,
                segments: manifest_segments.clone(),
                sealed_at: sealed_at.to_rfc3339(),
            };
            let bytes = serde_json::to_vec_pretty(&manifest)?;
            let metadata = self
                .artifacts
                .put(
                    caller,
                    PutArtifactRequest {
                        mime_type: Some(MANIFEST_MIME.to_owned()),
                        filename: Some(format!("{}.recording.json", recording.recording_key)),
                        classification: artifact_classification(&recording.classification)?,
                        data_labels: labels(&recording.labels)?,
                        retention_expires_at: None,
                        metadata: serde_json::json!({
                            "provenance": {
                                "kind": "recording_manifest",
                                "recording_id": recording_id,
                                "application_id": recording.application_id,
                                "recording_key": recording.recording_key,
                            }
                        }),
                    },
                    bytes,
                )
                .await?;
            let artifact_id = PlatformArtifactId::from_uuid(metadata.artifact_id.as_uuid());
            self.store
                .stage_recording_manifest(&platform_identity, recording_id, artifact_id)
                .await?;
            artifact_id
        };
        self.store
            .complete_recording_seal(RecordingSeal {
                identity: platform_identity,
                recording_id,
                task_id: None,
                manifest_artifact_id,
                segments: bindings,
                sealed_at,
            })
            .await?;
        Ok(SealRecordingOutput {
            recording_id: recording_id.to_string(),
            manifest_artifact_uri: artifact_uri(manifest_artifact_id),
            segment_artifact_uris: manifest_segments
                .into_iter()
                .map(|segment| segment.artifact_uri)
                .collect(),
        })
    }

    async fn sealed_output(
        &self,
        identity: &PlatformIdentity,
        recording: RecordingRecord,
    ) -> Result<SealRecordingOutput> {
        let recording_id = RecordingId::from_uuid(record_uuid(&recording.id, "recording")?);
        let manifest = recording
            .manifest_artifact
            .as_ref()
            .context("sealed recording has no manifest artifact")?;
        let segments = self
            .store
            .recording_segments(identity.tenant_id, recording_id, MAX_SEGMENTS)
            .await?;
        let segment_artifact_uris = segments
            .iter()
            .map(|segment| {
                let artifact = segment
                    .artifact
                    .as_ref()
                    .context("sealed segment has no artifact")?;
                Ok(artifact_uri(PlatformArtifactId::from_uuid(record_uuid(
                    artifact,
                    "artifact_occurrence",
                )?)))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(SealRecordingOutput {
            recording_id: recording_id.to_string(),
            manifest_artifact_uri: artifact_uri(PlatformArtifactId::from_uuid(record_uuid(
                manifest,
                "artifact_occurrence",
            )?)),
            segment_artifact_uris,
        })
    }

    async fn view(
        &self,
        tenant_id: veoveo_platform_store::TenantId,
        recording: RecordingRecord,
    ) -> Result<RecordingView> {
        let recording_id = RecordingId::from_uuid(record_uuid(&recording.id, "recording")?);
        let segment_count = self
            .store
            .recording_segments(tenant_id, recording_id, MAX_SEGMENTS)
            .await?
            .len();
        Ok(RecordingView {
            recording_id: recording_id.to_string(),
            dataset: recording.dataset,
            application_id: recording.application_id,
            recording_key: recording.recording_key,
            state: recording_state(recording.state).to_owned(),
            classification: recording.classification,
            labels: recording.labels,
            started_at: recording.started_at.to_rfc3339(),
            last_data_at: recording.last_data_at.to_rfc3339(),
            ended_at: recording.ended_at.map(|value| value.to_rfc3339()),
            sealed_at: recording.sealed_at.map(|value| value.to_rfc3339()),
            manifest_artifact_uri: recording.manifest_artifact.map(|record| {
                artifact_uri(PlatformArtifactId::from_uuid(
                    record_uuid(&record, "artifact_occurrence")
                        .expect("validated platform artifact record"),
                ))
            }),
            segment_count,
        })
    }

    async fn validate_segment(&self, segment: &SegmentRecord) -> Result<()> {
        let path = self.segment_path(&segment.relative_path)?;
        let inspection = tokio::task::spawn_blocking(move || inspect_segment(&path))
            .await
            .context("segment validation worker panicked")??;
        ensure!(
            i64::try_from(inspection.byte_len).ok() == Some(segment.byte_len),
            "segment byte length no longer matches catalog"
        );
        ensure!(
            segment.sha256.as_deref() == Some(inspection.sha256.as_str()),
            "segment sha256 no longer matches catalog"
        );
        Ok(())
    }

    fn segment_path(&self, relative: &str) -> Result<PathBuf> {
        let path = self.spool_root.join(relative);
        let canonical = path
            .canonicalize()
            .with_context(|| format!("canonicalizing segment {}", path.display()))?;
        ensure!(
            canonical.starts_with(&self.spool_root) && canonical.is_file(),
            "segment path escapes the configured spool root"
        );
        Ok(canonical)
    }
}

fn visible(recording: &RecordingRecord, identity: &GatewayInternalIdentity) -> bool {
    labels_visible(
        recording,
        identity
            .principal
            .data_labels
            .iter()
            .map(|label| label.as_str()),
    )
}

fn labels_visible<'a>(
    recording: &RecordingRecord,
    clearance: impl IntoIterator<Item = &'a str>,
) -> bool {
    let clearance: BTreeSet<&str> = clearance.into_iter().collect();
    recording
        .labels
        .iter()
        .all(|label| clearance.contains(label.as_str()))
}

fn ensure_seal_scope(identity: &GatewayInternalIdentity) -> Result<()> {
    ensure!(
        identity
            .principal
            .scopes
            .iter()
            .any(|scope| scope.as_str() == "admin:manage"),
        "admin:manage scope is required to seal recordings"
    );
    Ok(())
}

fn labels(values: &[String]) -> Result<BTreeSet<DataLabelId>> {
    values
        .iter()
        .map(|value| label(value))
        .collect::<Result<BTreeSet<_>>>()
}

fn label(value: &str) -> Result<DataLabelId> {
    DataLabelId::new(value.to_owned()).map_err(Into::into)
}

fn artifact_classification(value: &str) -> Result<Option<DataLabelId>> {
    if value == "unclassified" {
        Ok(None)
    } else {
        label(value).map(Some)
    }
}

fn segment_view(segment: &SegmentRecord) -> Result<SegmentView> {
    Ok(SegmentView {
        segment_id: record_uuid(&segment.id, "segment")?.to_string(),
        ordinal: segment.ordinal,
        state: segment_state(segment.state).to_owned(),
        byte_len: segment.byte_len,
        message_count: segment.message_count,
        sha256: segment.sha256.clone(),
        artifact_uri: segment.artifact.as_ref().map(|artifact| {
            artifact_uri(PlatformArtifactId::from_uuid(
                record_uuid(artifact, "artifact_occurrence")
                    .expect("validated platform artifact record"),
            ))
        }),
        created_at: segment.created_at.to_rfc3339(),
        updated_at: segment.updated_at.to_rfc3339(),
    })
}

fn parse_recording_id(value: &str) -> Result<RecordingId> {
    let value = uuid::Uuid::parse_str(value).context("recording_id must be a UUIDv7")?;
    ensure!(
        value.get_version_num() == 7,
        "recording_id must be a UUIDv7"
    );
    Ok(RecordingId::from_uuid(value))
}

fn record_uuid(record: &RecordId, table: &str) -> Result<uuid::Uuid> {
    ensure!(
        record.table.as_str() == table,
        "record has unexpected table"
    );
    let raw = match &record.key {
        RecordIdKey::Uuid(value) => value.to_string(),
        RecordIdKey::String(value) => value.clone(),
        other => anyhow::bail!("record key is not UUID: {other:?}"),
    };
    let value = uuid::Uuid::parse_str(&raw)?;
    ensure!(value.get_version_num() == 7, "record key is not UUIDv7");
    Ok(value)
}

fn artifact_uri(id: PlatformArtifactId) -> String {
    format!("artifact://{id}")
}

fn recording_state(state: RecordingState) -> &'static str {
    match state {
        RecordingState::Live => "live",
        RecordingState::Ready => "ready",
        RecordingState::Sealing => "sealing",
        RecordingState::Sealed => "sealed",
        RecordingState::Interrupted => "interrupted",
        RecordingState::Failed => "failed",
    }
}

fn segment_state(state: SegmentState) -> &'static str {
    match state {
        SegmentState::Writing => "writing",
        SegmentState::Frozen => "frozen",
        SegmentState::Sealed => "sealed",
        SegmentState::Failed => "failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_recording_ids_require_uuid_v7() {
        assert!(parse_recording_id(&uuid::Uuid::now_v7().to_string()).is_ok());
        assert!(parse_recording_id(&uuid::Uuid::new_v4().to_string()).is_err());
    }
}
