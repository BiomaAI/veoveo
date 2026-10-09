//! Native Task identity, scheduling, recovery and operation dispatch.
use super::{
    SERVER_SLUG,
    app_state::{AppState, update_task},
    artifact_output::ArtifactWriter,
    outputs,
    ownership::{identity_from_runtime, runtime_owner},
    sql_ops,
};
use chrono::{TimeDelta, Utc};
use rmcp::model::{CallToolResult, ContentBlock};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    num::{NonZeroU32, NonZeroU64},
    sync::Arc,
    time::Duration,
};
use tokio_util::sync::CancellationToken;
use veoveo_artifact_contract::{
    IssueArtifactWriteCapabilityRequest, IssuedArtifactWriteCapability,
};
use veoveo_duckdb_mcp::contract::{
    DuckDbExecuteRequest, DuckDbExportRequest, DuckDbIngestRequest, DuckDbQueryOutput,
    DuckDbQueryRequest, DuckDbQueryUsage, DuckDbTaskKind, DuckDbUsageDetails,
};
use veoveo_task_runtime::{
    CreateTask as DurableCreateTask, RecoveryClass, TaskFailure, TaskRetentionPin, TaskSnapshot,
    TaskTransition,
};
use veoveo_types::{TaskId, TaskTypeDefinition};

