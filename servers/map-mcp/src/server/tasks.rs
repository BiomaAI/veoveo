use std::{
    collections::BTreeSet,
    num::{NonZeroU32, NonZeroU64},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, bail};
use chrono::{TimeDelta, Utc};
use rmcp::model::{CallToolResult, ContentBlock, Resource};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::{
    ArtifactId, ArtifactProvenance, ArtifactPut, ArtifactWriteIdempotencyKey, ComplianceMetadata,
    GatewayInternalIdentity, InvocationMode, InvocationProvenance,
    IssueArtifactWriteCapabilityRequest, IssuedArtifactWriteCapability, PlaneCaller, PrincipalKind,
};
use veoveo_task_runtime::{
    CreateTask, RecoveryClass, TaskError, TaskFailure, TaskId, TaskOwner, TaskRetentionPin,
    TaskSnapshot, TaskTransition,
};

use crate::{
    contract::{
        BuildTravelModelRequest, BuildVectorTilesOutput, BuildVectorTilesRequest,
        DeriveRasterRequest, ExportFeatureLayerOutput, ExportFeatureLayerRequest,
        ImportFeatureLayerRequest, InspectGeoPackageRequest, LayerProduct, LayerProductId,
        MAX_RASTER_FULL_DERIVATION_PIXELS, RASTER_DERIVATION_SCHEMA_VERSION, RasterDerivation,
        RasterDerivationId, RasterDerivationOperation, ReachableAreaRequest, RouteMatrixRequest,
        RouteRequest, TravelModelId, TravelModelRecord,
    },
    server::auth::ForwardedBearer,
    state::MapApplication,
};

mod feature_transfers;

const SERVER_SLUG: &str = "map";
const ROUTE_TASK: &str = "route";
const ROUTE_MATRIX_TASK: &str = "route_matrix";
const BUILD_TRAVEL_MODEL_TASK: &str = "build_travel_model";
const REACHABLE_AREA_TASK: &str = "reachable_area";
const IMPORT_FEATURE_LAYER_TASK: &str = "import_feature_layer";
const INSPECT_GEOPACKAGE_TASK: &str = "inspect_geopackage";
const EXPORT_FEATURE_LAYER_TASK: &str = "export_feature_layer";
const BUILD_VECTOR_TILES_TASK: &str = "build_vector_tiles";
const DERIVE_RASTER_TASK: &str = "derive_raster";

const TASK_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
const TASK_POLL_INTERVAL_MS: u64 = 3_000;
const TASK_LEASE_DURATION: Duration = Duration::from_secs(120);
const TASK_LEASE_HEARTBEAT: Duration = Duration::from_secs(40);
const ARTIFACT_CAPABILITY_TTL: TimeDelta = TimeDelta::hours(24);

#[derive(Clone)]
pub(crate) struct MapTaskExtension {
    state: Arc<MapApplication>,
}

