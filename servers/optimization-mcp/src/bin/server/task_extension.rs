use std::{
    collections::{BTreeMap, BTreeSet},
    num::{NonZeroU32, NonZeroU64},
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::{TimeDelta, Utc};
use futures::StreamExt;
use veoveo_mcp_contract::{
    GatewayInternalIdentity, IssueArtifactWriteCapabilityRequest, PlaneCaller,
};
use veoveo_mcp_task_extension::{
    AcknowledgeTaskResult, AdapterError, CancelTaskParams, CreateTaskResult, GetTaskParams,
    GetTaskResult, ProtocolTaskId, TaskExtensionHandler, TaskSubscription, ToolCallParams,
    UpdateTaskParams, project_snapshot, task_seed,
};
use veoveo_optimization_mcp::{
    domain::{
        EngineProvenance, NonNegativeF64, OptimizationAuthority, OptimizationSolution,
        OptimizeRouteScenariosRequest, OptimizeRoutesRequest, ProblemFamily, RunTimings,
        SolutionDetail, SolveConvexRequest, SolveMilpRequest, VerifySolutionRequest,
    },
    executor::{
        ExecutorModelFamily, ExecutorOperation, ExecutorRequest, ExecutorResult, ExecutorRouteNode,
        ExecutorRouteVisit, ExecutorRoutingSolution, ExecutorRoutingStatus, ExecutorVehicleRoute,
    },
    problem_store::{PreparedProblem, PreparedProblemRef},
    profiles::{convex_executor_profile, executor_profile},
    solution_builder::{
        SolutionContext, build_convex_solution, build_milp_solution, build_route_scenario_solution,
        build_routing_solution,
    },
    verification::{
        DEFAULT_ABSOLUTE_TOLERANCE, DEFAULT_RELATIVE_TOLERANCE, VerificationTolerance,
        verify_convex_candidate, verify_milp_candidate, verify_routing_solution,
    },
};
use veoveo_task_runtime::{
    CreateTask, RecoveryClass, TaskError, TaskFailure, TaskId, TaskOwner, TaskRetentionPin,
    TaskSnapshot, TaskTransition,
};

use super::{
    app_state::{AppState, update_task},
    internal_auth::ForwardedBearer,
    outputs::{RequestedArtifacts, solution_result, verification_result},
    ownership::{caller_from, runtime_owner, task_owner_from_runtime},
    problems::{
        load_prepared_problem_by_uri, load_solution, prepare_convex, prepare_milp,
        prepare_route_scenarios, prepare_routes,
    },
    records::{
        OptimizationTaskRequest, PreparedVerifyTask, SolveTaskCommon, TASK_TOOLS,
        VERIFY_SOLUTION_TASK,
    },
};

const SERVER_SLUG: &str = "optimization";
const TASK_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
const TASK_POLL_INTERVAL_MS: u64 = 3_000;
const TASK_LEASE_DURATION: Duration = Duration::from_secs(120);
const TASK_LEASE_HEARTBEAT: Duration = Duration::from_secs(40);
const ARTIFACT_CAPABILITY_TTL: TimeDelta = TimeDelta::hours(24);

#[derive(Clone)]
pub(super) struct OptimizationTaskExtension {
    state: Arc<AppState>,
}

#[derive(Clone)]
pub(super) struct AuthenticatedCaller {
    identity: GatewayInternalIdentity,
    plane: PlaneCaller,
}

impl OptimizationTaskExtension {
    pub(super) fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    async fn authorized_snapshot(
        &self,
        caller: &AuthenticatedCaller,
        task_id: ProtocolTaskId,
    ) -> Result<TaskSnapshot, AdapterError> {
        let snapshot = self
            .state
            .tasks
            .get(&task_id.to_string())
            .await
            .map_err(|error| AdapterError::internal(error.to_string()))?
            .ok_or_else(|| AdapterError::invalid_params("unknown task id"))?;
        let owner = runtime_owner(&caller.identity);
        if snapshot.owner.allows(
            &owner.principal_key,
            &owner.profile,
            owner.tenant_key.as_deref(),
            &owner.data_labels,
        ) && snapshot.owner.authority.work_context == owner.authority.work_context
        {
            Ok(snapshot)
        } else {
            Err(AdapterError::invalid_params("unknown task id"))
        }
    }
}

impl TaskExtensionHandler for OptimizationTaskExtension {
    type Caller = AuthenticatedCaller;

    fn authenticate(
        &self,
        extensions: &axum::http::Extensions,
    ) -> Result<Self::Caller, AdapterError> {
        let identity = extensions
            .get::<GatewayInternalIdentity>()
            .cloned()
            .ok_or_else(|| AdapterError::unauthorized("gateway identity missing"))?;
        let bearer = extensions
            .get::<ForwardedBearer>()
            .map(|bearer| bearer.0.clone())
            .ok_or_else(|| AdapterError::unauthorized("forwarded bearer missing"))?;
        Ok(AuthenticatedCaller {
            plane: caller_from(identity.clone(), bearer),
            identity,
        })
    }

    async fn start_tool_task(
        &self,
        caller: &Self::Caller,
        request: ToolCallParams,
    ) -> Result<Option<CreateTaskResult>, AdapterError> {
        if !TASK_TOOLS.contains(&request.name.as_str()) {
            return Ok(None);
        }
        let arguments = serde_json::Value::Object(request.arguments.into_iter().collect());
        let task_id = TaskId::new();
        let submitted_at = Utc::now();
        let durable = match request.name.as_str() {
            "optimize_routes" => {
                let input: OptimizeRoutesRequest = decode(arguments)?;
                executor_profile(&input.policy, ProblemFamily::Routing, false).map_err(invalid)?;
                let prepared =
                    prepare_routes(self.state.as_ref(), &caller.identity, &caller.plane, &input)
                        .await
                        .map_err(invalid)?;
                let prepared_ref = self
                    .state
                    .problem_store
                    .stage(task_id, &prepared)
                    .await
                    .map_err(internal)?;
                let capability = issue_output_capability(
                    self.state.as_ref(),
                    &caller.plane,
                    &task_id,
                    2 + u32::from(input.output.include_route_table_artifact),
                )
                .await
                .map_err(internal)?;
                OptimizationTaskRequest::OptimizeRoutes {
                    common: common(
                        &prepared,
                        prepared_ref,
                        input.policy.profile_uri.clone(),
                        task_id,
                        submitted_at,
                        capability,
                    )?,
                    input,
                }
            }
            "optimize_route_scenarios" => {
                let input: OptimizeRouteScenariosRequest = decode(arguments)?;
                executor_profile(&input.policy, ProblemFamily::RouteScenarios, false)
                    .map_err(invalid)?;
                let prepared = prepare_route_scenarios(
                    self.state.as_ref(),
                    &caller.identity,
                    &caller.plane,
                    &input,
                )
                .await
                .map_err(invalid)?;
                let prepared_ref = self
                    .state
                    .problem_store
                    .stage(task_id, &prepared)
                    .await
                    .map_err(internal)?;
                let capability = issue_output_capability(
                    self.state.as_ref(),
                    &caller.plane,
                    &task_id,
                    2 + u32::from(input.output.include_route_table_artifact),
                )
                .await
                .map_err(internal)?;
                OptimizationTaskRequest::OptimizeRouteScenarios {
                    common: common(
                        &prepared,
                        prepared_ref,
                        input.policy.profile_uri.clone(),
                        task_id,
                        submitted_at,
                        capability,
                    )?,
                    input,
                }
            }
            "solve_convex" => {
                let input: SolveConvexRequest = decode(arguments)?;
                executor_profile(&input.policy, ProblemFamily::Convex, false).map_err(invalid)?;
                let prepared =
                    prepare_convex(self.state.as_ref(), &caller.identity, &caller.plane, &input)
                        .await
                        .map_err(invalid)?;
                let prepared_ref = self
                    .state
                    .problem_store
                    .stage(task_id, &prepared)
                    .await
                    .map_err(internal)?;
                let artifact_count = 2 + u32::from(input.output.retain_warm_start);
                let capability = issue_output_capability(
                    self.state.as_ref(),
                    &caller.plane,
                    &task_id,
                    artifact_count,
                )
                .await
                .map_err(internal)?;
                OptimizationTaskRequest::SolveConvex {
                    common: common(
                        &prepared,
                        prepared_ref,
                        input.policy.profile_uri.clone(),
                        task_id,
                        submitted_at,
                        capability,
                    )?,
                    input,
                }
            }
            "solve_milp" => {
                let input: SolveMilpRequest = decode(arguments)?;
                executor_profile(
                    &input.policy,
                    ProblemFamily::Milp,
                    input.output.retain_incumbents,
                )
                .map_err(invalid)?;
                let prepared =
                    prepare_milp(self.state.as_ref(), &caller.identity, &caller.plane, &input)
                        .await
                        .map_err(invalid)?;
                let prepared_ref = self
                    .state
                    .problem_store
                    .stage(task_id, &prepared)
                    .await
                    .map_err(internal)?;
                let artifact_count = 2
                    + u32::from(input.output.retain_warm_start)
                    + u32::from(input.output.retain_incumbents);
                let capability = issue_output_capability(
                    self.state.as_ref(),
                    &caller.plane,
                    &task_id,
                    artifact_count,
                )
                .await
                .map_err(internal)?;
                OptimizationTaskRequest::SolveMilp {
                    common: common(
                        &prepared,
                        prepared_ref,
                        input.policy.profile_uri.clone(),
                        task_id,
                        submitted_at,
                        capability,
                    )?,
                    input,
                }
            }
            VERIFY_SOLUTION_TASK => {
                let input: VerifySolutionRequest = decode(arguments)?;
                let solution = load_solution(
                    self.state.as_ref(),
                    &caller.identity,
                    &caller.plane,
                    input.solution_uri.as_str(),
                )
                .await
                .map_err(invalid)?;
                let prepared = load_prepared_problem_by_uri(
                    self.state.as_ref(),
                    &caller.identity,
                    solution.problem_uri.as_str(),
                )
                .await
                .map_err(invalid)?;
                if prepared.resource().record.problem_uri != solution.problem_uri {
                    return Err(AdapterError::invalid_params(
                        "solution problem identity does not match the prepared problem",
                    ));
                }
                let prepared_ref = find_prepared_ref(
                    self.state.as_ref(),
                    &caller.identity,
                    solution.problem_uri.as_str(),
                )
                .await
                .map_err(internal)?;
                let capability =
                    issue_output_capability(self.state.as_ref(), &caller.plane, &task_id, 1)
                        .await
                        .map_err(internal)?;
                OptimizationTaskRequest::VerifySolution {
                    request: PreparedVerifyTask {
                        input,
                        solution,
                        prepared: prepared_ref,
                        submitted_at,
                        artifact_write_capability: capability,
                    },
                }
            }
            _ => return Ok(None),
        };
        let retention_pins = request.meta.task_retention_pin.into_iter().collect();
        let snapshot = start_task(
            self.state.clone(),
            task_id,
            caller.identity.clone(),
            durable,
            retention_pins,
        )
        .await
        .map_err(internal)?;
        Ok(Some(CreateTaskResult::new(task_seed(&snapshot))))
    }

    async fn get_task(
        &self,
        caller: &Self::Caller,
        request: GetTaskParams,
    ) -> Result<GetTaskResult, AdapterError> {
        let snapshot = self.authorized_snapshot(caller, request.task_id).await?;
        let task = project_snapshot(&self.state.tasks, snapshot)
            .await
            .map_err(|error| AdapterError::internal(error.to_string()))?;
        Ok(GetTaskResult::new(task))
    }

    async fn update_task(
        &self,
        caller: &Self::Caller,
        request: UpdateTaskParams,
    ) -> Result<AcknowledgeTaskResult, AdapterError> {
        self.authorized_snapshot(caller, request.task_id).await?;
        self.state
            .tasks
            .submit_input_responses(&request.task_id.to_string(), request.input_responses)
            .await
            .map_err(|error| AdapterError::internal(error.to_string()))?;
        Ok(AcknowledgeTaskResult::complete())
    }

    async fn cancel_task(
        &self,
        caller: &Self::Caller,
        request: CancelTaskParams,
    ) -> Result<AcknowledgeTaskResult, AdapterError> {
        self.authorized_snapshot(caller, request.task_id).await?;
        self.state
            .tasks
            .cancel(&request.task_id.to_string())
            .await
            .map_err(|error| AdapterError::internal(error.to_string()))?;
        Ok(AcknowledgeTaskResult::complete())
    }

    async fn subscribe_tasks(
        &self,
        caller: &Self::Caller,
        task_ids: Vec<ProtocolTaskId>,
    ) -> Result<TaskSubscription, AdapterError> {
        let updates = self
            .state
            .tasks
            .live_updates()
            .await
            .map_err(|error| AdapterError::internal(error.to_string()))?;
        let mut accepted = Vec::new();
        for task_id in task_ids {
            if self.authorized_snapshot(caller, task_id).await.is_ok() {
                accepted.push(task_id);
            }
        }
        let accepted_set: BTreeSet<_> = accepted.iter().copied().collect();
        let runtime = self.state.tasks.clone();
        let caller_owner = runtime_owner(&caller.identity);
        let stream = updates.filter_map(move |update| {
            let accepted = accepted_set.clone();
            let runtime = runtime.clone();
            let caller_owner = caller_owner.clone();
            async move {
                let snapshot = match update {
                    Ok(update) => update.snapshot,
                    Err(error) => return Some(Err(AdapterError::internal(error.to_string()))),
                };
                if !accepted.contains(&ProtocolTaskId::from(snapshot.task_id))
                    || !snapshot.owner.allows(
                        &caller_owner.principal_key,
                        &caller_owner.profile,
                        caller_owner.tenant_key.as_deref(),
                        &caller_owner.data_labels,
                    )
                    || snapshot.owner.authority.work_context != caller_owner.authority.work_context
                {
                    return None;
                }
                Some(
                    project_snapshot(&runtime, snapshot)
                        .await
                        .map_err(|error| AdapterError::internal(error.to_string())),
                )
            }
        });
        Ok(TaskSubscription {
            accepted_task_ids: accepted,
            updates: Box::pin(stream),
        })
    }
}

pub(super) async fn recover_tasks(
    state: Arc<AppState>,
    resumable: Vec<TaskSnapshot>,
) -> anyhow::Result<()> {
    for snapshot in resumable {
        if !TASK_TOOLS.contains(&snapshot.task_type.as_str()) {
            anyhow::bail!(
                "unknown resumable Optimization task type `{}`",
                snapshot.task_type
            );
        }
        let request: OptimizationTaskRequest = serde_json::from_value(snapshot.request.clone())?;
        if request.task_type() != snapshot.task_type {
            anyhow::bail!("Optimization task type does not match its persisted request");
        }
        if let Err(error) = schedule_task(state.clone(), snapshot, request).await {
            match error.downcast_ref::<TaskError>() {
                Some(TaskError::LeaseHeld(task_id) | TaskError::Conflict(task_id)) => {
                    tracing::info!(
                        task_id,
                        "another worker claimed recovered Optimization task"
                    );
                }
                _ => return Err(error),
            }
        }
    }
    Ok(())
}

async fn start_task(
    state: Arc<AppState>,
    task_id: TaskId,
    identity: GatewayInternalIdentity,
    request: OptimizationTaskRequest,
    retention_pins: BTreeSet<TaskRetentionPin>,
) -> anyhow::Result<TaskSnapshot> {
    let task_type = request.task_type().to_owned();
    let created = state
        .tasks
        .create(CreateTask {
            task_id,
            owner: runtime_owner(&identity),
            server: SERVER_SLUG.to_owned(),
            task_type,
            request: serde_json::to_value(&request)?,
            recovery_class: RecoveryClass::Resume,
            idempotency_key: None,
            ttl_ms: Some(TASK_TTL_MS),
            poll_interval_ms: Some(TASK_POLL_INTERVAL_MS),
            retention_pins,
        })
        .await?;
    schedule_task(state, created.snapshot, request).await
}

async fn schedule_task(
    state: Arc<AppState>,
    snapshot: TaskSnapshot,
    request: OptimizationTaskRequest,
) -> anyhow::Result<TaskSnapshot> {
    let task_id = snapshot.task_id.to_string();
    let claimed = state.tasks.claim(&task_id, TASK_LEASE_DURATION).await?;
    let owner = snapshot.owner.clone();
    let cancellation = tokio_util::sync::CancellationToken::new();
    let join = tokio::spawn(run_task(
        state.clone(),
        task_id.clone(),
        owner,
        request,
        cancellation.clone(),
    ));
    state
        .tasks
        .register_worker(&task_id, cancellation, join)
        .await?;
    Ok(claimed.snapshot)
}

async fn run_task(
    state: Arc<AppState>,
    task_id: String,
    owner: TaskOwner,
    request: OptimizationTaskRequest,
    cancellation: tokio_util::sync::CancellationToken,
) {
    let work = run_task_inner(
        state.clone(),
        task_id.clone(),
        owner,
        request,
        cancellation.clone(),
    );
    tokio::pin!(work);
    let mut heartbeat = tokio::time::interval(TASK_LEASE_HEARTBEAT);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    heartbeat.tick().await;
    loop {
        tokio::select! {
            () = &mut work => break,
            _ = heartbeat.tick() => {
                if let Err(error) = state.tasks.renew_lease(&task_id, TASK_LEASE_DURATION).await {
                    tracing::warn!(task_id, "Optimization task lease heartbeat failed: {error}");
                    cancellation.cancel();
                    break;
                }
            }
        }
    }
}

async fn run_task_inner(
    state: Arc<AppState>,
    task_id: String,
    runtime_owner: TaskOwner,
    request: OptimizationTaskRequest,
    cancellation: tokio_util::sync::CancellationToken,
) {
    update_task(
        &state,
        &task_id,
        TaskTransition::Running {
            message: "preparing governed cuOpt execution".to_owned(),
            progress: 0.05,
        },
    )
    .await;
    let result = execute_task(
        state.as_ref(),
        &task_id,
        &runtime_owner,
        request,
        cancellation.clone(),
    )
    .await;
    if cancellation.is_cancelled() {
        update_task(&state, &task_id, TaskTransition::Cancelled).await;
        return;
    }
    match result {
        Ok(tool_result) => match serde_json::to_value(tool_result) {
            Ok(result) => {
                update_task(
                    &state,
                    &task_id,
                    TaskTransition::Succeeded {
                        message: "cuOpt execution completed".to_owned(),
                        result,
                    },
                )
                .await;
                for uri in [
                    veoveo_optimization_mcp::uris::PROBLEMS_URI,
                    veoveo_optimization_mcp::uris::RUNS_URI,
                    veoveo_optimization_mcp::uris::SOLUTIONS_URI,
                ] {
                    state.subscriptions.notify_resource_updated(uri).await;
                }
                state.resource_observers.notify_changed().await;
            }
            Err(error) => fail_task(&state, &task_id, "result_serialization_failed", error).await,
        },
        Err(error) => fail_task(&state, &task_id, "optimization_failed", error).await,
    }
}

async fn execute_task(
    state: &AppState,
    task_id: &str,
    runtime_owner: &TaskOwner,
    request: OptimizationTaskRequest,
    cancellation: tokio_util::sync::CancellationToken,
) -> anyhow::Result<rmcp::model::CallToolResult> {
    let owner = task_owner_from_runtime(task_id, runtime_owner).map_err(anyhow::Error::msg)?;
    match request {
        OptimizationTaskRequest::OptimizeRoutes { common, input } => {
            let prepared = state.problem_store.load(&common.prepared).await?;
            let PreparedProblem::Routing {
                resource, compiled, ..
            } = &prepared
            else {
                anyhow::bail!("prepared problem is not routing");
            };
            let executor_permit = acquire_executor_slot(state, task_id, &cancellation).await?;
            let queue_seconds = executor_permit.queue_seconds;
            update_solving(state, task_id, &common).await;
            let profile = executor_profile(&input.policy, ProblemFamily::Routing, false)?;
            let executor_started_at = Utc::now();
            let response = state
                .executor
                .execute(
                    &ExecutorRequest::new(
                        common.run_id.clone(),
                        profile,
                        ExecutorOperation::SolveRoutes {
                            problem: compiled.clone(),
                        },
                    ),
                    cancellation,
                )
                .await?;
            drop(executor_permit);
            let ExecutorResult::Routes { solution: raw } = response.result else {
                return executor_result_error(response.result);
            };
            let solution = build_routing_solution(
                compiled,
                &raw,
                solution_context(
                    state,
                    &common,
                    &owner,
                    resource.record.problem_uri.clone(),
                    executor_started_at,
                    queue_seconds,
                ),
            )?;
            update_publishing(state, task_id).await;
            solution_result(
                state,
                &common.artifact_write_capability,
                task_id,
                &owner,
                &prepared,
                solution,
                RequestedArtifacts::Routing(&input.output),
            )
            .await
        }
        OptimizationTaskRequest::OptimizeRouteScenarios { common, input } => {
            let prepared = state.problem_store.load(&common.prepared).await?;
            let PreparedProblem::RouteScenarios { resource, cases } = &prepared else {
                anyhow::bail!("prepared problem is not a route-scenario batch");
            };
            let executor_permit = acquire_executor_slot(state, task_id, &cancellation).await?;
            let queue_seconds = executor_permit.queue_seconds;
            update_solving(state, task_id, &common).await;
            let profile = executor_profile(&input.policy, ProblemFamily::RouteScenarios, false)?;
            let executor_started_at = Utc::now();
            let response = state
                .executor
                .execute(
                    &ExecutorRequest::new(
                        common.run_id.clone(),
                        profile,
                        ExecutorOperation::SolveRouteScenarios {
                            cases: cases
                                .iter()
                                .map(
                                    |case| veoveo_optimization_mcp::executor::CompiledRouteCase {
                                        case_id: case.case_id.clone(),
                                        problem: case.compiled.clone(),
                                    },
                                )
                                .collect(),
                        },
                    ),
                    cancellation,
                )
                .await?;
            drop(executor_permit);
            let ExecutorResult::RouteScenarios { solutions: raw } = response.result else {
                return executor_result_error(response.result);
            };
            let case_refs = cases
                .iter()
                .map(|case| (&case.case_id, &case.compiled))
                .collect::<Vec<_>>();
            let solution = build_route_scenario_solution(
                &case_refs,
                &raw,
                solution_context(
                    state,
                    &common,
                    &owner,
                    resource.record.problem_uri.clone(),
                    executor_started_at,
                    queue_seconds,
                ),
            )?;
            update_publishing(state, task_id).await;
            solution_result(
                state,
                &common.artifact_write_capability,
                task_id,
                &owner,
                &prepared,
                solution,
                RequestedArtifacts::Routing(&input.output),
            )
            .await
        }
        OptimizationTaskRequest::SolveConvex { common, input } => {
            let prepared = state.problem_store.load(&common.prepared).await?;
            let PreparedProblem::Convex {
                resource,
                problem,
                compiled,
            } = &prepared
            else {
                anyhow::bail!("prepared problem is not convex");
            };
            let executor_permit = acquire_executor_slot(state, task_id, &cancellation).await?;
            let queue_seconds = executor_permit.queue_seconds;
            update_solving(state, task_id, &common).await;
            let profile = convex_executor_profile(&input.policy, problem.kind)?;
            let executor_started_at = Utc::now();
            let response = state
                .executor
                .execute(
                    &ExecutorRequest::new(
                        common.run_id.clone(),
                        profile,
                        ExecutorOperation::SolveModel {
                            family: ExecutorModelFamily::Convex,
                            model: compiled.clone(),
                        },
                    ),
                    cancellation,
                )
                .await?;
            drop(executor_permit);
            let ExecutorResult::Model { solution: raw } = response.result else {
                return executor_result_error(response.result);
            };
            let solution = build_convex_solution(
                problem,
                &raw,
                solution_context(
                    state,
                    &common,
                    &owner,
                    resource.record.problem_uri.clone(),
                    executor_started_at,
                    queue_seconds,
                ),
            )?;
            update_publishing(state, task_id).await;
            solution_result(
                state,
                &common.artifact_write_capability,
                task_id,
                &owner,
                &prepared,
                solution,
                RequestedArtifacts::Convex(&input.output),
            )
            .await
        }
        OptimizationTaskRequest::SolveMilp { common, input } => {
            let prepared = state.problem_store.load(&common.prepared).await?;
            let PreparedProblem::Milp {
                resource,
                problem,
                compiled,
            } = &prepared
            else {
                anyhow::bail!("prepared problem is not MILP");
            };
            let executor_permit = acquire_executor_slot(state, task_id, &cancellation).await?;
            let queue_seconds = executor_permit.queue_seconds;
            update_solving(state, task_id, &common).await;
            let profile = executor_profile(
                &input.policy,
                ProblemFamily::Milp,
                input.output.retain_incumbents,
            )?;
            let executor_started_at = Utc::now();
            let response = state
                .executor
                .execute(
                    &ExecutorRequest::new(
                        common.run_id.clone(),
                        profile,
                        ExecutorOperation::SolveModel {
                            family: ExecutorModelFamily::Milp,
                            model: compiled.clone(),
                        },
                    ),
                    cancellation,
                )
                .await?;
            drop(executor_permit);
            let ExecutorResult::Model { solution: raw } = response.result else {
                return executor_result_error(response.result);
            };
            let solution = build_milp_solution(
                problem,
                &raw,
                solution_context(
                    state,
                    &common,
                    &owner,
                    resource.record.problem_uri.clone(),
                    executor_started_at,
                    queue_seconds,
                ),
            )?;
            update_publishing(state, task_id).await;
            solution_result(
                state,
                &common.artifact_write_capability,
                task_id,
                &owner,
                &prepared,
                solution,
                RequestedArtifacts::Milp(&input.output),
            )
            .await
        }
        OptimizationTaskRequest::VerifySolution { request } => {
            let prepared = state.problem_store.load(&request.prepared).await?;
            let tolerance = VerificationTolerance::new(
                request
                    .input
                    .absolute_tolerance
                    .map_or(DEFAULT_ABSOLUTE_TOLERANCE, NonNegativeF64::get),
                request
                    .input
                    .relative_tolerance
                    .map_or(DEFAULT_RELATIVE_TOLERANCE, NonNegativeF64::get),
            );
            let report = reverify_solution(&prepared, &request.solution, tolerance)?;
            verification_result(
                state,
                &request.artifact_write_capability,
                &owner,
                &request.solution,
                report,
            )
            .await
        }
    }
}

fn solution_context(
    state: &AppState,
    common: &SolveTaskCommon,
    owner: &veoveo_optimization_mcp::state::TaskOwner,
    problem_uri: veoveo_optimization_mcp::domain::OptimizationProblemUri,
    executor_started_at: chrono::DateTime<Utc>,
    queue_seconds: NonNegativeF64,
) -> SolutionContext {
    let total_before_executor = executor_started_at
        .signed_duration_since(common.submitted_at)
        .num_milliseconds()
        .max(0) as f64
        / 1_000.0;
    SolutionContext {
        run_id: common.run_id.clone(),
        problem_uri,
        engine: EngineProvenance {
            name: "NVIDIA cuOpt".to_owned(),
            version: state.executor_health.cuopt_version.clone(),
            container_digest: veoveo_optimization_mcp::domain::CUOPT_CONTAINER_DIGEST.to_owned(),
            executor_protocol: veoveo_optimization_mcp::domain::EXECUTOR_PROTOCOL_VERSION
                .to_owned(),
            gpu_name: Some(state.executor_health.gpu_name.clone()),
            gpu_uuid: Some(state.executor_health.gpu_uuid.clone()),
            compute_capability: Some(state.executor_health.compute_capability.clone()),
            solver_profile_uri: common.profile_uri.clone(),
        },
        timings: RunTimings {
            queue_seconds,
            preparation_seconds: NonNegativeF64::new(
                (total_before_executor - queue_seconds.get()).max(0.0),
            )
            .expect("preparation duration is non-negative"),
            ..Default::default()
        },
        authority: OptimizationAuthority {
            principal_id: owner.principal_id.clone(),
            work_context: Some(owner.authority.work_context.clone()),
            policy_revision: owner.authority.policy_revision.clone(),
            submitted_at: common.submitted_at,
        },
        created_at: Utc::now(),
    }
}

struct ExecutorPermit {
    _permit: tokio::sync::OwnedSemaphorePermit,
    queue_seconds: NonNegativeF64,
}

async fn acquire_executor_slot(
    state: &AppState,
    task_id: &str,
    cancellation: &tokio_util::sync::CancellationToken,
) -> anyhow::Result<ExecutorPermit> {
    update_task(
        state,
        task_id,
        TaskTransition::Running {
            message: "queued for cuOpt GPU execution".to_owned(),
            progress: 0.20,
        },
    )
    .await;
    let queued_at = Instant::now();
    let permit = tokio::select! {
        permit = state.executor_slot.clone().acquire_owned() => {
            permit.map_err(|_| anyhow::anyhow!("cuOpt executor queue is closed"))?
        }
        () = cancellation.cancelled() => {
            anyhow::bail!("cuOpt execution cancelled while queued")
        }
    };
    Ok(ExecutorPermit {
        _permit: permit,
        queue_seconds: NonNegativeF64::new(queued_at.elapsed().as_secs_f64())
            .expect("elapsed queue duration is non-negative"),
    })
}

fn reverify_solution(
    prepared: &PreparedProblem,
    solution: &OptimizationSolution,
    tolerance: VerificationTolerance,
) -> anyhow::Result<veoveo_optimization_mcp::domain::VerificationReport> {
    match (prepared, &solution.detail) {
        (
            PreparedProblem::Routing { compiled, .. },
            SolutionDetail::Routing { summaries, routes },
        ) => {
            let raw = routing_candidate(compiled, summaries.first(), routes, None)?;
            Ok(verify_routing_solution(compiled, &raw, tolerance).report)
        }
        (
            PreparedProblem::RouteScenarios { cases, .. },
            SolutionDetail::Routing { summaries, routes },
        ) => {
            let mut reports = Vec::new();
            for case in cases {
                let summary = summaries
                    .iter()
                    .find(|summary| summary.case_id.as_ref() == Some(&case.case_id));
                let raw = routing_candidate(&case.compiled, summary, routes, Some(&case.case_id))?;
                reports.push(verify_routing_solution(&case.compiled, &raw, tolerance).report);
            }
            Ok(merge_verification_reports(reports, tolerance))
        }
        (
            PreparedProblem::Convex { problem, .. },
            SolutionDetail::Convex {
                quality, variables, ..
            },
        ) => Ok(
            verify_convex_candidate(problem, variables, quality.primal_objective, tolerance).report,
        ),
        (
            PreparedProblem::Milp { problem, .. },
            SolutionDetail::Milp {
                quality, variables, ..
            },
        ) => Ok(
            verify_milp_candidate(problem, variables, quality.primal_objective, tolerance).report,
        ),
        _ => anyhow::bail!("solution family does not match its prepared problem"),
    }
}

fn routing_candidate(
    problem: &veoveo_optimization_mcp::executor::CompiledRoutingProblem,
    summary: Option<&veoveo_optimization_mcp::domain::RouteSolutionSummary>,
    routes: &[veoveo_optimization_mcp::domain::VehicleRoute],
    case_id: Option<&veoveo_optimization_mcp::domain::RouteCaseId>,
) -> anyhow::Result<ExecutorRoutingSolution> {
    let vehicle_indices = problem
        .vehicles
        .iter()
        .enumerate()
        .map(|(index, vehicle)| (vehicle.vehicle_id.clone(), index as u32))
        .collect::<BTreeMap<_, _>>();
    let raw_routes = routes
        .iter()
        .filter(|route| route.case_id.as_ref() == case_id)
        .map(|route| {
            let vehicle = vehicle_indices
                .get(&route.vehicle_id)
                .copied()
                .ok_or_else(|| {
                    anyhow::anyhow!("solution references unknown vehicle {}", route.vehicle_id)
                })?;
            let nodes = route
                .stops
                .iter()
                .map(|stop| {
                    let node = match stop.node_kind {
                        veoveo_optimization_mcp::domain::RouteNodeKind::Depot => {
                            ExecutorRouteNode::Depot {
                                location: location_index(problem, &stop.location_id)?,
                            }
                        }
                        veoveo_optimization_mcp::domain::RouteNodeKind::Break => {
                            ExecutorRouteNode::Break {
                                location: location_index(problem, &stop.location_id)?,
                            }
                        }
                        kind => {
                            let order_id = stop.order_id.as_ref().ok_or_else(|| {
                                anyhow::anyhow!("solution order stop omits order id")
                            })?;
                            let node = problem
                                .nodes
                                .iter()
                                .position(|candidate| {
                                    &candidate.order_id == order_id && candidate.kind == kind
                                })
                                .or_else(|| {
                                    (kind
                                        == veoveo_optimization_mcp::domain::RouteNodeKind::Service)
                                        .then(|| {
                                            problem.nodes.iter().position(|candidate| {
                                                &candidate.order_id == order_id
                                            })
                                        })?
                                })
                                .ok_or_else(|| {
                                    anyhow::anyhow!(
                                        "solution references unknown order stop {order_id}"
                                    )
                                })?;
                            ExecutorRouteNode::Order { node: node as u32 }
                        }
                    };
                    Ok(ExecutorRouteVisit {
                        node,
                        arrival: stop.arrival,
                    })
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
            Ok(ExecutorVehicleRoute { vehicle, nodes })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(ExecutorRoutingSolution {
        status: ExecutorRoutingStatus::Success,
        message: "reconstructed for independent verification".to_owned(),
        objective: summary.map_or_else(
            || veoveo_optimization_mcp::domain::FiniteF64::default(),
            |summary| summary.objective,
        ),
        objective_components: summary
            .map(|summary| summary.objective_components.clone())
            .unwrap_or_default(),
        vehicles_used: raw_routes.len() as u32,
        routes: raw_routes,
        undeliverable_nodes: Vec::new(),
        solve_seconds: NonNegativeF64::default(),
    })
}

fn location_index(
    problem: &veoveo_optimization_mcp::executor::CompiledRoutingProblem,
    location: &veoveo_optimization_mcp::domain::LocationId,
) -> anyhow::Result<u32> {
    problem
        .location_ids
        .iter()
        .position(|candidate| candidate == location)
        .map(|index| index as u32)
        .ok_or_else(|| anyhow::anyhow!("solution references unknown location {location}"))
}

fn merge_verification_reports(
    reports: Vec<veoveo_optimization_mcp::domain::VerificationReport>,
    tolerance: VerificationTolerance,
) -> veoveo_optimization_mcp::domain::VerificationReport {
    let mut merged = veoveo_optimization_mcp::verification::empty_report(tolerance);
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
    }
    merged
}

fn maximum(left: Option<NonNegativeF64>, right: Option<NonNegativeF64>) -> Option<NonNegativeF64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(if left > right { left } else { right }),
        (left, right) => left.or(right),
    }
}

async fn update_solving(state: &AppState, task_id: &str, common: &SolveTaskCommon) {
    update_task(
        state,
        task_id,
        TaskTransition::Running {
            message: format!("solving {:?} with cuOpt", common.family),
            progress: 0.25,
        },
    )
    .await;
}

async fn update_publishing(state: &AppState, task_id: &str) {
    update_task(
        state,
        task_id,
        TaskTransition::Running {
            message: "verifying and publishing immutable solution".to_owned(),
            progress: 0.85,
        },
    )
    .await;
}

fn executor_result_error<T>(result: ExecutorResult) -> anyhow::Result<T> {
    match result {
        ExecutorResult::Error { error } => {
            anyhow::bail!("cuOpt {:?}: {}", error.code, error.message)
        }
        other => anyhow::bail!("cuOpt returned unexpected result {other:?}"),
    }
}

fn common(
    prepared: &PreparedProblem,
    prepared_ref: PreparedProblemRef,
    profile_uri: veoveo_optimization_mcp::domain::OptimizationProfileUri,
    _task_id: TaskId,
    submitted_at: chrono::DateTime<Utc>,
    artifact_write_capability: veoveo_mcp_contract::IssuedArtifactWriteCapability,
) -> Result<SolveTaskCommon, AdapterError> {
    Ok(SolveTaskCommon {
        problem_id: prepared.resource().record.problem_id.clone(),
        run_id: veoveo_optimization_mcp::domain::RunId::new(),
        family: prepared.resource().record.family,
        profile_uri,
        submitted_at,
        prepared: prepared_ref,
        artifact_write_capability,
    })
}

async fn issue_output_capability(
    state: &AppState,
    caller: &PlaneCaller,
    task_id: &TaskId,
    count: u32,
) -> anyhow::Result<veoveo_mcp_contract::IssuedArtifactWriteCapability> {
    state
        .artifacts
        .issue_write_capability(
            caller,
            &IssueArtifactWriteCapabilityRequest {
                task_id: task_id.to_string(),
                expires_at: Utc::now() + ARTIFACT_CAPABILITY_TTL,
                max_artifact_count: NonZeroU32::new(count)
                    .ok_or_else(|| anyhow::anyhow!("artifact count must be positive"))?,
                max_total_bytes: NonZeroU64::new(state.max_artifact_bytes)
                    .ok_or_else(|| anyhow::anyhow!("artifact byte limit must be positive"))?,
            },
        )
        .await
}

async fn find_prepared_ref(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    problem_uri: &str,
) -> anyhow::Result<PreparedProblemRef> {
    let caller_owner = runtime_owner(identity);
    for snapshot in state.tasks.list().await? {
        if !snapshot.owner.allows(
            &caller_owner.principal_key,
            &caller_owner.profile,
            caller_owner.tenant_key.as_deref(),
            &caller_owner.data_labels,
        ) || snapshot.owner.authority.work_context != caller_owner.authority.work_context
        {
            continue;
        }
        let Ok(request) =
            serde_json::from_value::<OptimizationTaskRequest>(snapshot.request.clone())
        else {
            continue;
        };
        let Some(common) = request.common() else {
            continue;
        };
        if veoveo_optimization_mcp::uris::problem_uri(&common.problem_id) == problem_uri {
            return Ok(common.prepared.clone());
        }
    }
    anyhow::bail!("prepared problem reference is unavailable")
}

async fn fail_task(state: &AppState, task_id: &str, code: &str, error: impl std::fmt::Display) {
    tracing::warn!(task_id, "Optimization task failed: {error}");
    update_task(
        state,
        task_id,
        TaskTransition::Failed(TaskFailure::new(code, error.to_string())),
    )
    .await;
}

fn decode<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> Result<T, AdapterError> {
    serde_json::from_value(value).map_err(|error| AdapterError::invalid_params(error.to_string()))
}

fn invalid(error: impl std::fmt::Display) -> AdapterError {
    AdapterError::invalid_params(error.to_string())
}

fn internal(error: impl std::fmt::Display) -> AdapterError {
    AdapterError::internal(error.to_string())
}
