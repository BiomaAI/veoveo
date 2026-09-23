use super::{DurableRequest, SpeechService, owner};
use rmcp::{ErrorData as McpError, model::CallToolResult};
use veoveo_mcp_contract::{ArtifactPlane, PlaneCaller};
use veoveo_speech_contract::TranscriptionOutput;
use veoveo_task_runtime::{TaskSnapshot, authorized_snapshot};

impl SpeechService {
    pub async fn authorize(
        &self,
        caller: &PlaneCaller,
        task: &str,
        read_source: bool,
    ) -> Result<TaskSnapshot, McpError> {
        let snapshot = authorized_snapshot(&self.tasks, &owner(&caller.identity), task).await?;
        if snapshot.owner.authority.work_context != caller.identity.authority.work_context
            || snapshot.owner.authority.tenant != caller.identity.authority.tenant
        {
            return Err(McpError::invalid_params("unknown transcription", None));
        }
        if read_source {
            let request: DurableRequest = serde_json::from_value(snapshot.request.clone())
                .map_err(|_| McpError::internal_error("invalid persisted transcription", None))?;
            self.artifacts
                .head(caller, &request.source.artifact_id)
                .await
                .map_err(|_| McpError::invalid_params("recording access is unavailable", None))?;
        }
        Ok(snapshot)
    }

    pub fn output(snapshot: &TaskSnapshot) -> Result<Option<TranscriptionOutput>, McpError> {
        let Some(result) = snapshot.result.clone() else {
            return Ok(None);
        };
        let result: CallToolResult = serde_json::from_value(result).map_err(|_| {
            McpError::internal_error("invalid persisted transcription result", None)
        })?;
        result
            .structured_content
            .map(serde_json::from_value)
            .transpose()
            .map_err(|_| McpError::internal_error("invalid persisted transcription output", None))
    }
}
