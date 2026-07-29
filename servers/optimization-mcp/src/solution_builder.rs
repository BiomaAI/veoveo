use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use crate::{
    domain::{
        ConvexProblem, EngineProvenance, IncumbentSummary, MathematicalQuality, MilpProblem,
        NonNegativeF64, OptimizationAuthority, OptimizationProblemUri, OptimizationSolution,
        OptimizationSolutionUri, ProblemFamily, RouteCaseId, RouteSolutionSummary, RunId,
        RunTimings, SolutionDetail, SolutionFeasibility, SolutionId, SolverTermination,
        UnitInterval, VariableValue, VerificationCode, VerificationFinding, VerificationReport,
        VerificationSeverity,
    },
    executor::{
        CompiledRoutingProblem, ExecutorMathematicalSolution, ExecutorModelStatus,
        ExecutorRouteCaseSolution, ExecutorRoutingSolution, ExecutorRoutingStatus,
    },
    verification::{
        VerificationTolerance, verify_convex_candidate, verify_milp_candidate,
        verify_routing_solution,
    },
};

#[derive(Debug, Clone)]
pub struct SolutionContext {
    pub run_id: RunId,
    pub problem_uri: OptimizationProblemUri,
    pub engine: EngineProvenance,
    pub timings: RunTimings,
    pub authority: OptimizationAuthority,
    pub created_at: DateTime<Utc>,
}

pub fn build_routing_solution(
    problem: &CompiledRoutingProblem,
    executor: &ExecutorRoutingSolution,
    context: SolutionContext,
) -> anyhow::Result<OptimizationSolution> {
    build_route_cases(
        vec![(None, problem, executor)],
        ProblemFamily::Routing,
        context,
    )
}

