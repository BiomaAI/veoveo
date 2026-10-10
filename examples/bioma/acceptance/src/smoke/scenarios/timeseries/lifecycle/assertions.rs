//! General workload output agreement; the original deterministic fixture stays separate.
use super::super::assertions;
use super::*;
use re_sdk::{external::re_log_types::LogMsg, log::Chunk};
use re_sdk_types::archetypes::TextDocument;
use sha2::{Digest, Sha256};
use veoveo_timeseries_mcp::contract::{TimeseriesRecordingTask, TimeseriesRrdProvenance};
/// Delivery hints and update times may change between notification and readback.
pub(super) fn terminal_agreement(
    delivered: &DetailedTask,
    current: &DetailedTask,
    id: &CanonicalTaskId,
    created: &str,
) -> Result<()> {
    same_task(delivered, id, created)?;
    same_task(current, id, created)?;
    ensure!(
        matches!(
            delivered.status(),
            TaskStatus::Completed | TaskStatus::Cancelled
        ) && delivered.payload == current.payload,
        "Timeseries lifecycle delivered/current terminal payloads differ"
    );
    Ok(())
}

pub(super) async fn verify_output(
    client: &SmokeMcpClient,
    request: &TimeseriesForecastRequest,
    id: &CanonicalTaskId,
    terminal: &DetailedTask,
    file: &mut std::fs::File,
    receipt: &mut Receipt,
) -> Result<()> {
    let payload = task_payload(client, id.as_str())
        .await
        .map_err(|_| anyhow!("Timeseries lifecycle completed payload read failed"))?;
    let rmcp::model::TaskPayload::Completed { result } = &terminal.payload else {
        bail!("Timeseries lifecycle terminal omitted result")
    };
    let delivered: rmcp::model::CallToolResult =
        serde_json::from_value(Value::Object(result.clone()))?;
    ensure!(
        serde_json::to_value(&delivered)? == serde_json::to_value(&payload)?,
        "Timeseries lifecycle delivered/current result differs"
    );
    ensure!(
        payload.is_error != Some(true),
        "Timeseries lifecycle returned tool error"
    );
    let output: TimeseriesForecastOutput = serde_json::from_value(
        payload
            .structured_content
            .context("Timeseries lifecycle output omitted structured content")?,
    )?;
    let metadata: veoveo_timeseries_mcp::contract::TimeseriesForecastMetadata =
        serde_json::from_value(output.artifact.metadata.clone())?;
    ensure!(
        metadata.task_id.as_uuid().get_version_num() == 7
            && metadata.task_id == metadata.provenance.task_id
            && metadata.summary == output.forecast
            && output.forecast.method == request.method
            && output.forecast.horizon == request.horizon
            && output.forecast.source_rows > 0,
        "Timeseries lifecycle output identity/summary differs"
    );
    assertions::require_provenance(request, &metadata.provenance)?;
    ensure!(
        output.result_uri.artifact_id() == output.artifact.artifact_id()
            && output.artifact.byte_len > 0
            && output.artifact.mime_type.as_deref() == Some(assertions::RRD_MIME),
        "Timeseries lifecycle Artifact handoff differs"
    );
    let current: ArtifactMetadata = reads::json(
        client,
        &veoveo_artifact_mcp::contract::metadata_uri(output.artifact.artifact_id()),
        file,
        receipt,
    )
    .await?;
    assertions::current_metadata(&output, &current)?;
    let owner_bytes = reads::blob(client, &output.result_uri.to_uri()?, file, receipt).await?;
    let artifact_uri =
        veoveo_artifact_mcp::contract::ArtifactResource::Occurrence(output.artifact.artifact_id())
            .to_uri();
    let artifact_bytes = reads::blob(client, &artifact_uri, file, receipt).await?;
    ensure!(
        owner_bytes == artifact_bytes && owner_bytes.len() as u64 == output.artifact.byte_len,
        "Timeseries lifecycle current Artifact bytes differ"
    );
    recording(
        &owner_bytes,
        &metadata.provenance,
        metadata.task_id,
        request,
    )?;
    receipt.lifecycle.as_mut().unwrap().artifact_digest = Some(
        veoveo_types::Sha256Digest::from_bytes(Sha256::digest(&owner_bytes).into()),
    );
    let usage_uri = TimeseriesTaskUsageUri::new(metadata.task_id)?.to_uri()?;
    let usage: veoveo_mcp_contract::UsageReport =
        reads::json(client, &usage_uri, file, receipt).await?;
    ensure!(
        usage.task_id.parse::<veoveo_types::TaskId>()? == metadata.task_id
            && usage.usage_uri == usage_uri.as_str()
            && usage.records.len() == 1
            && usage.total_kind == Some(veoveo_mcp_contract::UsageKind::Actual)
            && usage.total_amount.is_none()
            && usage.currency.is_none(),
        "Timeseries lifecycle usage identity differs"
    );
    let usage_metadata: veoveo_timeseries_mcp::contract::TimeseriesForecastUsageMetadata =
        serde_json::from_value(usage.records[0].metadata.clone())?;
    ensure!(
        usage.records[0].task_id.parse::<veoveo_types::TaskId>()? == metadata.task_id
            && usage.records[0].kind == veoveo_mcp_contract::UsageKind::Actual
            && usage.records[0].model_id == "timeseries/naive-trend"
            && usage.records[0].unit.as_deref() == Some("source_row")
            && usage.records[0].amount.is_none()
            && usage.records[0].currency.is_none()
            && usage.records[0].source_id.is_none()
            && usage.records[0].provider_job_id.is_none()
            && usage_metadata.horizon == request.horizon
            && usage_metadata.series_count == output.forecast.series.len()
            && usage.records[0].quantity == Some(output.forecast.source_rows as f64),
        "Timeseries lifecycle usage request/quantity differs"
    );
    receipt.lifecycle.as_mut().unwrap().output = Some(output);
    persist(file, receipt)
}

