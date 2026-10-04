//! Concrete domain addresses retain their parent ID after wire admission.

use super::{OptimizationContractError, ProblemId, RunId, SolutionId, SolverProfileId};

#[veoveo_types::resource_address(
    cached_checked(OptimizationContractErrorAddresses),
    template = "optimization://problem/{problem_id}"
)]
pub struct OptimizationProblemUri {
    #[resource(cache)]
    wire: String,
    #[resource(variable = "problem_id",  accessor = id)]
    id: ProblemId,
}
#[veoveo_types::resource_address(
    cached_checked(OptimizationContractErrorAddresses),
    template = "optimization://run/{run_id}"
)]
pub struct OptimizationRunUri {
    #[resource(cache)]
    wire: String,
    #[resource(variable = "run_id",  accessor = id)]
    id: RunId,
}
#[veoveo_types::resource_address(
    cached_checked(OptimizationContractErrorAddresses),
    template = "optimization://solution/{solution_id}"
)]
pub struct OptimizationSolutionUri {
    #[resource(cache)]
    wire: String,
    #[resource(variable = "solution_id",  accessor = id)]
    id: SolutionId,
}
#[veoveo_types::resource_address(
    cached_checked(OptimizationContractErrorAddresses),
    template = "optimization://profile/{profile_id}"
)]
pub struct OptimizationProfileUri {
    #[resource(cache)]
    wire: String,
    #[resource(variable = "profile_id",  accessor = id)]
    id: SolverProfileId,
}
/// Builders accept the corresponding parent identity.
/// ```compile_fail
/// use veoveo_optimization_mcp::contract::{OptimizationProblemUri, RunId};
/// OptimizationProblemUri::new(RunId::new());
/// ```
/// ```compile_fail
/// use veoveo_optimization_mcp::contract::OptimizationSolutionUri;
/// OptimizationSolutionUri::new("solution-id");
/// ```
const _: () = ();

#[doc(hidden)]
pub struct OptimizationContractErrorAddresses;
impl veoveo_types::ResourceProfile for OptimizationContractErrorAddresses {
    type Error = OptimizationContractError;
    const PROFILE: veoveo_types::ResourceProfileSpec<Self::Error> =
        veoveo_types::ResourceProfileSpec {
            route_error: |name, _| OptimizationContractError::InvalidUri(name),
        };
}
