use std::collections::BTreeSet;
use veoveo_map_mcp::contract::MapTravelModelUri;

use chrono::Utc;
use veoveo_mcp_contract::{GatewayInternalIdentity, PlaneCaller};
use veoveo_optimization_mcp::{
    compiler::{
        compile_convex_problem, compile_milp_problem, compile_routing_initial_solution,
        compile_routing_problem,
    },
    contract::{
        ArtifactModelFormat, ConvexProblem, ConvexProblemSource, MilpProblem, MilpProblemSource,
        OptimizationAuthority, OptimizationProblemDefinition, OptimizationProblemResource,
        OptimizationProblemUri, OptimizationSolution, OptimizationSolutionUri,
        OptimizeRouteScenariosRequest, OptimizeRoutesRequest, ProblemDimensions, ProblemFamily,
        ProblemId, RouteScenario, RoutingProblem, RoutingProblemSource, SolveConvexRequest,
        SolveMilpRequest, TravelModelSource, decode_map_travel_model,
    },
    problem_store::{PreparedProblem, PreparedRouteCase},
};

use super::{app_state::AppState, ownership::runtime_owner};

pub(super) async fn prepare_routes(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    caller: &PlaneCaller,
    input: &OptimizeRoutesRequest,
) -> anyhow::Result<PreparedProblem> {
    let problem = materialize_routing_source(state, identity, caller, &input.problem).await?;
    let mut compiled = compile_routing_problem(&problem)?;
    if let Some(solution_uri) = &input.initial_solution {
        let solution = load_solution(state, identity, caller, solution_uri).await?;
        compiled.initial_solution = Some(compile_routing_initial_solution(&compiled, &solution)?);
    }
    let definition = OptimizationProblemDefinition::Routing {
        problem: problem.clone(),
    };
    let resource = problem_resource(
        identity,
        ProblemFamily::Routing,
        problem.version.clone(),
        definition,
        routing_dimensions(&compiled),
    )?;
    Ok(PreparedProblem::Routing {
        resource,
        problem,
        compiled,
    })
}

pub(super) async fn prepare_route_scenarios(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    caller: &PlaneCaller,
    input: &OptimizeRouteScenariosRequest,
) -> anyhow::Result<PreparedProblem> {
    input.validate()?;
    let mut cases = Vec::with_capacity(input.cases.len());
    let mut public_cases = Vec::with_capacity(input.cases.len());
    let mut dimensions = ProblemDimensions::default();
    for case in &input.cases {
        let problem = materialize_routing_source(state, identity, caller, &case.problem).await?;
        let mut compiled = compile_routing_problem(&problem)?;
        if let Some(solution_uri) = &case.initial_solution {
            let solution = load_solution(state, identity, caller, solution_uri).await?;
            compiled.initial_solution =
                Some(compile_routing_initial_solution(&compiled, &solution)?);
        }
        add_dimensions(&mut dimensions, &routing_dimensions(&compiled));
        cases.push(PreparedRouteCase {
            case_id: case.case_id.clone(),
            problem: problem.clone(),
            compiled,
        });
        public_cases.push(RouteScenario {
            case_id: case.case_id.clone(),
            problem: RoutingProblemSource::Inline { problem },
            initial_solution: case.initial_solution.clone(),
        });
    }
    let definition = OptimizationProblemDefinition::RouteScenarios {
        cases: public_cases,
    };
    let resource = problem_resource(
        identity,
        ProblemFamily::RouteScenarios,
        veoveo_optimization_mcp::contract::ROUTING_PROBLEM_VERSION.to_owned(),
        definition,
        dimensions,
    )?;
    Ok(PreparedProblem::RouteScenarios { resource, cases })
}

