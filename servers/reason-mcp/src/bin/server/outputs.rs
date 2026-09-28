use std::collections::{BTreeMap, BTreeSet};
use veoveo_reason_mcp::contract::{AnalysisId, ModelId, PipelineId};

use anyhow::{Context, Result};
use rmcp::model::{CallToolResult, ContentBlock, Resource};
use serde::Serialize;
use veoveo_artifact_contract::{ArtifactPut, ComplianceMetadata};
use veoveo_mcp_contract::{ArtifactWriteIdempotencyKey, IssuedArtifactWriteCapability, now_utc};
use veoveo_platform_store::{DomainUsageDraft, DomainUsageKind, OpenObject};
use veoveo_reason_mcp::{
    annotation::{MP4_MIME_TYPE, RESULTS_MIME_TYPE, RRD_MIME_TYPE},
    contract::{AnalyzeRecordingOutput, ReasoningResults, ReasoningSummary},
};
use veoveo_recording_video::runtime::MaterializedVideo;
use veoveo_types::DataLabelId;

use super::app_state::AppState;

pub(super) struct AnalysisProducts {
    pub(super) results: ReasoningResults,
    pub(super) annotations_rrd: Vec<u8>,
    pub(super) source: MaterializedVideo,
    pub(super) include_source_clip: bool,
}