#[derive(Clone)]
pub(crate) struct AuthenticatedCaller {
    identity: GatewayInternalIdentity,
    caller: PlaneCaller,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DurableImportRequest {
    input: ImportFeatureLayerRequest,
    identity: GatewayInternalIdentity,
    source_digest_sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DurableExportRequest {
    input: ExportFeatureLayerRequest,
    identity: GatewayInternalIdentity,
    product_id: LayerProductId,
    created_at: chrono::DateTime<Utc>,
    artifact_write_capability: IssuedArtifactWriteCapability,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DurableVectorTileRequest {
    input: BuildVectorTilesRequest,
    identity: GatewayInternalIdentity,
    product_id: LayerProductId,
    created_at: chrono::DateTime<Utc>,
    artifact_write_capability: IssuedArtifactWriteCapability,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DurableRasterDerivationRequest {
    input: DeriveRasterRequest,
    identity: GatewayInternalIdentity,
    source_release_id: crate::contract::DatasetReleaseId,
    source_digest_sha256: String,
    source_crs: String,
    source_transform: [f64; 6],
    derivation_id: RasterDerivationId,
    created_at: chrono::DateTime<Utc>,
    artifact_write_capability: IssuedArtifactWriteCapability,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DurableTravelModelRequest {
    input: BuildTravelModelRequest,
    identity: GatewayInternalIdentity,
    travel_model_id: TravelModelId,
    created_at: chrono::DateTime<Utc>,
    artifact_write_capability: IssuedArtifactWriteCapability,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "request", rename_all = "snake_case")]
enum MapTaskRequest {
    Route(RouteRequest),
    RouteMatrix(RouteMatrixRequest),
    BuildTravelModel(DurableTravelModelRequest),
    ReachableArea(ReachableAreaRequest),
    InspectGeoPackage(feature_transfers::DurableGeoPackageInspectionRequest),
    ImportFeatureLayer(DurableImportRequest),
    ExportFeatureLayer(DurableExportRequest),
    BuildVectorTiles(DurableVectorTileRequest),
    DeriveRaster(DurableRasterDerivationRequest),
}

impl MapTaskExtension {
    pub(crate) fn new(state: Arc<MapApplication>) -> Self {
        Self { state }
    }
}

impl veoveo_task_runtime::DurableTaskService for MapTaskExtension {
    type Caller = AuthenticatedCaller;

    fn authenticate(
        &self,
        context: &rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<Self::Caller, rmcp::ErrorData> {
        let parts = context
            .extensions
            .get::<axum::http::request::Parts>()
            .ok_or_else(|| rmcp::ErrorData::invalid_request("gateway identity missing", None))?;
        let identity = parts
            .extensions
            .get::<GatewayInternalIdentity>()
            .cloned()
            .ok_or_else(|| rmcp::ErrorData::invalid_request("gateway identity missing", None))?;
        let bearer = parts
            .extensions
            .get::<ForwardedBearer>()
            .map(|bearer| bearer.0.clone())
            .ok_or_else(|| rmcp::ErrorData::invalid_request("forwarded bearer missing", None))?;
        Ok(AuthenticatedCaller {
            caller: self.state.caller(identity.clone(), bearer),
            identity,
        })
    }

    async fn start_tool_task(
        &self,
        caller: &Self::Caller,
        request: rmcp::model::CallToolRequestParams,
    ) -> Result<Option<rmcp::model::CreateTaskResult>, rmcp::ErrorData> {
        let arguments = serde_json::Value::Object(request.arguments.unwrap_or_default());
        let task_id = TaskId::new();
        let args = match request.name.as_ref() {
            ROUTE_TASK => {
                require_scope(&caller.identity, "map:route")?;
                MapTaskRequest::Route(
                    serde_json::from_value(arguments).map_err(|error| {
                        rmcp::ErrorData::invalid_params(error.to_string(), None)
                    })?,
                )
            }
            ROUTE_MATRIX_TASK => {
                require_scope(&caller.identity, "map:route_matrix")?;
                MapTaskRequest::RouteMatrix(
                    serde_json::from_value(arguments).map_err(|error| {
                        rmcp::ErrorData::invalid_params(error.to_string(), None)
                    })?,
                )
            }
            BUILD_TRAVEL_MODEL_TASK => {
                require_scope(&caller.identity, "map:route_matrix")?;
                let input: BuildTravelModelRequest = serde_json::from_value(arguments)
                    .map_err(|error| rmcp::ErrorData::invalid_params(error.to_string(), None))?;
                input
                    .validate()
                    .map_err(|error| rmcp::ErrorData::invalid_params(error.to_string(), None))?;
                MapTaskRequest::BuildTravelModel(DurableTravelModelRequest {
                    input,
                    identity: caller.identity.clone(),
                    travel_model_id: TravelModelId::new(),
                    created_at: Utc::now(),
                    artifact_write_capability: issue_output_capability(
                        self.state.as_ref(),
                        &caller.caller,
                        &task_id,
                    )
                    .await
                    .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None))?,
                })
            }
            REACHABLE_AREA_TASK => {
                require_scope(&caller.identity, "map:route")?;
                MapTaskRequest::ReachableArea(
                    serde_json::from_value(arguments).map_err(|error| {
                        rmcp::ErrorData::invalid_params(error.to_string(), None)
                    })?,
                )
            }
            INSPECT_GEOPACKAGE_TASK => {
                require_scope(&caller.identity, "map:feature:read")?;
                let input: InspectGeoPackageRequest = serde_json::from_value(arguments)
                    .map_err(|error| rmcp::ErrorData::invalid_params(error.to_string(), None))?;
                MapTaskRequest::InspectGeoPackage(
                    feature_transfers::prepare_inspection(
                        self.state.as_ref(),
                        caller,
                        &task_id,
                        input,
                    )
                    .await
                    .map_err(|error| rmcp::ErrorData::invalid_params(error.to_string(), None))?,
                )
            }
            IMPORT_FEATURE_LAYER_TASK => {
                require_scope(&caller.identity, "map:feature:write")?;
                let input: ImportFeatureLayerRequest = serde_json::from_value(arguments)
                    .map_err(|error| rmcp::ErrorData::invalid_params(error.to_string(), None))?;
                MapTaskRequest::ImportFeatureLayer(
                    prepare_import_request(self.state.as_ref(), caller, &task_id, input)
                        .await
                        .map_err(|error| {
                            rmcp::ErrorData::invalid_params(error.to_string(), None)
                        })?,
                )
            }
            EXPORT_FEATURE_LAYER_TASK => {
                require_scope(&caller.identity, "map:feature:publish")?;
                let input: ExportFeatureLayerRequest = serde_json::from_value(arguments)
                    .map_err(|error| rmcp::ErrorData::invalid_params(error.to_string(), None))?;
                validate_publication_request(self.state.as_ref(), &caller.identity, &input)
                    .await
                    .map_err(|error| rmcp::ErrorData::invalid_params(error.to_string(), None))?;
                MapTaskRequest::ExportFeatureLayer(DurableExportRequest {
                    input,
                    identity: caller.identity.clone(),
                    product_id: LayerProductId::new(),
                    created_at: Utc::now(),
                    artifact_write_capability: issue_output_capability(
                        self.state.as_ref(),
                        &caller.caller,
                        &task_id,
                    )
                    .await
                    .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None))?,
                })
            }
            BUILD_VECTOR_TILES_TASK => {
                require_scope(&caller.identity, "map:feature:publish")?;
                let input: BuildVectorTilesRequest = serde_json::from_value(arguments)
                    .map_err(|error| rmcp::ErrorData::invalid_params(error.to_string(), None))?;
                validate_tile_request(&input)
                    .map_err(|error| rmcp::ErrorData::invalid_params(error.to_string(), None))?;
                validate_publication_request(
                    self.state.as_ref(),
                    &caller.identity,
                    &ExportFeatureLayerRequest {
                        layer_id: input.layer_id.clone(),
                        publication_id: input.publication_id.clone(),
                        format: crate::contract::FeatureExportFormat::GeoJsonSeq,
                    },
                )
                .await
                .map_err(|error| rmcp::ErrorData::invalid_params(error.to_string(), None))?;
                MapTaskRequest::BuildVectorTiles(DurableVectorTileRequest {
                    input,
                    identity: caller.identity.clone(),
                    product_id: LayerProductId::new(),
                    created_at: Utc::now(),
                    artifact_write_capability: issue_output_capability(
                        self.state.as_ref(),
                        &caller.caller,
                        &task_id,
                    )
                    .await
                    .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None))?,
                })
            }
            DERIVE_RASTER_TASK => {
                require_scope(&caller.identity, "map:dataset:read")?;
                require_scope(&caller.identity, "map:raster:derive")?;
                let input: DeriveRasterRequest = serde_json::from_value(arguments)
                    .map_err(|error| rmcp::ErrorData::invalid_params(error.to_string(), None))?;
                MapTaskRequest::DeriveRaster(
                    prepare_raster_request(self.state.as_ref(), caller, &task_id, input)
                        .await
                        .map_err(|error| {
                            rmcp::ErrorData::invalid_params(error.to_string(), None)
                        })?,
                )
            }
            _ => return Ok(None),
        };
        let retention_pins = veoveo_task_runtime::retention_pins(request.meta.as_ref())?;
        let uses_task_directory = args.uses_task_directory();
        let task_key = task_id.to_string();
        let snapshot = start_map_task(
            self.state.clone(),
            task_id,
            caller.identity.clone(),
            args,
            retention_pins,
        )
        .await;
        let snapshot = match snapshot {
            Ok(snapshot) => snapshot,
            Err(error) => {
                if uses_task_directory {
                    cleanup_task_directory(self.state.as_ref(), &task_key).await;
                }
                return Err(rmcp::ErrorData::internal_error(error.to_string(), None));
            }
        };
        Ok(Some(rmcp::model::CreateTaskResult::new(
            veoveo_task_runtime::task_seed(&snapshot),
        )))
    }

    async fn get_task(
        &self,
        caller: &Self::Caller,
        request: rmcp::model::GetTaskParams,
    ) -> Result<rmcp::model::GetTaskResult, rmcp::ErrorData> {
        veoveo_task_runtime::get_durable_task(
            &self.state.tasks,
            &runtime_owner(&caller.identity),
            request,
        )
        .await
    }

    async fn update_task(
        &self,
        caller: &Self::Caller,
        request: rmcp::model::UpdateTaskParams,
    ) -> Result<(), rmcp::ErrorData> {
        veoveo_task_runtime::update_durable_task(
            &self.state.tasks,
            &runtime_owner(&caller.identity),
            request,
        )
        .await
    }

    async fn cancel_task(
        &self,
        caller: &Self::Caller,
        task_id: String,
    ) -> Result<(), rmcp::ErrorData> {
        veoveo_task_runtime::cancel_durable_task(
            &self.state.tasks,
            &runtime_owner(&caller.identity),
            task_id,
        )
        .await
    }

    async fn subscribe_tasks(
        &self,
        caller: &Self::Caller,
        task_ids: Vec<String>,
    ) -> Result<veoveo_task_runtime::DurableTaskSubscription, rmcp::ErrorData> {
        veoveo_task_runtime::subscribe_durable_tasks(
            &self.state.tasks,
            runtime_owner(&caller.identity),
            task_ids,
        )
        .await
    }
}

