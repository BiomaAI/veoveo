use super::camera::{capacity_usage, selected_entity};
use super::*;

impl SimulationViewService {
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
}

impl SimulationViewService {
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
}

pub(super) fn active_lease(stream: &LiveViewState) -> bool {
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

pub(super) fn close_lease(lease: &mut Lease) {
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
