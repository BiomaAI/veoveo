use super::lease::{active_lease, close_lease};
use super::{advance_desired, check_revision, *};

impl SimulationViewService {
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
}

impl SimulationViewService {
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

pub(super) fn capacity_usage(
    state: &ServiceState,
    excluding: Option<&LiveCameraId>,
) -> CapacityUsage {
    let mut usage = CapacityUsage::default();
    for session in state.sessions.values() {
        if session.lifecycle != SessionLifecycle::Closed {
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

pub(super) fn selected_entity(definition: &CameraDefinition) -> Option<&str> {
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