pub(crate) async fn visible_travel_models(
    state: &MapApplication,
    identity: &GatewayInternalIdentity,
) -> anyhow::Result<Vec<TravelModelRecord>> {
    let caller_owner = runtime_owner(identity);
    let mut records: Vec<TravelModelRecord> = Vec::new();
    for snapshot in state.tasks.list().await? {
        if snapshot.task_type != BUILD_TRAVEL_MODEL_TASK
            || !snapshot.owner.allows(
                &caller_owner.principal_key,
                &caller_owner.profile,
                caller_owner.tenant_key.as_deref(),
                &caller_owner.data_labels,
            )
            || snapshot.owner.authority.work_context != caller_owner.authority.work_context
        {
            continue;
        }
        let Some(structured) = snapshot.result.as_ref().and_then(|result| {
            result
                .get("structuredContent")
                .or_else(|| result.get("structured_content"))
        }) else {
            continue;
        };
        if let Ok(record) = serde_json::from_value(structured.clone()) {
            records.push(record);
        }
    }
    records.sort_by_key(|record| record.created_at);
    Ok(records)
}

pub(super) async fn recover_tasks(
    state: Arc<MapApplication>,
    resumable: Vec<TaskSnapshot>,
) -> anyhow::Result<()> {
    for snapshot in resumable {
        if !matches!(
            snapshot.task_type.as_str(),
            ROUTE_TASK
                | ROUTE_MATRIX_TASK
                | BUILD_TRAVEL_MODEL_TASK
                | REACHABLE_AREA_TASK
                | INSPECT_GEOPACKAGE_TASK
                | IMPORT_FEATURE_LAYER_TASK
                | EXPORT_FEATURE_LAYER_TASK
                | BUILD_VECTOR_TILES_TASK
                | DERIVE_RASTER_TASK
        ) {
            anyhow::bail!("unknown resumable Map task type `{}`", snapshot.task_type);
        }
        let request: MapTaskRequest = serde_json::from_value(snapshot.request.clone())?;
        if request.task_type() != snapshot.task_type {
            anyhow::bail!("Map task type does not match its persisted request");
        }
        if let Err(error) = schedule_map_task(state.clone(), snapshot, request).await {
            match error.downcast_ref::<TaskError>() {
                Some(TaskError::LeaseHeld(task_id) | TaskError::Conflict(task_id)) => {
                    tracing::info!(task_id, "another replica claimed recovered Map task");
                }
                _ => return Err(error),
            }
        }
    }
    Ok(())
}

async fn start_map_task(
    state: Arc<MapApplication>,
    task_id: TaskId,
    identity: GatewayInternalIdentity,
    request: MapTaskRequest,
    retention_pins: BTreeSet<TaskRetentionPin>,
) -> anyhow::Result<TaskSnapshot> {
    let task_type = request.task_type().to_owned();
    let created = state
        .tasks
        .create(CreateTask {
            task_id,
            owner: runtime_owner(&identity),
            server: SERVER_SLUG.to_owned(),
            task_type,
            request: serde_json::to_value(&request)?,
            recovery_class: RecoveryClass::Resume,
            idempotency_key: None,
            ttl_ms: Some(TASK_TTL_MS),
            poll_interval_ms: Some(TASK_POLL_INTERVAL_MS),
            retention_pins,
        })
        .await?;
    schedule_map_task(state, created.snapshot, request).await
}

async fn schedule_map_task(
    state: Arc<MapApplication>,
    snapshot: TaskSnapshot,
    request: MapTaskRequest,
) -> anyhow::Result<TaskSnapshot> {
    let task_id = snapshot.task_id.to_string();
    let claimed = state.tasks.claim(&task_id, TASK_LEASE_DURATION).await?;
    let owner = snapshot.owner.clone();
    let cancellation = CancellationToken::new();
    let join = tokio::spawn(run_map_task(
        state.clone(),
        task_id.clone(),
        owner,
        request,
        cancellation.clone(),
    ));
    state
        .tasks
        .register_worker(&task_id, cancellation, join)
        .await?;
    Ok(claimed.snapshot)
}

