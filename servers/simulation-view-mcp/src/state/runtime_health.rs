use super::lease::{active_lease, close_lease};
use super::*;

impl SimulationViewService {
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
}
