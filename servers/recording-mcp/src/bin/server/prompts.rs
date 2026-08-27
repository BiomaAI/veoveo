use rmcp::{
    ErrorData as McpError,
    model::{GetPromptResult, JsonObject, Prompt, PromptArgument, PromptMessage, Role},
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Copy)]
pub(super) enum RecordingPrompt {
    Inspect,
    Project,
    Seal,
}

impl RecordingPrompt {
    pub(super) const ALL: [Self; 3] = [Self::Inspect, Self::Project, Self::Seal];

    pub(super) fn by_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|prompt| prompt.name() == name)
    }

    fn name(self) -> &'static str {
        match self {
            Self::Inspect => "recording-inspect",
            Self::Project => "recording-project",
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
            Self::Project => (
                "Project recording",
                "Draft a deterministic bounded Apache Arrow projection.",
                vec![
                    required("dataset_id", "Recording dataset UUIDv7."),
                    required("recording_id", "Recording UUIDv7."),
                    optional("timeline", "Rerun timeline name."),
                    optional("entity_path", "Exact Rerun entity path."),
                    optional("component_id", "Exact Rerun component identifier."),
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
            dataset_id: Option<String>,
            recording_id: String,
            timeline: Option<String>,
            entity_path: Option<String>,
            component_id: Option<String>,
        }
        let args: Args = serde_json::from_value(Value::Object(arguments.unwrap_or_default()))
            .map_err(|error| McpError::invalid_params(error.to_string(), None))?;
        let text = match self {
            Self::Inspect => format!(
                "Read recording://recordings/{} and recording://recordings/{}/layers. Report lifecycle state, classification, labels, layer health, and Artifact availability.",
                args.recording_id, args.recording_id
            ),
            Self::Project => format!(
                "Call create_recording_projection with dataset_id {}, recording_id {}, timeline {}, exact entity path {}, exact component identifier {}, explicit sampling, fixed row/byte/deadline bounds, and a fresh idempotency key. Consume the returned Arrow stream through the authorized host path and summarize only the projected result metadata.",
                args.dataset_id.as_deref().unwrap_or("<dataset UUIDv7>"),
                args.recording_id,
                args.timeline.as_deref().unwrap_or("tick"),
                args.entity_path.as_deref().unwrap_or("<exact entity path>"),
                args.component_id
                    .as_deref()
                    .unwrap_or("<exact component identifier>")
            ),
            Self::Seal => format!(
                "Read recording://recordings/{0} and recording://recordings/{0}/layers. Only if every immutable layer is committed, call seal_recording for {0}; then report the manifest and layer Artifact URIs.",
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
