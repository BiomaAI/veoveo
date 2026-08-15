use std::sync::{Arc, LazyLock};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, CancelTaskParams,
        CompleteRequestParams, CompleteResult, CompletionInfo, GetPromptRequestParams,
        GetTaskParams, GetTaskResult, ListPromptsResult, ListResourceTemplatesResult,
        ListResourcesResult, ListToolsResult, PaginatedRequestParams, Prompt,
        ReadResourceRequestParams, ReadResourceResult, Reference, Resource, ResourceContents,
        ResourceTemplate, ServerCapabilities, ServerInfo, SubscriptionFilter, UpdateTaskParams,
    },
    service::{RequestContext, SubscriptionContext},
    tool_handler, tool_router,
};
use serde::Serialize;
use veoveo_mcp_contract::{
    Page, UsageKind, UsageRecord, UsageReport,
    docs::{ContractDeclaration, ServerDocs},
    paginate,
};
use veoveo_optimization_mcp::{
    domain::{
        CUOPT_CONTAINER_DIGEST, CUOPT_STABLE_VERSION, EngineProvenance, OptimizationAuthority,
        OptimizationProblemUri, OptimizationRunRecord, OptimizationRunUri, OptimizationSolution,
        OptimizationSolutionUri, OptimizationToolOutput, OptimizeRouteScenariosRequest,
        OptimizeRoutesRequest, ProblemFamily, RunPhase, RunTimings, SolutionDetail,
        SolutionFeasibility, SolveConvexRequest, SolveMilpRequest, SolverTermination,
        VerifySolutionOutput, VerifySolutionRequest,
    },
    profiles::profiles,
    uris,
};
use veoveo_platform_store::{
    DomainUsageKind as StoreUsageKind, DomainUsageRecord, TaskId as StoreTaskId, TaskStatus,
};
use veoveo_task_runtime::TaskSnapshot;

use super::{
    app_state::AppState,
    index::{
        OPTIMIZATION_INDEX_PAGE_SIZE, OptimizationCollection, OptimizationCompletionDomain,
        completion_candidates, find_run_task, load_usage_index_page, parse_collection_uri,
        parse_usage_index_uri, visible_task_page,
    },
    ownership::{internal_caller, internal_identity, require_task_owner, task_owner_from_runtime},
    problems::{load_prepared_problem_by_uri, load_solution},
    prompts::OptimizationPrompt,
    records::SolveTaskCommon,
    task_extension::OptimizationTaskExtension,
};

const LIST_PAGE_SIZE: usize = 100;
const SERVER_SLUG: &str = "optimization";

pub(super) static SERVER_DOCS: LazyLock<ServerDocs> =
    LazyLock::new(|| veoveo_mcp_contract::server_docs!("optimization"));

#[derive(Clone)]
pub(super) struct OptimizationMcp {
    state: Arc<AppState>,
    task_service: OptimizationTaskExtension,
    #[allow(dead_code)]
    tool_router: ToolRouter<OptimizationMcp>,
}