const MCP_TASK_POLL_INTERVAL_MS: u64 = 3000;
const MCP_TASK_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1000;
const TASK_LEASE_DURATION: Duration = Duration::from_secs(120);
const TASK_LEASE_HEARTBEAT: Duration = Duration::from_secs(40);
const ARTIFACT_CAPABILITY_TTL: TimeDelta = TimeDelta::hours(24);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "operation",
    content = "request",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub(super) enum TaskArgs {
    Query(DuckDbQueryRequest),
    Execute(DuckDbExecuteRequest),
    Ingest(DuckDbIngestRequest),
    Export(DuckDbExportRequest),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
struct DuckdbTaskRequest {
    args: TaskArgs,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    artifact_write_capability: Option<IssuedArtifactWriteCapability>,
}

pub(super) fn parse_task_args(name: &str, arguments: Value) -> Result<TaskArgs, String> {
    let invalid = |error: serde_json::Error| format!("invalid {name} arguments: {error}");
    match DuckDbTaskKind::from_wire_name(name).ok_or("unknown DuckDB tool")? {
        DuckDbTaskKind::Query => Ok(TaskArgs::Query(
            serde_json::from_value(arguments).map_err(invalid)?,
        )),
        DuckDbTaskKind::Execute => Ok(TaskArgs::Execute(
            serde_json::from_value(arguments).map_err(invalid)?,
        )),
        DuckDbTaskKind::Ingest => Ok(TaskArgs::Ingest(
            serde_json::from_value(arguments).map_err(invalid)?,
        )),
        DuckDbTaskKind::Export => Ok(TaskArgs::Export(
            serde_json::from_value(arguments).map_err(invalid)?,
        )),
    }
}

fn task_recovery_class(args: &TaskArgs) -> RecoveryClass {
    match args {
        TaskArgs::Query(_) | TaskArgs::Export(_) => RecoveryClass::Resume,
        TaskArgs::Execute(_) | TaskArgs::Ingest(_) => RecoveryClass::InterruptedIndeterminate,
    }
}

fn task_needs_artifact_capability(args: &TaskArgs) -> bool {
    matches!(args, TaskArgs::Query(_) | TaskArgs::Export(_))
}

pub(super) async fn start_duckdb_task(
    state: Arc<AppState>,
    identity: veoveo_mcp_contract::GatewayInternalIdentity,
    caller: veoveo_mcp_contract::PlaneCaller,
    args: TaskArgs,
    retention_pins: BTreeSet<TaskRetentionPin>,
) -> Result<TaskSnapshot, String> {
    let task_id = TaskId::new();
    let artifact_write_capability = if task_needs_artifact_capability(&args) {
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
    let recovery_class = task_recovery_class(&args);
    let request = DuckdbTaskRequest {
        args,
        artifact_write_capability,
    };
    let created = state
        .tasks
        .create(DurableCreateTask {
            task_id,
            owner: runtime_owner(&identity),
            server: SERVER_SLUG.to_owned(),
            task_type: request.args.task_type(),
            request: serde_json::to_value(&request).map_err(|error| error.to_string())?,
            recovery_class,
            idempotency_key: None,
            ttl_ms: Some(MCP_TASK_TTL_MS),
            poll_interval_ms: Some(MCP_TASK_POLL_INTERVAL_MS),
            retention_pins,
        })
        .await
        .map_err(|error| error.to_string())?;
    schedule_duckdb_task(state, created.snapshot, request, identity, Some(caller))
        .await
        .map_err(|error| error.to_string())
}

impl TaskArgs {
    fn task_type(&self) -> veoveo_types::TaskTypeName {
        match self {
            Self::Query(_) => DuckDbTaskKind::Query.name(),
            Self::Execute(_) => DuckDbTaskKind::Execute.name(),
            Self::Ingest(_) => DuckDbTaskKind::Ingest.name(),
            Self::Export(_) => DuckDbTaskKind::Export.name(),
        }
    }
}

async fn schedule_duckdb_task(
    state: Arc<AppState>,
    snapshot: TaskSnapshot,
    request: DuckdbTaskRequest,
    identity: veoveo_mcp_contract::GatewayInternalIdentity,
    caller: Option<veoveo_mcp_contract::PlaneCaller>,
) -> anyhow::Result<TaskSnapshot> {
    let task_id = snapshot.task_id;
    let claimed = state.tasks.claim(task_id, TASK_LEASE_DURATION).await?;
    let cancellation = CancellationToken::new();
    let join = tokio::spawn(run_task(
        state.clone(),
        task_id,
        identity,
        request,
        caller,
        cancellation.clone(),
    ));
    state
        .tasks
        .register_worker(task_id, cancellation, join)
        .await?;
    Ok(claimed.snapshot)
}

pub(super) async fn resume_duckdb_task(
    state: Arc<AppState>,
    snapshot: TaskSnapshot,
) -> anyhow::Result<()> {
    let request: DuckdbTaskRequest = serde_json::from_value(snapshot.request.clone())?;
    if !matches!(&request.args, TaskArgs::Query(_) | TaskArgs::Export(_)) {
        anyhow::bail!("mutation task cannot be resumed");
    }
    let identity = identity_from_runtime(&snapshot.owner).map_err(anyhow::Error::msg)?;
    schedule_duckdb_task(state, snapshot, request, identity, None)
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
    identity: veoveo_mcp_contract::GatewayInternalIdentity,
    request: DuckdbTaskRequest,
    caller: Option<veoveo_mcp_contract::PlaneCaller>,
    cancellation: CancellationToken,
) {
    let work = run_task_inner(
        state.clone(),
        task_id,
        identity,
        request,
        caller,
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
    identity: veoveo_mcp_contract::GatewayInternalIdentity,
    request: DuckdbTaskRequest,
    caller: Option<veoveo_mcp_contract::PlaneCaller>,
    cancellation: CancellationToken,
) {
    if !update_task(
        &state,
        task_id,
        TaskTransition::Running {
            message: "running DuckDB operation".to_owned(),
            progress: 0.1,
        },
    )
    .await
    {
        return;
    }
    let artifact_write_capability = request.artifact_write_capability;
    let result = match request.args {
        TaskArgs::Query(request) => {
            let writer = match ArtifactWriter::for_task(
                artifact_write_capability.as_ref(),
                task_id,
                DuckDbTaskKind::Query,
                &identity,
            ) {
                Ok(writer) => writer,
                Err(error) => {
                    fail_task(&state, task_id, error).await;
                    return;
                }
            };
            match sql_ops::query_op(&state, &writer, &identity, request).await {
                Ok(output) => {
                    if let Err(error) = outputs::record_op_usage(
                        &state,
                        task_id,
                        output.row_count(),
                        DuckDbUsageDetails::Query {
                            result: query_usage(&output),
                        },
                    )
                    .await
                    {
                        fail_task(&state, task_id, format!("usage write failed: {error}")).await;
                        return;
                    }
                    outputs::query_result(&output)
                }
                Err(err) => {
                    fail_task(&state, task_id, format!("query failed: {}", err.message)).await;
                    return;
                }
            }
        }
        TaskArgs::Execute(request) => match sql_ops::execute_op(&state, &identity, request).await {
            Ok(output) => {
                if let Err(error) = outputs::record_op_usage(
                    &state,
                    task_id,
                    output.rows_changed,
                    DuckDbUsageDetails::Execute {
                        db: output.db.clone(),
                        statements: output.statements,
                    },
                )
                .await
                {
                    fail_task(&state, task_id, format!("usage write failed: {error}")).await;
                    return;
                }
                outputs::execute_result(&output)
            }
            Err(err) => {
                fail_task(&state, task_id, format!("execute failed: {}", err.message)).await;
                return;
            }
        },
        TaskArgs::Ingest(request) => {
            let caller = match caller.as_ref() {
                Some(caller) => caller,
                None => {
                    fail_task(
                        &state,
                        task_id,
                        "interrupted ingest cannot be replayed".to_owned(),
                    )
                    .await;
                    return;
                }
            };
            match sql_ops::ingest_op(&state, caller, &identity, request).await {
                Ok(output) => {
                    if let Err(error) = outputs::record_op_usage(
                        &state,
                        task_id,
                        output.rows_ingested,
                        DuckDbUsageDetails::Ingest {
                            db: output.db.clone(),
                            table: output.table.clone(),
                        },
                    )
                    .await
                    {
                        fail_task(&state, task_id, format!("usage write failed: {error}")).await;
                        return;
                    }
                    outputs::ingest_result(&output)
                }
                Err(err) => {
                    fail_task(&state, task_id, format!("ingest failed: {}", err.message)).await;
                    return;
                }
            }
        }
        TaskArgs::Export(request) => {
            let writer = match ArtifactWriter::for_task(
                artifact_write_capability.as_ref(),
                task_id,
                DuckDbTaskKind::Export,
                &identity,
            ) {
                Ok(writer) => writer,
                Err(error) => {
                    fail_task(&state, task_id, error).await;
                    return;
                }
            };
            match sql_ops::export_op(&state, &writer, &identity, request).await {
                Ok(output) => {
                    if let Err(error) = outputs::record_op_usage(
                        &state,
                        task_id,
                        output.rows_exported(),
                        DuckDbUsageDetails::Export {
                            db: output.db().clone(),
                            artifact: output.artifact().artifact_id(),
                        },
                    )
                    .await
                    {
                        fail_task(&state, task_id, format!("usage write failed: {error}")).await;
                        return;
                    }
                    outputs::export_result(&output)
                }
                Err(err) => {
                    fail_task(&state, task_id, format!("export failed: {}", err.message)).await;
                    return;
                }
            }
        }
    };
    let result = match result {
        Ok(result) => result,
        Err(err) => {
            fail_task(
                &state,
                task_id,
                format!("result assembly failed: {}", err.message),
            )
            .await;
            return;
        }
    };
    if cancellation.is_cancelled() {
        update_task(&state, task_id, TaskTransition::Cancelled).await;
        return;
    }
    let transition =
        match veoveo_task_runtime::mcp_task_completion("DuckDB operation completed", result) {
            Ok(transition) => transition,
            Err(error) => {
                fail_task(
                    &state,
                    task_id,
                    format!("serializing result failed: {error}"),
                )
                .await;
                return;
            }
        };
    super::app_state::update_task_with_stop(&state, task_id, transition, Some(&cancellation)).await;
}

fn query_usage(output: &DuckDbQueryOutput) -> DuckDbQueryUsage {
    match output.artifact() {
        Some(artifact) => DuckDbQueryUsage::Artifact {
            artifact: artifact.artifact_id(),
        },
        None => DuckDbQueryUsage::Inline {
            rows_returned: output.rows().len(),
            truncated: output.truncated(),
        },
    }
}

async fn fail_task(state: &AppState, task_id: TaskId, message: String) {
    tracing::warn!(%task_id, "duckdb task failed: {message}");
    complete_tool_error(state, task_id, message).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn task_args(name: &str, arguments: Value) -> TaskArgs {
        parse_task_args(name, arguments).unwrap()
    }

    #[test]
    fn only_read_operations_are_resumable() {
        let query = task_args("query", json!({"db": "analytics", "sql": "select 1"}));
        let export = task_args(
            "export",
            json!({
                "db": "analytics",
                "selection": {"kind": "database"},
                "format": "duck_db"
            }),
        );
        let execute = task_args(
            "execute",
            json!({"db": "analytics", "sql": "create table rows(value int)"}),
        );
        let ingest = task_args(
            "ingest",
            json!({
                "db": "analytics",
                "table": "rows",
                "source": {"kind": "inline_csv", "csv": "value\n1\n"},
                "mode": "append"
            }),
        );

        assert_eq!(task_recovery_class(&query), RecoveryClass::Resume);
        assert_eq!(task_recovery_class(&export), RecoveryClass::Resume);
        assert_eq!(
            task_recovery_class(&execute),
            RecoveryClass::InterruptedIndeterminate
        );
        assert_eq!(
            task_recovery_class(&ingest),
            RecoveryClass::InterruptedIndeterminate
        );
    }
}

#[cfg(test)]
#[path = "task_execution_tests.rs"]
mod execution_tests;
