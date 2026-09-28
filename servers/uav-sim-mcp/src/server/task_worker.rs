use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use rmcp::model::{CallToolResult, ContentBlock};
use serde::Serialize;
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::PlaneCaller;
use veoveo_task_runtime::{
    CreateTask, RecoveryClass, TaskFailure, TaskPayloadState, TaskRetentionPin, TaskSnapshot,
    TaskTransition,
};
use veoveo_types::TaskId;

use crate::contract::{
    DurableOperation, DurableOperationResult, ExecuteMissionRequest,
    ExecuteVehicleMissionPlanRequest, MissionWaypoint, SessionId, VehicleMission,
    VehicleMissionPlan, Wgs84Position,
};
use crate::uris;

use super::control_authority::{DispatchedMission, MissionExecutionGuard};
use super::ownership::runtime_owner;
use super::state::AppState;

const TASK_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
const TASK_POLL_INTERVAL_MS: u64 = 3_000;
const TASK_LEASE_DURATION: Duration = Duration::from_secs(120);
const TASK_LEASE_HEARTBEAT: Duration = Duration::from_secs(40);

pub(super) async fn start_operation(
    state: Arc<AppState>,
    caller: PlaneCaller,
    operation: DurableOperation,
    retention_pins: BTreeSet<TaskRetentionPin>,
) -> Result<TaskSnapshot, String> {
    let request = serde_json::to_value(&operation).map_err(|error| error.to_string())?;
    let created = create_task(
        &state,
        &caller,
        operation.task_type(),
        request,
        recovery_class(&operation),
        retention_pins,
    )
    .await?;
    schedule_operation(state, created.snapshot, operation, None).await
}

pub(super) async fn start_vehicle_mission_plan(
    state: Arc<AppState>,
    caller: PlaneCaller,
    request: ExecuteVehicleMissionPlanRequest,
    retention_pins: BTreeSet<TaskRetentionPin>,
) -> Result<TaskSnapshot, String> {
    let (plan, guard) = state
        .control_authority
        .begin_execution(
            &caller.identity,
            &request.plan_id,
            request.expected_revision,
        )
        .await
        .map_err(|error| error.to_string())?;
    let operation = match mission_operation(&plan) {
        Ok(operation) => operation,
        Err(error) => {
            release_failed_execution(&state, &guard).await;
            return Err(error);
        }
    };
    let task_request = match serde_json::to_value(&request) {
        Ok(request) => request,
        Err(error) => {
            release_failed_execution(&state, &guard).await;
            return Err(error.to_string());
        }
    };
    let created = create_task(
        &state,
        &caller,
        "execute_vehicle_mission_plan",
        task_request,
        RecoveryClass::InterruptedIndeterminate,
        retention_pins,
    )
    .await;
    let created = match created {
        Ok(created) => created,
        Err(error) => {
            release_failed_execution(&state, &guard).await;
            return Err(error);
        }
    };
    schedule_operation(state, created.snapshot, operation, Some(guard)).await
}

