//! HTTP transport for the artifact plane.

use std::sync::Arc;

use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use serde::Deserialize;
use veoveo_mcp_contract::access::{AccessLevel, ArtifactId, Subject};
use veoveo_mcp_contract::storage::ArtifactMetadata;
use veoveo_mcp_contract::{
    ArtifactPlane, ArtifactPlaneError, ArtifactShareLinkId, ArtifactWriteCapabilityId,
    CreateArtifactShareLinkRequest, GrantList, IssueArtifactWriteCapabilityRequest,
    ListArtifactsRequest, PlaneCaller, PutArtifactRequest, PutGrantRequest,
    RedeemArtifactWriteCapabilityRequest, SetArtifactReleaseStateRequest,
};

use crate::PlaneAuthenticator;
use crate::ledger::ArtifactRepository;
use crate::service::{ArtifactDownload, ArtifactService, DownloadDelivery};
use crate::store::BlobStore;

const MAX_UPLOAD_BODY_BYTES: usize = 256 * 1024 * 1024;

pub struct AppState<R: ArtifactRepository, S: BlobStore> {
    service: Arc<ArtifactService<R, S>>,
    auth: PlaneAuthenticator,
}

impl<R: ArtifactRepository, S: BlobStore> Clone for AppState<R, S> {
    fn clone(&self) -> Self {
        Self {
            service: Arc::clone(&self.service),
            auth: self.auth.clone(),
        }
    }
}

impl<R: ArtifactRepository, S: BlobStore> AppState<R, S> {
    pub fn new(service: ArtifactService<R, S>, auth: PlaneAuthenticator) -> Self {
        Self {
            service: Arc::new(service),
            auth,
        }
    }
}

pub fn router<R, S>(state: AppState<R, S>) -> Router
where
    R: ArtifactRepository + 'static,
    S: BlobStore + 'static,
{
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(healthz))
        .route(
            "/artifacts",
            get(list_artifacts::<R, S>).post(put_artifact::<R, S>),
        )
        .route("/artifacts/{artifact_id}", get(get_artifact::<R, S>))
        .route("/artifacts/{artifact_id}/meta", get(head_artifact::<R, S>))
        .route(
            "/artifacts/{artifact_id}/download",
            get(download_artifact::<R, S>),
        )
        .route(
            "/artifacts/{artifact_id}/grants",
            get(list_grants::<R, S>)
                .post(add_grant::<R, S>)
                .delete(remove_grant::<R, S>),
        )
        .route(
            "/artifacts/{artifact_id}/release-state",
            axum::routing::put(set_release_state::<R, S>),
        )
        .route(
            "/artifacts/{artifact_id}/share-links",
            post(create_share_link::<R, S>),
        )
        .route(
            "/artifacts/{artifact_id}/share-links/{link_id}",
            axum::routing::delete(revoke_share_link::<R, S>),
        )
        .route(
            "/artifact-write-capabilities",
            post(issue_write_capability::<R, S>),
        )
        .route(
            "/artifact-write-capabilities/{capability_id}/redeem",
            post(redeem_write_capability::<R, S>),
        )
        .route("/resolve", get(resolve_artifact::<R, S>))
        .route("/s/{token}", get(redeem_public_share::<R, S>))
        .layer(DefaultBodyLimit::max(MAX_UPLOAD_BODY_BYTES))
        .with_state(state)
}

pub struct ApiError(ArtifactPlaneError);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match &self.0 {
            ArtifactPlaneError::NotFound => (StatusCode::NOT_FOUND, self.0.to_string()),
            ArtifactPlaneError::Denied(_) => (StatusCode::FORBIDDEN, self.0.to_string()),
            ArtifactPlaneError::Unauthenticated => (StatusCode::UNAUTHORIZED, self.0.to_string()),
            ArtifactPlaneError::InvalidRequest(_) => (StatusCode::BAD_REQUEST, self.0.to_string()),
            ArtifactPlaneError::Conflict(_) => (StatusCode::CONFLICT, self.0.to_string()),
            ArtifactPlaneError::Transport(_) => {
                (StatusCode::SERVICE_UNAVAILABLE, self.0.to_string())
            }
        };
        if let ArtifactPlaneError::Denied(decision) = &self.0
            && let Ok(encoded) = serde_json::to_string(decision)
        {
            return (status, [("x-artifact-decision", encoded)], message).into_response();
        }
        (status, message).into_response()
    }
}

