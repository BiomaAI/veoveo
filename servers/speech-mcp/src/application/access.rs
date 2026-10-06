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
        let request: DurableRequest = serde_json::from_value(snapshot.request.clone())
            .map_err(|_| McpError::internal_error("invalid persisted transcription", None))?;
        decode_output(snapshot.task_id, &request.source.artifact_uri, result)
    }
}

fn decode_output(
    task: veoveo_types::TaskId,
    source: &veoveo_artifact_contract::ArtifactUri,
    result: serde_json::Value,
) -> Result<Option<TranscriptionOutput>, McpError> {
    let result: CallToolResult = serde_json::from_value(result)
        .map_err(|_| McpError::internal_error("invalid persisted transcription result", None))?;
    if result.is_error == Some(true) {
        return Err(McpError::internal_error(
            "errored persisted transcription result",
            None,
        ));
    }
    let output: Option<TranscriptionOutput> = result
        .structured_content
        .map(serde_json::from_value)
        .transpose()
        .map_err(|_| McpError::internal_error("invalid persisted transcription output", None))?;
    if output.as_ref().is_some_and(|output| {
        output.result_uri.id().task_id() != task || &output.source_artifact_uri != source
    }) {
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
        let transcript = veoveo_artifact_contract::ArtifactId::new();
        let captions = veoveo_artifact_contract::ArtifactId::new();
        let metadata = |id, mime| {
            serde_json::json!({"artifact_id": id,
            "artifact_uri": format!("speech://artifact/{id}"), "mime_type": mime,
            "byte_len": 1, "created_at": "2026-09-29T00:00:00Z", "metadata": {
                "source_artifact_uri": artifact.plane_uri(), "source_sha256": "a".repeat(64),
                "model": "upstream model", "model_revision": "upstream revision"
            }})
        };
        let mut result = CallToolResult::success(vec![]);
        result.structured_content = Some(
            serde_json::json!({"result_uri": TranscriptionUri::new(task),
            "source_artifact_uri": artifact.plane_uri(), "transcript": metadata(transcript, "application/json"), "captions": metadata(captions, "text/vtt"),
            "duration_seconds": 1.0}),
        );
        assert!(
            decode_output(
                task.task_id(),
                &artifact.plane_uri(),
                serde_json::to_value(&result).unwrap()
            )
            .unwrap()
            .is_some()
        );
        let valid = serde_json::to_value(&result).unwrap();
        let mut errored = result.clone();
        errored.is_error = Some(true);
        assert!(
            decode_output(
                task.task_id(),
                &artifact.plane_uri(),
                serde_json::to_value(errored).unwrap()
            )
            .is_err()
        );
        assert!(
            decode_output(
                task.task_id(),
                &veoveo_artifact_contract::ArtifactId::new().plane_uri(),
                valid.clone()
            )
            .is_err()
        );
        for (pointer, bad) in [
            ("/structuredContent/duration_seconds", serde_json::json!(-1)),
            (
                "/structuredContent/transcript/artifact_uri",
                serde_json::json!(transcript.plane_uri()),
            ),
            (
                "/structuredContent/captions/artifact_id",
                serde_json::json!(transcript),
            ),
            (
                "/structuredContent/transcript/metadata/source_sha256",
                serde_json::json!("wrong"),
            ),
        ] {
            let mut invalid = valid.clone();
            *invalid.pointer_mut(pointer).unwrap() = bad;
            assert!(
                decode_output(task.task_id(), &artifact.plane_uri(), invalid).is_err(),
                "{pointer}"
            );
        }
        for uri in [
            TranscriptionUri::new(TranscriptionId::new()).to_string(),
            artifact.plane_uri().to_string(),
        ] {
            result.structured_content.as_mut().unwrap()["result_uri"] = uri.into();
            assert!(
                decode_output(
                    task.task_id(),
                    &artifact.plane_uri(),
                    serde_json::to_value(&result).unwrap()
                )
                .is_err()
            );
        }
    }
}
