use super::{advance_desired, *};

impl SimulationViewService {
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
        session.reconciliation.pose_authorization_renewal_at = Some(at);
        session.reconciliation.retry_at = Some(at);
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
        desired.session.reconciliation.pose_authorization_renewal_at = None;
        desired.session.reconciliation.retry_at = None;
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
                let retry_due = session
                    .reconciliation
                    .retry_at
                    .is_some_and(|retry_at| retry_at <= now);
                let renewal_due = session
                    .reconciliation
                    .pose_authorization_renewal_at
                    .is_some_and(|renewal_at| renewal_at <= now);
                if session.reconciliation.phase == ReconciliationPhase::Blocked {
                    retry_due
                } else {
                    session.reconciliation.desired_revision
                        > session.reconciliation.realized_revision
                        || session.reconciliation.phase == ReconciliationPhase::Pending
                        || renewal_due
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
            .filter_map(|session| {
                if session.reconciliation.phase == ReconciliationPhase::Blocked {
                    session.reconciliation.retry_at
                } else {
                    session.reconciliation.pose_authorization_renewal_at
                }
            })
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
            session.reconciliation.retry_at = None;
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
            session.reconciliation.retry_at = None;
            session.updated_at = now;
        }
    }

    pub(crate) fn reconciliation_ready(&self) -> bool {
        let now = Utc::now();
        self.state
            .lock()
            .expect("simulation-view state lock poisoned")
            .sessions
            .values()
            .all(|session| {
                let pose_authorization_ready = session.pose_source.as_ref().is_none_or(|source| {
                    source.revoked
                        || (source.expires_at > now
                            && session
                                .reconciliation
                                .pose_authorization_renewal_at
                                .is_some())
                });
                session.reconciliation.desired_revision == session.reconciliation.realized_revision
                    && matches!(
                        session.reconciliation.phase,
                        ReconciliationPhase::Healthy | ReconciliationPhase::Revoked
                    )
                    && pose_authorization_ready
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
            session.reconciliation.pose_authorization_renewal_at = Some(at);
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
        session.reconciliation.pose_authorization_renewal_at = None;
        session.reconciliation.retry_at = None;
        Ok(result)
    }

    pub(crate) fn mark_reconciliation_phase(
        &self,
        session_id: &LiveSessionId,
        phase: ReconciliationPhase,
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
            session.reconciliation.retry_at = None;
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
        retry_at: DateTime<Utc>,
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
            session.reconciliation.retry_at = Some(retry_at);
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
                session.reconciliation.pose_authorization_renewal_at = None;
            }
            session.reconciliation.retry_at = None;
            session.reconciliation.failed_dependency = None;
            session.reconciliation.failure_code = None;
            session.reconciliation.diagnostic = None;
            session.updated_at = at;
        }
    }
}
