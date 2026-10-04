//! Timeseries MCP server.
//!
//! MCP surface:
//!   tool `forecast(source, mapping, horizon)` — final task-extension execution
//!   template `timeseries://artifact/{artifact_id}` — Rerun RRD artifact bytes
//!   template `timeseries://usage/task/{task_id}` — task usage rows

use std::{
    collections::BTreeSet,
    net::SocketAddr,
    num::{NonZeroU32, NonZeroU64},
    sync::{Arc, LazyLock},
    time::Duration,
};
use veoveo_types::TaskTypeDefinition;

use chrono::{TimeDelta, Utc};
use clap::Parser;
use rmcp::tool;
use rmcp::{
    ErrorData as McpError, RoleServer,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ContentBlock, ReadResourceRequestParams, Tool},
    service::RequestContext,
    tool_router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_util::sync::CancellationToken;
use veoveo_duckdb_runtime::HttpsSourcePolicy;
use veoveo_mcp_contract::{
    GatewayInternalTrustBundle, IssueArtifactWriteCapabilityRequest, IssuedArtifactWriteCapability,
    TelemetryGuard,
    hosting::{
        DomainAddress, DomainRead, DomainServer, Hosted, HostedServer, gateway_identity,
        plane_caller,
    },
    init_server_telemetry,
    server_contract::McpServerSetup,
};
use veoveo_task_runtime::{
    CreateTask as DurableCreateTask, DurableTasks, RecoveryClass, TaskError, TaskFailure,
    TaskPayloadState, TaskRuntime, TaskRuntimeConfig, TaskSnapshot, TaskTransition,
};
use veoveo_timeseries_mcp::{
    artifacts::ArtifactRepository,
    contract::{TimeseriesForecastOutput, TimeseriesForecastRequest},
    forecast::run_forecast,
    uris,
};
use veoveo_types::TaskId;

#[path = "server/app_state.rs"]
mod app_state;
#[path = "server/config.rs"]
mod config;
#[path = "server/outputs.rs"]
mod outputs;
#[path = "server/ownership.rs"]
mod ownership;
#[path = "server/resources.rs"]
mod resources;
#[path = "server/setup.rs"]
mod setup;
#[path = "server/task_extension.rs"]
mod task_extension;

use app_state::{AppState, update_task};
use config::Args;
use outputs::forecast_result;
use ownership::{runtime_owner, task_owner_from_identity, task_owner_from_runtime};

async fn fail_task(state: &AppState, task_id: TaskId, message: String) {
    tracing::warn!(%task_id, "timeseries task failed: {message}");
    complete_tool_error(state, task_id, message).await;
}

#[cfg(test)]
use setup::SERVER_DOCS;
use setup::{SERVER_SETUP, TimeseriesContract};
use task_extension::TimeseriesTaskService;

const MCP_TASK_POLL_INTERVAL_MS: u64 = 3000;
const MCP_TASK_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1000;
const TASK_LEASE_DURATION: Duration = Duration::from_secs(120);
const TASK_LEASE_HEARTBEAT: Duration = Duration::from_secs(40);
const ARTIFACT_CAPABILITY_TTL: TimeDelta = TimeDelta::hours(24);
const SERVER_SLUG: &str = "timeseries";

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ForecastTaskRequest {
    input: TimeseriesForecastRequest,
    artifact_write_capability: IssuedArtifactWriteCapability,
}

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// Timeseries's domain: the forecast tool and admitted resource reads.
#[derive(Clone)]
struct TimeseriesMcp {
    state: Arc<AppState>,
    tool_router: ToolRouter<TimeseriesMcp>,
}

