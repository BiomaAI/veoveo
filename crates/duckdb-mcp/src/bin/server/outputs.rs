use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, ContentBlock, Resource},
};
use veoveo_duckdb_mcp::contract::{
    DuckDbExecuteOutput, DuckDbExportOutput, DuckDbIngestOutput, DuckDbQueryOutput,
};
use veoveo_mcp_contract::{UsageKind, UsageRecord, now_utc, set_related_task_meta};

use super::app_state::AppState;

fn finish<T: serde::Serialize>(
    output: &T,
    blocks: Vec<ContentBlock>,
    task_id: Option<&str>,
) -> Result<CallToolResult, McpError> {
    let mut result = CallToolResult::success(blocks);
    result.structured_content = Some(
        serde_json::to_value(output)
            .map_err(|err| McpError::internal_error(err.to_string(), None))?,
    );
    if let Some(task_id) = task_id {
        set_related_task_meta(&mut result.meta, task_id);
    }
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

pub(super) fn query_result(
    output: &DuckDbQueryOutput,
    task_id: Option<&str>,
) -> Result<CallToolResult, McpError> {
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
    finish(output, blocks, task_id)
}

pub(super) fn execute_result(
    output: &DuckDbExecuteOutput,
    task_id: Option<&str>,
) -> Result<CallToolResult, McpError> {
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
    finish(output, blocks, task_id)
}

pub(super) fn ingest_result(
    output: &DuckDbIngestOutput,
    task_id: Option<&str>,
) -> Result<CallToolResult, McpError> {
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
    finish(output, blocks, task_id)
}

pub(super) fn export_result(
    output: &DuckDbExportOutput,
    task_id: Option<&str>,
) -> Result<CallToolResult, McpError> {
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
    finish(output, blocks, task_id)
}

pub(super) fn record_op_usage(
    state: &AppState,
    task_id: &str,
    op: &'static str,
    quantity: u64,
    metadata: serde_json::Value,
) {
    let record = UsageRecord {
        task_id: task_id.to_string(),
        source_id: None,
        provider_job_id: None,
        model_id: format!("duckdb/{op}"),
        kind: UsageKind::Actual,
        quantity: Some(quantity as f64),
        unit: Some("row".to_string()),
        amount: None,
        currency: None,
        recorded_at: now_utc(),
        metadata,
    };
    if let Err(err) = state.durable.record_usage(&record) {
        tracing::warn!(task_id, "failed to record usage: {err}");
    }
}