pub(super) async fn prepare_convex(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    caller: &PlaneCaller,
    input: &SolveConvexRequest,
) -> anyhow::Result<PreparedProblem> {
    let problem = materialize_convex_source(state, identity, caller, &input.problem).await?;
    let compiled = compile_convex_problem(&problem)?;
    let definition = OptimizationProblemDefinition::Convex {
        problem: problem.clone(),
    };
    let resource = problem_resource(
        identity,
        ProblemFamily::Convex,
        problem.version.clone(),
        definition,
        mathematical_dimensions(&compiled),
    )?;
    Ok(PreparedProblem::Convex {
        resource,
        problem,
        compiled,
    })
}

pub(super) async fn prepare_milp(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    caller: &PlaneCaller,
    input: &SolveMilpRequest,
) -> anyhow::Result<PreparedProblem> {
    let mut problem = veoveo_optimization_mcp::contract::MilpProblemValue::from(
        materialize_milp_source(state, identity, caller, &input.problem).await?,
    );
    if let Some(solution_uri) = &input.initial_solution {
        let solution = load_solution(state, identity, caller, solution_uri).await?;
        let values = solution_variable_map(&solution)?;
        problem.mip_start = Some(
            problem
                .variables
                .iter()
                .map(|variable| {
                    values.get(&variable.variable_id).copied().ok_or_else(|| {
                        anyhow::anyhow!("initial solution omits variable {}", variable.variable_id)
                    })
                })
                .collect::<anyhow::Result<Vec<_>>>()?,
        );
    }
    let problem = problem.build()?;
    let compiled = compile_milp_problem(&problem)?;
    let definition = OptimizationProblemDefinition::Milp {
        problem: problem.clone(),
    };
    let resource = problem_resource(
        identity,
        ProblemFamily::Milp,
        problem.version.clone(),
        definition,
        mathematical_dimensions(&compiled),
    )?;
    Ok(PreparedProblem::Milp {
        resource,
        problem,
        compiled,
    })
}

pub(super) async fn load_solution(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    caller: &PlaneCaller,
    solution_uri: &OptimizationSolutionUri,
) -> anyhow::Result<OptimizationSolution> {
    let task = veoveo_optimization_mcp::reads::OptimizationReads::new(&state.tasks)?
        .solution(&runtime_owner(identity), solution_uri)
        .await?
        .ok_or_else(|| anyhow::anyhow!("unknown or unauthorized solution {solution_uri}"))?;
    let output = task
        .output
        .ok_or_else(|| anyhow::anyhow!("solution task has no terminal output"))?;
    anyhow::ensure!(
        &output.result_uri == solution_uri,
        "selected output disagrees with requested solution"
    );
    let artifact_id = match output.solution_artifact.artifact_uri.address() {
        veoveo_artifact_contract::ArtifactAddress::Presented {
            scheme,
            artifact_id,
        } if scheme == &*veoveo_optimization_mcp::contract::uris::SCHEME => *artifact_id,
        _ => anyhow::bail!("solution artifact must use its Optimization presentation"),
    };
    let artifact = state
        .artifacts
        .get(caller, &artifact_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("solution artifact is unavailable"))?;
    veoveo_optimization_mcp::solution_builder::admit_solution_bytes(
        &artifact.bytes,
        solution_uri,
        output.run_uri.id(),
        &output.problem_uri,
        output.family,
    )
}

pub(super) async fn load_prepared_problem_by_uri(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    problem_uri: &OptimizationProblemUri,
) -> anyhow::Result<PreparedProblem> {
    let task = veoveo_optimization_mcp::reads::OptimizationReads::new(&state.tasks)?
        .problem(&runtime_owner(identity), problem_uri.id())
        .await?
        .ok_or_else(|| anyhow::anyhow!("unknown or unauthorized problem {problem_uri}"))?;
    let common = task
        .request
        .common()
        .ok_or_else(|| anyhow::anyhow!("problem task is not a solve task"))?;
    anyhow::ensure!(
        &common.problem_id == problem_uri.id(),
        "selected Task disagrees with requested problem"
    );
    state
        .problem_store
        .load_selected(&common.prepared, &common.problem_id, common.family)
        .await
}

