use rmcp::{
    ErrorData as McpError,
    model::{GetPromptResult, JsonObject, Prompt, PromptArgument, PromptMessage, Role},
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Copy)]
pub enum MapPrompt {
    PrepareRoute,
    ReviewRoute,
    PrepareMatrix,
}

impl MapPrompt {
    pub const ALL: [Self; 3] = [Self::PrepareRoute, Self::ReviewRoute, Self::PrepareMatrix];

    pub fn by_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|value| value.name() == name)
    }

    fn name(self) -> &'static str {
        match self {
            Self::PrepareRoute => "prepare_route_request",
            Self::ReviewRoute => "review_route",
            Self::PrepareMatrix => "prepare_logistics_matrix",
        }
    }

    pub fn definition(self) -> Prompt {
        let (title, description, arguments) = match self {
            Self::PrepareRoute => (
                "Prepare route request",
                "Prepare a route call from visible profiles, endpoints, validity, and data policy.",
                vec![
                    required("mobility_profile_id", "Visible mobility profile id."),
                    required("origin", "Origin location, facility, or WGS84 position."),
                    required(
                        "destination",
                        "Destination location, facility, or WGS84 position.",
                    ),
                    optional("departure_time", "ISO 8601 departure time."),
                ],
            ),
            Self::ReviewRoute => (
                "Review route",
                "Review provenance, status, restrictions, reserves, and alternatives before use.",
                vec![required("route_id", "Visible route id.")],
            ),
            Self::PrepareMatrix => (
                "Prepare logistics matrix",
                "Prepare a bounded many-to-many matrix for Optimization MCP.",
                vec![
                    required("mobility_profile_id", "Visible mobility profile id."),
                    required("origins", "Comma-separated origin ids or positions."),
                    required(
                        "destinations",
                        "Comma-separated destination ids or positions.",
                    ),
                ],
            ),
        };
        Prompt::new(self.name(), Some(description), Some(arguments)).with_title(title)
    }

    pub fn render(self, arguments: Option<JsonObject>) -> Result<GetPromptResult, McpError> {
        #[derive(Deserialize)]
        struct Arguments {
            mobility_profile_id: Option<String>,
            origin: Option<String>,
            destination: Option<String>,
            departure_time: Option<String>,
            route_id: Option<String>,
            origins: Option<String>,
            destinations: Option<String>,
        }
        let arguments: Arguments =
            serde_json::from_value(Value::Object(arguments.unwrap_or_default()))
                .map_err(|error| McpError::invalid_params(error.to_string(), None))?;
        let text = match self {
            Self::PrepareRoute => format!(
                "Read map://mobility-profile/{profile}/{{profile_version}} and inspect the endpoint resources for {origin} and {destination}. Confirm profile validity at {departure}. Invoke route as a durable task with an explicit objective, constraints, required map families, and whether planning-advisory output is acceptable. Do not replace unavailable coverage with straight-line geometry.",
                profile = required_value(arguments.mobility_profile_id, "mobility_profile_id")?,
                origin = required_value(arguments.origin, "origin")?,
                destination = required_value(arguments.destination, "destination")?,
                departure = arguments
                    .departure_time
                    .as_deref()
                    .unwrap_or("the intended departure time"),
            ),
            Self::ReviewRoute => format!(
                "Read map://route/{route}. Check status, validation id, base release ids, operational snapshot, restrictions, facilities, arrival time, reserves represented in costs, and every alternative. Reject stale, invalidated, unavailable, or disallowed planning-advisory output.",
                route = required_value(arguments.route_id, "route_id")?,
            ),
            Self::PrepareMatrix => format!(
                "Read the profile {profile}, resolve origins [{origins}] and destinations [{destinations}], and keep the request within 20 by 20 and 400 total cells. Invoke route_matrix as a durable task. Pass the resulting map://matrix/{{matrix_id}} resource to Optimization MCP without recomputing GIS costs.",
                profile = required_value(arguments.mobility_profile_id, "mobility_profile_id")?,
                origins = required_value(arguments.origins, "origins")?,
                destinations = required_value(arguments.destinations, "destinations")?,
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

fn required_value(value: Option<String>, name: &str) -> Result<String, McpError> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| McpError::invalid_params(format!("missing prompt argument `{name}`"), None))
}
