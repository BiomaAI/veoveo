use std::{
    collections::BTreeMap,
    net::{IpAddr, Ipv4Addr},
    sync::{Arc, Mutex, Weak},
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use uuid::Uuid;
use veoveo_mcp_contract::{
    LIVE_VIEW_SCHEMA, LiveCameraHealth, LiveCameraId, LiveColorMatrix, LiveColorMetadata,
    LiveColorPrimaries, LiveColorRange, LiveColorTransfer, LiveMediaEndpoint, LiveMediaTransport,
    LiveSessionId, LiveStreamProductId, LiveViewAccessToken, LiveViewCodec, LiveViewConnection,
    LiveViewHardwareEncoder, LiveViewId, LiveViewLifecycle, LiveViewOwner, LiveViewState,
    LiveViewUri, PrincipalId,
};
use veoveo_simulation_pose::PoseIngressStatus;
use veoveo_simulation_scene::GeospatialLayerCatalog;

use crate::{
    contract::{
        AuthorizePoseProducerRequest, BindSceneRequest, CameraAdmission, CameraDefinition,
        CameraRecord, CameraStreamPolicy, CapacityDimension, CapacityProfile, CapacityRejection,
        CapacityState, CapacityUsage, CloseCameraRequest, CloseLiveViewRequest, CloseResult,
        CloseSessionRequest, CreateCameraRequest, CreateSessionRequest, GeospatialLayerHealth,
        LayerFailureCode, LayerFailureDiagnostic, LayerLifecycle, OpenLiveViewRequest,
        PoseAuthorizationRenewalState, PoseSourceState, ReconciliationFailureCode,
        ReconciliationPhase, ReconciliationStatus, RenewLiveViewRequest, RevokePoseProducerRequest,
        SessionLifecycle, SetCameraRequest, SimulationViewError, SimulationViewSession,
    },
    uris,
};

mod camera;
mod lease;
mod reconciliation;
mod runtime_health;
mod session;

#[derive(Debug, Clone)]
pub struct SimulationViewConfig {
    pub capacity: CapacityProfile,
    pub maximum_asset_bytes: u64,
    pub lease_duration: Duration,
    pub endpoint: LiveMediaEndpoint,
    pub maximum_frame_age_ms: u32,
    pub layer_catalog: Arc<GeospatialLayerCatalog>,
}

impl Default for SimulationViewConfig {
    fn default() -> Self {
        Self {
            capacity: CapacityProfile {
                profile: "rtx4090-development-v1".to_owned(),
                maximum_logical_cameras: 16,
                maximum_rendered_cameras: 4,
                maximum_streamed_cameras: 2,
                maximum_render_pixels_per_second: 497_664_000,
                maximum_nvenc_sessions: 2,
                gpu_memory_budget_bytes: 20 * 1024 * 1024 * 1024,
                maximum_entity_instances: 10_000,
                maximum_cameras_per_owner: 8,
                maximum_cameras_per_work_context: 12,
            },
            maximum_asset_bytes: 4 * 1024 * 1024 * 1024,
            lease_duration: Duration::from_secs(120),
            endpoint: LiveMediaEndpoint {
                transport: LiveMediaTransport::WebRtc,
                signaling_url: "https://simulation-view.invalid/signaling".to_owned(),
                media_host: IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10)),
                media_port: 47998,
            },
            maximum_frame_age_ms: 500,
            layer_catalog: Arc::new(GeospatialLayerCatalog::empty()),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Lease {
    pub state: LiveViewState,
    pub token_hash: [u8; 32],
    events: tokio::sync::watch::Sender<LeaseSignal>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LeaseSignal {
    pub active: bool,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub(crate) struct SignalingAuthorization {
    pub state: LiveViewState,
    pub events: tokio::sync::watch::Receiver<LeaseSignal>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DurableCamera {
    pub camera: CameraRecord,
    pub render_slot: u16,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DurableSimulationViewState {
    pub session: SimulationViewSession,
    pub cameras: Vec<DurableCamera>,
}

#[derive(Debug, Default)]
struct ServiceState {
    sessions: BTreeMap<LiveSessionId, SimulationViewSession>,
    cameras: BTreeMap<LiveCameraId, CameraRecord>,
    camera_slots: BTreeMap<LiveCameraId, u16>,
    leases: BTreeMap<LiveViewId, Lease>,
}

#[derive(Debug)]
pub struct SimulationViewService {
    config: SimulationViewConfig,
    state: Mutex<ServiceState>,
    operations: tokio::sync::Mutex<BTreeMap<LiveSessionId, Weak<tokio::sync::Mutex<()>>>>,
}

impl SimulationViewService {
    pub fn new(config: SimulationViewConfig) -> Result<Arc<Self>, SimulationViewError> {
        config
            .endpoint
            .validate()
            .map_err(|_| SimulationViewError::Access)?;
        if config.capacity.maximum_logical_cameras == 0
            || config.capacity.maximum_rendered_cameras == 0
            || config.capacity.maximum_streamed_cameras == 0
            || config.capacity.maximum_nvenc_sessions == 0
            || config.capacity.maximum_render_pixels_per_second == 0
            || config.capacity.gpu_memory_budget_bytes == 0
            || config.capacity.maximum_entity_instances == 0
            || config.capacity.maximum_cameras_per_owner == 0
            || config.capacity.maximum_cameras_per_work_context == 0
            || config.maximum_asset_bytes == 0
            || config.lease_duration.is_zero()
            || config.maximum_frame_age_ms == 0
            || u32::from(config.endpoint.media_port)
                .saturating_add(config.capacity.maximum_rendered_cameras)
                .saturating_sub(1)
                > u32::from(u16::MAX)
        {
            return Err(SimulationViewError::Access);
        }
        Ok(Arc::new(Self {
            config,
            state: Mutex::new(ServiceState::default()),
            operations: tokio::sync::Mutex::new(BTreeMap::new()),
        }))
    }

    pub(crate) async fn operation_guard(
        &self,
        session_id: &LiveSessionId,
    ) -> tokio::sync::OwnedMutexGuard<()> {
        let gate = {
            let mut operations = self.operations.lock().await;
            operations.retain(|_, gate| gate.strong_count() > 0);
            if let Some(gate) = operations.get(session_id).and_then(Weak::upgrade) {
                gate
            } else {
                let gate = Arc::new(tokio::sync::Mutex::new(()));
                operations.insert(session_id.clone(), Arc::downgrade(&gate));
                gate
            }
        };
        gate.lock_owned().await
    }
}

fn owned_session<'a>(
    state: &'a ServiceState,
    owner: &LiveViewOwner,
    id: &LiveSessionId,
) -> Result<&'a SimulationViewSession, SimulationViewError> {
    let session = state
        .sessions
        .get(id)
        .ok_or_else(|| SimulationViewError::SessionNotFound(id.clone()))?;
    if &session.owner != owner {
        return Err(SimulationViewError::Ownership);
    }
    Ok(session)
}

fn owned_session_mut<'a>(
    state: &'a mut ServiceState,
    owner: &LiveViewOwner,
    id: &LiveSessionId,
) -> Result<&'a mut SimulationViewSession, SimulationViewError> {
    let session = state
        .sessions
        .get_mut(id)
        .ok_or_else(|| SimulationViewError::SessionNotFound(id.clone()))?;
    if &session.owner != owner {
        return Err(SimulationViewError::Ownership);
    }
    Ok(session)
}

fn check_revision(actual: u64, expected: u64) -> Result<(), SimulationViewError> {
    if actual == expected {
        Ok(())
    } else {
        Err(SimulationViewError::Revision { expected, actual })
    }
}

fn advance_session(session: &mut SimulationViewSession) {
    session.revision += 1;
    advance_desired(session);
}

fn advance_desired(session: &mut SimulationViewSession) {
    session.reconciliation.desired_revision =
        session.reconciliation.desired_revision.saturating_add(1);
    session.reconciliation.phase = ReconciliationPhase::Pending;
    session.reconciliation.retry_at = None;
    session.reconciliation.failed_dependency = None;
    session.reconciliation.failure_code = None;
    session.reconciliation.diagnostic = None;
    session.updated_at = Utc::now();
}

#[cfg(test)]
mod tests;
