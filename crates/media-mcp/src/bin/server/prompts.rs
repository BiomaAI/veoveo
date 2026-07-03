use rmcp::{
    ErrorData as McpError,
    model::{GetPromptResult, JsonObject, Prompt, PromptArgument, PromptMessage, Role},
    schemars,
};
use serde_json::Value;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct ModelSelectPromptArgs {
    goal: String,
    media_type: Option<String>,
    budget: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct ImageEditPromptArgs {
    image_url: String,
    edit_goal: String,
    constraints: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct VideoPromptArgs {
    brief: String,
    reference_url: Option<String>,
    duration: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct TaskReviewPromptArgs {
    task_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MediaPrompt {
    ModelSelect,
    ImageEdit,
    VideoGenerate,
    TaskReview,
}

impl MediaPrompt {
    pub(super) const ALL: [Self; 4] = [
        Self::ModelSelect,
        Self::ImageEdit,
        Self::VideoGenerate,
        Self::TaskReview,
    ];

    pub(super) fn by_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|prompt| prompt.name() == name)
    }

    fn name(self) -> &'static str {
        match self {
            Self::ModelSelect => "media-model-select",
            Self::ImageEdit => "media-image-edit",
            Self::VideoGenerate => "media-video-generate",
            Self::TaskReview => "media-task-review",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::ModelSelect => "Media model selection",
            Self::ImageEdit => "Image edit request",
            Self::VideoGenerate => "Video generation request",
            Self::TaskReview => "Media task review",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::ModelSelect => "Select media models and draft valid run arguments for a goal.",
            Self::ImageEdit => "Draft an image edit request using a source image URL.",
            Self::VideoGenerate => "Draft a video generation request from a creative brief.",
            Self::TaskReview => "Review task outputs, artifacts, and usage for a completed run.",
        }
    }

    fn arguments(self) -> Vec<PromptArgument> {
        match self {
            Self::ModelSelect => vec![
                required_arg("goal", "User goal for the media generation task."),
                optional_arg(
                    "media_type",
                    "Desired output type, such as image, video, audio, or 3D.",
                ),
                optional_arg("budget", "Budget or cost guidance for model selection."),
            ],
            Self::ImageEdit => vec![
                required_arg("image_url", "Public URL of the source image."),
                required_arg("edit_goal", "Specific visual change requested by the user."),
                optional_arg(
                    "constraints",
                    "Style, brand, safety, or composition constraints.",
                ),
            ],
            Self::VideoGenerate => vec![
                required_arg("brief", "Creative brief for the video."),
                optional_arg(
                    "reference_url",
                    "Optional public image or video reference URL.",
                ),
                optional_arg("duration", "Desired duration guidance."),
            ],
            Self::TaskReview => vec![required_arg(
                "task_id",
                "MCP task id returned by the run tool.",
            )],
        }
    }

    pub(super) fn prompt(self) -> Prompt {
        Prompt::new(
            self.name(),
            Some(self.description()),
            Some(self.arguments()),
        )
        .with_title(self.title())
    }

    pub(super) fn render(self, arguments: Option<JsonObject>) -> Result<GetPromptResult, McpError> {
        match self {
            Self::ModelSelect => {
                let args: ModelSelectPromptArgs = parse_prompt_args(self.name(), arguments)?;
                Ok(prompt_text(
                    self.description(),
                    format!(
                        "Prepare a media model selection for this goal:\n\n\
                         Goal: {}\n\
                         Media type: {}\n\
                         Budget guidance: {}\n\n\
                         Read media://models, choose the best candidate model ids, then read \
                         media://model/{{model_id}} for each candidate before drafting run \
                         arguments. Return the selected model id and a JSON input object that \
                         conforms exactly to the selected model schema.",
                        args.goal,
                        args.media_type.as_deref().unwrap_or("not specified"),
                        args.budget.as_deref().unwrap_or("not specified"),
                    ),
                ))
            }
            Self::ImageEdit => {
                let args: ImageEditPromptArgs = parse_prompt_args(self.name(), arguments)?;
                Ok(prompt_text(
                    self.description(),
                    format!(
                        "Draft an image edit run request.\n\n\
                         Source image URL: {}\n\
                         Edit goal: {}\n\
                         Constraints: {}\n\n\
                         Read media://models and prefer an image edit or image-to-image model. \
                         Then read media://model/{{model_id}} and produce only the model id plus \
                         an input JSON object that validates against that model schema.",
                        args.image_url,
                        args.edit_goal,
                        args.constraints.as_deref().unwrap_or("not specified"),
                    ),
                ))
            }
            Self::VideoGenerate => {
                let args: VideoPromptArgs = parse_prompt_args(self.name(), arguments)?;
                Ok(prompt_text(
                    self.description(),
                    format!(
                        "Draft a video generation run request.\n\n\
                         Brief: {}\n\
                         Reference URL: {}\n\
                         Duration guidance: {}\n\n\
                         Read media://models and choose a video-capable model. Then read \
                         media://model/{{model_id}} and produce only the model id plus an input \
                         JSON object that validates against that model schema.",
                        args.brief,
                        args.reference_url.as_deref().unwrap_or("not specified"),
                        args.duration.as_deref().unwrap_or("not specified"),
                    ),
                ))
            }
            Self::TaskReview => {
                let args: TaskReviewPromptArgs = parse_prompt_args(self.name(), arguments)?;
                Ok(prompt_text(
                    self.description(),
                    format!(
                        "Review media task {}.\n\n\
                         Read tasks/get for current status. If completed, read tasks/result, \
                         inspect any media://artifact/{{sha256}} links, and read \
                         media://usage/task/{} for estimate and actual billing records. Summarize \
                         artifact count, output types, final cost, and any missing actual usage.",
                        args.task_id, args.task_id,
                    ),
                ))
            }
        }
    }
}

fn required_arg(name: &str, description: &str) -> PromptArgument {
    PromptArgument::new(name)
        .with_description(description)
        .with_required(true)
}

fn optional_arg(name: &str, description: &str) -> PromptArgument {
    PromptArgument::new(name)
        .with_description(description)
        .with_required(false)
}

fn parse_prompt_args<T: serde::de::DeserializeOwned>(
    prompt_name: &str,
    arguments: Option<JsonObject>,
) -> Result<T, McpError> {
    serde_json::from_value(Value::Object(arguments.unwrap_or_default())).map_err(|e| {
        McpError::invalid_params(
            format!("invalid arguments for prompt {prompt_name}: {e}"),
            None,
        )
    })
}

fn prompt_text(description: &str, text: String) -> GetPromptResult {
    GetPromptResult::new(vec![PromptMessage::new_text(Role::User, text)])
        .with_description(description)
}
