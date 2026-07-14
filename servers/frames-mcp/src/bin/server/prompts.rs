use rmcp::{
    ErrorData as McpError,
    model::{GetPromptResult, JsonObject, Prompt, PromptArgument, PromptMessage, Role},
    schemars,
};
use serde_json::Value;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct FrameAuditArgs {
    mission: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct LocalFrameSelectArgs {
    workflow: String,
    origin_hint: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct TransformExplainArgs {
    operation_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FramesPrompt {
    FrameAudit,
    LocalFrameSelect,
    TransformExplain,
}

impl FramesPrompt {
    pub(super) const ALL: [Self; 3] = [
        Self::FrameAudit,
        Self::LocalFrameSelect,
        Self::TransformExplain,
    ];

    pub(super) fn by_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|prompt| prompt.name() == name)
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::FrameAudit => "frames-frame-audit",
            Self::LocalFrameSelect => "frames-local-frame-select",
            Self::TransformExplain => "frames-transform-explain",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::FrameAudit => "Coordinate frame audit",
            Self::LocalFrameSelect => "Local frame selection",
            Self::TransformExplain => "Transform explanation",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::FrameAudit => {
                "Audit a mission request for frame, unit, axis, datum, and origin assumptions."
            }
            Self::LocalFrameSelect => {
                "Choose ENU or NED and an origin strategy for a robot workflow."
            }
            Self::TransformExplain => {
                "Explain a recorded coordinate operation and its assumptions."
            }
        }
    }

    fn arguments(self) -> Vec<PromptArgument> {
        match self {
            Self::FrameAudit => vec![required_arg("mission", "Mission or tool request to audit.")],
            Self::LocalFrameSelect => vec![
                required_arg("workflow", "Robot, UAV, or simulation workflow."),
                optional_arg("origin_hint", "Candidate local-frame origin."),
            ],
            Self::TransformExplain => vec![required_arg(
                "operation_id",
                "Coordinate operation id from frames://operation/{operation_id}.",
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
            Self::FrameAudit => {
                let args: FrameAuditArgs = parse_prompt_args(self.name(), arguments)?;
                Ok(prompt_text(
                    self.description(),
                    format!(
                        "Audit this mission for coordinate safety:\n\n{}\n\n\
                         Identify every frame, datum, unit, axis order, local-frame origin, \
                         body-frame convention, and approximation assumption. Recommend exact \
                         Frames MCP tools and resources to call before execution. Refer Earth CRS \
                         or geofence work to Map MCP.",
                        args.mission
                    ),
                ))
            }
            Self::LocalFrameSelect => {
                let args: LocalFrameSelectArgs = parse_prompt_args(self.name(), arguments)?;
                Ok(prompt_text(
                    self.description(),
                    format!(
                        "Select a local tangent frame for this workflow:\n\n{}\n\n\
                         Origin hint: {}\n\n\
                         Choose ENU or NED, state the origin and datum, and draft a \
                         derive_local_frame request.",
                        args.workflow,
                        args.origin_hint.as_deref().unwrap_or("not specified")
                    ),
                ))
            }
            Self::TransformExplain => {
                let args: TransformExplainArgs = parse_prompt_args(self.name(), arguments)?;
                Ok(prompt_text(
                    self.description(),
                    format!(
                        "Read frames://operation/{} and explain the transform chain, \
                         source/target frame, datum assumptions, engine, \
                         approximation status, and any warnings in operator-readable language.",
                        args.operation_id
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
