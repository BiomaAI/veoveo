//! Domain resource discovery, reads, completions, and subscription admission.
use super::super::{control_authority::ControlCollection, task_index};
use super::*;
use crate::contract::{UavLiveViewCursor, UavMissionCursor, UavPlanCursor, UavUsageCursor};
use veoveo_mcp_contract::hosting::{completion, json_read, rank_completions, served_by_host};
use veoveo_types::ResourceAddress;

impl UavSimMcp {
    /// Reads one admitted address. The host serves documents and the contract.
    pub(super) async fn resource_read(
        &self,
        resource: UavResource,
        uri: &str,
        context: &RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let identity = gateway_identity(context)?;
        require_resource_scope(&identity, &resource)?;
        self.read_address(uri, &resource, &identity).await
    }

    async fn read_address(
        &self,
        uri: &str,
        resource: &UavResource,
        identity: &GatewayInternalIdentity,
    ) -> Result<ReadResourceResult, McpError> {
        match resource {
            UavResource::LiveApp => Ok(ReadResourceResult::new(vec![
                veoveo_mcp_apps_extension::app_html_contents(uri, crate::live_app::html()),
            ])),
            UavResource::Docs | UavResource::Document(_) | UavResource::Contract => {
                Err(served_by_host())
            }
            UavResource::ControlGrants { cursor } => {
                let page = self
                    .state
                    .control_authority
                    .grants_page(
                        identity,
                        identity_has_scope(identity, UavScope::Admin),
                        None,
                        cursor.as_ref().map(UavGrantCursor::position),
                    )
                    .await
                    .map_err(authority_error)?;
                json_read(uri, &page)
            }
            UavResource::ControlGrant(id) => {
                let grant = self
                    .state
                    .control_authority
                    .visible_grant(identity, identity_has_scope(identity, UavScope::Admin), id)
                    .await
                    .map_err(authority_error)?
                    .ok_or_else(|| McpError::resource_not_found("control grant not found", None))?;
                json_read(uri, &grant)
            }
            UavResource::MissionPlans { cursor } => {
                let page = self
                    .state
                    .control_authority
                    .plans_page(
                        identity,
                        identity_has_scope(identity, UavScope::Admin),
                        cursor.as_ref().map(UavPlanCursor::position),
                    )
                    .await
                    .map_err(authority_error)?;
                json_read(uri, &page)
            }
            UavResource::MissionPlan(id) => {
                let plan = self
                    .state
                    .control_authority
                    .visible_plan(identity, identity_has_scope(identity, UavScope::Admin), id)
                    .await
                    .map_err(authority_error)?
                    .ok_or_else(|| McpError::resource_not_found("mission plan not found", None))?;
                json_read(uri, &plan)
            }
            UavResource::Usage { cursor } => json_read(
                uri,
                &task_index::usage_page(
                    &self.state.tasks,
                    identity,
                    cursor.as_ref().map(UavUsageCursor::position),
                )
                .await
                .map_err(internal)?,
            ),
            UavResource::Missions { cursor } => json_read(
                uri,
                &task_index::missions_page(
                    self.state.tasks.platform_store(),
                    identity,
                    cursor.as_ref().map(UavMissionCursor::position),
                )
                .await
                .map_err(internal)?,
            ),
            UavResource::UsageTask(id) => {
                let task = task_index::task(self.state.tasks.platform_store(), identity, *id)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| McpError::resource_not_found("task not found", None))?;
                json_read(uri, &task_usage(&task, uri))
            }
            UavResource::Mission(id) => {
                let task = task_index::mission(self.state.tasks.platform_store(), identity, id)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| McpError::resource_not_found("mission not found", None))?;
                json_read(uri, &task)
            }
            UavResource::Sessions
            | UavResource::Session(_)
            | UavResource::World(_)
            | UavResource::Tiles(_)
            | UavResource::Vehicles(_)
            | UavResource::Recordings(_)
            | UavResource::Vehicle { .. }
            | UavResource::LiveCameras(_)
            | UavResource::LiveCamera { .. }
            | UavResource::StreamProducts(_)
            | UavResource::StreamProduct { .. }
            | UavResource::LiveViews { .. }
            | UavResource::LiveView { .. } => {
                let state = self.resource_state(resource, identity).await?;
                match resource {
                    UavResource::Sessions => json_read(uri, &vec![session_summary(&state)]),
                    UavResource::Session(_) => json_read(uri, &state),
                    UavResource::World(_) => json_read(uri, &world_view(&state)),
                    UavResource::Tiles(_) => json_read(uri, &state.tiles),
                    UavResource::Vehicles(_) => json_read(uri, &state.vehicles),
                    UavResource::Recordings(_) => json_read(uri, &state.recordings),
                    UavResource::Vehicle { vehicle, .. } => {
                        let vehicle = state
                            .vehicles
                            .iter()
                            .find(|row| &row.vehicle_id == vehicle)
                            .ok_or_else(|| {
                                McpError::resource_not_found("vehicle not found", None)
                            })?;
                        json_read(uri, vehicle)
                    }
                    UavResource::LiveCameras(_) => json_read(uri, &state.live_cameras),
                    UavResource::LiveCamera { camera, .. } => {
                        let camera = state
                            .live_cameras
                            .iter()
                            .find(|row| &row.camera_id == camera)
                            .ok_or_else(|| {
                                McpError::resource_not_found("live camera not found", None)
                            })?;
                        json_read(uri, camera)
                    }
                    UavResource::StreamProducts(_) => json_read(uri, &state.stream_products),
                    UavResource::StreamProduct { product, .. } => {
                        let product = state
                            .stream_products
                            .iter()
                            .find(|row| &row.stream_product_id == product)
                            .ok_or_else(|| {
                                McpError::resource_not_found("stream product not found", None)
                            })?;
                        json_read(uri, product)
                    }
                    UavResource::LiveViews { session, cursor } => {
                        let owner = crate::server::ownership::live_view_owner(identity);
                        let views = self
                            .state
                            .live_views
                            .page(
                                &owner,
                                &identity.actor.id,
                                session,
                                cursor.as_ref().map(UavLiveViewCursor::position),
                            )
                            .await;
                        let page = index::page(
                            views,
                            |view| {
                                Ok(UavLiveViewCursor::new(
                                    session.clone(),
                                    view.live_view_id.clone(),
                                )?
                                .as_str()
                                .to_owned())
                            },
                            Ok,
                        )
                        .map_err(internal)?;
                        json_read(uri, &page)
                    }
                    UavResource::LiveView { session, view } => {
                        let owner = crate::server::ownership::live_view_owner(identity);
                        let view = self
                            .state
                            .live_views
                            .get(&owner, &identity.actor.id, view)
                            .await
                            .map_err(live_view_error)?;
                        if &view.session_id != session {
                            return Err(McpError::resource_not_found(
                                "live view not found in session",
                                None,
                            ));
                        }
                        json_read(uri, &view)
                    }
                    _ => Err(McpError::resource_not_found(
                        "unknown simulation state resource",
                        None,
                    )),
                }
            }
        }
    }

    async fn resource_state(
        &self,
        resource: &UavResource,
        identity: &GatewayInternalIdentity,
    ) -> Result<SimulationState, McpError> {
        let state = self.visible_state(identity).await?;
        match resource {
            UavResource::Session(session)
            | UavResource::World(session)
            | UavResource::Tiles(session)
            | UavResource::Vehicles(session)
            | UavResource::Recordings(session)
            | UavResource::Vehicle { session, .. } => require_session(&state, session)?,
            UavResource::LiveCameras(session)
            | UavResource::StreamProducts(session)
            | UavResource::LiveCamera { session, .. }
            | UavResource::StreamProduct { session, .. }
            | UavResource::LiveViews { session, .. }
            | UavResource::LiveView { session, .. } => require_live_session(&state, session)?,
            UavResource::Sessions => (),
            _ => {
                return Err(McpError::resource_not_found(
                    "unknown simulation state resource",
                    None,
                ));
            }
        }
        Ok(state)
    }

    pub(super) async fn resource_complete(
        &self,
        request: CompleteRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, McpError> {
        let Reference::Resource(reference) = &request.r#ref else {
            return Ok(CompleteResult::default());
        };
        let identity = gateway_identity(&context)?;
        index::validate_needle(&request.argument.value)?;
        let completion = match (reference.uri.as_str(), request.argument.name.as_str()) {
            (uris::CONTROL_GRANT_TEMPLATE, "grant_id") => Some(ControlCollection::Grants),
            (uris::MISSION_PLAN_TEMPLATE, "plan_id") => Some(ControlCollection::Plans),
            _ => None,
        };
        if let Some(domain) = completion {
            require_any_identity_scope(&identity, &[UavScope::Control, UavScope::Admin])?;
            return index::completion(
                self.state
                    .control_authority
                    .complete_ids(
                        &identity,
                        identity_has_scope(&identity, UavScope::Admin),
                        domain,
                        &request.argument.value,
                    )
                    .await
                    .map_err(authority_error)?,
            );
        }
        let completion = match (reference.uri.as_str(), request.argument.name.as_str()) {
            (uris::MISSION_TEMPLATE, "mission_id") => Some(task_index::CompletionDomain::Missions),
            (uris::USAGE_TASK_TEMPLATE, "task_id") => Some(task_index::CompletionDomain::Tasks),
            _ => None,
        };
        if let Some(domain) = completion {
            require_any_identity_scope(
                &identity,
                &[UavScope::Read, UavScope::Control, UavScope::Admin],
            )?;
            return index::completion(
                task_index::complete(
                    self.state.tasks.platform_store(),
                    &identity,
                    domain,
                    &request.argument.value,
                )
                .await
                .map_err(internal)?,
            );
        }
        let state = self.visible_state(&identity).await?;
        let values = match (reference.uri.as_str(), request.argument.name.as_str()) {
            (uris::SESSION_TEMPLATE, "session_id")
            | (uris::WORLD_TEMPLATE, "session_id")
            | (uris::TILES_TEMPLATE, "session_id")
            | (uris::VEHICLES_TEMPLATE, "session_id")
            | (uris::RECORDINGS_TEMPLATE, "session_id")
            | (uris::LIVE_CAMERAS_TEMPLATE, "session_id")
            | (uris::LIVE_CAMERA_TEMPLATE, "session_id")
            | (uris::STREAM_PRODUCTS_TEMPLATE, "session_id")
            | (uris::STREAM_PRODUCT_TEMPLATE, "session_id")
            | (uris::LIVE_VIEWS_TEMPLATE, "session_id")
            | (uris::LIVE_VIEWS_PAGE_TEMPLATE, "session_id")
            | (uris::LIVE_VIEW_TEMPLATE, "session_id")
            | (uris::VEHICLE_TEMPLATE, "session_id") => vec![state.session_id.to_string()],
            (uris::VEHICLE_TEMPLATE, "vehicle_id") => state
                .vehicles
                .iter()
                .map(|vehicle| vehicle.vehicle_id.to_string())
                .collect(),
            (uris::LIVE_CAMERA_TEMPLATE, "camera_id") => state
                .live_cameras
                .iter()
                .map(|camera| camera.camera_id.to_string())
                .collect(),
            (uris::STREAM_PRODUCT_TEMPLATE, "product_id") => state
                .stream_products
                .iter()
                .map(|product| product.stream_product_id.to_string())
                .collect(),
            _ => return Ok(CompleteResult::default()),
        };
        complete_values(values, &request.argument.value)
    }
}

