use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

use axum::{
    body::{Body, to_bytes},
    extract::{Extension, Path as AxumPath, Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::{TimeDelta, Utc};
use veoveo_mcp_contract::{GatewayAction, PolicyTarget, ServerSlug};
use veoveo_mcp_gateway::{AuthenticatedSubject, build_upstream_http_client};

use crate::{
    admin::admin_profile_id,
    audit::{
        AdminAuthorizationRequest, AdminOperationAuditRecord, AdminOperationFailure,
        AdminOperationStatus, authorize_admin_target_request, internal_error_response,
        record_admin_target_operation_audit,
    },
    runtime::AdminState,
};

const MAX_ADMIN_REQUEST_BYTES: usize = 8 * 1024 * 1024;
const MAX_ADMIN_RESPONSE_BYTES: usize = 32 * 1024 * 1024;
const INTERNAL_ADMIN_TOKEN_TTL_SECONDS: i64 = 60;
const ADMIN_PROXY_TIMEOUT: Duration = Duration::from_secs(60);
const ADMIN_PROXY_METHOD: &str = "admin/server/proxy";

pub(crate) async fn proxy_server_admin(
    State(state): State<AdminState>,
    AxumPath((profile, server, path)): AxumPath<(String, String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    request: Request,
) -> Response {
    let started_at = Instant::now();
    let Some(profile_id) = admin_profile_id(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(server_slug) = ServerSlug::new(server) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !valid_admin_path(&path) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let action = match *request.method() {
        Method::GET | Method::HEAD => GatewayAction::AdminRead,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE => GatewayAction::AdminWrite,
        _ => return StatusCode::METHOD_NOT_ALLOWED.into_response(),
    };
    let target = PolicyTarget::Server {
        server: server_slug.clone(),
    };
    let metadata = BTreeMap::from([
        ("operation".to_owned(), "proxy_server_admin".to_owned()),
        ("server".to_owned(), server_slug.to_string()),
        ("path".to_owned(), path.clone()),
        ("http_method".to_owned(), request.method().to_string()),
    ]);
    let (catalog, profile, subject) = match authorize_admin_target_request(
        &state,
        &profile_id,
        subject,
        AdminAuthorizationRequest {
            action,
            target: target.clone(),
            method: ADMIN_PROXY_METHOD,
            metadata: metadata.clone(),
            started_at,
        },
    )
    .await
    {
        Ok(authorized) => authorized,
        Err(response) => return *response,
    };
    let Some((_, _, server_manifest)) = catalog.profile_server(&profile_id, &server_slug) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let server_manifest = server_manifest.clone();
    let expires_at = std::cmp::min(
        subject.access_token.expires_at,
        Utc::now() + TimeDelta::seconds(INTERNAL_ADMIN_TOKEN_TTL_SECONDS),
    );
    if expires_at <= Utc::now() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let internal_token = match state.internal_token_issuer.issue(
        profile_id,
        server_slug.clone(),
        subject.principal.clone(),
        expires_at,
    ) {
        Ok(token) => token,
        Err(error) => return internal_error_response(error),
    };
    let mut upstream_url = match url::Url::parse(server_manifest.upstream.url.as_str()) {
        Ok(url) => url,
        Err(error) => return internal_error_response(error),
    };
    let mount = server_manifest.mount_path.as_str().trim_end_matches('/');
    upstream_url.set_path(&format!("{mount}/admin/{path}"));
    upstream_url.set_query(request.uri().query());

    let method = request.method().clone();
    let request_headers = request.headers().clone();
    let body = match to_bytes(request.into_body(), MAX_ADMIN_REQUEST_BYTES).await {
        Ok(body) => body,
        Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
    };
    let client = match build_upstream_http_client(&catalog, &server_manifest).await {
        Ok(client) => client,
        Err(error) => return internal_error_response(format!("admin upstream client: {error:?}")),
    };
    let mut builder = client
        .request(method, upstream_url)
        .bearer_auth(internal_token.bearer_token)
        .body(body);
    for name in [
        header::CONTENT_TYPE,
        header::IF_MATCH,
        header::IF_NONE_MATCH,
        header::ACCEPT,
    ] {
        if let Some(value) = request_headers.get(&name) {
            builder = builder.header(name, value);
        }
    }
    if let Some(value) = request_headers.get("idempotency-key") {
        builder = builder.header("idempotency-key", value);
    }
    let upstream = match tokio::time::timeout(ADMIN_PROXY_TIMEOUT, builder.send()).await {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => {
            record_result(
                &state,
                &profile,
                &subject,
                target,
                action,
                started_at,
                AdminOperationStatus::Failed,
                metadata,
            )
            .await;
            tracing::warn!(server = %server_slug, "admin upstream request failed: {error}");
            return StatusCode::BAD_GATEWAY.into_response();
        }
        Err(_) => {
            record_result(
                &state,
                &profile,
                &subject,
                target,
                action,
                started_at,
                AdminOperationStatus::Failed,
                metadata,
            )
            .await;
            return StatusCode::GATEWAY_TIMEOUT.into_response();
        }
    };
    let status = upstream.status();
    let upstream_headers = upstream.headers().clone();
    let response_body = match tokio::time::timeout(ADMIN_PROXY_TIMEOUT, upstream.bytes()).await {
        Ok(Ok(bytes)) if bytes.len() <= MAX_ADMIN_RESPONSE_BYTES => bytes,
        Ok(Ok(_)) => return StatusCode::BAD_GATEWAY.into_response(),
        Ok(Err(error)) => {
            tracing::warn!(server = %server_slug, "reading admin upstream response failed: {error}");
            return StatusCode::BAD_GATEWAY.into_response();
        }
        Err(_) => return StatusCode::GATEWAY_TIMEOUT.into_response(),
    };
    let mut response = Response::new(Body::from(response_body));
    *response.status_mut() = status;
    copy_response_headers(&upstream_headers, response.headers_mut());
    record_result(
        &state,
        &profile,
        &subject,
        target,
        action,
        started_at,
        if status.is_success() {
            AdminOperationStatus::Succeeded
        } else {
            AdminOperationStatus::Rejected
        },
        metadata,
    )
    .await;
    response
}

fn valid_admin_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 2_048
        && path.split('/').all(|segment| {
            !segment.is_empty()
                && segment != "."
                && segment != ".."
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        })
}

fn copy_response_headers(source: &HeaderMap, target: &mut HeaderMap) {
    for name in [
        header::CONTENT_TYPE,
        header::ETAG,
        header::CACHE_CONTROL,
        header::RETRY_AFTER,
    ] {
        if let Some(value) = source.get(&name) {
            target.insert(name, value.clone());
        }
    }
    target.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
}

#[allow(clippy::too_many_arguments)]
async fn record_result(
    state: &AdminState,
    profile: &veoveo_mcp_contract::GatewayProfile,
    subject: &AuthenticatedSubject,
    target: PolicyTarget,
    action: GatewayAction,
    started_at: Instant,
    status: AdminOperationStatus,
    metadata: BTreeMap<String, String>,
) {
    if let Err(error) = record_admin_target_operation_audit(
        state,
        profile,
        subject,
        target,
        AdminOperationAuditRecord {
            action,
            method: ADMIN_PROXY_METHOD,
            started_at,
            status,
            failure: matches!(status, AdminOperationStatus::Failed)
                .then_some(AdminOperationFailure::ServerAdminProxy),
            metadata,
        },
    )
    .await
    {
        tracing::error!("recording server admin proxy result failed: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::valid_admin_path;

    #[test]
    fn admin_path_rejects_traversal_and_encoded_punctuation() {
        assert!(valid_admin_path("sources/source-123"));
        assert!(!valid_admin_path("sources/../secrets"));
        assert!(!valid_admin_path("sources/%2e%2e/secrets"));
        assert!(!valid_admin_path("https://attacker.example"));
    }
}