async fn run_map_task(
    state: Arc<MapApplication>,
    task_id: String,
    owner: TaskOwner,
    request: MapTaskRequest,
    cancellation: CancellationToken,
) {
    let work = run_map_task_inner(
        state.clone(),
        task_id.clone(),
        owner,
        request,
        cancellation.clone(),
    );
    tokio::pin!(work);
    let mut heartbeat = tokio::time::interval(TASK_LEASE_HEARTBEAT);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    heartbeat.tick().await;
    loop {
        tokio::select! {
            () = &mut work => break,
            _ = heartbeat.tick() => {
                if let Err(error) = state.tasks.renew_lease(&task_id, TASK_LEASE_DURATION).await {
                    tracing::warn!(task_id, "Map task lease heartbeat failed: {error}");
                    cancellation.cancel();
                    break;
                }
            }
        }
    }
}

async fn run_map_task_inner(
    state: Arc<MapApplication>,
    task_id: String,
    owner: TaskOwner,
    request: MapTaskRequest,
    cancellation: CancellationToken,
) {
    let uses_task_directory = request.uses_task_directory();
    let publishes_travel_model = matches!(&request, MapTaskRequest::BuildTravelModel(_));
    update_task(
        &state,
        &task_id,
        TaskTransition::Running {
            message: format!("calculating {}", request.description()),
            progress: 0.05,
        },
    )
    .await;
    if cancellation.is_cancelled() {
        update_task(&state, &task_id, TaskTransition::Cancelled).await;
        if uses_task_directory {
            cleanup_task_directory(state.as_ref(), &task_id).await;
        }
        return;
    }
    let result = match request {
        MapTaskRequest::Route(request) => {
            match state.scope_from_task_owner(&owner).await {
                Ok(scope) => state.routes.route(&scope, request).await.and_then(|route| {
                    tool_result(format!("planned route {}", route.route_id), &route)
                }),
                Err(error) => Err(error),
            }
        }
        MapTaskRequest::RouteMatrix(request) => match state.scope_from_task_owner(&owner).await {
            Ok(scope) => state
                .routes
                .route_matrix(&scope, request)
                .await
                .and_then(|matrix| {
                    tool_result(format!("calculated matrix {}", matrix.matrix_id), &matrix)
                }),
            Err(error) => Err(error),
        },
        MapTaskRequest::BuildTravelModel(request) => {
            run_travel_model_task(state.as_ref(), &task_id, request).await
        }
        MapTaskRequest::ReachableArea(request) => match state.scope_from_task_owner(&owner).await {
            Ok(scope) => state
                .routes
                .reachable_area(&scope, request)
                .await
                .and_then(|area| {
                    tool_result(
                        format!("calculated reachable area {}", area.reachable_area_id),
                        &area,
                    )
                }),
            Err(error) => Err(error),
        },
        MapTaskRequest::InspectGeoPackage(request) => {
            feature_transfers::run_inspection(
                state.as_ref(),
                &task_id,
                request,
                cancellation.clone(),
            )
            .await
        }
        MapTaskRequest::ImportFeatureLayer(request) => {
            run_import_task(state.as_ref(), &task_id, request, cancellation.clone()).await
        }
        MapTaskRequest::ExportFeatureLayer(request) => {
            run_export_task(state.as_ref(), &task_id, request, cancellation.clone()).await
        }
        MapTaskRequest::BuildVectorTiles(request) => {
            run_vector_tile_task(state.as_ref(), &task_id, request).await
        }
        MapTaskRequest::DeriveRaster(request) => {
            run_raster_derivation_task(state.as_ref(), &task_id, request, cancellation.clone())
                .await
        }
    };
    if cancellation.is_cancelled() {
        update_task(&state, &task_id, TaskTransition::Cancelled).await;
        if uses_task_directory {
            cleanup_task_directory(state.as_ref(), &task_id).await;
        }
        return;
    }
    match result {
        Ok(tool_result) => match serde_json::to_value(tool_result) {
            Ok(result) => {
                update_task(
                    &state,
                    &task_id,
                    TaskTransition::Succeeded {
                        message: "Map calculation completed".to_owned(),
                        result,
                    },
                )
                .await;
                if publishes_travel_model {
                    state
                        .subscriptions
                        .notify_resource_updated(crate::uris::TRAVEL_MODELS_URI)
                        .await;
                    state.resource_observers.notify_changed().await;
                }
            }
            Err(error) => fail_task(&state, &task_id, "result_serialization_failed", error).await,
        },
        Err(error) => fail_task(&state, &task_id, "map_calculation_failed", error).await,
    }
    if uses_task_directory {
        cleanup_task_directory(state.as_ref(), &task_id).await;
    }
}

impl MapTaskRequest {
    fn task_type(&self) -> &'static str {
        match self {
            Self::Route(_) => ROUTE_TASK,
            Self::RouteMatrix(_) => ROUTE_MATRIX_TASK,
            Self::BuildTravelModel(_) => BUILD_TRAVEL_MODEL_TASK,
            Self::ReachableArea(_) => REACHABLE_AREA_TASK,
            Self::InspectGeoPackage(_) => INSPECT_GEOPACKAGE_TASK,
            Self::ImportFeatureLayer(_) => IMPORT_FEATURE_LAYER_TASK,
            Self::ExportFeatureLayer(_) => EXPORT_FEATURE_LAYER_TASK,
            Self::BuildVectorTiles(_) => BUILD_VECTOR_TILES_TASK,
            Self::DeriveRaster(_) => DERIVE_RASTER_TASK,
        }
    }

    fn description(&self) -> &'static str {
        match self {
            Self::Route(_) => "logistics route",
            Self::RouteMatrix(_) => "logistics route matrix",
            Self::BuildTravelModel(_) => "cuOpt travel model",
            Self::ReachableArea(_) => "land reachable area",
            Self::InspectGeoPackage(_) => "GeoPackage inspection",
            Self::ImportFeatureLayer(_) => "authored feature import",
            Self::ExportFeatureLayer(_) => "authored feature export",
            Self::BuildVectorTiles(_) => "published feature vector tiles",
            Self::DeriveRaster(_) => "governed raster derivation",
        }
    }

    fn uses_task_directory(&self) -> bool {
        matches!(
            self,
            Self::InspectGeoPackage(_)
                | Self::ImportFeatureLayer(_)
                | Self::ExportFeatureLayer(_)
                | Self::BuildVectorTiles(_)
                | Self::DeriveRaster(_)
        )
    }
}

