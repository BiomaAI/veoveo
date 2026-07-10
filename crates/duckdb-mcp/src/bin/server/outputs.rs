use std::collections::BTreeMap;

use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, ContentBlock, Resource},
};
use veoveo_duckdb_mcp::contract::{
    DuckDbExecuteOutput, DuckDbExportOutput, DuckDbIngestOutput, DuckDbQueryOutput,
};
use veoveo_mcp_contract::{UsageKind, UsageRecord, now_utc};
use veoveo_platform_store::{DomainUsageDraft, DomainUsageKind, DomainUsageRecord, OpenObject};
use veoveo_task_runtime::TaskId;

use super::app_state::AppState;

fn finish<T: serde::Serialize>(
    output: &T,
    blocks: Vec<ContentBlock>,
) -> Result<CallToolResult, McpError> {
    let mut result = CallToolResult::success(blocks);
    result.structured_content = Some(
        serde_json::to_value(output)
            .map_err(|err| McpError::internal_error(err.to_string(), None))?,
    );
    Ok(result)
}

fn artifact_link(
    artifact: &veoveo_mcp_contract::ArtifactMetadata,
    title: &str,
    description: &str,
) -> ContentBlock {
    let mut resource = Resource::new(artifact.artifact_uri.clone(), title.to_string())
        .with_title(title.to_string())
        .with_description(description.to_string());
    if let Some(mime_type) = &artifact.mime_type {
        resource = resource.with_mime_type(mime_type.clone());
    }
    ContentBlock::ResourceLink(resource)
}

pub(super) fn query_result(output: &DuckDbQueryOutput) -> Result<CallToolResult, McpError> {
    let mut blocks = Vec::new();
    match &output.artifact {
        Some(artifact) => {
            blocks.push(ContentBlock::text(format!(
                "query exported {} row(s) to {}",
                output.row_count, artifact.artifact_uri
            )));
            blocks.push(artifact_link(
                artifact,
                "query result",
                "Immutable query result artifact.",
            ));
        }
        None => {
            blocks.push(ContentBlock::text(format!(
                "query returned {} row(s){}",
                output.row_count,
                if output.truncated {
                    "; inline rows truncated — re-run with artifact output for the full set"
                } else {
                    ""
                }
            )));
        }
    }
    finish(output, blocks)
}

pub(super) fn execute_result(output: &DuckDbExecuteOutput) -> Result<CallToolResult, McpError> {
    let blocks = vec![ContentBlock::text(format!(
        "executed {} statement(s) on `{}`{}{}",
        output.statements,
        output.db,
        if output.rows_changed > 0 {
            format!("; {} row(s) changed", output.rows_changed)
        } else {
            String::new()
        },
        if output.db_created {
            "; database created"
        } else {
            ""
        },
    ))];
    finish(output, blocks)
}

pub(super) fn ingest_result(output: &DuckDbIngestOutput) -> Result<CallToolResult, McpError> {
    let blocks = vec![ContentBlock::text(format!(
        "ingested {} row(s) into `{}`.`{}`{}",
        output.rows_ingested,
        output.db,
        output.table,
        if output.db_created {
            "; database created"
        } else {
            ""
        },
    ))];
    finish(output, blocks)
}

pub(super) fn export_result(output: &DuckDbExportOutput) -> Result<CallToolResult, McpError> {
    let blocks = vec![
        ContentBlock::text(format!(
            "exported `{}` ({} row(s)) to {}",
            output.db, output.rows_exported, output.artifact.artifact_uri
        )),
        artifact_link(
            &output.artifact,
            "export",
            "Immutable exported data artifact.",
        ),
    ];
    finish(output, blocks)
}

pub(super) async fn record_op_usage(
    state: &AppState,
    task_id: &str,
    op: &'static str,
    quantity: u64,
    metadata: serde_json::Value,
) -> anyhow::Result<()> {
    let metadata = match metadata {
        serde_json::Value::Object(values) => {
            OpenObject::new(values.into_iter().collect::<BTreeMap<_, _>>())
        }
        value => OpenObject::new(BTreeMap::from([("value".to_owned(), value)])),
    };
    state
        .tasks
        .platform_store()
        .upsert_domain_usage(DomainUsageDraft {
            task_id: task_id.parse::<TaskId>()?,
            server: "duckdb".to_owned(),
            source_id: None,
            provider_job_id: None,
            model_id: format!("duckdb/{op}"),
            kind: DomainUsageKind::Actual,
            quantity: Some(quantity as f64),
            unit: Some("row".to_owned()),
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