/// Reads and subscription admission use the same domain permission rules.
fn require_resource_scope(
    identity: &GatewayInternalIdentity,
    resource: &UavResource,
) -> Result<(), McpError> {
    match resource {
        UavResource::Docs | UavResource::Document(_) | UavResource::Contract => Ok(()),
        UavResource::LiveApp => super::super::auth::require_scope(identity, UavScope::Stream),
        UavResource::ControlGrants { .. }
        | UavResource::ControlGrant(_)
        | UavResource::MissionPlans { .. }
        | UavResource::MissionPlan(_) => {
            require_any_identity_scope(identity, &[UavScope::Control, UavScope::Admin])
        }
        UavResource::LiveCameras(_)
        | UavResource::LiveCamera { .. }
        | UavResource::StreamProducts(_)
        | UavResource::StreamProduct { .. }
        | UavResource::LiveViews { .. }
        | UavResource::LiveView { .. } => {
            require_any_identity_scope(
                identity,
                &[UavScope::Read, UavScope::Control, UavScope::Admin],
            )?;
            super::super::auth::require_scope(identity, UavScope::Stream)
        }
        UavResource::Sessions
        | UavResource::Session(_)
        | UavResource::World(_)
        | UavResource::Tiles(_)
        | UavResource::Vehicles(_)
        | UavResource::Recordings(_)
        | UavResource::Vehicle { .. }
        | UavResource::Usage { .. }
        | UavResource::UsageTask(_)
        | UavResource::Missions { .. }
        | UavResource::Mission(_) => require_any_identity_scope(
            identity,
            &[UavScope::Read, UavScope::Control, UavScope::Admin],
        ),
    }
}

