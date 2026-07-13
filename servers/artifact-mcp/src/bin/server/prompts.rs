use rmcp::{
    ErrorData as McpError,
    model::{GetPromptResult, JsonObject, Prompt, PromptArgument, PromptMessage, Role},
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
struct ShareReviewArgs {
    artifact_id: String,
    audience: String,
}

#[derive(Debug, Deserialize)]
struct AccessReviewArgs {
    artifact_id: String,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum ArtifactPrompt {
    ShareReview,
    AccessReview,
}

impl ArtifactPrompt {
    pub(super) const ALL: [Self; 2] = [Self::ShareReview, Self::AccessReview];

    pub(super) fn by_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|value| value.name() == name)
    }

    fn name(self) -> &'static str {
        match self {
            Self::ShareReview => "artifact-share-review",
            Self::AccessReview => "artifact-access-review",
        }
    }

    pub(super) fn prompt(self) -> Prompt {
        match self {
            Self::ShareReview => Prompt::new(
                self.name(),
                Some("Review whether to use an authorized grant or anyone-with-link sharing."),
                Some(vec![
                    PromptArgument::new("artifact_id").with_required(true),
                    PromptArgument::new("audience").with_required(true),
                ]),
            )
            .with_title("Artifact sharing review"),
            Self::AccessReview => Prompt::new(
                self.name(),
                Some("Review current artifact grants and release state."),
                Some(vec![PromptArgument::new("artifact_id").with_required(true)]),
            )
            .with_title("Artifact access review"),
        }
    }

    pub(super) fn render(self, arguments: Option<JsonObject>) -> Result<GetPromptResult, McpError> {
        let value = Value::Object(arguments.unwrap_or_default());
        let text = match self {
            Self::ShareReview => {
                let args: ShareReviewArgs = parse(self.name(), value)?;
                format!(
                    "Review artifact {} for sharing with {}. Read its metadata and grants. Use a named user/group grant for an authorized audience. Use an anyone-with-link share only when the artifact is explicitly releasable, with the shortest practical expiry and an optional download cap. Do not expose the bearer URL outside the intended audience.",
                    args.artifact_id, args.audience
                )
            }
            Self::AccessReview => {
                let args: AccessReviewArgs = parse(self.name(), value)?;
                format!(
                    "Review artifact {}. Read artifact://metadata/{} and artifact://grants/{}. Report its owner, labels, retention, release state, and effective named grants. Flag public-link eligibility separately from authorized access.",
                    args.artifact_id, args.artifact_id, args.artifact_id
                )
            }
        };
        Ok(GetPromptResult::new(vec![PromptMessage::new_text(
            Role::User,
            text,
        )]))
    }
}

fn parse<T: serde::de::DeserializeOwned>(name: &str, value: Value) -> Result<T, McpError> {
    serde_json::from_value(value).map_err(|error| {
        McpError::invalid_params(format!("invalid {name} arguments: {error}"), None)
    })
}
