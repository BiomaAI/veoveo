use rmcp::{
    ErrorData as McpError,
    model::{GetPromptResult, JsonObject, Prompt, PromptArgument, PromptMessage, Role},
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Copy)]
pub(super) enum PerceptionPrompt {
    AnalyzeRecording,
    ExtractClip,
}

impl PerceptionPrompt {
    pub(super) const ALL: [Self; 2] = [Self::AnalyzeRecording, Self::ExtractClip];

    pub(super) fn by_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|prompt| prompt.name() == name)
    }

    fn name(self) -> &'static str {
        match self {
            Self::AnalyzeRecording => "perception-analyze-recording",
            Self::ExtractClip => "perception-extract-clip",
        }
    }

    pub(super) fn definition(self) -> Prompt {
        let (title, description, arguments) = match self {
            Self::AnalyzeRecording => (
                "Analyze recorded video",
                "Prepare a governed NVIDIA-backed perception analysis over a Rerun video range.",
                vec![
                    required("recording_uri", "Canonical recording resource URI."),
                    required("entity_path", "Rerun VideoStream entity path."),
                    required("timeline", "Rerun timeline name."),
                    required("start", "Inclusive raw timeline index."),
                    required("end", "Inclusive raw timeline index."),
                    required("pipeline_id", "Perception pipeline identifier."),
                ],
            ),
            Self::ExtractClip => (
                "Extract recorded clip",
                "Prepare a governed MP4 extraction from Rerun VideoStream samples.",
                vec![
                    required("recording_uri", "Canonical recording resource URI."),
                    required("entity_path", "Rerun VideoStream entity path."),
                    required("timeline", "Rerun timeline name."),
                    required("start", "Inclusive raw timeline index."),
                    required("end", "Inclusive raw timeline index."),
                ],
            ),
        };
        Prompt::new(self.name(), Some(description), Some(arguments)).with_title(title)
    }

    pub(super) fn render(self, arguments: Option<JsonObject>) -> Result<GetPromptResult, McpError> {
        #[derive(Deserialize)]
        struct Args {
            recording_uri: String,
            entity_path: String,
            timeline: String,
            start: i64,
            end: i64,
            pipeline_id: Option<String>,
        }
        let args: Args = serde_json::from_value(Value::Object(arguments.unwrap_or_default()))
            .map_err(|error| McpError::invalid_params(error.to_string(), None))?;
        let text = match self {
            Self::AnalyzeRecording => format!(
                "Read perception://pipelines and verify pipeline {}. Call analyze_recording with video recording_uri {}, entity_path {}, timeline {}, durable range {}..={}, and the selected pipeline. Treat the returned analysis and artifact URIs as canonical.",
                args.pipeline_id.as_deref().unwrap_or("<required>"),
                args.recording_uri,
                args.entity_path,
                args.timeline,
                args.start,
                args.end,
            ),
            Self::ExtractClip => format!(
                "Call extract_clip with video recording_uri {}, entity_path {}, timeline {}, and durable range {}..={}. Return the governed MP4 artifact URI.",
                args.recording_uri, args.entity_path, args.timeline, args.start, args.end,
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