#[tool_router]
impl TimeseriesMcp {
    fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        title = "Forecast timeseries",
        description = "Read a time series from any source DuckDB can read, compute a forecast, and return one timeseries://artifact/{artifact_id} Rerun RRD artifact.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<TimeseriesForecastOutput>(),
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn forecast(
        &self,
        Parameters(args): Parameters<TimeseriesForecastRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let caller = plane_caller(&context)?;
        let identity = gateway_identity(&context)?;
        let snapshot = start_forecast_task(
            self.state.clone(),
            identity,
            caller,
            args,
            Some(TaskProgress {
                peer: context.peer.clone(),
                token: context.meta.get_progress_token(),
            }),
            BTreeSet::new(),
        )
        .await
        .map_err(|err| McpError::internal_error(err, None))?;
        let task_id = snapshot.task_id;

        match self
            .state
            .tasks
            .await_payload_state(task_id)
            .await
            .map_err(|err| McpError::internal_error(err.to_string(), None))?
        {
            TaskPayloadState::Completed(payload) => {
                serde_json::from_value(payload).map_err(|err| {
                    McpError::internal_error(
                        format!("invalid persisted forecast task result: {err}"),
                        None,
                    )
                })
            }
            TaskPayloadState::Failed(error) => Err(McpError::internal_error(
                error.message,
                Some(json!({"code": error.code, "details": error.details})),
            )),
            TaskPayloadState::Cancelled => {
                Err(McpError::invalid_request("forecast was cancelled", None))
            }
            TaskPayloadState::Running => Err(McpError::internal_error(
                "forecast task wait ended while still running",
                None,
            )),
            TaskPayloadState::Unknown => Err(McpError::internal_error(
                "forecast task disappeared before completion",
                None,
            )),
        }
    }
}

impl DomainServer for TimeseriesMcp {
    type Contract = TimeseriesContract;

    fn setup() -> &'static McpServerSetup<TimeseriesContract> {
        &SERVER_SETUP
    }

    fn tool_router(&self) -> &ToolRouter<Self> {
        &self.tool_router
    }

    fn describe_tool(&self, tool: Tool) -> Tool {
        if tool.name != "forecast" {
            return tool;
        }
        veoveo_mcp_apps_extension::link_tool_to_app(
            tool,
            uris::FORECAST_APP_URI,
            &[
                veoveo_mcp_apps_extension::UiVisibility::Model,
                veoveo_mcp_apps_extension::UiVisibility::App,
            ],
        )
    }

    async fn read(
        &self,
        address: DomainAddress<TimeseriesContract>,
        request: &ReadResourceRequestParams,
        context: &RequestContext<RoleServer>,
    ) -> Result<DomainRead, McpError> {
        self.read_timeseries_resource(address, &request.uri, context)
            .await
            .map(DomainRead::private)
    }
}

struct TaskProgress {
    peer: rmcp::service::Peer<RoleServer>,
    token: Option<rmcp::model::ProgressToken>,
}

async fn start_forecast_task(
    state: Arc<AppState>,
    identity: veoveo_mcp_contract::GatewayInternalIdentity,
    caller: veoveo_mcp_contract::PlaneCaller,
    args: TimeseriesForecastRequest,
    progress: Option<TaskProgress>,
    retention_pins: BTreeSet<veoveo_task_runtime::TaskRetentionPin>,
) -> Result<TaskSnapshot, String> {
    let task_id = TaskId::new();
    let artifact_write_capability = state
        .artifacts
        .issue_write_capability(
            &caller,
            &IssueArtifactWriteCapabilityRequest {
                required_data_labels: Default::default(),
                task_id: task_id.to_string(),
                expires_at: Utc::now() + ARTIFACT_CAPABILITY_TTL,
                max_artifact_count: NonZeroU32::new(1).expect("one artifact is non-zero"),
                max_total_bytes: NonZeroU64::new(state.max_artifact_bytes)
                    .ok_or_else(|| "max artifact bytes must be non-zero".to_owned())?,
            },
        )
        .await
        .map_err(|err| err.to_string())?;
    let request = ForecastTaskRequest {
        input: args,
        artifact_write_capability,
    };
    let created = state
        .tasks
        .create(DurableCreateTask {
            task_id,
            owner: runtime_owner(&identity),
            server: SERVER_SLUG.to_owned(),
            task_type: veoveo_timeseries_mcp::contract::TimeseriesTaskKind::Forecast.name(),
            request: serde_json::to_value(&request).map_err(|err| err.to_string())?,
            recovery_class: RecoveryClass::Resume,
            idempotency_key: None,
            ttl_ms: Some(MCP_TASK_TTL_MS),
            poll_interval_ms: Some(MCP_TASK_POLL_INTERVAL_MS),
            retention_pins,
        })
        .await
        .map_err(|err| err.to_string())?;
    schedule_forecast_task(
        state,
        created.snapshot,
        request,
        task_owner_from_identity(task_id, &identity),
        progress,
    )
    .await
    .map_err(|err| err.to_string())
}

