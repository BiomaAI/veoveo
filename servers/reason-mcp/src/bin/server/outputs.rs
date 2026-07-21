use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result};
use rmcp::model::{CallToolResult, ContentBlock, Resource};
use veoveo_mcp_contract::{
    ArtifactPut, ArtifactWriteIdempotencyKey, ComplianceMetadata, DataLabelId,
    IssuedArtifactWriteCapability, now_utc,
};
use veoveo_reason_mcp::{
    annotation::{MP4_MIME_TYPE, RESULTS_MIME_TYPE, RRD_MIME_TYPE},
    contract::{AnalyzeRecordingOutput, ReasoningResults, ReasoningSummary},
    uris,
};
use veoveo_recording_video::MaterializedVideo;
use veoveo_platform_store::{DomainUsageDraft, DomainUsageKind, OpenObject};
use veoveo_task_runtime::TaskId;

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
        format!("{task_id}.reason.json"),
        compliance.clone(),
        serde_json::json!({
            "provenance": {
                "kind": "reason_results",
                "analysis_id": task_id,
                "recording_id": products.source.recording_id,
                "pipeline_id": products.results.pipeline_id,
                "model_id": products.results.model_id,
                "prompt_revision": products.results.prompt_revision,
                "task_kind": products.results.task.kind(),
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
                "kind": "reason_annotation_layer",
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
                        "kind": "reason_source_clip",
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
    let event_count = products.results.answer.event_count();
    let output = AnalyzeRecordingOutput {
        analysis_uri: uris::analysis_uri(task_id),
        results_uri: uris::results_uri(task_id),
        pipeline_uri: uris::pipeline_uri(&products.results.pipeline_id),
        model_uri: uris::model_uri(&products.results.model_id),
        summary: ReasoningSummary {
            observed_frames: products.results.observed_frames,
            event_count,
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
            ArtifactWriteIdempotencyKey::new(format!("reason:{task_id}:{kind}"))?,
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

async fn record_usage(state: &AppState, task_id: &str, results: &ReasoningResults) -> Result<()> {
    state
        .tasks
        .platform_store()
        .upsert_domain_usage(DomainUsageDraft {
            task_id: task_id.parse::<TaskId>()?,
            server: "reason".to_owned(),
            source_id: Some(results.recording_uri.clone()),
            provider_job_id: None,
            model_id: results.model_id.clone(),
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