impl From<ArtifactPlaneError> for ApiError {
    fn from(error: ArtifactPlaneError) -> Self {
        Self(error)
    }
}

async fn healthz() -> &'static str {
    "ok"
}

fn bearer(headers: &HeaderMap) -> Result<&str, ApiError> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.is_empty())
        .ok_or(ApiError(ArtifactPlaneError::Unauthenticated))
}

fn caller<R: ArtifactRepository, S: BlobStore>(
    state: &AppState<R, S>,
    headers: &HeaderMap,
) -> Result<PlaneCaller, ApiError> {
    Ok(state.auth.authenticate(bearer(headers)?)?)
}

fn parse_artifact_id(value: &str) -> Result<ArtifactId, ApiError> {
    ArtifactId::parse(value)
        .map_err(|error| ApiError(ArtifactPlaneError::InvalidRequest(error.to_string())))
}

fn put_request(headers: &HeaderMap) -> Result<PutArtifactRequest, ApiError> {
    match headers.get("x-artifact-put") {
        Some(value) => {
            let value = value.to_str().map_err(|_| {
                ArtifactPlaneError::InvalidRequest("x-artifact-put must be UTF-8".into())
            })?;
            serde_json::from_str(value).map_err(|error| {
                ApiError(ArtifactPlaneError::InvalidRequest(format!(
                    "invalid x-artifact-put: {error}"
                )))
            })
        }
        None => Ok(PutArtifactRequest::default()),
    }
}

#[derive(Deserialize)]
struct LevelQuery {
    level: Option<String>,
}

fn parse_level(value: Option<&str>) -> Result<AccessLevel, ApiError> {
    match value.unwrap_or("read") {
        "read" => Ok(AccessLevel::Read),
        "write" => Ok(AccessLevel::Write),
        "admin" => Ok(AccessLevel::Admin),
        other => Err(ApiError(ArtifactPlaneError::InvalidRequest(format!(
            "unknown access level `{other}`"
        )))),
    }
}

fn metadata_headers(metadata: &ArtifactMetadata) -> Result<HeaderMap, ApiError> {
    let encoded = base64::engine::general_purpose::STANDARD.encode(
        serde_json::to_vec(metadata)
            .map_err(|error| ArtifactPlaneError::Transport(error.to_string()))?,
    );
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(
            metadata
                .mime_type
                .as_deref()
                .unwrap_or("application/octet-stream"),
        )
        .map_err(|_| ArtifactPlaneError::InvalidRequest("invalid artifact mime type".into()))?,
    );
    headers.insert(
        "x-artifact-id",
        HeaderValue::from_str(&metadata.artifact_id.to_string())
            .map_err(|error| ArtifactPlaneError::Transport(error.to_string()))?,
    );
    headers.insert(
        "x-artifact-metadata",
        HeaderValue::from_str(&encoded)
            .map_err(|error| ArtifactPlaneError::Transport(error.to_string()))?,
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; sandbox"),
    );
    Ok(headers)
}

fn bytes_with_metadata(metadata: &ArtifactMetadata, bytes: Vec<u8>) -> Result<Response, ApiError> {
    let mut response = Response::new(Body::from(bytes));
    *response.status_mut() = StatusCode::OK;
    *response.headers_mut() = metadata_headers(metadata)?;
    Ok(response)
}

fn download_response(download: ArtifactDownload) -> Result<Response, ApiError> {
    match download.delivery {
        DownloadDelivery::Bytes(bytes) => bytes_with_metadata(&download.metadata, bytes),
        DownloadDelivery::SignedRedirect(url) => {
            let mut response = StatusCode::TEMPORARY_REDIRECT.into_response();
            response.headers_mut().insert(
                header::LOCATION,
                HeaderValue::from_str(&url)
                    .map_err(|error| ArtifactPlaneError::Transport(error.to_string()))?,
            );
            response
                .headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            response.headers_mut().insert(
                header::REFERRER_POLICY,
                HeaderValue::from_static("no-referrer"),
            );
            Ok(response)
        }
        DownloadDelivery::Stream(stream) => {
            let mut response = Response::new(Body::from_stream(stream));
            *response.status_mut() = StatusCode::OK;
            *response.headers_mut() = metadata_headers(&download.metadata)?;
            Ok(response)
        }
    }
}

async fn put_artifact<R: ArtifactRepository, S: BlobStore>(
    State(state): State<AppState<R, S>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let caller = caller(&state, &headers)?;
    let metadata = state
        .service
        .put(&caller, put_request(&headers)?, body.to_vec())
        .await?;
    Ok((StatusCode::CREATED, Json(metadata)).into_response())
}