#[tool_router]
impl OptimizationMcp {
    pub(super) fn new(state: Arc<AppState>) -> Self {
        Self {
            task_service: OptimizationTaskExtension::new(state.clone()),
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
        description = "Solve one service-routing or pickup-delivery problem with heterogeneous vehicles, cost and transit-time matrices, windows, breaks, capacities, order-vehicle restrictions, optional orders, fixed costs, and weighted cuOpt routing objectives. Inline, immutable Optimization problem, artifact, and Map travel-model sources are accepted. This operation requires durable task invocation.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<veoveo_optimization_mcp::domain::OptimizationToolOutput>(),
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
        description = "Solve two to sixty-four independent routing cases as one cuOpt GPU batch and return case-addressed, independently verified alternatives. This operation requires durable task invocation.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<veoveo_optimization_mcp::domain::OptimizationToolOutput>(),
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
        description = "Solve a typed continuous LP, QP, QCQP, or SOCP formulation through cuOpt's GPU mathematical solver, then independently check variables, bounds, constraints, and objective. This operation requires durable task invocation.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<veoveo_optimization_mcp::domain::OptimizationToolOutput>(),
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
        description = "Solve a typed linear MILP with continuous, integer, and semi-continuous variables, optional MIP start, bounded quality target, and retained incumbent history. The result is independently checked for bounds, integrality, constraints, and objective. This operation requires durable task invocation.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<veoveo_optimization_mcp::domain::OptimizationToolOutput>(),
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
        description = "Re-run the server's independent route, bound, integrality, constraint, and objective checks against an immutable Optimization solution with caller-selected finite tolerances. This operation requires durable task invocation.",
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

#[tool_handler(router = self.tool_router)]
impl ServerHandler for OptimizationMcp {
    fn supported_protocol_versions(
        &self,
    ) -> std::borrow::Cow<'static, [rmcp::model::ProtocolVersion]> {
        veoveo_mcp_contract::final_protocol_versions()
    }

    fn get_info(&self) -> ServerInfo {
        let mut capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_prompts()
            .enable_resources()
            .enable_resources_subscribe()
            .enable_resources_list_changed()
            .enable_completions()
            .build();
        capabilities.extensions.get_or_insert_default().insert(
            rmcp::model::TASKS_EXTENSION_ID.to_owned(),
            rmcp::model::JsonObject::new(),
        );
        let mut info = ServerInfo::default();
        info.capabilities = capabilities;
        info.server_info =
            rmcp::model::Implementation::new("optimization", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "GPU optimization through NVIDIA cuOpt. Read optimization://capabilities and \
             optimization://profiles first. Use optimize_routes for one routing model, \
             optimize_route_scenarios for homogeneous alternatives, solve_convex for continuous \
             LP/QP/QCQP/SOCP models, and solve_milp for mixed-integer linear models. Every tool \
             uses the MCP Task API. Problem, run, and solution identities are separate immutable \
             resources. Inspect the solution verification resource before operational use."
                .to_owned(),
        );
        info
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        if context
            .meta
            .client_capabilities()
            .is_some_and(|caps| caps.supports_tasks())
        {
            let caller = veoveo_task_runtime::DurableTaskService::authenticate(
                &self.task_service,
                &context,
            )?;
            if let Some(created) = veoveo_task_runtime::DurableTaskService::start_tool_task(
                &self.task_service,
                &caller,
                request.clone(),
            )
            .await?
            {
                return Ok(created.into());
            }
        }
        let call = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        self.tool_router.call(call).await
    }

    async fn get_task(
        &self,
        request: GetTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, McpError> {
        let caller =
            veoveo_task_runtime::DurableTaskService::authenticate(&self.task_service, &context)?;
        veoveo_task_runtime::DurableTaskService::get_task(&self.task_service, &caller, request)
            .await
    }

    async fn update_task(
        &self,
        request: UpdateTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let caller =
            veoveo_task_runtime::DurableTaskService::authenticate(&self.task_service, &context)?;
        veoveo_task_runtime::DurableTaskService::update_task(&self.task_service, &caller, request)
            .await
    }

    async fn cancel_task(
        &self,
        request: CancelTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let caller =
            veoveo_task_runtime::DurableTaskService::authenticate(&self.task_service, &context)?;
        veoveo_task_runtime::DurableTaskService::cancel_task(
            &self.task_service,
            &caller,
            request.task_id,
        )
        .await
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let mut tools = self.tool_router.list_all();
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        let page = mcp_page(tools, request.as_ref())?;
        Ok(ListToolsResult {
            tools: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: None,
        })
    }

    async fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let mut resources = well_known_resources();
        resources.extend(root_resources());
        resources.push(json_descriptor(
            uris::CAPABILITIES_URI,
            "Optimization capabilities",
            "Live cuOpt, GPU, contract, size-limit, and problem-family capabilities.",
        ));
        for profile in profiles() {
            resources.push(json_descriptor(
                profile.profile_uri.as_str(),
                &profile.title,
                &profile.description,
            ));
        }
        // Growing domain state stays behind bounded collection roots and
        // exact resource templates instead of inflating this catalog.
        resources.sort_by(|left, right| left.uri.cmp(&right.uri));
        resources.dedup_by(|left, right| left.uri == right.uri);
        let page = mcp_page(resources, request.as_ref())?;
        Ok(ListResourcesResult {
            resources: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: None,
        })
    }

    async fn list_resource_templates(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        let page = mcp_page(resource_templates(), request.as_ref())?;
        Ok(ListResourceTemplatesResult {
            resource_templates: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ReadResourceResponse, McpError> {
        let cacheable = request.request_state.is_none() && request.input_responses.is_none();
        async {
            let identity = internal_identity(&context)?;
            let caller = internal_caller(&context)?;
            let uri = request.uri.as_str();
            if uri == uris::DOCS_URI {
                return json_resource(uri, &SERVER_DOCS.iter().collect::<Vec<_>>());
            }
            if let Some(doc_id) = uris::parse_doc_uri(uri) {
                let doc = SERVER_DOCS
                    .doc(doc_id)
                    .ok_or_else(|| not_found("server document"))?;
                return Ok(ReadResourceResult::new(vec![
                    ResourceContents::text(doc.body, uri).with_mime_type("text/markdown"),
                ]));
            }
            if uri == uris::CONTRACT_URI {
                return json_resource(uri, &ContractDeclaration::from_docs(&SERVER_DOCS));
            }
            if uri == uris::CAPABILITIES_URI {
                return json_resource(uri, &capabilities(&self.state));
            }
            if uri == uris::PROFILES_URI {
                return json_resource(uri, &profiles());
            }
            if let Some(profile_id) = uris::parse_profile_uri(uri) {
                let profile = profiles()
                    .iter()
                    .find(|profile| profile.profile_id == profile_id)
                    .ok_or_else(|| not_found("solver profile"))?;
                return json_resource(uri, profile);
            }
            if let Some(collection_request) = parse_collection_uri(uri)
                .map_err(|error| McpError::invalid_params(error.to_string(), None))?
            {
                let page = visible_task_page(&self.state, &identity, &collection_request).await?;
                return match collection_request.collection {
                    OptimizationCollection::Problems => {
                        let problems = page
                            .items
                            .into_iter()
                            .filter_map(|task| {
                                let common = task.request.common()?;
                                Some(ProblemIndexEntry {
                                    problem_uri: OptimizationProblemUri::parse(uris::problem_uri(
                                        &common.problem_id,
                                    ))
                                    .ok()?,
                                    family: common.family,
                                })
                            })
                            .collect();
                        json_resource(
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
                            .filter_map(|task| {
                                let common = task.request.common()?;
                                Some(RunIndexEntry {
                                    run_uri: OptimizationRunUri::parse(uris::run_uri(
                                        &common.run_id,
                                    ))
                                    .ok()?,
                                    family: common.family,
                                    phase: run_phase(&task.snapshot),
                                })
                            })
                            .collect();
                        json_resource(
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
                            .filter_map(|task| task.output)
                            .map(|output| SolutionIndexEntry {
                                result_uri: output.result_uri,
                                family: output.family,
                                feasibility: output.feasibility,
                                termination: output.termination,
                            })
                            .collect();
                        json_resource(
                            uri,
                            &SolutionIndexPage {
                                solutions,
                                limit: OPTIMIZATION_INDEX_PAGE_SIZE,
                                next_cursor: page.next_cursor,
                            },
                        )
                    }
                };
            }
            if uris::parse_problem_uri(uri).is_some() {
                let prepared = load_prepared_problem_by_uri(&self.state, &identity, uri)
                    .await
                    .map_err(not_found_error)?;
                return json_resource(uri, prepared.resource());
            }
            if let Some(run_id) = uris::parse_run_uri(uri) {
                let task = find_run_task(&self.state, &identity, &run_id)
                    .await?
                    .ok_or_else(|| not_found("run"))?;
                let common = task.request.common().expect("matched solve task");
                let solution = if let Some(output) = &task.output {
                    Some(
                        load_solution(&self.state, &identity, &caller, output.result_uri.as_str())
                            .await
                            .map_err(not_found_error)?,
                    )
                } else {
                    None
                };
                return json_resource(
                    uri,
                    &run_record(&self.state, &task.snapshot, common, solution.as_ref())?,
                );
            }
            if let Some(run_id) = uris::parse_run_incumbents_uri(uri) {
                let solution = solution_for_run(&self.state, &identity, &caller, &run_id).await?;
                let incumbents = match &solution.detail {
                    SolutionDetail::Milp { incumbents, .. } => incumbents.clone(),
                    _ => Vec::new(),
                };
                return json_resource(uri, &incumbents);
            }
            if uris::parse_solution_uri(uri).is_some() {
                let solution = load_solution(&self.state, &identity, &caller, uri)
                    .await
                    .map_err(not_found_error)?;
                return json_resource(uri, &solution);
            }
            if let Some(solution_id) = uris::parse_solution_routes_uri(uri) {
                let solution_uri = uris::solution_uri(&solution_id);
                let solution = load_solution(&self.state, &identity, &caller, &solution_uri)
                    .await
                    .map_err(not_found_error)?;
                let SolutionDetail::Routing { routes, .. } = solution.detail else {
                    return Err(McpError::invalid_params(
                        "solution is not a routing solution",
                        None,
                    ));
                };
                return json_resource(uri, &routes);
            }
            if let Some(solution_id) = uris::parse_solution_variables_uri(uri) {
                let solution_uri = uris::solution_uri(&solution_id);
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
                return json_resource(uri, &variables);
            }
            if let Some(solution_id) = uris::parse_solution_verification_uri(uri) {
                let solution_uri = uris::solution_uri(&solution_id);
                let solution = load_solution(&self.state, &identity, &caller, &solution_uri)
                    .await
                    .map_err(not_found_error)?;
                return json_resource(uri, &solution.verification);
            }
            if let Some(after) = parse_usage_index_uri(uri)
                .map_err(|error| McpError::invalid_params(error.to_string(), None))?
            {
                return json_resource(
                    uri,
                    &load_usage_index_page(&self.state, &identity, after).await?,
                );
            }
            if let Some(task_id) = uris::parse_usage_task_uri(uri) {
                require_task_owner(&self.state, &context, task_id).await?;
                let records = self
                    .state
                    .tasks
                    .platform_store()
                    .domain_usage_for_task(
                        SERVER_SLUG,
                        task_id
                            .parse::<StoreTaskId>()
                            .map_err(|error| McpError::invalid_params(error.to_string(), None))?,
                    )
                    .await
                    .map_err(internal)?;
                if records.is_empty() {
                    return Err(not_found("task usage"));
                }
                let report = UsageReport::new(task_id, uri).with_records(
                    records
                        .into_iter()
                        .map(|record| usage_record(task_id, record))
                        .collect(),
                );
                return json_resource(uri, &report);
            }
            if let Some(artifact_id) = uris::parse_artifact_uri(uri) {
                let artifact = self
                    .state
                    .artifacts
                    .get(&caller, &artifact_id)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("artifact"))?;
                return Ok(ReadResourceResult::new(vec![
                    ResourceContents::blob(BASE64_STANDARD.encode(artifact.bytes), uri)
                        .with_mime_type(
                            artifact
                                .metadata
                                .mime_type
                                .unwrap_or_else(|| "application/octet-stream".to_owned()),
                        ),
                ]));
            }
            Err(McpError::resource_not_found(
                format!("unknown Optimization resource `{uri}`"),
                None,
            ))
        }
        .await
        .map(|result| veoveo_mcp_contract::private_resource_response(result, cacheable))
    }

    async fn list_prompts(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        let prompts = OptimizationPrompt::ALL
            .into_iter()
            .map(OptimizationPrompt::definition)
            .collect::<Vec<Prompt>>();
        let page = mcp_page(prompts, request.as_ref())?;
        Ok(ListPromptsResult {
            prompts: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: None,
        })
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::GetPromptResponse, McpError> {
        async {
            OptimizationPrompt::by_name(&request.name)
                .ok_or_else(|| McpError::invalid_params("unknown Optimization prompt", None))?
                .render(request.arguments)
        }
        .await
        .map(Into::into)
    }

    async fn complete(
        &self,
        request: CompleteRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, McpError> {
        let Reference::Resource(reference) = &request.r#ref else {
            return Ok(CompleteResult::default());
        };
        let identity = internal_identity(&context)?;
        let needle = request.argument.value.to_ascii_lowercase();
        let (values, total, has_more) =
            completion_values(&self.state, &identity, &reference.uri, &needle).await?;
        Ok(CompleteResult::new(
            CompletionInfo::with_pagination(values, total, has_more)
                .map_err(|error| McpError::internal_error(error, None))?,
        ))
    }

    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        veoveo_mcp_contract::accepted_subscription_filter(requested)
    }

    async fn listen(&self, context: SubscriptionContext) -> Result<(), McpError> {
        internal_identity(context.request_context())?;
        for uri in context.accepted().resource_subscriptions.iter().flatten() {
            if !is_subscribable(uri) {
                return Err(McpError::invalid_params(
                    "resource is immutable or not subscribable",
                    None,
                ));
            }
        }
        veoveo_task_runtime::listen_durable_subscriptions(
            &self.task_service,
            context,
            Some(self.state.subscriptions.as_ref()),
            Some(self.state.resource_observers.as_ref()),
        )
        .await
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
    next_cursor: Option<String>,
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
    next_cursor: Option<String>,
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
    next_cursor: Option<String>,
}

fn capabilities(state: &AppState) -> OptimizationCapabilities {
    OptimizationCapabilities {
        contract_version: veoveo_optimization_mcp::domain::OPTIMIZATION_CONTRACT_VERSION,
        cuopt_version: CUOPT_STABLE_VERSION,
        cuopt_container_digest: CUOPT_CONTAINER_DIGEST,
        gpu_required: true,
        gpu_name: state.executor_health.gpu_name.clone(),
        gpu_uuid: state.executor_health.gpu_uuid.clone(),
        compute_capability: state.executor_health.compute_capability.clone(),
        problem_families: vec!["routing", "route_scenarios", "convex", "milp"],
        routing_order_families: vec!["service", "pickup_delivery"],
        model_artifact_formats: vec!["optimization_json_v1"],
        maximum_inline_matrix_cells: veoveo_optimization_mcp::domain::MAX_INLINE_MATRIX_CELLS,
        maximum_inline_model_nonzeros: veoveo_optimization_mcp::domain::MAX_INLINE_MODEL_NONZEROS,
        maximum_route_cases: veoveo_optimization_mcp::domain::MAX_ROUTE_CASES,
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
    let owner = task_owner_from_runtime(&snapshot.task_id.to_string(), &snapshot.owner)
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
        run_uri: veoveo_optimization_mcp::domain::OptimizationRunUri::parse(uris::run_uri(
            &common.run_id,
        ))
        .map_err(internal)?,
        problem_uri: veoveo_optimization_mcp::domain::OptimizationProblemUri::parse(
            uris::problem_uri(&common.problem_id),
        )
        .map_err(internal)?,
        family: common.family,
        phase: run_phase(snapshot),
        incumbent,
        solution_uri: output.map(|output| output.result_uri),
        engine: solution.map_or_else(
            || EngineProvenance {
                name: "NVIDIA cuOpt".to_owned(),
                version: state.executor_health.cuopt_version.clone(),
                container_digest: CUOPT_CONTAINER_DIGEST.to_owned(),
                executor_protocol: veoveo_optimization_mcp::domain::EXECUTOR_PROTOCOL_VERSION
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
    run_id: &veoveo_optimization_mcp::domain::RunId,
) -> Result<OptimizationSolution, McpError> {
    let output = find_run_task(state, identity, run_id)
        .await?
        .and_then(|task| task.output)
        .ok_or_else(|| not_found("completed run solution"))?;
    load_solution(state, identity, caller, output.result_uri.as_str())
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
        Some(OptimizationCompletionDomain::Problems)
    } else if template == uris::RUN_TEMPLATE || template == uris::RUN_INCUMBENTS_TEMPLATE {
        Some(OptimizationCompletionDomain::Runs)
    } else if matches!(
        template,
        uris::SOLUTION_TEMPLATE
            | uris::SOLUTION_ROUTES_TEMPLATE
            | uris::SOLUTION_VARIABLES_TEMPLATE
            | uris::SOLUTION_VERIFICATION_TEMPLATE
    ) {
        Some(OptimizationCompletionDomain::Solutions)
    } else {
        None
    };
    let Some(domain) = domain else {
        return Ok((Vec::new(), Some(0), false));
    };
    let page =
        completion_candidates(state, identity, domain, needle, CompletionInfo::MAX_VALUES).await?;
    Ok((page.values, None, page.has_more))
}

fn resource_templates() -> Vec<ResourceTemplate> {
    [
        (
            uris::DOC_TEMPLATE,
            "Server document",
            "Embedded crate document.",
        ),
        (
            uris::PROFILE_TEMPLATE,
            "Solver profile",
            "Curated bounded cuOpt solver policy.",
        ),
        (
            uris::PROBLEM_TEMPLATE,
            "Optimization problem",
            "Immutable normalized problem and dimensions.",
        ),
        (
            uris::PROBLEMS_PAGE_TEMPLATE,
            "Optimization problem page",
            "Bounded problem index page selected by its opaque cursor.",
        ),
        (
            uris::RUN_TEMPLATE,
            "Optimization run",
            "Durable execution state and engine provenance.",
        ),
        (
            uris::RUNS_PAGE_TEMPLATE,
            "Optimization run page",
            "Bounded run index page selected by its opaque cursor.",
        ),
        (
            uris::RUN_INCUMBENTS_TEMPLATE,
            "MILP incumbents",
            "Ordered retained incumbent summaries.",
        ),
        (
            uris::SOLUTION_TEMPLATE,
            "Optimization solution",
            "Immutable verified solution.",
        ),
        (
            uris::SOLUTIONS_PAGE_TEMPLATE,
            "Optimization solution page",
            "Bounded solution index page selected by its opaque cursor.",
        ),
        (
            uris::SOLUTION_ROUTES_TEMPLATE,
            "Solution routes",
            "Case-addressed vehicle routes.",
        ),
        (
            uris::SOLUTION_VARIABLES_TEMPLATE,
            "Solution variables",
            "Mathematical variable values.",
        ),
        (
            uris::SOLUTION_VERIFICATION_TEMPLATE,
            "Solution verification",
            "Independent feasibility and objective checks.",
        ),
        (
            uris::ARTIFACT_TEMPLATE,
            "Optimization artifact",
            "Immutable bytes on the shared artifact plane.",
        ),
        (
            uris::USAGE_TASK_TEMPLATE,
            "Optimization task usage",
            "Measured GPU solve usage.",
        ),
        (
            uris::USAGE_PAGE_TEMPLATE,
            "Optimization usage page",
            "Bounded usage index page selected by its opaque cursor.",
        ),
    ]
    .into_iter()
    .map(|(uri, title, description)| {
        ResourceTemplate::new(uri, title)
            .with_title(title)
            .with_description(description)
            .with_mime_type(if uri == uris::DOC_TEMPLATE {
                "text/markdown"
            } else {
                "application/json"
            })
    })
    .collect()
}

fn well_known_resources() -> Vec<Resource> {
    let mut resources = vec![json_descriptor(
        uris::DOCS_URI,
        "Server documents",
        "Index of crate documents embedded at build time.",
    )];
    resources.extend(SERVER_DOCS.iter().map(|doc| {
        Resource::new(format!("optimization://docs/{}", doc.id), doc.title)
            .with_title(doc.title)
            .with_description("Crate document embedded at build time.")
            .with_mime_type("text/markdown")
    }));
    resources.push(json_descriptor(
        uris::CONTRACT_URI,
        "Contract declaration",
        "Machine-readable contract revision, compliance, and capabilities.",
    ));
    resources
}

fn root_resources() -> Vec<Resource> {
    [
        (uris::PROFILES_URI, "Solver profiles"),
        (uris::PROBLEMS_URI, "Optimization problems"),
        (uris::RUNS_URI, "Optimization runs"),
        (uris::SOLUTIONS_URI, "Optimization solutions"),
        (uris::USAGE_URI, "Optimization usage"),
    ]
    .into_iter()
    .map(|(uri, title)| json_descriptor(uri, title, "Authorized Optimization index."))
    .collect()
}

fn is_subscribable(uri: &str) -> bool {
    matches!(
        uri,
        uris::PROBLEMS_URI | uris::RUNS_URI | uris::SOLUTIONS_URI
    )
}

fn json_descriptor(uri: &str, title: &str, description: &str) -> Resource {
    Resource::new(uri.to_owned(), title.to_owned())
        .with_title(title.to_owned())
        .with_description(description.to_owned())
        .with_mime_type("application/json")
}

fn json_resource<T: Serialize>(uri: &str, value: &T) -> Result<ReadResourceResult, McpError> {
    Ok(ReadResourceResult::new(vec![
        ResourceContents::text(
            serde_json::to_string(value)
                .map_err(|error| McpError::internal_error(error.to_string(), None))?,
            uri,
        )
        .with_mime_type("application/json"),
    ]))
}

fn mcp_page<T>(
    items: Vec<T>,
    request: Option<&PaginatedRequestParams>,
) -> Result<Page<T>, McpError> {
    paginate(items, request, LIST_PAGE_SIZE)
        .map_err(|error| McpError::invalid_params(error.to_string(), None))
}

fn usage_record(task_id: &str, record: DomainUsageRecord) -> UsageRecord {
    UsageRecord {
        task_id: task_id.to_owned(),
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
        format!("{name} requires task-based invocation"),
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
