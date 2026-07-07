//! Internal HTTP surface for the artifact plane.
//!
//! This is service-to-service plumbing, not an MCP client contract: domain
//! servers call it with a forwarded gateway bearer. Every handler authenticates,
//! then delegates to the [`ArtifactService`], which is the policy-enforcement
//! point.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use veoveo_mcp_contract::access::{AccessLevel, ArtifactSha256, Subject};
use veoveo_mcp_contract::{
    ArtifactPlane, ArtifactPlaneError, GrantList, PlaneCaller, PutArtifactRequest, PutGrantRequest,
};

use crate::PlaneAuthenticator;
use crate::service::ArtifactService;
use crate::{EncryptedObjectStore, PostgresGrantLedger};

type ProdService = ArtifactService<PostgresGrantLedger, EncryptedObjectStore>;

#[derive(Clone)]
pub struct AppState {
    service: Arc<ProdService>,
    auth: PlaneAuthenticator,
}

impl AppState {
    pub fn new(service: ProdService, auth: PlaneAuthenticator) -> Self {
        Self {
            service: Arc::new(service),
            auth,
        }
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(healthz))
        .route("/artifacts", post(put_artifact))
        .route("/artifacts/{sha}", get(get_artifact))
        .route("/artifacts/{sha}/meta", get(head_artifact))
        .route(
            "/artifacts/{sha}/grants",
            get(list_grants).post(add_grant).delete(remove_grant),
        )
        .route("/resolve", get(resolve_artifact))
        .with_state(state)
}

/// Maps a plane error onto an HTTP status. Bodies are terse: this is internal.
struct ApiError(ArtifactPlaneError);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match &self.0 {
            ArtifactPlaneError::NotFound => (StatusCode::NOT_FOUND, self.0.to_string()),
            ArtifactPlaneError::Denied(_) => (StatusCode::FORBIDDEN, self.0.to_string()),
            ArtifactPlaneError::Unauthenticated => (StatusCode::UNAUTHORIZED, self.0.to_string()),
            ArtifactPlaneError::InvalidRequest(_) => (StatusCode::BAD_REQUEST, self.0.to_string()),
            ArtifactPlaneError::Conflict(_) => (StatusCode::CONFLICT, self.0.to_string()),
            ArtifactPlaneError::Transport(_) => (StatusCode::BAD_GATEWAY, self.0.to_string()),
        };
        (status, message).into_response()
    }
}

impl From<ArtifactPlaneError> for ApiError {
    fn from(e: ArtifactPlaneError) -> Self {
        ApiError(e)
    }
}

async fn healthz() -> &'static str {
    "ok"
}

fn caller(state: &AppState, headers: &HeaderMap) -> Result<PlaneCaller, ApiError> {
    let value = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or(ArtifactPlaneError::Unauthenticated)?;
    let token = value
        .strip_prefix("Bearer ")
        .ok_or(ArtifactPlaneError::Unauthenticated)?;
    Ok(state.auth.authenticate(token)?)
}

fn parse_sha(sha: &str) -> Result<ArtifactSha256, ApiError> {
    ArtifactSha256::new(sha)
        .map_err(|e| ApiError(ArtifactPlaneError::InvalidRequest(e.to_string())))
}

#[derive(Deserialize)]
struct LevelQuery {
    level: Option<String>,
}

fn parse_level(level: Option<&str>) -> Result<AccessLevel, ApiError> {
    match level.unwrap_or("read") {
        "read" => Ok(AccessLevel::Read),
        "write" => Ok(AccessLevel::Write),
        "admin" => Ok(AccessLevel::Admin),
        other => Err(ApiError(ArtifactPlaneError::InvalidRequest(format!(
            "unknown level `{other}`"
        )))),
    }
}

/// `POST /artifacts` — bytes in the body, `PutArtifactRequest` JSON in the
/// `x-artifact-put` header (tenant/owner are never accepted from the client).
async fn put_artifact(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let caller = caller(&state, &headers)?;
    let request: PutArtifactRequest = match headers.get("x-artifact-put") {
        Some(v) => {
            let s = v
                .to_str()
                .map_err(|_| ArtifactPlaneError::InvalidRequest("x-artifact-put not utf8".into()))?;
            serde_json::from_str(s)
                .map_err(|e| ArtifactPlaneError::InvalidRequest(format!("x-artifact-put: {e}")))?
        }
        None => PutArtifactRequest::default(),
    };
    let metadata = state
        .service
        .put(&caller, request, body.to_vec())
        .await?;
    Ok((StatusCode::CREATED, Json(metadata)).into_response())
}

/// `GET /artifacts/{sha}` — returns the raw bytes.
async fn get_artifact(
    State(state): State<AppState>,
    Path(sha): Path<String>,
    Query(q): Query<LevelQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let caller = caller(&state, &headers)?;
    let sha = parse_sha(&sha)?;
    let level = parse_level(q.level.as_deref())?;
    let object = state.service.get(&caller, &sha, level).await?;
    let mime = object
        .metadata
        .mime_type
        .clone()
        .unwrap_or_else(|| "application/octet-stream".to_string());
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, mime),
            ("x-artifact-sha256".parse().unwrap(), object.metadata.sha256.clone()),
            ("x-artifact-uri".parse().unwrap(), object.metadata.artifact_uri.clone()),
        ],
        object.bytes,
    )
        .into_response())
}

/// `GET /artifacts/{sha}/meta` — metadata only, gated at read.
async fn head_artifact(
    State(state): State<AppState>,
    Path(sha): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let caller = caller(&state, &headers)?;
    let sha = parse_sha(&sha)?;
    let metadata = state.service.head(&caller, &sha).await?;
    Ok(Json(metadata).into_response())
}

/// `GET /resolve?uri=artifact://{sha}` — cross-server input resolution.
#[derive(Deserialize)]
struct ResolveQuery {
    uri: String,
}

async fn resolve_artifact(
    State(state): State<AppState>,
    Query(q): Query<ResolveQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let caller = caller(&state, &headers)?;
    let object = state.service.resolve(&caller, &q.uri).await?;
    let mime = object
        .metadata
        .mime_type
        .clone()
        .unwrap_or_else(|| "application/octet-stream".to_string());
    Ok((StatusCode::OK, [(header::CONTENT_TYPE, mime)], object.bytes).into_response())
}

async fn add_grant(
    State(state): State<AppState>,
    Path(sha): Path<String>,
    headers: HeaderMap,
    Json(req): Json<PutGrantRequest>,
) -> Result<Response, ApiError> {
    let caller = caller(&state, &headers)?;
    let sha = parse_sha(&sha)?;
    state
        .service
        .grant(&caller, &sha, req.subject, req.level)
        .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn remove_grant(
    State(state): State<AppState>,
    Path(sha): Path<String>,
    headers: HeaderMap,
    Json(subject): Json<Subject>,
) -> Result<Response, ApiError> {
    let caller = caller(&state, &headers)?;
    let sha = parse_sha(&sha)?;
    state.service.revoke(&caller, &sha, &subject).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn list_grants(
    State(state): State<AppState>,
    Path(sha): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let caller = caller(&state, &headers)?;
    let sha = parse_sha(&sha)?;
    let grants = state.service.list_grants(&caller, &sha).await?;
    Ok(Json(GrantList { grants }).into_response())
}