async fn list_artifacts<R: ArtifactRepository, S: BlobStore>(
    State(state): State<AppState<R, S>>,
    Query(request): Query<ListArtifactsRequest>,
    headers: HeaderMap,
) -> Result<Json<veoveo_mcp_contract::ArtifactPage>, ApiError> {
    let caller = caller(&state, &headers)?;
    Ok(Json(state.service.list(&caller, request).await?))
}

async fn get_artifact<R: ArtifactRepository, S: BlobStore>(
    State(state): State<AppState<R, S>>,
    Path(artifact_id): Path<String>,
    Query(query): Query<LevelQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let caller = caller(&state, &headers)?;
    let artifact_id = parse_artifact_id(&artifact_id)?;
    let object = state
        .service
        .get(&caller, &artifact_id, parse_level(query.level.as_deref())?)
        .await?;
    bytes_with_metadata(&object.metadata, object.bytes)
}

async fn head_artifact<R: ArtifactRepository, S: BlobStore>(
    State(state): State<AppState<R, S>>,
    Path(artifact_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ArtifactMetadata>, ApiError> {
    let caller = caller(&state, &headers)?;
    Ok(Json(
        state
            .service
            .head(&caller, &parse_artifact_id(&artifact_id)?)
            .await?,
    ))
}

async fn download_artifact<R: ArtifactRepository, S: BlobStore>(
    State(state): State<AppState<R, S>>,
    Path(artifact_id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let caller = caller(&state, &headers)?;
    download_response(
        state
            .service
            .download(&caller, parse_artifact_id(&artifact_id)?)
            .await?,
    )
}

#[derive(Deserialize)]
struct ResolveQuery {
    uri: String,
}

async fn resolve_artifact<R: ArtifactRepository, S: BlobStore>(
    State(state): State<AppState<R, S>>,
    Query(query): Query<ResolveQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let caller = caller(&state, &headers)?;
    let object = state.service.resolve(&caller, &query.uri).await?;
    bytes_with_metadata(&object.metadata, object.bytes)
}

async fn add_grant<R: ArtifactRepository, S: BlobStore>(
    State(state): State<AppState<R, S>>,
    Path(artifact_id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<PutGrantRequest>,
) -> Result<StatusCode, ApiError> {
    let caller = caller(&state, &headers)?;
    state
        .service
        .grant(
            &caller,
            &parse_artifact_id(&artifact_id)?,
            request.subject,
            request.level,
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_grant<R: ArtifactRepository, S: BlobStore>(
    State(state): State<AppState<R, S>>,
    Path(artifact_id): Path<String>,
    headers: HeaderMap,
    Json(subject): Json<Subject>,
) -> Result<StatusCode, ApiError> {
    let caller = caller(&state, &headers)?;
    state
        .service
        .revoke(&caller, &parse_artifact_id(&artifact_id)?, &subject)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_grants<R: ArtifactRepository, S: BlobStore>(
    State(state): State<AppState<R, S>>,
    Path(artifact_id): Path<String>,
    headers: HeaderMap,
) -> Result<Json<GrantList>, ApiError> {
    let caller = caller(&state, &headers)?;
    Ok(Json(GrantList {
        grants: state
            .service
            .list_grants(&caller, &parse_artifact_id(&artifact_id)?)
            .await?,
    }))
}

async fn set_release_state<R: ArtifactRepository, S: BlobStore>(
    State(state): State<AppState<R, S>>,
    Path(artifact_id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<SetArtifactReleaseStateRequest>,
) -> Result<Json<ArtifactMetadata>, ApiError> {
    let caller = caller(&state, &headers)?;
    Ok(Json(
        state
            .service
            .set_release_state(
                &caller,
                &parse_artifact_id(&artifact_id)?,
                request.release_state,
            )
            .await?,
    ))
}

async fn create_share_link<R: ArtifactRepository, S: BlobStore>(
    State(state): State<AppState<R, S>>,
    Path(artifact_id): Path<String>,
    headers: HeaderMap,
    Json(request): Json<CreateArtifactShareLinkRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let caller = caller(&state, &headers)?;
    let link = state
        .service
        .create_share_link(&caller, &parse_artifact_id(&artifact_id)?, request)
        .await?;
    Ok((StatusCode::CREATED, Json(link)))
}

async fn revoke_share_link<R: ArtifactRepository, S: BlobStore>(
    State(state): State<AppState<R, S>>,
    Path((artifact_id, link_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let caller = caller(&state, &headers)?;
    let link_id = ArtifactShareLinkId::parse(link_id)?;
    state
        .service
        .revoke_share_link(&caller, &parse_artifact_id(&artifact_id)?, &link_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn issue_write_capability<R: ArtifactRepository, S: BlobStore>(
    State(state): State<AppState<R, S>>,
    headers: HeaderMap,
    Json(request): Json<IssueArtifactWriteCapabilityRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let caller = caller(&state, &headers)?;
    let issued = state
        .service
        .issue_write_capability(&caller, request)
        .await?;
    Ok((StatusCode::CREATED, Json(issued)))
}

async fn redeem_write_capability<R: ArtifactRepository, S: BlobStore>(
    State(state): State<AppState<R, S>>,
    Path(capability_id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, ApiError> {
    let capability_id = ArtifactWriteCapabilityId::parse(capability_id)?;
    let request_header = headers
        .get("x-artifact-capability-redeem")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| {
            ArtifactPlaneError::InvalidRequest("missing x-artifact-capability-redeem header".into())
        })?;
    let request: RedeemArtifactWriteCapabilityRequest = serde_json::from_str(request_header)
        .map_err(|error| {
            ArtifactPlaneError::InvalidRequest(format!(
                "invalid x-artifact-capability-redeem: {error}"
            ))
        })?;
    if request.capability_id != capability_id {
        return Err(ArtifactPlaneError::InvalidRequest(
            "capability id does not match request path".into(),
        )
        .into());
    }
    let metadata = state
        .service
        .redeem_write_capability(bearer(&headers)?, request, body.to_vec())
        .await?;
    Ok((StatusCode::CREATED, Json(metadata)))
}

async fn redeem_public_share<R: ArtifactRepository, S: BlobStore>(
    State(state): State<AppState<R, S>>,
    Path(token): Path<String>,
) -> Result<Response, ApiError> {
    match state.service.redeem_public_share(&token).await {
        Ok(download) => download_response(download),
        Err(ArtifactPlaneError::Transport(message)) => {
            Err(ApiError(ArtifactPlaneError::Transport(message)))
        }
        Err(_) => Err(ApiError(ArtifactPlaneError::NotFound)),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::num::{NonZeroU32, NonZeroU64};

    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
    use chrono::{TimeDelta, Utc};
    use veoveo_artifact_client::HttpArtifactPlane;
    use veoveo_mcp_contract::gateway::{
        GatewayProfileId, PrincipalId, PrincipalKind, ServerSlug, TenantId, TokenIssuer,
        TokenSubject,
    };
    use veoveo_mcp_contract::internal_auth::{
        GatewayInternalSigningKey, GatewayInternalTokenIssuer, GatewayInternalTrustBundle,
    };
    use veoveo_mcp_contract::{
        ArtifactPlane, ArtifactReleaseState, ArtifactWriteCapabilityId,
        CreateArtifactShareLinkRequest, IssueArtifactWriteCapabilityRequest, PlaneCaller,
        Principal, PutArtifactRequest, RedeemArtifactWriteCapabilityRequest,
    };

    use super::*;
    use crate::ledger::testing::InMemoryRepository;
    use crate::store::testing::InMemoryBlobStore;

    const KEY_ID: &str = "artifact-http-test";
    const PRIVATE_KEY_DER_B64: &str =
        "MC4CAQAwBQYDK2VwBCIEII4AsVspz8h7mpqvOkgslJP07HfqpiWMZA+6Ii90lVBl";
    const PUBLIC_KEY_X: &str = "OMOoJJu_AQS7UM8u2GVtMVj8W1zcE6QhR0DMBr9HEcg";

    fn signing_key() -> GatewayInternalSigningKey {
        GatewayInternalSigningKey::new(KEY_ID, BASE64_STANDARD.decode(PRIVATE_KEY_DER_B64).unwrap())
            .unwrap()
    }

    fn trust_bundle() -> GatewayInternalTrustBundle {
        GatewayInternalTrustBundle::from_json(&format!(
            r#"{{"keys":[{{"kty":"OKP","crv":"Ed25519","x":"{PUBLIC_KEY_X}","alg":"EdDSA","use":"sig","kid":"{KEY_ID}"}}]}}"#
        ))
        .unwrap()
    }

    fn signed_caller() -> PlaneCaller {
        let now = Utc::now();
        let issuer = GatewayInternalTokenIssuer::new(
            TokenIssuer::new("veoveo-internal").unwrap(),
            signing_key(),
        );
        let issued = issuer
            .issue(
                GatewayProfileId::new("operator").unwrap(),
                ServerSlug::new("media").unwrap(),
                Principal {
                    id: PrincipalId::new("alice").unwrap(),
                    kind: PrincipalKind::User,
                    issuer: TokenIssuer::new("https://idp.example.com").unwrap(),
                    subject: TokenSubject::new("alice-subject").unwrap(),
                    tenant: Some(TenantId::new("acme").unwrap()),
                    groups: BTreeSet::new(),
                    group_roles: BTreeSet::new(),
                    roles: BTreeSet::new(),
                    scopes: BTreeSet::new(),
                    data_labels: BTreeSet::new(),
                    assurances: BTreeSet::new(),
                    authenticated_at: Some(now),
                },
                now + TimeDelta::minutes(5),
            )
            .unwrap();
        PlaneCaller {
            bearer_token: issued.bearer_token,
            identity: issued.identity,
            memberships: BTreeSet::new(),
        }
    }

    async fn spawn_service() -> (String, PlaneCaller) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let service = ArtifactService::with_options(
            InMemoryRepository::default(),
            InMemoryBlobStore::default(),
            &base,
            1024,
            8,
        );
        let auth = PlaneAuthenticator::new(
            TokenIssuer::new("veoveo-internal").unwrap(),
            vec![ServerSlug::new("media").unwrap()],
            trust_bundle(),
        );
        let app = router(AppState::new(service, auth));
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (base, signed_caller())
    }

    #[tokio::test]
    async fn public_link_is_release_gated_counted_and_requires_no_identity() {
        let (base, caller) = spawn_service().await;
        let plane = HttpArtifactPlane::new(&base);
        let metadata = plane
            .put(
                &caller,
                PutArtifactRequest::default(),
                b"shared-data".to_vec(),
            )
            .await
            .unwrap();
        let listed = plane
            .list(&caller, ListArtifactsRequest::default())
            .await
            .unwrap();
        assert_eq!(listed.artifacts.len(), 1);
        assert_eq!(listed.artifacts[0].artifact_id, metadata.artifact_id);
        plane
            .set_release_state(
                &caller,
                &metadata.artifact_id,
                ArtifactReleaseState::Releasable,
            )
            .await
            .unwrap();
        let link = plane
            .create_share_link(
                &caller,
                &metadata.artifact_id,
                CreateArtifactShareLinkRequest {
                    expires_at: None,
                    max_downloads: NonZeroU64::new(1),
                },
            )
            .await
            .unwrap();
        let http = reqwest::Client::new();
        let first = http.get(&link.url).send().await.unwrap();
        assert_eq!(first.status(), reqwest::StatusCode::OK);
        assert_eq!(first.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(first.headers()[header::REFERRER_POLICY], "no-referrer");
        assert_eq!(first.headers()[header::CONTENT_DISPOSITION], "attachment");
        assert_eq!(first.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert_eq!(first.bytes().await.unwrap(), b"shared-data".as_slice());
        assert_eq!(
            http.get(&link.url).send().await.unwrap().status(),
            reqwest::StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn capability_path_mismatch_is_rejected_before_redemption() {
        let (base, caller) = spawn_service().await;
        let plane = HttpArtifactPlane::new(&base);
        let task_id = uuid::Uuid::now_v7().to_string();
        let issued = plane
            .issue_write_capability(
                &caller,
                &IssueArtifactWriteCapabilityRequest {
                    task_id: task_id.clone(),
                    expires_at: Utc::now() + TimeDelta::minutes(5),
                    max_artifact_count: NonZeroU32::new(1).unwrap(),
                    max_total_bytes: NonZeroU64::new(16).unwrap(),
                },
            )
            .await
            .unwrap();
        let request = RedeemArtifactWriteCapabilityRequest {
            capability_id: issued.capability_id,
            task_id,
            idempotency_key: veoveo_mcp_contract::ArtifactWriteIdempotencyKey::new("output-0")
                .unwrap(),
            artifact: PutArtifactRequest::default(),
        };
        let response = reqwest::Client::new()
            .post(format!(
                "{base}/artifact-write-capabilities/{}/redeem",
                ArtifactWriteCapabilityId::new()
            ))
            .bearer_auth(issued.secret.expose_secret())
            .header(
                "x-artifact-capability-redeem",
                serde_json::to_string(&request).unwrap(),
            )
            .body("bytes")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    }
}