fn tool_result<T: Serialize>(text: String, value: &T) -> anyhow::Result<CallToolResult> {
    let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
    result.structured_content = Some(serde_json::to_value(value)?);
    Ok(result)
}

pub(super) fn tool_result_with_links<T, I, U, L>(
    text: String,
    value: &T,
    links: I,
) -> anyhow::Result<CallToolResult>
where
    T: Serialize,
    I: IntoIterator<Item = (U, L)>,
    U: Into<String>,
    L: Into<String>,
{
    let mut result = tool_result(text, value)?;
    result.content.extend(links.into_iter().map(|(uri, title)| {
        let title = title.into();
        ContentBlock::resource_link(
            Resource::new(uri.into(), title.clone())
                .with_title(title)
                .with_mime_type("application/json"),
        )
    }));
    Ok(result)
}

async fn prepare_raster_request(
    state: &MapApplication,
    caller: &AuthenticatedCaller,
    task_id: &TaskId,
    input: DeriveRasterRequest,
) -> anyhow::Result<DurableRasterDerivationRequest> {
    input.validate()?;
    let scope = state.scope(&caller.identity).await?;
    let raster = state
        .analytics
        .raster_product(&scope.tenant_key(), &input.raster_id)?
        .context("unknown raster product")?;
    if let Some(band) = raster_operation_band(&input.operation)
        && !raster.bands.iter().any(|candidate| candidate.index == band)
    {
        bail!("raster operation selects an unavailable band");
    }
    if raster_operation_reads_full_source(&input.operation)
        && u64::from(raster.width) * u64::from(raster.height) > MAX_RASTER_FULL_DERIVATION_PIXELS
    {
        bail!(
            "full-raster derivations are limited to {MAX_RASTER_FULL_DERIVATION_PIXELS} source pixels"
        );
    }
    let artifact_id = raster
        .artifact_uri
        .strip_prefix("artifact://")
        .context("raster source is not an Artifact-plane URI")
        .and_then(|value| ArtifactId::parse(value).map_err(anyhow::Error::from))?;
    let artifact = state
        .artifacts
        .get(&caller.caller, &artifact_id)
        .await?
        .context("raster source artifact is unavailable or unauthorized")?;
    if artifact.metadata.byte_len > state.max_artifact_bytes
        || artifact.bytes.len() as u64 != artifact.metadata.byte_len
        || hex::encode(Sha256::digest(&artifact.bytes)) != raster.checksum_sha256
    {
        bail!("raster source artifact failed its bound size or digest check");
    }
    let directory = task_directory(state, &task_id.to_string())?;
    tokio::fs::create_dir(&directory)
        .await
        .with_context(|| format!("creating raster task directory {}", directory.display()))?;
    let staging = async {
        let mut file = tokio::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(directory.join("source.tif"))
            .await?;
        file.write_all(&artifact.bytes).await?;
        file.sync_all().await
    }
    .await;
    if let Err(error) = staging {
        cleanup_task_directory(state, &task_id.to_string()).await;
        return Err(error).context("staging authorized raster artifact");
    }
    let artifact_write_capability =
        match issue_output_capability(state, &caller.caller, task_id).await {
            Ok(capability) => capability,
            Err(error) => {
                cleanup_task_directory(state, &task_id.to_string()).await;
                return Err(error).context("issuing raster output capability");
            }
        };
    Ok(DurableRasterDerivationRequest {
        input,
        identity: caller.identity.clone(),
        source_release_id: raster.release_id,
        source_digest_sha256: raster.checksum_sha256,
        source_crs: raster.crs,
        source_transform: raster.transform,
        derivation_id: RasterDerivationId::new(),
        created_at: Utc::now(),
        artifact_write_capability,
    })
}

fn raster_operation_band(operation: &RasterDerivationOperation) -> Option<u32> {
    match operation {
        RasterDerivationOperation::Sample { band, .. }
        | RasterDerivationOperation::ClassMask { band, .. }
        | RasterDerivationOperation::Contour { band, .. }
        | RasterDerivationOperation::Polygonize { band }
        | RasterDerivationOperation::Skeletonize { band, .. }
        | RasterDerivationOperation::DeriveLines { band, .. }
        | RasterDerivationOperation::CorridorMaximum { band, .. } => Some(*band),
        RasterDerivationOperation::Window { .. } => None,
    }
}

fn raster_operation_reads_full_source(operation: &RasterDerivationOperation) -> bool {
    !matches!(
        operation,
        RasterDerivationOperation::Sample { .. }
            | RasterDerivationOperation::Window { .. }
            | RasterDerivationOperation::CorridorMaximum { .. }
    )
}

