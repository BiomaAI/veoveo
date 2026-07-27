use std::{
    collections::BTreeMap,
    net::{IpAddr, Ipv4Addr},
    sync::{Arc, Mutex},
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
    LiveSessionId, LiveViewAccessToken, LiveViewCodec, LiveViewConnection, LiveViewHardwareEncoder,
    LiveViewId, LiveViewLifecycle, LiveViewOwner, LiveViewState, LiveViewUri,
};
use veoveo_simulation_pose::PoseIngressStatus;

use crate::{
    contract::{
        AuthorizePoseProducerRequest, BindSceneRequest, CameraAdmission, CameraDefinition,
        CameraRecord, CameraStreamPolicy, CapacityDimension, CapacityProfile, CapacityRejection,
        CapacityState, CapacityUsage, CloseCameraRequest, CloseLiveViewRequest, CloseResult,
        CloseSessionRequest, CreateCameraRequest, CreateSessionRequest, OpenLiveViewRequest,
        PoseSourceState, RenewLiveViewRequest, RevokePoseProducerRequest, SessionLifecycle,
        SetCameraRequest, SimulationViewError, SimulationViewSession,
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
        }
    }
}

#[derive(Debug)]
struct Lease {
    state: LiveViewState,
    token_hash: [u8; 32],
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
        }))
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
        }
        let now = Utc::now();
        let session = SimulationViewSession {
            session_id: request.session_id,
            epoch_id: request.epoch_id,
            owner,
            lifecycle: SessionLifecycle::Created,
            revision: 1,
            scene: None,
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
        let source = PoseSourceState {
            producer_id: request.producer_id,
            spiffe_id: request.spiffe_id,
            authorized_at: now,
            expires_at: request.expires_at,
            revoked: false,
            last_sequence: None,
            last_snapshot_at: None,
            stale: true,
        };
        session.pose_source = Some(source.clone());
        advance_session(session);
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
        let result = source.clone();
        advance_session(session);
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
        Ok(CameraAdmission::Admitted {
            camera: Box::new(camera.clone()),
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
        Ok(CloseResult {
            resource_uri: uris::camera(&request.session_id, &request.camera_id),
            closed: true,
        })
    }

    pub fn open_live_view(
        &self,
        owner: &LiveViewOwner,
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
        if camera.definition.stream_policy == CameraStreamPolicy::OnDemand {
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
            resource_uri: LiveViewUri::new(uris::stream(&request.session_id, &live_view_id))
                .map_err(|_| SimulationViewError::Access)?,
            owner: owner.clone(),
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
        state.leases.insert(
            live_view_id,
            Lease {
                state: stream.clone(),
                token_hash: token_hash(&token),
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
        if &lease.state.owner != owner {
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
        if &lease.state.owner != owner {
            return Err(SimulationViewError::Ownership);
        }
        close_lease(lease);
        Ok(CloseResult {
            resource_uri: lease.state.resource_uri.as_str().to_owned(),
            closed: true,
        })
    }

    pub fn authorize_signaling(
        &self,
        live_view_id: &LiveViewId,
        token: &str,
    ) -> Result<LiveViewState, SimulationViewError> {
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
        Ok(lease.state.clone())
    }

    pub fn signaling_active(&self, live_view_id: &LiveViewId) -> bool {
        self.state
            .lock()
            .expect("simulation-view state lock poisoned")
            .leases
            .get(live_view_id)
            .is_some_and(|lease| {
                lease.state.expires_at > Utc::now()
                    && matches!(
                        lease.state.lifecycle,
                        LiveViewLifecycle::Ready | LiveViewLifecycle::Live
                    )
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

    pub(crate) fn fail_session(&self, session_id: &LiveSessionId) {
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        if let Some(session) = state.sessions.get_mut(session_id) {
            session.lifecycle = SessionLifecycle::Failed;
            if let Some(source) = session.pose_source.as_mut() {
                source.revoked = true;
                source.stale = true;
            }
            advance_session(session);
        }
        for camera in state
            .cameras
            .values_mut()
            .filter(|camera| camera.session_id == *session_id)
        {
            camera.health = LiveCameraHealth::Failed;
            camera.updated_at = Utc::now();
        }
        for lease in state
            .leases
            .values_mut()
            .filter(|lease| lease.state.session_id == *session_id)
        {
            lease.state.lifecycle = LiveViewLifecycle::Failed;
            lease.state.connected_viewers = 0;
            lease.token_hash.fill(0);
            lease.state.expires_at = Utc::now();
        }
    }

    pub(crate) fn fail_camera(&self, camera_id: &LiveCameraId) {
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        if let Some(camera) = state.cameras.get_mut(camera_id) {
            camera.health = LiveCameraHealth::Failed;
            camera.updated_at = Utc::now();
        }
        for lease in state
            .leases
            .values_mut()
            .filter(|lease| lease.state.camera_id == *camera_id)
        {
            lease.state.lifecycle = LiveViewLifecycle::Failed;
            lease.state.connected_viewers = 0;
            lease.token_hash.fill(0);
            lease.state.expires_at = Utc::now();
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
            || status.authorized_until != source.expires_at
        {
            return Err(SimulationViewError::Producer);
        }
        source.last_sequence = status.last_sequence;
        source.last_snapshot_at = status.last_snapshot_at;
        source.stale = status.stale;
        Ok(())
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

    pub(crate) fn mark_stream_ready(&self, stream_id: &LiveViewId) {
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        if let Some(lease) = state.leases.get_mut(stream_id) {
            lease.state.lifecycle = LiveViewLifecycle::Ready;
        }
    }

    pub(crate) fn abort_stream(&self, stream_id: &LiveViewId) {
        let mut state = self
            .state
            .lock()
            .expect("simulation-view state lock poisoned");
        if let Some(lease) = state.leases.get_mut(stream_id) {
            lease.state.lifecycle = LiveViewLifecycle::Failed;
            lease.state.connected_viewers = 0;
            lease.token_hash.fill(0);
            lease.state.expires_at = Utc::now();
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
        session_id: &LiveSessionId,
    ) -> Vec<LiveViewState> {
        self.state
            .lock()
            .expect("simulation-view state lock poisoned")
            .leases
            .values()
            .filter(|lease| lease.state.owner == *owner && lease.state.session_id == *session_id)
            .map(|lease| lease.state.clone())
            .collect()
    }

    pub(crate) fn render_slot(&self, camera_id: &LiveCameraId) -> Result<u16, SimulationViewError> {
        self.state
            .lock()
            .expect("simulation-view state lock poisoned")
            .camera_slots
            .get(camera_id)
            .copied()
            .ok_or_else(|| SimulationViewError::CameraNotFound(camera_id.clone()))
    }

    pub fn get_stream(
        &self,
        owner: &LiveViewOwner,
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
        if &stream.owner != owner {
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
    for lease in state.leases.values().filter(|lease| {
        !matches!(
            lease.state.lifecycle,
            LiveViewLifecycle::Closed | LiveViewLifecycle::Failed
        ) && lease.state.expires_at > Utc::now()
    }) {
        if state
            .cameras
            .get(&lease.state.camera_id)
            .is_some_and(|camera| camera.definition.stream_policy == CameraStreamPolicy::OnDemand)
        {
            usage.streamed_cameras += 1;
            usage.nvenc_sessions += 1;
        }
    }
    usage
}

fn camera_memory(definition: &CameraDefinition) -> u64 {
    u64::from(definition.width_px)
        .saturating_mul(u64::from(definition.height_px))
        .saturating_mul(16)
        .saturating_mul(3)
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
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use veoveo_mcp_contract::{
        AccessSubject, ArtifactId, FrameId, FrameWorldId, FrameWorldRevisionId,
        FrameWorldRevisionUri, GroupId, PolicyVersion, PrincipalId, TenantId, WorkContextId,
        WorldFrameUri,
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
                intensity_lux: 10_000.0,
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
                    authorized_until: expires_at,
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
                OpenLiveViewRequest {
                    session_id: session.session_id.clone(),
                    camera_id: camera.camera_id,
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
                RenewLiveViewRequest {
                    session_id: session.session_id,
                    live_view_id: first.stream.live_view_id.clone(),
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
                        OpenLiveViewRequest {
                            session_id: session.session_id.clone(),
                            camera_id: camera.camera_id,
                        },
                    )
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(streams[0].stream.endpoint.media_port, 47998);
        assert_eq!(streams[1].stream.endpoint.media_port, 47999);
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
                OpenLiveViewRequest {
                    session_id: session.session_id.clone(),
                    camera_id: camera.camera_id.clone(),
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
                .get_stream(&owner, &stream.stream.live_view_id)
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
