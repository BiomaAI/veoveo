use super::{
    auth::{InternalAuthState, artifact_caller, authenticate},
    mcp::RecordingMcp,
    state::AppState,
};
use axum::{
    Extension, Json, Router,
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use re_protos::cloud::v1alpha1::rerun_cloud_service_server::RerunCloudServiceServer;
use rmcp::transport::streamable_http_server::StreamableHttpService;
use serde::Serialize;
use std::{collections::BTreeSet, sync::Arc};
use tokio_util::sync::CancellationToken;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_mcp_contract::GatewayInternalTokenVerifier;
use veoveo_platform_store::{RecordingId, RecordingProjectionReceiptId};
use veoveo_recording_mcp::{
    admin,
    blueprint_playback::recording_scoped_blueprint,
    contract::CreateRecordingCatalogGrantRequest,
    live_stream::{
        FRAMED_RRD_CONTENT_TYPE, LIVE_RRD_START_HEADER, LiveRrdStart, authorized_live_rrd_stream,
    },
    playback::{RECORDING_GRANT_HEADER, playback_application_id, playback_store_id},
    service::PlaybackArchiveSelection,
};

pub(super) fn router(
    state: Arc<AppState>,
    verifier: GatewayInternalTokenVerifier,
    allowed_hosts: BTreeSet<String>,
    cancellation: &CancellationToken,
) -> Router {
    let auth_state = InternalAuthState {
        verifier,
        allowed_hosts: Arc::new(allowed_hosts.iter().cloned().collect()),
    };
    let service = StreamableHttpService::new(
        {
            let state = state.clone();
            move || Ok(RecordingMcp::new(state.clone()))
        },
        veoveo_mcp_contract::stateless_session_manager(),
        veoveo_mcp_contract::canonical_streamable_http_server_config()
            .with_allowed_hosts(allowed_hosts)
            .with_cancellation_token(cancellation.child_token()),
    );
    let mcp = Router::new()
        .route_service("/", service.clone())
        .route_service("/{*path}", service)
        .layer(middleware::from_fn(
            veoveo_mcp_contract::enforce_serialized_mcp_response,
        ))
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            authenticate,
        ));
    let admin_router = admin::router().layer(middleware::from_fn_with_state(
        auth_state.clone(),
        authenticate,
    ));
    let storage_diagnostics_router = Router::new()
        .route("/admin/storage", get(storage_diagnostics))
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            authenticate,
        ));
    let playback = Router::new()
        .route("/catalog-grants", post(catalog_grant))
        .route("/{recording_id}/playback", get(playback_manifest))
        .route(
            "/{recording_id}/live/rrd-stream",
            get(playback_live_recording),
        )
        .route(
            "/{recording_id}/blueprints/{revision}/data.rrd",
            get(playback_blueprint),
        )
        .route(
            "/{recording_id}/projections/{projection_id}/data.arrow",
            get(projection_data),
        )
        .layer(middleware::from_fn_with_state(auth_state, authenticate));
    let redap = tonic::service::Routes::new(RerunCloudServiceServer::new(
        state.playback.scoped_redap_service(),
    ))
    .into_axum_router()
    .layer(tonic_web::GrpcWebLayer::new())
    .with_state::<Arc<AppState>>(());
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(ready))
        .merge(storage_diagnostics_router)
        .nest_service("/admin", admin_router)
        .nest("/mcp", mcp)
        .nest("/recordings", playback)
        .merge(redap)
        .with_state(state)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO)),
        )
}

fn parse_recording_id(
    value: &str,
) -> Result<RecordingId, veoveo_recording_mcp::contract::RecordingContractError> {
    let id = veoveo_recording_mcp::contract::RecordingId::parse(value)?;
    Ok(RecordingId::from_uuid(id.as_uuid()))
}