async fn run_raster_derivation_task(
    state: &MapApplication,
    task_id: &str,
    request: DurableRasterDerivationRequest,
    cancellation: CancellationToken,
) -> anyhow::Result<CallToolResult> {
    request.input.validate()?;
    let directory = task_directory(state, task_id)?;
    let source_path = directory.join("source.tif");
    let bytes = tokio::fs::read(&source_path).await?;
    if bytes.len() as u64 > state.max_artifact_bytes
        || hex::encode(Sha256::digest(&bytes)) != request.source_digest_sha256
    {
        bail!("staged raster failed its bound size or digest check");
    }
    let output_dir = directory.join("output");
    let generated = state
        .raster
        .derive(
            &source_path,
            &output_dir,
            &request.input.operation,
            cancellation,
        )
        .await?;
    let output = tokio::fs::read(&generated.path).await?;
    let output_digest_sha256 = hex::encode(Sha256::digest(&output));
    let mut artifact = ArtifactPut::new(output);
    artifact.mime_type = Some(generated.mime_type.clone());
    artifact.filename = Some(generated.filename);
    artifact.compliance = artifact_compliance(&request.identity);
    artifact.metadata = serde_json::json!({
        "domain": "map_raster",
        "schema_version": RASTER_DERIVATION_SCHEMA_VERSION,
        "derivation_id": request.derivation_id,
        "source_raster_id": request.input.raster_id,
        "source_release_id": request.source_release_id,
        "operation": request.input.operation,
        "algorithm_revision": request.input.algorithm_revision,
        "source_checksum_sha256": request.source_digest_sha256,
        "source_crs": request.source_crs,
        "source_transform": request.source_transform,
        "output_crs": generated.output_crs,
        "output_transform": generated.output_transform,
        "output_mime_type": generated.mime_type,
        "output_checksum_sha256": output_digest_sha256,
    });
    let metadata = state
        .artifacts
        .put_with_capability(
            &request.artifact_write_capability,
            ArtifactWriteIdempotencyKey::new(format!("map:{task_id}:raster-derivation"))?,
            artifact,
        )
        .await?;
    let scope = state.scope(&request.identity).await?;
    let derivation = RasterDerivation {
        schema_version: RASTER_DERIVATION_SCHEMA_VERSION,
        derivation_id: request.derivation_id,
        source_raster_id: request.input.raster_id,
        source_release_id: request.source_release_id,
        source_checksum_sha256: request.source_digest_sha256,
        source_crs: request.source_crs,
        source_transform: request.source_transform,
        operation: request.input.operation,
        algorithm_revision: request.input.algorithm_revision,
        output_artifact_uri: metadata.artifact_uri,
        output_mime_type: generated.mime_type,
        output_crs: generated.output_crs,
        output_transform: generated.output_transform,
        output_checksum_sha256: output_digest_sha256,
        created_by: request.identity.actor.id,
        work_context: request.identity.authority.work_context,
        created_at: request.created_at,
    };
    state
        .analytics
        .put_raster_derivation(&scope.tenant_key(), &derivation)?;
    state
        .subscriptions
        .notify_resource_updated(crate::uris::RASTER_DERIVATIONS_URI)
        .await;
    let mut result = tool_result_with_links(
        format!("created raster derivation {}", derivation.derivation_id),
        &derivation,
        [(
            crate::uris::raster_derivation_uri(derivation.derivation_id.as_str()),
            "Raster derivation",
        )],
    )?;
    result.content.push(ContentBlock::resource_link(
        Resource::new(
            derivation.output_artifact_uri.clone(),
            "Derived raster artifact",
        )
        .with_title("Derived raster artifact")
        .with_mime_type(derivation.output_mime_type.clone()),
    ));
    Ok(result)
}

async fn prepare_import_request(
    state: &MapApplication,
    caller: &AuthenticatedCaller,
    task_id: &TaskId,
    input: ImportFeatureLayerRequest,
) -> anyhow::Result<DurableImportRequest> {
    let scope = state.scope(&caller.identity).await?;
    let layer = state
        .authoring
        .layer(&caller.identity, &scope, &input.layer_id)
        .await?
        .context("unknown feature layer")?;
    if layer.archived_at.is_some() || layer.revision != input.expected_layer_revision {
        bail!("feature layer is archived or its revision does not match the import request");
    }
    let artifact = state
        .artifacts
        .get(&caller.caller, &input.source_artifact_id)
        .await?
        .context("unknown or unauthorized source artifact")?;
    if artifact.metadata.byte_len > state.max_artifact_bytes
        || artifact.bytes.len() as u64 > state.max_artifact_bytes
        || artifact.metadata.byte_len != artifact.bytes.len() as u64
    {
        bail!("source artifact exceeds the configured byte limit or has inconsistent metadata");
    }
    let directory = task_directory(state, &task_id.to_string())?;
    tokio::fs::create_dir(&directory)
        .await
        .with_context(|| format!("creating import task directory {}", directory.display()))?;
    let staged_filename = feature_transfers::staged_import_filename(&input.source);
    let staging = async {
        let mut file = tokio::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(directory.join(staged_filename))
            .await?;
        file.write_all(&artifact.bytes).await?;
        file.sync_all().await
    }
    .await;
    if let Err(error) = staging {
        cleanup_task_directory(state, &task_id.to_string()).await;
        return Err(error).context("staging authorized import artifact");
    }
    Ok(DurableImportRequest {
        input,
        identity: caller.identity.clone(),
        source_digest_sha256: hex::encode(Sha256::digest(&artifact.bytes)),
    })
}

async fn validate_publication_request(
    state: &MapApplication,
    identity: &GatewayInternalIdentity,
    input: &ExportFeatureLayerRequest,
) -> anyhow::Result<()> {
    let scope = state.scope(identity).await?;
    state
        .authoring
        .product_publication(identity, &scope, &input.layer_id, &input.publication_id)
        .await?;
    Ok(())
}

fn validate_tile_request(input: &BuildVectorTilesRequest) -> anyhow::Result<()> {
    if input.tiles.is_empty() || input.tiles.len() > crate::contract::MAX_VECTOR_TILES {
        bail!(
            "a vector tile task must contain between one and {} tiles",
            crate::contract::MAX_VECTOR_TILES
        );
    }
    let mut previous = None;
    for tile in &input.tiles {
        tile.validate().map_err(anyhow::Error::msg)?;
        if previous.is_some_and(|value| value >= *tile) {
            bail!("vector tile coordinates must be unique and sorted by z, x, then y");
        }
        previous = Some(*tile);
    }
    Ok(())
}

async fn issue_output_capability(
    state: &MapApplication,
    caller: &PlaneCaller,
    task_id: &TaskId,
) -> anyhow::Result<IssuedArtifactWriteCapability> {
    state
        .artifacts
        .issue_write_capability(
            caller,
            &IssueArtifactWriteCapabilityRequest {
                task_id: task_id.to_string(),
                expires_at: Utc::now() + ARTIFACT_CAPABILITY_TTL,
                max_artifact_count: NonZeroU32::new(1).expect("one is non-zero"),
                max_total_bytes: NonZeroU64::new(state.max_artifact_bytes)
                    .context("maximum artifact bytes must be non-zero")?,
            },
        )
        .await
}

