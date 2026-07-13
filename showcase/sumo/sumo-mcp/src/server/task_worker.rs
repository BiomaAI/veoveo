use std::collections::BTreeSet;
use std::num::{NonZeroU32, NonZeroU64};
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use chrono::{TimeDelta, Utc};
use rmcp::model::{CallToolResult, ContentBlock, Resource};
use serde::Serialize;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::{
    ArtifactWriteIdempotencyKey, DataLabelId, IssueArtifactWriteCapabilityRequest,
    IssuedArtifactWriteCapability, PlaneCaller, PutArtifactRequest,
    RedeemArtifactWriteCapabilityRequest,
};
use veoveo_task_runtime::{
    CreateTask, RecoveryClass, TaskFailure, TaskId, TaskPayloadState, TaskRetentionPin,
    TaskSnapshot, TaskTransition,
};

use crate::contract::{
    DurableOperation, DurableTaskRequest, OfflineOperation, OfflineOperationRequest,
    OfflineOperationResult, RunBatchResult,
};

use super::ownership::runtime_owner;
use super::state::AppState;

const TASK_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
const TASK_POLL_INTERVAL_MS: u64 = 3_000;
const TASK_LEASE_DURATION: Duration = Duration::from_secs(120);
const TASK_LEASE_HEARTBEAT: Duration = Duration::from_secs(40);
const CONGESTION_THRESHOLD_MPS: f64 = 5.0;
const OFFLINE_COMMAND_TIMEOUT: Duration = Duration::from_secs(120);

pub(super) async fn start_operation(
    state: Arc<AppState>,
    caller: PlaneCaller,
    operation: DurableOperation,
    retention_pins: BTreeSet<TaskRetentionPin>,
) -> Result<TaskSnapshot, String> {
    let task_id = TaskId::new();
    let recovery_class = match &operation {
        DurableOperation::RunBatch(_) => RecoveryClass::InterruptedIndeterminate,
        DurableOperation::GenerateNetwork(_)
        | DurableOperation::ComputeRoutes(_)
        | DurableOperation::OptimizeSignals(_) => RecoveryClass::Resume,
    };
    let task_type = operation_name(&operation).to_owned();
    let artifact_write_capability = if matches!(&operation, DurableOperation::RunBatch(_)) {
        None
    } else {
        Some(
            state
                .artifacts
                .issue_write_capability(
                    &caller,
                    &IssueArtifactWriteCapabilityRequest {
                        task_id: task_id.to_string(),
                        expires_at: Utc::now() + TimeDelta::hours(24),
                        max_artifact_count: NonZeroU32::new(1).expect("one is non-zero"),
                        max_total_bytes: NonZeroU64::new(state.max_artifact_bytes)
                            .ok_or_else(|| "max artifact bytes must be non-zero".to_owned())?,
                    },
                )
                .await
                .map_err(|error| error.to_string())?,
        )
    };
    let request = DurableTaskRequest {
        operation,
        artifact_write_capability,
        data_labels: caller.clearance().clone(),
    };
    let created = state
        .tasks
        .create(CreateTask {
            task_id,
            owner: runtime_owner(&caller.identity),
            server: "sumo".to_owned(),
            task_type,
            request: serde_json::to_value(&request).map_err(|error| error.to_string())?,
            recovery_class,
            idempotency_key: None,
            ttl_ms: Some(TASK_TTL_MS),
            poll_interval_ms: Some(TASK_POLL_INTERVAL_MS),
            retention_pins,
        })
        .await
        .map_err(|error| error.to_string())?;
    schedule_operation(state, created.snapshot, request).await
}

pub(super) async fn resume_operation(
    state: Arc<AppState>,
    snapshot: TaskSnapshot,
) -> Result<(), String> {
    let request: DurableTaskRequest =
        serde_json::from_value(snapshot.request.clone()).map_err(|error| error.to_string())?;
    if matches!(&request.operation, DurableOperation::RunBatch(_)) {
        return Err("mutating SUMO batches are never resumed".to_owned());
    }
    schedule_operation(state, snapshot, request)
        .await
        .map(|_| ())
}

