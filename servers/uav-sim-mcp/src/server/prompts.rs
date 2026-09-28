use crate::{
    contract::{MissionId, SessionId},
    uris,
};
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
            session_id: SessionId,
            mission_id: Option<MissionId>,
            objective: Option<String>,
        }
        let args: Args = serde_json::from_value(Value::Object(arguments.unwrap_or_default()))
            .map_err(|error| McpError::invalid_params(error.to_string(), None))?;
        let session = uris::session(&args.session_id);
        let world = uris::world(&args.session_id);
        let vehicles = uris::vehicles(&args.session_id);
        let text = match self {
            Self::MissionPlan => format!(
                "Read {session}, {world}, {}, and {vehicles}. Ask Map MCP to resolve and route the objective, then call Map's prepare_route_handoff tool. Pass that handoff unchanged to prepare_vehicle_mission for mission {} and only the vehicle granted to your authenticated principal. Objective: {}. Do not invent coordinates, restrictions, mobility profiles, or world revisions. Do not call execute_vehicle_mission_plan until the operator accepts the admitted plan.",
                uris::CONTROL_GRANTS,
                args.mission_id
                    .ok_or_else(|| McpError::invalid_params("mission_id is required", None))?,
                args.objective.as_deref().unwrap_or("unspecified")
            ),
            Self::SessionReview => format!(
                "Read {session}, {world}, {}, {vehicles}, and {}. Report tile readiness, native sensor-stream health, frame identity, PX4 connectivity, flight states, collisions, recording availability, and results of relevant Tasks.",
                uris::tiles(&args.session_id),
                uris::recordings(&args.session_id)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompts_validate_ids_before_building_resource_addresses() {
        for prompt in UavSimPrompt::ALL {
            for id in ["..", "s/world", "s?cursor=x", "s#fragment"] {
                let args = serde_json::json!({"session_id": id, "mission_id": "m"});
                assert!(
                    prompt
                        .render(Some(args.as_object().unwrap().clone()))
                        .is_err()
                );
            }
            let args = serde_json::json!({"session_id": "Session_1.2", "mission_id": "mission-1"});
            let result = prompt
                .render(Some(args.as_object().unwrap().clone()))
                .unwrap();
            let text = serde_json::to_string(&result).unwrap();
            assert!(text.contains("uav-sim://session/Session_1.2/world"));
            assert!(text.contains("uav-sim://session/Session_1.2/vehicles"));
        }
        for args in [
            serde_json::json!({"session_id":"s"}),
            serde_json::json!({"session_id":"s","mission_id":".."}),
        ] {
            assert!(
                UavSimPrompt::MissionPlan
                    .render(Some(args.as_object().unwrap().clone()))
                    .is_err()
            );
        }
    }
}