async fn run_travel_model_task(
    state: &MapApplication,
    task_id: &str,
    request: DurableTravelModelRequest,
) -> anyhow::Result<CallToolResult> {
    let scope = state.scope(&request.identity).await?;
    let travel_model_uri = crate::uris::travel_model_uri(request.travel_model_id.as_str());
    let cost_metric = request.input.cost_metric;
    let time_model = request.input.time_model;
    let (package, profiles) = state
        .routes
        .build_travel_model(&scope, request.input, travel_model_uri.clone())
        .await?;
    let unavailable_cell_count = package
        .model
        .cost_matrices
        .iter()
        .map(|matrix| matrix.unavailable_cells.len() as u64)
        .sum();
    let location_count =
        u32::try_from(package.model.location_ids.len()).context("too many travel locations")?;
    let vehicle_type_count = u32::try_from(package.model.cost_matrices.len())
        .context("too many travel vehicle types")?;
    let bytes = serde_json::to_vec(&package)?;
    if bytes.len() as u64 > state.max_artifact_bytes {
        bail!(
            "travel-model package is {} bytes and exceeds the {}-byte limit",
            bytes.len(),
            state.max_artifact_bytes
        );
    }
    let mut artifact = ArtifactPut::new(bytes);
    artifact.mime_type = Some("application/json".to_owned());
    artifact.filename = Some(format!("{}.travel-model.json", request.travel_model_id));
    artifact.compliance = artifact_compliance(&request.identity);
    artifact.metadata = serde_json::json!({
        "domain": "map_travel_model",
        "travel_model_id": request.travel_model_id,
        "version": crate::contract::TRAVEL_MODEL_ARTIFACT_VERSION,
        "location_count": location_count,
        "vehicle_type_count": vehicle_type_count,
        "unavailable_cell_count": unavailable_cell_count,
    });
    let artifact = state
        .artifacts
        .put_with_capability(
            &request.artifact_write_capability,
            ArtifactWriteIdempotencyKey::new(format!("map:{task_id}:travel-model"))?,
            artifact,
        )
        .await?
        .without_download_url();
    let record = TravelModelRecord {
        travel_model_id: request.travel_model_id,
        travel_model_uri: travel_model_uri.clone(),
        manifest_uri: artifact.artifact_id.plane_uri(),
        artifact: artifact.clone(),
        cost_metric,
        time_model,
        location_count,
        vehicle_type_count,
        unavailable_cell_count,
        profiles,
        created_by: request.identity.actor.id.clone(),
        work_context: request.identity.authority.work_context.clone(),
        created_at: request.created_at,
    };
    tool_result_with_links(
        format!(
            "built travel model {} with {} locations and {} vehicle types",
            record.travel_model_id, record.location_count, record.vehicle_type_count
        ),
        &record,
        [
            (travel_model_uri, "cuOpt travel model"),
            (artifact.artifact_uri, "travel-model artifact"),
        ],
    )
}

async fn run_import_task(
    state: &MapApplication,
    task_id: &str,
    mut request: DurableImportRequest,
    cancellation: CancellationToken,
) -> anyhow::Result<CallToolResult> {
    let directory = task_directory(state, task_id)?;
    let staged_filename = feature_transfers::staged_import_filename(&request.input.source);
    let bytes = tokio::fs::read(directory.join(staged_filename))
        .await
        .context("reading staged import artifact")?;
    if bytes.len() as u64 > state.max_artifact_bytes
        || hex::encode(Sha256::digest(&bytes)) != request.source_digest_sha256
    {
        bail!("staged import artifact failed its bound size or digest check");
    }
    let bytes = feature_transfers::transcode_import(
        state,
        task_id,
        &mut request.input,
        bytes,
        cancellation,
    )
    .await?;
    let scope = state.scope(&request.identity).await?;
    let output = state
        .authoring
        .import_features(&request.identity, &scope, request.input, &bytes)
        .await?;
    state
        .subscriptions
        .notify_resource_updated(crate::uris::FEATURE_LAYERS_URI)
        .await;
    state
        .subscriptions
        .notify_resource_updated(&crate::uris::feature_layer_uri(
            output.changeset.layer_id.as_str(),
        ))
        .await;
    state
        .subscriptions
        .notify_resource_updated(&crate::uris::features_uri(
            output.changeset.layer_id.as_str(),
        ))
        .await;
    tool_result_with_links(
        format!(
            "imported {} authored features",
            output.imported_feature_count
        ),
        &output,
        [
            (
                crate::uris::feature_layer_uri(output.changeset.layer_id.as_str()),
                "Authored feature layer",
            ),
            (
                crate::uris::changeset_uri(
                    output.changeset.layer_id.as_str(),
                    output.changeset.changeset_id.as_str(),
                ),
                "Feature changeset",
            ),
        ],
    )
}

async fn run_export_task(
    state: &MapApplication,
    task_id: &str,
    request: DurableExportRequest,
    cancellation: CancellationToken,
) -> anyhow::Result<CallToolResult> {
    let directory = task_directory(state, task_id)?;
    tokio::fs::create_dir_all(&directory).await?;
    let scope = state.scope(&request.identity).await?;
    let generated = feature_transfers::generate_export(
        state,
        &request.identity,
        &scope,
        &request.input,
        &directory,
        cancellation,
    )
    .await?;
    let product = publish_generated_product(
        state,
        task_id,
        &request.identity,
        &scope,
        &request.input.layer_id,
        &request.input.publication_id,
        request.product_id,
        request.created_at,
        &request.artifact_write_capability,
        generated,
    )
    .await?;
    let product_uri = crate::uris::layer_product_uri(
        product.layer_id.as_str(),
        product.publication_id.as_str(),
        product.product_id.as_str(),
    );
    let output = ExportFeatureLayerOutput { product };
    tool_result_with_links(
        "exported published feature layer".to_owned(),
        &output,
        [(product_uri, "Feature layer product")],
    )
}

async fn run_vector_tile_task(
    state: &MapApplication,
    task_id: &str,
    request: DurableVectorTileRequest,
) -> anyhow::Result<CallToolResult> {
    let directory = task_directory(state, task_id)?;
    tokio::fs::create_dir_all(&directory).await?;
    let scope = state.scope(&request.identity).await?;
    let generated = state
        .authoring
        .generate_vector_tiles(
            &request.identity,
            &scope,
            &request.input,
            &directory,
            state.max_artifact_bytes,
        )
        .await?;
    let tile_count = generated.tile_count.context("vector tile count missing")?;
    let product = publish_generated_product(
        state,
        task_id,
        &request.identity,
        &scope,
        &request.input.layer_id,
        &request.input.publication_id,
        request.product_id,
        request.created_at,
        &request.artifact_write_capability,
        generated,
    )
    .await?;
    let product_uri = crate::uris::layer_product_uri(
        product.layer_id.as_str(),
        product.publication_id.as_str(),
        product.product_id.as_str(),
    );
    let output = BuildVectorTilesOutput {
        product,
        tile_count,
    };
    tool_result_with_links(
        format!("built {tile_count} Mapbox Vector Tiles"),
        &output,
        [(product_uri, "Vector tile layer product")],
    )
}

