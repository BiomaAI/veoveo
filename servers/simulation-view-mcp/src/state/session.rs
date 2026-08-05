use super::lease::close_lease;
use super::{advance_session, check_revision, owned_session, owned_session_mut, *};

impl SimulationViewService {
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
        session.reconciliation.pose_authorization_renewal_at = None;
        session.reconciliation.retry_at = None;
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
        session.reconciliation.pose_authorization_renewal_at = None;
        session.reconciliation.retry_at = None;
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
}

impl SimulationViewService {
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
}
