use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result};
use rmcp::model::{CallToolResult, ContentBlock, Resource};
use veoveo_mcp_contract::{
    ArtifactPut, ArtifactWriteIdempotencyKey, ComplianceMetadata, DataLabelId,
    IssuedArtifactWriteCapability, now_utc,
};
use veoveo_perception_mcp::{
    annotation::{MP4_MIME_TYPE, RESULTS_MIME_TYPE, RRD_MIME_TYPE},
    contract::{
        AnalysisResults, AnalysisSummary, AnalyzeRecordingOutput, ExtractClipOutput, IndexRange,
    },
    source::MaterializedVideo,
    uris,
};
use veoveo_platform_store::{DomainUsageDraft, DomainUsageKind, OpenObject};
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
    let results_bytes = serde_json::to_vec_pretty(&products.results)?;
    let results_artifact = put(
        state,
        capability,
        task_id,
        "results",
        results_bytes,
        RESULTS_MIME_TYPE,
        format!("{task_id}.perception.json"),
        compliance.clone(),
        serde_json::json!({
            "provenance": {
                "kind": "perception_results",
                "analysis_id": task_id,
                "recording_id": products.source.recording_id,
                "pipeline_id": products.results.pipeline_id,
                "model_id": products.results.model_id,
            }
        }),
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
        serde_json::json!({
            "provenance": {
                "kind": "perception_annotation_layer",
                "analysis_id": task_id,
                "recording_id": products.source.recording_id,
                "results_artifact_uri": results_artifact.artifact_uri,
            }
        }),
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
                serde_json::json!({
                    "provenance": {
                        "kind": "perception_source_clip",
                        "analysis_id": task_id,
                        "recording_id": products.source.recording_id,
                        "entity_path": products.results.entity_path,
                        "timeline": products.results.timeline,
                        "decode_start_index": products.source.clip.decode_start_index,
                    }
                }),
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
    let output = AnalyzeRecordingOutput {
        analysis_uri: uris::analysis_uri(task_id),
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
        "perception analysis completed: {} frame(s), {detection_count} detection(s)",
        products.results.processed_frames
    ))];
    blocks.push(resource_link(
        &results_artifact.artifact_uri,
        "Perception results",
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
            "Perception source clip",
            MP4_MIME_TYPE,
        ));
    }
    let mut result = CallToolResult::success(blocks);
    result.structured_content = Some(serde_json::to_value(output)?);
    Ok(result)
}

pub(super) async fn publish_clip(
    state: &AppState,
    capability: &IssuedArtifactWriteCapability,
    task_id: &str,
    recording_uri: &str,
    source: MaterializedVideo,
) -> Result<CallToolResult> {
    let artifact = put(
        state,
        capability,
        task_id,
        "clip",
        source.mp4,
        MP4_MIME_TYPE,
        format!("{task_id}.mp4"),
        compliance(&source.classification, &source.labels)?,
        serde_json::json!({
            "provenance": {
                "kind": "recording_video_clip",
                "task_id": task_id,
                "recording_id": source.recording_id,
                "entity_path": source.clip.entity_path,
                "timeline": source.clip.timeline,
                "decode_start_index": source.clip.decode_start_index,
            }
        }),
    )
    .await?;
    let output = ExtractClipOutput {
        recording_uri: recording_uri.to_owned(),
        entity_path: source.clip.entity_path.clone(),
        timeline: source.clip.timeline.clone(),
        decode_start_index: source.clip.decode_start_index,
        requested_range: IndexRange {
            start: source.clip.requested_start_index,
            end: source.clip.requested_end_index,
        },
        sample_count: source.clip.samples.len() as u64,
        artifact: artifact.clone(),
    };
    let mut result = CallToolResult::success(vec![
        ContentBlock::text(format!(
            "extracted {} H.264 sample(s) into an MP4 clip",
            source.clip.samples.len()
        )),
        resource_link(
            &artifact.artifact_uri,
            "Recording video clip",
            MP4_MIME_TYPE,
        ),
    ]);
    result.structured_content = Some(serde_json::to_value(output)?);
    Ok(result)
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
            ArtifactWriteIdempotencyKey::new(format!("perception:{task_id}:{kind}"))?,
            artifact,
        )
        .await
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
            server: "perception".to_owned(),
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
        .context("recording perception usage")?;
    Ok(())
}
