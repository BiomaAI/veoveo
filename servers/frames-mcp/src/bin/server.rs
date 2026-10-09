//! Frames MCP server.
//!
//! MCP surface:
//!   tools for complete world publication and bounded coordinate conversion
//!   task-required `batch_transform`
//!   templates for frames, CRS metadata, operations, artifacts, and usage

use std::{
    collections::BTreeSet,
    net::SocketAddr,
    num::{NonZeroU32, NonZeroU64},
    sync::{Arc, LazyLock},
    time::Duration,
};
use veoveo_types::TaskTypeDefinition;

use chrono::{DateTime, TimeDelta, Utc};
use clap::Parser;
use rmcp::tool;
use rmcp::{
    ErrorData as McpError, RoleServer,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, CompleteRequestParams, CompleteResult, ContentBlock,
        GetPromptRequestParams, GetPromptResult, Prompt, ReadResourceRequestParams, Tool,
    },
    service::RequestContext,
    tool_router,
};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use veoveo_artifact_contract::{
    IssueArtifactWriteCapabilityRequest, IssuedArtifactWriteCapability,
};
use veoveo_frames_mcp::contract::{CoordinateOperationId, CoordinateSpace};
use veoveo_frames_mcp::{
    artifacts::ArtifactRepository,
    contract::{
        BatchTransformOutput, BatchTransformRequest, ConvertFrameOutput, ConvertFrameRequest,
        CreateWorldOutput, CreateWorldRequest, PublishWorldOutput, PublishWorldRequest,
    },
    engine,
    state::{FrameScope, FramesState},
    uris,
};
use veoveo_mcp_contract::{
    GatewayInternalTrustBundle, TelemetryGuard,
    hosting::{
        DomainAddress, DomainRead, DomainServer, Hosted, HostedServer, gateway_identity,
        structured_result, unknown_prompt,
    },
    init_server_telemetry,
    server_contract::McpServerSetup,
};
use veoveo_task_runtime::{
    CreateTask as DurableCreateTask, DurableTasks, RecoveryClass, TaskFailure,
    TaskRecoveryObserver, TaskRetentionPin, TaskRuntime, TaskSnapshot, TaskTransition,
};
use veoveo_types::TaskId;

#[path = "server/app_state.rs"]
mod app_state;
#[path = "server/completion.rs"]
mod completion;
#[path = "server/config.rs"]
mod config;
#[path = "server/outputs.rs"]
mod outputs;
#[path = "server/ownership.rs"]
mod ownership;
#[path = "server/prompts.rs"]
mod prompts;
#[path = "server/resources.rs"]
mod resources;
#[path = "server/setup.rs"]
mod setup;
#[path = "server/subscriptions.rs"]
mod subscriptions;
#[path = "server/task_extension.rs"]
mod task_extension;

use app_state::{AppState, update_task};
use config::Cli;
use ownership::{frame_scope_from_identity, frame_scope_from_runtime, runtime_owner};
use prompts::FramesPrompt;

async fn fail_task(state: &AppState, task_id: TaskId, message: String) {
    tracing::warn!(%task_id, "Frames task failed: {message}");
    complete_tool_error(state, task_id, message).await;
}

#[cfg(test)]
use setup::SERVER_DOCS;
use setup::{FramesContract, SERVER_SETUP};
use subscriptions::FramesSubscriptions;
use task_extension::FramesTaskService;

const MCP_TASK_POLL_INTERVAL_MS: u64 = 3_000;
const MCP_TASK_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
const TASK_LEASE_DURATION: Duration = Duration::from_secs(120);
const TASK_LEASE_HEARTBEAT: Duration = Duration::from_secs(40);
const ARTIFACT_CAPABILITY_TTL: TimeDelta = TimeDelta::hours(24);
const SERVER_SLUG: &str = "frames";
const BATCH_ARTIFACT_MIME: &str = "application/json";

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[derive(Clone)]
struct FramesMcp {
    state: Arc<AppState>,
    tool_router: ToolRouter<FramesMcp>,
}

#[tool_router]
impl FramesMcp {
    fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        title = "Convert coordinate frame",
        description = "Convert WGS84, ECEF, ENU, and NED coordinates using registered frame definitions.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ConvertFrameOutput>(),
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn convert_frame(
        &self,
        Parameters(args): Parameters<ConvertFrameRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = gateway_identity(&context)?;
        let scope = frame_scope_from_identity(&self.state, &identity).await?;
        let worlds = resolve_worlds(&self.state, &scope, &args).await?;
        let output = engine::convert_frame(args, &worlds).map_err(invalid_params)?;
        record_direct_operation(&self.state, &identity, &output.provenance).await?;
        structured_result(
            format!("converted {} point(s)", output.points.len()),
            &output,
        )
    }

