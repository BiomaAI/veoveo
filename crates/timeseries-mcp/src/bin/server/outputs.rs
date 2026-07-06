use rmcp::model::{CallToolResult, ContentBlock, Resource};
use veoveo_mcp_contract::{
    ArtifactPut, ComplianceMetadata, TimeseriesForecastOutput, UsageKind, UsageRecord, now_utc,
    set_related_task_meta,
};
use veoveo_timeseries_mcp::{
    forecast::{ForecastArtifact, RRD_FILENAME, RRD_MIME_TYPE},
    state::TaskOwner,
};

use super::{app_state::AppState, ownership::artifact_owner_from_task};

pub(super) async fn forecast_result(
    state: &AppState,
    task_id: &str,
    owner: &TaskOwner,
    artifact: ForecastArtifact,
) -> anyhow::Result<CallToolResult> {
    let mut put = ArtifactPut::new(artifact.rrd_bytes);
    put.mime_type = Some(RRD_MIME_TYPE.to_string());
    put.filename = Some(RRD_FILENAME.to_string());
    put.compliance = ComplianceMetadata {
        tenant_id: owner.tenant.clone(),
        owner_id: Some(owner.principal_id.clone()),
        data_labels: owner.data_labels.clone(),
        ..Default::default()
    };
    put.metadata = artifact.metadata;
    let mut metadata = state.artifacts.put(put).await?;
    let artifact_owner = artifact_owner_from_task(&metadata.sha256, owner);
    state.durable.record_artifact_owner(&artifact_owner)?;
    metadata.compliance.owner_id = Some(owner.principal_id.clone());
    metadata.compliance.tenant_id = owner.tenant.clone();
    metadata.compliance.data_labels = owner.data_labels.clone();
    record_usage(state, task_id, &artifact.summary)?;

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
    set_related_task_meta(&mut result.meta, task_id);
    Ok(result)
}

fn record_usage(
    state: &AppState,
    task_id: &str,
    summary: &veoveo_mcp_contract::TimeseriesForecastSummary,
) -> anyhow::Result<()> {
    state.durable.record_usage(&UsageRecord {
        task_id: task_id.to_string(),
        source_id: None,
        provider_job_id: None,
        model_id: "timeseries/naive-trend".to_string(),
        kind: UsageKind::Actual,
        quantity: Some(summary.source_rows as f64),
        unit: Some("source_row".to_string()),
        amount: None,
        currency: None,
        recorded_at: now_utc(),
        metadata: serde_json::json!({
            "series_count": summary.series.len(),
            "horizon": summary.horizon,
            "artifact_format": "rerun_rrd",
        }),
    })?;
    Ok(())
}