async fn materialize_routing_source(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    caller: &PlaneCaller,
    source: &RoutingProblemSource,
) -> anyhow::Result<RoutingProblem> {
    let problem = match source {
        RoutingProblemSource::Inline { problem } => problem.clone(),
        RoutingProblemSource::Resource { uri } => {
            let prepared = load_prepared_problem_by_uri(state, identity, uri).await?;
            let PreparedProblem::Routing { problem, .. } = prepared else {
                anyhow::bail!("resource {} is not a routing problem", uri);
            };
            problem
        }
        RoutingProblemSource::Artifact { manifest_uri } => {
            read_json_artifact(state, caller, manifest_uri).await?
        }
    };
    let mut problem = veoveo_optimization_mcp::contract::RoutingProblemValue::from(problem);
    materialize_travel_model(state, caller, &mut problem).await?;
    problem.validate()?;
    Ok(problem.build()?)
}

async fn materialize_travel_model(
    state: &AppState,
    caller: &PlaneCaller,
    problem: &mut veoveo_optimization_mcp::contract::RoutingProblemValue,
) -> anyhow::Result<()> {
    let (artifact_uri, expected_map_uri): (
        &veoveo_artifact_contract::ArtifactUri,
        Option<&MapTravelModelUri>,
    ) = match &problem.travel_model {
        TravelModelSource::Inline { .. } => return Ok(()),
        TravelModelSource::Artifact { manifest_uri } => (manifest_uri, None),
        TravelModelSource::MapResource { uri, manifest_uri } => (manifest_uri, Some(uri)),
    };
    let bytes = read_artifact_bytes(state, caller, artifact_uri).await?;
    problem.travel_model = TravelModelSource::Inline {
        model: decode_map_travel_model(&bytes, expected_map_uri)?,
    };
    Ok(())
}

async fn materialize_convex_source(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    caller: &PlaneCaller,
    source: &ConvexProblemSource,
) -> anyhow::Result<ConvexProblem> {
    let problem = match source {
        ConvexProblemSource::Inline { problem } => problem.clone(),
        ConvexProblemSource::Resource { uri } => {
            let prepared = load_prepared_problem_by_uri(state, identity, uri).await?;
            let PreparedProblem::Convex { problem, .. } = prepared else {
                anyhow::bail!("resource {} is not a convex problem", uri);
            };
            problem
        }
        ConvexProblemSource::Artifact { model } => {
            if model.format != ArtifactModelFormat::OptimizationJsonV2 {
                anyhow::bail!("unsupported convex artifact format");
            }
            read_json_artifact(state, caller, &model.uri).await?
        }
    };
    problem.validate()?;
    Ok(problem)
}

async fn materialize_milp_source(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    caller: &PlaneCaller,
    source: &MilpProblemSource,
) -> anyhow::Result<MilpProblem> {
    let problem = match source {
        MilpProblemSource::Inline { problem } => problem.clone(),
        MilpProblemSource::Resource { uri } => {
            let prepared = load_prepared_problem_by_uri(state, identity, uri).await?;
            let PreparedProblem::Milp { problem, .. } = prepared else {
                anyhow::bail!("resource {} is not a MILP problem", uri);
            };
            problem
        }
        MilpProblemSource::Artifact { model } => {
            if model.format != ArtifactModelFormat::OptimizationJsonV2 {
                anyhow::bail!("unsupported MILP artifact format");
            }
            read_json_artifact(state, caller, &model.uri).await?
        }
    };
    problem.validate()?;
    Ok(problem)
}

async fn read_json_artifact<T: serde::de::DeserializeOwned>(
    state: &AppState,
    caller: &PlaneCaller,
    uri: &veoveo_artifact_contract::ArtifactUri,
) -> anyhow::Result<T> {
    Ok(serde_json::from_slice(
        &read_artifact_bytes(state, caller, uri).await?,
    )?)
}

async fn read_artifact_bytes(
    state: &AppState,
    caller: &PlaneCaller,
    uri: &veoveo_artifact_contract::ArtifactUri,
) -> anyhow::Result<Vec<u8>> {
    let artifact = state.artifacts.resolve(caller, uri).await?;
    if artifact.bytes.len() as u64 > state.max_artifact_bytes {
        anyhow::bail!(
            "input artifact is {} bytes and exceeds the {}-byte limit",
            artifact.bytes.len(),
            state.max_artifact_bytes
        );
    }
    Ok(artifact.bytes)
}

