use std::{collections::BTreeMap, net::IpAddr, sync::Arc, time::Duration};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tokio::sync::{Mutex, watch};
use uuid::Uuid;
use veoveo_mcp_contract::{
    LIVE_VIEW_SCHEMA, LiveCameraId, LiveCameraRig, LiveCameraStreamPolicy, LiveColorMatrix,
    LiveColorMetadata, LiveColorPrimaries, LiveColorRange, LiveColorTransfer, LiveMediaEndpoint,
    LiveMediaTransport, LiveSessionId, LiveViewAccessToken, LiveViewCapacityDimension,
    LiveViewCodec, LiveViewConnection, LiveViewHardwareEncoder, LiveViewId, LiveViewLifecycle,
    LiveViewOwner, LiveViewState, LiveViewUri, PrincipalId,
};

use crate::{
    adapter::Adapter,
    contract::{
        CloseLiveViewRequest, CloseLiveViewResult, OpenLiveViewRequest, RenewLiveViewRequest,
        SimulationState,
    },
    server::live_view_audit::LiveViewAudit,
};

mod reconciliation;

use reconciliation::ProductReconciliationError;

#[derive(Debug, Clone)]
struct Lease {
    state: LiveViewState,
    token_hash: [u8; 32],
    generation: u64,
    events: watch::Sender<LeaseSignal>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LeaseSignal {
    pub(super) active: bool,
    pub(super) expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub(super) struct SignalingAuthorization {
    pub(super) state: LiveViewState,
    pub(super) events: watch::Receiver<LeaseSignal>,
    pub(super) admission: SignalingAdmission,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SignalingAdmission {
    Initial,
    Reconnect,
}

#[derive(Debug)]
struct LiveViewStateStore {
    leases: BTreeMap<LiveViewId, Lease>,
}

#[derive(Debug, Clone)]
pub(super) struct LiveViewConfig {
    pub(super) lease_duration: Duration,
    pub(super) public_signaling_url: String,
    pub(super) public_media_host: IpAddr,
    pub(super) public_media_port_base: u16,
    pub(super) maximum_frame_age_ms: u32,
    pub(super) maximum_viewer_leases: u32,
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
            !config.lease_duration.is_zero(),
            "live-view lease duration must be positive"
        );
        anyhow::ensure!(
            config.maximum_frame_age_ms > 0,
            "maximum live-view frame age must be positive"
        );
        anyhow::ensure!(
            config.maximum_viewer_leases > 0,
            "maximum viewer leases must be positive"
        );
        LiveMediaEndpoint {
            transport: LiveMediaTransport::WebRtc,
            signaling_url: config.public_signaling_url.clone(),
            media_host: config.public_media_host,
            media_port: config.public_media_port_base,
        }
        .validate()?;
        Ok(Arc::new(Self {
            adapter,
            audit,
            config,
            state: Mutex::new(LiveViewStateStore {
                leases: BTreeMap::new(),
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
        if let Some(existing_id) = state.leases.iter().find_map(|(id, lease)| {
            (active(&lease.state)
                && lease.state.owner == owner
                && lease.state.viewer_actor == viewer_actor
                && lease.state.viewer_instance_id == request.viewer_instance_id
                && lease.state.camera_id == request.camera_id)
                .then(|| id.clone())
        }) {
            let connection = rotate(&self.config, state.leases.get_mut(&existing_id).unwrap())?;
            let generation = state.leases[&existing_id].generation;
            drop(state);
            self.arm_expiry(existing_id, generation, connection.stream.expires_at);
            return Ok(connection);
        }
        if state
            .leases
            .values()
            .filter(|lease| active(&lease.state))
            .count()
            >= self.config.maximum_viewer_leases as usize
        {
            return Err(LiveViewError::Capacity(
                LiveViewCapacityDimension::ViewerLeases,
            ));
        }

        let mut simulation = self
            .adapter
            .state()
            .await
            .map_err(|error| LiveViewError::Runtime(error.to_string()))?;
        if simulation.session_id.as_str() != request.session_id.as_str() {
            return Err(LiveViewError::SessionNotFound(request.session_id));
        }
        reconciliation::ensure_unique_capacity_slots(&simulation)?;
        let camera = simulation
            .live_cameras
            .iter()
            .find(|camera| camera.camera_id == request.camera_id)
            .cloned()
            .ok_or_else(|| LiveViewError::CameraNotFound(request.camera_id.clone()))?;
        if camera.stream_policy == LiveCameraStreamPolicy::Disabled {
            return Err(LiveViewError::CameraUnavailable);
        }
        let mut capacity_slot = inactive_capacity_slot(&simulation);
        if capacity_slot.is_none() {
            simulation = self
                .reclaim_untracked_products_locked(simulation, &state)
                .await?;
            capacity_slot = inactive_capacity_slot(&simulation);
        }
        let capacity_slot = capacity_slot.ok_or(LiveViewError::Capacity(
            LiveViewCapacityDimension::ViewerSlots,
        ))?;

        let now = Utc::now();
        let expires_at = expiry(now, self.config.lease_duration)?;
        let live_view_id = LiveViewId::new(format!("view-{}", Uuid::now_v7()))
            .map_err(|_| LiveViewError::Identifier)?;
        let token = new_token()?;
        let endpoint = self.endpoint(capacity_slot)?;
        let resource_uri = LiveViewUri::new(format!(
            "uav-sim://session/{}/live-view/{live_view_id}",
            request.session_id
        ))
        .map_err(|_| LiveViewError::Identifier)?;
        let product = match self
            .adapter
            .assign_live_product(capacity_slot, &camera.camera_id, &live_view_id)
            .await
        {
            Ok(product) => product,
            Err(error) => {
                let transition_error = LiveViewError::Runtime(error.to_string());
                self.reconcile_exact_product(capacity_slot, &live_view_id)
                    .await?;
                return Err(transition_error);
            }
        };
        if product.capacity_slot != capacity_slot
            || product.camera_id.as_ref() != Some(&camera.camera_id)
            || product.live_view_id.as_ref() != Some(&live_view_id)
        {
            self.reconcile_exact_product(capacity_slot, &live_view_id)
                .await?;
            return Err(LiveViewError::Contract);
        }
        let stream = LiveViewState {
            schema_version: LIVE_VIEW_SCHEMA.to_owned(),
            live_view_id: live_view_id.clone(),
            stream_product_id: product.stream_product_id.clone(),
            capacity_slot,
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
            viewer_limit: 1,
            camera_health: camera.health,
            last_frame_at: product.last_frame_at.or(camera.last_frame_at),
            source_to_render_p95_microseconds: product.source_to_render_p95_microseconds,
            source_to_render_samples: product.source_to_render_samples,
            maximum_frame_age_ms: self.config.maximum_frame_age_ms,
            endpoint,
            created_at: now,
            expires_at,
        };
        if stream.lifecycle == LiveViewLifecycle::Failed {
            self.reconcile_exact_product(capacity_slot, &live_view_id)
                .await?;
            return Err(LiveViewError::CameraUnavailable);
        }
        if stream.validate().is_err() {
            self.reconcile_exact_product(capacity_slot, &live_view_id)
                .await?;
            return Err(LiveViewError::Contract);
        }
        let (events, _) = watch::channel(signal(&stream));
        let lease = Lease {
            state: stream.clone(),
            token_hash: token_hash(&token),
            generation: 1,
            events,
        };
        state.leases.insert(live_view_id.clone(), lease);
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
        let lease = state
            .leases
            .get_mut(&request.live_view_id)
            .filter(|lease| lease.state.session_id == request.session_id)
            .ok_or_else(|| LiveViewError::ViewNotFound(request.live_view_id.clone()))?;
        if lease.state.viewer_actor != *viewer_actor
            || lease.state.viewer_instance_id != request.viewer_instance_id
        {
            return Err(LiveViewError::Ownership);
        }
        if !active(&lease.state) {
            return Err(LiveViewError::ViewUnavailable);
        }
        if lease.state.owner != *owner {
            close_lease(lease);
            let capacity_slot = lease.state.capacity_slot;
            let live_view_id = lease.state.live_view_id.clone();
            drop(state);
            self.release_product(capacity_slot, &live_view_id).await?;
            return Err(LiveViewError::AuthorityRevoked);
        }
        let connection = rotate(&self.config, lease)?;
        let generation = lease.generation;
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
        let lease = state
            .leases
            .get_mut(&request.live_view_id)
            .filter(|lease| lease.state.session_id == request.session_id)
            .ok_or_else(|| LiveViewError::ViewNotFound(request.live_view_id.clone()))?;
        authorize_owner(lease, owner, viewer_actor, &request.viewer_instance_id)?;
        let resource_uri = lease.state.resource_uri.as_str().to_owned();
        if matches!(
            lease.state.lifecycle,
            LiveViewLifecycle::Closed | LiveViewLifecycle::Failed
        ) {
            return Ok(CloseLiveViewResult {
                resource_uri,
                closed: true,
            });
        }
        close_lease(lease);
        let capacity_slot = lease.state.capacity_slot;
        let live_view_id = lease.state.live_view_id.clone();
        drop(state);
        self.release_product(capacity_slot, &live_view_id).await?;
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

    pub(super) async fn get(
        &self,
        owner: &LiveViewOwner,
        viewer_actor: &PrincipalId,
        live_view_id: &LiveViewId,
    ) -> Result<LiveViewState, LiveViewError> {
        let state = self.state.lock().await;
        let lease = state
            .leases
            .get(live_view_id)
            .ok_or_else(|| LiveViewError::ViewNotFound(live_view_id.clone()))?;
        if lease.state.owner != *owner || lease.state.viewer_actor != *viewer_actor {
            return Err(LiveViewError::Ownership);
        }
        let mut result = lease.state.clone();
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
                        && product.capacity_slot == result.capacity_slot
                        && product.live_view_id.as_ref() == Some(&result.live_view_id)
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
            let lease = product.live_view_id.as_ref().and_then(|live_view_id| {
                state.leases.get(live_view_id).filter(|lease| {
                    lease.state.capacity_slot == product.capacity_slot && active(&lease.state)
                })
            });
            product.active_viewer_leases = u32::from(lease.is_some());
            product.connected_viewers = lease
                .map(|lease| lease.state.connected_viewers)
                .unwrap_or(0);
        }
    }

    pub(super) async fn authorize_signaling(
        &self,
        live_view_id: &LiveViewId,
        token: &str,
        admission: SignalingAdmission,
    ) -> Result<SignalingAuthorization, LiveViewError> {
        let supplied: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        let mut state = self.state.lock().await;
        let lease = state
            .leases
            .get_mut(live_view_id)
            .ok_or_else(|| LiveViewError::ViewNotFound(live_view_id.clone()))?;
        if !active(&lease.state) || supplied.ct_eq(&lease.token_hash).unwrap_u8() != 1 {
            return Err(LiveViewError::Access);
        }
        match admission {
            SignalingAdmission::Reconnect if lease.state.connected_viewers == 0 => {
                return Err(LiveViewError::Access);
            }
            SignalingAdmission::Reconnect => {}
            SignalingAdmission::Initial
                if lease.state.connected_viewers >= lease.state.viewer_limit =>
            {
                return Err(LiveViewError::Access);
            }
            SignalingAdmission::Initial => {
                lease.state.connected_viewers += 1;
                lease.state.lifecycle = LiveViewLifecycle::Live;
            }
        }
        Ok(SignalingAuthorization {
            state: lease.state.clone(),
            events: lease.events.subscribe(),
            admission,
        })
    }

    pub(super) async fn cancel_signaling_admission(
        &self,
        live_view_id: &LiveViewId,
        admission: SignalingAdmission,
    ) {
        if admission == SignalingAdmission::Reconnect {
            return;
        }
        let mut state = self.state.lock().await;
        let Some(lease) = state.leases.get_mut(live_view_id) else {
            return;
        };
        if !active(&lease.state) {
            return;
        }
        if lease.state.connected_viewers > 0 {
            lease.state.connected_viewers -= 1;
        }
        if lease.state.connected_viewers == 0 {
            lease.state.lifecycle = LiveViewLifecycle::Ready;
        }
    }

    fn endpoint(&self, slot: u16) -> Result<LiveMediaEndpoint, LiveViewError> {
        Ok(LiveMediaEndpoint {
            transport: LiveMediaTransport::WebRtc,
            signaling_url: self.config.public_signaling_url.clone(),
            media_host: self.config.public_media_host,
            media_port: self
                .config
                .public_media_port_base
                .checked_add(slot)
                .ok_or(LiveViewError::Contract)?,
        })
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
            let Some(lease) = state.leases.get_mut(&live_view_id) else {
                return;
            };
            if lease.generation != generation || lease.state.expires_at > Utc::now() {
                return;
            }
            close_lease(lease);
            let expired = lease.state.clone();
            let capacity_slot = lease.state.capacity_slot;
            drop(state);
            if let Err(error) = service.release_product(capacity_slot, &live_view_id).await {
                tracing::error!(
                    %error,
                    %live_view_id,
                    capacity_slot,
                    "failed to release expired viewer product"
                );
                if let Some(audit) = &service.audit {
                    let mut details = error.audit_details();
                    details.insert(
                        "failure_code".to_owned(),
                        serde_json::Value::String(error.code().to_owned()),
                    );
                    if let Err(audit_error) = audit
                        .append_lease(
                            &expired,
                            "expiry_release_failed",
                            veoveo_platform_store::AuditOutcome::Denied,
                            details,
                        )
                        .await
                    {
                        tracing::error!(
                            %audit_error,
                            %live_view_id,
                            "failed to persist live-view expiry cleanup audit"
                        );
                    }
                }
            }
            if let Some(audit) = &service.audit
                && let Err(error) = audit
                    .append_lease(
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
    lease: &Lease,
    owner: &LiveViewOwner,
    viewer_actor: &PrincipalId,
    viewer_instance_id: &veoveo_mcp_contract::LiveViewerInstanceId,
) -> Result<(), LiveViewError> {
    if lease.state.owner != *owner
        || lease.state.viewer_actor != *viewer_actor
        || lease.state.viewer_instance_id != *viewer_instance_id
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

fn close_lease(lease: &mut Lease) {
    lease.state.lifecycle = LiveViewLifecycle::Closed;
    lease.state.connected_viewers = 0;
    lease.token_hash.fill(0);
    lease.state.expires_at = Utc::now();
    lease.generation = lease.generation.saturating_add(1);
    let _ = lease.events.send(signal(&lease.state));
}

fn rotate(config: &LiveViewConfig, lease: &mut Lease) -> Result<LiveViewConnection, LiveViewError> {
    let token = new_token()?;
    lease.token_hash = token_hash(&token);
    lease.state.expires_at = expiry(Utc::now(), config.lease_duration)?;
    lease.generation = lease.generation.saturating_add(1);
    let _ = lease.events.send(signal(&lease.state));
    Ok(LiveViewConnection {
        stream: lease.state.clone(),
        access_token: token,
    })
}

fn signal(state: &LiveViewState) -> LeaseSignal {
    LeaseSignal {
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
    Sha256::digest(token.expose_for_signaling().as_bytes()).into()
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
    #[error("live-view signaling authorization failed")]
    Access,
    #[error("live-view {0} capacity is exhausted")]
    Capacity(LiveViewCapacityDimension),
    #[error("invalid live-view identifier")]
    Identifier,
    #[error("invalid live-view contract")]
    Contract,
    #[error("live-view time overflow")]
    Time,
    #[error("simulator live-product transition failed: {0}")]
    Runtime(String),
    #[error("simulator live-product reconciliation failed: {0}")]
    Reconciliation(ProductReconciliationError),
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
            Self::Capacity(_) => "viewer_capacity_exhausted",
            Self::Identifier => "invalid_identifier",
            Self::Contract => "invalid_contract",
            Self::Time => "time_overflow",
            Self::Runtime(_) => "product_transition_failed",
            Self::Reconciliation(_) => "orphan_product_cleanup_failed",
        }
    }

    pub(super) fn capacity_dimension(&self) -> Option<LiveViewCapacityDimension> {
        match self {
            Self::Capacity(dimension) => Some(*dimension),
            _ => None,
        }
    }

    pub(super) fn audit_details(&self) -> BTreeMap<String, serde_json::Value> {
        match self {
            Self::Reconciliation(error) => error.audit_details(),
            _ => BTreeMap::new(),
        }
    }
}

fn inactive_capacity_slot(simulation: &SimulationState) -> Option<u16> {
    simulation
        .stream_products
        .iter()
        .filter(|product| reconciliation::inactive_product(product))
        .map(|product| product.capacity_slot)
        .min()
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

    async fn service(viewer_slots: u16) -> (Arc<LiveViewService>, Arc<Adapter>) {
        service_with(
            viewer_slots,
            Duration::from_secs(30),
            u32::from(viewer_slots),
        )
        .await
    }

    async fn service_with(
        viewer_slots: u16,
        lease_duration: Duration,
        maximum_viewer_leases: u32,
    ) -> (Arc<LiveViewService>, Arc<Adapter>) {
        let mut state = fake_state().unwrap();
        let template = state.stream_products[0].clone();
        state.stream_products = (0..viewer_slots)
            .map(
                |capacity_slot| veoveo_mcp_contract::LiveStreamProductState {
                    stream_product_id: veoveo_mcp_contract::LiveStreamProductId::new(format!(
                        "product-slot-{capacity_slot}"
                    ))
                    .unwrap(),
                    capacity_slot,
                    camera_id: None,
                    live_view_id: None,
                    lifecycle: veoveo_mcp_contract::LiveStreamProductLifecycle::Inactive,
                    active_viewer_leases: 0,
                    connected_viewers: 0,
                    nvenc_sessions: 0,
                    encoded_frames: 0,
                    source_to_render_p95_microseconds: None,
                    source_to_render_samples: 0,
                    last_frame_at: None,
                    visible: None,
                    diagnostic: template.diagnostic.clone(),
                },
            )
            .collect();
        let adapter = Arc::new(Adapter::Fake(Arc::new(TokioMutex::new(FakeAdapter::new(
            state,
        )))));
        let service = LiveViewService::new_for_test(
            adapter.clone(),
            test_config(lease_duration, maximum_viewer_leases),
        )
        .unwrap();
        (service, adapter)
    }

    fn test_config(lease_duration: Duration, maximum_viewer_leases: u32) -> LiveViewConfig {
        LiveViewConfig {
            lease_duration,
            public_signaling_url: "wss://example.test/uav-sim/signaling".to_owned(),
            public_media_host: "192.0.2.10".parse().unwrap(),
            public_media_port_base: 47_998,
            maximum_frame_age_ms: 2_000,
            maximum_viewer_leases,
        }
    }

    fn request(instance: &str) -> OpenLiveViewRequest {
        OpenLiveViewRequest {
            session_id: LiveSessionId::new("session-alpha").unwrap(),
            camera_id: LiveCameraId::new("follow").unwrap(),
            viewer_instance_id: LiveViewerInstanceId::new(instance).unwrap(),
        }
    }

    fn fake_adapter(adapter: &Arc<Adapter>) -> Arc<TokioMutex<FakeAdapter>> {
        match adapter.as_ref() {
            Adapter::Fake(adapter) => adapter.clone(),
            Adapter::Http(_) => panic!("test requires the fake simulator adapter"),
        }
    }

    fn cleanup_failure(error: &LiveViewError) -> String {
        error
            .audit_details()
            .get("cleanup_failure")
            .and_then(serde_json::Value::as_str)
            .expect("reconciliation error carries a typed cleanup failure")
            .to_owned()
    }

    #[tokio::test]
    async fn distinct_actors_receive_distinct_native_products_and_tokens() {
        let (service, adapter) = service(2).await;
        let alice = PrincipalId::new("alice").unwrap();
        let bob = PrincipalId::new("bob").unwrap();
        let first = service
            .open(owner(), alice, request("browser-a"))
            .await
            .unwrap();
        let second = service
            .open(owner(), bob, request("browser-b"))
            .await
            .unwrap();
        assert_ne!(first.stream.live_view_id, second.stream.live_view_id);
        assert_ne!(
            first.access_token.expose_for_signaling(),
            second.access_token.expose_for_signaling()
        );
        assert_ne!(
            first.stream.stream_product_id,
            second.stream.stream_product_id
        );
        assert_ne!(first.stream.capacity_slot, second.stream.capacity_slot);
        let runtime = adapter.state().await.unwrap();
        assert_eq!(runtime.stream_products.len(), 2);
        assert_eq!(
            runtime
                .stream_products
                .iter()
                .map(|product| product.nvenc_sessions)
                .sum::<u32>(),
            2
        );
    }

    #[tokio::test]
    async fn one_actor_gets_independent_browser_products() {
        let (service, adapter) = service(2).await;
        let actor = PrincipalId::new("alice").unwrap();
        let first = service
            .open(owner(), actor.clone(), request("browser-a"))
            .await
            .unwrap();
        let second = service
            .open(owner(), actor.clone(), request("browser-b"))
            .await
            .unwrap();

        assert_ne!(first.stream.live_view_id, second.stream.live_view_id);
        assert_ne!(
            first.access_token.expose_for_signaling(),
            second.access_token.expose_for_signaling()
        );
        assert_ne!(
            first.stream.stream_product_id,
            second.stream.stream_product_id
        );

        service
            .close(
                &owner(),
                &actor,
                CloseLiveViewRequest {
                    session_id: first.stream.session_id,
                    live_view_id: first.stream.live_view_id.clone(),
                    viewer_instance_id: LiveViewerInstanceId::new("browser-a").unwrap(),
                },
            )
            .await
            .unwrap();

        assert!(
            service
                .authorize_signaling(
                    &second.stream.live_view_id,
                    second.access_token.expose_for_signaling(),
                    SignalingAdmission::Initial,
                )
                .await
                .is_ok()
        );
        let runtime = adapter.state().await.unwrap();
        assert_eq!(runtime.stream_products.len(), 2);
        assert_eq!(runtime.stream_products[0].nvenc_sessions, 0);
        assert_eq!(runtime.stream_products[1].nvenc_sessions, 1);
    }

    #[tokio::test]
    async fn renewal_invalidates_the_prior_token_for_only_that_lease() {
        let (service, _) = service(1).await;
        let actor = PrincipalId::new("alice").unwrap();
        let instance = LiveViewerInstanceId::new("browser-a").unwrap();
        let opened = service
            .open(owner(), actor.clone(), request("browser-a"))
            .await
            .unwrap();
        let renewed = service
            .renew(
                &owner(),
                &actor,
                RenewLiveViewRequest {
                    session_id: LiveSessionId::new("session-alpha").unwrap(),
                    live_view_id: opened.stream.live_view_id.clone(),
                    viewer_instance_id: instance,
                },
            )
            .await
            .unwrap();
        assert!(
            service
                .authorize_signaling(
                    &opened.stream.live_view_id,
                    opened.access_token.expose_for_signaling(),
                    SignalingAdmission::Initial,
                )
                .await
                .is_err()
        );
        assert!(
            service
                .authorize_signaling(
                    &renewed.stream.live_view_id,
                    renewed.access_token.expose_for_signaling(),
                    SignalingAdmission::Initial,
                )
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn changed_viewer_authority_revokes_only_that_actors_lease() {
        let (service, adapter) = service(2).await;
        let alice = PrincipalId::new("alice").unwrap();
        let bob = PrincipalId::new("bob").unwrap();
        let alice_view = service
            .open(owner(), alice.clone(), request("browser-a"))
            .await
            .unwrap();
        let bob_view = service
            .open(owner(), bob.clone(), request("browser-b"))
            .await
            .unwrap();
        let mut changed_owner = owner();
        changed_owner.policy_revision = PolicyVersion::new("policy-2").unwrap();

        let error = service
            .renew(
                &changed_owner,
                &alice,
                RenewLiveViewRequest {
                    session_id: alice_view.stream.session_id.clone(),
                    live_view_id: alice_view.stream.live_view_id.clone(),
                    viewer_instance_id: LiveViewerInstanceId::new("browser-a").unwrap(),
                },
            )
            .await
            .unwrap_err();

        assert!(matches!(error, LiveViewError::AuthorityRevoked));
        assert!(
            service
                .authorize_signaling(
                    &alice_view.stream.live_view_id,
                    alice_view.access_token.expose_for_signaling(),
                    SignalingAdmission::Initial,
                )
                .await
                .is_err()
        );
        assert!(
            service
                .authorize_signaling(
                    &bob_view.stream.live_view_id,
                    bob_view.access_token.expose_for_signaling(),
                    SignalingAdmission::Initial,
                )
                .await
                .is_ok()
        );
        let products = adapter.state().await.unwrap().stream_products;
        assert_eq!(products[0].nvenc_sessions, 0);
        assert_eq!(products[1].nvenc_sessions, 1);
    }

    #[tokio::test]
    async fn close_releases_only_its_slot_and_the_slot_can_be_reassigned() {
        let (service, adapter) = service(2).await;
        let first = service
            .open(
                owner(),
                PrincipalId::new("alice").unwrap(),
                request("browser-a"),
            )
            .await
            .unwrap();
        let second = service
            .open(
                owner(),
                PrincipalId::new("bob").unwrap(),
                request("browser-b"),
            )
            .await
            .unwrap();
        assert_ne!(first.stream.capacity_slot, second.stream.capacity_slot);
        service
            .close(
                &owner(),
                &PrincipalId::new("alice").unwrap(),
                CloseLiveViewRequest {
                    session_id: first.stream.session_id,
                    live_view_id: first.stream.live_view_id.clone(),
                    viewer_instance_id: LiveViewerInstanceId::new("browser-a").unwrap(),
                },
            )
            .await
            .unwrap();
        let products = adapter.state().await.unwrap().stream_products;
        assert_eq!(
            products[usize::from(first.stream.capacity_slot)].nvenc_sessions,
            0
        );
        assert_eq!(
            products[usize::from(second.stream.capacity_slot)].nvenc_sessions,
            1
        );
        let third = service
            .open(
                owner(),
                PrincipalId::new("charlie").unwrap(),
                request("browser-c"),
            )
            .await
            .unwrap();
        assert_eq!(third.stream.capacity_slot, first.stream.capacity_slot);
        assert_ne!(third.stream.live_view_id, first.stream.live_view_id);
    }

    #[tokio::test]
    async fn lost_assignment_response_reclaims_only_the_exact_product() {
        let (service, adapter) = service(1).await;
        fake_adapter(&adapter)
            .lock()
            .await
            .fail_assignment_after_mutation(None);

        let error = service
            .open(
                owner(),
                PrincipalId::new("alice").unwrap(),
                request("browser-a"),
            )
            .await
            .unwrap_err();

        assert!(matches!(error, LiveViewError::Runtime(_)));
        let runtime = adapter.state().await.unwrap();
        assert!(reconciliation::inactive_product(
            &runtime.stream_products[0]
        ));
        let release_calls = fake_adapter(&adapter).lock().await.release_calls().to_vec();
        assert_eq!(release_calls.len(), 1);
        assert_eq!(release_calls[0].0, 0);

        let retried = service
            .open(
                owner(),
                PrincipalId::new("alice").unwrap(),
                request("browser-a"),
            )
            .await
            .unwrap();
        assert_eq!(retried.stream.capacity_slot, 0);
        assert_ne!(retried.stream.live_view_id, release_calls[0].1);
    }

    #[tokio::test]
    async fn orphan_reclaim_preserves_a_valid_logical_lease() {
        let (service, adapter) = service(2).await;
        let valid = service
            .open(
                owner(),
                PrincipalId::new("alice").unwrap(),
                request("browser-a"),
            )
            .await
            .unwrap();
        let orphan_id = LiveViewId::new("view-orphan").unwrap();
        {
            let fake = fake_adapter(&adapter);
            let mut fake = fake.lock().await;
            let product = &mut fake.state_mut().stream_products[1];
            product.camera_id = Some(LiveCameraId::new("follow").unwrap());
            product.live_view_id = Some(orphan_id.clone());
            product.lifecycle = veoveo_mcp_contract::LiveStreamProductLifecycle::Ready;
            product.active_viewer_leases = 1;
            product.nvenc_sessions = 1;
        }

        let opened = service
            .open(
                owner(),
                PrincipalId::new("bob").unwrap(),
                request("browser-b"),
            )
            .await
            .unwrap();

        let runtime = adapter.state().await.unwrap();
        assert_eq!(
            runtime.stream_products[0].live_view_id,
            Some(valid.stream.live_view_id)
        );
        assert_eq!(
            runtime.stream_products[1].live_view_id,
            Some(opened.stream.live_view_id)
        );
        assert_eq!(
            fake_adapter(&adapter).lock().await.release_calls(),
            &[(1, orphan_id)]
        );
    }

    #[tokio::test]
    async fn failed_exact_cleanup_is_typed_and_fails_closed() {
        let (service, adapter) = service(1).await;
        {
            let fake = fake_adapter(&adapter);
            let mut fake = fake.lock().await;
            fake.fail_assignment_after_mutation(None);
            fake.fail_release_before_mutation();
        }

        let error = service
            .open(
                owner(),
                PrincipalId::new("alice").unwrap(),
                request("browser-a"),
            )
            .await
            .unwrap_err();

        assert_eq!(error.code(), "orphan_product_cleanup_failed");
        assert_eq!(cleanup_failure(&error), "exact_release_failed");
        assert_eq!(
            adapter.state().await.unwrap().stream_products[0].nvenc_sessions,
            1
        );
    }

    #[tokio::test]
    async fn mismatched_assignment_identity_is_never_released() {
        let (service, adapter) = service(1).await;
        let replacement = LiveViewId::new("view-owned-elsewhere").unwrap();
        fake_adapter(&adapter)
            .lock()
            .await
            .fail_assignment_after_mutation(Some(replacement.clone()));

        let error = service
            .open(
                owner(),
                PrincipalId::new("alice").unwrap(),
                request("browser-a"),
            )
            .await
            .unwrap_err();

        assert_eq!(error.code(), "orphan_product_cleanup_failed");
        assert_eq!(cleanup_failure(&error), "assignment_identity_mismatch");
        assert!(
            fake_adapter(&adapter)
                .lock()
                .await
                .release_calls()
                .is_empty()
        );
        assert_eq!(
            adapter.state().await.unwrap().stream_products[0].live_view_id,
            Some(replacement)
        );
    }

    #[tokio::test]
    async fn release_response_loss_is_accepted_only_after_inactive_is_observed() {
        let (service, adapter) = service(1).await;
        let actor = PrincipalId::new("alice").unwrap();
        let opened = service
            .open(owner(), actor.clone(), request("browser-a"))
            .await
            .unwrap();
        fake_adapter(&adapter)
            .lock()
            .await
            .fail_release_after_mutation();

        service
            .close(
                &owner(),
                &actor,
                CloseLiveViewRequest {
                    session_id: opened.stream.session_id,
                    live_view_id: opened.stream.live_view_id,
                    viewer_instance_id: LiveViewerInstanceId::new("browser-a").unwrap(),
                },
            )
            .await
            .unwrap();

        assert!(reconciliation::inactive_product(
            &adapter.state().await.unwrap().stream_products[0]
        ));
    }

    #[tokio::test]
    async fn successful_release_without_runtime_mutation_fails_closed() {
        let (service, adapter) = service(1).await;
        let actor = PrincipalId::new("alice").unwrap();
        let opened = service
            .open(owner(), actor.clone(), request("browser-a"))
            .await
            .unwrap();
        fake_adapter(&adapter)
            .lock()
            .await
            .release_without_mutation();

        let error = service
            .close(
                &owner(),
                &actor,
                CloseLiveViewRequest {
                    session_id: opened.stream.session_id,
                    live_view_id: opened.stream.live_view_id,
                    viewer_instance_id: LiveViewerInstanceId::new("browser-a").unwrap(),
                },
            )
            .await
            .unwrap_err();

        assert_eq!(cleanup_failure(&error), "exact_release_not_observed");
        assert_eq!(
            adapter.state().await.unwrap().stream_products[0].nvenc_sessions,
            1
        );
    }

    #[tokio::test]
    async fn cleanup_rejects_missing_and_duplicate_capacity_slots() {
        let (service, adapter) = service(1).await;
        let expected = LiveViewId::new("view-expected").unwrap();
        let missing = service
            .reconcile_exact_product(9, &expected)
            .await
            .unwrap_err();
        assert_eq!(cleanup_failure(&missing), "capacity_slot_not_unique");

        {
            let fake = fake_adapter(&adapter);
            let mut fake = fake.lock().await;
            let duplicate = fake.state_mut().stream_products[0].clone();
            fake.state_mut().stream_products.push(duplicate);
        }
        let duplicate = service
            .reconcile_exact_product(0, &expected)
            .await
            .unwrap_err();
        assert_eq!(cleanup_failure(&duplicate), "capacity_slot_not_unique");
    }

    #[tokio::test]
    async fn cleanup_refresh_failure_is_typed() {
        let (service, adapter) = service(1).await;
        fake_adapter(&adapter).lock().await.fail_state_call(1);

        let error = service
            .reconcile_exact_product(0, &LiveViewId::new("view-expected").unwrap())
            .await
            .unwrap_err();

        assert_eq!(cleanup_failure(&error), "runtime_state_unavailable");
    }

    #[tokio::test]
    async fn concurrent_open_respects_the_exact_viewer_capacity() {
        let (service, _) = service_with(1, Duration::from_secs(30), 8).await;
        let first = service.open(
            owner(),
            PrincipalId::new("alice").unwrap(),
            request("browser-a"),
        );
        let second = service.open(
            owner(),
            PrincipalId::new("bob").unwrap(),
            request("browser-b"),
        );
        let results = tokio::join!(first, second);
        assert_eq!(
            usize::from(results.0.is_ok()) + usize::from(results.1.is_ok()),
            1
        );
        assert!(
            matches!(
                results.0,
                Err(LiveViewError::Capacity(
                    LiveViewCapacityDimension::ViewerSlots
                ))
            ) || matches!(
                results.1,
                Err(LiveViewError::Capacity(
                    LiveViewCapacityDimension::ViewerSlots
                ))
            )
        );
    }

    #[tokio::test]
    async fn lease_limit_identifies_viewer_lease_capacity() {
        let (service, _) = service_with(2, Duration::from_secs(30), 1).await;
        service
            .open(
                owner(),
                PrincipalId::new("alice").unwrap(),
                request("browser-a"),
            )
            .await
            .unwrap();

        let error = service
            .open(
                owner(),
                PrincipalId::new("bob").unwrap(),
                request("browser-b"),
            )
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            LiveViewError::Capacity(LiveViewCapacityDimension::ViewerLeases)
        ));
    }

    #[tokio::test]
    async fn exact_expiry_closes_the_lease_and_releases_its_slot() {
        let (service, adapter) = service_with(1, Duration::from_millis(10), 8).await;
        let opened = service
            .open(
                owner(),
                PrincipalId::new("alice").unwrap(),
                request("browser-a"),
            )
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(35)).await;
        let closed = service
            .get(
                &owner(),
                &PrincipalId::new("alice").unwrap(),
                &opened.stream.live_view_id,
            )
            .await
            .unwrap();
        assert_eq!(closed.lifecycle, LiveViewLifecycle::Closed);
        let product = adapter.state().await.unwrap().stream_products.remove(0);
        assert_eq!(
            product.lifecycle,
            veoveo_mcp_contract::LiveStreamProductLifecycle::Inactive
        );
        assert_eq!(product.nvenc_sessions, 0);
        assert!(product.camera_id.is_none());
        assert!(product.live_view_id.is_none());
    }

    #[tokio::test]
    async fn failed_signaling_admission_preserves_the_lease_and_product_for_retry() {
        let (service, adapter) = service(1).await;
        let actor = PrincipalId::new("alice").unwrap();
        let opened = service
            .open(owner(), actor.clone(), request("browser-a"))
            .await
            .unwrap();
        service
            .authorize_signaling(
                &opened.stream.live_view_id,
                opened.access_token.expose_for_signaling(),
                SignalingAdmission::Initial,
            )
            .await
            .unwrap();
        service
            .cancel_signaling_admission(&opened.stream.live_view_id, SignalingAdmission::Initial)
            .await;
        let ready = service
            .get(&owner(), &actor, &opened.stream.live_view_id)
            .await
            .unwrap();
        assert_eq!(ready.lifecycle, LiveViewLifecycle::Ready);
        assert_eq!(ready.connected_viewers, 0);
        let product = adapter.state().await.unwrap().stream_products.remove(0);
        assert_eq!(
            product.lifecycle,
            veoveo_mcp_contract::LiveStreamProductLifecycle::Ready
        );
        assert_eq!(product.nvenc_sessions, 1);
        service
            .authorize_signaling(
                &opened.stream.live_view_id,
                opened.access_token.expose_for_signaling(),
                SignalingAdmission::Initial,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn signaling_reconnect_reuses_the_admitted_viewer() {
        let (service, _) = service(1).await;
        let actor = PrincipalId::new("alice").unwrap();
        let opened = service
            .open(owner(), actor.clone(), request("browser-a"))
            .await
            .unwrap();

        assert!(
            service
                .authorize_signaling(
                    &opened.stream.live_view_id,
                    opened.access_token.expose_for_signaling(),
                    SignalingAdmission::Reconnect,
                )
                .await
                .is_err()
        );
        let first = service
            .authorize_signaling(
                &opened.stream.live_view_id,
                opened.access_token.expose_for_signaling(),
                SignalingAdmission::Initial,
            )
            .await
            .unwrap();
        assert_eq!(first.admission, SignalingAdmission::Initial);
        let resumed = service
            .authorize_signaling(
                &opened.stream.live_view_id,
                opened.access_token.expose_for_signaling(),
                SignalingAdmission::Reconnect,
            )
            .await
            .unwrap();
        assert_eq!(resumed.admission, SignalingAdmission::Reconnect);
        service
            .cancel_signaling_admission(&opened.stream.live_view_id, SignalingAdmission::Reconnect)
            .await;

        let live = service
            .list(&owner(), &actor, &opened.stream.session_id)
            .await
            .remove(0);
        assert_eq!(live.lifecycle, LiveViewLifecycle::Live);
        assert_eq!(live.connected_viewers, 1);
    }

    #[tokio::test]
    async fn product_usage_projects_only_the_matching_connected_lease() {
        let (service, adapter) = service(2).await;
        let opened = service
            .open(
                owner(),
                PrincipalId::new("alice").unwrap(),
                request("browser-a"),
            )
            .await
            .unwrap();
        service
            .authorize_signaling(
                &opened.stream.live_view_id,
                opened.access_token.expose_for_signaling(),
                SignalingAdmission::Initial,
            )
            .await
            .unwrap();

        let mut state = adapter.state().await.unwrap();
        service.project_product_usage(&mut state).await;

        let assigned = &state.stream_products[usize::from(opened.stream.capacity_slot)];
        assert_eq!(assigned.active_viewer_leases, 1);
        assert_eq!(assigned.connected_viewers, 1);
        let idle = &state.stream_products[1 - usize::from(opened.stream.capacity_slot)];
        assert_eq!(idle.active_viewer_leases, 0);
        assert_eq!(idle.connected_viewers, 0);
    }

    #[tokio::test]
    async fn explicit_close_releases_an_admitted_signaling_lease() {
        let (service, adapter) = service(1).await;
        let actor = PrincipalId::new("alice").unwrap();
        let opened = service
            .open(owner(), actor.clone(), request("browser-a"))
            .await
            .unwrap();
        service
            .authorize_signaling(
                &opened.stream.live_view_id,
                opened.access_token.expose_for_signaling(),
                SignalingAdmission::Initial,
            )
            .await
            .unwrap();

        let result = service
            .close(
                &owner(),
                &actor,
                CloseLiveViewRequest {
                    session_id: opened.stream.session_id,
                    live_view_id: opened.stream.live_view_id,
                    viewer_instance_id: LiveViewerInstanceId::new("browser-a").unwrap(),
                },
            )
            .await
            .unwrap();

        assert!(result.closed);
        assert_eq!(
            adapter.state().await.unwrap().stream_products[0].nvenc_sessions,
            0
        );
    }

    #[tokio::test]
    async fn mcp_restart_releases_stale_products_and_opens_a_fresh_lease() {
        let (before_restart, adapter) = service(1).await;
        let actor = PrincipalId::new("alice").unwrap();
        let opened = before_restart
            .open(owner(), actor.clone(), request("browser-a"))
            .await
            .unwrap();
        assert_eq!(
            adapter.state().await.unwrap().stream_products[0].nvenc_sessions,
            1
        );

        let after_restart =
            LiveViewService::new_for_test(adapter.clone(), test_config(Duration::from_secs(30), 1))
                .unwrap();
        after_restart.reconcile_untracked_products().await.unwrap();

        let released = adapter.state().await.unwrap().stream_products.remove(0);
        assert_eq!(
            released.lifecycle,
            veoveo_mcp_contract::LiveStreamProductLifecycle::Inactive
        );
        assert_eq!(released.nvenc_sessions, 0);
        assert!(released.camera_id.is_none());
        assert!(released.live_view_id.is_none());
        assert!(
            after_restart
                .list(
                    &owner(),
                    &actor,
                    &LiveSessionId::new("session-alpha").unwrap()
                )
                .await
                .is_empty()
        );

        let reopened = after_restart
            .open(owner(), actor, request("browser-a"))
            .await
            .unwrap();
        assert_ne!(reopened.stream.live_view_id, opened.stream.live_view_id);
        assert_eq!(reopened.stream.capacity_slot, opened.stream.capacity_slot);
        assert_eq!(
            adapter.state().await.unwrap().stream_products[0].nvenc_sessions,
            1
        );
    }
}
