//! Canonical durable product envelopes shared by publishers, readers and lookup settlement.
use crate::contract::RunRecordingOutput;
use rmcp::model::{CallToolResult, ContentBlock, Resource};
pub const RUN_COMPLETED: &str = "Recording run completed.";
pub fn recording_result(output: RunRecordingOutput) -> anyhow::Result<CallToolResult> {
    let id = output.run_id();
    let mut result = CallToolResult::success(vec![
        ContentBlock::text(RUN_COMPLETED),
        ContentBlock::ResourceLink(
            Resource::new(output.result_uri().to_string(), "result")
                .with_title("Stream recording results")
                .with_mime_type(crate::annotation::RESULTS_MIME_TYPE),
        ),
    ]);
    veoveo_mcp_contract::set_related_task_meta(&mut result.meta, id.to_string());
    result.structured_content = Some(serde_json::to_value(output)?);
    Ok(result)
}
pub fn validate(result: &CallToolResult) -> anyhow::Result<Option<RunRecordingOutput>> {
    if result.is_error == Some(true) {
        return Ok(None);
    }
    let output: RunRecordingOutput = serde_json::from_value(
        result
            .structured_content
            .clone()
            .ok_or_else(|| anyhow::anyhow!("owner product has no structured output"))?,
    )?;
    anyhow::ensure!(
        result.content == recording_result(output.clone())?.content,
        "owner product content does not match its canonical result link"
    );
    Ok(Some(output))
}
