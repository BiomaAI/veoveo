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

    pub fn create_session(
        &self,
        owner: LiveViewOwner,
        request: CreateSessionRequest,
    ) -> Result<SimulationViewSession, SimulationViewError> {
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        let (desired_revision, authorization_revision) =
            if let Some(existing) = state.sessions.get(&request.session_id) {
                if existing.owner != owner {
                    return Err(SimulationViewError::Ownership);
                }
                if existing.lifecycle != SessionLifecycle::Closed {
                    if existing.epoch_id != request.epoch_id {
                        return Err(SimulationViewError::SessionAlreadyExists(
                            request.session_id,
                        ));
                    }
                    return Ok(existing.clone());
                }
                (
                    existing.reconciliation.desired_revision.saturating_add(1),
                    existing.reconciliation.authorization_revision,
                )
            } else {
                (1, 0)
            };
        state
            .leases
            .retain(|_, lease| lease.state.session_id != request.session_id);
        let now = Utc::now();
        let mut reconciliation = ReconciliationStatus::pending(desired_revision);
        reconciliation.authorization_revision = authorization_revision;
        let session = SimulationViewSession {
            session_id: request.session_id,
            epoch_id: request.epoch_id,
            owner,
            lifecycle: SessionLifecycle::Created,
            revision: 1,
            reconciliation,
            scene: None,
            geospatial_layer: None,
            pose_source: None,
            created_at: now,
            updated_at: now,
        };
        state
            .sessions
            .insert(session.session_id.clone(), session.clone());
        Ok(session)
    }

    pub fn validate_bind_scene(
        &self,
        owner: &LiveViewOwner,
        request: &BindSceneRequest,
    ) -> Result<(), SimulationViewError> {
        let state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        self.validate_scene_binding(&state, owner, request)
    }

    pub fn bind_scene(
        &self,
        owner: &LiveViewOwner,
        request: BindSceneRequest,
    ) -> Result<SimulationViewSession, SimulationViewError> {
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        self.validate_scene_binding(&state, owner, &request)?;
        let session = owned_session_mut(&mut state, owner, &request.session_id)?;
        if session.scene.is_some() {
            return Ok(session.clone());
        }
        session.geospatial_layer = request
            .scene
            .body
            .geospatial_layer_id
            .as_ref()
            .and_then(|layer_id| self.config.layer_catalog.get(layer_id))
            .map(|layer| GeospatialLayerHealth {
                layer_id: layer.layer_id.clone(),
                lifecycle: LayerLifecycle::Configured,
                resident_bytes: 0,
                visible_tile_count: 0,
                pending_tile_count: 0,
                attribution: layer.license.attribution.clone(),
                attribution_url: layer.license.attribution_url.clone(),
                failure: None,
            });
        session.scene = Some(request.scene);
        session.lifecycle = SessionLifecycle::SceneBound;
        advance_session(session);
        Ok(session.clone())
    }

    fn validate_scene_binding(
        &self,
        state: &ServiceState,
        owner: &LiveViewOwner,
        request: &BindSceneRequest,
    ) -> Result<(), SimulationViewError> {
        let current_entities = state
            .sessions
            .values()
            .filter_map(|session| session.scene.as_ref())
            .map(|scene| scene.body.entities.len() as u64)
            .sum::<u64>();
        let session = owned_session(state, owner, &request.session_id)?;
        check_revision(session.revision, request.expected_revision)?;
        if request.scene.body.session_id != session.session_id
            || request.scene.body.epoch_id != session.epoch_id
        {
            return Err(SimulationViewError::InvalidScene);
        }
        if let Some(scene) = &session.scene {
            if scene.digest == request.scene.digest {
                return Ok(());
            }
            return Err(SimulationViewError::SceneAlreadyBound);
        }
        request.scene.validate(
            self.config.capacity.maximum_entity_instances,
            self.config.maximum_asset_bytes,
        )?;
        if let Some(layer_id) = &request.scene.body.geospatial_layer_id {
            self.config
                .layer_catalog
                .validate_scene_binding(
                    layer_id,
                    &request.scene.body.frame_revision,
                    &request.scene.body.simulation_frame,
                )
                .map_err(|error| SimulationViewError::GeospatialLayer(error.to_string()))?;
        }
        if current_entities.saturating_add(request.scene.body.entities.len() as u64)
            > u64::from(self.config.capacity.maximum_entity_instances)
        {
            return Err(SimulationViewError::InvalidScene);
        }
        Ok(())
    }

    pub fn authorize_pose_producer(
        &self,
        owner: &LiveViewOwner,
        request: AuthorizePoseProducerRequest,
    ) -> Result<PoseSourceState, SimulationViewError> {
        let now = Utc::now();
        if !request.spiffe_id.starts_with("spiffe://")
            || request.spiffe_id.len() > 512
            || request.spiffe_id.chars().any(char::is_whitespace)
            || request.expires_at <= now
            || request.expires_at > now + chrono::Duration::hours(24)
        {
            return Err(SimulationViewError::Producer);
        }
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        let session = owned_session_mut(&mut state, owner, &request.session_id)?;
        check_revision(session.revision, request.expected_revision)?;
        if session.scene.is_none() || session.lifecycle == SessionLifecycle::Closed {
            return Err(SimulationViewError::Lifecycle);
        }
        let authorization_lifetime_seconds =
            u64::try_from(request.expires_at.signed_duration_since(now).num_seconds())
                .map_err(|_| SimulationViewError::Producer)?;
        if authorization_lifetime_seconds == 0 {
            return Err(SimulationViewError::Producer);
        }
        let authorization_revision = session
            .reconciliation
            .authorization_revision
            .saturating_add(1);
        let source = PoseSourceState {
            producer_id: request.producer_id,
            spiffe_id: request.spiffe_id,
            authorization_revision,
            authorization_lifetime_seconds,
            authorized_at: now,
            expires_at: request.expires_at,
            revoked: false,
            last_sequence: None,
            last_snapshot_at: None,
            stale: true,
        };
        session.pose_source = Some(source.clone());
        advance_session(session);
        session.reconciliation.producer_id = Some(source.producer_id.clone());
        session.reconciliation.producer_spiffe_id = Some(source.spiffe_id.clone());
        session.reconciliation.authorization_revision = source.authorization_revision;
        session.reconciliation.authorization_expires_at = Some(source.expires_at);
        session.reconciliation.renewal_state = PoseAuthorizationRenewalState::Scheduled;
        Ok(source)
    }

    pub fn revoke_pose_producer(
        &self,
        owner: &LiveViewOwner,
        request: RevokePoseProducerRequest,
    ) -> Result<PoseSourceState, SimulationViewError> {
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        let session = owned_session_mut(&mut state, owner, &request.session_id)?;
        check_revision(session.revision, request.expected_revision)?;
        let source = session
            .pose_source
            .as_mut()
            .filter(|source| source.producer_id == request.producer_id)
            .ok_or(SimulationViewError::Producer)?;
        source.revoked = true;
        source.stale = true;
        source.authorization_revision = source.authorization_revision.saturating_add(1);
        let result = source.clone();
        advance_session(session);
        session.reconciliation.phase = ReconciliationPhase::Revoked;
        session.reconciliation.renewal_state = PoseAuthorizationRenewalState::Revoked;
        session.reconciliation.producer_id = Some(result.producer_id.clone());
        session.reconciliation.producer_spiffe_id = Some(result.spiffe_id.clone());
        session.reconciliation.authorization_revision = result.authorization_revision;
        session.reconciliation.authorization_expires_at = Some(result.expires_at);
        for camera in state
            .cameras
            .values_mut()
            .filter(|camera| camera.session_id == request.session_id)
        {
            camera.health = LiveCameraHealth::Stale;
            camera.updated_at = Utc::now();
        }
        for lease in state
            .leases
            .values_mut()
            .filter(|lease| lease.state.session_id == request.session_id)
        {
            close_lease(lease);
        }
        Ok(result)
    }

    pub fn close_session(
        &self,
        owner: &LiveViewOwner,
        request: CloseSessionRequest,
    ) -> Result<CloseResult, SimulationViewError> {
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        {
            let session = owned_session_mut(&mut state, owner, &request.session_id)?;
            check_revision(session.revision, request.expected_revision)?;
            session.lifecycle = SessionLifecycle::Closed;
            if let Some(source) = session.pose_source.as_mut() {
                source.revoked = true;
                source.stale = true;
                source.authorization_revision = source.authorization_revision.saturating_add(1);
                session.reconciliation.authorization_revision = source.authorization_revision;
            }
            advance_session(session);
        }
        for lease in state
            .leases
            .values_mut()
            .filter(|lease| lease.state.session_id == request.session_id)
        {
            close_lease(lease);
        }
        state
            .cameras
            .retain(|_, camera| camera.session_id != request.session_id);
        let active_camera_ids = state.cameras.keys().cloned().collect::<Vec<_>>();
        state
            .camera_slots
            .retain(|camera_id, _| active_camera_ids.contains(camera_id));
        Ok(CloseResult {
            resource_uri: uris::session(&request.session_id),
            closed: true,
        })
    }

    pub fn create_camera(
        &self,
        owner: &LiveViewOwner,
        request: CreateCameraRequest,
    ) -> Result<CameraAdmission, SimulationViewError> {
        request.definition.validate()?;
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        validate_camera_session(&state, owner, &request.session_id, &request.definition)?;
        if let Some(rejection) = self.camera_rejection(&state, owner, &request.definition, None) {
            return Ok(CameraAdmission::Rejected { rejection });
        }
        let now = Utc::now();
        let camera = CameraRecord {
            camera_id: LiveCameraId::new(format!("camera-{}", Uuid::now_v7()))
                .map_err(|_| SimulationViewError::InvalidIdentifier("camera".to_owned()))?,
            session_id: request.session_id,
            owner: owner.clone(),
            revision: 1,
            definition: request.definition,
            health: LiveCameraHealth::Warming,
            last_pose_sequence: None,
            last_frame_at: None,
            created_at: now,
            updated_at: now,
        };
        let render_slot = first_available_render_slot(
            &state.camera_slots,
            self.config.capacity.maximum_rendered_cameras,
        )
        .ok_or(SimulationViewError::Lifecycle)?;
        state
            .camera_slots
            .insert(camera.camera_id.clone(), render_slot);
        state
            .cameras
            .insert(camera.camera_id.clone(), camera.clone());
        if let Some(session) = state.sessions.get_mut(&camera.session_id) {
            advance_desired(session);
        }
        Ok(CameraAdmission::Admitted {
            camera: Box::new(camera),
        })
    }

    pub fn set_camera(
        &self,
        owner: &LiveViewOwner,
        request: SetCameraRequest,
    ) -> Result<CameraAdmission, SimulationViewError> {
        request.definition.validate()?;
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        validate_camera_session(&state, owner, &request.session_id, &request.definition)?;
        let camera = state
            .cameras
            .get(&request.camera_id)
            .filter(|camera| camera.session_id == request.session_id)
            .ok_or_else(|| SimulationViewError::CameraNotFound(request.camera_id.clone()))?;
        if &camera.owner != owner {
            return Err(SimulationViewError::Ownership);
        }
        check_revision(camera.revision, request.expected_revision)?;
        if let Some(rejection) =
            self.camera_rejection(&state, owner, &request.definition, Some(&request.camera_id))
        {
            return Ok(CameraAdmission::Rejected { rejection });
        }
        for lease in state
            .leases
            .values_mut()
            .filter(|lease| lease.state.camera_id == request.camera_id)
        {
            close_lease(lease);
        }
        let camera = state
            .cameras
            .get_mut(&request.camera_id)
            .expect("camera checked above");
        camera.definition = request.definition;
        camera.revision += 1;
        camera.health = LiveCameraHealth::Warming;
        camera.updated_at = Utc::now();
        let result = camera.clone();
        if let Some(session) = state.sessions.get_mut(&request.session_id) {
            advance_desired(session);
        }
        Ok(CameraAdmission::Admitted {
            camera: Box::new(result),
        })
    }

    pub fn close_camera(
        &self,
        owner: &LiveViewOwner,
        request: CloseCameraRequest,
    ) -> Result<CloseResult, SimulationViewError> {
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        let camera = state
            .cameras
            .get(&request.camera_id)
            .filter(|camera| camera.session_id == request.session_id)
            .ok_or_else(|| SimulationViewError::CameraNotFound(request.camera_id.clone()))?;
        if &camera.owner != owner {
            return Err(SimulationViewError::Ownership);
        }
        check_revision(camera.revision, request.expected_revision)?;
        state.cameras.remove(&request.camera_id);
        state.camera_slots.remove(&request.camera_id);
        for lease in state
            .leases
            .values_mut()
            .filter(|lease| lease.state.camera_id == request.camera_id)
        {
            close_lease(lease);
        }
        if let Some(session) = state.sessions.get_mut(&request.session_id) {
            advance_desired(session);
        }
        Ok(CloseResult {
            resource_uri: uris::camera(&request.session_id, &request.camera_id),
            closed: true,
        })
    }

    pub fn open_live_view(
        &self,
        owner: &LiveViewOwner,
        viewer_actor: &PrincipalId,
        request: OpenLiveViewRequest,
    ) -> Result<LiveViewConnection, SimulationViewError> {
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        let camera = state
            .cameras
            .get(&request.camera_id)
            .filter(|camera| camera.session_id == request.session_id)
            .ok_or_else(|| SimulationViewError::CameraNotFound(request.camera_id.clone()))?
            .clone();
        let render_slot = *state
            .camera_slots
            .get(&request.camera_id)
            .ok_or(SimulationViewError::Lifecycle)?;
        if &camera.owner != owner {
            return Err(SimulationViewError::Ownership);
        }
        if camera.definition.stream_policy == CameraStreamPolicy::Disabled {
            return Err(SimulationViewError::Lifecycle);
        }
        if let Some(existing_id) = state.leases.iter().find_map(|(id, lease)| {
            (lease.state.owner == *owner
                && lease.state.viewer_actor == *viewer_actor
                && lease.state.viewer_instance_id == request.viewer_instance_id
                && lease.state.camera_id == request.camera_id
                && !matches!(
                    lease.state.lifecycle,
                    LiveViewLifecycle::Closed | LiveViewLifecycle::Failed
                )
                && lease.state.expires_at > Utc::now())
            .then(|| id.clone())
        }) {
            return rotate_lease(&self.config, state.leases.get_mut(&existing_id).unwrap());
        }
        let stream_product_active = state
            .leases
            .values()
            .any(|lease| lease.state.camera_id == request.camera_id && active_lease(&lease.state));
        if camera.definition.stream_policy == CameraStreamPolicy::OnDemand && !stream_product_active
        {
            let usage = capacity_usage(&state, None);
            if usage.streamed_cameras >= self.config.capacity.maximum_streamed_cameras
                || usage.nvenc_sessions >= self.config.capacity.maximum_nvenc_sessions
            {
                return Err(SimulationViewError::Lifecycle);
            }
        }
        let live_view_id = LiveViewId::new(format!("stream-{}", Uuid::now_v7()))
            .map_err(|_| SimulationViewError::InvalidIdentifier("stream".to_owned()))?;
        let now = Utc::now();
        let expires_at = expiry(now, self.config.lease_duration)?;
        let token = new_token()?;
        let selected_entity_id = selected_entity(&camera.definition).map(ToOwned::to_owned);
        let mut endpoint = self.config.endpoint.clone();
        endpoint.media_port = endpoint
            .media_port
            .checked_add(render_slot)
            .ok_or(SimulationViewError::Access)?;
        let stream = LiveViewState {
            schema_version: LIVE_VIEW_SCHEMA.to_owned(),
            live_view_id: live_view_id.clone(),
            stream_product_id: stream_product_id(&request.camera_id),
            resource_uri: LiveViewUri::new(uris::stream(&request.session_id, &live_view_id))
                .map_err(|_| SimulationViewError::Access)?,
            owner: owner.clone(),
            viewer_actor: viewer_actor.clone(),
            viewer_instance_id: request.viewer_instance_id,
            session_id: request.session_id,
            camera_id: request.camera_id,
            lifecycle: LiveViewLifecycle::Starting,
            semantic_source: camera.definition.rig.source(),
            selected_entity_id,
            camera_revision: camera.revision,
            codec: LiveViewCodec::H264,
            hardware_encoder: LiveViewHardwareEncoder::NvidiaNvenc,
            color: LiveColorMetadata {
                primaries: LiveColorPrimaries::Bt709,
                transfer: LiveColorTransfer::Bt709,
                matrix: LiveColorMatrix::Bt709,
                range: LiveColorRange::Limited,
            },
            width_px: camera.definition.width_px,
            height_px: camera.definition.height_px,
            frame_rate_millihertz: camera.definition.frame_rate_millihertz,
            connected_viewers: 0,
            viewer_limit: 1,
            camera_health: camera.health,
            last_frame_at: camera.last_frame_at,
            maximum_frame_age_ms: self.config.maximum_frame_age_ms,
            endpoint,
            created_at: now,
            expires_at,
        };
        stream.validate().map_err(|_| SimulationViewError::Access)?;
        let (events, _) = tokio::sync::watch::channel(lease_signal(&stream));
        state.leases.insert(
            live_view_id,
            Lease {
                state: stream.clone(),
                token_hash: token_hash(&token),
                events,
            },
        );
        Ok(LiveViewConnection {
            stream,
            access_token: token,
        })
    }

    pub fn renew_live_view(
        &self,
        owner: &LiveViewOwner,
        viewer_actor: &PrincipalId,
        request: RenewLiveViewRequest,
    ) -> Result<LiveViewConnection, SimulationViewError> {
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        let lease = state
            .leases
            .get_mut(&request.live_view_id)
            .filter(|lease| lease.state.session_id == request.session_id)
            .ok_or_else(|| SimulationViewError::LiveViewNotFound(request.live_view_id.clone()))?;
        if &lease.state.owner != owner
            || &lease.state.viewer_actor != viewer_actor
            || lease.state.viewer_instance_id != request.viewer_instance_id
        {
            return Err(SimulationViewError::Ownership);
        }
        if matches!(
            lease.state.lifecycle,
            LiveViewLifecycle::Closed | LiveViewLifecycle::Failed
        ) {
            return Err(SimulationViewError::Lifecycle);
        }
        rotate_lease(&self.config, lease)
    }

    pub fn close_live_view(
        &self,
        owner: &LiveViewOwner,
        viewer_actor: &PrincipalId,
        request: CloseLiveViewRequest,
    ) -> Result<CloseResult, SimulationViewError> {
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        let lease = state
            .leases
            .get_mut(&request.live_view_id)
            .filter(|lease| lease.state.session_id == request.session_id)
            .ok_or_else(|| SimulationViewError::LiveViewNotFound(request.live_view_id.clone()))?;
        if &lease.state.owner != owner
            || &lease.state.viewer_actor != viewer_actor
            || lease.state.viewer_instance_id != request.viewer_instance_id
        {
            return Err(SimulationViewError::Ownership);
        }
        close_lease(lease);
        let resource_uri = lease.state.resource_uri.as_str().to_owned();
        Ok(CloseResult {
            resource_uri,
            closed: true,
        })
    }

    pub(crate) fn authorize_signaling(
        &self,
        live_view_id: &LiveViewId,
        token: &str,
    ) -> Result<SignalingAuthorization, SimulationViewError> {
        let supplied: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        let lease = state
            .leases
            .get_mut(live_view_id)
            .ok_or_else(|| SimulationViewError::LiveViewNotFound(live_view_id.clone()))?;
        if lease.state.expires_at <= Utc::now()
            || matches!(
                lease.state.lifecycle,
                LiveViewLifecycle::Closed | LiveViewLifecycle::Failed
            )
            || supplied.ct_eq(&lease.token_hash).unwrap_u8() != 1
            || lease.state.connected_viewers >= lease.state.viewer_limit
        {
            return Err(SimulationViewError::Access);
        }
        lease.state.connected_viewers += 1;
        lease.state.lifecycle = LiveViewLifecycle::Live;
        Ok(SignalingAuthorization {
            state: lease.state.clone(),
            events: lease.events.subscribe(),
        })
    }

    pub fn disconnect_signaling(&self, live_view_id: &LiveViewId) {
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        let Some(lease) = state.leases.get_mut(live_view_id) else {
            return;
        };
        lease.state.connected_viewers = lease.state.connected_viewers.saturating_sub(1);
        if lease.state.connected_viewers == 0 && lease.state.lifecycle == LiveViewLifecycle::Live {
            lease.state.lifecycle = LiveViewLifecycle::Ready;
        }
    }

    pub fn revoke_owner(&self, owner: &LiveViewOwner) {
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        for lease in state
            .leases
            .values_mut()
            .filter(|lease| lease.state.owner == *owner)
        {
            close_lease(lease);
        }
    }

    pub(crate) fn apply_camera_status(
        &self,
        camera_id: &LiveCameraId,
        last_pose_sequence: Option<u64>,
        last_frame_at: Option<DateTime<Utc>>,
    ) {
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        if let Some(camera) = state.cameras.get_mut(camera_id) {
            camera.health = LiveCameraHealth::Healthy;
            camera.last_pose_sequence = last_pose_sequence;
            camera.last_frame_at = last_frame_at;
            camera.updated_at = Utc::now();
        }
        for lease in state
            .leases
            .values_mut()
            .filter(|lease| lease.state.camera_id == *camera_id)
        {
            lease.state.camera_health = LiveCameraHealth::Healthy;
            lease.state.last_frame_at = last_frame_at;
        }
    }

    pub(crate) fn refresh_camera_status(
        &self,
        camera_id: &LiveCameraId,
        ready: bool,
        last_pose_sequence: Option<u64>,
        last_frame_at: Option<DateTime<Utc>>,
    ) {
        let health = if ready {
            LiveCameraHealth::Healthy
        } else if last_pose_sequence.is_some() || last_frame_at.is_some() {
            LiveCameraHealth::Stale
        } else {
            LiveCameraHealth::Warming
        };
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        if let Some(camera) = state.cameras.get_mut(camera_id) {
            camera.health = health;
            camera.last_pose_sequence = last_pose_sequence;
            camera.last_frame_at = last_frame_at;
            camera.updated_at = Utc::now();
        }
        for lease in state
            .leases
            .values_mut()
            .filter(|lease| lease.state.camera_id == *camera_id)
        {
            lease.state.camera_health = health;
            lease.state.last_frame_at = last_frame_at;
        }
    }

    pub(crate) fn mark_camera_stale(&self, camera_id: &LiveCameraId) {
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        if let Some(camera) = state.cameras.get_mut(camera_id) {
            camera.health = if camera.last_pose_sequence.is_some() || camera.last_frame_at.is_some()
            {
                LiveCameraHealth::Stale
            } else {
                LiveCameraHealth::Warming
            };
            camera.updated_at = Utc::now();
        }
        let health = state
            .cameras
            .get(camera_id)
            .map(|camera| camera.health)
            .unwrap_or(LiveCameraHealth::Stale);
        for lease in state
            .leases
            .values_mut()
            .filter(|lease| lease.state.camera_id == *camera_id)
        {
            lease.state.camera_health = health;
        }
    }

    pub(crate) fn apply_pose_status(
        &self,
        session_id: &LiveSessionId,
        status: &PoseIngressStatus,
    ) -> Result<(), SimulationViewError> {
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| SimulationViewError::SessionNotFound(session_id.clone()))?;
        let source = session
            .pose_source
            .as_mut()
            .ok_or(SimulationViewError::Lifecycle)?;
        if status.session_id.as_str() != session_id.as_str()
            || status.epoch_id != session.epoch_id
            || status.producer_id != source.producer_id.as_str()
            || status.producer_spiffe_id != source.spiffe_id
            || status.authorization_revision != source.authorization_revision
            || status.authorized_until != source.expires_at
            || status.revoked != source.revoked
        {
            return Err(SimulationViewError::Producer);
        }
        source.last_sequence = status.last_sequence;
        source.last_snapshot_at = status.last_snapshot_at;
        source.stale = status.stale;
        Ok(())
    }

    pub(crate) fn apply_layer_status(
        &self,
        session_id: &LiveSessionId,
        status: GeospatialLayerHealth,
    ) -> Result<(), SimulationViewError> {
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| SimulationViewError::SessionNotFound(session_id.clone()))?;
        let expected = session
            .scene
            .as_ref()
            .and_then(|scene| scene.body.geospatial_layer_id.as_ref())
            .ok_or(SimulationViewError::Lifecycle)?;
        if &status.layer_id != expected {
            return Err(SimulationViewError::GeospatialLayer(
                "renderer returned health for a different layer".to_owned(),
            ));
        }
        session.geospatial_layer = Some(status);
        session.updated_at = Utc::now();
        Ok(())
    }

    pub(crate) fn mark_layer_unavailable(&self, session_id: &LiveSessionId) {
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        if let Some(layer) = state
            .sessions
            .get_mut(session_id)
            .and_then(|session| session.geospatial_layer.as_mut())
        {
            layer.lifecycle = LayerLifecycle::Failed;
            layer.failure = Some(LayerFailureDiagnostic {
                code: LayerFailureCode::ProviderUnavailable,
                message: "renderer layer health is unavailable".to_owned(),
            });
        }
    }

    pub(crate) fn mark_pose_stale(&self, session_id: &LiveSessionId) {
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        if let Some(source) = state
            .sessions
            .get_mut(session_id)
            .and_then(|session| session.pose_source.as_mut())
        {
            source.stale = true;
        }
    }

    pub(crate) fn mark_stream_product_ready(&self, camera_id: &LiveCameraId) {
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        for lease in state
            .leases
            .values_mut()
            .filter(|lease| lease.state.camera_id == *camera_id && active_lease(&lease.state))
        {
            lease.state.lifecycle = LiveViewLifecycle::Ready;
        }
    }

    pub(crate) fn cancel_stream_admission(&self, stream_id: &LiveViewId) {
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        if let Some(lease) = state.leases.get_mut(stream_id) {
            close_lease(lease);
        }
    }

    pub(crate) fn durable_state(
        &self,
        session_id: &LiveSessionId,
    ) -> Result<DurableSimulationViewState, SimulationViewError> {
        let state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        let session = state
            .sessions
            .get(session_id)
            .ok_or_else(|| SimulationViewError::SessionNotFound(session_id.clone()))?
            .clone();
        let cameras = state
            .cameras
            .values()
            .filter(|camera| camera.session_id == *session_id)
            .map(|camera| {
                let render_slot = state
                    .camera_slots
                    .get(&camera.camera_id)
                    .copied()
                    .ok_or(SimulationViewError::Lifecycle)?;
                Ok(DurableCamera {
                    camera: camera.clone(),
                    render_slot,
                })
            })
            .collect::<Result<Vec<_>, SimulationViewError>>()?;
        Ok(DurableSimulationViewState { session, cameras })
    }

    #[cfg(test)]
    pub(crate) fn mutate_runtime_state_for_test(
        &self,
        session_id: &LiveSessionId,
        camera_ids: &[LiveCameraId],
        stream_id: &LiveViewId,
        sequence: u64,
    ) {
        let at = Utc::now()
            + chrono::Duration::seconds(i64::try_from(sequence).expect("test sequence fits i64"));
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        let session = state.sessions.get_mut(session_id).unwrap();
        session.updated_at = at;
        session.geospatial_layer = Some(GeospatialLayerHealth {
            layer_id: crate::contract::GeospatialLayerId::new("synthetic-layer").unwrap(),
            lifecycle: LayerLifecycle::Ready,
            resident_bytes: sequence * 1024,
            visible_tile_count: u32::try_from(sequence).unwrap(),
            pending_tile_count: u32::try_from(sequence + 1).unwrap(),
            attribution: "Synthetic map fixture".to_owned(),
            attribution_url: "https://example.test/map".to_owned(),
            failure: None,
        });
        session.reconciliation.phase = ReconciliationPhase::Cameras;
        session.reconciliation.next_attempt_at = Some(at);
        session.reconciliation.last_successful_reconciliation_at = Some(at);
        if let Some(source) = session.pose_source.as_mut() {
            source.authorized_at = at;
            source.expires_at = at + chrono::Duration::minutes(10);
            source.last_sequence = Some(sequence);
            source.last_snapshot_at = Some(at);
            source.stale = sequence.is_multiple_of(2);
        }
        for camera_id in camera_ids {
            let camera = state.cameras.get_mut(camera_id).unwrap();
            camera.health = if sequence.is_multiple_of(2) {
                LiveCameraHealth::Healthy
            } else {
                LiveCameraHealth::Stale
            };
            camera.last_pose_sequence = Some(sequence);
            camera.last_frame_at = Some(at);
            camera.updated_at = at;
        }
        let lease = state.leases.get_mut(stream_id).unwrap();
        lease.state.lifecycle = LiveViewLifecycle::Live;
        lease.state.connected_viewers = 1;
        lease.state.camera_health = LiveCameraHealth::Healthy;
        lease.state.last_frame_at = Some(at);
        lease.state.expires_at = at + chrono::Duration::minutes(2);
        lease.token_hash = [u8::try_from(sequence).unwrap(); 32];
    }

    #[cfg(test)]
    pub(crate) fn mutate_camera_intent_for_test(&self, camera_id: &LiveCameraId) {
        self.state
            .lock()
            .expect("simulation-view state lock poisoned")
            .cameras
            .get_mut(camera_id)
            .unwrap()
            .definition
            .width_px += 1;
    }

    pub(crate) fn restore_durable_state(
        &self,
        mut desired: DurableSimulationViewState,
    ) -> Result<(), SimulationViewError> {
        let now = Utc::now();
        for camera in &mut desired.cameras {
            camera.camera.health = LiveCameraHealth::Warming;
            camera.camera.last_pose_sequence = None;
            camera.camera.last_frame_at = None;
            camera.camera.updated_at = now;
        }
        if let Some(source) = desired.session.pose_source.as_mut() {
            source.last_sequence = None;
            source.last_snapshot_at = None;
            source.stale = true;
        }
        if desired.session.lifecycle != SessionLifecycle::Closed {
            desired.session.lifecycle = if desired.session.scene.is_some() {
                SessionLifecycle::SceneBound
            } else {
                SessionLifecycle::Created
            };
        }
        desired.session.reconciliation.phase = if desired
            .session
            .pose_source
            .as_ref()
            .is_some_and(|source| source.revoked)
        {
            ReconciliationPhase::Revoked
        } else {
            ReconciliationPhase::Pending
        };
        desired.session.reconciliation.next_attempt_at = Some(now);
        let session_id = desired.session.session_id.clone();
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        if state.sessions.contains_key(&session_id) {
            return Err(SimulationViewError::SessionAlreadyExists(session_id));
        }
        for durable in desired.cameras {
            if durable.camera.session_id != session_id
                || state.cameras.contains_key(&durable.camera.camera_id)
                || state
                    .camera_slots
                    .values()
                    .any(|slot| *slot == durable.render_slot)
            {
                return Err(SimulationViewError::Lifecycle);
            }
            state
                .camera_slots
                .insert(durable.camera.camera_id.clone(), durable.render_slot);
            state
                .cameras
                .insert(durable.camera.camera_id.clone(), durable.camera);
        }
        state.sessions.insert(session_id, desired.session);
        Ok(())
    }

    pub(crate) fn reconciliation_sessions(&self) -> Vec<SimulationViewSession> {
        let now = Utc::now();
        self.state
            .lock()
            .expect("simulation-view state lock poisoned")
            .sessions
            .values()
            .filter(|session| {
                let due = session
                    .reconciliation
                    .next_attempt_at
                    .is_some_and(|next_attempt| next_attempt <= now);
                if session.reconciliation.phase == ReconciliationPhase::Blocked {
                    due
                } else {
                    session.reconciliation.desired_revision
                        > session.reconciliation.realized_revision
                        || session.reconciliation.phase == ReconciliationPhase::Pending
                        || due
                }
            })
            .cloned()
            .collect()
    }

    pub(crate) fn next_reconciliation_at(&self) -> Option<DateTime<Utc>> {
        let now = Utc::now();
        let state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        if state.sessions.values().any(|session| {
            session.reconciliation.phase != ReconciliationPhase::Blocked
                && (session.reconciliation.desired_revision
                    > session.reconciliation.realized_revision
                    || session.reconciliation.phase == ReconciliationPhase::Pending)
        }) {
            return Some(now);
        }
        state
            .sessions
            .values()
            .filter_map(|session| session.reconciliation.next_attempt_at)
            .min()
    }

    pub(crate) fn request_runtime_reconciliation(&self, session_id: &LiveSessionId) {
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        let Some(session) = state.sessions.get_mut(session_id) else {
            return;
        };
        if session.lifecycle != SessionLifecycle::Closed {
            session.reconciliation.phase = ReconciliationPhase::Pending;
            session.reconciliation.next_attempt_at = Some(Utc::now());
            session.updated_at = Utc::now();
        }
    }

    pub(crate) fn request_all_runtime_reconciliation(&self) {
        let now = Utc::now();
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        for session in state
            .sessions
            .values_mut()
            .filter(|session| session.lifecycle != SessionLifecycle::Closed)
        {
            session.reconciliation.phase = ReconciliationPhase::Pending;
            session.reconciliation.next_attempt_at = Some(now);
            session.updated_at = now;
        }
    }

    pub(crate) fn reconciliation_ready(&self) -> bool {
        self.state
            .lock()
            .expect("simulation-view state lock poisoned")
            .sessions
            .values()
            .all(|session| {
                session.reconciliation.desired_revision == session.reconciliation.realized_revision
                    && matches!(
                        session.reconciliation.phase,
                        ReconciliationPhase::Healthy | ReconciliationPhase::Revoked
                    )
            })
    }

    pub(crate) fn reconciliation_cameras(
        &self,
        session_id: &LiveSessionId,
    ) -> Vec<(CameraRecord, u16)> {
        let state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        state
            .cameras
            .values()
            .filter(|camera| camera.session_id == *session_id)
            .filter_map(|camera| {
                state
                    .camera_slots
                    .get(&camera.camera_id)
                    .copied()
                    .map(|slot| (camera.clone(), slot))
            })
            .collect()
    }

    pub(crate) fn schedule_pose_renewal(&self, session_id: &LiveSessionId, at: DateTime<Utc>) {
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        if let Some(session) = state.sessions.get_mut(session_id) {
            session.reconciliation.renewal_state = if session
                .pose_source
                .as_ref()
                .is_some_and(|source| source.revoked)
            {
                PoseAuthorizationRenewalState::Revoked
            } else {
                PoseAuthorizationRenewalState::Scheduled
            };
            session.reconciliation.next_attempt_at = Some(at);
            session.updated_at = Utc::now();
        }
    }

    pub(crate) fn renew_pose_authorization(
        &self,
        session_id: &LiveSessionId,
        now: DateTime<Utc>,
    ) -> Result<PoseSourceState, SimulationViewError> {
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| SimulationViewError::SessionNotFound(session_id.clone()))?;
        let source = session
            .pose_source
            .as_mut()
            .ok_or(SimulationViewError::Lifecycle)?;
        if source.revoked {
            return Err(SimulationViewError::ProducerRevoked);
        }
        let lifetime = chrono::Duration::seconds(
            i64::try_from(source.authorization_lifetime_seconds)
                .map_err(|_| SimulationViewError::Time)?,
        );
        source.authorized_at = now;
        source.expires_at = now
            .checked_add_signed(lifetime)
            .ok_or(SimulationViewError::Time)?;
        source.authorization_revision = source.authorization_revision.saturating_add(1);
        let result = source.clone();
        advance_desired(session);
        session.reconciliation.phase = ReconciliationPhase::PoseAuthorization;
        session.reconciliation.renewal_state = PoseAuthorizationRenewalState::Renewing;
        session.reconciliation.authorization_revision = result.authorization_revision;
        session.reconciliation.authorization_expires_at = Some(result.expires_at);
        Ok(result)
    }

    pub(crate) fn mark_reconciliation_phase(
        &self,
        session_id: &LiveSessionId,
        phase: ReconciliationPhase,
        next_attempt_at: Option<DateTime<Utc>>,
    ) {
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        if let Some(session) = state.sessions.get_mut(session_id) {
            let verifying_converged_state = session.reconciliation.desired_revision
                == session.reconciliation.realized_revision
                && matches!(
                    session.reconciliation.phase,
                    ReconciliationPhase::Healthy | ReconciliationPhase::Revoked
                );
            if !verifying_converged_state {
                session.reconciliation.phase = phase;
            }
            session.reconciliation.next_attempt_at = next_attempt_at;
            session.reconciliation.failed_dependency = None;
            session.reconciliation.failure_code = None;
            session.reconciliation.diagnostic = None;
            session.updated_at = Utc::now();
        }
    }

    pub(crate) fn mark_reconciliation_failed(
        &self,
        session_id: &LiveSessionId,
        dependency: &str,
        code: ReconciliationFailureCode,
        diagnostic: &str,
        next_attempt_at: DateTime<Utc>,
    ) {
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        if let Some(session) = state.sessions.get_mut(session_id) {
            session.reconciliation.phase = ReconciliationPhase::Blocked;
            session.reconciliation.renewal_state =
                if code == ReconciliationFailureCode::PoseAuthorizationExpired {
                    PoseAuthorizationRenewalState::Expired
                } else {
                    PoseAuthorizationRenewalState::Blocked
                };
            session.reconciliation.failed_dependency = Some(dependency.to_owned());
            session.reconciliation.failure_code = Some(code);
            session.reconciliation.diagnostic = Some(diagnostic.to_owned());
            session.reconciliation.next_attempt_at = Some(next_attempt_at);
            session.updated_at = Utc::now();
        }
    }

    pub(crate) fn mark_reconciliation_healthy(
        &self,
        session_id: &LiveSessionId,
        at: DateTime<Utc>,
    ) {
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        if let Some(session) = state.sessions.get_mut(session_id) {
            if session.lifecycle != SessionLifecycle::Closed {
                session.lifecycle = if session.scene.is_some() {
                    SessionLifecycle::Ready
                } else {
                    SessionLifecycle::Created
                };
            }
            session.reconciliation.realized_revision = session.reconciliation.desired_revision;
            session.reconciliation.phase = if session
                .pose_source
                .as_ref()
                .is_some_and(|source| source.revoked)
            {
                ReconciliationPhase::Revoked
            } else {
                ReconciliationPhase::Healthy
            };
            session.reconciliation.renewal_state = match session.pose_source.as_ref() {
                Some(source) if source.revoked => PoseAuthorizationRenewalState::Revoked,
                Some(_) => PoseAuthorizationRenewalState::Scheduled,
                None => PoseAuthorizationRenewalState::Current,
            };
            session.reconciliation.last_successful_reconciliation_at = Some(at);
            if session.lifecycle == SessionLifecycle::Closed
                || session.pose_source.is_none()
                || session
                    .pose_source
                    .as_ref()
                    .is_some_and(|source| source.revoked)
            {
                session.reconciliation.next_attempt_at = None;
            }
            session.reconciliation.failed_dependency = None;
            session.reconciliation.failure_code = None;
            session.reconciliation.diagnostic = None;
            session.updated_at = at;
        }
    }

    pub fn capacity(&self) -> CapacityState {
        let state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        CapacityState {
            limits: self.config.capacity.clone(),
            usage: capacity_usage(&state, None),
        }
    }

    pub fn get_session(
        &self,
        owner: &LiveViewOwner,
        id: &LiveSessionId,
    ) -> Result<SimulationViewSession, SimulationViewError> {
        let state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        let session = state
            .sessions
            .get(id)
            .ok_or_else(|| SimulationViewError::SessionNotFound(id.clone()))?;
        if &session.owner != owner {
            return Err(SimulationViewError::Ownership);
        }
        Ok(session.clone())
    }

    pub fn list_sessions(&self, owner: &LiveViewOwner) -> Vec<SimulationViewSession> {
        self.state
            .lock()
            .expect("simulation-view state lock poisoned")
            .sessions
            .values()
            .filter(|session| session.owner == *owner)
            .cloned()
            .collect()
    }

    pub fn list_cameras(
        &self,
        owner: &LiveViewOwner,
        session_id: &LiveSessionId,
    ) -> Vec<CameraRecord> {
        self.state
            .lock()
            .expect("simulation-view state lock poisoned")
            .cameras
            .values()
            .filter(|camera| camera.owner == *owner && camera.session_id == *session_id)
            .cloned()
            .collect()
    }

    pub fn get_camera(
        &self,
        owner: &LiveViewOwner,
        camera_id: &LiveCameraId,
    ) -> Result<CameraRecord, SimulationViewError> {
        let state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        let camera = state
            .cameras
            .get(camera_id)
            .ok_or_else(|| SimulationViewError::CameraNotFound(camera_id.clone()))?;
        if &camera.owner != owner {
            return Err(SimulationViewError::Ownership);
        }
        Ok(camera.clone())
    }

    pub fn list_streams(
        &self,
        owner: &LiveViewOwner,
        viewer_actor: &PrincipalId,
        session_id: &LiveSessionId,
    ) -> Vec<LiveViewState> {
        self.state
            .lock()
            .expect("simulation-view state lock poisoned")
            .leases
            .values()
            .filter(|lease| {
                lease.state.owner == *owner
                    && lease.state.viewer_actor == *viewer_actor
                    && lease.state.session_id == *session_id
            })
            .map(|lease| lease.state.clone())
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn render_slot(&self, camera_id: &LiveCameraId) -> Result<u16, SimulationViewError> {
        self.state
            .lock()
            .expect("simulation-view state lock poisoned")
            .camera_slots
            .get(camera_id)
            .copied()
            .ok_or_else(|| SimulationViewError::CameraNotFound(camera_id.clone()))
    }

    pub(crate) fn stream_product_admission(
        &self,
        stream_id: &LiveViewId,
    ) -> Result<(LiveStreamProductId, u16, bool), SimulationViewError> {
        let state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        let stream = &state
            .leases
            .get(stream_id)
            .ok_or_else(|| SimulationViewError::LiveViewNotFound(stream_id.clone()))?
            .state;
        let render_slot = state
            .camera_slots
            .get(&stream.camera_id)
            .copied()
            .ok_or(SimulationViewError::Lifecycle)?;
        let active_demand = state
            .leases
            .values()
            .filter(|lease| lease.state.camera_id == stream.camera_id && active_lease(&lease.state))
            .count();
        Ok((
            stream.stream_product_id.clone(),
            render_slot,
            active_demand == 1,
        ))
    }

    pub(crate) fn stream_product_can_stop(&self, camera_id: &LiveCameraId) -> bool {
        let state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        state.cameras.get(camera_id).is_some_and(|camera| {
            camera.definition.stream_policy == CameraStreamPolicy::OnDemand
                && !state
                    .leases
                    .values()
                    .any(|lease| lease.state.camera_id == *camera_id && active_lease(&lease.state))
        })
    }

    pub fn get_stream(
        &self,
        owner: &LiveViewOwner,
        viewer_actor: &PrincipalId,
        stream_id: &LiveViewId,
    ) -> Result<LiveViewState, SimulationViewError> {
        let state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        let stream = &state
            .leases
            .get(stream_id)
            .ok_or_else(|| SimulationViewError::LiveViewNotFound(stream_id.clone()))?
            .state;
        if &stream.owner != owner || &stream.viewer_actor != viewer_actor {
            return Err(SimulationViewError::Ownership);
        }
        Ok(stream.clone())
    }

    fn camera_rejection(
        &self,
        state: &ServiceState,
        owner: &LiveViewOwner,
        definition: &CameraDefinition,
        excluding: Option<&LiveCameraId>,
    ) -> Option<CapacityRejection> {
        let usage = capacity_usage(state, excluding);
        let limits = &self.config.capacity;
        let streamed = u64::from(definition.stream_policy == CameraStreamPolicy::Continuous);
        let checks = [
            (
                CapacityDimension::LogicalCameras,
                u64::from(usage.logical_cameras) + 1,
                u64::from(limits.maximum_logical_cameras),
            ),
            (
                CapacityDimension::RenderedCameras,
                u64::from(usage.rendered_cameras) + 1,
                u64::from(limits.maximum_rendered_cameras),
            ),
            (
                CapacityDimension::StreamedCameras,
                u64::from(usage.streamed_cameras) + streamed,
                u64::from(limits.maximum_streamed_cameras),
            ),
            (
                CapacityDimension::RenderPixelsPerSecond,
                usage
                    .render_pixels_per_second
                    .saturating_add(definition.pixels_per_second()),
                limits.maximum_render_pixels_per_second,
            ),
            (
                CapacityDimension::NvencSessions,
                u64::from(usage.nvenc_sessions) + streamed,
                u64::from(limits.maximum_nvenc_sessions),
            ),
            (
                CapacityDimension::GpuMemoryBytes,
                usage
                    .reserved_gpu_memory_bytes
                    .saturating_add(camera_memory(definition)),
                limits.gpu_memory_budget_bytes,
            ),
        ];
        if let Some((dimension, requested, maximum)) = checks
            .into_iter()
            .find(|(_, requested, maximum)| requested > maximum)
        {
            return Some(rejection(limits, dimension, requested, maximum));
        }
        let owner_count = state
            .cameras
            .values()
            .filter(|camera| {
                camera.owner == *owner
                    && excluding.is_none_or(|excluded| &camera.camera_id != excluded)
            })
            .count() as u64
            + 1;
        if owner_count > u64::from(limits.maximum_cameras_per_owner) {
            return Some(rejection(
                limits,
                CapacityDimension::OwnerQuota,
                owner_count,
                u64::from(limits.maximum_cameras_per_owner),
            ));
        }
        let context_count = state
            .cameras
            .values()
            .filter(|camera| {
                camera.owner.work_context == owner.work_context
                    && excluding.is_none_or(|excluded| &camera.camera_id != excluded)
            })
            .count() as u64
            + 1;
        (context_count > u64::from(limits.maximum_cameras_per_work_context)).then(|| {
            rejection(
                limits,
                CapacityDimension::WorkContextQuota,
                context_count,
                u64::from(limits.maximum_cameras_per_work_context),
            )
        })
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
    session.reconciliation.next_attempt_at = None;
    session.reconciliation.failed_dependency = None;
    session.reconciliation.failure_code = None;
    session.reconciliation.diagnostic = None;
    session.updated_at = Utc::now();
}

fn validate_camera_session(
    state: &ServiceState,
    owner: &LiveViewOwner,
    session_id: &LiveSessionId,
    definition: &CameraDefinition,
) -> Result<(), SimulationViewError> {
    let session = state
        .sessions
        .get(session_id)
        .ok_or_else(|| SimulationViewError::SessionNotFound(session_id.clone()))?;
    if &session.owner != owner {
        return Err(SimulationViewError::Ownership);
    }
    let scene = session
        .scene
        .as_ref()
        .ok_or(SimulationViewError::Lifecycle)?;
    if session.lifecycle == SessionLifecycle::Closed {
        return Err(SimulationViewError::Lifecycle);
    }
    if !scene
        .body
        .allowed_camera_kinds
        .contains(&definition.rig.source())
    {
        return Err(SimulationViewError::CameraKind);
    }
    let entity_exists = |target: &veoveo_simulation_pose::EntityId| {
        scene
            .body
            .entities
            .iter()
            .any(|entity| entity.entity_id == *target)
    };
    let targets_exist = match &definition.rig {
        crate::contract::CameraRig::Orbit { target_entity, .. }
        | crate::contract::CameraRig::FollowEntity { target_entity, .. }
        | crate::contract::CameraRig::ChaseEntity { target_entity, .. }
        | crate::contract::CameraRig::MountedEntity { target_entity, .. } => {
            entity_exists(target_entity)
        }
        crate::contract::CameraRig::FormationOverview {
            target_entities, ..
        } => target_entities.iter().all(entity_exists),
        crate::contract::CameraRig::Fixed { .. } | crate::contract::CameraRig::LookAt { .. } => {
            true
        }
    };
    if !targets_exist {
        return Err(SimulationViewError::InvalidCameraRig);
    }
    Ok(())
}

fn capacity_usage(state: &ServiceState, excluding: Option<&LiveCameraId>) -> CapacityUsage {
    let mut usage = CapacityUsage::default();
    for session in state.sessions.values() {
        if !matches!(
            session.lifecycle,
            SessionLifecycle::Closed | SessionLifecycle::Failed
        ) {
            usage.entity_instances = usage.entity_instances.saturating_add(
                session
                    .scene
                    .as_ref()
                    .map_or(0, |scene| scene.body.entities.len() as u32),
            );
        }
    }
    for camera in state
        .cameras
        .values()
        .filter(|camera| excluding.is_none_or(|excluded| &camera.camera_id != excluded))
    {
        usage.logical_cameras += 1;
        usage.rendered_cameras += 1;
        usage.render_pixels_per_second = usage
            .render_pixels_per_second
            .saturating_add(camera.definition.pixels_per_second());
        usage.reserved_gpu_memory_bytes = usage
            .reserved_gpu_memory_bytes
            .saturating_add(camera_memory(&camera.definition));
        if camera.definition.stream_policy == CameraStreamPolicy::Continuous {
            usage.streamed_cameras += 1;
            usage.nvenc_sessions += 1;
        }
    }
    let demanded_on_demand_cameras = state
        .leases
        .values()
        .filter(|lease| active_lease(&lease.state))
        .filter_map(|lease| {
            state
                .cameras
                .get(&lease.state.camera_id)
                .filter(|camera| camera.definition.stream_policy == CameraStreamPolicy::OnDemand)
                .map(|_| lease.state.camera_id.clone())
        })
        .collect::<std::collections::BTreeSet<_>>();
    usage.streamed_cameras = usage
        .streamed_cameras
        .saturating_add(u32::try_from(demanded_on_demand_cameras.len()).unwrap_or(u32::MAX));
    usage.nvenc_sessions = usage
        .nvenc_sessions
        .saturating_add(u32::try_from(demanded_on_demand_cameras.len()).unwrap_or(u32::MAX));
    usage
}

fn camera_memory(definition: &CameraDefinition) -> u64 {
    u64::from(definition.width_px)
        .saturating_mul(u64::from(definition.height_px))
        .saturating_mul(16)
        .saturating_mul(3)
}

fn active_lease(stream: &LiveViewState) -> bool {
    !matches!(
        stream.lifecycle,
        LiveViewLifecycle::Closed | LiveViewLifecycle::Failed
    ) && stream.expires_at > Utc::now()
}

fn stream_product_id(camera_id: &LiveCameraId) -> LiveStreamProductId {
    let digest = Sha256::digest(camera_id.as_str().as_bytes());
    let suffix = digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    LiveStreamProductId::new(format!("product-{suffix}"))
        .expect("digest-derived stream product identifier is valid")
}

fn rejection(
    profile: &CapacityProfile,
    dimension: CapacityDimension,
    requested: u64,
    maximum: u64,
) -> CapacityRejection {
    CapacityRejection {
        dimension,
        requested,
        available: maximum,
        profile: profile.profile.clone(),
    }
}

fn selected_entity(definition: &CameraDefinition) -> Option<&str> {
    match &definition.rig {
        crate::contract::CameraRig::Orbit { target_entity, .. }
        | crate::contract::CameraRig::FollowEntity { target_entity, .. }
        | crate::contract::CameraRig::ChaseEntity { target_entity, .. }
        | crate::contract::CameraRig::MountedEntity { target_entity, .. } => {
            Some(target_entity.as_str())
        }
        _ => None,
    }
}

fn first_available_render_slot(slots: &BTreeMap<LiveCameraId, u16>, maximum: u32) -> Option<u16> {
    (0..maximum)
        .map_while(|slot| u16::try_from(slot).ok())
        .find(|candidate| !slots.values().any(|slot| slot == candidate))
}

fn new_token() -> Result<LiveViewAccessToken, SimulationViewError> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| SimulationViewError::Access)?;
    LiveViewAccessToken::new(URL_SAFE_NO_PAD.encode(bytes)).map_err(|_| SimulationViewError::Access)
}

