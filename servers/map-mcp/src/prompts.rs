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
    AuthorFeatureLayer,
}

impl MapPrompt {
    pub const ALL: [Self; 4] = [
        Self::PrepareRoute,
        Self::ReviewRoute,
        Self::PrepareMatrix,
        Self::AuthorFeatureLayer,
    ];

    pub fn by_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|value| value.name() == name)
    }

    fn name(self) -> &'static str {
        match self {
            Self::PrepareRoute => "prepare_route_request",
            Self::ReviewRoute => "review_route",
            Self::PrepareMatrix => "prepare_logistics_matrix",
            Self::AuthorFeatureLayer => "author_feature_layer",
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
            Self::AuthorFeatureLayer => (
                "Author feature layer",
                "Prepare a governed feature-layer change, validation, commit, and optional publication.",
                vec![
                    required(
                        "objective",
                        "The map content to create or change and its intended use.",
                    ),
                    optional(
                        "layer_id",
                        "Existing feature layer id, when updating a layer.",
                    ),
                    optional(
                        "source_artifact_id",
                        "Authorized artifact id for a bulk GeoJSON import.",
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
            objective: Option<String>,
            layer_id: Option<String>,
            source_artifact_id: Option<String>,
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
            Self::AuthorFeatureLayer => format!(
                "Author this map content: {objective}. {layer} Read the layer, schema, and style resources before changing existing content. Use validate_feature_changes before commit_feature_changes, keep the returned base revisions unchanged, and resolve every conflict explicitly rather than overwriting it. Use import_feature_layer as a durable task for {input}; use direct changesets only for bounded interactive edits. Query the committed head and inspect its changeset resource. Publish only when an immutable release is required, then use export_feature_layer or build_vector_tiles as durable tasks for derived artifacts. Never treat generic authored features as routing restrictions or routable network data.",
                objective = required_value(arguments.objective, "objective")?,
                layer = arguments.layer_id.as_deref().map_or_else(
                    || "Create a layer with an explicit content class, JSON Schema 2020-12 property contract, and safe style.".to_owned(),
                    |layer_id| format!("Work in map://feature-layer/{layer_id}.")
                ),
                input = arguments.source_artifact_id.as_deref().unwrap_or(
                    "an authorized RFC 7946 GeoJSON, OGC JSON-FG 1.0, or RFC 8142 sequence artifact"
                ),
            ),
        };
        Ok(GetPromptResult::new(vec![PromptMessage::new_text(
            Role::User,
            text,
        )]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn authoring_prompt_requires_an_objective_and_explains_governance() {
        let arguments = serde_json::from_value(json!({
            "objective": "map inspected culverts",
            "layer_id": "feature-layer-019be7be-68f8-7000-8000-000000000001"
        }))
        .unwrap();
        let rendered = MapPrompt::AuthorFeatureLayer
            .render(Some(arguments))
            .unwrap();
        let text = serde_json::to_string(&rendered).unwrap();
        assert!(text.contains("validate_feature_changes"));
        assert!(text.contains("commit_feature_changes"));
        assert!(text.contains("Never treat generic authored features as routing restrictions"));
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
