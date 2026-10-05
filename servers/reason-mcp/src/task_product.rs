//! Canonical durable product envelopes shared by publishers, readers and lookup settlement.
use crate::contract::AnalyzeRecordingOutput;
use rmcp::model::{CallToolResult, ContentBlock, Resource};
pub const ANALYSIS_COMPLETED: &str = "Analysis completed.";
pub fn analysis_tool_result(output: AnalyzeRecordingOutput) -> anyhow::Result<CallToolResult> {
    let id = output.analysis_id();
    let mut result = CallToolResult::success(vec![
        ContentBlock::text(ANALYSIS_COMPLETED),
        ContentBlock::ResourceLink(
            Resource::new(output.result_uri().to_string(), "analysis_result")
                .with_title("Reason analysis result")
                .with_mime_type("application/vnd.veoveo.reason-results+json"),
        ),
    ]);
    veoveo_mcp_contract::set_related_task_meta(&mut result.meta, id.to_string());
    result.structured_content = Some(serde_json::to_value(output)?);
    Ok(result)
}
pub fn validate(result: &CallToolResult) -> anyhow::Result<Option<AnalyzeRecordingOutput>> {
    if result.is_error == Some(true) {
        return Ok(None);
    }
    let output: AnalyzeRecordingOutput = serde_json::from_value(
        result
            .structured_content
            .clone()
            .ok_or_else(|| anyhow::anyhow!("owner product has no structured output"))?,
    )?;
    anyhow::ensure!(
        result.content == analysis_tool_result(output.clone())?.content,
        "owner product content does not match its canonical result link"
    );
    Ok(Some(output))
}
