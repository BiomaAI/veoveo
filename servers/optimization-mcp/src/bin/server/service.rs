use std::sync::Arc;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use rmcp::{
    ErrorData as McpError, RoleServer,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, CompleteRequestParams, CompleteResult, CompletionInfo,
        GetPromptRequestParams, GetPromptResult, Prompt, ReadResourceRequestParams,
        ReadResourceResult, Reference, ResourceContents, Tool,
    },
    service::RequestContext,
    tool_router,
};
use serde::Serialize;
use veoveo_mcp_contract::{
    ResourceListObservers, SubscriptionHub, UsageKind, UsageRecord, UsageReport,
    hosting::{
        DomainAddress, DomainRead, DomainServer, ResourceSubscriptions, gateway_identity,
        json_read, plane_caller, served_by_host, unknown_prompt,
    },
    server_contract::McpServerSetup,
};
use veoveo_optimization_mcp::{
    contract::uris,
    contract::{
        CUOPT_CONTAINER_DIGEST, CUOPT_STABLE_VERSION, EngineProvenance,
        OPTIMIZATION_INDEX_PAGE_SIZE, OptimizationAuthority, OptimizationCollection,
        OptimizationIndexCursor, OptimizationProblemUri, OptimizationResource,
        OptimizationRunRecord, OptimizationRunUri, OptimizationSolution, OptimizationSolutionUri,
        OptimizationToolOutput, OptimizeRouteScenariosRequest, OptimizeRoutesRequest,
        ProblemFamily, RunPhase, RunTimings, SolutionDetail, SolutionFeasibility,
        SolveConvexRequest, SolveMilpRequest, SolverTermination, VerifySolutionOutput,
        VerifySolutionRequest,
    },
    profiles::profiles,
    reads::{OptimizationCompletionPage, OptimizationReads},
    task_records::SolveTaskCommon,
    usage::OptimizationUsage,
};
use veoveo_platform_store::{DomainUsageKind as StoreUsageKind, DomainUsageRecord, TaskStatus};
use veoveo_task_runtime::TaskSnapshot;
use veoveo_types::TaskId;

use super::{
    app_state::AppState,
    ownership::{runtime_owner, task_owner_from_runtime},
    problems::{load_prepared_problem_by_uri, load_solution},
    prompts::OptimizationPrompt,
    setup::{OptimizationContract, SERVER_SETUP},
};

const ROUTES_APP_TOOLS: &[&str] = &["optimize_route_scenarios", "optimize_routes"];
const MODELS_APP_TOOLS: &[&str] = &["solve_convex", "solve_milp", "verify_solution"];

#[derive(Clone)]
pub(super) struct OptimizationMcp {
    state: Arc<AppState>,
    tool_router: ToolRouter<OptimizationMcp>,
}

#[tool_router]
impl OptimizationMcp {
    pub(super) fn new(state: Arc<AppState>) -> Self {
        std::sync::LazyLock::force(&SERVER_SETUP);
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    #[cfg(test)]
    pub(super) fn tool_definitions() -> Vec<rmcp::model::Tool> {
        Self::tool_router().list_all()
    }

    #[rmcp::tool(
        title = "Optimize vehicle routes",
        description = "Solve a vehicle-routing or pickup-and-delivery problem with cuOpt: mixed vehicle types, cost and transit-time matrices, time windows, breaks, capacities, order-vehicle restrictions, optional orders, fixed costs, and weighted objectives. The problem can be inline, a saved Optimization problem, an artifact, or a Map travel model. The solution is checked independently of the solver. Run as an MCP Task.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<veoveo_optimization_mcp::contract::OptimizationToolOutput>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn optimize_routes(
        &self,
        Parameters(_request): Parameters<OptimizeRoutesRequest>,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        task_required("optimize_routes")
    }