async fn ready(State(state): State<Arc<AppState>>) -> StatusCode {
    if let Err(error) = state.recordings.platform_store().healthcheck().await {
        tracing::warn!("recording MCP store readiness failed: {error}");
        return StatusCode::SERVICE_UNAVAILABLE;
    }
    if let Err(error) = state.recordings.storage_readiness() {
        tracing::warn!("recording MCP storage readiness failed: {error}");
        return StatusCode::SERVICE_UNAVAILABLE;
    }
    StatusCode::OK
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordingStorageDiagnostics {
    schema: &'static str,
    layer_cache: Option<veoveo_recording_reader::cache::LayerCacheStats>,
    projection_scratch: Option<veoveo_recording_mcp::service::ProjectionRuntimeStats>,
}

async fn storage_diagnostics(State(state): State<Arc<AppState>>) -> Response {
    let diagnostics = state
        .recordings
        .layer_cache_stats()
        .and_then(|layer_cache| {
            Ok(RecordingStorageDiagnostics {
                schema: "veoveo.ai/recording-storage-diagnostics/v1",
                layer_cache,
                projection_scratch: state.recordings.projection_runtime_stats()?,
            })
        });
    match diagnostics {
        Ok(diagnostics) => Json(diagnostics).into_response(),
        Err(error) => {
            tracing::warn!(%error, "recording storage diagnostics failed");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

fn requested_read_grant(
    headers: &HeaderMap,
) -> Result<Option<veoveo_recording_mcp::contract::RecordingReadGrantId>, StatusCode> {
    let mut values = headers.get_all(RECORDING_GRANT_HEADER).iter();
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let value = value.to_str().map_err(|_| StatusCode::BAD_REQUEST)?;
    veoveo_recording_mcp::contract::RecordingReadGrantId::parse(value)
        .map(Some)
        .map_err(|_| StatusCode::BAD_REQUEST)
}

async fn playback_manifest(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<veoveo_mcp_contract::GatewayInternalIdentity>,
    Path(recording_id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let Ok(recording_id) = parse_recording_id(&recording_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let requested_grant = match requested_read_grant(&headers) {
        Ok(grant) => grant,
        Err(status) => {
            return (
                status,
                "x-veoveo-recording-grant requires one canonical RFC UUIDv7",
            )
                .into_response();
        }
    };
    let artifact_caller = match artifact_caller(identity.clone(), &headers) {
        Ok(caller) => caller,
        Err(error) => {
            tracing::warn!(%error, %recording_id, "recording playback omitted Artifact authority");
            return StatusCode::UNAUTHORIZED.into_response();
        }
    };
    state.playback.prune_catalogs();
    let plan = match state
        .recordings
        .playback_plan(
            &identity,
            Some(&artifact_caller),
            recording_id,
            PlaybackArchiveSelection::SealedViewer,
        )
        .await
    {
        Ok(Some(plan)) => plan,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            tracing::error!(%error, %recording_id, "recording playback manifest failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let grant = match state
        .recordings
        .issue_read_grant(
            &identity,
            plan.dataset_id,
            veoveo_platform_store::RecordingReadGrantClass::ViewerSegment,
            vec![plan.recording_id],
            plan.catalog_revision.clone(),
            requested_grant,
        )
        .await
    {
        Ok(grant) => grant,
        Err(error) => {
            tracing::error!(%error, %recording_id, "recording viewer grant failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    match state.playback.prepare_manifest(plan, grant).await {
        Ok(manifest) => axum::Json(manifest).into_response(),
        Err(error) => {
            tracing::error!(%error, %recording_id, "recording playback catalog preparation failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn catalog_grant(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<veoveo_mcp_contract::GatewayInternalIdentity>,
    headers: HeaderMap,
    Json(request): Json<CreateRecordingCatalogGrantRequest>,
) -> Response {
    let requested_grant = match requested_read_grant(&headers) {
        Ok(grant) => grant,
        Err(status) => {
            return (
                status,
                "x-veoveo-recording-grant requires one canonical RFC UUIDv7",
            )
                .into_response();
        }
    };
    let recording_ids = request
        .recording_ids()
        .iter()
        .map(|id| RecordingId::from_uuid(id.as_uuid()))
        .collect::<Vec<_>>();
    let dataset_id =
        veoveo_platform_store::RecordingDatasetId::from_uuid(request.dataset_id().as_uuid());
    let artifact_caller = match artifact_caller(identity.clone(), &headers) {
        Ok(caller) => caller,
        Err(error) => {
            tracing::warn!(%error, %dataset_id, "catalog grant omitted Artifact authority");
            return StatusCode::UNAUTHORIZED.into_response();
        }
    };
    state.playback.prune_catalogs();
    let plans = match state
        .recordings
        .dataset_playback_plans(
            &identity,
            &artifact_caller,
            dataset_id,
            recording_ids.clone(),
        )
        .await
    {
        Ok(Some(plans)) => plans,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            tracing::error!(%error, %dataset_id, "dataset catalog planning failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let catalog_revision = veoveo_recording_mcp::service::catalog_set_revision(&plans);

    let grant = match state
        .recordings
        .issue_read_grant(
            &identity,
            dataset_id,
            veoveo_platform_store::RecordingReadGrantClass::CatalogDataset,
            recording_ids,
            catalog_revision,
            requested_grant,
        )
        .await
    {
        Ok(grant) => grant,
        Err(error) => {
            tracing::error!(%error, %dataset_id, "durable dataset catalog grant failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    match state.playback.prepare_catalog_grant(plans, grant).await {
        Ok(grant) => Json(grant).into_response(),
        Err(error) => {
            tracing::error!(%error, %dataset_id, "virtual dataset catalog failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn playback_live_recording(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<veoveo_mcp_contract::GatewayInternalIdentity>,
    Path(recording_id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let Ok(recording_id) = parse_recording_id(&recording_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let plan = match state
        .recordings
        .playback_plan(
            &identity,
            None,
            recording_id,
            PlaybackArchiveSelection::Omit,
        )
        .await
    {
        Ok(Some(plan)) => plan,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            tracing::error!(%error, %recording_id, "live recording authorization failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let Some(live) = plan.live.as_ref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        != Some(FRAMED_RRD_CONTENT_TYPE)
    {
        return StatusCode::NOT_ACCEPTABLE.into_response();
    }
    let Some(start) = headers
        .get(LIVE_RRD_START_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(LiveRrdStart::parse)
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    tracing::info!(
        %recording_id,
        layer_id = %live.descriptor.layer_id,
        current_byte_len = live.descriptor.current_byte_len,
        history_seconds = live.descriptor.history_seconds,
        video_preroll_seconds = live.descriptor.video_preroll_seconds,
        ?start,
        "governed Rerun channel playback opened"
    );
    let playback_store_id = match playback_store_id(plan.dataset_id, recording_id) {
        Ok(store_id) => store_id,
        Err(error) => {
            tracing::error!(%error, %recording_id, "live playback identity construction failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let stream = authorized_live_rrd_stream(
        state.recordings.clone(),
        identity,
        recording_id,
        state.recordings.live_history(),
        playback_store_id,
        start,
    );
    let mut response = Response::new(Body::from_stream(stream));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static(FRAMED_RRD_CONTENT_TYPE),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("private, no-store"),
    );
    response.headers_mut().insert(
        header::HeaderName::from_static("x-accel-buffering"),
        header::HeaderValue::from_static("no"),
    );
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        header::HeaderValue::from_static("nosniff"),
    );
    response
}

async fn playback_blueprint(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<veoveo_mcp_contract::GatewayInternalIdentity>,
    Path((recording_id, revision)): Path<(String, u64)>,
    headers: HeaderMap,
) -> Response {
    let Ok(recording_id) = parse_recording_id(&recording_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let artifact_caller = match artifact_caller(identity.clone(), &headers) {
        Ok(caller) => caller,
        Err(error) => {
            tracing::warn!(%error, %recording_id, "recording Blueprint omitted Artifact authority");
            return StatusCode::UNAUTHORIZED.into_response();
        }
    };
    state.playback.prune_catalogs();
    let plan = match state
        .recordings
        .playback_plan(
            &identity,
            Some(&artifact_caller),
            recording_id,
            PlaybackArchiveSelection::Omit,
        )
        .await
    {
        Ok(Some(plan)) => plan,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            tracing::error!(%error, %recording_id, "recording Blueprint authorization failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let Some(blueprint) = plan.blueprint.filter(|value| value.revision == revision) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let application_id = match playback_application_id(plan.dataset_id) {
        Ok(value) => value,
        Err(error) => {
            tracing::error!(%error, %recording_id, "Blueprint playback identity construction failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let result = tokio::task::spawn_blocking(move || {
        recording_scoped_blueprint(
            &blueprint.path,
            &application_id,
            &blueprint.blueprint_id,
            blueprint.byte_len,
            &blueprint.sha256,
        )
    })
    .await;
    match result {
        Ok(Ok(bytes)) => rrd_response(Body::from(bytes)),
        Ok(Err(error)) => {
            tracing::error!(%error, %recording_id, revision, "recording Blueprint playback failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
        Err(error) => {
            tracing::error!(%error, %recording_id, revision, "recording Blueprint worker failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn projection_data(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<veoveo_mcp_contract::GatewayInternalIdentity>,
    Path((recording_id, projection_id)): Path<(String, String)>,
) -> Response {
    let Ok(recording_id) = parse_recording_id(&recording_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(projection_id) =
        veoveo_recording_mcp::contract::RecordingProjectionId::parse(&projection_id)
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let projection_id = RecordingProjectionReceiptId::from_uuid(projection_id.as_uuid());
    let download = match state
        .recordings
        .projection_download(&identity, recording_id, projection_id)
        .await
    {
        Ok(Some(download)) => download,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            tracing::error!(%error, %recording_id, %projection_id, "recording projection redemption failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let file = match tokio::fs::File::open(&download.path).await {
        Ok(file) => file,
        Err(error) => {
            tracing::error!(%error, %recording_id, %projection_id, "recording projection result disappeared");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let stream = tokio_util::io::ReaderStream::new(file);
    let mut response = Response::new(Body::from_stream(stream));
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/vnd.apache.arrow.stream"),
    );
    headers.insert(
        header::CONTENT_LENGTH,
        header::HeaderValue::from_str(&download.byte_len.to_string())
            .expect("u64 is a valid Content-Length"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("private, no-store"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        header::HeaderValue::from_static("attachment; filename=recording-projection.arrow"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        header::HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::HeaderName::from_static("x-veoveo-payload-sha256"),
        header::HeaderValue::from_str(download.sha256.hex())
            .expect("SHA-256 hex is a valid header value"),
    );
    response
}

fn rrd_response(body: Body) -> Response {
    let mut response = Response::new(body);
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/vnd.rerun.rrd"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("private, no-store"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        header::HeaderValue::from_static("inline"),
    );
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        header::HeaderValue::from_static("nosniff"),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::{HeaderMap, RECORDING_GRANT_HEADER, StatusCode, requested_read_grant};
    use axum::http::HeaderValue;
    use veoveo_recording_mcp::contract::RecordingReadGrantId;

    #[test]
    fn grant_header_admits_one_typed_canonical_identity() {
        assert_eq!(requested_read_grant(&HeaderMap::new()).unwrap(), None);
        let id = RecordingReadGrantId::new();
        let mut headers = HeaderMap::new();
        headers.insert(
            RECORDING_GRANT_HEADER,
            HeaderValue::from_str(&id.to_string()).unwrap(),
        );
        assert_eq!(requested_read_grant(&headers).unwrap(), Some(id));
        headers.append(
            RECORDING_GRANT_HEADER,
            HeaderValue::from_str(&id.to_string()).unwrap(),
        );
        assert_eq!(requested_read_grant(&headers), Err(StatusCode::BAD_REQUEST));
    }

    #[test]
    fn grant_header_rejects_invalid_spelling_version_variant_and_encoding() {
        for value in [
            b"".as_slice(),
            b"private-id",
            b"01983DA0-0000-7000-8000-000000000001",
            b"01983da0-0000-4000-8000-000000000001",
            b"01983da0-0000-7000-c000-000000000001",
            &[0xff],
        ] {
            let mut headers = HeaderMap::new();
            headers.insert(
                RECORDING_GRANT_HEADER,
                HeaderValue::from_bytes(value).unwrap(),
            );
            assert_eq!(requested_read_grant(&headers), Err(StatusCode::BAD_REQUEST));
        }
    }
}
