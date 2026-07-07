//! Internal HTTP surface for the artifact plane.
//!
//! This is service-to-service plumbing, not an MCP client contract: domain
//! servers call it with a forwarded gateway bearer. Every handler authenticates,
//! then delegates to an [`ArtifactPlane`] (in production, the `ArtifactService`
//! PEP). The router is generic over the plane so tests can drive it with the
//! in-memory reference implementation.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use serde::Deserialize;
use veoveo_mcp_contract::access::{AccessLevel, ArtifactSha256, Subject};
use veoveo_mcp_contract::storage::ArtifactMetadata;
use veoveo_mcp_contract::{
    ArtifactPlane, ArtifactPlaneError, GrantList, PlaneCaller, PutArtifactRequest, PutGrantRequest,
};

use crate::PlaneAuthenticator;

/// Shared handler state: the plane implementation plus the authenticator.
pub struct AppState<P> {
    service: Arc<P>,
    auth: PlaneAuthenticator,
}

impl<P> Clone for AppState<P> {
    fn clone(&self) -> Self {
        Self {
            service: Arc::clone(&self.service),
            auth: self.auth.clone(),
        }
    }
}

impl<P> AppState<P> {
    pub fn new(service: P, auth: PlaneAuthenticator) -> Self {
        Self {
            service: Arc::new(service),
            auth,
        }
    }
}

/// Anything the router needs from a plane.
pub trait PlaneService: ArtifactPlane + Send + Sync + 'static {}
impl<T: ArtifactPlane + Send + Sync + 'static> PlaneService for T {}

pub fn router<P: PlaneService>(state: AppState<P>) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(healthz))
        .route("/artifacts", post(put_artifact::<P>))
        .route("/artifacts/{sha}", get(get_artifact::<P>))
        .route("/artifacts/{sha}/meta", get(head_artifact::<P>))
        .route(
            "/artifacts/{sha}/grants",
            get(list_grants::<P>)
                .post(add_grant::<P>)
                .delete(remove_grant::<P>),
        )
        .route("/resolve", get(resolve_artifact::<P>))
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
        // On a denial, carry the precise decision so the client keeps the reason
        // chain (tenant / clearance / need-to-know) rather than a coarse 403.
        if let ArtifactPlaneError::Denied(decision) = &self.0 {
            if let Ok(encoded) = serde_json::to_string(decision) {
                return (status, [("x-artifact-decision", encoded)], message).into_response();
            }
        }
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

fn caller<P>(state: &AppState<P>, headers: &HeaderMap) -> Result<PlaneCaller, ApiError> {
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

/// The canonical HTTP encoding of an artifact read: raw bytes in the body, the
/// full [`ArtifactMetadata`] as a base64 JSON header so the client rebuilds an
/// `ArtifactObject` in one round trip.
fn bytes_with_metadata(metadata: &ArtifactMetadata, bytes: Vec<u8>) -> Response {
    let mime = metadata
        .mime_type
        .clone()
        .unwrap_or_else(|| "application/octet-stream".to_string());
    let meta_json = serde_json::to_vec(metadata).unwrap_or_default();
    let meta_b64 = base64::engine::general_purpose::STANDARD.encode(meta_json);
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, mime),
            ("x-artifact-sha256".parse().unwrap(), metadata.sha256.clone()),
            ("x-artifact-metadata".parse().unwrap(), meta_b64),
        ],
        bytes,
    )
        .into_response()
}

/// `POST /artifacts` — bytes in the body, `PutArtifactRequest` JSON in the
/// `x-artifact-put` header (tenant/owner are never accepted from the client).
async fn put_artifact<P: PlaneService>(
    State(state): State<AppState<P>>,
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
    let metadata = state.service.put(&caller, request, body.to_vec()).await?;
    Ok((StatusCode::CREATED, Json(metadata)).into_response())
}

/// `GET /artifacts/{sha}` — returns the raw bytes plus metadata header.
async fn get_artifact<P: PlaneService>(
    State(state): State<AppState<P>>,
    Path(sha): Path<String>,
    Query(q): Query<LevelQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let caller = caller(&state, &headers)?;
    let sha = parse_sha(&sha)?;
    let level = parse_level(q.level.as_deref())?;
    let object = state.service.get(&caller, &sha, level).await?;
    Ok(bytes_with_metadata(&object.metadata, object.bytes))
}

