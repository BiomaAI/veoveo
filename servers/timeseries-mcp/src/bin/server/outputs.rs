use rmcp::model::{CallToolResult, ContentBlock, Resource};
use veoveo_artifact_contract::{ArtifactPut, ComplianceMetadata};
use veoveo_artifact_contract::{ArtifactWriteIdempotencyKey, IssuedArtifactWriteCapability};
use veoveo_mcp_contract::{UsageKind, UsageRecord, now_utc};
use veoveo_platform_store::{DomainUsageDraft, DomainUsageKind, DomainUsageRecord, OpenObject};
use veoveo_timeseries_mcp::{
    contract::{TimeseriesArtifactUri, TimeseriesForecastSummary, TimeseriesForecastUsageMetadata},
    forecast::{ForecastArtifact, RRD_FILENAME, RRD_MIME_TYPE},
    state::TaskOwner,
};
use veoveo_types::TaskId;

use super::app_state::AppState;

pub(super) async fn forecast_result(
    state: &AppState,
    capability: &IssuedArtifactWriteCapability,
    task_id: TaskId,
    owner: &TaskOwner,
    artifact: ForecastArtifact,
) -> anyhow::Result<CallToolResult> {
    veoveo_timeseries_mcp::contract::validate_forecast_preview(
        &artifact.summary,
        &artifact.preview,
    )?;
    let mut put = ArtifactPut::new(artifact.rrd_bytes);
    put.mime_type = Some(RRD_MIME_TYPE.to_string());
    put.filename = Some(RRD_FILENAME.to_string());
    // The plane stamps tenant + owner from the verified identity and records the
    // owner grant; carry the caller's labels as artifact classification.
    put.compliance = ComplianceMetadata {
        data_labels: owner.data_labels.clone(),
        ..Default::default()
    };
    put.metadata = serde_json::to_value(artifact.metadata)?;
    let metadata = state
        .artifacts
        .put_with_capability(
            capability,
            ArtifactWriteIdempotencyKey::new(format!("timeseries:{task_id}:forecast"))?,
            put,
        )
        .await?;
    record_usage(state, task_id, &artifact.summary).await?;

    let public_metadata = metadata
        .clone()
        .without_download_url()
        .presented_under_scheme(&veoveo_timeseries_mcp::uris::SCHEME);
    let result_uri = TimeseriesArtifactUri::new(public_metadata.artifact_id());
    let mut blocks = vec![ContentBlock::text(forecast_status(&artifact.summary))];
    blocks.push(ContentBlock::ResourceLink(
        Resource::new(result_uri.as_str(), "forecast")
            .with_title("Timeseries forecast RRD")
            .with_description(
                "Rerun recording containing observed series, forecast, and provenance.",
            )
            .with_mime_type(RRD_MIME_TYPE),
    ));
    let mut result = CallToolResult::success(blocks);
    result.structured_content = Some(serde_json::to_value(
        veoveo_timeseries_mcp::contract::TimeseriesForecastOutputBuilder {
            result_uri,
            forecast: artifact.summary,
            preview: artifact.preview,
            artifact: public_metadata,
        }
        .build()?,
    )?);
    Ok(result)
}

async fn record_usage(
    state: &AppState,
    task_id: TaskId,
    summary: &TimeseriesForecastSummary,
) -> anyhow::Result<()> {
    state
        .tasks
        .platform_store()
        .upsert_domain_usage(DomainUsageDraft {
            task_id,
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
            metadata: serde_json::from_value::<OpenObject>(serde_json::to_value(
                TimeseriesForecastUsageMetadata::new(summary.series.len(), summary.horizon)?,
            )?)?,
        })
        .await?;
    Ok(())
}

pub(super) fn usage_record(task_id: TaskId, record: DomainUsageRecord) -> UsageRecord {
    UsageRecord {
        task_id: task_id.to_string(),
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

fn forecast_status(summary: &TimeseriesForecastSummary) -> String {
    format!(
        "timeseries forecast completed; source rows: {}; series: {}; forecast steps: {}",
        summary.source_rows,
        summary.series.len(),
        summary.horizon
    )
}

#[cfg(test)]
mod terminal_status_tests {
    use super::*;
    use veoveo_timeseries_mcp::contract::TimeseriesForecastMethod;

    #[test]
    fn terminal_status_is_short_and_identity_free() {
        let status = forecast_status(
            &veoveo_timeseries_mcp::contract::TimeseriesForecastSummaryBuilder {
                method: TimeseriesForecastMethod::NaiveTrend,
                horizon: veoveo_timeseries_mcp::contract::TimeseriesForecastHorizon::new(12)
                    .unwrap(),
                source_rows: 48,
                series: vec![veoveo_timeseries_mcp::contract::TimeseriesSeriesSummary {
                    series_id: "series".into(),
                    observed_rows: 48,
                    forecast_rows: 12,
                }],
            }
            .build()
            .unwrap(),
        );

        assert_eq!(
            status,
            "timeseries forecast completed; source rows: 48; series: 1; forecast steps: 12"
        );
        assert!(!status.contains("://"));
        assert!(!status.contains("artifact-"));
        assert!(status.len() < 256);
    }
}
