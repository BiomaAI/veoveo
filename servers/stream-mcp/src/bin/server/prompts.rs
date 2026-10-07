use rmcp::{
    ErrorData as McpError,
    model::{GetPromptResult, JsonObject, Prompt, PromptArgument, PromptMessage, Role},
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Copy)]
pub(super) enum StreamPrompt {
    RunRecording,
    StartLiveSession,
}

impl StreamPrompt {
    pub(super) const ALL: [Self; 2] = [Self::RunRecording, Self::StartLiveSession];

    pub(super) fn by_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|prompt| prompt.name() == name)
    }

    fn name(self) -> &'static str {
        match self {
            Self::RunRecording => "stream_run_recording",
            Self::StartLiveSession => "stream_start_live_session",
        }
    }

    pub(super) fn definition(self) -> Prompt {
        let (title, description, arguments) = match self {
            Self::RunRecording => (
                "Run a pipeline over recorded video",
                "Plan a GPU Stream run over a Rerun video range.",
                vec![
                    required("recording_uri", "Canonical recording resource URI."),
                    required("entity_path", "Rerun VideoStream entity path."),
                    required("timeline", "Rerun timeline name."),
                    required("start", "Inclusive raw timeline index."),
                    required("end", "Inclusive raw timeline index."),
                    required("pipeline_id", "Stream pipeline identifier."),
                ],
            ),
            Self::StartLiveSession => (
                "Start a live stream session",
                "Start a configured live GStreamer pipeline and return its ingest address.",
                vec![required("pipeline_id", "Live Stream pipeline identifier.")],
            ),
        };
        Prompt::new(self.name(), Some(description), Some(arguments)).with_title(title)
    }

    pub(super) fn render(self, arguments: Option<JsonObject>) -> Result<GetPromptResult, McpError> {
        #[derive(Deserialize)]
        struct Args {
            recording_uri: Option<veoveo_recording_mcp::contract::RecordingUri>,
            entity_path: Option<String>,
            timeline: Option<String>,
            start: Option<i64>,
            end: Option<i64>,
            pipeline_id: veoveo_stream_mcp::contract::PipelineId,
        }
        let args: Args = serde_json::from_value(Value::Object(arguments.unwrap_or_default()))
            .map_err(|error| McpError::invalid_params(error.to_string(), None))?;
        let pipeline_id = &args.pipeline_id;
        let pipeline_uri = veoveo_stream_mcp::uris::pipeline_uri(pipeline_id);
        let text = match self {
            Self::RunRecording => format!(
                "Read stream://pipelines and verify pipeline {pipeline_id}. Call run_recording with video recordingUri {}, entityPath {}, timeline {}, range {}..={}, and the selected pipeline. Treat the returned run and artifact URIs as canonical.",
                args.recording_uri
                    .as_ref()
                    .map(|uri| uri.as_str())
                    .unwrap_or("<required>"),
                args.entity_path.as_deref().unwrap_or("<required>"),
                args.timeline.as_deref().unwrap_or("<required>"),
                args.start
                    .map_or_else(|| "<required>".to_owned(), |value| value.to_string()),
                args.end
                    .map_or_else(|| "<required>".to_owned(), |value| value.to_string()),
            ),
            Self::StartLiveSession => format!(
                "Read {pipeline_uri} and verify it supports live input. Call start_live_session with that pipelineId, then subscribe to the returned session and results resources. Send the source to the returned ingress without waiting for Recording Hub."
            ),
        };
        Ok(GetPromptResult::new(vec![PromptMessage::new_text(
            Role::User,
            text,
        )]))
    }
}

fn required(name: &str, description: &str) -> PromptArgument {
    PromptArgument::new(name)
        .with_description(description)
        .with_required(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_prompt_requires_an_admitted_pipeline_id_and_builds_its_address() {
        let render = |id: &str| {
            StreamPrompt::StartLiveSession.render(Some(
                [("pipeline_id".into(), Value::String(id.into()))]
                    .into_iter()
                    .collect(),
            ))
        };
        let response = serde_json::to_string(&render("camera-2").unwrap()).unwrap();
        assert!(response.contains("stream://pipeline/camera-2"));
        assert!(StreamPrompt::StartLiveSession.render(None).is_err());
        for id in ["camera/other", "camera?cursor=bad", "", "Camera"] {
            assert!(render(id).is_err());
        }
    }
}
