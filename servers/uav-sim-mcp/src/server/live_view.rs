use std::{collections::BTreeMap, sync::Arc, time::Duration};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tokio::sync::{Mutex, watch};
use uuid::Uuid;
use veoveo_mcp_contract::{
    LIVE_VIEW_SCHEMA, LiveCameraId, LiveCameraRig, LiveCameraStreamPolicy, LiveColorMatrix,
    LiveColorMetadata, LiveColorPrimaries, LiveColorRange, LiveColorTransfer, LiveMediaEndpoint,
    LiveMediaTransport, LiveSessionId, LiveViewAccessToken, LiveViewCodec, LiveViewConnection,
    LiveViewHardwareEncoder, LiveViewId, LiveViewLifecycle, LiveViewOwner, LiveViewState,
    LiveViewUri, PrincipalId,
};

use crate::{
    adapter::Adapter,
    contract::{
        CloseLiveViewRequest, CloseLiveViewResult, OpenLiveViewRequest, RenewLiveViewRequest,
        SimulationState,
    },
    server::live_view_audit::LiveViewAudit,
};

#[derive(Debug, Clone)]
struct ViewerSession {
    state: LiveViewState,
    token_hash: [u8; 32],
    generation: u64,
    events: watch::Sender<ViewerSignal>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ViewerSignal {
    pub(super) active: bool,
    pub(super) expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub(super) struct StreamAuthorization {
    pub(super) state: LiveViewState,
    pub(super) events: watch::Receiver<ViewerSignal>,
}

#[derive(Debug)]
struct LiveViewStateStore {
    sessions: BTreeMap<LiveViewId, ViewerSession>,
}

#[derive(Debug, Clone)]
pub(super) struct LiveViewConfig {
    pub(super) session_duration: Duration,
    pub(super) public_stream_url: String,
    pub(super) maximum_frame_age_ms: u32,
}

pub(super) struct LiveViewService {
    adapter: Arc<Adapter>,
    audit: Option<Arc<LiveViewAudit>>,
    config: LiveViewConfig,
    state: Mutex<LiveViewStateStore>,
}

impl LiveViewService {
    pub(super) fn new(
        adapter: Arc<Adapter>,
        audit: Arc<LiveViewAudit>,
        config: LiveViewConfig,
    ) -> anyhow::Result<Arc<Self>> {
        Self::build(adapter, Some(audit), config)
    }

    #[cfg(test)]
    fn new_for_test(adapter: Arc<Adapter>, config: LiveViewConfig) -> anyhow::Result<Arc<Self>> {
        Self::build(adapter, None, config)
    }

    fn build(
        adapter: Arc<Adapter>,
        audit: Option<Arc<LiveViewAudit>>,
        config: LiveViewConfig,
    ) -> anyhow::Result<Arc<Self>> {
        anyhow::ensure!(
            !config.session_duration.is_zero(),
            "live-view session duration must be positive"
        );
        anyhow::ensure!(
            config.maximum_frame_age_ms > 0,
            "maximum live-view frame age must be positive"
        );
        LiveMediaEndpoint {
            transport: LiveMediaTransport::WebSocketH264,
            stream_url: config.public_stream_url.clone(),
        }
        .validate()?;
        Ok(Arc::new(Self {
            adapter,
            audit,
            config,
            state: Mutex::new(LiveViewStateStore {
                sessions: BTreeMap::new(),
            }),
        }))
    }

    pub(super) async fn open(
        self: &Arc<Self>,
        owner: LiveViewOwner,
        viewer_actor: PrincipalId,
        request: OpenLiveViewRequest,
    ) -> Result<LiveViewConnection, LiveViewError> {
        let mut state = self.state.lock().await;
        if let Some(existing_id) = state.sessions.iter().find_map(|(id, session)| {
            (active(&session.state)
                && session.state.owner == owner
                && session.state.viewer_actor == viewer_actor
                && session.state.viewer_instance_id == request.viewer_instance_id
                && session.state.camera_id == request.camera_id)
                .then(|| id.clone())
        }) {
            let connection = rotate(
                &self.config,
                state
                    .sessions
                    .get_mut(&existing_id)
                    .expect("session exists"),
            )?;
            let generation = state.sessions[&existing_id].generation;
            drop(state);
            self.arm_expiry(existing_id, generation, connection.stream.expires_at);
            return Ok(connection);
        }

        let simulation = self
            .adapter
            .state()
            .await
            .map_err(|error| LiveViewError::Runtime(error.to_string()))?;
        if simulation.session_id.as_str() != request.session_id.as_str() {
            return Err(LiveViewError::SessionNotFound(request.session_id));
        }
        let camera = simulation
            .live_cameras
            .iter()
            .find(|camera| camera.camera_id == request.camera_id)
            .cloned()
            .ok_or_else(|| LiveViewError::CameraNotFound(request.camera_id.clone()))?;
        if camera.stream_policy == LiveCameraStreamPolicy::Disabled {
            return Err(LiveViewError::CameraUnavailable);
        }
        let product = simulation
            .stream_products
            .iter()
            .find(|product| product.camera_id == camera.camera_id)
            .cloned()
            .ok_or(LiveViewError::CameraUnavailable)?;
        if product.lifecycle == veoveo_mcp_contract::LiveStreamProductLifecycle::Failed {
            return Err(LiveViewError::CameraUnavailable);
        }

        let now = Utc::now();
        let expires_at = expiry(now, self.config.session_duration)?;
        let live_view_id = LiveViewId::new(format!("view-{}", Uuid::now_v7()))
            .map_err(|_| LiveViewError::Identifier)?;
        let token = new_token()?;
        let resource_uri = LiveViewUri::new(format!(
            "uav-sim://session/{}/live-view/{live_view_id}",
            request.session_id
        ))
        .map_err(|_| LiveViewError::Identifier)?;
        let stream = LiveViewState {
            schema_version: LIVE_VIEW_SCHEMA.to_owned(),
            live_view_id: live_view_id.clone(),
            stream_product_id: product.stream_product_id.clone(),
            resource_uri,
            owner,
            viewer_actor,
            viewer_instance_id: request.viewer_instance_id,
            session_id: request.session_id,
            camera_id: camera.camera_id.clone(),
            lifecycle: product_lifecycle(&product),
            semantic_source: camera.rig.source(),
            selected_entity_id: selected_entity(&camera.rig),
            camera_revision: camera.revision,
            codec: LiveViewCodec::H264,
            hardware_encoder: LiveViewHardwareEncoder::NvidiaNvenc,
            color: LiveColorMetadata {
                primaries: LiveColorPrimaries::Bt709,
                transfer: LiveColorTransfer::Bt709,
                matrix: LiveColorMatrix::Bt709,
                range: LiveColorRange::Limited,
            },
            width_px: camera.width_px,
            height_px: camera.height_px,
            frame_rate_millihertz: camera.frame_rate_millihertz,
            connected_viewers: 0,
            camera_health: camera.health,
            last_frame_at: product.last_frame_at.or(camera.last_frame_at),
            source_to_render_p95_microseconds: product.source_to_render_p95_microseconds,
            source_to_render_samples: product.source_to_render_samples,
            maximum_frame_age_ms: self.config.maximum_frame_age_ms,
            endpoint: LiveMediaEndpoint {
                transport: LiveMediaTransport::WebSocketH264,
                stream_url: self.config.public_stream_url.clone(),
            },
            created_at: now,
            expires_at,
        };
        stream.validate().map_err(|_| LiveViewError::Contract)?;
        let (events, _) = watch::channel(signal(&stream));
        state.sessions.insert(
            live_view_id.clone(),
            ViewerSession {
                state: stream.clone(),
                token_hash: token_hash(&token),
                generation: 1,
                events,
            },
        );
        drop(state);
        self.arm_expiry(live_view_id, 1, expires_at);
        Ok(LiveViewConnection {
            stream,
            access_token: token,
        })
    }

    pub(super) async fn renew(
        self: &Arc<Self>,
        owner: &LiveViewOwner,
        viewer_actor: &PrincipalId,
        request: RenewLiveViewRequest,
    ) -> Result<LiveViewConnection, LiveViewError> {
        let mut state = self.state.lock().await;
        let session = state
            .sessions
            .get_mut(&request.live_view_id)
            .filter(|session| session.state.session_id == request.session_id)
            .ok_or_else(|| LiveViewError::ViewNotFound(request.live_view_id.clone()))?;
        if session.state.viewer_actor != *viewer_actor
            || session.state.viewer_instance_id != request.viewer_instance_id
        {
            return Err(LiveViewError::Ownership);
        }
        if !active(&session.state) {
            return Err(LiveViewError::ViewUnavailable);
        }
        if session.state.owner != *owner {
            close_session(session);
            return Err(LiveViewError::AuthorityRevoked);
        }
        let connection = rotate(&self.config, session)?;
        let generation = session.generation;
        drop(state);
        self.arm_expiry(
            request.live_view_id,
            generation,
            connection.stream.expires_at,
        );
        Ok(connection)
    }

    pub(super) async fn close(
        self: &Arc<Self>,
        owner: &LiveViewOwner,
        viewer_actor: &PrincipalId,
        request: CloseLiveViewRequest,
    ) -> Result<CloseLiveViewResult, LiveViewError> {
        let mut state = self.state.lock().await;
        let session = state
            .sessions
            .get_mut(&request.live_view_id)
            .filter(|session| session.state.session_id == request.session_id)
            .ok_or_else(|| LiveViewError::ViewNotFound(request.live_view_id.clone()))?;
        authorize_owner(session, owner, viewer_actor, &request.viewer_instance_id)?;
        let resource_uri = session.state.resource_uri.as_str().to_owned();
        close_session(session);
        Ok(CloseLiveViewResult {
            resource_uri,
            closed: true,
        })
    }

    pub(super) async fn list(
        &self,
        owner: &LiveViewOwner,
        viewer_actor: &PrincipalId,
        session_id: &LiveSessionId,
    ) -> Vec<LiveViewState> {
        let state = self.state.lock().await;
        state
            .sessions
            .values()
            .filter(|session| {
                session.state.owner == *owner
                    && session.state.viewer_actor == *viewer_actor
                    && session.state.session_id == *session_id
            })
            .map(|session| session.state.clone())
            .collect()
    }

    pub(super) async fn get(
        &self,
        owner: &LiveViewOwner,
        viewer_actor: &PrincipalId,
        live_view_id: &LiveViewId,
    ) -> Result<LiveViewState, LiveViewError> {
        let state = self.state.lock().await;
        let session = state
            .sessions
            .get(live_view_id)
            .ok_or_else(|| LiveViewError::ViewNotFound(live_view_id.clone()))?;
        if session.state.owner != *owner || session.state.viewer_actor != *viewer_actor {
            return Err(LiveViewError::Ownership);
        }
        let mut result = session.state.clone();
        drop(state);
        if active(&result) {
            let simulation = self
                .adapter
                .state()
                .await
                .map_err(|error| LiveViewError::Runtime(error.to_string()))?;
            let product = simulation
                .stream_products
                .iter()
                .find(|product| {
                    product.stream_product_id == result.stream_product_id
                        && product.camera_id == result.camera_id
                })
                .ok_or(LiveViewError::ViewUnavailable)?;
            result.lifecycle = product_lifecycle(product);
            result.last_frame_at = product.last_frame_at;
            result.source_to_render_p95_microseconds = product.source_to_render_p95_microseconds;
            result.source_to_render_samples = product.source_to_render_samples;
            if let Some(camera) = simulation
                .live_cameras
                .iter()
                .find(|camera| camera.camera_id == result.camera_id)
            {
                result.camera_health = camera.health;
            }
            result.validate().map_err(|_| LiveViewError::Contract)?;
        }
        Ok(result)
    }

    pub(super) async fn project_product_usage(&self, simulation: &mut SimulationState) {
        let state = self.state.lock().await;
        for product in &mut simulation.stream_products {
            let sessions = state.sessions.values().filter(|session| {
                active(&session.state)
                    && session.state.stream_product_id == product.stream_product_id
            });
            let (active_viewers, connected_viewers) = sessions.fold(
                (0_u32, 0_u32),
                |(active_viewers, connected_viewers), session| {
                    (
                        active_viewers.saturating_add(1),
                        connected_viewers.saturating_add(session.state.connected_viewers),
                    )
                },
            );
            product.active_viewers = active_viewers;
            product.connected_viewers = connected_viewers;
        }
    }

    pub(super) async fn authorize_stream(
        &self,
        live_view_id: &LiveViewId,
        token: &str,
    ) -> Result<StreamAuthorization, LiveViewError> {
        let supplied: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        let mut state = self.state.lock().await;
        let session = state
            .sessions
            .get_mut(live_view_id)
            .ok_or_else(|| LiveViewError::ViewNotFound(live_view_id.clone()))?;
        if !active(&session.state) || supplied.ct_eq(&session.token_hash).unwrap_u8() != 1 {
            return Err(LiveViewError::Access);
        }
        if session.state.connected_viewers != 0 {
            return Err(LiveViewError::Access);
        }
        session.state.connected_viewers = 1;
        session.state.lifecycle = LiveViewLifecycle::Live;
        Ok(StreamAuthorization {
            state: session.state.clone(),
            events: session.events.subscribe(),
        })
    }

    pub(super) async fn finish_stream(&self, live_view_id: &LiveViewId) {
        let mut state = self.state.lock().await;
        let Some(session) = state.sessions.get_mut(live_view_id) else {
            return;
        };
        if active(&session.state) {
            session.state.connected_viewers = 0;
            session.state.lifecycle = LiveViewLifecycle::Ready;
        }
    }

    fn arm_expiry(
        self: &Arc<Self>,
        live_view_id: LiveViewId,
        generation: u64,
        expires_at: DateTime<Utc>,
    ) {
        let service = self.clone();
        tokio::spawn(async move {
            let wait = (expires_at - Utc::now()).to_std().unwrap_or_default();
            tokio::time::sleep(wait).await;
            let mut state = service.state.lock().await;
            let Some(session) = state.sessions.get_mut(&live_view_id) else {
                return;
            };
            if session.generation != generation || session.state.expires_at > Utc::now() {
                return;
            }
            close_session(session);
            let expired = session.state.clone();
            drop(state);
            if let Some(audit) = &service.audit
                && let Err(error) = audit
                    .append_authorization(
                        &expired,
                        "expired",
                        veoveo_platform_store::AuditOutcome::Allowed,
                        BTreeMap::new(),
                    )
                    .await
            {
                tracing::error!(%error, live_view_id = %expired.live_view_id, "failed to persist live-view expiry audit");
            }
        });
    }
}

fn authorize_owner(
    session: &ViewerSession,
    owner: &LiveViewOwner,
    viewer_actor: &PrincipalId,
    viewer_instance_id: &veoveo_mcp_contract::LiveViewerInstanceId,
) -> Result<(), LiveViewError> {
    if session.state.owner != *owner
        || session.state.viewer_actor != *viewer_actor
        || session.state.viewer_instance_id != *viewer_instance_id
    {
        return Err(LiveViewError::Ownership);
    }
    Ok(())
}

fn selected_entity(rig: &LiveCameraRig) -> Option<String> {
    match rig {
        LiveCameraRig::Orbit {
            target_entity_id, ..
        }
        | LiveCameraRig::FollowEntity {
            target_entity_id, ..
        }
        | LiveCameraRig::ChaseEntity {
            target_entity_id, ..
        }
        | LiveCameraRig::StabilizedMountedEntity {
            target_entity_id, ..
        } => Some(target_entity_id.to_string()),
        LiveCameraRig::Fixed { .. }
        | LiveCameraRig::LookAt { .. }
        | LiveCameraRig::FormationOverview { .. } => None,
    }
}

fn product_lifecycle(product: &veoveo_mcp_contract::LiveStreamProductState) -> LiveViewLifecycle {
    match product.lifecycle {
        veoveo_mcp_contract::LiveStreamProductLifecycle::Inactive
        | veoveo_mcp_contract::LiveStreamProductLifecycle::Starting => LiveViewLifecycle::Starting,
        veoveo_mcp_contract::LiveStreamProductLifecycle::Ready => LiveViewLifecycle::Ready,
        veoveo_mcp_contract::LiveStreamProductLifecycle::Failed => LiveViewLifecycle::Failed,
    }
}

fn active(state: &LiveViewState) -> bool {
    !matches!(
        state.lifecycle,
        LiveViewLifecycle::Closed | LiveViewLifecycle::Failed
    ) && state.expires_at > Utc::now()
}

fn close_session(session: &mut ViewerSession) {
    session.state.lifecycle = LiveViewLifecycle::Closed;
    session.state.connected_viewers = 0;
    session.token_hash.fill(0);
    session.state.expires_at = Utc::now();
    session.generation = session.generation.saturating_add(1);
    let _ = session.events.send(signal(&session.state));
}

fn rotate(
    config: &LiveViewConfig,
    session: &mut ViewerSession,
) -> Result<LiveViewConnection, LiveViewError> {
    let token = new_token()?;
    session.token_hash = token_hash(&token);
    session.state.expires_at = expiry(Utc::now(), config.session_duration)?;
    session.generation = session.generation.saturating_add(1);
    let _ = session.events.send(signal(&session.state));
    Ok(LiveViewConnection {
        stream: session.state.clone(),
        access_token: token,
    })
}

fn signal(state: &LiveViewState) -> ViewerSignal {
    ViewerSignal {
        active: active(state),
        expires_at: state.expires_at,
    }
}

fn new_token() -> Result<LiveViewAccessToken, LiveViewError> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| LiveViewError::Access)?;
    LiveViewAccessToken::new(URL_SAFE_NO_PAD.encode(bytes)).map_err(|_| LiveViewError::Access)
}