async fn schedule_operation(
    state: Arc<AppState>,
    snapshot: TaskSnapshot,
    request: DurableTaskRequest,
) -> Result<TaskSnapshot, String> {
    let task_id = snapshot.task_id.to_string();
    let claimed = state
        .tasks
        .claim(&task_id, TASK_LEASE_DURATION)
        .await
        .map_err(|error| error.to_string())?;
    let cancellation = CancellationToken::new();
    let join = tokio::spawn(run_task(
        state.clone(),
        task_id.clone(),
        request,
        cancellation.clone(),
    ));
    state
        .tasks
        .register_worker(&task_id, cancellation, join)
        .await
        .map_err(|error| error.to_string())?;
    Ok(claimed.snapshot)
}

async fn run_task(
    state: Arc<AppState>,
    task_id: String,
    request: DurableTaskRequest,
    cancellation: CancellationToken,
) {
    let work = execute_operation(
        state.clone(),
        task_id.clone(),
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
                    tracing::warn!(task_id, %error, "SUMO task lease heartbeat failed");
                    cancellation.cancel();
                    break;
                }
            }
        }
    }
}

async fn execute_operation(
    state: Arc<AppState>,
    task_id: String,
    request: DurableTaskRequest,
    cancellation: CancellationToken,
) {
    let DurableTaskRequest {
        operation,
        artifact_write_capability,
        data_labels,
    } = request;
    let result = match operation {
        DurableOperation::RunBatch(input) => {
            if input.steps == 0 || input.steps > 100_000 {
                Err(anyhow::anyhow!("steps must be in 1..=100000"))
            } else {
                run_batch(&state, input.steps, &cancellation)
                    .await
                    .and_then(|result| {
                        tool_result(
                            format!("advanced the simulation by {} steps", result.steps_advanced),
                            &result,
                        )
                    })
            }
        }
        DurableOperation::GenerateNetwork(input) => {
            match require_capability(artifact_write_capability.as_ref()) {
                Ok(capability) => run_offline(
                    &state,
                    &task_id,
                    OfflineOperation::GenerateNetwork,
                    input,
                    capability,
                    &data_labels,
                )
                .await
                .and_then(|result| offline_tool_result("generated SUMO network", &result)),
                Err(error) => Err(error),
            }
        }
        DurableOperation::ComputeRoutes(input) => {
            match require_capability(artifact_write_capability.as_ref()) {
                Ok(capability) => run_offline(
                    &state,
                    &task_id,
                    OfflineOperation::ComputeRoutes,
                    input,
                    capability,
                    &data_labels,
                )
                .await
                .and_then(|result| offline_tool_result("computed SUMO routes", &result)),
                Err(error) => Err(error),
            }
        }
        DurableOperation::OptimizeSignals(input) => {
            match require_capability(artifact_write_capability.as_ref()) {
                Ok(capability) => run_offline(
                    &state,
                    &task_id,
                    OfflineOperation::OptimizeSignals,
                    input,
                    capability,
                    &data_labels,
                )
                .await
                .and_then(|result| offline_tool_result("optimized SUMO signals", &result)),
                Err(error) => Err(error),
            }
        }
    };

    if cancellation.is_cancelled() {
        transition(&state, &task_id, TaskTransition::Cancelled).await;
        return;
    }
    match result {
        Ok(result) => match serde_json::to_value(result) {
            Ok(result) => {
                transition(
                    &state,
                    &task_id,
                    TaskTransition::Succeeded {
                        message: "completed".to_owned(),
                        result,
                    },
                )
                .await;
            }
            Err(error) => {
                transition(
                    &state,
                    &task_id,
                    TaskTransition::Failed(TaskFailure::new(
                        "result_serialization_failed",
                        error.to_string(),
                    )),
                )
                .await;
            }
        },
        Err(error) => {
            tracing::warn!(task_id, %error, "SUMO task failed");
            transition(
                &state,
                &task_id,
                TaskTransition::Failed(TaskFailure::new(
                    "sumo_operation_failed",
                    error.to_string(),
                )),
            )
            .await;
        }
    }
}

async fn run_batch(
    state: &AppState,
    steps: u32,
    cancellation: &CancellationToken,
) -> Result<RunBatchResult> {
    let mut minimum = f64::INFINITY;
    let mut world = state.world.lock().await;
    for _ in 0..steps {
        if cancellation.is_cancelled() {
            break;
        }
        world.driver.step(1)?;
        let current = world.driver.state()?;
        minimum = minimum.min(current.mean_speed_mps);
        world.publisher.publish(&current)?;
        tokio::task::yield_now().await;
    }
    let final_state = world.driver.state()?;
    Ok(RunBatchResult {
        steps_advanced: steps,
        final_simulation_time_s: final_state.simulation_time_s,
        minimum_mean_speed_mps: minimum,
        congestion_detected: minimum < CONGESTION_THRESHOLD_MPS,
    })
}

