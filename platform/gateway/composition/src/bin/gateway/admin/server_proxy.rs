use std::time::{Duration, Instant};
use veoveo_gateway_contract::GatewayAction;
use veoveo_mcp_contract::audit::AdministrativeOperation;

use axum::{
    body::{Body, to_bytes},
    extract::{Extension, Path as AxumPath, Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::{TimeDelta, Utc};
use veoveo_mcp_contract::{PolicyTarget, ServerSlug};
use veoveo_mcp_gateway::AuthenticatedSubject;

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
    let Ok(server_slug) = ServerSlug::parse(server) else {
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
    let (catalog, profile, subject) = match authorize_admin_target_request(
        &state,
        &profile_id,
        subject,
        AdminAuthorizationRequest {
            audit_target: None,
            action: action.into(),
            target: target.clone(),
            operation: AdministrativeOperation::ServerProxy,
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
        subject.actor.clone(),
        subject.authority.clone(),
        Some(subject.request_context()),
        expires_at,
    ) {
        Ok(token) => token,
        Err(error) => return internal_error_response(error),
    };
    let upstream_url = match upstream_admin_url(
        server_manifest.upstream.url.as_str(),
        &path,
        request.uri().query(),
    ) {
        Ok(url) => url,
        Err(error) => return internal_error_response(error),
    };

    let method = request.method().clone();
    let request_headers = request.headers().clone();
    let body = match to_bytes(request.into_body(), MAX_ADMIN_REQUEST_BYTES).await {
        Ok(body) => body,
        Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
    };
    let client = match state.upstream_http.client(&catalog, &server_manifest).await {
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
    )
    .await;
    response
}

/// Admin and MCP routes share the upstream mount, independent of its public mount.
fn upstream_admin_url(endpoint: &str, path: &str, query: Option<&str>) -> anyhow::Result<url::Url> {
    anyhow::ensure!(valid_admin_path(path), "invalid server admin path");
    let mut url = url::Url::parse(endpoint)?;
    anyhow::ensure!(
        url.path_segments()
            .and_then(|mut segments| segments.next_back())
            == Some("mcp"),
        "server admin proxy requires an upstream MCP endpoint ending in /mcp"
    );
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("server admin upstream must have a hierarchical URL"))?
        .pop()
        .push("admin")
        .extend(path.split('/'));
    url.set_query(query);
    url.set_fragment(None);
    Ok(url)
}

fn valid_admin_path(path: &str) -> bool {
    !path.is_empty() && path.len() <= 2_048 && path.split('/').all(valid_admin_path_segment)
}

fn valid_admin_path_segment(segment: &str) -> bool {
    segment
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphanumeric)
        && segment
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
        && segment
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
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
) {
    if let Err(error) = record_admin_target_operation_audit(
        state,
        profile,
        subject,
        target,
        AdminOperationAuditRecord {
            audit_target: None,
            action: action.into(),
            operation: AdministrativeOperation::ServerProxy,
            started_at,
            status,
            failure: matches!(status, AdminOperationStatus::Failed)
                .then_some(AdminOperationFailure::ServerAdminProxy),
        },
    )
    .await
    {
        tracing::error!("recording server admin proxy result failed: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::{upstream_admin_url, valid_admin_path};

    #[test]
    fn admin_routes_follow_the_internal_mount_including_root() {
        for (endpoint, expected) in [
            (
                "http://recording:8796/mcp",
                "http://recording:8796/admin/docs/llms.txt",
            ),
            (
                "http://frames:8783/frames/mcp",
                "http://frames:8783/frames/admin/docs/llms.txt",
            ),
            (
                "https://internal.example/services/custom/mcp",
                "https://internal.example/services/custom/admin/docs/llms.txt",
            ),
        ] {
            let url = upstream_admin_url(endpoint, "docs/llms.txt", Some("cursor=a%2Fb&limit=10"))
                .unwrap();
            assert_eq!(url.as_str(), format!("{expected}?cursor=a%2Fb&limit=10"));
        }
        assert!(upstream_admin_url("http://server/custom", "docs/agents", None).is_err());
        assert!(upstream_admin_url("http://server/mcp", "../secrets", None).is_err());
    }

    #[test]
    fn admin_path_rejects_traversal_and_encoded_punctuation() {
        assert!(valid_admin_path("sources/source-123"));
        assert!(valid_admin_path("docs/llms.txt"));
        assert!(!valid_admin_path("sources/../secrets"));
        assert!(!valid_admin_path("docs/.hidden"));
        assert!(!valid_admin_path("docs/trailing."));
        assert!(!valid_admin_path("sources/%2e%2e/secrets"));
        assert!(!valid_admin_path("https://attacker.example"));
    }
}
