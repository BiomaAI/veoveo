use rmcp::{
    ErrorData as McpError,
    model::{GetPromptResult, JsonObject, Prompt, PromptArgument, PromptMessage, Role},
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Copy)]
pub(super) enum ReasonPrompt {
    AnalyzeRecording,
    AnswerQuestion,
}

impl ReasonPrompt {
    pub(super) const ALL: [Self; 2] = [Self::AnalyzeRecording, Self::AnswerQuestion];

    pub(super) fn by_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|prompt| prompt.name() == name)
    }

    fn name(self) -> &'static str {
        match self {
            Self::AnalyzeRecording => "reason-analyze-recording",
            Self::AnswerQuestion => "reason-answer-question",
        }
    }

    pub(super) fn definition(self) -> Prompt {
        let (title, description, arguments) = match self {
            Self::AnalyzeRecording => (
                "Reason over recorded video",
                "Prepare a governed world-model reasoning analysis over a Rerun video range.",
                vec![
                    required("recording_uri", "Canonical recording resource URI."),
                    required("entity_path", "Rerun VideoStream entity path."),
                    required("timeline", "Rerun timeline name."),
                    required("start", "Inclusive raw timeline index."),
                    required("end", "Inclusive raw timeline index."),
                    required("pipeline_id", "Reason pipeline identifier."),
                    required("prompt", "What to describe or which events to detect."),
                ],
            ),
            Self::AnswerQuestion => (
                "Answer a question about recorded video",
                "Prepare a governed question-answering pass over a Rerun video range.",
                vec![
                    required("recording_uri", "Canonical recording resource URI."),
                    required("entity_path", "Rerun VideoStream entity path."),
                    required("timeline", "Rerun timeline name."),
                    required("start", "Inclusive raw timeline index."),
                    required("end", "Inclusive raw timeline index."),
                    required("pipeline_id", "Reason pipeline identifier."),
                    required("question", "The question to answer about the range."),
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
            pipeline_id: String,
            prompt: Option<String>,
            question: Option<String>,
        }
        let args: Args = serde_json::from_value(Value::Object(arguments.unwrap_or_default()))
            .map_err(|error| McpError::invalid_params(error.to_string(), None))?;
        let text = match self {
            Self::AnalyzeRecording => format!(
                "Read reason://pipelines and verify pipeline {}. Call analyze_recording with video recording_uri {}, entity_path {}, timeline {}, durable range {}..={}, and a describe_segment or detect_events task using: {}. Optionally pass the results artifact of a completed perception analysis as grounding so events can cite track identities. Treat the returned analysis and artifact URIs as canonical.",
                args.pipeline_id,
                args.recording_uri,
                args.entity_path,
                args.timeline,
                args.start,
                args.end,
                args.prompt.as_deref().unwrap_or("<required>"),
            ),
            Self::AnswerQuestion => format!(
                "Call analyze_recording with video recording_uri {}, entity_path {}, timeline {}, durable range {}..={}, pipeline {}, and an answer_question task asking: {}. Return the typed answer and cite the reason://analysis resource.",
                args.recording_uri,
                args.entity_path,
                args.timeline,
                args.start,
                args.end,
                args.pipeline_id,
                args.question.as_deref().unwrap_or("<required>"),
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
