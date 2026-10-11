use std::{collections::BTreeMap, sync::Arc, time::Duration};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tokio::sync::{Mutex, watch};
use uuid::Uuid;

use crate::contract::{
    LIVE_VIEW_SCHEMA, LiveCameraId, LiveCameraRig, LiveCameraStreamPolicy, LiveColorMatrix,
    LiveColorMetadata, LiveColorPrimaries, LiveColorRange, LiveColorTransfer, LiveMediaEndpoint,
    LiveMediaTransport, LiveSessionId, LiveViewAccessToken, LiveViewCodec, LiveViewConnection,
    LiveViewHardwareEncoder, LiveViewId, LiveViewLifecycle, LiveViewOwner, LiveViewState,
    LiveViewUri,
};
use veoveo_mcp_contract::audit::{AuditContext, AuditOutcome, AuditReason, LiveViewActivity};
use veoveo_types::PrincipalId;

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
    audit_context: AuditContext,
    token_hash: [u8; 32],
    generation: u64,
    events: watch::Sender<ViewerSignal>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ViewerSignal {
    pub(super) active: bool,
    pub(super) generation: u64,
    pub(super) expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub(super) struct StreamAuthorization {
    pub(super) state: LiveViewState,
    pub(super) generation: u64,
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
    state: Arc<Mutex<LiveViewStateStore>>,
    grants: super::live_view_store::LiveViewStore,
    issuer: super::live_view_store::LiveViewIssuerId,
    observation: tokio::sync::OnceCell<()>,
    stop: tokio_util::sync::CancellationToken,
    workers: tokio_util::task::TaskTracker,
}

impl LiveViewService {
    pub(super) async fn new(
        store: veoveo_platform_store::PlatformStore,
        adapter: Arc<Adapter>,
        audit: Arc<LiveViewAudit>,
        config: LiveViewConfig,
    ) -> anyhow::Result<Arc<Self>> {
        let service = Self::build(store, adapter, Some(audit), config)?;
        service.ensure_observation().await?;
        Ok(service)
    }

    #[cfg(test)]
    async fn new_for_test(
        store: veoveo_platform_store::PlatformStore,
        adapter: Arc<Adapter>,
        config: LiveViewConfig,
    ) -> anyhow::Result<Arc<Self>> {
        let service = Self::build(store, adapter, None, config)?;
        service.ensure_observation().await?;
        Ok(service)
    }
    #[cfg(test)]
    pub(super) fn build_for_fixture(
        store: veoveo_platform_store::PlatformStore,
        adapter: Arc<Adapter>,
        audit: Arc<LiveViewAudit>,
        config: LiveViewConfig,
    ) -> anyhow::Result<Arc<Self>> {
        Self::build(store, adapter, Some(audit), config)
    }

    fn build(
        store: veoveo_platform_store::PlatformStore,
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
            grants: super::live_view_store::LiveViewStore::new(store),
            issuer: super::live_view_store::LiveViewIssuerId::new(),
            observation: tokio::sync::OnceCell::new(),
            adapter,
            audit,
            config,
            state: Arc::new(Mutex::new(LiveViewStateStore {
                sessions: BTreeMap::new(),
            })),
            stop: tokio_util::sync::CancellationToken::new(),
            workers: tokio_util::task::TaskTracker::new(),
        }))
    }

    pub(super) async fn open(
        self: &Arc<Self>,
        owner: LiveViewOwner,
        audit_context: AuditContext,
        request: OpenLiveViewRequest,
    ) -> Result<LiveViewConnection, LiveViewError> {
        self.ensure_observation().await.map_err(storage_error)?;
        validate_audit_owner(&audit_context, &owner)?;
        let viewer_actor = audit_context.actor.principal.clone();
        let mut state = self.state.lock().await;
        if self.stop.is_cancelled() {
            return Err(LiveViewError::Audit);
        }
        if let Some(existing_id) = state.sessions.iter().find_map(|(id, session)| {
            (active(&session.state)
                && session.state.owner == owner
                && session.state.viewer_actor == viewer_actor
                && session.state.viewer_instance_id == request.viewer_instance_id
                && session.state.camera_id == request.camera_id)
                .then(|| id.clone())
        }) {
            let mut candidate = state.sessions[&existing_id].clone();
            candidate.audit_context = audit_context;
            let connection = rotate(&self.config, &mut candidate)?;
            self.record_required(&candidate, LiveViewActivity::Renew)
                .await?;
            let original = self
                .grants
                .owned(
                    &existing_id,
                    &owner,
                    &viewer_actor,
                    Some(&request.session_id),
                    Some(&request.viewer_instance_id),
                )
                .await
                .map_err(storage_error)?
                .ok_or(LiveViewError::ViewUnavailable)?;
            if original.issuer != self.issuer.as_uuid() || !active(&original.state.0) {
                return Err(LiveViewError::ViewUnavailable);
            }
            if !self
                .grants
                .replace(&original, self.grant(&candidate, None))
                .await
                .map_err(storage_error)?
            {
                return Err(LiveViewError::Storage);
            }
            let generation = candidate.generation;
            let _ = candidate
                .events
                .send_replace(signal_generation(&candidate.state, candidate.generation));
            state.sessions.insert(existing_id.clone(), candidate);
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
            .find(|product| product.region(&camera.camera_id).is_some())
            .cloned()
            .ok_or(LiveViewError::CameraUnavailable)?;
        let source_region = product
            .region(&camera.camera_id)
            .cloned()
            .ok_or(LiveViewError::CameraUnavailable)?;
        if product.lifecycle == crate::contract::LiveStreamProductLifecycle::Failed {
            return Err(LiveViewError::CameraUnavailable);
        }

        let now = Utc::now();
        let expires_at = expiry(now, self.config.session_duration)?;
        let live_view_id = LiveViewId::parse(format!("view-{}", Uuid::now_v7()))
            .map_err(|_| LiveViewError::Identifier)?;
        let token = new_token()?;
        let resource_uri =
            LiveViewUri::new(crate::uris::live_view(&request.session_id, &live_view_id).as_str())
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
            coded_width_px: product.coded_width_px,
            coded_height_px: product.coded_height_px,
            source_region,
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
        let candidate = ViewerSession {
            state: stream.clone(),
            audit_context,
            token_hash: token_hash(&token),
            generation: 1,
            events,
        };
        self.record_required(&candidate, LiveViewActivity::Issue)
            .await?;
        self.grants
            .create(self.grant(&candidate, None))
            .await
            .map_err(storage_error)?;
        state.sessions.insert(live_view_id.clone(), candidate);
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
        audit_context: &AuditContext,
        request: RenewLiveViewRequest,
    ) -> Result<LiveViewConnection, LiveViewError> {
        self.ensure_observation().await.map_err(storage_error)?;
        validate_audit_owner(audit_context, owner)?;
        let original = self
            .grants
            .stable_owned(
                &request.live_view_id,
                owner,
                &audit_context.actor.principal,
                Some(&request.session_id),
                Some(&request.viewer_instance_id),
            )
            .await
            .map_err(storage_error)?
            .ok_or_else(|| LiveViewError::ViewNotFound(request.live_view_id.clone()))?;
        if original.issuer != self.issuer.as_uuid() {
            return Err(LiveViewError::ViewUnavailable);
        }
        if !active(&original.state.0) {
            return Err(LiveViewError::ViewUnavailable);
        }
        if original.state.0.owner != *owner {
            let mut next = original.clone();
            next.state.0.lifecycle = LiveViewLifecycle::Closed;
            next.state.0.connected_viewers = 0;
            next.token_hash.fill(0);
            next.generation += 1;
            next.closed_at = Some(Utc::now());
            let draft = LiveViewAudit::draft(
                audit_context,
                crate::uris::live_view(&request.session_id, &request.live_view_id),
                LiveViewActivity::Revoke,
                AuditOutcome::Succeeded,
                AuditReason::Accepted,
            )
            .map_err(|_| LiveViewError::Audit)?;
            if !self
                .commit_revocation(&original, next, draft)
                .await
                .map_err(storage_error)?
            {
                return Err(LiveViewError::Storage);
            }
            let mut local = self.state.lock().await;
            if let Some(session) = local.sessions.get_mut(&request.live_view_id) {
                close_session(session);
            }
            return Err(LiveViewError::AuthorityRevoked);
        }
        let mut state = self.state.lock().await;
        let session = state
            .sessions
            .get(&request.live_view_id)
            .ok_or(LiveViewError::ViewUnavailable)?;
        authorize_owner(
            session,
            owner,
            &audit_context.actor.principal,
            &request.viewer_instance_id,
        )?;
        let mut candidate = session.clone();
        candidate.state = original.state.0.clone();
        candidate.generation = original.generation as u64;
        candidate.audit_context = audit_context.clone();
        let connection = rotate(&self.config, &mut candidate)?;
        self.record_required(&candidate, LiveViewActivity::Renew)
            .await?;
        if !self
            .grants
            .replace(&original, self.grant(&candidate, None))
            .await
            .map_err(storage_error)?
        {
            return Err(LiveViewError::Storage);
        }
        let generation = candidate.generation;
        let _ = candidate
            .events
            .send_replace(signal_generation(&candidate.state, generation));
        state
            .sessions
            .insert(request.live_view_id.clone(), candidate);
        drop(state);
        self.arm_expiry(
            request.live_view_id,
            generation,
            connection.stream.expires_at,
        );
        Ok(connection)
    }

    pub(super) async fn close(
        &self,
        owner: &LiveViewOwner,
        audit_context: &AuditContext,
        request: CloseLiveViewRequest,
    ) -> Result<CloseLiveViewResult, LiveViewError> {
        self.ensure_observation().await.map_err(storage_error)?;
        validate_audit_owner(audit_context, owner)?;
        // A bounded CAS loser reread never turns absence into a closed receipt.
        for _ in 0..3 {
            let original = self
                .grants
                .stable_owned(
                    &request.live_view_id,
                    owner,
                    &audit_context.actor.principal,
                    Some(&request.session_id),
                    Some(&request.viewer_instance_id),
                )
                .await
                .map_err(storage_error)?
                .ok_or_else(|| LiveViewError::ViewNotFound(request.live_view_id.clone()))?;
            if original.closed_at.is_some() {
                return Ok(CloseLiveViewResult {
                    resource_uri: original.state.0.resource_uri.as_str().to_owned(),
                    closed: true,
                });
            }
            let mut next = original.clone();
            next.state.0.lifecycle = LiveViewLifecycle::Closed;
            next.state.0.connected_viewers = 0;
            next.token_hash.fill(0);
            next.generation = next
                .generation
                .checked_add(1)
                .ok_or(LiveViewError::Storage)?;
            next.closed_at = Some(Utc::now());
            {
                let mut local = self.state.lock().await;
                if let Some(session) = local.sessions.get_mut(&request.live_view_id) {
                    close_session(session);
                }
            }
            let draft = LiveViewAudit::draft(
                audit_context,
                crate::uris::live_view(&request.session_id, &request.live_view_id),
                LiveViewActivity::Close,
                AuditOutcome::Succeeded,
                AuditReason::Accepted,
            )
            .map_err(|_| LiveViewError::Audit)?;
            if self
                .commit_revocation(&original, next.clone(), draft)
                .await
                .map_err(storage_error)?
            {
                let mut state = self.state.lock().await;
                if let Some(local) = state.sessions.get_mut(&request.live_view_id) {
                    close_session(local);
                }
                drop(state);
                self.grants.prune().await.map_err(storage_error)?;
                return Ok(CloseLiveViewResult {
                    resource_uri: next.state.0.resource_uri.as_str().to_owned(),
                    closed: true,
                });
            }
        }
        Err(LiveViewError::Storage)
    }

    pub(super) async fn page(
        &self,
        owner: &LiveViewOwner,
        viewer_actor: &PrincipalId,
        session_id: &LiveSessionId,
        after: Option<&LiveViewId>,
    ) -> Result<Vec<LiveViewState>, LiveViewError> {
        self.ensure_observation().await.map_err(storage_error)?;
        Ok(self
            .grants
            .selected(owner, viewer_actor, Some(session_id), after)
            .await
            .map_err(storage_error)?
            .into_iter()
            .map(|row| row.state.0)
            .collect())
    }

    pub(super) async fn get(
        &self,
        owner: &LiveViewOwner,
        viewer_actor: &PrincipalId,
        live_view_id: &LiveViewId,
    ) -> Result<LiveViewState, LiveViewError> {
        self.ensure_observation().await.map_err(storage_error)?;
        let session = self
            .grants
            .owned(live_view_id, owner, viewer_actor, None, None)
            .await
            .map_err(storage_error)?
            .ok_or_else(|| LiveViewError::ViewNotFound(live_view_id.clone()))?;
        let mut result = session.state.0;
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
                        && product.region(&result.camera_id).is_some()
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

    pub(super) async fn project_product_usage(
        &self,
        simulation: &mut SimulationState,
    ) -> Result<(), LiveViewError> {
        self.ensure_observation().await.map_err(storage_error)?;
        let products = simulation
            .stream_products
            .iter()
            .map(|product| product.stream_product_id.clone())
            .collect::<Vec<_>>();
        let rows = self
            .grants
            .usage(&simulation.session_id, &products)
            .await
            .map_err(storage_error)?;
        for product in &mut simulation.stream_products {
            product.active_viewers = 0;
            product.connected_viewers = 0;
        }
        for row in rows {
            let id = crate::contract::LiveStreamProductId::parse(row.product)
                .map_err(|_| LiveViewError::Storage)?;
            if let Some(product) = simulation
                .stream_products
                .iter_mut()
                .find(|product| product.stream_product_id == id)
            {
                product.active_viewers =
                    u32::try_from(row.active).map_err(|_| LiveViewError::Storage)?;
                product.connected_viewers =
                    u32::try_from(row.connected).map_err(|_| LiveViewError::Storage)?;
            }
        }
        Ok(())
    }

    pub(super) async fn authorize_stream(
        &self,
        live_view_id: &LiveViewId,
        token: &str,
    ) -> Result<StreamAuthorization, LiveViewError> {
        self.ensure_observation().await.map_err(storage_error)?;
        let grant = self
            .grants
            .get(live_view_id)
            .await
            .map_err(storage_error)?
            .ok_or_else(|| LiveViewError::ViewNotFound(live_view_id.clone()))?;
        if grant.issuer != self.issuer.as_uuid() || !active(&grant.state.0) {
            return Err(LiveViewError::Access);
        }
        let supplied: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        let mut state = self.state.lock().await;
        let session = state
            .sessions
            .get_mut(live_view_id)
            .ok_or_else(|| LiveViewError::ViewNotFound(live_view_id.clone()))?;
        if session.generation as i64 != grant.generation
            || !active(&session.state)
            || supplied.ct_eq(&session.token_hash).unwrap_u8() != 1
        {
            return Err(LiveViewError::Access);
        }
        if session.state.connected_viewers != 0 {
            return Err(LiveViewError::Access);
        }
        let mut next = grant.clone();
        next.state.0.connected_viewers = 1;
        next.state.0.lifecycle = LiveViewLifecycle::Live;
        match self.grants.replace(&grant, next.clone()).await {
            Ok(true) if !self.stop.is_cancelled() => {}
            _ => {
                for session in state.sessions.values_mut() {
                    close_session(session);
                }
                self.stop.cancel();
                return Err(LiveViewError::Storage);
            }
        }
        let session = state
            .sessions
            .get_mut(live_view_id)
            .ok_or(LiveViewError::Access)?;
        session.state = next.state.0;
        Ok(StreamAuthorization {
            generation: session.generation,
            state: session.state.clone(),
            events: session.events.subscribe(),
        })
    }

    pub(super) async fn finish_stream(&self, live_view_id: &LiveViewId, generation: u64) {
        let outcome = async {
            self.ensure_observation().await?;
            let Some(original) = self.grants.get(live_view_id).await? else {
                anyhow::bail!("finishing stream grant absent");
            };
            if original.generation != generation as i64
                || original.issuer != self.issuer.as_uuid()
                || !active(&original.state.0)
            {
                return Ok::<_, anyhow::Error>(());
            }
            let mut next = original.clone();
            next.state.0.connected_viewers = 0;
            next.state.0.lifecycle = LiveViewLifecycle::Ready;
            if self.grants.replace(&original, next).await? {
                let mut local = self.state.lock().await;
                if let Some(session) = local.sessions.get_mut(live_view_id)
                    && session.generation == generation
                    && active(&session.state)
                {
                    session.state.connected_viewers = 0;
                    session.state.lifecycle = LiveViewLifecycle::Ready;
                }
            }
            Ok(())
        }
        .await;
        if outcome.is_err() {
            self.fail_closed().await;
        }
    }

    async fn record_required(
        &self,
        session: &ViewerSession,
        activity: LiveViewActivity,
    ) -> Result<(), LiveViewError> {
        if let Some(audit) = &self.audit {
            let draft = LiveViewAudit::draft(
                &session.audit_context,
                crate::uris::live_view(&session.state.session_id, &session.state.live_view_id),
                activity,
                AuditOutcome::Succeeded,
                AuditReason::Accepted,
            )
            .map_err(|_| LiveViewError::Audit)?;
            audit
                .required(draft)
                .await
                .map_err(|_| LiveViewError::Audit)?;
        }
        Ok(())
    }

    async fn commit_revocation(
        &self,
        original: &super::live_view_store::Grant,
        next: super::live_view_store::Grant,
        draft: veoveo_mcp_contract::audit::AuditDraft,
    ) -> anyhow::Result<bool> {
        match self.grants.revoke(original, next, draft).await {
            Ok(changed) => Ok(changed),
            Err(error) => {
                self.fail_closed().await;
                Err(error)
            }
        }
    }

    fn arm_expiry(
        self: &Arc<Self>,
        live_view_id: LiveViewId,
        generation: u64,
        expires_at: DateTime<Utc>,
    ) {
        let service = self.clone();
        self.workers.spawn(async move {
            let wait = (expires_at - Utc::now()).to_std().unwrap_or_default();
            tokio::select! { _ = tokio::time::sleep(wait) => {}, _ = service.stop.cancelled() => return }
            let outcome = async {
                let grant = service.grants.get(&live_view_id).await?.ok_or_else(||anyhow::anyhow!("expiry grant absent"))?;
                if grant.generation != generation as i64 || grant.closed_at.is_some() || grant.expires_at > Utc::now() { return Ok::<_,anyhow::Error>(()); }
                let mut state = service.state.lock().await;
                let session = state.sessions.get_mut(&live_view_id).ok_or_else(||anyhow::anyhow!("expiry projection absent"))?;
                let draft = LiveViewAudit::draft(&session.audit_context, crate::uris::live_view(&session.state.session_id, &live_view_id), LiveViewActivity::Expire, AuditOutcome::Succeeded, AuditReason::Accepted)?;
                drop(state);
                let mut next = grant.clone();
                next.state.0.lifecycle = LiveViewLifecycle::Closed; next.state.0.connected_viewers = 0; next.token_hash.fill(0); next.generation += 1; next.closed_at = Some(Utc::now());
                if service.commit_revocation(&grant,next,draft).await? {
                    let mut local = service.state.lock().await;
                    if let Some(session) = local.sessions.get_mut(&live_view_id) && session.generation == generation { close_session(session); }
                }
                Ok(())
            }.await;
            if outcome.is_err() { service.fail_closed().await; }

        });
    }

    /// HTTP and stream admission must stop before the session drain.
    pub(super) async fn shutdown(&self) -> anyhow::Result<()> {
        // No admission/observer restart is allowed once shutdown starts.
        self.stop.cancel();
        self.workers.close();
        tokio::time::timeout(Duration::from_secs(20), async {
            self.workers.wait().await;
            let grants = self.grants.issuer_grants(&self.issuer).await?;
            let mut state = self.state.lock().await;
            for session in state.sessions.values_mut() {
                close_session(session);
            }
            for original in grants {
                let session = state
                    .sessions
                    .get(&original.state.0.live_view_id)
                    .ok_or_else(|| anyhow::anyhow!("owned grant lacks shutdown audit context"))?;
                let draft = LiveViewAudit::draft(
                    &session.audit_context,
                    crate::uris::live_view(&session.state.session_id, &session.state.live_view_id),
                    LiveViewActivity::Close,
                    AuditOutcome::Succeeded,
                    AuditReason::Accepted,
                )?;
                let mut next = original.clone();
                next.state.0.lifecycle = LiveViewLifecycle::Closed;
                next.state.0.connected_viewers = 0;
                next.token_hash.fill(0);
                next.generation += 1;
                next.closed_at = Some(Utc::now());
                self.grants.revoke(&original, next, draft).await?;
            }
            Ok::<_, anyhow::Error>(())
        })
        .await??;
        Ok(())
    }
}

fn storage_error(_: anyhow::Error) -> LiveViewError {
    LiveViewError::Storage
}

fn validate_audit_owner(
    context: &AuditContext,
    owner: &LiveViewOwner,
) -> Result<(), LiveViewError> {
    if context.actor.tenant.as_ref() != Some(&owner.tenant)
        || context.authority.work_context.as_ref() != Some(&owner.work_context)
        || context.authority.policy_revision.as_ref() != Some(&owner.policy_revision)
    {
        return Err(LiveViewError::Ownership);
    }
    Ok(())
}

fn authorize_owner(
    session: &ViewerSession,
    owner: &LiveViewOwner,
    viewer_actor: &PrincipalId,
    viewer_instance_id: &crate::contract::LiveViewerInstanceId,
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

fn product_lifecycle(product: &crate::contract::LiveStreamProductState) -> LiveViewLifecycle {
    match product.lifecycle {
        crate::contract::LiveStreamProductLifecycle::Inactive
        | crate::contract::LiveStreamProductLifecycle::Starting => LiveViewLifecycle::Starting,
        crate::contract::LiveStreamProductLifecycle::Ready => LiveViewLifecycle::Ready,
        crate::contract::LiveStreamProductLifecycle::Failed => LiveViewLifecycle::Failed,
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
    session.generation = session.generation.saturating_add(1);
    let _ = session
        .events
        .send_replace(signal_generation(&session.state, session.generation));
}

fn rotate(
    config: &LiveViewConfig,
    session: &mut ViewerSession,
) -> Result<LiveViewConnection, LiveViewError> {
    let token = new_token()?;
    session.token_hash = token_hash(&token);
    session.state.connected_viewers = 0;
    session.state.lifecycle = LiveViewLifecycle::Ready;
    session.state.expires_at = expiry(Utc::now(), config.session_duration)?;
    session.generation = session.generation.saturating_add(1);
    Ok(LiveViewConnection {
        stream: session.state.clone(),
        access_token: token,
    })
}

fn signal(state: &LiveViewState) -> ViewerSignal {
    signal_generation(state, 1)
}
fn signal_generation(state: &LiveViewState, generation: u64) -> ViewerSignal {
    ViewerSignal {
        generation,
        active: active(state),
        expires_at: state.expires_at,
    }
}

fn new_token() -> Result<LiveViewAccessToken, LiveViewError> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| LiveViewError::Access)?;
    LiveViewAccessToken::parse(URL_SAFE_NO_PAD.encode(bytes)).map_err(|_| LiveViewError::Access)
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
    #[error("live-view persistence or observation unavailable; outcome requires reconciliation")]
    Storage,
    #[error("required live-view audit commit unavailable")]
    Audit,
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
    pub(super) fn audit_reason(&self) -> AuditReason {
        match self {
            Self::SessionNotFound(_) | Self::CameraNotFound(_) | Self::ViewNotFound(_) => {
                AuditReason::NotFound
            }
            Self::Ownership | Self::Access => AuditReason::PolicyDenied,
            Self::AuthorityRevoked => AuditReason::Revoked,
            Self::CameraUnavailable | Self::ViewUnavailable | Self::Audit | Self::Storage => {
                AuditReason::Unavailable
            }
            Self::Identifier | Self::Contract | Self::Time | Self::Runtime(_) => {
                AuditReason::InternalFailure
            }
        }
    }
}

#[path = "live_view/observer.rs"]
mod observer;
#[cfg(test)]
#[path = "live_view/tests.rs"]
mod tests;
