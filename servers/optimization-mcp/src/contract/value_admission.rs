//! Intrinsic relationships independent of executors, Store and live authority.
use super::*;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub(super) fn mismatch() -> OptimizationContractError {
    OptimizationContractError::InvalidProblem("Optimization value relationships disagree".into())
}
pub(super) fn artifact(
    value: &veoveo_artifact_contract::ArtifactMetadata,
) -> Result<(), OptimizationContractError> {
    if value.download_url.is_some()
        || !matches!(value.artifact_uri.address(), veoveo_artifact_contract::ArtifactAddress::Presented { scheme, .. } if scheme == &*super::uris::SCHEME)
    {
        return Err(mismatch());
    }
    Ok(())
}
pub(crate) fn solution_digest(
    value: &OptimizationSolutionValue,
) -> Result<veoveo_artifact_contract::UploadSha256, OptimizationContractError> {
    // Preserve the struct serializer's field order: replacement occurs at the
    // digest field, rather than sorting an intermediate JSON object.
    #[derive(serde::Serialize)]
    struct Preimage<'a> {
        solution_id: &'a SolutionId,
        solution_uri: &'a OptimizationSolutionUri,
        run_id: &'a RunId,
        problem_uri: &'a OptimizationProblemUri,
        feasibility: SolutionFeasibility,
        termination: SolverTermination,
        detail: &'a SolutionDetail,
        verification: &'a VerificationReport,
        engine: &'a EngineProvenance,
        timings: &'a RunTimings,
        digest_sha256: &'a str,
        authority: &'a OptimizationAuthority,
        created_at: chrono::DateTime<chrono::Utc>,
    }
    let preimage = Preimage {
        solution_id: &value.solution_id,
        solution_uri: &value.solution_uri,
        run_id: &value.run_id,
        problem_uri: &value.problem_uri,
        feasibility: value.feasibility,
        termination: value.termination,
        detail: &value.detail,
        verification: &value.verification,
        engine: &value.engine,
        timings: &value.timings,
        digest_sha256: "",
        authority: &value.authority,
        created_at: value.created_at,
    };
    let bytes = serde_json::to_vec(&preimage).map_err(|_| mismatch())?;
    veoveo_artifact_contract::UploadSha256::parse(hex::encode(Sha256::digest(bytes)))
        .map_err(|_| mismatch())
}
pub fn definition_digest(
    definition: &OptimizationProblemDefinition,
) -> Result<veoveo_artifact_contract::UploadSha256, OptimizationContractError> {
    let bytes = serde_json::to_vec(definition).map_err(|_| mismatch())?;
    veoveo_artifact_contract::UploadSha256::parse(hex::encode(Sha256::digest(bytes)))
        .map_err(|_| mismatch())
}
fn linear_count(terms: &[LinearTerm]) -> Result<u64, OptimizationContractError> {
    let mut sums = BTreeMap::<&VariableId, f64>::new();
    for term in terms {
        *sums.entry(&term.variable_id).or_default() += term.coefficient.get();
    }
    if sums.values().any(|value| !value.is_finite()) {
        return Err(mismatch());
    }
    Ok(sums.values().filter(|value| **value != 0.0).count() as u64)
}
fn quadratic_count(
    terms: &[QuadraticTerm],
    retain_zero: bool,
) -> Result<u64, OptimizationContractError> {
    let mut sums = BTreeMap::<(&VariableId, &VariableId), f64>::new();
    for term in terms {
        *sums
            .entry((&term.left_variable_id, &term.right_variable_id))
            .or_default() += term.coefficient.get();
    }
    if sums.values().any(|value| !value.is_finite()) {
        return Err(mismatch());
    }
    Ok(sums
        .values()
        .filter(|value| retain_zero || **value != 0.0)
        .count() as u64)
}
fn mathematical_dimensions(
    variables: usize,
    objective: &ModelObjective,
    linear: &[LinearConstraint],
    quadratic: &[QuadraticConstraint],
) -> Result<ProblemDimensions, OptimizationContractError> {
    linear_count(&objective.linear_terms)?;
    let mut nonzeros = quadratic_count(&objective.quadratic_terms, false)?;
    let mut constraints = linear.len() as u64;
    for row in linear {
        nonzeros += linear_count(&row.terms)?;
    }
    for row in quadratic {
        let sides = u64::from(row.bounds.lower.is_some()) + u64::from(row.bounds.upper.is_some());
        constraints += sides;
        nonzeros += sides
            * (linear_count(&row.linear_terms)? + quadratic_count(&row.quadratic_terms, true)?);
    }
    Ok(ProblemDimensions {
        variables: Some(variables as u64),
        constraints: Some(constraints),
        nonzeros: Some(nonzeros),
        ..Default::default()
    })
}
fn routing_dimensions(problem: &RoutingProblem) -> ProblemDimensions {
    ProblemDimensions {
        locations: Some(problem.locations.len() as u64),
        orders: Some(problem.orders.len() as u64),
        vehicles: Some(problem.fleet.vehicles.len() as u64),
        ..Default::default()
    }
}
impl OptimizationProblemDefinition {
    pub fn family(&self) -> ProblemFamily {
        match self {
            Self::Routing { .. } => ProblemFamily::Routing,
            Self::RouteScenarios { .. } => ProblemFamily::RouteScenarios,
            Self::Convex { .. } => ProblemFamily::Convex,
            Self::Milp { .. } => ProblemFamily::Milp,
        }
    }
    pub fn dimensions(&self) -> Result<ProblemDimensions, OptimizationContractError> {
        match self {
            Self::Routing { problem } => Ok(routing_dimensions(problem)),
            Self::Convex { problem } => mathematical_dimensions(
                problem.variables.len(),
                &problem.objective,
                &problem.linear_constraints,
                &problem.quadratic_constraints,
            ),
            Self::Milp { problem } => mathematical_dimensions(
                problem.variables.len(),
                &problem.objective,
                &problem.constraints,
                &[],
            ),
            Self::RouteScenarios { cases } => {
                require_collection("route cases", cases.len(), 2, MAX_ROUTE_CASES)?;
                let mut seen = std::collections::BTreeSet::new();
                let mut total = ProblemDimensions {
                    locations: Some(0),
                    orders: Some(0),
                    vehicles: Some(0),
                    ..Default::default()
                };
                for case in cases {
                    if !seen.insert(&case.case_id) {
                        return Err(mismatch());
                    }
                    let RoutingProblemSource::Inline { problem } = &case.problem else {
                        return Err(mismatch());
                    };
                    let next = routing_dimensions(problem);
                    *total.locations.as_mut().unwrap() += next.locations.unwrap();
                    *total.orders.as_mut().unwrap() += next.orders.unwrap();
                    *total.vehicles.as_mut().unwrap() += next.vehicles.unwrap();
                }
                Ok(total)
            }
        }
    }
}

pub(super) fn validate_mathematical_coefficients(
    objective: &ModelObjective,
    linear: &[LinearConstraint],
    quadratic: &[QuadraticConstraint],
) -> Result<(), OptimizationContractError> {
    mathematical_dimensions(0, objective, linear, quadratic).map(|_| ())
}