fn token_hash(token: &LiveViewAccessToken) -> [u8; 32] {
    Sha256::digest(token.expose_for_stream().as_bytes()).into()
}

fn expiry(now: DateTime<Utc>, duration: Duration) -> Result<DateTime<Utc>, LiveViewError> {
    now.checked_add_signed(chrono::Duration::from_std(duration).map_err(|_| LiveViewError::Time)?)
        .ok_or(LiveViewError::Time)
}

#[derive(Debug, thiserror::Error)]
pub(super) enum LiveViewError {
    #[error("simulation session {0} was not found")]
    SessionNotFound(LiveSessionId),
    #[error("live camera {0} was not found")]
    CameraNotFound(LiveCameraId),
    #[error("live view {0} was not found")]
    ViewNotFound(LiveViewId),
    #[error("live camera is not streamable or its product is unavailable")]
    CameraUnavailable,
    #[error("live view is closed, expired, or failed")]
    ViewUnavailable,
    #[error("live-view ownership does not match the caller")]
    Ownership,
    #[error("live-view viewer authority was revoked")]
    AuthorityRevoked,
    #[error("live-view stream authorization failed")]
    Access,
    #[error("invalid live-view identifier")]
    Identifier,
    #[error("invalid live-view contract")]
    Contract,
    #[error("live-view time overflow")]
    Time,
    #[error("simulator live-stream state failed: {0}")]
    Runtime(String),
}