    #[rmcp::tool(
        title = "Optimize route scenarios",
        description = "Solve 2 to 64 independent routing cases as one cuOpt GPU batch and return a checked solution for each case. Run as an MCP Task.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<veoveo_optimization_mcp::contract::OptimizationToolOutput>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn optimize_route_scenarios(
        &self,
        Parameters(_request): Parameters<OptimizeRouteScenariosRequest>,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        task_required("optimize_route_scenarios")
    }

    #[rmcp::tool(
        title = "Solve a convex model",
        description = "Solve a continuous LP, QP, QCQP, or SOCP problem with cuOpt on the GPU, then check variables, bounds, constraints, and objective independently of the solver. Run as an MCP Task.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<veoveo_optimization_mcp::contract::OptimizationToolOutput>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn solve_convex(
        &self,
        Parameters(_request): Parameters<SolveConvexRequest>,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        task_required("solve_convex")
    }

    #[rmcp::tool(
        title = "Solve a mixed-integer model",
        description = "Solve a linear MILP with continuous, integer, and semi-continuous variables, an optional MIP start, an optional quality target, and a history of incumbent solutions. The result is checked for bounds, integrality, constraints, and objective independently of the solver. Run as an MCP Task.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<veoveo_optimization_mcp::contract::OptimizationToolOutput>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn solve_milp(
        &self,
        Parameters(_request): Parameters<SolveMilpRequest>,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        task_required("solve_milp")
    }

    #[rmcp::tool(
        title = "Verify an optimization solution",
        description = "Re-check a saved Optimization solution for route feasibility, bounds, integrality, constraints, and objective, using tolerances you choose. Does not rerun cuOpt. Run as an MCP Task.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<VerifySolutionOutput>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn verify_solution(
        &self,
        Parameters(_request): Parameters<VerifySolutionRequest>,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        task_required("verify_solution")
    }
}

impl DomainServer for OptimizationMcp {
    type Contract = OptimizationContract;

    fn setup() -> &'static McpServerSetup<OptimizationContract> {
        &SERVER_SETUP
    }

    fn tool_router(&self) -> &ToolRouter<Self> {
        &self.tool_router
    }

    fn describe_tool(&self, tool: Tool) -> Tool {
        let app = if ROUTES_APP_TOOLS.contains(&tool.name.as_ref()) {
            uris::ROUTES_APP_URI
        } else if MODELS_APP_TOOLS.contains(&tool.name.as_ref()) {
            uris::MODELS_APP_URI
        } else {
            return tool;
        };
        veoveo_mcp_apps_extension::link_tool_to_app(
            tool,
            app,
            &[
                veoveo_mcp_apps_extension::UiVisibility::Model,
                veoveo_mcp_apps_extension::UiVisibility::App,
            ],
        )
    }

    async fn read(
        &self,
        address: DomainAddress<OptimizationContract>,
        request: &ReadResourceRequestParams,
        context: &RequestContext<RoleServer>,
    ) -> Result<DomainRead, McpError> {
        self.read_optimization_resource(address, &request.uri, context)
            .await
            .map(DomainRead::private)
    }

    fn prompts(&self) -> Vec<Prompt> {
        OptimizationPrompt::ALL
            .into_iter()
            .map(OptimizationPrompt::definition)
            .collect()
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        OptimizationPrompt::by_name(&request.name)
            .ok_or_else(|| unknown_prompt(&request.name))?
            .render(request.arguments)
    }

    async fn complete(
        &self,
        request: CompleteRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, McpError> {
        let Reference::Resource(reference) = &request.r#ref else {
            return Ok(CompleteResult::default());
        };
        let identity = gateway_identity(&context)?;
        let needle = request.argument.value.to_ascii_lowercase();
        let (values, total, has_more) =
            completion_values(&self.state, &identity, &reference.uri, &needle).await?;
        Ok(CompleteResult::new(
            CompletionInfo::with_pagination(values, total, has_more)
                .map_err(|error| McpError::internal_error(error, None))?,
        ))
    }
}

/// Optimization's problem, run and solution indexes, plus resource-list changes.
pub(super) struct OptimizationSubscriptions {
    state: Arc<AppState>,
}

impl OptimizationSubscriptions {
    pub(super) fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

impl ResourceSubscriptions for OptimizationSubscriptions {
    type Address = OptimizationResource;

    async fn authorize(
        &self,
        addresses: Vec<OptimizationResource>,
        context: &RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        gateway_identity(context)?;
        // Only the unpaged index roots change; every other resource is immutable.
        let subscribable = |address: &OptimizationResource| matches!(address, OptimizationResource::Collection(index) if index.cursor().is_none());
        if addresses.iter().all(subscribable) {
            Ok(())
        } else {
            Err(McpError::invalid_params(
                "resource is immutable or not subscribable",
                None,
            ))
        }
    }