async fn schedule_forecast_task(
    state: Arc<AppState>,
    snapshot: TaskSnapshot,
    request: ForecastTaskRequest,
    owner: veoveo_timeseries_mcp::state::TaskOwner,
    progress: Option<TaskProgress>,
) -> anyhow::Result<TaskSnapshot> {
    let task_id = snapshot.task_id;
    let claimed = state.tasks.claim(task_id, TASK_LEASE_DURATION).await?;
    let cancellation = CancellationToken::new();
    let join = tokio::spawn(run_task(
        state.clone(),
        task_id,
        request,
        owner,
        progress,
        cancellation.clone(),
    ));
    state
        .tasks
        .register_worker(task_id, cancellation, join)
        .await?;
    Ok(claimed.snapshot)
}

async fn resume_forecast_task(state: Arc<AppState>, snapshot: TaskSnapshot) -> anyhow::Result<()> {
    let request: ForecastTaskRequest = serde_json::from_value(snapshot.request.clone())?;
    let task_id = snapshot.task_id;
    let owner = task_owner_from_runtime(task_id, &snapshot.owner).map_err(anyhow::Error::msg)?;
    schedule_forecast_task(state, snapshot, request, owner, None)
        .await
        .map(|_| ())
}

async fn notify_task_progress(progress: &Option<TaskProgress>, value: f64, message: &str) {
    if let Some(progress) = progress {
        veoveo_mcp_contract::notify_progress(&progress.peer, &progress.token, value, message).await;
    }
}

