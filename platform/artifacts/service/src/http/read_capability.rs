use super::*;
use veoveo_mcp_contract::{
    ArtifactReadCapabilityId, ArtifactReadCapabilityScope, ArtifactTaskId,
    IssueArtifactReadCapabilityRequest, IssuedArtifactReadCapability,
};

pub(super) fn routes<R: ArtifactRepository + 'static, S: BlobStore + 'static>()
-> Router<AppState<R, S>> {
    Router::new()
        .route("/artifact-read-capabilities", post(issue::<R, S>))
        .route(
            "/artifact-read-capabilities/{capability}",
            get(scope::<R, S>).delete(revoke::<R, S>),
        )
        .route(
            "/artifact-read-capabilities/{capability}/artifacts/{artifact}/meta",
            get(metadata::<R, S>),
        )
        .route(
            "/artifact-read-capabilities/{capability}/artifacts/{artifact}/download",
            get(download::<R, S>),
        )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskQuery {
    task_id: ArtifactTaskId,
}

fn capability_id(value: &str) -> Result<ArtifactReadCapabilityId, ApiError> {
    ArtifactReadCapabilityId::parse(value).map_err(ApiError)
}

async fn issue<R: ArtifactRepository, S: BlobStore>(
    State(state): State<AppState<R, S>>,
    headers: HeaderMap,
    Json(request): Json<IssueArtifactReadCapabilityRequest>,
) -> Result<Json<IssuedArtifactReadCapability>, ApiError> {
    let caller = caller(&state, &headers)?;
    Ok(Json(
        state
            .service
            .issue_read_capability(&caller, request)
            .await?,
    ))
}

async fn revoke<R: ArtifactRepository, S: BlobStore>(
    State(state): State<AppState<R, S>>,
    Path(capability): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let caller = caller(&state, &headers)?;
    state
        .service
        .revoke_read_capability(&caller, capability_id(&capability)?)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn metadata<R: ArtifactRepository, S: BlobStore>(
    State(state): State<AppState<R, S>>,
    Path((capability, artifact)): Path<(String, String)>,
    Query(query): Query<TaskQuery>,
    headers: HeaderMap,
) -> Result<Json<ArtifactMetadata>, ApiError> {
    Ok(Json(
        state
            .service
            .head_with_read_capability(
                capability_id(&capability)?,
                bearer(&headers)?,
                query.task_id,
                parse_artifact_id(&artifact)?,
            )
            .await?,
    ))
}

async fn download<R: ArtifactRepository, S: BlobStore>(
    State(state): State<AppState<R, S>>,
    Path((capability, artifact)): Path<(String, String)>,
    Query(query): Query<TaskQuery>,
    method: Method,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let body = if method == Method::HEAD {
        DownloadBody::Omit
    } else {
        DownloadBody::Include
    };
    let download = state
        .service
        .download_with_read_capability(
            capability_id(&capability)?,
            bearer(&headers)?,
            query.task_id,
            parse_artifact_id(&artifact)?,
            requested_range(&headers)?,
            body,
        )
        .await?;
    download_response(download)
}

async fn scope<R: ArtifactRepository, S: BlobStore>(
    State(state): State<AppState<R, S>>,
    Path(capability): Path<String>,
    Query(query): Query<TaskQuery>,
    headers: HeaderMap,
) -> Result<Json<ArtifactReadCapabilityScope>, ApiError> {
    Ok(Json(
        state
            .service
            .read_capability_scope(
                capability_id(&capability)?,
                bearer(&headers)?,
                query.task_id,
            )
            .await?,
    ))
}