    fn hub(&self) -> &SubscriptionHub {
        self.state.subscriptions.as_ref()
    }

    fn resource_lists(&self) -> Option<&ResourceListObservers> {
        Some(self.state.resource_observers.as_ref())
    }
}

impl OptimizationMcp {
    /// Reads one admitted address. The host serves documents and the contract.
    async fn read_optimization_resource(
        &self,
        address: OptimizationResource,
        uri: &str,
        context: &RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let identity = gateway_identity(context)?;
        let caller = plane_caller(context)?;
        match &address {
            OptimizationResource::Docs
            | OptimizationResource::Document(_)
            | OptimizationResource::Contract => Err(served_by_host()),
            OptimizationResource::RoutesApp | OptimizationResource::ModelsApp => {
                let routes = matches!(address, OptimizationResource::RoutesApp);
                let (app_id, title, subtitle, tools) = if routes {
                    (
                        "optimization-routes",
                        "Routes",
                        "Submit GPU route plans and inspect independently verified solutions",
                        vec![
                            veoveo_mcp_apps_extension::WorkbenchTool {
                                label: "Optimize routes",
                                name: "optimize_routes",
                                arguments_json: "{}",
                            },
                            veoveo_mcp_apps_extension::WorkbenchTool {
                                label: "Compare route scenarios",
                                name: "optimize_route_scenarios",
                                arguments_json: "{}",
                            },
                        ],
                    )
                } else {
                    (
                        "optimization-models",
                        "Models",
                        "Solve GPU mathematical models and inspect verification evidence",
                        vec![
                            veoveo_mcp_apps_extension::WorkbenchTool {
                                label: "Solve convex model",
                                name: "solve_convex",
                                arguments_json: "{}",
                            },
                            veoveo_mcp_apps_extension::WorkbenchTool {
                                label: "Solve mixed-integer model",
                                name: "solve_milp",
                                arguments_json: "{}",
                            },
                            veoveo_mcp_apps_extension::WorkbenchTool {
                                label: "Verify solution",
                                name: "verify_solution",
                                arguments_json: r#"{"solution_uri":""}"#,
                            },
                        ],
                    )
                };
                let resources = if routes {
                    vec![
                        veoveo_mcp_apps_extension::WorkbenchResource {
                            label: "Capabilities",
                            uri: uris::CAPABILITIES_URI,
                        },
                        veoveo_mcp_apps_extension::WorkbenchResource {
                            label: "Route runs",
                            uri: uris::RUNS_URI,
                        },
                        veoveo_mcp_apps_extension::WorkbenchResource {
                            label: "Route solutions",
                            uri: uris::SOLUTIONS_URI,
                        },
                    ]
                } else {
                    vec![
                        veoveo_mcp_apps_extension::WorkbenchResource {
                            label: "Solver profiles",
                            uri: uris::PROFILES_URI,
                        },
                        veoveo_mcp_apps_extension::WorkbenchResource {
                            label: "Problems",
                            uri: uris::PROBLEMS_URI,
                        },
                        veoveo_mcp_apps_extension::WorkbenchResource {
                            label: "Solutions",
                            uri: uris::SOLUTIONS_URI,
                        },
                    ]
                };
                let html = veoveo_mcp_apps_extension::workbench_app_html(
                    &veoveo_mcp_apps_extension::WorkbenchApp {
                        app_id,
                        title,
                        subtitle,
                        empty_message: "No optimization runs are visible to this identity.",
                        resources: &resources,
                        tools: &tools,
                        stream_result: None,
                    },
                );
                Ok(ReadResourceResult::new(vec![
                    veoveo_mcp_apps_extension::app_html_contents(uri, &html),
                ]))
            }
            OptimizationResource::Capabilities => json_read(uri, &capabilities(&self.state)),
            OptimizationResource::Profiles => json_read(uri, &profiles()),
            OptimizationResource::Profile(profile_uri) => {
                let profile = profiles()
                    .iter()
                    .find(|profile| &profile.profile_id == profile_uri.id())
                    .ok_or_else(|| not_found("solver profile"))?;
                json_read(uri, profile)
            }
            OptimizationResource::Collection(collection_request) => {
                let page = OptimizationReads::new(&self.state.tasks)
                    .map_err(internal)?
                    .page(&runtime_owner(&identity), collection_request)
                    .await
                    .map_err(internal)?;
                match collection_request.collection() {
                    OptimizationCollection::Problems => {
                        let problems = page
                            .items
                            .into_iter()
                            .map(|task| {
                                let common =
                                    task.request.common().expect("reader admits solve Tasks");
                                Ok(ProblemIndexEntry {
                                    problem_uri: OptimizationProblemUri::new(
                                        common.problem_id.clone(),
                                    )
                                    .map_err(internal)?,
                                    family: common.family,
                                })
                            })
                            .collect::<Result<Vec<_>, McpError>>()?;
                        json_read(
                            uri,
                            &ProblemIndexPage {
                                problems,
                                limit: OPTIMIZATION_INDEX_PAGE_SIZE,
                                next_cursor: page.next_cursor,
                            },
                        )
                    }
                    OptimizationCollection::Runs => {
                        let runs = page
                            .items
                            .into_iter()
                            .map(|task| {
                                let common =
                                    task.request.common().expect("reader admits solve Tasks");
                                Ok(RunIndexEntry {
                                    run_uri: OptimizationRunUri::new(common.run_id.clone())
                                        .map_err(internal)?,
                                    family: common.family,
                                    phase: run_phase(&task.snapshot),
                                })
                            })
                            .collect::<Result<Vec<_>, McpError>>()?;
                        json_read(
                            uri,
                            &RunIndexPage {
                                runs,
                                limit: OPTIMIZATION_INDEX_PAGE_SIZE,
                                next_cursor: page.next_cursor,
                            },
                        )
                    }
                    OptimizationCollection::Solutions => {
                        let solutions = page
                            .items
                            .into_iter()
                            .map(|task| {
                                task.output
                                    .expect("reader admits successful solution Tasks")
                            })
                            .map(|output| SolutionIndexEntry {
                                result_uri: output.result_uri,
                                family: output.family,
                                feasibility: output.feasibility,
                                termination: output.termination,
                            })
                            .collect();
                        json_read(
                            uri,
                            &SolutionIndexPage {
                                solutions,
                                limit: OPTIMIZATION_INDEX_PAGE_SIZE,
                                next_cursor: page.next_cursor,
                            },
                        )
                    }
                }
            }
            OptimizationResource::Problem(problem_uri) => {
                let prepared = load_prepared_problem_by_uri(&self.state, &identity, problem_uri)
                    .await
                    .map_err(not_found_error)?;
                json_read(uri, prepared.resource())
            }
            OptimizationResource::Run(run_uri) => {
                let task = OptimizationReads::new(&self.state.tasks)
                    .map_err(internal)?
                    .run(&runtime_owner(&identity), run_uri.id())
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("run"))?;
                let common = task.request.common().expect("matched solve task");
                let solution = if let Some(output) = &task.output {
                    Some(
                        load_solution(&self.state, &identity, &caller, &output.result_uri)
                            .await
                            .map_err(not_found_error)?,
                    )
                } else {
                    None
                };
                json_read(
                    uri,
                    &run_record(&self.state, &task.snapshot, common, solution.as_ref())?,
                )
            }
            OptimizationResource::RunIncumbents(run_id) => {
                let solution = solution_for_run(&self.state, &identity, &caller, run_id).await?;
                let incumbents = match &solution.detail {
                    SolutionDetail::Milp { incumbents, .. } => incumbents.clone(),
                    _ => Vec::new(),
                };
                json_read(uri, &incumbents)
            }
            OptimizationResource::Solution(solution_uri) => {
                let solution = load_solution(&self.state, &identity, &caller, solution_uri)
                    .await
                    .map_err(not_found_error)?;
                json_read(uri, &solution)
            }
            OptimizationResource::SolutionRoutes(solution_id) => {
                let solution_uri =
                    OptimizationSolutionUri::new(solution_id.clone()).map_err(internal)?;
                let solution = load_solution(&self.state, &identity, &caller, &solution_uri)
                    .await
                    .map_err(not_found_error)?;
                let SolutionDetail::Routing { routes, .. } = solution.detail else {
                    return Err(McpError::invalid_params(
                        "solution is not a routing solution",
                        None,
                    ));
                };
                json_read(uri, &routes)
            }
            OptimizationResource::SolutionVariables(solution_id) => {
                let solution_uri =
                    OptimizationSolutionUri::new(solution_id.clone()).map_err(internal)?;
                let solution = load_solution(&self.state, &identity, &caller, &solution_uri)
                    .await
                    .map_err(not_found_error)?;
                let variables = match solution.detail {
                    SolutionDetail::Convex { variables, .. }
                    | SolutionDetail::Milp { variables, .. } => variables,
                    SolutionDetail::Routing { .. } => {
                        return Err(McpError::invalid_params(
                            "solution is not a mathematical solution",
                            None,
                        ));
                    }
                };
                json_read(uri, &variables)
            }
            OptimizationResource::SolutionVerification(solution_id) => {
                let solution_uri =
                    OptimizationSolutionUri::new(solution_id.clone()).map_err(internal)?;
                let solution = load_solution(&self.state, &identity, &caller, &solution_uri)
                    .await
                    .map_err(not_found_error)?;
                json_read(uri, &solution.verification)
            }
            OptimizationResource::Usage(index) => {
                let page = OptimizationUsage::new(&self.state.tasks)
                    .map_err(internal)?
                    .page(&runtime_owner(&identity), index.cursor())
                    .await
                    .map_err(internal)?;
                json_read(uri, &page)
            }
            OptimizationResource::TaskUsage(address) => {
                let records = OptimizationUsage::new(&self.state.tasks)
                    .map_err(internal)?
                    .task(&runtime_owner(&identity), address)
                    .await
                    .map_err(internal)?;
                if records.is_empty() {
                    return Err(not_found("task usage"));
                }
                let task_id = address.task_id();
                let report = UsageReport::new(task_id.to_string(), address.as_str()).with_records(
                    records
                        .into_iter()
                        .map(|record| usage_record(task_id, record))
                        .collect(),
                );
                json_read(uri, &report)
            }
            OptimizationResource::Artifact(artifact_id) => {
                let artifact = self
                    .state
                    .artifacts
                    .get(&caller, artifact_id)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("artifact"))?;
                Ok(ReadResourceResult::new(vec![
                    ResourceContents::blob(BASE64_STANDARD.encode(artifact.bytes), uri)
                        .with_mime_type(
                            artifact
                                .metadata
                                .mime_type
                                .unwrap_or_else(|| "application/octet-stream".to_owned()),
                        ),
                ]))
            }
        }
    }
}

