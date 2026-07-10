use std::collections::BTreeMap;

use rmcp::model::{CallToolResult, ContentBlock, Resource};
use veoveo_mcp_contract::{
    ArtifactPut, ArtifactWriteIdempotencyKey, ComplianceMetadata, IssuedArtifactWriteCapability,
    UsageKind, UsageRecord, now_utc,
};
use veoveo_platform_store::{DomainUsageDraft, DomainUsageKind, DomainUsageRecord, OpenObject};
use veoveo_task_runtime::TaskId;
use veoveo_timeseries_mcp::{
    contract::{TimeseriesForecastOutput, TimeseriesForecastSummary},
    forecast::{ForecastArtifact, RRD_FILENAME, RRD_MIME_TYPE},
    state::TaskOwner,
};

use super::app_state::AppState;

pub(super) async fn forecast_result(
    state: &AppState,
    capability: &IssuedArtifactWriteCapability,
    task_id: &str,
    owner: &TaskOwner,
    artifact: ForecastArtifact,
) -> anyhow::Result<CallToolResult> {
    let mut put = ArtifactPut::new(artifact.rrd_bytes);
    put.mime_type = Some(RRD_MIME_TYPE.to_string());
    put.filename = Some(RRD_FILENAME.to_string());
    // The plane stamps tenant + owner from the verified identity and records the
    // owner grant; carry the caller's labels as artifact classification.
    put.compliance = ComplianceMetadata {
        data_labels: owner.data_labels.clone(),
        ..Default::default()
    };
    put.metadata = artifact.metadata;
    let metadata = state
        .artifacts
        .put_with_capability(
            capability,
            ArtifactWriteIdempotencyKey::new(format!("timeseries:{task_id}:forecast"))?,
            put,
        )
        .await?;
    record_usage(state, task_id, &artifact.summary).await?;

    let public_metadata = metadata.clone().without_download_url();
    let mut blocks = vec![ContentBlock::text(format!(
        "timeseries forecast completed with {} source row(s), {} series, {} forecast step(s); artifact {}",
        artifact.summary.source_rows,
        artifact.summary.series.len(),
        artifact.summary.horizon,
        public_metadata.artifact_uri
    ))];
    blocks.push(ContentBlock::ResourceLink(
        Resource::new(public_metadata.artifact_uri.clone(), "forecast")
            .with_title("Timeseries forecast RRD")
            .with_description(
                "Rerun recording containing observed series, forecast, and provenance.",
            )
            .with_mime_type(RRD_MIME_TYPE),
    ));
    let mut result = CallToolResult::success(blocks);
    result.structured_content = Some(serde_json::to_value(TimeseriesForecastOutput {
        forecast: artifact.summary,
        artifact: public_metadata,
    })?);
    Ok(result)
}

async fn record_usage(
    state: &AppState,
    task_id: &str,
    summary: &TimeseriesForecastSummary,
) -> anyhow::Result<()> {
    state
        .tasks
        .platform_store()
        .upsert_domain_usage(DomainUsageDraft {
            task_id: task_id.parse::<TaskId>()?,
            server: "timeseries".to_owned(),
            source_id: None,
            provider_job_id: None,
            model_id: "timeseries/naive-trend".to_owned(),
            kind: DomainUsageKind::Actual,
            quantity: Some(summary.source_rows as f64),
            unit: Some("source_row".to_owned()),
            amount: None,
            currency: None,
            recorded_at: now_utc(),
            metadata: OpenObject::new(BTreeMap::from([
                (
                    "series_count".into(),
                    serde_json::json!(summary.series.len()),
                ),
                ("horizon".into(), serde_json::json!(summary.horizon)),
                ("artifact_format".into(), serde_json::json!("rerun_rrd")),
            ])),
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
