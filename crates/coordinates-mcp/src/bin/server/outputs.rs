use rmcp::model::{CallToolResult, ContentBlock, Resource};
use veoveo_mcp_contract::{
    ArtifactMetadata, ArtifactPut, BatchTransformOutput, ComplianceMetadata, PlaneCaller,
    UsageKind, UsageRecord, now_utc, set_related_task_meta,
};

use super::{app_state::AppState, ownership::TaskOwner};

pub(super) async fn batch_result(
    state: &AppState,
    caller: &PlaneCaller,
    task_id: &str,
    owner: &TaskOwner,
    mut output: BatchTransformOutput,
    write_artifact: bool,
) -> anyhow::Result<CallToolResult> {
    if write_artifact && output.artifact.is_none() {
        output.artifact = Some(store_artifact(state, caller, task_id, owner, &output).await?);
    }
    state
        .coordinates
        .record_usage(UsageRecord {
            task_id: task_id.to_string(),
            source_id: Some(
                output
                    .result
                    .provenance
                    .operation
                    .operation_id
                    .as_str()
                    .to_string(),
            ),
            provider_job_id: None,
            model_id: "coordinates/batch-convert-frame".to_string(),
            kind: UsageKind::Actual,
            quantity: Some(output.result.points.len() as f64),
            unit: Some("point".to_string()),
            amount: None,
            currency: None,
            recorded_at: now_utc(),
            metadata: serde_json::json!({
                "operation_id": output.result.provenance.operation.operation_id,
                "operation_uri": output.result.provenance.operation.operation_uri,
                "target_frame": output.result.provenance.operation.target_frame,
            }),
        })
        .await;

    let mut blocks = vec![ContentBlock::text(format!(
        "batch transform completed with {} point(s)",
        output.result.points.len()
    ))];
    if let Some(artifact) = &output.artifact {
        blocks.push(artifact_link(
            artifact,
            "coordinates batch output",
            "JSON artifact containing the batch coordinate transform result.",
        ));
    }
    let mut result = CallToolResult::success(blocks);
    result.structured_content = Some(serde_json::to_value(&output)?);
    set_related_task_meta(&mut result.meta, task_id);
    Ok(result)
}

async fn store_artifact(
    state: &AppState,
    caller: &PlaneCaller,
    task_id: &str,
    owner: &TaskOwner,
    output: &BatchTransformOutput,
) -> anyhow::Result<ArtifactMetadata> {
    let mut put = ArtifactPut::new(serde_json::to_vec_pretty(output)?);
    put.mime_type = Some("application/json".to_string());
    put.filename = Some(format!("coordinates-batch-{task_id}.json"));
    put.compliance = ComplianceMetadata {
        data_labels: owner.data_labels.clone(),
        ..Default::default()
    };
    put.metadata = serde_json::json!({
        "task_id": task_id,
        "artifact_format": "coordinates_batch_json",
        "operation_id": output.result.provenance.operation.operation_id,
        "operation_uri": output.result.provenance.operation.operation_uri,
        "source_frame": output.result.provenance.operation.source_frame,
        "target_frame": output.result.provenance.operation.target_frame,
        "source_crs": output.result.provenance.source_crs,
        "target_crs": output.result.provenance.target_crs,
        "approximation_used": output.result.provenance.approximation_used,
    });
    Ok(state
        .artifacts
        .put(caller, put)
        .await?
        .without_download_url())
}

fn artifact_link(artifact: &ArtifactMetadata, title: &str, description: &str) -> ContentBlock {
    let mut resource = Resource::new(artifact.artifact_uri.clone(), title.to_string())
        .with_title(title.to_string())
        .with_description(description.to_string());
    if let Some(mime_type) = &artifact.mime_type {
        resource = resource.with_mime_type(mime_type.clone());
    }
    ContentBlock::ResourceLink(resource)
}
