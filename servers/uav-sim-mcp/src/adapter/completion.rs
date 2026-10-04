//! A physical completion receipt is independent of recording catalog availability.
use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::AdapterError;
use crate::contract::{
    CaptureDatasetResult, DurableOperation, DurableOperationResult, MissionId, MissionLifecycle,
    MissionResult, RecordingKey, ScenarioResult, SessionId, VehicleId,
};

// A malformed recording key cannot erase a correlated physical completion.
// Admit these provider values only when projecting the settled result.
#[derive(Debug, Deserialize)]
#[serde(transparent)]
struct UnresolvedRecordingKey(String);

impl UnresolvedRecordingKey {
    fn admit(self) -> Result<RecordingKey, AdapterError> {
        RecordingKey::parse(self.0).map_err(|_| {
            AdapterError::InvalidRecordingCatalog("invalid producer recording key".to_owned())
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AdapterScenarioResult {
    session_id: SessionId,
    elapsed_seconds: f64,
    final_simulation_time_s: f64,
    collision_count: u64,
    recording_keys: Vec<UnresolvedRecordingKey>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AdapterMissionResult {
    mission_id: MissionId,
    lifecycle: MissionLifecycle,
    started_at: DateTime<Utc>,
    finished_at: DateTime<Utc>,
    completed_waypoints: u64,
    recording_keys: Vec<UnresolvedRecordingKey>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AdapterCaptureDatasetResult {
    session_id: SessionId,
    elapsed_seconds: f64,
    recording_keys: Vec<UnresolvedRecordingKey>,
}

#[derive(Debug, Deserialize)]
#[serde(
    tag = "result",
    content = "output",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(super) enum AdapterDurableOperationResult {
    RunScenario(AdapterScenarioResult),
    ExecuteMission(AdapterMissionResult),
    CaptureDataset(AdapterCaptureDatasetResult),
}

impl AdapterDurableOperationResult {
    pub(super) fn correlate(
        self,
        operation: &DurableOperation,
    ) -> Result<CompletedOperation, AdapterError> {
        let (result, keys) = match self {
            Self::RunScenario(value) => (
                DurableOperationResult::RunScenario(ScenarioResult {
                    session_id: value.session_id,
                    elapsed_seconds: value.elapsed_seconds,
                    final_simulation_time_s: value.final_simulation_time_s,
                    collision_count: value.collision_count,
                    recording_uris: Vec::new(),
                }),
                value.recording_keys,
            ),
            Self::ExecuteMission(value) => (
                DurableOperationResult::ExecuteMission(MissionResult {
                    mission_id: value.mission_id,
                    lifecycle: value.lifecycle,
                    started_at: value.started_at,
                    finished_at: value.finished_at,
                    completed_waypoints: value.completed_waypoints,
                    recording_uris: Vec::new(),
                }),
                value.recording_keys,
            ),
            Self::CaptureDataset(value) => (
                DurableOperationResult::CaptureDataset(CaptureDatasetResult {
                    session_id: value.session_id,
                    elapsed_seconds: value.elapsed_seconds,
                    recording_uris: Vec::new(),
                }),
                value.recording_keys,
            ),
        };
        let mut completion = CompletedOperation::new(operation, result)?;
        completion.recording_keys = keys;
        Ok(completion)
    }
}

/// Constructed only after a response agrees with its dispatched operation.
/// Catalog resolution and Task delivery can fail after this observation.
#[derive(Debug)]
pub struct CompletedOperation {
    operation: DurableOperation,
    result: DurableOperationResult,
    recording_keys: Vec<UnresolvedRecordingKey>,
}

impl CompletedOperation {
    pub(super) fn new(
        operation: &DurableOperation,
        result: DurableOperationResult,
    ) -> Result<Self, AdapterError> {
        let valid = match (operation, &result) {
            (
                DurableOperation::ExecuteMission(request),
                DurableOperationResult::ExecuteMission(result),
            ) => {
                let expected = request.vehicles.iter().try_fold(0_u64, |count, vehicle| {
                    count.checked_add(u64::try_from(vehicle.waypoints.len()).ok()?)
                });
                result.mission_id == request.mission_id
                    && result.lifecycle == MissionLifecycle::Completed
                    && result.finished_at >= result.started_at
                    && expected == Some(result.completed_waypoints)
            }
            (
                DurableOperation::RunScenario(request),
                DurableOperationResult::RunScenario(result),
            ) => {
                result.session_id == request.session_id
                    && finite_nonnegative(result.elapsed_seconds)
                    && finite_nonnegative(result.final_simulation_time_s)
            }
            (
                DurableOperation::CaptureDataset(request),
                DurableOperationResult::CaptureDataset(result),
            ) => {
                result.session_id == request.session_id
                    && finite_nonnegative(result.elapsed_seconds)
            }
            _ => false,
        };
        if !valid {
            return Err(AdapterError::UncorrelatedCompletion);
        }
        Ok(Self {
            operation: operation.clone(),
            result,
            recording_keys: Vec::new(),
        })
    }

    pub fn confirms_mission(
        &self,
        session: &SessionId,
        vehicle: &VehicleId,
        mission: &MissionId,
    ) -> bool {
        matches!(&self.operation, DurableOperation::ExecuteMission(request)
            if &request.session_id == session && &request.mission_id == mission
                && request.vehicles.len() == 1 && &request.vehicles[0].vehicle_id == vehicle)
    }

    pub(super) async fn resolve(
        mut self,
        adapter: &super::Adapter,
    ) -> Result<DurableOperationResult, AdapterError> {
        if let super::Adapter::Http(adapter) = adapter {
            let keys = self
                .recording_keys
                .into_iter()
                .map(UnresolvedRecordingKey::admit)
                .collect::<Result<Vec<_>, _>>()?;
            let uris = adapter.resolve_recording_keys(keys).await?;
            match &mut self.result {
                DurableOperationResult::RunScenario(value) => value.recording_uris = uris,
                DurableOperationResult::ExecuteMission(value) => value.recording_uris = uris,
                DurableOperationResult::CaptureDataset(value) => value.recording_uris = uris,
            }
        }
        Ok(self.result)
    }
}

fn finite_nonnegative(value: f64) -> bool {
    value.is_finite() && value >= 0.0
}
