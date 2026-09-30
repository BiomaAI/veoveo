use super::{DurableRequest, SpeechService, owner};
use rmcp::{ErrorData as McpError, model::CallToolResult};
use veoveo_mcp_contract::{ArtifactPlane, PlaneCaller};
use veoveo_speech_contract::{TranscriptionId, TranscriptionOutput};
use veoveo_task_runtime::{TaskSnapshot, authorized_snapshot};

impl SpeechService {
    pub async fn authorize(
        &self,
        caller: &PlaneCaller,
        task: TranscriptionId,
        read_source: bool,
    ) -> Result<TaskSnapshot, McpError> {
        let snapshot = authorized_snapshot(
            &self.tasks.for_owner(&owner(&caller.identity)),
            &task.to_string(),
        )
        .await?;
        if snapshot.owner.authority.work_context != caller.identity.authority.work_context
            || snapshot.owner.authority.tenant != caller.identity.authority.tenant
        {
            return Err(McpError::invalid_params("unknown transcription", None));
        }
        if read_source {
            let request: DurableRequest = serde_json::from_value(snapshot.request.clone())
                .map_err(|_| McpError::internal_error("invalid persisted transcription", None))?;
            self.artifacts
                .head(caller, &request.source.artifact_id())
                .await
                .map_err(|_| McpError::invalid_params("recording access is unavailable", None))?;
        }
        Self::output(&snapshot)?;
        Ok(snapshot)
    }

    pub fn output(snapshot: &TaskSnapshot) -> Result<Option<TranscriptionOutput>, McpError> {
        let Some(result) = snapshot.result.clone() else {
            return Ok(None);
        };
        decode_output(snapshot.task_id, result)
    }
}

fn decode_output(
    task: veoveo_types::TaskId,
    result: serde_json::Value,
) -> Result<Option<TranscriptionOutput>, McpError> {
    let result: CallToolResult = serde_json::from_value(result)
        .map_err(|_| McpError::internal_error("invalid persisted transcription result", None))?;
    let output: Option<TranscriptionOutput> = result
        .structured_content
        .map(serde_json::from_value)
        .transpose()
        .map_err(|_| McpError::internal_error("invalid persisted transcription output", None))?;
    if output
        .as_ref()
        .is_some_and(|output| output.result_uri.id().task_id() != task)
    {
        return Err(McpError::internal_error(
            "transcription result identity mismatch",
            None,
        ));
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_speech_contract::TranscriptionUri;
    #[test]
    fn output_read_rejects_a_resource_for_another_task_or_family() {
        let task = TranscriptionId::new();
        let artifact = veoveo_artifact_contract::ArtifactId::new();
        let metadata = serde_json::json!({"artifact_id": artifact, "artifact_uri": artifact.plane_uri(),
            "byte_len": 1, "created_at": "2026-09-29T00:00:00Z"});
        let mut result = CallToolResult::success(vec![]);
        result.structured_content = Some(
            serde_json::json!({"result_uri": TranscriptionUri::new(task),
            "source_artifact_uri": artifact.plane_uri(), "transcript": metadata, "captions": metadata,
            "duration_seconds": 1.0}),
        );
        assert!(
            decode_output(task.task_id(), serde_json::to_value(&result).unwrap())
                .unwrap()
                .is_some()
        );
        for uri in [
            TranscriptionUri::new(TranscriptionId::new()).to_string(),
            artifact.plane_uri().to_string(),
        ] {
            result.structured_content.as_mut().unwrap()["result_uri"] = uri.into();
            assert!(decode_output(task.task_id(), serde_json::to_value(&result).unwrap()).is_err());
        }
    }
}
