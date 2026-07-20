use std::{collections::BTreeMap, time::Instant};

use axum::{
    body::Body,
    extract::{Extension, Path, State},
    http::{HeaderMap, Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::{TimeDelta, Utc};
use veoveo_mcp_contract::{
    ArtifactId, AuditEvent, GatewayAction, GatewayProfileId, McpMethodName, PolicyEffect,
    PolicyTarget, PrincipalAuditAttributes, ResourceUri, TraceId,
};
use veoveo_mcp_gateway::{AuthenticatedSubject, PolicyRequest, merge_principal_audit_metadata};

use crate::runtime::{ArtifactDownloadState, current_catalog, current_http_client};

const INTERNAL_DOWNLOAD_TOKEN_TTL_SECONDS: i64 = 60;

pub(super) async fn download_artifact(
    State(state): State<ArtifactDownloadState>,
    Path((profile, artifact_id)): Path<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    method: Method,
    request_headers: HeaderMap,
) -> Response {
    let started_at = Instant::now();
    let Ok(profile) = GatewayProfileId::new(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(artifact_id) = ArtifactId::parse(artifact_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(artifact_uri) = ResourceUri::new(artifact_id.plane_uri()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let catalog = current_catalog(&state.catalog);
    if catalog.profile(&profile).is_none() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let trace_id = match TraceId::new(uuid::Uuid::new_v4().to_string()) {
        Ok(trace_id) => trace_id,
        Err(error) => {
            tracing::error!("failed to create artifact download trace id: {error}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let target = PolicyTarget::Artifact {
        server: state.artifact_server.clone(),
        artifact_uri,
    };
    let decision = catalog.decide(PolicyRequest {
        principal: &subject.principal,
        profile: &profile,
        action: GatewayAction::ArtifactRead,
        target: &target,
        trace_id: &trace_id,
    });
    if let Err(error) = state
        .gateway_state
        .record_audit_event(&AuditEvent {
            event_id: trace_id.clone(),
            timestamp: decision.evaluated_at,
            trace_id,
            profile: profile.clone(),
            method: match McpMethodName::new("artifact/download") {
                Ok(method) => method,
                Err(error) => {
                    tracing::error!("invalid artifact download audit method: {error}");
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            },
            action: GatewayAction::ArtifactRead,
            target,
            decision: decision.clone(),
            principal: Some(subject.principal.id.clone()),
            principal_attributes: Some(PrincipalAuditAttributes::from(&subject.principal)),
            tenant: subject.principal.tenant.clone(),
            token_issuer: Some(subject.access_token.issuer.clone()),
            latency_ms: u64::try_from(started_at.elapsed().as_millis()).ok(),
            metadata: merge_principal_audit_metadata(
                BTreeMap::from([("artifact_id".to_owned(), artifact_id.to_string())]),
                &subject.principal,
            ),
        })
        .await
    {
        tracing::error!("failed to record artifact download policy decision: {error}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    if decision.effect != PolicyEffect::Allow {
        tracing::warn!(
            profile = %profile,
            principal = %subject.principal.id,
            artifact_id = %artifact_id,
            reason = ?decision.reason,
            "artifact download denied by gateway policy"
        );
        return StatusCode::FORBIDDEN.into_response();
    }
    drop(catalog);

    let expires_at = std::cmp::min(
        subject.access_token.expires_at,
        Utc::now() + TimeDelta::seconds(INTERNAL_DOWNLOAD_TOKEN_TTL_SECONDS),
    );
    let internal_token = match state.internal_token_issuer.issue(
        profile,
        state.artifact_server,
        subject.actor,
        subject.authority,
        expires_at,
    ) {
        Ok(token) => token,
        Err(error) => {
            tracing::error!("failed to issue artifact download token: {error}");
            return StatusCode::UNAUTHORIZED.into_response();
        }
    };
    let url = format!(
        "{}/artifacts/{artifact_id}/download",
        state.artifact_service_url
    );
    let mut request = current_http_client(&state.http)
        .request(method, url)
        .bearer_auth(internal_token.bearer_token);
    for name in [
        header::RANGE,
        header::IF_RANGE,
        header::IF_MATCH,
        header::IF_NONE_MATCH,
        header::IF_MODIFIED_SINCE,
        header::IF_UNMODIFIED_SINCE,
    ] {
        if let Some(value) = request_headers.get(&name) {
            request = request.header(name, value);
        }
    }
    let upstream = match request.send().await {
        Ok(response) => response,
        Err(error) => {
            tracing::error!(artifact_id = %artifact_id, "artifact service download failed: {error}");
            return StatusCode::BAD_GATEWAY.into_response();
        }
    };

    proxy_download_response(upstream)
}

fn proxy_download_response(upstream: reqwest::Response) -> Response {
    let status = upstream.status();
    let mut headers = HeaderMap::new();
    for name in [
        header::LOCATION,
        header::CONTENT_TYPE,
        header::CONTENT_LENGTH,
        header::CONTENT_RANGE,
        header::CONTENT_DISPOSITION,
        header::ACCEPT_RANGES,
        header::CACHE_CONTROL,
        header::ETAG,
        header::LAST_MODIFIED,
        header::REFERRER_POLICY,
        header::X_CONTENT_TYPE_OPTIONS,
        header::CONTENT_SECURITY_POLICY,
    ] {
        if let Some(value) = upstream.headers().get(&name) {
            headers.insert(name, value.clone());
        }
    }
    let mut response = Response::new(Body::from_stream(upstream.bytes_stream()));
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    response
}

#[cfg(test)]
mod tests {
    use axum::{Router, http::HeaderValue, routing::get};

    use super::*;

    #[tokio::test]
    async fn signed_redirect_is_forwarded_without_caching() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new().route(
            "/download",
            get(|| async {
                let mut response = StatusCode::TEMPORARY_REDIRECT.into_response();
                response.headers_mut().insert(
                    header::LOCATION,
                    HeaderValue::from_static("https://objects.example/signed"),
                );
                response
                    .headers_mut()
                    .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
                response
            }),
        );
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let upstream = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap()
            .get(format!("http://{address}/download"))
            .send()
            .await
            .unwrap();

        let response = proxy_download_response(upstream);
        assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(
            response.headers().get(header::LOCATION).unwrap(),
            "https://objects.example/signed"
        );
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "no-store"
        );
    }
}
