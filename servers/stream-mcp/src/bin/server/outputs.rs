use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result};
use rmcp::model::{CallToolResult, ContentBlock, Resource};
use serde::Serialize;
use veoveo_mcp_contract::{
    ArtifactPut, ArtifactWriteIdempotencyKey, ComplianceMetadata, DataLabelId,
    IssuedArtifactWriteCapability, now_utc,
};
use veoveo_platform_store::{DomainUsageDraft, DomainUsageKind, OpenObject};
use veoveo_recording_video::MaterializedVideo;
use veoveo_stream_mcp::{
    annotation::{MP4_MIME_TYPE, RESULTS_MIME_TYPE, RRD_MIME_TYPE},
    contract::{AnalysisResults, AnalysisSummary, RunRecordingOutput},
    uris,
};
use veoveo_task_runtime::TaskId;

use super::app_state::AppState;

pub(super) struct AnalysisProducts {
    pub(super) results: AnalysisResults,
    pub(super) annotations_rrd: Vec<u8>,
    pub(super) source: MaterializedVideo,
    pub(super) include_source_clip: bool,
}

pub(super) async fn publish_analysis(
    state: &AppState,
    capability: &IssuedArtifactWriteCapability,
    task_id: &str,
    products: AnalysisProducts,
) -> Result<CallToolResult> {
    let compliance = compliance(&products.source.classification, &products.source.labels)?;
    let source_snapshot_sha256 = products.results.source_snapshot.digest_sha256()?;
    let results_bytes = serde_json::to_vec_pretty(&products.results)?;
    let results_artifact = put(
        state,
        capability,
        task_id,
        "results",
        results_bytes,
        RESULTS_MIME_TYPE,
        format!("{task_id}.stream.json"),
        compliance.clone(),
        artifact_metadata(StreamArtifactProvenance::Results {
            run_id: task_id.to_owned(),
            recording_id: products.source.recording_id.to_string(),
            pipeline_id: products.results.pipeline_id.clone(),
            model_id: products.results.model_id.clone(),
            source_snapshot_sha256: source_snapshot_sha256.clone(),
        })?,
    )
    .await?;
    let annotations_artifact = put(
        state,
        capability,
        task_id,
        "annotations",
        products.annotations_rrd,
        RRD_MIME_TYPE,
        format!("{task_id}.annotations.rrd"),
        compliance.clone(),
        artifact_metadata(StreamArtifactProvenance::AnnotationLayer {
            run_id: task_id.to_owned(),
            recording_id: products.source.recording_id.to_string(),
            results_artifact_uri: results_artifact.artifact_uri.clone(),
            source_snapshot_sha256: source_snapshot_sha256.clone(),
        })?,
    )
    .await?;
    let source_clip_artifact = if products.include_source_clip {
        Some(
            put(
                state,
                capability,
                task_id,
                "source-clip",
                products.source.mp4,
                MP4_MIME_TYPE,
                format!("{task_id}.source.mp4"),
                compliance,
                artifact_metadata(StreamArtifactProvenance::SourceClip {
                    run_id: task_id.to_owned(),
                    recording_id: products.source.recording_id.to_string(),
                    entity_path: products.results.entity_path.clone(),
                    timeline: products.results.timeline.clone(),
                    decode_start_index: products.source.clip.decode_start_index,
                    source_snapshot_sha256,
                })?,
            )
            .await?,
        )
    } else {
        None
    };
    record_usage(state, task_id, &products.results).await?;
    let detection_count = products
        .results
        .frames
        .iter()
        .map(|frame| frame.detections.len() as u64)
        .sum();
    let output = RunRecordingOutput {
        run_uri: uris::run_uri(task_id),
        results_uri: uris::results_uri(task_id),
        pipeline_uri: uris::pipeline_uri(&products.results.pipeline_id),
        model_uri: uris::model_uri(&products.results.model_id),
        summary: AnalysisSummary {
            processed_frames: products.results.processed_frames,
            detection_count,
            elapsed_ms: products.results.elapsed_ms,
            decode_start_index: products.source.clip.decode_start_index,
            requested_start_index: products.source.clip.requested_start_index,
            requested_end_index: products.source.clip.requested_end_index,
        },
        results_artifact: results_artifact.clone(),
        annotations_artifact: annotations_artifact.clone(),
        source_clip_artifact: source_clip_artifact.clone(),
    };
    let mut blocks = vec![ContentBlock::text(format!(
        "stream recording run completed: {} frame(s), {detection_count} detection(s)",
        products.results.processed_frames
    ))];
    blocks.push(resource_link(
        &results_artifact.artifact_uri,
        "Stream results",
        RESULTS_MIME_TYPE,
    ));
    blocks.push(resource_link(
        &annotations_artifact.artifact_uri,
        "Rerun annotation layer",
        RRD_MIME_TYPE,
    ));
    if let Some(artifact) = &source_clip_artifact {
        blocks.push(resource_link(
            &artifact.artifact_uri,
            "Stream source clip",
            MP4_MIME_TYPE,
        ));
    }
    let mut result = CallToolResult::success(blocks);
    result.structured_content = Some(serde_json::to_value(output)?);
    Ok(result)
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct StreamArtifactMetadata {
    provenance: StreamArtifactProvenance,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum StreamArtifactProvenance {
    #[serde(rename = "stream_results")]
    Results {
        run_id: String,
        recording_id: String,
        pipeline_id: String,
        model_id: String,
        source_snapshot_sha256: String,
    },
    #[serde(rename = "stream_annotation_layer")]
    AnnotationLayer {
        run_id: String,
        recording_id: String,
        results_artifact_uri: String,
        source_snapshot_sha256: String,
    },
    #[serde(rename = "stream_source_clip")]
    SourceClip {
        run_id: String,
        recording_id: String,
        entity_path: String,
        timeline: String,
        decode_start_index: i64,
        source_snapshot_sha256: String,
    },
}

fn artifact_metadata(provenance: StreamArtifactProvenance) -> Result<serde_json::Value> {
    Ok(serde_json::to_value(StreamArtifactMetadata { provenance })?)
}

#[allow(clippy::too_many_arguments)]
async fn put(
    state: &AppState,
    capability: &IssuedArtifactWriteCapability,
    task_id: &str,
    kind: &str,
    bytes: Vec<u8>,
    mime_type: &str,
    filename: String,
    compliance: ComplianceMetadata,
    metadata: serde_json::Value,
) -> Result<veoveo_mcp_contract::ArtifactMetadata> {
    let mut artifact = ArtifactPut::new(bytes);
    artifact.mime_type = Some(mime_type.to_owned());
    artifact.filename = Some(filename);
    artifact.compliance = compliance;
    artifact.metadata = metadata;
    state
        .artifacts
        .put_with_capability(
            capability,
            ArtifactWriteIdempotencyKey::new(format!("stream:{task_id}:{kind}"))?,
            artifact,
        )
        .await
        .with_context(|| format!("publishing `{kind}` Stream artifact"))
}

fn compliance(classification: &str, labels: &[String]) -> Result<ComplianceMetadata> {
    Ok(ComplianceMetadata {
        classification: (classification != "unclassified")
            .then(|| DataLabelId::new(classification.to_owned()))
            .transpose()?,
        data_labels: labels
            .iter()
            .cloned()
            .map(DataLabelId::new)
            .collect::<Result<BTreeSet<_>, _>>()?,
        ..Default::default()
    })
}

fn resource_link(uri: &str, title: &str, mime_type: &str) -> ContentBlock {
    ContentBlock::ResourceLink(
        Resource::new(uri.to_owned(), title.to_owned())
            .with_title(title.to_owned())
            .with_mime_type(mime_type),
    )
}

async fn record_usage(state: &AppState, task_id: &str, results: &AnalysisResults) -> Result<()> {
    state
        .tasks
        .platform_store()
        .upsert_domain_usage(DomainUsageDraft {
            task_id: task_id.parse::<TaskId>()?,
            server: "stream".to_owned(),
            source_id: Some(results.recording_uri.clone()),
            provider_job_id: None,
            model_id: results.model_id.clone(),
            kind: DomainUsageKind::Actual,
            quantity: Some(results.processed_frames as f64),
            unit: Some("decoded_frame".to_owned()),
            amount: None,
            currency: None,
            recorded_at: now_utc(),
            metadata: OpenObject::new(BTreeMap::from([
                ("pipeline_id".into(), serde_json::json!(results.pipeline_id)),
                ("entity_path".into(), serde_json::json!(results.entity_path)),
                ("timeline".into(), serde_json::json!(results.timeline)),
            ])),
        })
        .await
        .context("recording stream usage")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use veoveo_mcp_contract::{MAX_ARTIFACT_PUT_DESCRIPTOR_BYTES, PutArtifactRequest};

    #[test]
    fn artifact_descriptors_reference_bounded_snapshot_digests() {
        let digest = "a".repeat(64);
        let variants = [
            artifact_metadata(StreamArtifactProvenance::Results {
                run_id: "019fa7ee-4191-73e1-b084-2341d4900a06".to_owned(),
                recording_id: "019fa7e9-d7c6-7fe1-bdff-0a5313586c3c".to_owned(),
                pipeline_id: "uav-primary-detection".to_owned(),
                model_id: "primary-detector".to_owned(),
                source_snapshot_sha256: digest.clone(),
            })
            .unwrap(),
            artifact_metadata(StreamArtifactProvenance::AnnotationLayer {
                run_id: "019fa7ee-4191-73e1-b084-2341d4900a06".to_owned(),
                recording_id: "019fa7e9-d7c6-7fe1-bdff-0a5313586c3c".to_owned(),
                results_artifact_uri: "stream://artifact/019fa7ee-4191-73e1-b084-2341d4900a07"
                    .to_owned(),
                source_snapshot_sha256: digest.clone(),
            })
            .unwrap(),
            artifact_metadata(StreamArtifactProvenance::SourceClip {
                run_id: "019fa7ee-4191-73e1-b084-2341d4900a06".to_owned(),
                recording_id: "019fa7e9-d7c6-7fe1-bdff-0a5313586c3c".to_owned(),
                entity_path: "/uav/camera/primary".to_owned(),
                timeline: "simulation_time".to_owned(),
                decode_start_index: 41_296_000_000,
                source_snapshot_sha256: digest,
            })
            .unwrap(),
        ];

        for metadata in variants {
            let request = PutArtifactRequest {
                mime_type: Some("application/octet-stream".to_owned()),
                filename: Some("artifact.bin".to_owned()),
                classification: None,
                data_labels: BTreeSet::new(),
                retention_expires_at: None,
                metadata,
            };
            assert!(
                serde_json::to_vec(&request).unwrap().len() < MAX_ARTIFACT_PUT_DESCRIPTOR_BYTES
            );
        }
    }
}
