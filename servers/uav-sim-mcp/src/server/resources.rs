//! Domain resource discovery, reads, completions, and subscription admission.
use super::super::{control_authority::ControlCollection, task_index};
use super::*;

impl UavSimMcp {
    pub(super) async fn resource_read(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ReadResourceResponse, McpError> {
        let cacheable = request.request_state.is_none() && request.input_responses.is_none();
        async {
            let uri = request.uri.as_str();
            if uri == uris::LIVE_APP_URI {
                require_scope(&context, UavScope::Stream)?;
                return Ok(ReadResourceResult::new(vec![
                    veoveo_mcp_apps_extension::app_html_contents(uri, crate::live_app::html()),
                ]));
            }
            // Well-known surface (contract C18, C19): readable by any identity
            // that can list resources.
            if uri == uris::DOCS {
                internal_identity(&context)?;
                return json_resource(uri, &SERVER_DOCS.iter().collect::<Vec<_>>());
            }
            if let Some(doc_id) = uris::parse_doc(uri) {
                internal_identity(&context)?;
                let doc = SERVER_DOCS.doc(doc_id).ok_or_else(|| {
                    McpError::resource_not_found("unknown UAV simulation document", None)
                })?;
                return Ok(ReadResourceResult::new(vec![
                    ResourceContents::text(doc.body, uri).with_mime_type("text/markdown"),
                ]));
            }
            if uri == uris::CONTRACT {
                internal_identity(&context)?;
                return json_resource(uri, SERVER_DOCS.contract_declaration());
            }
            if let Some(after) = index::parse::<ControlGrantId>(uri, uris::CONTROL_GRANTS)? {
                let identity = require_any_scope(&context, &[UavScope::Control, UavScope::Admin])?;
                let page = self
                    .state
                    .control_authority
                    .grants_page(
                        &identity,
                        identity_has_scope(&identity, UavScope::Admin),
                        None,
                        after.as_ref(),
                    )
                    .await
                    .map_err(authority_error)?;
                return json_resource(uri, &page);
            }
            if let Some(id) = uris::parse_control_grant(uri) {
                let identity = require_any_scope(&context, &[UavScope::Control, UavScope::Admin])?;
                let grant = self
                    .state
                    .control_authority
                    .visible_grant(
                        &identity,
                        identity_has_scope(&identity, UavScope::Admin),
                        &ControlGrantId::new(id).map_err(invalid)?,
                    )
                    .await
                    .map_err(authority_error)?
                    .ok_or_else(|| McpError::resource_not_found("control grant not found", None))?;
                return json_resource(uri, &grant);
            }
            if let Some(after) = index::parse::<MissionPlanId>(uri, uris::MISSION_PLANS)? {
                let identity = require_any_scope(&context, &[UavScope::Control, UavScope::Admin])?;
                let page = self
                    .state
                    .control_authority
                    .plans_page(
                        &identity,
                        identity_has_scope(&identity, UavScope::Admin),
                        after.as_ref(),
                    )
                    .await
                    .map_err(authority_error)?;
                return json_resource(uri, &page);
            }
            if let Some(id) = uris::parse_mission_plan(uri) {
                let identity = require_any_scope(&context, &[UavScope::Control, UavScope::Admin])?;
                let plan = self
                    .state
                    .control_authority
                    .visible_plan(
                        &identity,
                        identity_has_scope(&identity, UavScope::Admin),
                        &MissionPlanId::new(id).map_err(invalid)?,
                    )
                    .await
                    .map_err(authority_error)?
                    .ok_or_else(|| McpError::resource_not_found("mission plan not found", None))?;
                return json_resource(uri, &plan);
            }
            let identity = require_any_scope(
                &context,
                &[UavScope::Read, UavScope::Control, UavScope::Admin],
            )?;
            if let Some(after) =
                index::parse::<veoveo_task_runtime::TaskPageCursor>(uri, uris::USAGE)?
            {
                if after
                    .as_ref()
                    .is_some_and(|cursor| cursor.task_id.as_uuid().get_version_num() != 7)
                {
                    return Err(McpError::invalid_params("invalid UAV task cursor", None));
                }
                return json_resource(
                    uri,
                    &task_index::usage_page(&self.state.tasks, &identity, after.as_ref())
                        .await
                        .map_err(internal)?,
                );
            }
            if let Some(after) = index::parse::<crate::contract::MissionId>(uri, uris::MISSIONS)? {
                return json_resource(
                    uri,
                    &task_index::missions_page(
                        self.state.tasks.platform_store(),
                        &identity,
                        after.as_ref(),
                    )
                    .await
                    .map_err(internal)?,
                );
            }
            if let Some(id) = uris::parse_usage_task(uri) {
                let task_id = parse_task_id(id)?;
                let task = task_index::task(self.state.tasks.platform_store(), &identity, task_id)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| McpError::resource_not_found("task not found", None))?;
                return json_resource(uri, &task_usage(&task, uri));
            }
            if let Some(id) = uris::parse_mission(uri) {
                let task = task_index::mission(
                    self.state.tasks.platform_store(),
                    &identity,
                    &crate::contract::MissionId::new(id).map_err(invalid)?,
                )
                .await
                .map_err(internal)?
                .ok_or_else(|| McpError::resource_not_found("mission not found", None))?;
                return json_resource(uri, &task);
            }
            let state = self.visible_state(&identity).await?;
            if let Some(session_id) = uris::parse_live_cameras(uri) {
                require_scope(&context, UavScope::Stream)?;
                require_session(&state, session_id.as_str())?;
                return json_resource(uri, &state.live_cameras);
            }
            if let Some((session_id, camera_id)) = uris::parse_live_camera(uri) {
                require_scope(&context, UavScope::Stream)?;
                require_session(&state, session_id.as_str())?;
                let camera = state
                    .live_cameras
                    .iter()
                    .find(|camera| camera.camera_id == camera_id)
                    .ok_or_else(|| McpError::resource_not_found("live camera not found", None))?;
                return json_resource(uri, camera);
            }
            if let Some(session_id) = uris::parse_stream_products(uri) {
                require_scope(&context, UavScope::Stream)?;
                require_session(&state, session_id.as_str())?;
                return json_resource(uri, &state.stream_products);
            }
            if let Some((session_id, product_id)) = uris::parse_stream_product(uri) {
                require_scope(&context, UavScope::Stream)?;
                require_session(&state, session_id.as_str())?;
                let product = state
                    .stream_products
                    .iter()
                    .find(|product| product.stream_product_id == product_id)
                    .ok_or_else(|| {
                        McpError::resource_not_found("stream product not found", None)
                    })?;
                return json_resource(uri, product);
            }
            let collection_root = uri.split_once('?').map_or(uri, |(root, _)| root);
            if let Some(session_id) = uris::parse_live_views(collection_root) {
                let identity = require_scope(&context, UavScope::Stream)?;
                require_session(&state, session_id.as_str())?;
                let owner = crate::server::ownership::live_view_owner(&identity);
                let after =
                    index::parse::<crate::contract::LiveViewId>(uri, collection_root)?.flatten();
                let views = self
                    .state
                    .live_views
                    .page(&owner, &identity.actor.id, &session_id, after.as_ref())
                    .await;
                return json_resource(
                    uri,
                    &index::page(views, collection_root, |view| view.live_view_id.clone(), Ok)
                        .map_err(internal)?,
                );
            }
            if let Some((session_id, live_view_id)) = uris::parse_live_view(uri) {
                let identity = require_scope(&context, UavScope::Stream)?;
                require_session(&state, session_id.as_str())?;
                let owner = crate::server::ownership::live_view_owner(&identity);
                let view = self
                    .state
                    .live_views
                    .get(&owner, &identity.actor.id, &live_view_id)
                    .await
                    .map_err(live_view_error)?;
                return json_resource(uri, &view);
            }
            if uri == uris::SESSIONS {
                return json_resource(uri, &vec![session_summary(&state)]);
            }
            if let Some(session_id) = uris::parse_session(uri) {
                require_session(&state, session_id)?;
                return json_resource(uri, &state);
            }
            if let Some(session_id) = uris::parse_world(uri) {
                require_session(&state, session_id)?;
                return json_resource(uri, &world_view(&state));
            }
            if let Some(session_id) = uris::parse_tiles(uri) {
                require_session(&state, session_id)?;
                return json_resource(uri, &state.tiles);
            }
            if let Some(session_id) = uris::parse_vehicles(uri) {
                require_session(&state, session_id)?;
                return json_resource(uri, &state.vehicles);
            }
            if let Some((session_id, vehicle_id)) = uris::parse_vehicle(uri) {
                require_session(&state, session_id)?;
                let vehicle = state
                    .vehicles
                    .iter()
                    .find(|vehicle| vehicle.vehicle_id.as_str() == vehicle_id)
                    .ok_or_else(|| {
                        McpError::resource_not_found(
                            format!("Vehicle `{}` was not found in this session.", vehicle_id),
                            None,
                        )
                    })?;
                return json_resource(uri, vehicle);
            }
            if let Some(session_id) = uris::parse_recordings(uri) {
                require_session(&state, session_id)?;
                return json_resource(uri, &state.recordings);
            }
            Err(McpError::resource_not_found(
                format!("unknown UAV simulation resource `{uri}`"),
                None,
            ))
        }
        .await
        .map(|result| veoveo_mcp_contract::private_resource_response(result, cacheable))
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

impl UavSimMcp {
    pub(super) async fn require_subscribable(
        &self,
        uri: &str,
        context: &RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let identity = internal_identity(context)?;
        if uri == uris::CONTROL_GRANTS || uris::parse_control_grant(uri).is_some() {
            require_any_identity_scope(&identity, &[UavScope::Control, UavScope::Admin])?;
            if let Some(id) = uris::parse_control_grant(uri) {
                self.state
                    .control_authority
                    .visible_grant(
                        &identity,
                        identity_has_scope(&identity, UavScope::Admin),
                        &ControlGrantId::new(id).map_err(invalid)?,
                    )
                    .await
                    .map_err(authority_error)?
                    .ok_or_else(|| McpError::resource_not_found("control grant not found", None))?;
            }
            return Ok(());
        }
        if uri == uris::MISSION_PLANS || uris::parse_mission_plan(uri).is_some() {
            require_any_identity_scope(&identity, &[UavScope::Control, UavScope::Admin])?;
            if let Some(id) = uris::parse_mission_plan(uri) {
                self.state
                    .control_authority
                    .visible_plan(
                        &identity,
                        identity_has_scope(&identity, UavScope::Admin),
                        &MissionPlanId::new(id).map_err(invalid)?,
                    )
                    .await
                    .map_err(authority_error)?
                    .ok_or_else(|| McpError::resource_not_found("mission plan not found", None))?;
            }
            return Ok(());
        }
        require_any_identity_scope(
            &identity,
            &[UavScope::Read, UavScope::Control, UavScope::Admin],
        )?;
        if matches!(uri, uris::USAGE | uris::MISSIONS) {
            return Ok(());
        }
        if let Some(id) = uris::parse_usage_task(uri) {
            task_index::task(
                self.state.tasks.platform_store(),
                &identity,
                parse_task_id(id)?,
            )
            .await
            .map_err(internal)?
            .ok_or_else(|| McpError::resource_not_found("task not found", None))?;
            return Ok(());
        }
        if let Some(id) = uris::parse_mission(uri) {
            task_index::mission(
                self.state.tasks.platform_store(),
                &identity,
                &crate::contract::MissionId::new(id).map_err(invalid)?,
            )
            .await
            .map_err(internal)?
            .ok_or_else(|| McpError::resource_not_found("mission not found", None))?;
            return Ok(());
        }
        let state = self.visible_state(&identity).await?;
        if let Some(session_id) = live_session_from_subscribable(uri) {
            require_scope(context, UavScope::Stream)?;
            require_session(&state, session_id.as_str())?;
            if let Some((_, camera_id)) = uris::parse_live_camera(uri)
                && !state
                    .live_cameras
                    .iter()
                    .any(|camera| camera.camera_id == camera_id)
            {
                return Err(McpError::resource_not_found("live camera not found", None));
            }
            if let Some((_, product_id)) = uris::parse_stream_product(uri)
                && !state
                    .stream_products
                    .iter()
                    .any(|product| product.stream_product_id == product_id)
            {
                return Err(McpError::resource_not_found(
                    "stream product not found",
                    None,
                ));
            }
            if let Some((_, live_view_id)) = uris::parse_live_view(uri) {
                let identity = internal_identity(context)?;
                let owner = crate::server::ownership::live_view_owner(&identity);
                self.state
                    .live_views
                    .get(&owner, &identity.actor.id, &live_view_id)
                    .await
                    .map_err(live_view_error)?;
            }
            return Ok(());
        }
        if let Some(session_id) = session_from_subscribable(uri) {
            require_session(&state, session_id)?;
            if let Some((_, vehicle_id)) = uris::parse_vehicle(uri)
                && !state
                    .vehicles
                    .iter()
                    .any(|vehicle| vehicle.vehicle_id.as_str() == vehicle_id)
            {
                return Err(McpError::resource_not_found(
                    format!("Vehicle `{}` was not found in this session.", vehicle_id),
                    None,
                ));
            }
            return Ok(());
        }
        Err(McpError::resource_not_found(
            "resource is not subscribable",
            None,
        ))
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

fn parse_task_id(value: &str) -> Result<veoveo_types::TaskId, McpError> {
    let id: veoveo_types::TaskId = value.parse().map_err(invalid)?;
    if id.as_uuid().get_version_num() != 7 {
        return Err(McpError::invalid_params("task ID must be UUIDv7", None));
    }
    Ok(id)
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
            Resource::new(uris::doc(doc.id), doc.title)
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

fn session_from_subscribable(uri: &str) -> Option<&str> {
    uris::parse_session(uri)
        .or_else(|| uris::parse_world(uri))
        .or_else(|| uris::parse_tiles(uri))
        .or_else(|| uris::parse_vehicles(uri))
        .or_else(|| uris::parse_recordings(uri))
        .or_else(|| uris::parse_vehicle(uri).map(|(session_id, _)| session_id))
}

fn live_session_from_subscribable(uri: &str) -> Option<LiveSessionId> {
    uris::parse_live_cameras(uri)
        .or_else(|| uris::parse_live_camera(uri).map(|(session, _)| session))
        .or_else(|| uris::parse_stream_products(uri))
        .or_else(|| uris::parse_stream_product(uri).map(|(session, _)| session))
        .or_else(|| uris::parse_live_views(uri))
        .or_else(|| uris::parse_live_view(uri).map(|(session, _)| session))
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
