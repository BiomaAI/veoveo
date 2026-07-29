use rmcp::{
    ErrorData as McpError,
    model::{GetPromptResult, JsonObject, Prompt, PromptArgument, PromptMessage, Role},
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Copy)]
pub(super) enum OptimizationPrompt {
    FormulateRouting,
    CompareRouteScenarios,
    FormulateMathematicalModel,
}

impl OptimizationPrompt {
    pub(super) const ALL: [Self; 3] = [
        Self::FormulateRouting,
        Self::CompareRouteScenarios,
        Self::FormulateMathematicalModel,
    ];

    pub(super) fn by_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|prompt| prompt.name() == name)
    }

    fn name(self) -> &'static str {
        match self {
            Self::FormulateRouting => "formulate_routing_problem",
            Self::CompareRouteScenarios => "compare_route_scenarios",
            Self::FormulateMathematicalModel => "formulate_mathematical_model",
        }
    }

    pub(super) fn definition(self) -> Prompt {
        let (title, description, arguments) = match self {
            Self::FormulateRouting => (
                "Formulate a routing problem",
                "Turn an operational routing objective into the cuOpt-native routing contract.",
                vec![
                    required(
                        "objective",
                        "Operational objective and service requirements.",
                    ),
                    required(
                        "travel_model_uri",
                        "Immutable map://travel-model URI to use.",
                    ),
                    optional(
                        "profile_uri",
                        "Solver profile URI; defaults to optimization://profile/balanced.",
                    ),
                ],
            ),
            Self::CompareRouteScenarios => (
                "Compare route scenarios",
                "Prepare a homogeneous GPU batch of routing alternatives.",
                vec![
                    required(
                        "question",
                        "Decision the scenario comparison should answer.",
                    ),
                    required(
                        "case_ids",
                        "Comma-separated stable case ids and their distinguishing assumptions.",
                    ),
                ],
            ),
            Self::FormulateMathematicalModel => (
                "Formulate a mathematical model",
                "Choose and formulate a convex or mixed-integer model for cuOpt.",
                vec![
                    required("objective", "Decision objective and business meaning."),
                    required("constraints", "Required constraints and units."),
                    optional(
                        "integer_decisions",
                        "Variables that must be integral or semi-continuous.",
                    ),
                ],
            ),
        };
        Prompt::new(self.name(), Some(description), Some(arguments)).with_title(title)
    }

    pub(super) fn render(self, arguments: Option<JsonObject>) -> Result<GetPromptResult, McpError> {
        #[derive(Deserialize)]
        struct Arguments {
            objective: Option<String>,
            travel_model_uri: Option<String>,
            profile_uri: Option<String>,
            question: Option<String>,
            case_ids: Option<String>,
            constraints: Option<String>,
            integer_decisions: Option<String>,
        }
        let arguments: Arguments =
            serde_json::from_value(Value::Object(arguments.unwrap_or_default()))
                .map_err(|error| McpError::invalid_params(error.to_string(), None))?;
        let text = match self {
            Self::FormulateRouting => format!(
                "Formulate this routing decision: {objective}. Read {travel_model} and preserve its location order, mobility profile, release pins, unavailable arcs, and cost units. Model service or pickup-delivery orders, but do not mix those order families in one cuOpt 26.06 problem. Give every order, vehicle, vehicle type, location, and capacity dimension a stable id. Make optional service explicit with a positive drop penalty and a positive prize objective weight. Read optimization://profiles, choose {profile}, validate windows and fleet bounds, then invoke optimize_routes as a durable task. Treat optimization://solution and its verification subresource as the decision record.",
                objective = required_value(arguments.objective, "objective")?,
                travel_model = required_value(arguments.travel_model_uri, "travel_model_uri")?,
                profile = arguments
                    .profile_uri
                    .as_deref()
                    .unwrap_or("optimization://profile/balanced"),
            ),
            Self::CompareRouteScenarios => format!(
                "Prepare a route-scenario batch answering: {question}. Use these stable cases: {cases}. Keep each case independently complete, materialize the same immutable travel-model inputs before submission, and use one solver profile for the GPU batch. Invoke optimize_route_scenarios as a durable task. Compare case objectives, vehicles used, dropped orders, feasibility, termination, and verification findings; do not compare unverified routes as operational equivalents.",
                question = required_value(arguments.question, "question")?,
                cases = required_value(arguments.case_ids, "case_ids")?,
            ),
            Self::FormulateMathematicalModel => format!(
                "Formulate this decision objective: {objective}. Required constraints and units: {constraints}. Integer or semi-continuous decisions: {integers}. Use solve_convex only when every variable is continuous and the declared LP, QP, QCQP, or SOCP structure is convex. Otherwise formulate a linear MILP and call solve_milp. Give every variable and constraint a stable id, combine duplicate terms, state finite bounds explicitly, keep units consistent, and use optimization://profile/balanced unless the deadline or quality target justifies another profile. Invoke the selected tool as a durable task and inspect independent bound, integrality, constraint, and objective verification before using the solution.",
                objective = required_value(arguments.objective, "objective")?,
                constraints = required_value(arguments.constraints, "constraints")?,
                integers = arguments
                    .integer_decisions
                    .as_deref()
                    .unwrap_or("none; all decisions are continuous"),
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