async fn run_offline(
    state: &AppState,
    task_id: &str,
    operation: OfflineOperation,
    request: OfflineOperationRequest,
    capability: &IssuedArtifactWriteCapability,
    data_labels: &BTreeSet<DataLabelId>,
) -> Result<OfflineOperationResult> {
    ensure!(
        matches!(request.kind.as_str(), "grid" | "spider" | "osm"),
        "kind must be grid, spider, or osm"
    );
    let task_dir = state.work_dir.join(task_id);
    tokio::fs::create_dir_all(&task_dir)
        .await
        .with_context(|| format!("creating {}", task_dir.display()))?;
    let network = task_dir.join("network.net.xml");
    generate_network(state, &request, &task_dir, &network).await?;
    let output = match operation {
        OfflineOperation::GenerateNetwork => network,
        OfflineOperation::ComputeRoutes => {
            let trips = task_dir.join("trips.xml");
            write_trips(&trips, request.seed).await?;
            let routes = task_dir.join("routes.rou.xml");
            run_command(
                &state.binaries.duarouter,
                &[
                    "-n",
                    path_str(&network)?,
                    "--route-files",
                    path_str(&trips)?,
                    "-o",
                    path_str(&routes)?,
                    "--ignore-errors",
                    "true",
                ],
                &task_dir,
            )
            .await?;
            routes
        }
        OfflineOperation::OptimizeSignals => {
            let signals = task_dir.join("signals.add.xml");
            run_command(
                &state.binaries.tls_coordinator,
                &["-n", path_str(&network)?, "-o", path_str(&signals)?],
                &task_dir,
            )
            .await?;
            signals
        }
    };
    let bytes = tokio::fs::read(&output)
        .await
        .with_context(|| format!("reading {}", output.display()))?;
    ensure!(
        bytes.len() as u64 <= state.max_artifact_bytes,
        "generated artifact exceeds the configured byte limit"
    );
    let filename = output
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("generated output filename is not UTF-8"))?
        .to_owned();
    let artifact = state
        .artifacts
        .redeem_write_capability(
            &capability.secret,
            &RedeemArtifactWriteCapabilityRequest {
                capability_id: capability.capability_id,
                task_id: capability.task_id.clone(),
                idempotency_key: ArtifactWriteIdempotencyKey::new(format!(
                    "sumo:{}",
                    operation.task_type()
                ))?,
                artifact: PutArtifactRequest {
                    mime_type: Some("application/xml".to_owned()),
                    filename: Some(filename),
                    classification: None,
                    data_labels: data_labels.clone(),
                    retention_expires_at: None,
                    metadata: serde_json::json!({
                        "operation": operation,
                        "kind": request.kind,
                        "seed": request.seed,
                    }),
                },
            },
            bytes,
        )
        .await
        .map_err(|error| anyhow::anyhow!("artifact plane error: {error}"))?
        .without_download_url();
    let _ = tokio::fs::remove_dir_all(&task_dir).await;
    Ok(OfflineOperationResult {
        operation,
        artifact,
    })
}

async fn generate_network(
    state: &AppState,
    request: &OfflineOperationRequest,
    task_dir: &Path,
    output: &Path,
) -> Result<()> {
    let size = 4 + request.seed % 4;
    let mode = match request.kind.as_str() {
        "grid" | "osm" => "--grid",
        "spider" => "--spider",
        _ => unreachable!("kind validated"),
    };
    let size_arg = if mode == "--grid" {
        "--grid.number"
    } else {
        "--spider.arm-number"
    };
    run_command(
        &state.binaries.netgenerate,
        &[
            mode,
            size_arg,
            &size.to_string(),
            "--seed",
            &request.seed.to_string(),
            "--tls.guess",
            "true",
            "-o",
            path_str(output)?,
        ],
        task_dir,
    )
    .await
}