    #[tool(
        title = "Create frame world",
        description = "Create an empty frame world. Add its frames with `publish_world`.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<CreateWorldOutput>(),
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn create_world(
        &self,
        Parameters(args): Parameters<CreateWorldRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = gateway_identity(&context)?;
        let scope = frame_scope_from_identity(&self.state, &identity).await?;
        let world = self
            .state
            .frames
            .create_world(&scope, args)
            .await
            .map_err(invalid_params)?;
        let output = CreateWorldOutput { world };
        structured_result(
            format!("created frame world {}", output.world.world_id()),
            &output,
        )
    }

    #[tool(
        title = "Publish frame world",
        description = "Check a complete frame tree with a single root and publish it as a new world revision.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<PublishWorldOutput>(),
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn publish_world(
        &self,
        Parameters(args): Parameters<PublishWorldRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = gateway_identity(&context)?;
        let scope = frame_scope_from_identity(&self.state, &identity).await?;
        let output = self
            .state
            .frames
            .publish_world(&scope, args)
            .await
            .map_err(invalid_params)?;
        let message = if output.created {
            format!(
                "published frame world revision {}",
                output.revision.revision_uri()
            )
        } else {
            format!("frame world already at {}", output.revision.revision_uri())
        };
        structured_result(message, &output)
    }

    #[tool(
        title = "Batch transform",
        description = "Convert a batch of coordinates between frames and optionally save the JSON output as an artifact. Run as an MCP Task.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<veoveo_frames_mcp::contract::BatchTransformTaskOutput>(),
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn batch_transform(
        &self,
        Parameters(_args): Parameters<BatchTransformRequest>,
    ) -> Result<CallToolResult, McpError> {
        Err(McpError::invalid_request(
            "`batch_transform` must be called as an MCP Task. Resend the call with task parameters.",
            None,
        ))
    }
}

fn invalid_params(err: impl std::fmt::Display) -> McpError {
    McpError::invalid_params(err.to_string(), None)
}

async fn record_direct_operation(
    state: &AppState,
    identity: &veoveo_mcp_contract::GatewayInternalIdentity,
    provenance: &veoveo_frames_mcp::contract::CoordinateOperationProvenance,
) -> Result<(), McpError> {
    state
        .frames
        .record_operation(
            &ownership::operation_scope_from_identity(identity),
            None,
            provenance,
        )
        .await
        .map_err(|error| McpError::internal_error(error.to_string(), None))
}

async fn resolve_worlds(
    state: &AppState,
    scope: &FrameScope,
    request: &ConvertFrameRequest,
) -> Result<engine::ResolvedWorlds, McpError> {
    let mut frame_uris = BTreeSet::new();
    for point in &request.points {
        if let veoveo_frames_mcp::contract::CoordinatePoint::WorldFrame(point) = point {
            frame_uris.insert(point.frame_uri.clone());
        }
    }
    if let CoordinateSpace::WorldFrame { frame_uri } = &request.target {
        frame_uris.insert(frame_uri.clone());
    }
    let mut revisions = BTreeSet::new();
    for frame_uri in frame_uris {
        revisions.insert(frame_uri.revision_uri());
    }
    let mut resolved = engine::ResolvedWorlds::default();
    for revision_uri in revisions {
        let revision = state
            .frames
            .require_revision(scope, &revision_uri)
            .await
            .map_err(invalid_params)?;
        resolved.insert(revision).map_err(invalid_params)?;
    }
    Ok(resolved)
}

impl DomainServer for FramesMcp {
    type Contract = FramesContract;

