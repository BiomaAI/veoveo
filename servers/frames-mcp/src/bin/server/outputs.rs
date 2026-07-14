use std::collections::BTreeMap;

use rmcp::model::{CallToolResult, ContentBlock, Resource};
use veoveo_frames_mcp::contract::BatchTransformOutput;
use veoveo_mcp_contract::{
    ArtifactMetadata, ArtifactPut, ArtifactWriteIdempotencyKey, ComplianceMetadata,
    IssuedArtifactWriteCapability, UsageKind, UsageRecord, now_utc,
};
use veoveo_platform_store::{
    DomainUsageDraft, DomainUsageKind, DomainUsageRecord, OpenObject, TaskId,
};
use veoveo_task_runtime::TaskOwner;

use super::app_state::AppState;

pub(super) async fn batch_result(
    state: &AppState,
    capability: Option<&IssuedArtifactWriteCapability>,
    task_id: &str,
    owner: &TaskOwner,
    mut output: BatchTransformOutput,
    write_artifact: bool,
) -> anyhow::Result<CallToolResult> {
    if write_artifact && output.artifact.is_none() {
        let capability = capability
            .ok_or_else(|| anyhow::anyhow!("task did not reserve artifact write capability"))?;
        output.artifact = Some(store_artifact(state, capability, task_id, owner, &output).await?);
    }
    record_usage(state, task_id, &output).await?;

    let mut blocks = vec![ContentBlock::text(format!(
        "batch transform completed with {} point(s)",
        output.result.points.len()
    ))];
    if let Some(artifact) = &output.artifact {
        blocks.push(artifact_link(
            artifact,
            "Frames batch output",
            "JSON artifact containing the batch coordinate transform result.",
        ));
    }
    let mut result = CallToolResult::success(blocks);
    result.structured_content = Some(serde_json::to_value(&output)?);
    Ok(result)
}

async fn store_artifact(
    state: &AppState,
    capability: &IssuedArtifactWriteCapability,
    task_id: &str,
    owner: &TaskOwner,
    output: &BatchTransformOutput,
) -> anyhow::Result<ArtifactMetadata> {
    let mut put = ArtifactPut::new(serde_json::to_vec_pretty(output)?);
    put.mime_type = Some("application/json".to_string());
    put.filename = Some(format!("frames-batch-{task_id}.json"));
    put.compliance = ComplianceMetadata {
        data_labels: owner
            .data_labels
            .iter()
            .cloned()
            .map(veoveo_mcp_contract::DataLabelId::new)
            .collect::<Result<_, _>>()?,
        ..Default::default()
    };
    put.metadata = serde_json::json!({
        "task_id": task_id,
        "artifact_format": "frames_batch_json",
        "operation_id": output.result.provenance.operation.operation_id,
        "operation_uri": output.result.provenance.operation.operation_uri,
        "source_frame": output.result.provenance.operation.source_frame,
        "target_frame": output.result.provenance.operation.target_frame,
        "source_crs": output.result.provenance.source_crs,
        "target_crs": output.result.provenance.target_crs,
        "approximation_used": output.result.provenance.approximation_used,
    });
    let idempotency_key = ArtifactWriteIdempotencyKey::new(format!("frames:{task_id}:batch"))?;
    Ok(state
        .artifacts
        .put_with_capability(capability, idempotency_key, put)
        .await?
        .without_download_url())
}

async fn record_usage(
    state: &AppState,
    task_id: &str,
    output: &BatchTransformOutput,
) -> anyhow::Result<()> {
    let metadata = OpenObject::new(BTreeMap::from([
        (
            "operation_id".to_owned(),
            serde_json::json!(output.result.provenance.operation.operation_id),
        ),
        (
            "operation_uri".to_owned(),
            serde_json::json!(output.result.provenance.operation.operation_uri),
        ),
        (
            "target_frame".to_owned(),
            serde_json::json!(output.result.provenance.operation.target_frame),
        ),
    ]));
    state
        .tasks
        .platform_store()
        .upsert_domain_usage(DomainUsageDraft {
            task_id: task_id.parse::<TaskId>()?,
            server: "frames".to_owned(),
            source_id: Some(output.result.provenance.operation.operation_id.to_string()),
            provider_job_id: None,
            model_id: "frames/batch-convert-frame".to_owned(),
            kind: DomainUsageKind::Actual,
            quantity: Some(output.result.points.len() as f64),
            unit: Some("point".to_owned()),
            amount: None,
            currency: None,
            recorded_at: now_utc(),
            metadata,
        })
        .await?;
    Ok(())
}

pub(super) fn usage_record(task_id: &str, record: DomainUsageRecord) -> UsageRecord {
    UsageRecord {
        task_id: task_id.to_owned(),
        source_id: record.source_id,
        provider_job_id: record.provider_job_id,
        model_id: record.model_id,
        kind: match record.kind {
            DomainUsageKind::Estimate => UsageKind::Estimate,
            DomainUsageKind::Actual => UsageKind::Actual,
        },
        quantity: record.quantity,
        unit: record.unit,
        amount: record.amount,
        currency: record.currency,
        recorded_at: record.recorded_at,
        metadata: serde_json::Value::Object(record.metadata.into_map().into_iter().collect()),
    }
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