async fn write_trips(path: &Path, seed: u64) -> Result<()> {
    let mut xml = String::from("<routes>\n");
    for index in 0..50_u64 {
        let from = (index.wrapping_mul(17).wrapping_add(seed)) % 4;
        let to = (index.wrapping_mul(29).wrapping_add(seed).wrapping_add(1)) % 4;
        xml.push_str(&format!(
            "  <trip id=\"t{index}\" depart=\"{index}\" from=\"edge_{from}\" to=\"edge_{to}\"/>\n"
        ));
    }
    xml.push_str("</routes>\n");
    tokio::fs::write(path, xml)
        .await
        .with_context(|| format!("writing {}", path.display()))
}

async fn run_command(binary: &Path, args: &[&str], cwd: &Path) -> Result<()> {
    let mut command = Command::new(binary);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let output = tokio::time::timeout(OFFLINE_COMMAND_TIMEOUT, command.output())
        .await
        .with_context(|| format!("{} timed out", binary.display()))?
        .with_context(|| format!("starting {}", binary.display()))?;
    ensure!(
        output.status.success(),
        "{} exited {}: {}",
        binary.display(),
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}

fn path_str(path: &Path) -> Result<&str> {
    path.to_str()
        .with_context(|| format!("path is not UTF-8: {}", path.display()))
}

fn tool_result<T: Serialize>(message: String, value: &T) -> Result<CallToolResult> {
    let mut result = CallToolResult::success(vec![ContentBlock::text(message)]);
    result.structured_content = Some(serde_json::to_value(value)?);
    Ok(result)
}

fn offline_tool_result(message: &str, value: &OfflineOperationResult) -> Result<CallToolResult> {
    let mut resource = Resource::new(value.artifact.artifact_uri.clone(), message.to_owned())
        .with_title(message.to_owned())
        .with_description("Governed SUMO XML artifact.");
    if let Some(mime_type) = &value.artifact.mime_type {
        resource = resource.with_mime_type(mime_type.clone());
    }
    let mut result = CallToolResult::success(vec![
        ContentBlock::text(format!("{message}: {}", value.artifact.artifact_uri)),
        ContentBlock::ResourceLink(resource),
    ]);
    result.structured_content = Some(serde_json::to_value(value)?);
    Ok(result)
}

fn require_capability(
    capability: Option<&IssuedArtifactWriteCapability>,
) -> Result<&IssuedArtifactWriteCapability> {
    capability.ok_or_else(|| anyhow::anyhow!("task did not reserve artifact write capability"))
}

fn operation_name(operation: &DurableOperation) -> &'static str {
    match operation {
        DurableOperation::RunBatch(_) => "run_batch",
        DurableOperation::GenerateNetwork(_) => "generate_network",
        DurableOperation::ComputeRoutes(_) => "compute_routes",
        DurableOperation::OptimizeSignals(_) => "optimize_signals",
    }
}

async fn transition(state: &AppState, task_id: &str, next: TaskTransition) {
    if let Err(error) = state.tasks.transition(task_id, next).await {
        tracing::warn!(task_id, %error, "SUMO task transition failed");
    }
}

pub(super) async fn await_result(
    state: &AppState,
    task_id: &str,
) -> Result<CallToolResult, rmcp::ErrorData> {
    match state
        .tasks
        .await_payload_state(task_id)
        .await
        .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None))?
    {
        TaskPayloadState::Completed(payload) => serde_json::from_value(payload)
            .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None)),
        TaskPayloadState::Failed(error) => Err(rmcp::ErrorData::internal_error(
            error.message,
            error.details,
        )),
        TaskPayloadState::Cancelled => Err(rmcp::ErrorData::invalid_request(
            "SUMO task was cancelled",
            None,
        )),
        TaskPayloadState::Running => Err(rmcp::ErrorData::internal_error(
            "SUMO task wait ended while still running",
            None,
        )),
        TaskPayloadState::Unknown => Err(rmcp::ErrorData::internal_error(
            "SUMO task disappeared before completion",
            None,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn recovery_class_is_explicit_by_operation() {
        assert_eq!(
            operation_name(&DurableOperation::RunBatch(
                crate::contract::RunBatchRequest { steps: 1 }
            )),
            "run_batch"
        );
        assert_eq!(
            OfflineOperation::GenerateNetwork.task_type(),
            "generate_network"
        );
    }

    #[tokio::test]
    async fn missing_offline_binary_fails_instead_of_faking_output() {
        let temp = tempfile::tempdir().unwrap();
        let result = run_command(
            &PathBuf::from("/definitely/missing/netgenerate"),
            &[],
            temp.path(),
        )
        .await;
        assert!(result.is_err());
    }
}
