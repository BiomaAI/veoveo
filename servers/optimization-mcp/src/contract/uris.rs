//! Fixed discovery declarations; dynamic addresses use the typed resource contract.
pub static SCHEME: std::sync::LazyLock<veoveo_types::ResourceScheme> =
    std::sync::LazyLock::new(|| {
        veoveo_types::ResourceScheme::parse("optimization")
            .expect("declared server resource scheme")
    });

pub const CAPABILITIES_URI: &str = "optimization://capabilities";
pub const ROUTES_APP_URI: &str = "ui://optimization/routes.html";
pub const MODELS_APP_URI: &str = "ui://optimization/models.html";
pub const PROFILES_URI: &str = "optimization://profiles";
pub const PROFILE_TEMPLATE: &str = crate::contract::OptimizationProfileUri::RESOURCE_TEMPLATE;
pub const PROBLEMS_URI: &str = "optimization://problems";
pub const PROBLEMS_PAGE_TEMPLATE: &str = "optimization://problems{?cursor}";
pub const PROBLEM_TEMPLATE: &str = crate::contract::OptimizationProblemUri::RESOURCE_TEMPLATE;
pub const RUNS_URI: &str = "optimization://runs";
pub const RUNS_PAGE_TEMPLATE: &str = "optimization://runs{?cursor}";
pub const RUN_TEMPLATE: &str = crate::contract::OptimizationRunUri::RESOURCE_TEMPLATE;
pub const RUN_INCUMBENTS_TEMPLATE: &str = "optimization://run/{run_id}/incumbents";
pub const SOLUTIONS_URI: &str = "optimization://solutions";
pub const SOLUTIONS_PAGE_TEMPLATE: &str = "optimization://solutions{?cursor}";
pub const SOLUTION_TEMPLATE: &str = crate::contract::OptimizationSolutionUri::RESOURCE_TEMPLATE;
pub const SOLUTION_ROUTES_TEMPLATE: &str = "optimization://solution/{solution_id}/routes";
pub const SOLUTION_VARIABLES_TEMPLATE: &str = "optimization://solution/{solution_id}/variables";
pub const SOLUTION_VERIFICATION_TEMPLATE: &str =
    "optimization://solution/{solution_id}/verification";
pub const ARTIFACT_TEMPLATE: &str = "optimization://artifact/{artifact_id}";
pub const DOCS_URI: &str = "optimization://docs";
pub const DOC_TEMPLATE: &str = "optimization://docs/{doc_id}";
pub const CONTRACT_URI: &str = "optimization://contract";
