use std::collections::BTreeSet;

use chrono::Utc;
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::{GatewayInternalIdentity, PlaneCaller};
use veoveo_optimization_mcp::{
    compiler::{
        compile_convex_problem, compile_milp_problem, compile_routing_initial_solution,
        compile_routing_problem,
    },
    domain::{
        ArtifactModelFormat, ConvexProblem, ConvexProblemSource, MapTravelModelUri, MilpProblem,
        MilpProblemSource, OptimizationAuthority, OptimizationProblemDefinition,
        OptimizationProblemRecord, OptimizationProblemResource, OptimizationProblemUri,
        OptimizationSolution, OptimizeRouteScenariosRequest, OptimizeRoutesRequest,
        ProblemDimensions, ProblemFamily, ProblemId, RouteScenario, RoutingProblem,
        RoutingProblemSource, SolveConvexRequest, SolveMilpRequest, TRAVEL_MODEL_ARTIFACT_VERSION,
        TravelModelArtifact, TravelModelSource,
    },
    problem_store::{PreparedProblem, PreparedRouteCase},
    solution_builder::verify_solution_digest,
    uris,
};

use super::{
    app_state::AppState,
    index::{find_problem_task, find_solution_task},
};

pub(super) async fn prepare_routes(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    caller: &PlaneCaller,
    input: &OptimizeRoutesRequest,
) -> anyhow::Result<PreparedProblem> {
    let problem = materialize_routing_source(state, identity, caller, &input.problem).await?;
    let mut compiled = compile_routing_problem(&problem)?;
    if let Some(solution_uri) = &input.initial_solution {
        let solution = load_solution(state, identity, caller, solution_uri.as_str()).await?;
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
            let solution = load_solution(state, identity, caller, solution_uri.as_str()).await?;
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
        veoveo_optimization_mcp::domain::ROUTING_PROBLEM_VERSION.to_owned(),
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
    let mut problem = materialize_milp_source(state, identity, caller, &input.problem).await?;
    if let Some(solution_uri) = &input.initial_solution {
        let solution = load_solution(state, identity, caller, solution_uri.as_str()).await?;
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
    solution_uri: &str,
) -> anyhow::Result<OptimizationSolution> {
    let solution_uri =
        veoveo_optimization_mcp::domain::OptimizationSolutionUri::parse(solution_uri.to_owned())?;
    let task = find_solution_task(state, identity, &solution_uri)
        .await?
        .ok_or_else(|| anyhow::anyhow!("unknown or unauthorized solution {solution_uri}"))?;
    let output = task
        .output
        .ok_or_else(|| anyhow::anyhow!("solution task has no terminal output"))?;
    let artifact_id = uris::parse_artifact_uri(&output.solution_artifact.artifact_uri)
        .ok_or_else(|| anyhow::anyhow!("solution artifact has an invalid URI"))?;
    let artifact = state
        .artifacts
        .get(caller, &artifact_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("solution artifact is unavailable"))?;
    let solution: OptimizationSolution = serde_json::from_slice(&artifact.bytes)?;
    if solution.solution_uri != solution_uri {
        anyhow::bail!("solution artifact identity does not match its resource");
    }
    verify_solution_digest(&solution)?;
    Ok(solution)
}

pub(super) async fn load_prepared_problem_by_uri(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    problem_uri: &str,
) -> anyhow::Result<PreparedProblem> {
    let problem_id = uris::parse_problem_uri(problem_uri)
        .ok_or_else(|| anyhow::anyhow!("invalid Optimization problem URI"))?;
    let task = find_problem_task(state, identity, &problem_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("unknown or unauthorized problem {problem_uri}"))?;
    let common = task
        .request
        .common()
        .ok_or_else(|| anyhow::anyhow!("problem task is not a solve task"))?;
    state.problem_store.load(&common.prepared).await
}

async fn materialize_routing_source(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    caller: &PlaneCaller,
    source: &RoutingProblemSource,
) -> anyhow::Result<RoutingProblem> {
    let mut problem = match source {
        RoutingProblemSource::Inline { problem } => problem.clone(),
        RoutingProblemSource::Resource { uri } => {
            let prepared = load_prepared_problem_by_uri(state, identity, uri.as_str()).await?;
            let PreparedProblem::Routing { problem, .. } = prepared else {
                anyhow::bail!("resource {} is not a routing problem", uri);
            };
            problem
        }
        RoutingProblemSource::Artifact { manifest_uri } => {
            read_json_artifact(state, caller, manifest_uri.as_str()).await?
        }
    };
    materialize_travel_model(state, caller, &mut problem).await?;
    problem.validate()?;
    Ok(problem)
}

async fn materialize_travel_model(
    state: &AppState,
    caller: &PlaneCaller,
    problem: &mut RoutingProblem,
) -> anyhow::Result<()> {
    let (artifact_uri, expected_map_uri): (&str, Option<&MapTravelModelUri>) = match &problem
        .travel_model
    {
        TravelModelSource::Inline { .. } => return Ok(()),
        TravelModelSource::Artifact { manifest_uri } => (manifest_uri.as_str(), None),
        TravelModelSource::MapResource { uri, manifest_uri } => (manifest_uri.as_str(), Some(uri)),
    };
    let artifact: TravelModelArtifact = read_json_artifact(state, caller, artifact_uri).await?;
    if artifact.version != TRAVEL_MODEL_ARTIFACT_VERSION {
        anyhow::bail!(
            "travel-model artifact version must be {}",
            TRAVEL_MODEL_ARTIFACT_VERSION
        );
    }
    if let Some(expected) = expected_map_uri
        && artifact.map_resource_uri.as_ref() != Some(expected)
    {
        anyhow::bail!("travel-model artifact does not attest the requested Map resource");
    }
    problem.travel_model = TravelModelSource::Inline {
        model: artifact.model,
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
            let prepared = load_prepared_problem_by_uri(state, identity, uri.as_str()).await?;
            let PreparedProblem::Convex { problem, .. } = prepared else {
                anyhow::bail!("resource {} is not a convex problem", uri);
            };
            problem
        }
        ConvexProblemSource::Artifact { model } => {
            if model.format != ArtifactModelFormat::OptimizationJsonV1 {
                anyhow::bail!("unsupported convex artifact format");
            }
            read_json_artifact(state, caller, model.uri.as_str()).await?
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
            let prepared = load_prepared_problem_by_uri(state, identity, uri.as_str()).await?;
            let PreparedProblem::Milp { problem, .. } = prepared else {
                anyhow::bail!("resource {} is not a MILP problem", uri);
            };
            problem
        }
        MilpProblemSource::Artifact { model } => {
            if model.format != ArtifactModelFormat::OptimizationJsonV1 {
                anyhow::bail!("unsupported MILP artifact format");
            }
            read_json_artifact(state, caller, model.uri.as_str()).await?
        }
    };
    problem.validate()?;
    Ok(problem)
}

async fn read_json_artifact<T: serde::de::DeserializeOwned>(
    state: &AppState,
    caller: &PlaneCaller,
    uri: &str,
) -> anyhow::Result<T> {
    let artifact = state.artifacts.resolve(caller, uri).await?;
    if artifact.bytes.len() as u64 > state.max_artifact_bytes {
        anyhow::bail!(
            "input artifact is {} bytes and exceeds the {}-byte limit",
            artifact.bytes.len(),
            state.max_artifact_bytes
        );
    }
    Ok(serde_json::from_slice(&artifact.bytes)?)
}

fn problem_resource(
    identity: &GatewayInternalIdentity,
    family: ProblemFamily,
    schema_version: String,
    definition: OptimizationProblemDefinition,
    dimensions: ProblemDimensions,
) -> anyhow::Result<OptimizationProblemResource> {
    let problem_id = ProblemId::new();
    let problem_uri = OptimizationProblemUri::parse(uris::problem_uri(&problem_id))?;
    let digest_sha256 = hex::encode(Sha256::digest(serde_json::to_vec(&definition)?));
    let created_at = Utc::now();
    Ok(OptimizationProblemResource {
        record: OptimizationProblemRecord {
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
        },
        definition,
    })
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
        veoveo_optimization_mcp::domain::VariableId,
        veoveo_optimization_mcp::domain::FiniteF64,
    >,
> {
    let variables = match &solution.detail {
        veoveo_optimization_mcp::domain::SolutionDetail::Convex { variables, .. }
        | veoveo_optimization_mcp::domain::SolutionDetail::Milp { variables, .. } => variables,
        veoveo_optimization_mcp::domain::SolutionDetail::Routing { .. } => {
            anyhow::bail!("routing solution cannot seed a MILP")
        }
    };
    Ok(variables
        .iter()
        .map(|value| (value.variable_id.clone(), value.value))
        .collect())
}