impl LiveViewError {
    pub(super) fn code(&self) -> &'static str {
        match self {
            Self::SessionNotFound(_) => "session_not_found",
            Self::CameraNotFound(_) => "camera_not_found",
            Self::ViewNotFound(_) => "view_not_found",
            Self::CameraUnavailable => "camera_unavailable",
            Self::ViewUnavailable => "view_unavailable",
            Self::Ownership => "ownership_mismatch",
            Self::AuthorityRevoked => "viewer_authority_revoked",
            Self::Access => "access_denied",
            Self::Identifier => "invalid_identifier",
            Self::Contract => "invalid_contract",
            Self::Time => "time_overflow",
            Self::Runtime(_) => "stream_state_failed",
        }
    }

    pub(super) fn audit_details(&self) -> BTreeMap<String, serde_json::Value> {
        BTreeMap::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{adapter::FakeAdapter, server::service::fake_state};
    use tokio::sync::Mutex as TokioMutex;
    use veoveo_mcp_contract::{
        AccessSubject, DataLabelId, GroupId, LiveViewerInstanceId, PolicyVersion, TenantId,
        WorkContextId,
    };

    fn owner() -> LiveViewOwner {
        LiveViewOwner {
            subject: AccessSubject::Group(GroupId::new("operators").unwrap()),
            tenant: TenantId::new("tenant-a").unwrap(),
            work_context: WorkContextId::new("context-a").unwrap(),
            policy_revision: PolicyVersion::new("policy-1").unwrap(),
            data_labels: [DataLabelId::new("simulation").unwrap()]
                .into_iter()
                .collect(),
        }
    }

    async fn service() -> (Arc<LiveViewService>, Arc<Adapter>) {
        let adapter = Arc::new(Adapter::Fake(Arc::new(TokioMutex::new(FakeAdapter::new(
            fake_state().unwrap(),
        )))));
        let service = LiveViewService::new_for_test(
            adapter.clone(),
            LiveViewConfig {
                session_duration: Duration::from_secs(30),
                public_stream_url: "wss://example.test/uav-sim/live".to_owned(),
                maximum_frame_age_ms: 2_000,
            },
        )
        .unwrap();
        (service, adapter)
    }

    fn request(instance: &str) -> OpenLiveViewRequest {
        OpenLiveViewRequest {
            session_id: LiveSessionId::new("session-alpha").unwrap(),
            camera_id: LiveCameraId::new("follow").unwrap(),
            viewer_instance_id: LiveViewerInstanceId::new(instance).unwrap(),
        }
    }

    #[tokio::test]
    async fn twenty_five_viewers_share_one_camera_product() {
        let (service, adapter) = service().await;
        let mut opened = Vec::new();
        for index in 0..25 {
            opened.push(
                service
                    .open(
                        owner(),
                        PrincipalId::new(format!("viewer-{index}")).unwrap(),
                        request(&format!("browser-{index}")),
                    )
                    .await
                    .unwrap(),
            );
        }
        assert!(opened.windows(2).all(|pair| {
            pair[0].stream.stream_product_id == pair[1].stream.stream_product_id
                && pair[0].stream.stream_product_id == pair[1].stream.stream_product_id
                && pair[0].stream.live_view_id != pair[1].stream.live_view_id
        }));
        let mut runtime = adapter.state().await.unwrap();
        service.project_product_usage(&mut runtime).await;
        let product = runtime
            .stream_products
            .iter()
            .find(|product| product.camera_id.as_str() == "follow")
            .unwrap();
        assert_eq!(product.active_viewers, 25);
        assert_eq!(product.nvenc_sessions, 1);
    }

    #[tokio::test]
    async fn closing_one_viewer_does_not_stop_the_shared_product() {
        let (service, adapter) = service().await;
        let actor = PrincipalId::new("alice").unwrap();
        let first = service
            .open(owner(), actor.clone(), request("browser-a"))
            .await
            .unwrap();
        let second = service
            .open(owner(), actor.clone(), request("browser-b"))
            .await
            .unwrap();
        service
            .close(
                &owner(),
                &actor,
                CloseLiveViewRequest {
                    session_id: first.stream.session_id,
                    live_view_id: first.stream.live_view_id,
                    viewer_instance_id: LiveViewerInstanceId::new("browser-a").unwrap(),
                },
            )
            .await
            .unwrap();
        assert!(
            service
                .authorize_stream(
                    &second.stream.live_view_id,
                    second.access_token.expose_for_stream(),
                )
                .await
                .is_ok()
        );
        assert_eq!(
            adapter.state().await.unwrap().stream_products[0].nvenc_sessions,
            1
        );
    }

    #[tokio::test]
    async fn stream_disconnect_releases_connection_state_immediately() {
        let (service, adapter) = service().await;
        let actor = PrincipalId::new("alice").unwrap();
        let opened = service
            .open(owner(), actor, request("browser-a"))
            .await
            .unwrap();
        service
            .authorize_stream(
                &opened.stream.live_view_id,
                opened.access_token.expose_for_stream(),
            )
            .await
            .unwrap();
        let mut connected = adapter.state().await.unwrap();
        service.project_product_usage(&mut connected).await;
        assert_eq!(connected.stream_products[0].connected_viewers, 1);
        service.finish_stream(&opened.stream.live_view_id).await;
        let mut disconnected = adapter.state().await.unwrap();
        service.project_product_usage(&mut disconnected).await;
        assert_eq!(disconnected.stream_products[0].connected_viewers, 0);
        assert_eq!(disconnected.stream_products[0].active_viewers, 1);
    }
}
