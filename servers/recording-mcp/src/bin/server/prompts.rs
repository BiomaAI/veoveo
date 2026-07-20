use rmcp::{
    ErrorData as McpError,
    model::{GetPromptResult, JsonObject, Prompt, PromptArgument, PromptMessage, Role},
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Copy)]
pub(super) enum RecordingPrompt {
    Inspect,
    Query,
    Seal,
}

impl RecordingPrompt {
    pub(super) const ALL: [Self; 3] = [Self::Inspect, Self::Query, Self::Seal];

    pub(super) fn by_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|prompt| prompt.name() == name)
    }

    fn name(self) -> &'static str {
        match self {
            Self::Inspect => "recording-inspect",
            Self::Query => "recording-query",
            Self::Seal => "recording-seal",
        }
    }

    pub(super) fn definition(self) -> Prompt {
        let (title, description, arguments) = match self {
            Self::Inspect => (
                "Inspect recording",
                "Inspect governed recording metadata and segment state.",
                vec![required("recording_id", "Recording UUIDv7.")],
            ),
            Self::Query => (
                "Query recording",
                "Draft a bounded temporal recording query.",
                vec![
                    required("recording_id", "Recording UUIDv7."),
                    optional("timeline", "Rerun timeline name."),
                    optional("entities", "Rerun entity path filter."),
                ],
            ),
            Self::Seal => (
                "Seal recording",
                "Validate and publish a recording as governed immutable artifacts.",
                vec![required("recording_id", "Recording UUIDv7.")],
            ),
        };
        Prompt::new(self.name(), Some(description), Some(arguments)).with_title(title)
    }

    pub(super) fn render(self, arguments: Option<JsonObject>) -> Result<GetPromptResult, McpError> {
        #[derive(Deserialize)]
        struct Args {
            recording_id: String,
            timeline: Option<String>,
            entities: Option<String>,
        }
        let args: Args = serde_json::from_value(Value::Object(arguments.unwrap_or_default()))
            .map_err(|error| McpError::invalid_params(error.to_string(), None))?;
        let text = match self {
            Self::Inspect => format!(
                "Read recording://recordings/{} and recording://recordings/{}/segments. Report lifecycle state, classification, labels, segment health, and artifact availability.",
                args.recording_id, args.recording_id
            ),
            Self::Query => format!(
                "Call query_recording with recording_id {}, timeline {}, entities {}, an inclusive range when the question is time-bounded, and a bounded max_rows. Summarize returned observations without claiming rows beyond the response.",
                args.recording_id,
                args.timeline.as_deref().unwrap_or("tick"),
                args.entities.as_deref().unwrap_or("/**")
            ),
            Self::Seal => format!(
                "Read recording://recordings/{0} and recording://recordings/{0}/segments. Only if every segment is frozen, call seal_recording for {0}; then report the manifest and segment artifact URIs.",
                args.recording_id
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

fn optional(name: &str, description: &str) -> PromptArgument {
    PromptArgument::new(name)
        .with_description(description)
        .with_required(false)
}