impl UavSimMcp {
    pub(super) async fn require_subscribable(
        &self,
        resource: &UavResource,
        context: &RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let uri = resource.to_uri().map_err(invalid)?;
        let identity = gateway_identity(context)?;
        require_resource_scope(&identity, resource)?;
        if !resource.is_subscribable() {
            return Err(McpError::resource_not_found(
                "resource is not subscribable",
                None,
            ));
        }
        match resource {
            UavResource::ControlGrants { .. }
            | UavResource::MissionPlans { .. }
            | UavResource::Usage { .. }
            | UavResource::Missions { .. } => Ok(()),
            UavResource::LiveViews { .. } => {
                self.resource_state(resource, &identity).await.map(|_| ())
            }
            // Exact targets share the read path's SQL visibility, session and child checks.
            _ => self
                .read_address(uri.as_str(), resource, &identity)
                .await
                .map(|_| ()),
        }
    }
}

impl UavSimMcp {
    pub(super) async fn resource_descriptors(
        &self,
        context: &RequestContext<RoleServer>,
    ) -> Result<Vec<Resource>, McpError> {
        let identity = gateway_identity(context)?;
        self.resource_descriptors_for_identity(&identity).await
    }

    pub(in crate::server) async fn resource_descriptors_for_identity(
        &self,
        identity: &GatewayInternalIdentity,
    ) -> Result<Vec<Resource>, McpError> {
        require_any_identity_scope(
            identity,
            &[UavScope::Read, UavScope::Control, UavScope::Admin],
        )?;
        let mut resources = discovery_roots(identity);
        if identity_has_scope(identity, UavScope::Stream) {
            let targets = super::super::agent_targets::targets(
                self.state.tasks.platform_store(),
                identity,
                &self.state.session_id,
            )
            .await
            .map_err(internal)?;
            resources.push(
                super::super::setup::live_app_resource(
                    &self.state.live_view_connect_origin,
                    &targets,
                )
                .map_err(internal)?,
            );
        }
        resources.sort_by(|left, right| left.uri.cmp(&right.uri));
        Ok(resources)
    }
}

