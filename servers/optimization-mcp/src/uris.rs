use veoveo_mcp_contract::{ArtifactId, ServerResourceUris};

use crate::domain::{ProblemId, RunId, SolutionId, SolverProfileId};

pub const CAPABILITIES_URI: &str = "optimization://capabilities";
pub const ROUTES_APP_URI: &str = "ui://optimization/routes.html";
pub const MODELS_APP_URI: &str = "ui://optimization/models.html";
pub const PROFILES_URI: &str = "optimization://profiles";
pub const PROFILE_TEMPLATE: &str = "optimization://profile/{profile_id}";
pub const PROBLEMS_URI: &str = "optimization://problems";
pub const PROBLEMS_PAGE_TEMPLATE: &str = "optimization://problems{?cursor}";
pub const PROBLEM_TEMPLATE: &str = "optimization://problem/{problem_id}";
pub const RUNS_URI: &str = "optimization://runs";
pub const RUNS_PAGE_TEMPLATE: &str = "optimization://runs{?cursor}";
pub const RUN_TEMPLATE: &str = "optimization://run/{run_id}";
pub const RUN_INCUMBENTS_TEMPLATE: &str = "optimization://run/{run_id}/incumbents";
pub const SOLUTIONS_URI: &str = "optimization://solutions";
pub const SOLUTIONS_PAGE_TEMPLATE: &str = "optimization://solutions{?cursor}";
pub const SOLUTION_TEMPLATE: &str = "optimization://solution/{solution_id}";
pub const SOLUTION_ROUTES_TEMPLATE: &str = "optimization://solution/{solution_id}/routes";
pub const SOLUTION_VARIABLES_TEMPLATE: &str = "optimization://solution/{solution_id}/variables";
pub const SOLUTION_VERIFICATION_TEMPLATE: &str =
    "optimization://solution/{solution_id}/verification";
pub const ARTIFACT_TEMPLATE: &str = "optimization://artifact/{artifact_id}";
pub const USAGE_URI: &str = "optimization://usage";
pub const USAGE_PAGE_TEMPLATE: &str = "optimization://usage{?cursor}";
pub const USAGE_TASK_TEMPLATE: &str = "optimization://usage/task/{task_id}";
pub const DOCS_URI: &str = "optimization://docs";
pub const DOC_TEMPLATE: &str = "optimization://docs/{doc_id}";
pub const CONTRACT_URI: &str = "optimization://contract";

fn optimization_uris() -> ServerResourceUris {
    ServerResourceUris::new("optimization")
}

pub fn artifact_uri(artifact_id: ArtifactId) -> String {
    optimization_uris().artifact_uri(artifact_id)
}

pub fn profile_uri(profile_id: &SolverProfileId) -> String {
    format!("optimization://profile/{profile_id}")
}

pub fn problem_uri(problem_id: &ProblemId) -> String {
    format!("optimization://problem/{problem_id}")
}

pub fn run_uri(run_id: &RunId) -> String {
    format!("optimization://run/{run_id}")
}

pub fn run_incumbents_uri(run_id: &RunId) -> String {
    format!("{}/incumbents", run_uri(run_id))
}

pub fn solution_uri(solution_id: &SolutionId) -> String {
    format!("optimization://solution/{solution_id}")
}

pub fn solution_routes_uri(solution_id: &SolutionId) -> String {
    format!("{}/routes", solution_uri(solution_id))
}

pub fn solution_variables_uri(solution_id: &SolutionId) -> String {
    format!("{}/variables", solution_uri(solution_id))
}

pub fn solution_verification_uri(solution_id: &SolutionId) -> String {
    format!("{}/verification", solution_uri(solution_id))
}

pub fn usage_task_uri(task_id: &str) -> String {
    optimization_uris().usage_task_uri(task_id)
}

pub fn parse_profile_uri(uri: &str) -> Option<SolverProfileId> {
    parse_id(uri, "optimization://profile/", SolverProfileId::new)
}

