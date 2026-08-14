use rmcp::{
    ErrorData as McpError,
    model::{GetPromptResult, JsonObject, Prompt, PromptArgument, PromptMessage, Role},
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Copy)]
pub(super) enum UavSimPrompt {
    MissionPlan,
    SessionReview,
}

impl UavSimPrompt {
    pub(super) const ALL: [Self; 2] = [Self::MissionPlan, Self::SessionReview];

    pub(super) fn by_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|prompt| prompt.name() == name)
    }

    fn name(self) -> &'static str {
        match self {
            Self::MissionPlan => "uav-sim-mission-plan",
            Self::SessionReview => "uav-sim-session-review",
        }
    }

    pub(super) fn definition(self) -> Prompt {
        let (title, description, arguments) = match self {
            Self::MissionPlan => (
                "Prepare UAV mission",
                "Prepare a typed mission against declared simulation vehicles and frames.",
                vec![
                    required("session_id", "Simulation session identity."),
                    required("mission_id", "New mission identity."),
                    optional("objective", "Bounded mission objective."),
                ],
            ),
            Self::SessionReview => (
                "Review UAV simulation session",
                "Inspect world, tile, native sensor-stream, vehicle, collision, recording, and task evidence.",
                vec![required("session_id", "Simulation session identity.")],
            ),
        };
        Prompt::new(self.name(), Some(description), Some(arguments)).with_title(title)
    }

    pub(super) fn render(self, arguments: Option<JsonObject>) -> Result<GetPromptResult, McpError> {
        #[derive(Deserialize)]
        struct Args {
            session_id: String,
            mission_id: Option<String>,
            objective: Option<String>,
        }
        let args: Args = serde_json::from_value(Value::Object(arguments.unwrap_or_default()))
            .map_err(|error| McpError::invalid_params(error.to_string(), None))?;
        let text = match self {
            Self::MissionPlan => format!(
                "Read uav-sim://session/{0}, uav-sim://session/{0}/world, uav-sim://control-grants, and uav-sim://session/{0}/vehicles. Ask Map MCP to resolve and route the objective, then call Map's prepare_route_handoff tool. Pass that handoff unchanged to prepare_vehicle_mission for mission {1} and only the vehicle granted to your authenticated principal. Objective: {2}. Do not invent coordinates, restrictions, mobility profiles, or world revisions. Do not call execute_vehicle_mission_plan until the operator accepts the admitted plan.",
                args.session_id,
                args.mission_id.as_deref().unwrap_or("unspecified"),
                args.objective.as_deref().unwrap_or("unspecified")
            ),
            Self::SessionReview => format!(
                "Read uav-sim://session/{0}, uav-sim://session/{0}/world, uav-sim://session/{0}/tiles, uav-sim://session/{0}/vehicles, and uav-sim://session/{0}/recordings. Report tile readiness, native sensor-stream health, frame identity, PX4 connectivity, flight states, collisions, recording availability, and relevant durable task evidence.",
                args.session_id
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