#[allow(clippy::too_many_arguments)]
async fn publish_generated_product(
    state: &MapApplication,
    task_id: &str,
    identity: &GatewayInternalIdentity,
    scope: &crate::catalog::MapScope,
    layer_id: &crate::contract::FeatureLayerId,
    publication_id: &crate::contract::LayerPublicationId,
    product_id: LayerProductId,
    created_at: chrono::DateTime<Utc>,
    capability: &IssuedArtifactWriteCapability,
    generated: crate::authoring::GeneratedLayerProduct,
) -> anyhow::Result<LayerProduct> {
    let publication = state
        .authoring
        .publication(identity, scope, layer_id, publication_id)
        .await?
        .context("unknown layer publication")?;
    let mut artifact = ArtifactPut::new(generated.bytes);
    artifact.mime_type = Some(generated.mime_type.to_owned());
    artifact.filename = Some(generated.filename);
    artifact.compliance = artifact_compliance(identity);
    artifact.metadata = serde_json::json!({
        "domain": "map_authoring",
        "layer_id": layer_id,
        "publication_id": publication_id,
        "product_id": product_id,
        "format": generated.format,
        "digest_sha256": generated.digest_sha256,
        "feature_count": generated.feature_count,
    });
    let metadata = state
        .artifacts
        .put_with_capability(
            capability,
            ArtifactWriteIdempotencyKey::new(format!("map:{task_id}:layer-product"))?,
            artifact,
        )
        .await?;
    let product = LayerProduct {
        product_id,
        publication_id: publication_id.clone(),
        layer_id: layer_id.clone(),
        layer_revision: publication.layer_revision,
        format: generated.format,
        artifact_uri: metadata.artifact_uri,
        mime_type: generated.mime_type.to_owned(),
        digest_sha256: generated.digest_sha256,
        size_bytes: metadata.byte_len,
        feature_count: generated.feature_count,
        created_by: identity.actor.id.clone(),
        work_context: identity.authority.work_context.clone(),
        created_at,
    };
    let product = state
        .authoring
        .record_layer_product(identity, scope, &product)
        .await?;
    state
        .subscriptions
        .notify_resource_updated(crate::uris::LAYER_PRODUCTS_URI)
        .await;
    Ok(product)
}

fn artifact_compliance(identity: &GatewayInternalIdentity) -> ComplianceMetadata {
    ComplianceMetadata {
        classification: identity.authority.output_policy.classification.clone(),
        tenant_id: identity.actor.tenant.clone(),
        owner: Some(identity.authority.output_policy.owner.clone()),
        work_context: Some(identity.authority.work_context.clone()),
        provenance: Some(artifact_provenance(identity)),
        data_labels: identity.authority.output_policy.data_labels.clone(),
        retention_expires_at: None,
    }
}

fn artifact_provenance(identity: &GatewayInternalIdentity) -> ArtifactProvenance {
    let (invocation_mode, initiator, delegation_id) = match &identity.authority.provenance {
        InvocationProvenance::Direct { initiator } => {
            (InvocationMode::Direct, Some(initiator.clone()), None)
        }
        InvocationProvenance::Delegated {
            initiator,
            delegation_id,
        } => (
            InvocationMode::Delegated,
            Some(initiator.clone()),
            Some(delegation_id.clone()),
        ),
        InvocationProvenance::Automated => (InvocationMode::Automated, None, None),
    };
    ArtifactProvenance {
        producer: identity.actor.id.clone(),
        invocation_mode,
        initiator,
        delegation_id,
        policy_revision: identity.authority.policy_revision.clone(),
    }
}

pub(super) fn task_directory(
    state: &MapApplication,
    task_id: &str,
) -> anyhow::Result<std::path::PathBuf> {
    let task_id: TaskId = task_id.parse().context("invalid durable task id")?;
    Ok(state.authoring_task_root.join(task_id.to_string()))
}

pub(super) async fn cleanup_task_directory(state: &MapApplication, task_id: &str) {
    let Ok(directory) = task_directory(state, task_id) else {
        return;
    };
    match tokio::fs::remove_dir_all(&directory).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => tracing::warn!(task_id, "failed to clean Map task directory: {error}"),
    }
}

async fn fail_task(
    state: &MapApplication,
    task_id: &str,
    code: &str,
    error: impl std::fmt::Display,
) {
    update_task(
        state,
        task_id,
        TaskTransition::Failed(TaskFailure::new(code, error.to_string())),
    )
    .await;
}

async fn update_task(state: &MapApplication, task_id: &str, transition: TaskTransition) {
    if let Err(error) = state.tasks.transition(task_id, transition).await {
        tracing::warn!(task_id, "Map task update failed: {error}");
    }
}

fn require_scope(
    identity: &GatewayInternalIdentity,
    required: &str,
) -> Result<(), rmcp::ErrorData> {
    identity
        .actor
        .scopes
        .iter()
        .any(|scope| scope.as_str() == required)
        .then_some(())
        .ok_or_else(|| {
            rmcp::ErrorData::invalid_request(format!("required scope `{required}` missing"), None)
        })
}

fn runtime_owner(identity: &GatewayInternalIdentity) -> TaskOwner {
    TaskOwner {
        principal_key: identity.actor.id.to_string(),
        principal_kind: match identity.actor.kind {
            PrincipalKind::User => veoveo_task_runtime::PrincipalKind::User,
            PrincipalKind::Service => veoveo_task_runtime::PrincipalKind::Service,
        },
        issuer: identity.actor.issuer.to_string(),
        subject: identity.actor.subject.to_string(),
        profile: identity.profile.to_string(),
        tenant_key: identity.actor.tenant.as_ref().map(ToString::to_string),
        data_labels: identity
            .actor
            .data_labels
            .iter()
            .map(ToString::to_string)
            .collect(),
        authority: identity.authority.clone(),
    }
}