async fn create_task(
    state: &AppState,
    caller: &PlaneCaller,
    task_type: &str,
    request: serde_json::Value,
    recovery_class: RecoveryClass,
    retention_pins: BTreeSet<TaskRetentionPin>,
) -> Result<veoveo_task_runtime::CreateTaskResult, String> {
    state
        .tasks
        .create(CreateTask {
            task_id: TaskId::new(),
            owner: runtime_owner(&caller.identity),
            server: "uav-sim".to_owned(),
            task_type: task_type.to_owned(),
            request,
            recovery_class,
            idempotency_key: None,
            ttl_ms: Some(TASK_TTL_MS),
            poll_interval_ms: Some(TASK_POLL_INTERVAL_MS),
            retention_pins,
        })
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn resume_queued_operation(
    state: Arc<AppState>,
    snapshot: TaskSnapshot,
) -> Result<(), String> {
    if snapshot.task_type == "execute_vehicle_mission_plan" {
        // The public request contains a plan address, never a replayable simulator command.
        // Recovery has no live dispatch guard. Preserve any retained vehicle fence.
        let id = snapshot.task_id.to_string();
        state
            .tasks
            .claim(&id, TASK_LEASE_DURATION)
            .await
            .map_err(|error| error.to_string())?;
        state
            .tasks
            .transition(
                &id,
                indeterminate("mission worker interrupted before recovery"),
            )
            .await
            .map_err(|error| error.to_string())?;
        return Ok(());
    }
    let operation: DurableOperation =
        serde_json::from_value(snapshot.request.clone()).map_err(|error| error.to_string())?;
    if matches!(operation, DurableOperation::ExecuteMission(_))
        || operation.task_type() != snapshot.task_type
    {
        return Err("retained UAV Task type does not match its declared recovery profile".into());
    }
    schedule_operation(state, snapshot, operation, None)
        .await
        .map(|_| ())
}

async fn schedule_operation(
    state: Arc<AppState>,
    snapshot: TaskSnapshot,
    operation: DurableOperation,
    authority: Option<MissionExecutionGuard>,
) -> Result<TaskSnapshot, String> {
    let task_id = snapshot.task_id;
    let claimed = match state
        .tasks
        .claim(&task_id.to_string(), TASK_LEASE_DURATION)
        .await
    {
        Ok(claimed) => claimed,
        Err(error) => {
            if let Some(guard) = authority.as_ref()
                && let Err(finalize_error) = state.control_authority.abort_execution(guard).await
            {
                tracing::error!(%finalize_error, "failed to release UAV mission authority after task claim failure");
            }
            return Err(error.to_string());
        }
    };
    let cancellation = CancellationToken::new();
    let join = tokio::spawn(run_task(
        state.clone(),
        task_id,
        operation,
        authority,
        cancellation.clone(),
    ));
    let worker_cancellation = cancellation.clone();
    if let Err(error) = state
        .tasks
        .register_worker(&task_id.to_string(), cancellation, join)
        .await
    {
        worker_cancellation.cancel();
        return Err(error.to_string());
    }
    Ok(claimed.snapshot)
}

async fn release_failed_execution(state: &AppState, guard: &MissionExecutionGuard) {
    if let Err(error) = state.control_authority.abort_execution(guard).await {
        tracing::error!(%error, "failed to release UAV mission authority after task start failure");
    }
}

async fn run_task(
    state: Arc<AppState>,
    task_id: TaskId,
    operation: DurableOperation,
    authority: Option<MissionExecutionGuard>,
    cancellation: CancellationToken,
) {
    let session_id = operation_session(&operation).clone();
    let mission_uri = match &operation {
        DurableOperation::ExecuteMission(request) => Some(uris::mission(&request.mission_id)),
        _ => None,
    };
    let authority = authority.map(MissionExecutionGuard::dispatch);
    let work = execute_operation(
        state.clone(),
        task_id,
        operation,
        authority.as_ref(),
        cancellation.clone(),
    );
    tokio::pin!(work);
    let mut heartbeat = tokio::time::interval(TASK_LEASE_HEARTBEAT);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    heartbeat.tick().await;
    let next = loop {
        tokio::select! {
            next = &mut work => break next,
            _ = heartbeat.tick() => {
                match state.tasks.renew_lease(&task_id.to_string(), TASK_LEASE_DURATION).await {
                    Ok(snapshot) => {
                        if snapshot.cancel_requested_at.is_some() {
                            cancellation.cancel();
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%task_id, %error, "UAV simulation task lease heartbeat failed");
                        cancellation.cancel();
                        break indeterminate("Task lease heartbeat failed; simulator outcome is unknown");
                    }
                }
            }
        }
    };
    if let Some(guard) = authority.as_ref() {
        state
            .subscribers
            .notify_resource_updated(uris::MISSION_PLANS)
            .await;
        state
            .subscribers
            .notify_resource_updated(uris::mission_plan(guard.plan_id()))
            .await;
    }
    transition(&state, task_id, next).await;
    state
        .subscribers
        .notify_resource_updated(uris::session(&session_id))
        .await;
    state
        .subscribers
        .notify_resource_updated(uris::recordings(&session_id))
        .await;
    if let Some(uri) = mission_uri {
        state.subscribers.notify_resource_updated(uri).await;
    }
}

async fn execute_operation(
    state: Arc<AppState>,
    task_id: TaskId,
    operation: DurableOperation,
    authority: Option<&DispatchedMission>,
    cancellation: CancellationToken,
) -> TaskTransition {
    let observed = tokio::select! {
        result = state.adapter.execute(&operation) => result,
        () = cancellation.cancelled() => {
            return indeterminate("Task wait cancelled; simulator outcome is unknown");
        }
    };
    let completed = match observed {
        Ok(completed) => completed,
        Err(error) => {
            tracing::warn!(%task_id, %error, "UAV simulator outcome is unknown");
            return indeterminate(
                "simulator completion could not be confirmed; inspect the mission before retrying",
            );
        }
    };
    // A correlated physical completion is settled before catalog enrichment or Task delivery.
    // Cancellation after this receipt cannot turn it into a failed physical mission.
    if let Some(guard) = authority
        && let Err(error) = state
            .control_authority
            .complete_execution(guard, &completed)
            .await
    {
        tracing::error!(%task_id, %error, "failed to persist UAV mission completion");
        return TaskTransition::Failed(TaskFailure::new(
            "mission_authority_finalization_failed",
            "simulator reported completion but mission settlement is unresolved; inspect the mission before retrying",
        ));
    }
    let result = state.adapter.resolve_result(completed).await;
    if cancellation.is_cancelled() {
        return TaskTransition::Failed(TaskFailure::new(
            "completed_after_cancellation",
            "simulator completion was confirmed after cancellation of the Task wait; inspect the mission plan",
        ));
    }
    match result
        .map_err(|error| error.to_string())
        .and_then(operation_tool_result)
    {
        Ok(result) => match serde_json::to_value(result) {
            Ok(result) => TaskTransition::Succeeded {
                message: "completed".to_owned(),
                result,
            },
            Err(error) => TaskTransition::Failed(TaskFailure::new(
                "result_serialization_failed",
                error.to_string(),
            )),
        },
        Err(error) => {
            tracing::warn!(%task_id, %error, "UAV completed operation result could not be published");
            TaskTransition::Failed(TaskFailure::new("uav_sim_result_unavailable", error))
        }
    }
}

fn indeterminate(reason: &str) -> TaskTransition {
    let mut failure = TaskFailure::interrupted_indeterminate();
    failure.message = reason.to_owned();
    TaskTransition::Failed(failure)
}

fn mission_operation(plan: &VehicleMissionPlan) -> Result<DurableOperation, String> {
    let last_index = plan
        .map_route
        .path
        .len()
        .checked_sub(1)
        .ok_or_else(|| "admitted Map route has no positions".to_owned())?;
    let waypoints = plan
        .map_route
        .path
        .iter()
        .enumerate()
        .map(|(index, position)| {
            Ok(MissionWaypoint {
                position: Wgs84Position {
                    latitude_degrees: position.latitude_deg,
                    longitude_degrees: position.longitude_deg,
                    ellipsoid_height_m: position.ellipsoidal_height_m.ok_or_else(|| {
                        "admitted Map route position lacks ellipsoidal height".to_owned()
                    })?,
                },
                speed_mps: plan.speed_mps,
                hold_seconds: if index == last_index {
                    plan.hold_seconds_at_destination
                } else {
                    0.0
                },
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(DurableOperation::ExecuteMission(ExecuteMissionRequest {
        session_id: plan.session_id.clone(),
        mission_id: plan.mission_id.clone(),
        expected_world_revision_uri: plan.expected_world_revision_uri.clone(),
        vehicles: vec![VehicleMission {
            vehicle_id: plan.vehicle_id.clone(),
            waypoints,
        }],
    }))
}

fn operation_tool_result(result: DurableOperationResult) -> Result<CallToolResult, String> {
    match result {
        DurableOperationResult::RunScenario(value) => structured_result(
            format!("ran scenario for {:.3} seconds", value.elapsed_seconds),
            &value,
        ),
        DurableOperationResult::ExecuteMission(value) => {
            structured_result(format!("completed mission {}", value.mission_id), &value)
        }
        DurableOperationResult::CaptureDataset(value) => structured_result(
            format!(
                "captured {:.3} seconds of sensor data",
                value.elapsed_seconds
            ),
            &value,
        ),
    }
}

fn structured_result<T: Serialize>(message: String, value: &T) -> Result<CallToolResult, String> {
    let mut result = CallToolResult::success(vec![ContentBlock::text(message)]);
    result.structured_content = Some(serde_json::to_value(value).map_err(|e| e.to_string())?);
    Ok(result)
}

fn operation_session(operation: &DurableOperation) -> &SessionId {
    match operation {
        DurableOperation::RunScenario(request) => &request.session_id,
        DurableOperation::ExecuteMission(request) => &request.session_id,
        DurableOperation::CaptureDataset(request) => &request.session_id,
    }
}

fn recovery_class(_operation: &DurableOperation) -> RecoveryClass {
    RecoveryClass::InterruptedIndeterminate
}

async fn transition(state: &AppState, task_id: TaskId, next: TaskTransition) {
    let completed = matches!(next, TaskTransition::Succeeded { .. });
    if let Err(error) = state.tasks.transition(&task_id.to_string(), next).await {
        // Cancellation may commit between observing completion and publishing its Task result.
        // The physical receipt has already settled the mission; preserve that distinction.
        if completed
            && let Ok(Some(snapshot)) = state.tasks.get(&task_id.to_string()).await
            && snapshot.status == veoveo_platform_store::TaskStatus::CancelRequested
        {
            let next = TaskTransition::Failed(TaskFailure::new(
                "completed_after_cancellation",
                "simulator completion was confirmed after cancellation of the Task wait; inspect the mission plan",
            ));
            if let Err(error) = state.tasks.transition(&task_id.to_string(), next).await {
                tracing::warn!(%task_id, %error, "UAV completed Task cancellation settlement failed");
            }
            return;
        }
        tracing::warn!(%task_id, %error, "UAV simulation task transition failed");
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
            "UAV simulation task was cancelled",
            None,
        )),
        TaskPayloadState::Running => Err(rmcp::ErrorData::internal_error(
            "UAV simulation task wait ended while still running",
            None,
        )),
        TaskPayloadState::Unknown => Err(rmcp::ErrorData::internal_error(
            "UAV simulation task disappeared before completion",
            None,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use veoveo_map_mcp::contract::{
        MAP_ROUTE_HANDOFF_SCHEMA, MapMobilityProfileUri, MapRouteHandoff, MobilityProfileId,
        MobilityProfileVersion, RouteStatus, ValidationId, Wgs84Position as MapPosition,
    };

    use crate::contract::{
        CaptureDatasetRequest, MissionId, MissionPlanId, MissionPlanLifecycle, SessionId, VehicleId,
    };

    #[test]
    fn every_live_operation_is_indeterminate_after_interruption() {
        let operation = DurableOperation::CaptureDataset(CaptureDatasetRequest {
            session_id: SessionId::new("alpha").unwrap(),
            duration_seconds: 1.0,
            sensors: vec!["down-camera".to_owned()],
        });
        assert_eq!(operation.task_type(), "capture_dataset");
        assert_eq!(
            recovery_class(&operation),
            RecoveryClass::InterruptedIndeterminate
        );
    }

    #[test]
    fn admitted_map_path_becomes_one_private_vehicle_mission() {
        let now = Utc::now();
        let revision_uri = veoveo_frames_mcp::contract::FrameWorldRevisionUri::new(
            &veoveo_frames_mcp::contract::FrameWorldId::new("nyc").unwrap(),
            &veoveo_frames_mcp::contract::FrameWorldRevisionId::new("revision-1").unwrap(),
        );
        let plan = VehicleMissionPlan {
            plan_id: MissionPlanId::new("plan-1").unwrap(),
            mission_id: MissionId::new("mission-1").unwrap(),
            principal_key: "issuer#pilot-1".to_owned(),
            session_id: SessionId::new("uav-showcase").unwrap(),
            vehicle_id: VehicleId::new("uav-1").unwrap(),
            expected_world_revision_uri: revision_uri,
            map_route: MapRouteHandoff {
                schema_profile: MAP_ROUTE_HANDOFF_SCHEMA.to_owned(),
                route_uri: "map://route/route-1".to_owned(),
                route_digest_sha256: "a".repeat(64),
                route_status: RouteStatus::Validated,
                mobility_profile_uri: MapMobilityProfileUri::new(
                    MobilityProfileId::from_stable_key(b"native"),
                    MobilityProfileVersion::FIRST,
                ),
                path: vec![
                    MapPosition {
                        longitude_deg: -74.006,
                        latitude_deg: 40.7128,
                        ellipsoidal_height_m: Some(100.0),
                    },
                    MapPosition {
                        longitude_deg: -74.0445,
                        latitude_deg: 40.6892,
                        ellipsoidal_height_m: Some(120.0),
                    },
                ],
                validation_id: ValidationId::new(),
                validated_at: now,
                operational_snapshot_id: "snapshot-1".to_owned(),
                base_release_ids: vec!["release-1".to_owned()],
                restriction_ids: Vec::new(),
                prepared_at: now,
            },
            speed_mps: 12.0,
            hold_seconds_at_destination: 8.0,
            state: MissionPlanLifecycle::Prepared,
            expires_at: now,
            revision: 0,
            created_at: now,
            updated_at: now,
        };

        let DurableOperation::ExecuteMission(request) = mission_operation(&plan).unwrap() else {
            panic!("mission plan must compile to the private simulator operation")
        };
        assert_eq!(request.vehicles.len(), 1);
        assert_eq!(request.vehicles[0].vehicle_id, plan.vehicle_id);
        assert_eq!(request.vehicles[0].waypoints[0].hold_seconds, 0.0);
        assert_eq!(request.vehicles[0].waypoints[1].hold_seconds, 8.0);
    }
}

#[cfg(test)]
mod native_tests;