/// `GET /artifacts/{sha}/meta` — metadata only, gated at read.
async fn head_artifact<P: PlaneService>(
    State(state): State<AppState<P>>,
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

async fn resolve_artifact<P: PlaneService>(
    State(state): State<AppState<P>>,
    Query(q): Query<ResolveQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let caller = caller(&state, &headers)?;
    let object = state.service.resolve(&caller, &q.uri).await?;
    Ok(bytes_with_metadata(&object.metadata, object.bytes))
}

async fn add_grant<P: PlaneService>(
    State(state): State<AppState<P>>,
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

async fn remove_grant<P: PlaneService>(
    State(state): State<AppState<P>>,
    Path(sha): Path<String>,
    headers: HeaderMap,
    Json(subject): Json<Subject>,
) -> Result<Response, ApiError> {
    let caller = caller(&state, &headers)?;
    let sha = parse_sha(&sha)?;
    state.service.revoke(&caller, &sha, &subject).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn list_grants<P: PlaneService>(
    State(state): State<AppState<P>>,
    Path(sha): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let caller = caller(&state, &headers)?;
    let sha = parse_sha(&sha)?;
    let grants = state.service.list_grants(&caller, &sha).await?;
    Ok(Json(GrantList { grants }).into_response())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use chrono::{TimeDelta, Utc};
    use veoveo_artifact_client::HttpArtifactPlane;
    use veoveo_mcp_contract::access::{AccessDecision, AccessLevel, ArtifactSha256};
    use veoveo_mcp_contract::gateway::{
        GatewayProfileId, PrincipalId, PrincipalKind, ServerSlug, TokenIssuer, TokenSubject,
    };
    use veoveo_mcp_contract::internal_auth::{
        GatewayInternalTokenIssuer, InternalTokenSecret,
    };
    use veoveo_mcp_contract::{
        ArtifactPlane, ArtifactPlaneError, JwtId, PlaneCaller, Principal, PutArtifactRequest,
        TenantId,
    };

    use super::*;
    use crate::ledger::testing::InMemoryLedger;
    use crate::service::ArtifactService;
    use crate::store::testing::InMemoryBlobStore;

    fn secret() -> InternalTokenSecret {
        InternalTokenSecret::new("local-dev-internal-token-secret-32-bytes-minimum").unwrap()
    }

    fn principal(id: &str, tenant: &str) -> Principal {
        Principal {
            id: PrincipalId::new(id).unwrap(),
            kind: PrincipalKind::User,
            issuer: TokenIssuer::new("https://idp.example.com").unwrap(),
            subject: TokenSubject::new("s").unwrap(),
            tenant: Some(TenantId::new(tenant).unwrap()),
            groups: BTreeSet::new(),
            roles: BTreeSet::new(),
            scopes: BTreeSet::new(),
            data_labels: BTreeSet::new(),
            assurances: BTreeSet::new(),
            authenticated_at: Some(Utc::now()),
        }
    }

    /// Mint a real gateway-signed internal token and package it as a caller,
    /// exactly as a domain server would forward it.
    fn signed_caller(id: &str, tenant: &str) -> PlaneCaller {
        let issuer =
            GatewayInternalTokenIssuer::new(TokenIssuer::new("veoveo-internal").unwrap(), secret());
        let issued = issuer
            .issue(
                GatewayProfileId::new("operator").unwrap(),
                ServerSlug::new("duckdb").unwrap(),
                principal(id, tenant),
                Utc::now() + TimeDelta::minutes(5),
            )
            .unwrap();
        PlaneCaller {
            bearer_token: issued.bearer_token,
            identity: issued.identity,
            memberships: BTreeSet::new(),
        }
    }

    async fn spawn_service() -> String {
        let service = ArtifactService::new(InMemoryLedger::default(), InMemoryBlobStore::default());
        let auth = PlaneAuthenticator::new(
            TokenIssuer::new("veoveo-internal").unwrap(),
            vec![ServerSlug::new("duckdb").unwrap()],
            secret(),
        );
        let app = router(AppState::new(service, auth));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn end_to_end_put_get_and_denials_over_http() {
        let base = spawn_service().await;
        let plane = HttpArtifactPlane::new(&base);

        let alice = signed_caller("alice", "acme");
        let meta = plane
            .put(&alice, PutArtifactRequest::default(), b"hello plane".to_vec())
            .await
            .unwrap();
        assert_eq!(meta.compliance.tenant_id.as_ref().unwrap().as_str(), "acme");
        let sha = ArtifactSha256::new(meta.sha256.clone()).unwrap();

        // Owner reads bytes + metadata back over HTTP.
        let obj = plane.get(&alice, &sha, AccessLevel::Read).await.unwrap();
        assert_eq!(obj.bytes, b"hello plane");
        assert_eq!(obj.metadata.sha256, meta.sha256);

        // Same-tenant stranger: DenyNeedToKnow, reason preserved via header.
        let bob = signed_caller("bob", "acme");
        assert_eq!(
            plane.get(&bob, &sha, AccessLevel::Read).await,
            Err(ArtifactPlaneError::Denied(AccessDecision::DenyNeedToKnow))
        );

        // Cross-tenant: even after a grant by id, DenyTenant.
        plane
            .grant(
                &alice,
                &sha,
                veoveo_mcp_contract::access::Subject::User(
                    PrincipalId::new("mallory").unwrap(),
                ),
                AccessLevel::Read,
            )
            .await
            .unwrap();
        let mallory = signed_caller("mallory", "evil");
        assert_eq!(
            plane.get(&mallory, &sha, AccessLevel::Read).await,
            Err(ArtifactPlaneError::Denied(AccessDecision::DenyTenant))
        );

        // Resolve via the neutral plane URI (cross-server input path).
        let resolved = plane.resolve(&alice, &meta.artifact_uri).await.unwrap();
        assert_eq!(resolved.bytes, b"hello plane");
    }

    #[tokio::test]
    async fn rejects_unsigned_and_missing_tokens() {
        let base = spawn_service().await;
        let plane = HttpArtifactPlane::new(&base);
        let mut forged = signed_caller("alice", "acme");
        forged.bearer_token = "not-a-jwt".to_string();
        let sha = ArtifactSha256::new("a".repeat(64)).unwrap();
        assert_eq!(
            plane.get(&forged, &sha, AccessLevel::Read).await,
            Err(ArtifactPlaneError::Unauthenticated)
        );
    }
}