fn problem_resource(
    identity: &GatewayInternalIdentity,
    family: ProblemFamily,
    schema_version: String,
    definition: OptimizationProblemDefinition,
    dimensions: ProblemDimensions,
) -> anyhow::Result<OptimizationProblemResource> {
    let problem_id = ProblemId::new();
    let problem_uri = OptimizationProblemUri::new(problem_id.clone())?;
    let digest_sha256 = veoveo_optimization_mcp::contract::definition_digest(&definition)?;
    let created_at = Utc::now();
    Ok(
        veoveo_optimization_mcp::contract::OptimizationProblemResourceValue {
            record: veoveo_optimization_mcp::contract::OptimizationProblemRecordValue {
                problem_id,
                problem_uri,
                family,
                schema_version,
                digest_sha256,
                dimensions,
                authority: OptimizationAuthority {
                    principal_id: identity.actor.id.clone(),
                    work_context: Some(identity.authority.work_context.clone()),
                    policy_revision: identity.authority.policy_revision.clone(),
                    submitted_at: created_at,
                },
                created_at,
            }
            .build()?,
            definition,
        }
        .build()?,
    )
}

fn routing_dimensions(
    compiled: &veoveo_optimization_mcp::executor::CompiledRoutingProblem,
) -> ProblemDimensions {
    ProblemDimensions {
        locations: Some(compiled.location_ids.len() as u64),
        orders: Some(
            compiled
                .nodes
                .iter()
                .map(|node| &node.order_id)
                .collect::<BTreeSet<_>>()
                .len() as u64,
        ),
        vehicles: Some(compiled.vehicles.len() as u64),
        ..Default::default()
    }
}

fn mathematical_dimensions(
    compiled: &veoveo_optimization_mcp::executor::CompiledMathematicalModel,
) -> ProblemDimensions {
    ProblemDimensions {
        variables: Some(compiled.variable_ids.len() as u64),
        constraints: Some(compiled.constraint_ids.len() as u64),
        nonzeros: Some(
            compiled.constraint_matrix.values.len() as u64
                + compiled
                    .quadratic_objective
                    .as_ref()
                    .map_or(0, |matrix| matrix.values.len() as u64)
                + compiled
                    .quadratic_constraints
                    .iter()
                    .map(|constraint| {
                        (constraint.linear_values.len() + constraint.values.len()) as u64
                    })
                    .sum::<u64>(),
        ),
        ..Default::default()
    }
}

fn add_dimensions(total: &mut ProblemDimensions, next: &ProblemDimensions) {
    total.locations = sum(total.locations, next.locations);
    total.orders = sum(total.orders, next.orders);
    total.vehicles = sum(total.vehicles, next.vehicles);
    total.variables = sum(total.variables, next.variables);
    total.constraints = sum(total.constraints, next.constraints);
    total.nonzeros = sum(total.nonzeros, next.nonzeros);
}

fn sum(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.saturating_add(right)),
        (left, right) => left.or(right),
    }
}

fn solution_variable_map(
    solution: &OptimizationSolution,
) -> anyhow::Result<
    std::collections::BTreeMap<
        veoveo_optimization_mcp::contract::VariableId,
        veoveo_optimization_mcp::contract::FiniteF64,
    >,
> {
    let variables = match &solution.detail {
        veoveo_optimization_mcp::contract::SolutionDetail::Convex { variables, .. }
        | veoveo_optimization_mcp::contract::SolutionDetail::Milp { variables, .. } => variables,
        veoveo_optimization_mcp::contract::SolutionDetail::Routing { .. } => {
            anyhow::bail!("routing solution cannot seed a MILP")
        }
    };
    Ok(variables
        .iter()
        .map(|value| (value.variable_id.clone(), value.value))
        .collect())
}