fn recording(
    bytes: &[u8],
    expected: &TimeseriesRrdProvenance,
    task_id: veoveo_types::TaskId,
    request: &TimeseriesForecastRequest,
) -> Result<()> {
    let messages = re_log_encoding::Decoder::<LogMsg>::decode_eager(std::io::Cursor::new(bytes))
        .map_err(|_| anyhow!("Timeseries lifecycle Artifact is not current RRD"))?;
    let mut provenance = None;
    let mut task = None;
    for (index, message) in messages.enumerate() {
        ensure!(
            index < 4096,
            "Timeseries lifecycle RRD message budget exhausted"
        );
        if let LogMsg::ArrowMsg(_, arrow) =
            message.map_err(|_| anyhow!("Timeseries lifecycle RRD message invalid"))?
        {
            let chunk = Chunk::from_arrow_msg(&arrow)
                .map_err(|_| anyhow!("Timeseries lifecycle RRD chunk invalid"))?;
            for batch in chunk.iter_component::<re_sdk_types::components::Text>(
                TextDocument::descriptor_text().component,
            ) {
                for text in batch.as_slice() {
                    let text = text.0.to_string();
                    ensure!(
                        text.len() <= 65536,
                        "Timeseries lifecycle RRD document exceeds64KiB"
                    );
                    match chunk.entity_path().to_string().as_str() {
                        "/timeseries/provenance" => {
                            ensure!(provenance.is_none(), "duplicate lifecycle provenance");
                            provenance =
                                Some(serde_json::from_str::<TimeseriesRrdProvenance>(&text)?);
                        }
                        "/timeseries/task" => {
                            ensure!(task.is_none(), "duplicate lifecycle Task");
                            task = Some(serde_json::from_str::<TimeseriesRecordingTask>(&text)?);
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    ensure!(
        provenance.as_ref() == Some(expected),
        "Timeseries lifecycle RRD provenance differs from current metadata"
    );
    let task = task.context("Timeseries lifecycle RRD omitted Task")?;
    ensure!(
        task.task_id == task_id && task.method == request.method && task.horizon == request.horizon,
        "Timeseries lifecycle RRD request/Task differs"
    );
    Ok(())
}

#[cfg(test)]
mod delivery_tests {
    use super::*;
    #[test]
    fn timeseries_terminal_agreement_checks_stable_identity_payload_and_allows_hints() -> Result<()>
    {
        let id = CanonicalTaskId::parse("gateway-forecast-delivery")?;
        let created = "2026-10-09T00:00:00Z";
        let task = rmcp::model::Task::new(id.as_str(), TaskStatus::Completed, created, created);
        let delivered = DetailedTask::new(
            task,
            rmcp::model::TaskPayload::Completed {
                result: serde_json::json!({"structuredContent":{"forecast":7}})
                    .as_object()
                    .unwrap()
                    .clone(),
            },
        );
        let mut current = delivered.clone();
        current.task.ttl_ms = Some(1000);
        current.task.poll_interval_ms = Some(50);
        current.task.last_updated_at = "2026-10-09T00:00:01Z".into();
        terminal_agreement(&delivered, &current, &id, created)?;
        current.task.task_id = "gateway-another-task".into();
        assert!(terminal_agreement(&delivered, &current, &id, created).is_err());
        current = delivered.clone();
        current.task.created_at = "2026-10-09T00:00:02Z".into();
        assert!(terminal_agreement(&delivered, &current, &id, created).is_err());
        current = delivered.clone();
        let rmcp::model::TaskPayload::Completed { result } = &mut current.payload else {
            unreachable!()
        };
        result.insert(
            "structuredContent".into(),
            serde_json::json!({"forecast":8}),
        );
        assert!(terminal_agreement(&delivered, &current, &id, created).is_err());
        current = DetailedTask::new(delivered.task.clone(), rmcp::model::TaskPayload::Cancelled);
        assert!(terminal_agreement(&delivered, &current, &id, created).is_err());
        terminal_agreement(&current, &current, &id, created)?;
        let working = DetailedTask::new(delivered.task.clone(), rmcp::model::TaskPayload::Working);
        assert!(terminal_agreement(&working, &working, &id, created).is_err());
        Ok(())
    }
}