#[derive(Debug, Serialize)]
struct OptimizationCapabilities {
    contract_version: &'static str,
    cuopt_version: &'static str,
    cuopt_container_digest: &'static str,
    gpu_required: bool,
    gpu_name: String,
    gpu_uuid: String,
    compute_capability: String,
    problem_families: Vec<&'static str>,
    routing_order_families: Vec<&'static str>,
    model_artifact_formats: Vec<&'static str>,
    maximum_inline_matrix_cells: usize,
    maximum_inline_model_nonzeros: usize,
    maximum_route_cases: usize,
    maximum_executor_frame_bytes: u64,
    independent_verification: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct ProblemIndexEntry {
    problem_uri: OptimizationProblemUri,
    family: ProblemFamily,
}

#[derive(Debug, Serialize)]
struct ProblemIndexPage {
    problems: Vec<ProblemIndexEntry>,
    limit: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<OptimizationIndexCursor>,
}

#[derive(Debug, Serialize)]
struct RunIndexEntry {
    run_uri: OptimizationRunUri,
    family: ProblemFamily,
    phase: RunPhase,
}

#[derive(Debug, Serialize)]
struct RunIndexPage {
    runs: Vec<RunIndexEntry>,
    limit: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<OptimizationIndexCursor>,
}

#[derive(Debug, Serialize)]
struct SolutionIndexEntry {
    result_uri: OptimizationSolutionUri,
    family: ProblemFamily,
    feasibility: SolutionFeasibility,
    termination: SolverTermination,
}

#[derive(Debug, Serialize)]
struct SolutionIndexPage {
    solutions: Vec<SolutionIndexEntry>,
    limit: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<OptimizationIndexCursor>,
}

fn capabilities(state: &AppState) -> OptimizationCapabilities {
    OptimizationCapabilities {
        contract_version: veoveo_optimization_mcp::contract::OPTIMIZATION_CONTRACT_VERSION,
        cuopt_version: CUOPT_STABLE_VERSION,
        cuopt_container_digest: CUOPT_CONTAINER_DIGEST,
        gpu_required: true,
        gpu_name: state.executor_health.gpu_name.clone(),
        gpu_uuid: state.executor_health.gpu_uuid.clone(),
        compute_capability: state.executor_health.compute_capability.clone(),
        problem_families: vec!["routing", "route_scenarios", "convex", "milp"],
        routing_order_families: vec!["service", "pickup_delivery"],
        model_artifact_formats: vec!["optimization_json_v1"],
        maximum_inline_matrix_cells: veoveo_optimization_mcp::contract::MAX_INLINE_MATRIX_CELLS,
        maximum_inline_model_nonzeros: veoveo_optimization_mcp::contract::MAX_INLINE_MODEL_NONZEROS,
        maximum_route_cases: veoveo_optimization_mcp::contract::MAX_ROUTE_CASES,
        maximum_executor_frame_bytes: state.max_executor_frame_bytes,
        independent_verification: vec![
            "routing_endpoints",
            "routing_precedence",
            "routing_windows",
            "routing_capacity",
            "routing_travel_arcs",
            "variable_bounds",
            "integrality",
            "linear_constraints",
            "quadratic_constraints",
            "objective",
        ],
    }
}

fn run_record(
    state: &AppState,
    snapshot: &TaskSnapshot,
    common: &SolveTaskCommon,
    solution: Option<&OptimizationSolution>,
) -> Result<OptimizationRunRecord, McpError> {
    let owner = task_owner_from_runtime(snapshot.task_id, &snapshot.owner)
        .map_err(|error| McpError::internal_error(error, None))?;
    let output = snapshot
        .result
        .as_ref()
        .and_then(|result| result.get("structuredContent"))
        .and_then(|value| serde_json::from_value::<OptimizationToolOutput>(value.clone()).ok());
    let incumbent = solution.and_then(|solution| match &solution.detail {
        SolutionDetail::Milp { incumbents, .. } => incumbents.last().cloned(),
        _ => None,
    });
    Ok(OptimizationRunRecord {
        run_id: common.run_id.clone(),
        run_uri: OptimizationRunUri::new(common.run_id.clone()).map_err(internal)?,
        problem_uri: OptimizationProblemUri::new(common.problem_id.clone()).map_err(internal)?,
        family: common.family,
        phase: run_phase(snapshot),
        incumbent,
        solution_uri: output.map(|output| output.result_uri),
        engine: solution.map_or_else(
            || EngineProvenance {
                name: "NVIDIA cuOpt".to_owned(),
                version: state.executor_health.cuopt_version.clone(),
                container_digest: CUOPT_CONTAINER_DIGEST.to_owned(),
                executor_protocol: veoveo_optimization_mcp::contract::EXECUTOR_PROTOCOL_VERSION
                    .to_owned(),
                gpu_name: Some(state.executor_health.gpu_name.clone()),
                gpu_uuid: Some(state.executor_health.gpu_uuid.clone()),
                compute_capability: Some(state.executor_health.compute_capability.clone()),
                solver_profile_uri: common.profile_uri.clone(),
            },
            |solution| solution.engine.clone(),
        ),
        timings: solution.map_or_else(RunTimings::default, |solution| solution.timings.clone()),
        authority: OptimizationAuthority {
            principal_id: owner.principal_id,
            work_context: Some(owner.authority.work_context),
            policy_revision: owner.authority.policy_revision,
            submitted_at: common.submitted_at,
        },
        created_at: snapshot.created_at,
        updated_at: snapshot.updated_at,
    })
}

fn run_phase(snapshot: &TaskSnapshot) -> RunPhase {
    match snapshot.status {
        TaskStatus::Queued => RunPhase::Queued,
        TaskStatus::Running | TaskStatus::Waiting | TaskStatus::CancelRequested => {
            match snapshot.status_message.as_deref() {
                Some(message) if message.contains("queued for cuOpt") => RunPhase::Queued,
                Some(message) if message.contains("solving") => RunPhase::Solving,
                Some(message) if message.contains("publishing") => RunPhase::Publishing,
                _ => RunPhase::Preparing,
            }
        }
        TaskStatus::Succeeded => RunPhase::Completed,
        TaskStatus::Failed => RunPhase::Failed,
        TaskStatus::Cancelled => RunPhase::Cancelled,
    }
}

async fn solution_for_run(
    state: &AppState,
    identity: &veoveo_mcp_contract::GatewayInternalIdentity,
    caller: &veoveo_mcp_contract::PlaneCaller,
    run_id: &veoveo_optimization_mcp::contract::RunId,
) -> Result<OptimizationSolution, McpError> {
    let output = OptimizationReads::new(&state.tasks)
        .map_err(internal)?
        .run(&runtime_owner(identity), run_id)
        .await
        .map_err(internal)?
        .and_then(|task| task.output)
        .ok_or_else(|| not_found("completed run solution"))?;
    load_solution(state, identity, caller, &output.result_uri)
        .await
        .map_err(not_found_error)
}

async fn completion_values(
    state: &AppState,
    identity: &veoveo_mcp_contract::GatewayInternalIdentity,
    template: &str,
    needle: &str,
) -> Result<(Vec<String>, Option<u32>, bool), McpError> {
    if template == uris::PROFILE_TEMPLATE {
        let matching = profiles()
            .iter()
            .map(|profile| profile.profile_id.to_string())
            .filter(|value| value.to_ascii_lowercase().contains(needle))
            .collect::<Vec<_>>();
        let total = matching.len();
        return Ok((
            matching
                .into_iter()
                .take(CompletionInfo::MAX_VALUES)
                .collect(),
            Some(total as u32),
            total > CompletionInfo::MAX_VALUES,
        ));
    }
    let domain = if template == uris::PROBLEM_TEMPLATE {
        Some(OptimizationCollection::Problems)
    } else if template == uris::RUN_TEMPLATE || template == uris::RUN_INCUMBENTS_TEMPLATE {
        Some(OptimizationCollection::Runs)
    } else if matches!(
        template,
        uris::SOLUTION_TEMPLATE
            | uris::SOLUTION_ROUTES_TEMPLATE
            | uris::SOLUTION_VARIABLES_TEMPLATE
            | uris::SOLUTION_VERIFICATION_TEMPLATE
    ) {
        Some(OptimizationCollection::Solutions)
    } else {
        None
    };
    let Some(domain) = domain else {
        return Ok((Vec::new(), Some(0), false));
    };
    let reads = OptimizationReads::new(&state.tasks).map_err(internal)?;
    let owner = runtime_owner(identity);
    let (values, has_more) = match domain {
        OptimizationCollection::Problems => completion_wire(
            reads
                .complete_problems(&owner, needle, CompletionInfo::MAX_VALUES)
                .await
                .map_err(internal)?,
        ),
        OptimizationCollection::Runs => completion_wire(
            reads
                .complete_runs(&owner, needle, CompletionInfo::MAX_VALUES)
                .await
                .map_err(internal)?,
        ),
        OptimizationCollection::Solutions => completion_wire(
            reads
                .complete_solutions(&owner, needle, CompletionInfo::MAX_VALUES)
                .await
                .map_err(internal)?,
        ),
    };
    Ok((values, None, has_more))
}

fn completion_wire<T: ToString>(page: OptimizationCompletionPage<T>) -> (Vec<String>, bool) {
    (
        page.values.into_iter().map(|id| id.to_string()).collect(),
        page.has_more,
    )
}

fn usage_record(task_id: TaskId, record: DomainUsageRecord) -> UsageRecord {
    UsageRecord {
        task_id: task_id.to_string(),
        source_id: record.source_id,
        provider_job_id: record.provider_job_id,
        model_id: record.model_id,
        kind: match record.kind {
            StoreUsageKind::Estimate => UsageKind::Estimate,
            StoreUsageKind::Actual => UsageKind::Actual,
        },
        quantity: record.quantity,
        unit: record.unit,
        amount: record.amount,
        currency: record.currency,
        recorded_at: record.recorded_at,
        metadata: serde_json::Value::Object(record.metadata.into_map().into_iter().collect()),
    }
}

fn task_required<T>(name: &str) -> Result<T, McpError> {
    Err(McpError::invalid_request(
        format!("`{name}` must be called as an MCP Task. Resend the call with task parameters."),
        None,
    ))
}

fn internal(error: impl std::fmt::Display) -> McpError {
    McpError::internal_error(error.to_string(), None)
}

fn not_found(label: &str) -> McpError {
    McpError::resource_not_found(format!("unknown or unauthorized {label}"), None)
}

fn not_found_error(error: impl std::fmt::Display) -> McpError {
    McpError::resource_not_found(error.to_string(), None)
}