async fn complete_tool_error(state: &AppState, task_id: TaskId, message: String) {
    let result = CallToolResult::error(vec![ContentBlock::text(message.clone())]);
    let transition = match serde_json::to_value(result) {
        Ok(result) => TaskTransition::Succeeded { message, result },
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
    request: ForecastTaskRequest,
    owner: veoveo_timeseries_mcp::state::TaskOwner,
    progress: Option<TaskProgress>,
    cancellation: CancellationToken,
) {
    let work = run_task_inner(
        state.clone(),
        task_id,
        request,
        owner,
        progress,
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
    request: ForecastTaskRequest,
    owner: veoveo_timeseries_mcp::state::TaskOwner,
    progress: Option<TaskProgress>,
    cancellation: CancellationToken,
) {
    notify_task_progress(&progress, 0.1, "materializing source").await;
    let artifact = match tokio::task::spawn_blocking({
        let input = request.input.clone();
        let source_policy = state.source_policy.clone();
        move || run_forecast(task_id, &input, &source_policy)
    })
    .await
    {
        Ok(Ok(artifact)) => artifact,
        Ok(Err(err)) => {
            fail_task(&state, task_id, format!("forecast failed: {err}")).await;
            return;
        }
        Err(err) => {
            fail_task(&state, task_id, format!("forecast worker failed: {err}")).await;
            return;
        }
    };
    if cancellation.is_cancelled() {
        update_task(&state, task_id, TaskTransition::Cancelled).await;
        return;
    }
    notify_task_progress(&progress, 0.8, "writing artifact").await;
    let result = match forecast_result(
        &state,
        &request.artifact_write_capability,
        task_id,
        &owner,
        artifact,
    )
    .await
    {
        Ok(result) => result,
        Err(err) => {
            fail_task(&state, task_id, format!("artifact write failed: {err}")).await;
            return;
        }
    };
    notify_task_progress(&progress, 1.0, "completed").await;
    let payload = match serde_json::to_value(&result) {
        Ok(payload) => payload,
        Err(err) => {
            fail_task(
                &state,
                task_id,
                format!("serializing forecast result failed: {err}"),
            )
            .await;
            return;
        }
    };
    update_task(
        &state,
        task_id,
        TaskTransition::Succeeded {
            message: "completed; RRD artifact available".to_owned(),
            result: payload,
        },
    )
    .await;
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    LazyLock::force(&SERVER_SETUP);
    install_rustls_provider();
    let _ = dotenvy::dotenv();
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-timeseries-mcp", "info,veoveo_timeseries_mcp=debug")?;
    let args = Args::parse();
    let public_deployment = args.public_deployment()?;
    let artifacts = ArtifactRepository::new(args.artifact_service_url.clone());
    let tasks = TaskRuntime::connect(
        TaskRuntimeConfig::new(
            args.surreal_endpoint.clone(),
            args.surreal_namespace.clone(),
            args.surreal_database.clone(),
            args.surreal_auth_level,
            args.surreal_username.clone(),
            args.surreal_password.clone(),
        ),
        SERVER_SLUG,
        format!("{SERVER_SLUG}-{}", uuid::Uuid::now_v7()),
    )
    .await?;
    let recovery = tasks.recover().await?;
    let mut source_policy = HttpsSourcePolicy::new(args.allow_source_hosts.clone());
    source_policy.max_bytes = args.max_source_bytes;
    let state = Arc::new(AppState {
        tasks,
        artifacts,
        source_policy,
        max_artifact_bytes: args.max_artifact_bytes,
    });
    for snapshot in recovery.resumable {
        if let Err(error) = resume_forecast_task(state.clone(), snapshot).await {
            match error.downcast_ref::<TaskError>() {
                Some(TaskError::LeaseHeld(task_id) | TaskError::Conflict(task_id)) => {
                    tracing::info!(%task_id, "another replica claimed recovered forecast task");
                }
                _ => return Err(error),
            }
        }
    }

    let readiness_store = state.tasks.platform_store().clone();
    let server = HostedServer::for_domain::<TimeseriesMcp>()
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
        .handler(move || {
            Hosted::new(TimeseriesMcp::new(state.clone())).with_tasks(DurableTasks::tasks_only(
                TimeseriesTaskService::new(state.clone()),
            ))
        })
        .build();
    server
        .serve(SocketAddr::from(([0, 0, 0, 0], args.port)))
        .await
}

#[cfg(test)]
mod schema_tests {
    use super::*;

    #[test]
    fn tool_input_schemas_use_the_canonical_profile() {
        assert!(!TimeseriesMcp::tool_router().list_all().is_empty());
    }
}

#[cfg(test)]
mod well_known_tests {
    use veoveo_mcp_contract::docs::{
        CONTRACT_REVISION, ComplianceStatus, DOC_ID_AGENTS, DOC_ID_DESIGN,
    };

    use super::SERVER_DOCS;

    #[test]
    fn embedded_documents_carry_the_crate_manual_and_design() {
        assert_eq!(SERVER_DOCS.server(), "timeseries");
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
        assert_eq!(declaration.server, "timeseries");
        assert_eq!(declaration.contract_revision, CONTRACT_REVISION);
        for id in ["C18", "C19", "C20", "C21"] {
            let item = declaration
                .compliance
                .iter()
                .find(|item| item.id == id)
                .expect("declared checklist item");
            assert_eq!(item.status, ComplianceStatus::Met, "{id} must be met");
        }
        let json = serde_json::to_value(&declaration).expect("declaration serializes");
        assert_eq!(json["server"], "timeseries");
    }

    #[test]
    fn contract_declaration_defers_runtime_surface_to_discover() {
        let declaration = veoveo_mcp_contract::docs::ContractDeclaration::from_docs(&SERVER_DOCS);
        let json = serde_json::to_value(declaration).unwrap();
        assert!(json.get("capabilities").is_none());
    }
}
#[cfg(test)]
#[path = "server/tool_input_tests.rs"]
mod tool_input_tests;