pub(super) async fn publish_analysis(
    state: &AppState,
    capability: &IssuedArtifactWriteCapability,
    task_id: AnalysisId,
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
        format!("{task_id}.reason.json"),
        compliance.clone(),
        artifact_metadata(ReasonArtifactProvenance::Results {
            analysis_id: task_id,
            recording_id: products.source.recording_id.to_string(),
            pipeline_id: products.results.pipeline_id.clone(),
            model_id: products.results.model_id.clone(),
            prompt_revision: products.results.prompt_revision.clone(),
            task_kind: products.results.task.kind().to_owned(),
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
        artifact_metadata(ReasonArtifactProvenance::AnnotationLayer {
            analysis_id: task_id,
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
                artifact_metadata(ReasonArtifactProvenance::SourceClip {
                    analysis_id: task_id,
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
    let event_count = products.results.answer.event_count();
    let output = AnalyzeRecordingOutput::new(
        task_id,
        products.results.pipeline_id.clone(),
        products.results.model_id.clone(),
        ReasoningSummary {
            observed_frames: products.results.observed_frames,
            event_count,
            elapsed_ms: products.results.elapsed_ms,
            decode_start_index: products.source.clip.decode_start_index,
            requested_start_index: products.source.clip.requested_start_index,
            requested_end_index: products.source.clip.requested_end_index,
        },
        results_artifact.clone(),
        annotations_artifact.clone(),
    )
    .with_source_clip(source_clip_artifact.clone());
    let mut blocks = vec![ContentBlock::text(format!(
        "reason analysis completed: {} `{}` over {} observed frame(s)",
        products.results.task.kind(),
        products.results.pipeline_id,
        products.results.observed_frames,
    ))];
    blocks.push(resource_link(
        &results_artifact.artifact_uri,
        "Reason results",
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
            "Reason source clip",
            MP4_MIME_TYPE,
        ));
    }
    let mut result = CallToolResult::success(blocks);
    result.structured_content = Some(serde_json::to_value(output)?);
    Ok(result)
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct ReasonArtifactMetadata {
    provenance: ReasonArtifactProvenance,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ReasonArtifactProvenance {
    #[serde(rename = "reason_results")]
    Results {
        analysis_id: AnalysisId,
        recording_id: String,
        pipeline_id: PipelineId,
        model_id: ModelId,
        prompt_revision: String,
        task_kind: String,
        source_snapshot_sha256: String,
    },
    #[serde(rename = "reason_annotation_layer")]
    AnnotationLayer {
        analysis_id: AnalysisId,
        recording_id: String,
        results_artifact_uri: veoveo_artifact_contract::ArtifactUri,
        source_snapshot_sha256: String,
    },
    #[serde(rename = "reason_source_clip")]
    SourceClip {
        analysis_id: AnalysisId,
        recording_id: String,
        entity_path: String,
        timeline: String,
        decode_start_index: i64,
        source_snapshot_sha256: String,
    },
}

fn artifact_metadata(provenance: ReasonArtifactProvenance) -> Result<serde_json::Value> {
    Ok(serde_json::to_value(ReasonArtifactMetadata { provenance })?)
}

#[allow(clippy::too_many_arguments)]
async fn put(
    state: &AppState,
    capability: &IssuedArtifactWriteCapability,
    task_id: AnalysisId,
    kind: &str,
    bytes: Vec<u8>,
    mime_type: &str,
    filename: String,
    compliance: ComplianceMetadata,
    metadata: serde_json::Value,
) -> Result<veoveo_artifact_contract::ArtifactMetadata> {
    let mut artifact = ArtifactPut::new(bytes);
    artifact.mime_type = Some(mime_type.to_owned());
    artifact.filename = Some(filename);
    artifact.compliance = compliance;
    artifact.metadata = metadata;
    state
        .artifacts
        .put_with_capability(
            capability,
            ArtifactWriteIdempotencyKey::new(format!("reason:{task_id}:{kind}"))?,
            artifact,
        )
        .await
        .with_context(|| format!("publishing `{kind}` Reason artifact"))
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

fn resource_link(
    uri: &veoveo_artifact_contract::ArtifactUri,
    title: &str,
    mime_type: &str,
) -> ContentBlock {
    ContentBlock::ResourceLink(
        Resource::new(uri.to_string(), title.to_owned())
            .with_title(title.to_owned())
            .with_mime_type(mime_type),
    )
}

async fn record_usage(
    state: &AppState,
    task_id: AnalysisId,
    results: &ReasoningResults,
) -> Result<()> {
    state
        .tasks
        .platform_store()
        .upsert_domain_usage(DomainUsageDraft {
            task_id: task_id.task_id(),
            server: "reason".to_owned(),
            source_id: Some(results.recording_uri.clone()),
            provider_job_id: None,
            model_id: results.model_id.to_string(),
            kind: DomainUsageKind::Actual,
            quantity: Some(results.observed_frames as f64),
            unit: Some("observed_frame".to_owned()),
            amount: None,
            currency: None,
            recorded_at: now_utc(),
            metadata: OpenObject::new(BTreeMap::from([
                ("pipeline_id".into(), serde_json::json!(results.pipeline_id)),
                ("task_kind".into(), serde_json::json!(results.task.kind())),
                ("entity_path".into(), serde_json::json!(results.entity_path)),
                ("timeline".into(), serde_json::json!(results.timeline)),
                (
                    "prompt_revision".into(),
                    serde_json::json!(results.prompt_revision),
                ),
            ])),
        })
        .await
        .context("recording reason usage")?;
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
            artifact_metadata(ReasonArtifactProvenance::Results {
                analysis_id: "019fa7ee-4191-73e1-b084-2341d4900a06".parse().unwrap(),
                recording_id: "019fa7e9-d7c6-7fe1-bdff-0a5313586c3c".to_owned(),
                pipeline_id: "video-reasoning".parse().unwrap(),
                model_id: "world-model".parse().unwrap(),
                prompt_revision: "v1".to_owned(),
                task_kind: "describe_segment".to_owned(),
                source_snapshot_sha256: digest.clone(),
            })
            .unwrap(),
            artifact_metadata(ReasonArtifactProvenance::AnnotationLayer {
                analysis_id: "019fa7ee-4191-73e1-b084-2341d4900a06".parse().unwrap(),
                recording_id: "019fa7e9-d7c6-7fe1-bdff-0a5313586c3c".to_owned(),
                results_artifact_uri: "reason://artifact/019fa7ee-4191-73e1-b084-2341d4900a07"
                    .parse()
                    .unwrap(),
                source_snapshot_sha256: digest.clone(),
            })
            .unwrap(),
            artifact_metadata(ReasonArtifactProvenance::SourceClip {
                analysis_id: "019fa7ee-4191-73e1-b084-2341d4900a06".parse().unwrap(),
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
