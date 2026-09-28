//! Domain resource discovery, reads, completions, and subscription admission.
use super::super::{control_authority::ControlCollection, task_index};
use super::*;
use crate::contract::{
    UavDocument, UavLiveViewCursor, UavMissionCursor, UavPlanCursor, UavResource, UavUsageCursor,
};

impl UavSimMcp {
    pub(super) async fn resource_read(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ReadResourceResponse, McpError> {
        let cacheable = request.request_state.is_none() && request.input_responses.is_none();
        let resource = UavResource::parse(&request.uri).map_err(invalid)?;
        let identity = internal_identity(&context)?;
        require_resource_scope(&identity, &resource)?;
        self.read_address(&request.uri, &resource, &identity)
            .await
            .map(|result| veoveo_mcp_contract::private_resource_response(result, cacheable))
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
            UavResource::Docs => json_resource(uri, &SERVER_DOCS.iter().collect::<Vec<_>>()),
            UavResource::Document(document) => {
                let doc = SERVER_DOCS.doc(document.id()).ok_or_else(|| {
                    McpError::resource_not_found("unknown UAV simulation document", None)
                })?;
                Ok(ReadResourceResult::new(vec![
                    ResourceContents::text(doc.body, uri).with_mime_type("text/markdown"),
                ]))
            }
            UavResource::Contract => json_resource(uri, SERVER_DOCS.contract_declaration()),
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
                json_resource(uri, &page)
            }
            UavResource::ControlGrant(id) => {
                let grant = self
                    .state
                    .control_authority
                    .visible_grant(identity, identity_has_scope(identity, UavScope::Admin), id)
                    .await
                    .map_err(authority_error)?
                    .ok_or_else(|| McpError::resource_not_found("control grant not found", None))?;
                json_resource(uri, &grant)
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
                json_resource(uri, &page)
            }
            UavResource::MissionPlan(id) => {
                let plan = self
                    .state
                    .control_authority
                    .visible_plan(identity, identity_has_scope(identity, UavScope::Admin), id)
                    .await
                    .map_err(authority_error)?
                    .ok_or_else(|| McpError::resource_not_found("mission plan not found", None))?;
                json_resource(uri, &plan)
            }
            UavResource::Usage { cursor } => json_resource(
                uri,
                &task_index::usage_page(
                    &self.state.tasks,
                    identity,
                    cursor.as_ref().map(UavUsageCursor::position),
                )
                .await
                .map_err(internal)?,
            ),
            UavResource::Missions { cursor } => json_resource(
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
                json_resource(uri, &task_usage(&task, uri))
            }
            UavResource::Mission(id) => {
                let task = task_index::mission(self.state.tasks.platform_store(), identity, id)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| McpError::resource_not_found("mission not found", None))?;
                json_resource(uri, &task)
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
                    UavResource::Sessions => json_resource(uri, &vec![session_summary(&state)]),
                    UavResource::Session(_) => json_resource(uri, &state),
                    UavResource::World(_) => json_resource(uri, &world_view(&state)),
                    UavResource::Tiles(_) => json_resource(uri, &state.tiles),
                    UavResource::Vehicles(_) => json_resource(uri, &state.vehicles),
                    UavResource::Recordings(_) => json_resource(uri, &state.recordings),
                    UavResource::Vehicle { vehicle, .. } => {
                        let vehicle = state
                            .vehicles
                            .iter()
                            .find(|row| &row.vehicle_id == vehicle)
                            .ok_or_else(|| {
                                McpError::resource_not_found("vehicle not found", None)
                            })?;
                        json_resource(uri, vehicle)
                    }
                    UavResource::LiveCameras(_) => json_resource(uri, &state.live_cameras),
                    UavResource::LiveCamera { camera, .. } => {
                        let camera = state
                            .live_cameras
                            .iter()
                            .find(|row| &row.camera_id == camera)
                            .ok_or_else(|| {
                                McpError::resource_not_found("live camera not found", None)
                            })?;
                        json_resource(uri, camera)
                    }
                    UavResource::StreamProducts(_) => json_resource(uri, &state.stream_products),
                    UavResource::StreamProduct { product, .. } => {
                        let product = state
                            .stream_products
                            .iter()
                            .find(|row| &row.stream_product_id == product)
                            .ok_or_else(|| {
                                McpError::resource_not_found("stream product not found", None)
                            })?;
                        json_resource(uri, product)
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
                        json_resource(uri, &page)
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
                        json_resource(uri, &view)
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
        let identity = internal_identity(&context)?;
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
        if reference.uri == uris::DOC_TEMPLATE && request.argument.name == "doc_id" {
            return complete_values(
                SERVER_DOCS.iter().map(|doc| doc.id.to_owned()).collect(),
                &request.argument.value,
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
        uri: &str,
        context: &RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let resource = UavResource::parse(uri).map_err(invalid)?;
        let identity = internal_identity(context)?;
        require_resource_scope(&identity, &resource)?;
        if !resource.is_subscribable() {
            return Err(McpError::resource_not_found(
                "resource is not subscribable",
                None,
            ));
        }
        match &resource {
            UavResource::ControlGrants { .. }
            | UavResource::MissionPlans { .. }
            | UavResource::Usage { .. }
            | UavResource::Missions { .. } => Ok(()),
            UavResource::LiveViews { .. } => {
                self.resource_state(&resource, &identity).await.map(|_| ())
            }
            // Exact targets share the read path's SQL visibility, session and child checks.
            _ => self
                .read_address(uri, &resource, &identity)
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
        let identity = internal_identity(context)?;
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
            resources.push(live_app_resource(
                &self.state.live_view_connect_origin,
                &targets,
            ));
        }
        resources.sort_by(|left, right| left.uri.cmp(&right.uri));
        Ok(resources)
    }
}

fn discovery_roots(identity: &GatewayInternalIdentity) -> Vec<Resource> {
    let mut roots = well_known_resources();
    for (uri, title, description) in [
        (
            uris::SESSIONS,
            "Simulation sessions",
            "Authorized simulation session index.",
        ),
        (
            uris::MISSIONS,
            "Simulation missions",
            "Paged authorized mission resource URIs.",
        ),
        (
            uris::USAGE,
            "Simulation task usage",
            "Paged authorized task usage resource URIs.",
        ),
    ] {
        roots.push(descriptor(uri.into(), title.into(), description));
    }
    if identity_has_scope(identity, UavScope::Control)
        || identity_has_scope(identity, UavScope::Admin)
    {
        roots.push(descriptor(
            uris::CONTROL_GRANTS.into(),
            "Vehicle control grants".into(),
            "Paged UAV principal-to-vehicle grants.",
        ));
        roots.push(descriptor(
            uris::MISSION_PLANS.into(),
            "Vehicle mission plans".into(),
            "Paged plans admitted from Map route handoffs.",
        ));
    }
    roots
}

pub(in crate::server) async fn observe(
    store: veoveo_platform_store::PlatformStore,
    subscribers: Arc<SubscriptionHub>,
    shutdown: CancellationToken,
) {
    use futures::StreamExt;
    use veoveo_platform_store::{PlatformTable, ResourceChangeTable};
    let mut changes = store.resource_changes(vec![
        ResourceChangeTable::UavVehicleControlGrant,
        ResourceChangeTable::UavVehicleMissionPlan,
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

/// Well-known surface resources (contract C18, C19). `list_resources` serves
/// these for every authorized identity; `capability_inventory` declares the
/// same URIs at `uav-sim://contract`.
fn well_known_resources() -> Vec<Resource> {
    let mut resources = vec![descriptor(
        uris::DOCS.to_owned(),
        "Server documents".to_owned(),
        "Index of the crate documents embedded at build time.",
    )];
    for doc in SERVER_DOCS.iter() {
        resources.push(
            Resource::new(
                uris::doc(UavDocument::parse(doc.id).expect("declared UAV document")),
                doc.title,
            )
            .with_title(doc.title)
            .with_description("Crate document embedded at build time.")
            .with_mime_type("text/markdown"),
        );
    }
    resources.push(descriptor(
        uris::CONTRACT.to_owned(),
        "Contract declaration".to_owned(),
        "Machine-readable contract revision, compliance, and capability inventory.",
    ));
    resources
}

/// Every advertised resource template. `list_resource_templates` serves this
/// list and the `uav-sim://contract` capability inventory declares it, so the
/// two cannot diverge.
pub(super) fn resource_templates() -> Vec<ResourceTemplate> {
    vec![
        template(
            uris::CONTROL_GRANTS_PAGE_TEMPLATE,
            "Vehicle control grant page",
            "100 visible grants per page.",
        ),
        template(
            uris::MISSION_PLANS_PAGE_TEMPLATE,
            "Vehicle mission plan page",
            "100 visible plans per page.",
        ),
        template(
            uris::MISSIONS_PAGE_TEMPLATE,
            "Mission page",
            "100 authorized mission resource URIs per page.",
        ),
        template(
            uris::USAGE_PAGE_TEMPLATE,
            "Task usage page",
            "100 authorized task usage URIs per page.",
        ),
        ResourceTemplate::new(uris::DOC_TEMPLATE, "Server document")
            .with_title("Server document")
            .with_description("Embedded crate document body (contract C18).")
            .with_mime_type("text/markdown"),
        template(
            uris::SESSION_TEMPLATE,
            "Simulation session",
            "Typed session state.",
        ),
        template(
            uris::WORLD_TEMPLATE,
            "Simulation world",
            "Frame, georeference, and world clock state.",
        ),
        template(
            uris::TILES_TEMPLATE,
            "Simulation tiles",
            "Google Photorealistic 3D Tiles load state inside the simulator.",
        ),
        template(
            uris::VEHICLES_TEMPLATE,
            "Simulation vehicles",
            "Vehicle inventory for one session.",
        ),
        template(
            uris::VEHICLE_TEMPLATE,
            "Simulation vehicle",
            "Typed state for one simulated vehicle.",
        ),
        template(
            uris::RECORDINGS_TEMPLATE,
            "Simulation recordings",
            "Governed recording identities emitted by one session.",
        ),
        template(
            uris::LIVE_CAMERAS_TEMPLATE,
            "Live cameras",
            "Authoritative operator-camera inventory.",
        ),
        template(
            uris::LIVE_CAMERA_TEMPLATE,
            "Live camera",
            "One authoritative operator camera.",
        ),
        template(
            uris::STREAM_PRODUCTS_TEMPLATE,
            "Stream products",
            "Stable camera-owned rendered and encoded products.",
        ),
        template(
            uris::STREAM_PRODUCT_TEMPLATE,
            "Stream product",
            "One camera-owned RTX render and NVIDIA NVENC product shared across viewers.",
        ),
        template(
            uris::LIVE_VIEWS_TEMPLATE,
            "Live views",
            "Caller-visible live-view authorizations without secret tokens.",
        ),
        template(
            uris::LIVE_VIEWS_PAGE_TEMPLATE,
            "Live view page",
            "100 caller-visible live-view authorizations per page.",
        ),
        template(
            uris::LIVE_VIEW_TEMPLATE,
            "Live view",
            "One caller-visible live-view authorization without its token.",
        ),
        template(
            uris::MISSION_TEMPLATE,
            "Simulation mission",
            "Authorized durable mission task state.",
        ),
        template(
            uris::CONTROL_GRANT_TEMPLATE,
            "Vehicle control grant",
            "One UAV-owned principal-to-vehicle authority grant.",
        ),
        template(
            uris::MISSION_PLAN_TEMPLATE,
            "Vehicle mission plan",
            "One UAV-owned plan admitted from a Map route handoff.",
        ),
        template(
            uris::USAGE_TASK_TEMPLATE,
            "Simulation task usage",
            "Usage report for one authorized task.",
        ),
    ]
}

fn descriptor(uri: String, title: String, description: &str) -> Resource {
    Resource::new(uri, title.clone())
        .with_title(title)
        .with_description(description)
        .with_mime_type("application/json")
}

fn template(uri: &str, title: &str, description: &str) -> ResourceTemplate {
    ResourceTemplate::new(uri, title)
        .with_title(title)
        .with_description(description)
        .with_mime_type("application/json")
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

fn complete_values(values: Vec<String>, needle: &str) -> Result<CompleteResult, McpError> {
    let needle = needle.to_lowercase();
    let mut matches = values
        .into_iter()
        .filter(|value| value.to_lowercase().contains(&needle))
        .collect::<Vec<_>>();
    matches.sort();
    matches.dedup();
    let total = matches.len();
    matches.truncate(CompletionInfo::MAX_VALUES);
    let completion = CompletionInfo::with_pagination(
        matches,
        Some(total as u32),
        total > CompletionInfo::MAX_VALUES,
    )
    .map_err(internal)?;
    Ok(CompleteResult::new(completion))
}

fn json_resource<T: Serialize>(uri: &str, value: &T) -> Result<ReadResourceResult, McpError> {
    Ok(ReadResourceResult::new(vec![
        ResourceContents::text(serde_json::to_string(value).map_err(internal)?, uri)
            .with_mime_type("application/json"),
    ]))
}

const LIVE_APP_ICON: &str = "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSIyNCIgaGVpZ2h0PSIyNCIgdmlld0JveD0iMCAwIDI0IDI0IiBmaWxsPSJub25lIiBzdHJva2U9IiM2NmU0ZmYiIHN0cm9rZS13aWR0aD0iMiI+PHJlY3QgeD0iMiIgeT0iNSIgd2lkdGg9IjIwIiBoZWlnaHQ9IjE0IiByeD0iMiIvPjxwYXRoIGQ9Im04IDlsNiAzLTYgM3oiLz48L3N2Zz4=";

pub(super) fn live_app_resource(
    connect_origin: &str,
    agent_message_targets: &[String],
) -> Resource {
    let resource = veoveo_mcp_apps_extension::app_resource_with_meta(
        uris::LIVE_APP_URI,
        "uav-sim-live-app",
        veoveo_mcp_apps_extension::ResourceUiMeta {
            csp: Some(veoveo_mcp_apps_extension::UiCsp {
                connect_domains: vec![connect_origin.to_owned()],
                ..Default::default()
            }),
            ..Default::default()
        },
    )
    .with_title("Live Cameras")
    .with_description(
        "Authoritative simulator cameras tiled into one native NVIDIA NVENC product shared across viewers.",
    )
    .with_icons(vec![rmcp::model::Icon::new(LIVE_APP_ICON)]);
    if agent_message_targets.is_empty() {
        resource
    } else {
        veoveo_mcp_apps_extension::with_agent_message_targets(
            resource,
            agent_message_targets.iter().cloned(),
        )
        .expect("validated UAV App agent message targets")
    }
}

#[cfg(test)]
#[path = "resource_tests.rs"]
mod tests;