fn token_hash(token: &LiveViewAccessToken) -> [u8; 32] {
    Sha256::digest(token.expose_for_signaling().as_bytes()).into()
}

fn expiry(now: DateTime<Utc>, duration: Duration) -> Result<DateTime<Utc>, SimulationViewError> {
    let duration = chrono::Duration::from_std(duration).map_err(|_| SimulationViewError::Time)?;
    now.checked_add_signed(duration)
        .ok_or(SimulationViewError::Time)
}

fn rotate_lease(
    config: &SimulationViewConfig,
    lease: &mut Lease,
) -> Result<LiveViewConnection, SimulationViewError> {
    let token = new_token()?;
    lease.token_hash = token_hash(&token);
    lease.state.expires_at = expiry(Utc::now(), config.lease_duration)?;
    let _ = lease.events.send(lease_signal(&lease.state));
    Ok(LiveViewConnection {
        stream: lease.state.clone(),
        access_token: token,
    })
}

fn close_lease(lease: &mut Lease) {
    lease.state.lifecycle = LiveViewLifecycle::Closed;
    lease.state.connected_viewers = 0;
    lease.token_hash.fill(0);
    lease.state.expires_at = Utc::now();
    let _ = lease.events.send(lease_signal(&lease.state));
}

fn lease_signal(state: &LiveViewState) -> LeaseSignal {
    LeaseSignal {
        active: !matches!(
            state.lifecycle,
            LiveViewLifecycle::Closed | LiveViewLifecycle::Failed
        ),
        expires_at: state.expires_at,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use veoveo_mcp_contract::{
        AccessSubject, ArtifactId, FrameId, FrameWorldId, FrameWorldRevisionId,
        FrameWorldRevisionUri, GroupId, LiveViewerInstanceId, PolicyVersion, PrincipalId, TenantId,
        WorkContextId, WorldFrameUri,
    };
    use veoveo_simulation_pose::{
        EntityId, EpochId, FrameRevision, POSE_INGRESS_CONTROL_SCHEMA, PoseIngressStatus,
        SessionId, Sha256Digest,
    };

    use super::*;
    use crate::contract::{
        AuthorizePoseProducerRequest, BindSceneRequest, CameraAdmission, CameraRecordingPolicy,
        CameraRig, GovernedArtifact, InterpolationPolicy, LocalTransform, ProducerId, PrototypeId,
        QuaternionXyzw, RendererMode, SCENE_SCHEMA, SceneAttribution, SceneDeclaration,
        SceneDeclarationBody, SceneEntity, SceneLighting, SceneQualityPolicy, Vector3,
        VisualAssetFormat, VisualPrototype,
    };

    fn view_owner(principal: &str) -> LiveViewOwner {
        let principal = PrincipalId::new(principal).unwrap();
        LiveViewOwner {
            subject: AccessSubject::Principal(principal),
            tenant: TenantId::new("tenant-a").unwrap(),
            work_context: WorkContextId::new("exercise-a").unwrap(),
            policy_revision: PolicyVersion::new("2026-07-26").unwrap(),
            data_labels: BTreeSet::new(),
        }
    }

    fn shared_view_owner(principal: &str) -> LiveViewOwner {
        let mut owner = view_owner(principal);
        owner.subject = AccessSubject::Group(GroupId::new("flight").unwrap());
        owner
    }

    fn viewer_actor() -> PrincipalId {
        PrincipalId::new("issuer#viewer").unwrap()
    }

    fn viewer_instance(value: &str) -> LiveViewerInstanceId {
        LiveViewerInstanceId::new(value).unwrap()
    }

    fn identity_transform() -> LocalTransform {
        LocalTransform {
            translation_m: Vector3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            orientation_xyzw: QuaternionXyzw {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                w: 1.0,
            },
            scale: Vector3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
        }
    }

    fn scene(session_id: LiveSessionId, epoch_id: EpochId) -> SceneDeclaration {
        let world_id = FrameWorldId::new("synthetic-world").unwrap();
        let revision_uri = FrameWorldRevisionUri::new(
            &world_id,
            &FrameWorldRevisionId::new("revision-1").unwrap(),
        );
        let simulation_frame =
            WorldFrameUri::new(&revision_uri, &FrameId::new("simulation").unwrap());
        let digest = Sha256Digest::new(format!("sha256:{}", "1".repeat(64))).unwrap();
        SceneDeclaration::from_body(SceneDeclarationBody {
            schema_version: SCENE_SCHEMA.to_owned(),
            session_id,
            epoch_id,
            frame_revision: FrameRevision {
                uri: revision_uri.to_string(),
                digest: digest.clone(),
            },
            simulation_frame,
            geospatial_layer_id: None,
            environment: GovernedArtifact {
                artifact_uri: ArtifactId::new().plane_uri(),
                digest: digest.clone(),
                format: VisualAssetFormat::Usd,
                byte_length: 1024,
            },
            prototypes: vec![VisualPrototype {
                prototype_id: PrototypeId::new("marker").unwrap(),
                asset: GovernedArtifact {
                    artifact_uri: ArtifactId::new().plane_uri(),
                    digest,
                    format: VisualAssetFormat::Glb,
                    byte_length: 512,
                },
                local_alignment: identity_transform(),
            }],
            entities: vec![SceneEntity {
                entity_id: EntityId::new("entity-1").unwrap(),
                prototype_id: PrototypeId::new("marker").unwrap(),
                static_transform: None,
            }],
            allowed_camera_kinds: vec![
                veoveo_mcp_contract::LiveCameraSource::Fixed,
                veoveo_mcp_contract::LiveCameraSource::FollowEntity,
            ],
            lighting: SceneLighting {
                sun_intensity_lux: 10_000.0,
                sky_intensity: 500.0,
                color_temperature_kelvin: 6500,
            },
            quality: SceneQualityPolicy {
                renderer: RendererMode::RaytracedLighting,
                maximum_texture_dimension: 4096,
                maximum_asset_bytes: 1_000_000,
                interpolation: InterpolationPolicy::Linear,
                maximum_pose_age_ms: 500,
            },
            attribution: vec![SceneAttribution {
                source: "Synthetic conformance fixture".to_owned(),
                license: "CC0-1.0".to_owned(),
                attribution_url: Some("https://example.test/fixture".to_owned()),
            }],
        })
        .unwrap()
    }

    fn camera_definition(stream_policy: CameraStreamPolicy) -> CameraDefinition {
        CameraDefinition {
            rig: CameraRig::FollowEntity {
                target_entity: EntityId::new("entity-1").unwrap(),
                offset_flu_m: Vector3 {
                    x: -5.0,
                    y: 0.0,
                    z: 2.0,
                },
                smoothing_seconds: 0.1,
            },
            width_px: 1280,
            height_px: 720,
            frame_rate_millihertz: 30_000,
            vertical_fov_degrees: 60.0,
            near_clip_m: 0.1,
            far_clip_m: 10_000.0,
            stream_policy,
            recording_policy: CameraRecordingPolicy::Disabled,
        }
    }

    fn bound_session(
        service: &SimulationViewService,
        owner: &LiveViewOwner,
    ) -> SimulationViewSession {
        let epoch_id = EpochId::new("epoch-1").unwrap();
        let session = service
            .create_session(
                owner.clone(),
                CreateSessionRequest {
                    session_id: LiveSessionId::new("simulation-session-1").unwrap(),
                    epoch_id: epoch_id.clone(),
                },
            )
            .unwrap();
        service
            .bind_scene(
                owner,
                BindSceneRequest {
                    session_id: session.session_id.clone(),
                    expected_revision: session.revision,
                    scene: scene(session.session_id, epoch_id),
                },
            )
            .unwrap()
    }

    #[test]
    fn caller_selected_session_identity_is_idempotent_and_owner_scoped() {
        let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
        let owner = view_owner("issuer#operator");
        let request = CreateSessionRequest {
            session_id: LiveSessionId::new("external-session-1").unwrap(),
            epoch_id: EpochId::new("epoch-1").unwrap(),
        };
        let created = service
            .create_session(owner.clone(), request.clone())
            .unwrap();
        let repeated = service
            .create_session(owner.clone(), request.clone())
            .unwrap();
        assert_eq!(created, repeated);

        assert!(matches!(
            service.create_session(
                owner,
                CreateSessionRequest {
                    session_id: request.session_id.clone(),
                    epoch_id: EpochId::new("epoch-2").unwrap(),
                },
            ),
            Err(SimulationViewError::SessionAlreadyExists(session_id))
                if session_id == request.session_id
        ));
        assert!(matches!(
            service.create_session(view_owner("issuer#other"), request),
            Err(SimulationViewError::Ownership)
        ));
    }

    #[test]
    fn work_context_output_owner_shares_state_without_crossing_contexts() {
        let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
        let automated = shared_view_owner("issuer#automation");
        let console_member = shared_view_owner("issuer#operator");
        let request = CreateSessionRequest {
            session_id: LiveSessionId::new("shared-session-1").unwrap(),
            epoch_id: EpochId::new("epoch-1").unwrap(),
        };
        let created = service.create_session(automated, request.clone()).unwrap();
        assert_eq!(
            service
                .get_session(&console_member, &request.session_id)
                .unwrap(),
            created
        );

        let mut other_context = console_member;
        other_context.work_context = WorkContextId::new("exercise-b").unwrap();
        assert!(matches!(
            service.get_session(&other_context, &request.session_id),
            Err(SimulationViewError::Ownership)
        ));
    }

    #[test]
    fn closed_session_identity_can_start_a_new_epoch() {
        let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
        let owner = view_owner("issuer#operator");
        let first = service
            .create_session(
                owner.clone(),
                CreateSessionRequest {
                    session_id: LiveSessionId::new("repeatable-session").unwrap(),
                    epoch_id: EpochId::new("epoch-1").unwrap(),
                },
            )
            .unwrap();
        let first = service
            .bind_scene(
                &owner,
                BindSceneRequest {
                    session_id: first.session_id.clone(),
                    expected_revision: first.revision,
                    scene: scene(first.session_id.clone(), first.epoch_id.clone()),
                },
            )
            .unwrap();
        let first_source = service
            .authorize_pose_producer(
                &owner,
                AuthorizePoseProducerRequest {
                    session_id: first.session_id.clone(),
                    expected_revision: first.revision,
                    producer_id: ProducerId::new("first-epoch-producer").unwrap(),
                    spiffe_id: "spiffe://veoveo.test/first-epoch-producer".to_owned(),
                    expires_at: Utc::now() + chrono::Duration::minutes(10),
                },
            )
            .unwrap();
        let first = service.get_session(&owner, &first.session_id).unwrap();
        service
            .close_session(
                &owner,
                CloseSessionRequest {
                    session_id: first.session_id.clone(),
                    expected_revision: first.revision,
                },
            )
            .unwrap();
        let restarted = service
            .create_session(
                owner,
                CreateSessionRequest {
                    session_id: first.session_id,
                    epoch_id: EpochId::new("epoch-2").unwrap(),
                },
            )
            .unwrap();
        assert_eq!(restarted.epoch_id.as_str(), "epoch-2");
        assert_eq!(restarted.lifecycle, SessionLifecycle::Created);
        assert_eq!(restarted.revision, 1);
        assert!(restarted.scene.is_none());
        assert!(restarted.pose_source.is_none());
        assert!(
            restarted.reconciliation.authorization_revision > first_source.authorization_revision
        );

        let restarted = service
            .bind_scene(
                &restarted.owner,
                BindSceneRequest {
                    session_id: restarted.session_id.clone(),
                    expected_revision: restarted.revision,
                    scene: scene(restarted.session_id.clone(), restarted.epoch_id.clone()),
                },
            )
            .unwrap();
        let second_source = service
            .authorize_pose_producer(
                &restarted.owner,
                AuthorizePoseProducerRequest {
                    session_id: restarted.session_id,
                    expected_revision: restarted.revision,
                    producer_id: ProducerId::new("second-epoch-producer").unwrap(),
                    spiffe_id: "spiffe://veoveo.test/second-epoch-producer".to_owned(),
                    expires_at: Utc::now() + chrono::Duration::minutes(10),
                },
            )
            .unwrap();
        assert!(second_source.authorization_revision > first_source.authorization_revision);
    }

    #[test]
    fn admits_multiple_cameras_and_returns_typed_capacity_rejection() {
        let mut config = SimulationViewConfig::default();
        config.capacity.maximum_logical_cameras = 2;
        config.capacity.maximum_rendered_cameras = 2;
        config.capacity.maximum_cameras_per_owner = 2;
        let service = SimulationViewService::new(config).unwrap();
        let owner = view_owner("issuer#operator");
        let session = bound_session(&service, &owner);

        for _ in 0..2 {
            assert!(matches!(
                service
                    .create_camera(
                        &owner,
                        CreateCameraRequest {
                            session_id: session.session_id.clone(),
                            definition: camera_definition(CameraStreamPolicy::OnDemand),
                        }
                    )
                    .unwrap(),
                CameraAdmission::Admitted { .. }
            ));
        }
        let admission = service
            .create_camera(
                &owner,
                CreateCameraRequest {
                    session_id: session.session_id,
                    definition: camera_definition(CameraStreamPolicy::OnDemand),
                },
            )
            .unwrap();
        assert!(matches!(
            admission,
            CameraAdmission::Rejected {
                rejection: CapacityRejection {
                    dimension: CapacityDimension::LogicalCameras,
                    requested: 3,
                    available: 2,
                    ..
                }
            }
        ));
    }

    #[test]
    fn runtime_pose_status_refreshes_the_public_health_without_exposing_transport() {
        let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
        let owner = view_owner("issuer#operator");
        let session = bound_session(&service, &owner);
        let expires_at = Utc::now() + chrono::Duration::minutes(10);
        let source = service
            .authorize_pose_producer(
                &owner,
                AuthorizePoseProducerRequest {
                    session_id: session.session_id.clone(),
                    expected_revision: session.revision,
                    producer_id: ProducerId::new("anonymous-producer").unwrap(),
                    spiffe_id: "spiffe://veoveo.test/anonymous-producer".to_owned(),
                    expires_at,
                },
            )
            .unwrap();
        assert!(source.stale);

        service
            .apply_pose_status(
                &session.session_id,
                &PoseIngressStatus {
                    schema_version: POSE_INGRESS_CONTROL_SCHEMA.to_owned(),
                    session_id: SessionId::new(session.session_id.as_str()).unwrap(),
                    epoch_id: session.epoch_id.clone(),
                    producer_id: source.producer_id.to_string(),
                    producer_spiffe_id: source.spiffe_id,
                    authorization_revision: source.authorization_revision,
                    authorized_until: expires_at,
                    revoked: false,
                    stale: false,
                    last_sequence: Some(42),
                    last_snapshot_at: Some(Utc::now()),
                },
            )
            .unwrap();

        let refreshed = service.get_session(&owner, &session.session_id).unwrap();
        let source = refreshed.pose_source.unwrap();
        assert!(!source.stale);
        assert_eq!(source.last_sequence, Some(42));
        assert!(source.last_snapshot_at.is_some());
    }

    #[test]
    fn bounded_authorization_renews_and_explicit_revocation_stops_reconciliation() {
        let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
        let owner = view_owner("issuer#operator");
        let session = bound_session(&service, &owner);
        let authorized_at = Utc::now();
        let source = service
            .authorize_pose_producer(
                &owner,
                AuthorizePoseProducerRequest {
                    session_id: session.session_id.clone(),
                    expected_revision: session.revision,
                    producer_id: ProducerId::new("renewable-producer").unwrap(),
                    spiffe_id: "spiffe://veoveo.test/renewable-producer".to_owned(),
                    expires_at: authorized_at + chrono::Duration::seconds(30),
                },
            )
            .unwrap();
        let renewed = service
            .renew_pose_authorization(
                &session.session_id,
                authorized_at + chrono::Duration::seconds(20),
            )
            .unwrap();
        assert_eq!(
            renewed.authorization_revision,
            source.authorization_revision + 1
        );
        assert_eq!(
            renewed.authorization_lifetime_seconds,
            source.authorization_lifetime_seconds
        );
        assert!((29..=30).contains(&renewed.authorization_lifetime_seconds));
        assert!(renewed.expires_at > source.expires_at);

        let current = service.get_session(&owner, &session.session_id).unwrap();
        let revoked = service
            .revoke_pose_producer(
                &owner,
                RevokePoseProducerRequest {
                    session_id: session.session_id.clone(),
                    expected_revision: current.revision,
                    producer_id: renewed.producer_id,
                },
            )
            .unwrap();
        assert!(revoked.revoked);
        assert!(matches!(
            service.renew_pose_authorization(&session.session_id, Utc::now()),
            Err(SimulationViewError::ProducerRevoked)
        ));
        let current = service.get_session(&owner, &session.session_id).unwrap();
        assert_eq!(current.reconciliation.phase, ReconciliationPhase::Revoked);
        assert_eq!(
            current.reconciliation.renewal_state,
            PoseAuthorizationRenewalState::Revoked
        );
    }

    #[test]
    fn durable_state_restores_runtime_intent_without_transient_health() {
        let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
        let owner = view_owner("issuer#operator");
        let session = bound_session(&service, &owner);
        let source = service
            .authorize_pose_producer(
                &owner,
                AuthorizePoseProducerRequest {
                    session_id: session.session_id.clone(),
                    expected_revision: session.revision,
                    producer_id: ProducerId::new("durable-producer").unwrap(),
                    spiffe_id: "spiffe://veoveo.test/durable-producer".to_owned(),
                    expires_at: Utc::now() + chrono::Duration::minutes(10),
                },
            )
            .unwrap();
        let CameraAdmission::Admitted { camera } = service
            .create_camera(
                &owner,
                CreateCameraRequest {
                    session_id: session.session_id.clone(),
                    definition: camera_definition(CameraStreamPolicy::OnDemand),
                },
            )
            .unwrap()
        else {
            panic!("camera should be admitted");
        };
        let stream = service
            .open_live_view(
                &owner,
                &viewer_actor(),
                OpenLiveViewRequest {
                    session_id: session.session_id.clone(),
                    camera_id: camera.camera_id.clone(),
                    viewer_instance_id: viewer_instance("browser-1"),
                },
            )
            .unwrap();
        let desired = service.durable_state(&session.session_id).unwrap();

        let restored = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
        restored.restore_durable_state(desired).unwrap();
        let restored_session = restored.get_session(&owner, &session.session_id).unwrap();
        assert_eq!(restored_session.lifecycle, SessionLifecycle::SceneBound);
        assert_eq!(
            restored_session.reconciliation.phase,
            ReconciliationPhase::Pending
        );
        assert!(!restored.reconciliation_ready());
        restored.mark_reconciliation_phase(
            &session.session_id,
            ReconciliationPhase::RendererSession,
            None,
        );
        assert!(!restored.reconciliation_ready());
        assert_eq!(
            restored_session
                .pose_source
                .as_ref()
                .unwrap()
                .authorization_revision,
            source.authorization_revision
        );
        assert!(restored_session.pose_source.as_ref().unwrap().stale);
        assert_eq!(
            restored
                .get_camera(&owner, &camera.camera_id)
                .unwrap()
                .health,
            LiveCameraHealth::Warming
        );
        assert!(matches!(
            restored.get_stream(&owner, &viewer_actor(), &stream.stream.live_view_id),
            Err(SimulationViewError::LiveViewNotFound(_))
        ));
    }

    #[test]
    fn blocked_reconciliation_waits_without_hiding_degraded_state() {
        let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
        let owner = view_owner("issuer#durability-operator");
        let session = bound_session(&service, &owner);
        service.mark_reconciliation_failed(
            &session.session_id,
            "store",
            ReconciliationFailureCode::StoreUnavailable,
            "durable state is unavailable",
            Utc::now() + chrono::Duration::minutes(1),
        );

        assert!(service.reconciliation_sessions().is_empty());
        assert!(!service.reconciliation_ready());
        let blocked = service.get_session(&owner, &session.session_id).unwrap();
        assert_eq!(blocked.reconciliation.phase, ReconciliationPhase::Blocked);
        assert_eq!(
            blocked.reconciliation.failure_code,
            Some(ReconciliationFailureCode::StoreUnavailable)
        );
        assert_eq!(
            blocked.reconciliation.failed_dependency.as_deref(),
            Some("store")
        );

        service.mark_reconciliation_failed(
            &session.session_id,
            "store",
            ReconciliationFailureCode::StoreUnavailable,
            "durable state is unavailable",
            Utc::now() - chrono::Duration::seconds(1),
        );
        assert_eq!(service.reconciliation_sessions().len(), 1);
    }

    #[test]
    fn routine_reconciliation_keeps_converged_durable_state_ready() {
        let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
        let owner = view_owner("issuer#durability-operator");
        let session = bound_session(&service, &owner);
        service.mark_reconciliation_healthy(&session.session_id, Utc::now());

        assert!(service.reconciliation_ready());
        service.mark_reconciliation_phase(
            &session.session_id,
            ReconciliationPhase::RendererSession,
            None,
        );

        assert!(service.reconciliation_ready());
        let current = service.get_session(&owner, &session.session_id).unwrap();
        assert_eq!(current.reconciliation.phase, ReconciliationPhase::Healthy);
        assert_eq!(
            current.reconciliation.desired_revision,
            current.reconciliation.realized_revision
        );
    }

    #[test]
    fn lease_tokens_are_rotated_and_checked_without_uri_authority() {
        let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
        let owner = view_owner("issuer#operator");
        let session = bound_session(&service, &owner);
        let CameraAdmission::Admitted { camera } = service
            .create_camera(
                &owner,
                CreateCameraRequest {
                    session_id: session.session_id.clone(),
                    definition: camera_definition(CameraStreamPolicy::OnDemand),
                },
            )
            .unwrap()
        else {
            panic!("camera should be admitted");
        };
        let first = service
            .open_live_view(
                &owner,
                &viewer_actor(),
                OpenLiveViewRequest {
                    session_id: session.session_id.clone(),
                    camera_id: camera.camera_id,
                    viewer_instance_id: viewer_instance("browser-1"),
                },
            )
            .unwrap();
        assert!(
            service
                .authorize_signaling(&first.stream.live_view_id, "not-the-token")
                .is_err()
        );
        let second = service
            .renew_live_view(
                &owner,
                &viewer_actor(),
                RenewLiveViewRequest {
                    session_id: session.session_id,
                    live_view_id: first.stream.live_view_id.clone(),
                    viewer_instance_id: viewer_instance("browser-1"),
                },
            )
            .unwrap();
        assert_ne!(
            first.access_token.expose_for_signaling(),
            second.access_token.expose_for_signaling()
        );
        assert!(
            service
                .authorize_signaling(
                    &second.stream.live_view_id,
                    first.access_token.expose_for_signaling()
                )
                .is_err()
        );
        assert!(
            service
                .authorize_signaling(
                    &second.stream.live_view_id,
                    second.access_token.expose_for_signaling()
                )
                .is_ok()
        );
    }

    #[test]
    fn shared_camera_uses_distinct_actor_leases_and_one_stream_product() {
        let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
        let owner = shared_view_owner("issuer#session-creator");
        let session = bound_session(&service, &owner);
        let CameraAdmission::Admitted { camera } = service
            .create_camera(
                &owner,
                CreateCameraRequest {
                    session_id: session.session_id.clone(),
                    definition: camera_definition(CameraStreamPolicy::OnDemand),
                },
            )
            .unwrap()
        else {
            panic!("camera should be admitted");
        };
        let actor_a = PrincipalId::new("issuer#viewer-a").unwrap();
        let actor_b = PrincipalId::new("issuer#viewer-b").unwrap();
        let first = service
            .open_live_view(
                &owner,
                &actor_a,
                OpenLiveViewRequest {
                    session_id: session.session_id.clone(),
                    camera_id: camera.camera_id.clone(),
                    viewer_instance_id: viewer_instance("browser-a"),
                },
            )
            .unwrap();
        let second = service
            .open_live_view(
                &owner,
                &actor_b,
                OpenLiveViewRequest {
                    session_id: session.session_id.clone(),
                    camera_id: camera.camera_id,
                    viewer_instance_id: viewer_instance("browser-b"),
                },
            )
            .unwrap();

        assert_ne!(first.stream.live_view_id, second.stream.live_view_id);
        assert_eq!(
            first.stream.stream_product_id,
            second.stream.stream_product_id
        );
        assert_eq!(service.capacity().usage.streamed_cameras, 1);
        assert_eq!(service.capacity().usage.nvenc_sessions, 1);
        assert!(
            service
                .authorize_signaling(
                    &first.stream.live_view_id,
                    first.access_token.expose_for_signaling()
                )
                .is_ok()
        );
        assert!(
            service
                .authorize_signaling(
                    &second.stream.live_view_id,
                    second.access_token.expose_for_signaling()
                )
                .is_ok()
        );
        assert!(matches!(
            service.close_live_view(
                &owner,
                &actor_b,
                CloseLiveViewRequest {
                    session_id: session.session_id.clone(),
                    live_view_id: first.stream.live_view_id.clone(),
                    viewer_instance_id: viewer_instance("browser-b"),
                },
            ),
            Err(SimulationViewError::Ownership)
        ));
        service
            .close_live_view(
                &owner,
                &actor_b,
                CloseLiveViewRequest {
                    session_id: session.session_id,
                    live_view_id: second.stream.live_view_id,
                    viewer_instance_id: viewer_instance("browser-b"),
                },
            )
            .unwrap();
        assert!(active_lease(
            &service
                .get_stream(&owner, &actor_a, &first.stream.live_view_id)
                .unwrap()
        ));
    }

    #[test]
    fn physical_render_slots_drive_unique_media_ports() {
        let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
        let owner = view_owner("issuer#operator");
        let session = bound_session(&service, &owner);
        let mut cameras = Vec::new();
        for _ in 0..2 {
            let CameraAdmission::Admitted { camera } = service
                .create_camera(
                    &owner,
                    CreateCameraRequest {
                        session_id: session.session_id.clone(),
                        definition: camera_definition(CameraStreamPolicy::OnDemand),
                    },
                )
                .unwrap()
            else {
                panic!("camera should be admitted");
            };
            cameras.push(camera);
        }
        assert_eq!(service.render_slot(&cameras[0].camera_id).unwrap(), 0);
        assert_eq!(service.render_slot(&cameras[1].camera_id).unwrap(), 1);
        let streams = cameras
            .into_iter()
            .map(|camera| {
                service
                    .open_live_view(
                        &owner,
                        &viewer_actor(),
                        OpenLiveViewRequest {
                            session_id: session.session_id.clone(),
                            camera_id: camera.camera_id,
                            viewer_instance_id: viewer_instance("browser-1"),
                        },
                    )
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(streams[0].stream.endpoint.media_port, 47998);
        assert_eq!(streams[1].stream.endpoint.media_port, 47999);
    }

    #[test]
    fn cancelled_stream_admission_does_not_change_durable_intent() {
        let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
        let owner = view_owner("issuer#operator");
        let session = bound_session(&service, &owner);
        let CameraAdmission::Admitted { camera } = service
            .create_camera(
                &owner,
                CreateCameraRequest {
                    session_id: session.session_id.clone(),
                    definition: camera_definition(CameraStreamPolicy::OnDemand),
                },
            )
            .unwrap()
        else {
            panic!("camera should be admitted");
        };
        let opened = service
            .open_live_view(
                &owner,
                &viewer_actor(),
                OpenLiveViewRequest {
                    session_id: session.session_id.clone(),
                    camera_id: camera.camera_id,
                    viewer_instance_id: viewer_instance("browser-1"),
                },
            )
            .unwrap();
        let revision_after_open = service
            .get_session(&owner, &session.session_id)
            .unwrap()
            .reconciliation
            .desired_revision;

        service.cancel_stream_admission(&opened.stream.live_view_id);

        let cancelled = service
            .get_stream(&owner, &viewer_actor(), &opened.stream.live_view_id)
            .unwrap();
        assert_eq!(cancelled.lifecycle, LiveViewLifecycle::Closed);
        assert_eq!(cancelled.connected_viewers, 0);
        let desired = service.durable_state(&session.session_id).unwrap();
        assert_eq!(
            desired.session.reconciliation.desired_revision,
            revision_after_open
        );
    }

    #[test]
    fn changing_camera_revision_closes_existing_stream() {
        let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
        let owner = view_owner("issuer#operator");
        let session = bound_session(&service, &owner);
        let CameraAdmission::Admitted { camera } = service
            .create_camera(
                &owner,
                CreateCameraRequest {
                    session_id: session.session_id.clone(),
                    definition: camera_definition(CameraStreamPolicy::OnDemand),
                },
            )
            .unwrap()
        else {
            panic!("camera should be admitted");
        };
        let stream = service
            .open_live_view(
                &owner,
                &viewer_actor(),
                OpenLiveViewRequest {
                    session_id: session.session_id.clone(),
                    camera_id: camera.camera_id.clone(),
                    viewer_instance_id: viewer_instance("browser-1"),
                },
            )
            .unwrap();
        service
            .set_camera(
                &owner,
                SetCameraRequest {
                    session_id: session.session_id,
                    camera_id: camera.camera_id,
                    expected_revision: camera.revision,
                    definition: camera_definition(CameraStreamPolicy::Continuous),
                },
            )
            .unwrap();
        assert_eq!(
            service
                .get_stream(&owner, &viewer_actor(), &stream.stream.live_view_id)
                .unwrap()
                .lifecycle,
            LiveViewLifecycle::Closed
        );
    }

    #[test]
    fn scene_binding_is_immutable_and_owner_scoped() {
        let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
        let owner = view_owner("issuer#operator");
        let other = view_owner("issuer#other");
        let session = bound_session(&service, &owner);
        assert!(matches!(
            service.get_session(&other, &session.session_id),
            Err(SimulationViewError::Ownership)
        ));

        let mut replacement = scene(session.session_id.clone(), session.epoch_id.clone());
        replacement.body.attribution[0].source = "Different".to_owned();
        replacement = SceneDeclaration::from_body(replacement.body).unwrap();
        assert!(matches!(
            service.bind_scene(
                &owner,
                BindSceneRequest {
                    session_id: session.session_id,
                    expected_revision: session.revision,
                    scene: replacement,
                }
            ),
            Err(SimulationViewError::SceneAlreadyBound)
        ));
    }
}
