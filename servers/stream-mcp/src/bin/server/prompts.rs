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
            Self::RunRecording => "stream-run-recording",
            Self::StartLiveSession => "stream-start-live-session",
        }
    }

    pub(super) fn definition(self) -> Prompt {
        let (title, description, arguments) = match self {
            Self::RunRecording => (
                "Run a pipeline over recorded video",
                "Prepare a governed GPU-backed Stream run over a Rerun video range.",
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
                "Start one operator-admitted live GStreamer graph and return its typed ingress.",
                vec![required("pipeline_id", "Live Stream pipeline identifier.")],
            ),
        };
        Prompt::new(self.name(), Some(description), Some(arguments)).with_title(title)
    }

    pub(super) fn render(self, arguments: Option<JsonObject>) -> Result<GetPromptResult, McpError> {
        #[derive(Deserialize)]
        struct Args {
            recording_uri: Option<String>,
            entity_path: Option<String>,
            timeline: Option<String>,
            start: Option<i64>,
            end: Option<i64>,
            pipeline_id: Option<String>,
        }
        let args: Args = serde_json::from_value(Value::Object(arguments.unwrap_or_default()))
            .map_err(|error| McpError::invalid_params(error.to_string(), None))?;
        let pipeline_id = args.pipeline_id.as_deref().unwrap_or("<required>");
        let text = match self {
            Self::RunRecording => format!(
                "Read stream://pipelines and verify pipeline {pipeline_id}. Call run_recording with video recording_uri {}, entity_path {}, timeline {}, durable range {}..={}, and the selected pipeline. Treat the returned run and artifact URIs as canonical.",
                args.recording_uri.as_deref().unwrap_or("<required>"),
                args.entity_path.as_deref().unwrap_or("<required>"),
                args.timeline.as_deref().unwrap_or("<required>"),
                args.start
                    .map_or_else(|| "<required>".to_owned(), |value| value.to_string()),
                args.end
                    .map_or_else(|| "<required>".to_owned(), |value| value.to_string()),
            ),
            Self::StartLiveSession => format!(
                "Read stream://pipeline/{pipeline_id} and verify it supports live input. Call start_live_session with that pipeline_id, then subscribe to the returned session and results resources. Send the source to the returned ingress without waiting for Recording Hub."
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