fn discovery_roots(identity: &GatewayInternalIdentity) -> Vec<Resource> {
    SERVER_SETUP
        .resources()
        .iter()
        .filter(|resource| match resource.address() {
            UavResource::ControlGrants { .. } | UavResource::MissionPlans { .. } => {
                identity_has_scope(identity, UavScope::Control)
                    || identity_has_scope(identity, UavScope::Admin)
            }
            UavResource::LiveApp => false, // Caller metadata is attached after its SQL selection.
            _ => true,
        })
        .map(|resource| resource.descriptor().clone())
        .collect()
}

pub(in crate::server) async fn observe(
    store: veoveo_platform_store::PlatformStore,
    subscribers: Arc<SubscriptionHub>,
    shutdown: CancellationToken,
) {
    use futures::StreamExt;
    use veoveo_platform_store::PlatformTable;
    let mut changes = store.resource_changes(vec![
        veoveo_modules::ObservationTable::from(crate::UavObservationTable::UavVehicleControlGrant),
        crate::UavObservationTable::UavVehicleMissionPlan.into(),
        crate::UavObservationTable::UavTask.into(),
        PlatformTable::Task.into(),
    ]);
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            change = changes.next() => {
                if change.is_none() { break; }
                subscribers.notify_resource_contents_changed().await;
            }
        }
    }
}

fn session_summary(state: &SimulationState) -> serde_json::Value {
    json!({
        "session_id": state.session_id,
        "lifecycle": state.lifecycle,
        "world": state.world,
        "tile_lifecycle": state.tiles.lifecycle,
        "vehicle_count": state.vehicles.len(),
        "recording_count": state.recordings.len(),
        "timing": state.timing,
        "updated_at": state.updated_at,
    })
}

pub(super) fn world_view(state: &SimulationState) -> serde_json::Value {
    json!({
        "session_id": state.session_id,
        "simulation_time_s": state.simulation_time_s,
        "physics_step": state.physics_step,
        "timing": state.timing,
        "world": state.world,
        "updated_at": state.updated_at,
    })
}

fn task_usage(task: &TaskSnapshot, uri: &str) -> UsageReport {
    let operation = serde_json::from_value::<DurableOperation>(task.request.clone()).ok();
    let declared_duration = match operation.as_ref() {
        Some(DurableOperation::RunScenario(request)) => Some(request.duration_seconds),
        Some(DurableOperation::CaptureDataset(request)) => Some(request.duration_seconds),
        Some(DurableOperation::ExecuteMission(_)) | None => None,
    };
    let completed_duration = task
        .started_at
        .zip(task.completed_at)
        .map(|(started, completed)| (completed - started).num_milliseconds() as f64 / 1_000.0);
    let (kind, quantity) = if task.status == TaskStatus::Succeeded {
        (UsageKind::Actual, completed_duration.or(declared_duration))
    } else {
        (UsageKind::Estimate, declared_duration)
    };
    UsageReport::new(task.task_id.to_string(), uri).with_records(vec![UsageRecord {
        task_id: task.task_id.to_string(),
        source_id: None,
        provider_job_id: None,
        model_id: "isaac-sim-6.0.1".to_owned(),
        kind,
        quantity,
        unit: Some("gpu_second".to_owned()),
        amount: None,
        currency: None,
        recorded_at: task.completed_at.unwrap_or(task.updated_at),
        metadata: json!({"gpu_count": 1, "task_type": task.task_type}),
    }])
}

fn complete_values(mut values: Vec<String>, needle: &str) -> Result<CompleteResult, McpError> {
    values.sort();
    values.dedup();
    completion(rank_completions(values.iter().map(String::as_str), needle))
}

#[cfg(test)]
#[path = "resource_tests.rs"]
mod tests;