    fn setup() -> &'static McpServerSetup<FramesContract> {
        &SERVER_SETUP
    }

    fn tool_router(&self) -> &ToolRouter<Self> {
        &self.tool_router
    }

    fn describe_tool(&self, tool: Tool) -> Tool {
        veoveo_mcp_apps_extension::link_tool_to_app(
            tool,
            uris::WORKSPACE_APP_URI,
            &[
                veoveo_mcp_apps_extension::UiVisibility::Model,
                veoveo_mcp_apps_extension::UiVisibility::App,
            ],
        )
    }

    async fn read(
        &self,
        address: DomainAddress<FramesContract>,
        request: &ReadResourceRequestParams,
        context: &RequestContext<RoleServer>,
    ) -> Result<DomainRead, McpError> {
        self.read_frames_resource(address, &request.uri, context)
            .await
            .map(DomainRead::private)
    }

    fn prompts(&self) -> Vec<Prompt> {
        FramesPrompt::ALL
            .into_iter()
            .map(FramesPrompt::prompt)
            .collect()
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        FramesPrompt::by_name(&request.name)
            .ok_or_else(|| unknown_prompt(&request.name))?
            .render(request.arguments)
    }

    async fn complete(
        &self,
        request: CompleteRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, McpError> {
        self.complete_frames(request, context).await
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct BatchTaskRequest {
    args: BatchTransformRequest,
    operation_id: CoordinateOperationId,
    operation_created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    artifact_write_capability: Option<IssuedArtifactWriteCapability>,
}

fn stamp_batch_provenance(
    result: &mut ConvertFrameOutput,
    operation_id: CoordinateOperationId,
    created_at: DateTime<Utc>,
) {
    let previous = &result.provenance.operation;
    result.provenance.operation =
        veoveo_frames_mcp::contract::CoordinateOperationRef::new(operation_id, created_at)
            .with_frames(previous.source_frame.clone(), previous.target_frame.clone());
}

async fn start_batch_task(
    state: Arc<AppState>,
    identity: veoveo_mcp_contract::GatewayInternalIdentity,
    caller: veoveo_mcp_contract::PlaneCaller,
    args: BatchTransformRequest,
    retention_pins: BTreeSet<TaskRetentionPin>,
) -> Result<TaskSnapshot, String> {
    let task_id = TaskId::new();
    let artifact_write_capability = if args.artifact {
        Some(
            state
                .artifacts
                .issue_write_capability(
                    &caller,
                    &IssueArtifactWriteCapabilityRequest {
                        required_data_labels: Default::default(),
                        task_id: veoveo_artifact_contract::ArtifactTaskId::try_from(
                            task_id.as_uuid(),
                        )
                        .map_err(|error| error.to_string())?,
                        expires_at: Utc::now() + ARTIFACT_CAPABILITY_TTL,
                        max_artifact_count: NonZeroU32::new(1).expect("one artifact is non-zero"),
                        max_total_bytes: NonZeroU64::new(state.max_artifact_bytes)
                            .ok_or_else(|| "max artifact bytes must be non-zero".to_owned())?,
                    },
                )
                .await
                .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };
    let request = BatchTaskRequest {
        args,
        operation_id: CoordinateOperationId::parse(format!("op-{}", uuid::Uuid::now_v7()))
            .map_err(|error| error.to_string())?,
        operation_created_at: Utc::now(),
        artifact_write_capability,
    };
    let created = state
        .tasks
        .create(DurableCreateTask {
            task_id,
            owner: runtime_owner(&identity),
            server: SERVER_SLUG.to_owned(),
            task_type: veoveo_frames_mcp::contract::FramesTaskKind::BatchTransform.name(),
            request: serde_json::to_value(&request).map_err(|error| error.to_string())?,
            recovery_class: RecoveryClass::Resume,
            idempotency_key: None,
            ttl_ms: Some(MCP_TASK_TTL_MS),
            poll_interval_ms: Some(MCP_TASK_POLL_INTERVAL_MS),
            retention_pins,
        })
        .await
        .map_err(|error| error.to_string())?;
    schedule_batch_task(state, created.snapshot, request)
        .await
        .map_err(|error| error.to_string())
}

async fn schedule_batch_task(
    state: Arc<AppState>,
    snapshot: TaskSnapshot,
    request: BatchTaskRequest,
) -> anyhow::Result<TaskSnapshot> {
    let task_id = snapshot.task_id;
    let claimed = state.tasks.claim(task_id, TASK_LEASE_DURATION).await?;
    let owner = snapshot.owner.clone();
    let cancellation = CancellationToken::new();
    let join = tokio::spawn(run_task(
        state.clone(),
        task_id,
        owner,
        request,
        cancellation.clone(),
    ));
    state
        .tasks
        .register_worker(task_id, cancellation, join)
        .await?;
    Ok(claimed.snapshot)
}

async fn resume_batch_task(state: Arc<AppState>, snapshot: TaskSnapshot) -> anyhow::Result<()> {
    let request: BatchTaskRequest = serde_json::from_value(snapshot.request.clone())?;
    schedule_batch_task(state, snapshot, request)
        .await
        .map(|_| ())
}

async fn complete_tool_error(state: &AppState, task_id: TaskId, message: String) {
    let result = CallToolResult::error(vec![ContentBlock::text(message.clone())]);
    let transition = match veoveo_task_runtime::mcp_task_completion(message, result) {
        Ok(transition) => transition,
        Err(error) => TaskTransition::Failed(TaskFailure::new(
            "result_serialization_failed",
            error.to_string(),
        )),
    };
    update_task(state, task_id, transition).await;
}

async fn run_task(
    state: Arc<AppState>,
    task_id: TaskId,
    owner: veoveo_task_runtime::TaskOwner,
    request: BatchTaskRequest,
    cancellation: CancellationToken,
) {
    let work = run_task_inner(state.clone(), task_id, owner, request, cancellation.clone());
    tokio::pin!(work);
    let mut heartbeat = tokio::time::interval(TASK_LEASE_HEARTBEAT);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    heartbeat.tick().await;
    loop {
        tokio::select! {
            () = &mut work => break,
            _ = heartbeat.tick() => {
                if let Err(error) = state.tasks.renew_lease(task_id, TASK_LEASE_DURATION).await {
                    tracing::warn!(%task_id, "task lease heartbeat failed: {error}");
                    cancellation.cancel();
                    break;
                }
            }
        }
    }
}

async fn run_task_inner(
    state: Arc<AppState>,
    task_id: TaskId,
    owner: veoveo_task_runtime::TaskOwner,
    request: BatchTaskRequest,
    cancellation: CancellationToken,
) {
    match app_state::start_work(&state.tasks, task_id).await {
        Ok(true) => {}
        Ok(false) => return,
        Err(error) => {
            tracing::warn!(%task_id, "Frames initial Task checkpoint failed: {error}");
            return;
        }
    }
    let scope = match frame_scope_from_runtime(&state, &owner).await {
        Ok(scope) => scope,
        Err(error) => {
            fail_task(
                &state,
                task_id,
                format!("coordinate identity failed: {error}"),
            )
            .await;
            return;
        }
    };
    let worlds = match resolve_worlds(&state, &scope, &request.args.convert).await {
        Ok(worlds) => worlds,
        Err(error) => {
            fail_task(&state, task_id, format!("world resolution failed: {error}")).await;
            return;
        }
    };
    let convert_args = request.args.convert.clone();
    let mut converted =
        match tokio::task::spawn_blocking(move || engine::convert_frame(convert_args, &worlds))
            .await
        {
            Ok(Ok(output)) => output,
            Ok(Err(error)) => {
                fail_task(&state, task_id, format!("batch transform failed: {error}")).await;
                return;
            }
            Err(error) => {
                fail_task(&state, task_id, format!("batch worker failed: {error}")).await;
                return;
            }
        };
    if cancellation.is_cancelled() {
        update_task(&state, task_id, TaskTransition::Cancelled).await;
        return;
    }
    stamp_batch_provenance(
        &mut converted,
        request.operation_id,
        request.operation_created_at,
    );
    let operation_scope = match ownership::operation_scope_from_runtime(&owner) {
        Ok(scope) => scope,
        Err(error) => {
            fail_task(
                &state,
                task_id,
                format!("operation authority failed: {error}"),
            )
            .await;
            return;
        }
    };
    if let Err(error) = state
        .frames
        .record_operation(&operation_scope, Some(task_id), &converted.provenance)
        .await
    {
        fail_task(
            &state,
            task_id,
            format!("operation provenance write failed: {error}"),
        )
        .await;
        return;
    }
    let output = BatchTransformOutput {
        result: converted,
        artifact: None,
    };
    let result = match outputs::batch_result(
        &state,
        request.artifact_write_capability.as_ref(),
        task_id,
        &owner,
        output,
        request.args.artifact,
    )
    .await
    {
        Ok(result) => result,
        Err(error) => {
            fail_task(&state, task_id, format!("batch output failed: {error}")).await;
            return;
        }
    };
    if cancellation.is_cancelled() {
        update_task(&state, task_id, TaskTransition::Cancelled).await;
        return;
    }
    let transition = match veoveo_task_runtime::mcp_task_completion(
        "batch coordinate transform completed",
        result,
    ) {
        Ok(transition) => transition,
        Err(error) => {
            fail_task(
                &state,
                task_id,
                format!("serializing batch result failed: {error}"),
            )
            .await;
            return;
        }
    };
    app_state::publish_task(&state, task_id, transition, &cancellation).await;
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    LazyLock::force(&SERVER_SETUP);
    install_rustls_provider();
    let _ = dotenvy::dotenv();
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-frames-mcp", "info,veoveo_frames_mcp=debug")?;
    let args = match Cli::parse() {
        Cli::Serve(args) => *args,
    };
    let public_deployment = args.public_deployment()?;
    let store_config = veoveo_platform_store::StoreConfig::builder(
        args.surreal_endpoint.clone(),
        args.surreal_namespace.clone(),
        args.surreal_database.clone(),
        veoveo_platform_store::StoreCredentials::new(
            args.surreal_auth_level,
            args.surreal_username.clone(),
            args.surreal_password.clone(),
        ),
    )
    .build()?;
    let startup = veoveo_frames_mcp::startup::FramesStartup::from_env(&store_config)?;
    let store = veoveo_platform_store::PlatformStore::connect(store_config).await?;
    startup.require(&store).await?;
    let tasks = TaskRuntime::new(
        store,
        SERVER_SLUG,
        format!("{SERVER_SLUG}-{}", uuid::Uuid::now_v7()),
    );
    let recovery = tasks.observe_startup_recovery().await?;
    let frames = FramesState::new(tasks.platform_store().clone());
    let state = Arc::new(AppState {
        tasks,
        frames,
        artifacts: ArtifactRepository::new(args.artifact_service_url.clone()),
        max_artifact_bytes: args.max_artifact_bytes,
        subscriptions: veoveo_mcp_contract::SubscriptionHub::new(),
    });
    let recovery_state = state.clone();
    let recovery_observer = TaskRecoveryObserver::start(recovery, move |report| {
        let state = recovery_state.clone();
        async move {
            for snapshot in report.resumable {
                let admitted = snapshot.clone();
                if let Err(error) = resume_batch_task(state.clone(), snapshot).await {
                    state
                        .tasks
                        .reconcile_recovery_claim(&admitted, error)
                        .await?;
                }
            }
            Ok(())
        }
    })
    .await?;

    let readiness_store = state.tasks.platform_store().clone();
    let server = HostedServer::for_domain::<FramesMcp>()
        .deployment(&public_deployment, args.allow_loopback_hosts)?
        .allowed_hosts(args.allowed_hosts.iter().cloned())
        .readiness(move || {
            let store = readiness_store.clone();
            async move {
                matches!(
                    tokio::time::timeout(std::time::Duration::from_secs(5), store.healthcheck())
                        .await,
                    Ok(Ok(()))
                )
            }
        })
        .internal_trust(GatewayInternalTrustBundle::from_json(
            &args.internal_trust_jwks,
        )?)?
        .handler({
            let state = state.clone();
            move || {
                Hosted::new(FramesMcp::new(state.clone())).with_tasks(DurableTasks::with_resources(
                    FramesTaskService::new(state.clone()),
                    FramesSubscriptions::new(state.clone()),
                ))
            }
        })
        .build();
    let _resource_observer = subscriptions::spawn_observer(state, server.cancellation_token());
    recovery_observer
        .serve(server.serve(SocketAddr::from(([0, 0, 0, 0], args.port))))
        .await
}

#[cfg(test)]
mod well_known_tests {
    use veoveo_mcp_contract::docs::{
        CONTRACT_REVISION, ComplianceStatus, DOC_ID_AGENTS, DOC_ID_DESIGN,
    };

    use super::SERVER_DOCS;

    #[test]
    fn embedded_documents_carry_the_crate_manual_and_design() {
        assert_eq!(SERVER_DOCS.server(), "frames");
        let agents = SERVER_DOCS.doc(DOC_ID_AGENTS).expect("agents document");
        assert!(agents.body.contains("## Contract Compliance"));
        let design = SERVER_DOCS.doc(DOC_ID_DESIGN).expect("design document");
        assert!(!design.body.is_empty());
        let index = SERVER_DOCS.llms_txt();
        assert!(index.contains("(agents)"));
        assert!(index.contains("(design)"));
    }

    #[test]
    fn contract_declaration_resolves_from_the_embedded_manual() {
        let declaration = veoveo_mcp_contract::docs::ContractDeclaration::from_docs(&SERVER_DOCS);
        assert_eq!(declaration.server().as_str(), "frames");
        assert_eq!(declaration.contract_revision(), CONTRACT_REVISION);
        for id in ["C18", "C19", "C20", "C21"] {
            let item = declaration
                .compliance()
                .iter()
                .find(|item| item.id.as_str() == id)
                .expect("declared checklist item");
            assert_eq!(item.status, ComplianceStatus::Met, "{id} must be met");
        }
        for item in declaration.compliance() {
            if item.status == ComplianceStatus::Pending {
                assert!(item.note.is_some(), "pending items must state a reason");
            }
        }
    }

    #[test]
    fn contract_declaration_defers_runtime_surface_to_discover() {
        let declaration = veoveo_mcp_contract::docs::ContractDeclaration::from_docs(&SERVER_DOCS);
        let json = serde_json::to_value(declaration).unwrap();
        assert!(json.get("capabilities").is_none());
    }
}

#[cfg(test)]
mod task_tests {
    use super::*;

    #[test]
    fn tool_input_schemas_use_the_canonical_profile() {
        assert!(!FramesMcp::tool_router().list_all().is_empty());
    }
    #[test]
    fn resumed_batch_output_is_byte_deterministic() {
        let request = ConvertFrameRequest {
            target: CoordinateSpace::EcefWgs84,
            points: vec![veoveo_frames_mcp::contract::CoordinatePoint::Wgs84(
                veoveo_frames_mcp::contract::Wgs84Position {
                    latitude_degrees: 37.421_999_9,
                    longitude_degrees: -122.084_057_5,
                    ellipsoid_height_m: 10.0,
                },
            )],
            allow_approximation: false,
        };
        let mut first =
            engine::convert_frame(request.clone(), &engine::ResolvedWorlds::default()).unwrap();
        let mut replay =
            engine::convert_frame(request, &engine::ResolvedWorlds::default()).unwrap();
        assert_ne!(
            first.provenance.operation.operation_id(),
            replay.provenance.operation.operation_id()
        );

        let operation_id =
            CoordinateOperationId::parse(format!("op-{}", uuid::Uuid::now_v7())).unwrap();
        let created_at = Utc::now();
        stamp_batch_provenance(&mut first, operation_id.clone(), created_at);
        stamp_batch_provenance(&mut replay, operation_id, created_at);
        let first = BatchTransformOutput {
            result: first,
            artifact: None,
        };
        let replay = BatchTransformOutput {
            result: replay,
            artifact: None,
        };
        let inline = veoveo_frames_mcp::contract::BatchTransformTaskOutput::new(first.clone());
        let wire = serde_json::to_value(inline).unwrap();
        assert!(wire.get("resultUri").is_none());
        assert!(
            serde_json::from_value::<veoveo_frames_mcp::contract::BatchTransformTaskOutput>(
                wire.clone()
            )
            .is_ok()
        );
        let mut null = wire;
        null["resultUri"] = serde_json::Value::Null;
        assert!(
            serde_json::from_value::<veoveo_frames_mcp::contract::BatchTransformTaskOutput>(null)
                .is_err()
        );
        let artifact: veoveo_artifact_contract::ArtifactMetadata =
            serde_json::from_value(serde_json::json!({
                "artifactId":"01983da0-0000-7000-8000-000000000001",
                "artifactUri":"artifact://01983da0-0000-7000-8000-000000000001",
                "byteLen":1,"createdAt":"2026-09-29T00:00:00Z"
            }))
            .unwrap();
        let mut published = first.clone();
        published.artifact = Some(artifact);
        let wire = serde_json::to_value(
            veoveo_frames_mcp::contract::BatchTransformTaskOutput::new(published),
        )
        .unwrap();
        assert!(
            serde_json::from_value::<veoveo_frames_mcp::contract::BatchTransformTaskOutput>(
                wire.clone()
            )
            .is_ok()
        );
        for invalid in [
            None,
            Some(serde_json::Value::Null),
            Some(serde_json::json!(
                "artifact://01983da0-0000-7000-8000-000000000099"
            )),
        ] {
            let mut value = wire.clone();
            match invalid {
                Some(uri) => {
                    value["resultUri"] = uri;
                }
                None => {
                    value.as_object_mut().unwrap().remove("resultUri");
                }
            }
            assert!(
                serde_json::from_value::<veoveo_frames_mcp::contract::BatchTransformTaskOutput>(
                    value
                )
                .is_err()
            );
        }
        assert_eq!(
            serde_json::to_vec_pretty(&first).unwrap(),
            serde_json::to_vec_pretty(&replay).unwrap()
        );
    }
}
#[cfg(test)]
#[path = "server/tool_input_tests.rs"]
mod tool_input_tests;