pub fn parse_problem_uri(uri: &str) -> Option<ProblemId> {
    parse_id(uri, "optimization://problem/", ProblemId::parse)
}

pub fn parse_run_uri(uri: &str) -> Option<RunId> {
    parse_id(uri, "optimization://run/", RunId::parse)
}

pub fn parse_run_incumbents_uri(uri: &str) -> Option<RunId> {
    let value = uri
        .strip_prefix("optimization://run/")?
        .strip_suffix("/incumbents")?;
    valid_segment(value)
        .then(|| RunId::parse(value))
        .and_then(Result::ok)
}

pub fn parse_solution_uri(uri: &str) -> Option<SolutionId> {
    parse_id(uri, "optimization://solution/", SolutionId::parse)
}

pub fn parse_solution_routes_uri(uri: &str) -> Option<SolutionId> {
    parse_solution_child(uri, "routes")
}

pub fn parse_solution_variables_uri(uri: &str) -> Option<SolutionId> {
    parse_solution_child(uri, "variables")
}

pub fn parse_solution_verification_uri(uri: &str) -> Option<SolutionId> {
    parse_solution_child(uri, "verification")
}

pub fn parse_artifact_uri(uri: &str) -> Option<ArtifactId> {
    optimization_uris().parse_artifact_uri(uri)
}

pub fn parse_usage_task_uri(uri: &str) -> Option<&str> {
    optimization_uris().parse_usage_task_uri(uri)
}

pub fn parse_doc_uri(uri: &str) -> Option<&str> {
    uri.strip_prefix("optimization://docs/")
        .filter(|value| valid_segment(value))
}

fn parse_solution_child(uri: &str, child: &str) -> Option<SolutionId> {
    let value = uri
        .strip_prefix("optimization://solution/")?
        .strip_suffix(&format!("/{child}"))?;
    valid_segment(value)
        .then(|| SolutionId::parse(value))
        .and_then(Result::ok)
}

fn parse_id<T, E>(
    uri: &str,
    prefix: &str,
    parser: impl FnOnce(String) -> Result<T, E>,
) -> Option<T> {
    let value = uri.strip_prefix(prefix)?;
    valid_segment(value)
        .then(|| parser(value.to_owned()))
        .and_then(Result::ok)
}

fn valid_segment(value: &str) -> bool {
    !value.is_empty() && !value.contains('/')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_known_uris_match_the_shared_contract_conventions() {
        let conventions = optimization_uris();
        assert_eq!(DOCS_URI, conventions.docs_root_uri());
        assert_eq!(CONTRACT_URI, conventions.contract_uri());
        assert_eq!(DOC_TEMPLATE, conventions.doc_template());
        assert_eq!(parse_doc_uri("optimization://docs/agents"), Some("agents"));
        assert_eq!(parse_doc_uri("optimization://docs"), None);
        assert_eq!(parse_doc_uri("optimization://docs/agents/extra"), None);
    }

    #[test]
    fn problem_run_and_solution_uris_are_disjoint() {
        let problem = ProblemId::new();
        let run = RunId::new();
        let solution = SolutionId::new();
        assert_eq!(parse_problem_uri(&problem_uri(&problem)), Some(problem));
        assert_eq!(parse_run_uri(&run_uri(&run)), Some(run));
        assert_eq!(
            parse_solution_uri(&solution_uri(&solution)),
            Some(solution.clone())
        );
        assert!(parse_solution_uri(&solution_routes_uri(&solution)).is_none());
    }

    #[test]
    fn artifact_and_usage_uris_round_trip() {
        let artifact_id = ArtifactId::new();
        let artifact = artifact_uri(artifact_id);
        assert_eq!(parse_artifact_uri(&artifact), Some(artifact_id));
        let usage = usage_task_uri("task-1");
        assert_eq!(parse_usage_task_uri(&usage), Some("task-1"));
    }
}