pub fn build_route_scenario_solution(
    cases: &[(&RouteCaseId, &CompiledRoutingProblem)],
    executor: &[ExecutorRouteCaseSolution],
    context: SolutionContext,
) -> anyhow::Result<OptimizationSolution> {
    let matched = cases
        .iter()
        .map(|(case_id, problem)| {
            let solution = executor
                .iter()
                .find(|solution| &solution.case_id == *case_id)
                .ok_or_else(|| anyhow::anyhow!("executor omitted route case {case_id}"))?;
            Ok((Some((*case_id).clone()), *problem, &solution.solution))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    build_route_cases(matched, ProblemFamily::RouteScenarios, context)
}

fn build_route_cases(
    cases: Vec<(
        Option<RouteCaseId>,
        &CompiledRoutingProblem,
        &ExecutorRoutingSolution,
    )>,
    family: ProblemFamily,
    mut context: SolutionContext,
) -> anyhow::Result<OptimizationSolution> {
    let mut summaries = Vec::with_capacity(cases.len());
    let mut routes = Vec::new();
    let mut reports = Vec::with_capacity(cases.len());
    let mut feasibility = SolutionFeasibility::Feasible;
    let mut termination = SolverTermination::Completed;
    let mut solve_seconds: f64 = 0.0;

    for (case_id, problem, executor) in cases {
        let mut verified =
            verify_routing_solution(problem, executor, VerificationTolerance::default());
        if !matches!(
            executor.status,
            ExecutorRoutingStatus::Success | ExecutorRoutingStatus::Timeout
        ) {
            verified.report.verified = false;
            verified.report.findings.push(VerificationFinding {
                code: VerificationCode::SolverReportedFailure,
                severity: VerificationSeverity::Error,
                message: format!(
                    "cuOpt routing terminated with {:?}: {}",
                    executor.status, executor.message
                ),
                variable_id: None,
                constraint_id: None,
                order_id: None,
                vehicle_id: None,
            });
        }
        for route in &mut verified.routes {
            route.case_id = case_id.clone();
        }
        let undeliverable_orders = executor
            .undeliverable_nodes
            .iter()
            .filter_map(|index| problem.nodes.get(*index as usize))
            .map(|node| node.order_id.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        summaries.push(RouteSolutionSummary {
            case_id: case_id.clone(),
            vehicles_used: executor.vehicles_used,
            orders_served: u32::try_from(verified.served_orders.len()).unwrap_or(u32::MAX),
            orders_dropped: u32::try_from(verified.dropped_orders.len()).unwrap_or(u32::MAX),
            undeliverable_orders,
            objective: executor.objective,
            objective_components: executor.objective_components.clone(),
        });
        routes.extend(verified.routes);
        solve_seconds = solve_seconds.max(executor.solve_seconds.get());
        if !verified.report.verified {
            feasibility = if executor.routes.is_empty() {
                SolutionFeasibility::NoSolution
            } else {
                SolutionFeasibility::Partial
            };
        }
        termination = combine_termination(termination, routing_termination(executor.status));
        reports.push(verified.report);
    }

    context.timings.solve_seconds =
        NonNegativeF64::new(solve_seconds).expect("executor solve time is non-negative");
    finish_solution(
        family,
        feasibility,
        termination,
        SolutionDetail::Routing { summaries, routes },
        merge_reports(reports),
        context,
    )
}

pub fn build_convex_solution(
    problem: &ConvexProblem,
    executor: &ExecutorMathematicalSolution,
    context: SolutionContext,
) -> anyhow::Result<OptimizationSolution> {
    let candidate = candidate_values(&problem.variables, executor);
    let verified = verify_convex_candidate(
        problem,
        &candidate,
        executor.primal_objective,
        VerificationTolerance::default(),
    );
    let quality = mathematical_quality(executor, &verified, executor.status);
    finish_solution(
        ProblemFamily::Convex,
        mathematical_feasibility(
            executor.status,
            verified.report.verified,
            !candidate.is_empty(),
        ),
        mathematical_termination(executor.status),
        SolutionDetail::Convex {
            quality,
            variables: verified.variables,
            constraints: verified.constraints,
        },
        verified.report,
        with_solve_time(context, executor.solve_seconds),
    )
}

pub fn build_milp_solution(
    problem: &MilpProblem,
    executor: &ExecutorMathematicalSolution,
    context: SolutionContext,
) -> anyhow::Result<OptimizationSolution> {
    let candidate = candidate_values(&problem.variables, executor);
    let verified = verify_milp_candidate(
        problem,
        &candidate,
        executor.primal_objective,
        VerificationTolerance::default(),
    );
    let quality = mathematical_quality(executor, &verified, executor.status);
    let incumbents = executor
        .incumbents
        .iter()
        .map(|incumbent| {
            let denominator = incumbent.objective.get().abs().max(1.0);
            let gap =
                ((incumbent.objective.get() - incumbent.bound.get()).abs() / denominator).min(1.0);
            IncumbentSummary {
                sequence: incumbent.sequence,
                objective: incumbent.objective,
                best_bound: Some(incumbent.bound),
                relative_gap: UnitInterval::new(gap).ok(),
                found_at_seconds: incumbent.found_at_seconds,
            }
        })
        .collect();
    finish_solution(
        ProblemFamily::Milp,
        mathematical_feasibility(
            executor.status,
            verified.report.verified,
            !candidate.is_empty(),
        ),
        mathematical_termination(executor.status),
        SolutionDetail::Milp {
            quality,
            variables: verified.variables,
            constraints: verified.constraints,
            incumbents,
        },
        verified.report,
        with_solve_time(context, executor.solve_seconds),
    )
}

fn candidate_values(
    variables: &[crate::domain::ModelVariable],
    executor: &ExecutorMathematicalSolution,
) -> Vec<VariableValue> {
    variables
        .iter()
        .zip(&executor.primal_solution)
        .map(|(variable, value)| VariableValue {
            variable_id: variable.variable_id.clone(),
            value: *value,
        })
        .collect()
}

fn mathematical_quality(
    executor: &ExecutorMathematicalSolution,
    verified: &crate::verification::CandidateVerification,
    status: ExecutorModelStatus,
) -> MathematicalQuality {
    let absolute_gap = executor
        .primal_objective
        .zip(executor.best_bound.or(executor.dual_objective))
        .and_then(|(primal, bound)| NonNegativeF64::new((primal.get() - bound.get()).abs()).ok());
    MathematicalQuality {
        proven_optimal: status == ExecutorModelStatus::Optimal && verified.report.verified,
        primal_objective: verified.objective.or(executor.primal_objective),
        dual_objective: executor.dual_objective,
        best_bound: executor.best_bound,
        absolute_gap,
        relative_gap: executor.relative_gap,
        primal_residual: executor.primal_residual,
        dual_residual: executor.dual_residual,
        maximum_constraint_violation: Some(verified.maximum_constraint_violation),
        maximum_integrality_violation: Some(verified.maximum_integrality_violation),
        maximum_bound_violation: Some(verified.maximum_bound_violation),
        iterations: executor.iterations,
        nodes: executor.nodes,
    }
}

fn mathematical_feasibility(
    status: ExecutorModelStatus,
    verified: bool,
    has_candidate: bool,
) -> SolutionFeasibility {
    match status {
        ExecutorModelStatus::Infeasible => SolutionFeasibility::Infeasible,
        ExecutorModelStatus::Unbounded | ExecutorModelStatus::InfeasibleOrUnbounded => {
            SolutionFeasibility::Unbounded
        }
        _ if verified && has_candidate => SolutionFeasibility::Feasible,
        _ if has_candidate => SolutionFeasibility::Partial,
        _ => SolutionFeasibility::NoSolution,
    }
}

fn routing_termination(status: ExecutorRoutingStatus) -> SolverTermination {
    match status {
        ExecutorRoutingStatus::Success => SolverTermination::Completed,
        ExecutorRoutingStatus::Timeout => SolverTermination::TimeLimit,
        ExecutorRoutingStatus::Infeasible => SolverTermination::Infeasible,
        ExecutorRoutingStatus::Failed | ExecutorRoutingStatus::Empty => SolverTermination::Failed,
    }
}

fn mathematical_termination(status: ExecutorModelStatus) -> SolverTermination {
    match status {
        ExecutorModelStatus::Optimal => SolverTermination::Optimal,
        ExecutorModelStatus::Feasible => SolverTermination::Completed,
        ExecutorModelStatus::Infeasible => SolverTermination::Infeasible,
        ExecutorModelStatus::Unbounded | ExecutorModelStatus::InfeasibleOrUnbounded => {
            SolverTermination::Unbounded
        }
        ExecutorModelStatus::TimeLimit => SolverTermination::TimeLimit,
        ExecutorModelStatus::IterationLimit => SolverTermination::IterationLimit,
        ExecutorModelStatus::NodeLimit => SolverTermination::NodeLimit,
        ExecutorModelStatus::NumericalFailure => SolverTermination::NumericalFailure,
        ExecutorModelStatus::Cancelled => SolverTermination::Cancelled,
        ExecutorModelStatus::Failed => SolverTermination::Failed,
    }
}

fn combine_termination(current: SolverTermination, next: SolverTermination) -> SolverTermination {
    use SolverTermination::*;
    match (current, next) {
        (Failed | NumericalFailure, _) | (_, Failed | NumericalFailure) => Failed,
        (Infeasible, _) | (_, Infeasible) => Infeasible,
        (TimeLimit, _) | (_, TimeLimit) => TimeLimit,
        (left, Completed) => left,
        (Completed, right) => right,
        (left, _) => left,
    }
}

fn merge_reports(reports: Vec<VerificationReport>) -> VerificationReport {
    let mut reports = reports.into_iter();
    let Some(mut merged) = reports.next() else {
        return crate::verification::empty_report(VerificationTolerance::default());
    };
    for report in reports {
        merged.verified &= report.verified;
        merged.findings.extend(report.findings);
        merged.maximum_constraint_violation = maximum(
            merged.maximum_constraint_violation,
            report.maximum_constraint_violation,
        );
        merged.maximum_integrality_violation = maximum(
            merged.maximum_integrality_violation,
            report.maximum_integrality_violation,
        );
        merged.maximum_bound_violation = maximum(
            merged.maximum_bound_violation,
            report.maximum_bound_violation,
        );
        if report.verified_at > merged.verified_at {
            merged.verified_at = report.verified_at;
        }
    }
    merged
}

fn maximum(left: Option<NonNegativeF64>, right: Option<NonNegativeF64>) -> Option<NonNegativeF64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(if left > right { left } else { right }),
        (left, right) => left.or(right),
    }
}

fn with_solve_time(mut context: SolutionContext, solve_seconds: NonNegativeF64) -> SolutionContext {
    context.timings.solve_seconds = solve_seconds;
    context
}

fn finish_solution(
    family: ProblemFamily,
    feasibility: SolutionFeasibility,
    termination: SolverTermination,
    detail: SolutionDetail,
    verification: VerificationReport,
    context: SolutionContext,
) -> anyhow::Result<OptimizationSolution> {
    let solution_id = SolutionId::new();
    let solution_uri = OptimizationSolutionUri::parse(crate::uris::solution_uri(&solution_id))?;
    let mut solution = OptimizationSolution {
        solution_id,
        solution_uri,
        run_id: context.run_id,
        problem_uri: context.problem_uri,
        feasibility,
        termination,
        detail,
        verification,
        engine: context.engine,
        timings: context.timings,
        digest_sha256: String::new(),
        authority: context.authority,
        created_at: context.created_at,
    };
    solution.digest_sha256 = calculate_solution_digest(&solution)?;
    debug_assert!(matches!(
        family,
        ProblemFamily::Routing
            | ProblemFamily::RouteScenarios
            | ProblemFamily::Convex
            | ProblemFamily::Milp
    ));
    Ok(solution)
}

pub fn calculate_solution_digest(solution: &OptimizationSolution) -> anyhow::Result<String> {
    let mut canonical = solution.clone();
    canonical.digest_sha256.clear();
    let bytes = serde_json::to_vec(&canonical)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

pub fn verify_solution_digest(solution: &OptimizationSolution) -> anyhow::Result<()> {
    let expected = calculate_solution_digest(solution)?;
    if solution.digest_sha256 != expected {
        anyhow::bail!("solution digest does not match its canonical contents");
    }
    Ok(())
}
