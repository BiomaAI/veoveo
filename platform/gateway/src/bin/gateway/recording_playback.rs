use std::{collections::BTreeMap, time::Instant};

use axum::{
    body::Body,
    extract::{Extension, Path, State},
    http::{HeaderMap, Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::{TimeDelta, Utc};
use veoveo_mcp_contract::{
    AuditEvent, GatewayAction, GatewayProfileId, McpMethodName, PolicyEffect, PolicyTarget,
    PrincipalAuditAttributes, ResourceUri, ServerSlug, TraceId,
};
use veoveo_mcp_gateway::{
    AuthenticatedSubject, PolicyRequest, build_upstream_http_client, merge_principal_audit_metadata,
};

use crate::runtime::{RecordingPlaybackState, current_catalog};

const RECORDING_SERVER: &str = "recording";
const INTERNAL_PLAYBACK_TOKEN_TTL_SECONDS: i64 = 60;

pub(super) async fn playback_manifest(
    State(state): State<RecordingPlaybackState>,
    Path((profile, recording_id)): Path<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    headers: HeaderMap,
) -> Response {
    proxy_playback(
        state,
        profile,
        recording_id,
        None,
        subject,
        Method::GET,
        headers,
    )
    .await
}

pub(super) async fn playback_segment(
    State(state): State<RecordingPlaybackState>,
    Path((profile, recording_id, segment_id)): Path<(String, String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    method: Method,
    headers: HeaderMap,
) -> Response {
    proxy_playback(
        state,
        profile,
        recording_id,
        Some(segment_id),
        subject,
        method,
        headers,
    )
    .await
}

async fn proxy_playback(
    state: RecordingPlaybackState,
    profile: String,
    recording_id: String,
    segment_id: Option<String>,
    subject: AuthenticatedSubject,
    method: Method,
    headers: HeaderMap,
) -> Response {
    let started_at = Instant::now();
    let Ok(profile) = GatewayProfileId::new(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(recording_uuid) = uuid::Uuid::parse_str(&recording_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if recording_uuid.get_version_num() != 7
        || segment_id.as_ref().is_some_and(|segment| {
            uuid::Uuid::parse_str(segment)
                .map(|id| id.get_version_num() != 7)
                .unwrap_or(true)
        })
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Ok(server) = ServerSlug::new(RECORDING_SERVER) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let Ok(uri) = ResourceUri::new(format!("recording://recordings/{recording_id}")) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let catalog = current_catalog(&state.catalog);
    let Some((_, _, manifest)) = catalog.profile_server(&profile, &server) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let manifest = manifest.clone();
    let trace_id = match TraceId::new(uuid::Uuid::new_v4().to_string()) {
        Ok(value) => value,
        Err(error) => {
            tracing::error!(%error, "failed to create recording playback trace id");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let target = PolicyTarget::Resource {
        server: server.clone(),
        uri,
    };
    let decision = catalog.decide(PolicyRequest {
        principal: &subject.principal,
        profile: &profile,
        action: GatewayAction::ResourcesRead,
        target: &target,
        trace_id: &trace_id,
    });
    let audit = AuditEvent {
        event_id: trace_id.clone(),
        timestamp: decision.evaluated_at,
        trace_id,
        profile: profile.clone(),
        method: McpMethodName::new("resources/read").expect("static MCP method"),
        action: GatewayAction::ResourcesRead,
        target,
        decision: decision.clone(),
        principal: Some(subject.principal.id.clone()),
        principal_attributes: Some(PrincipalAuditAttributes::from(&subject.principal)),
        tenant: subject.principal.tenant.clone(),
        token_issuer: Some(subject.access_token.issuer.clone()),
        latency_ms: u64::try_from(started_at.elapsed().as_millis()).ok(),
        metadata: merge_principal_audit_metadata(
            BTreeMap::from([
                ("recording_id".to_owned(), recording_id.clone()),
                (
                    "segment_id".to_owned(),
                    segment_id.clone().unwrap_or_default(),
                ),
            ]),
            &subject.principal,
        ),
    };
    if let Err(error) = state.gateway_state.record_audit_event(&audit).await {
        tracing::error!(%error, "failed to audit recording playback");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    if decision.effect != PolicyEffect::Allow {
        return StatusCode::FORBIDDEN.into_response();
    }
    let expires_at = std::cmp::min(
        subject.access_token.expires_at,
        Utc::now() + TimeDelta::seconds(INTERNAL_PLAYBACK_TOKEN_TTL_SECONDS),
    );
    let internal_token =
        match state
            .internal_token_issuer
            .issue(profile, server, subject.principal, expires_at)
        {
            Ok(token) => token,
            Err(error) => {
                tracing::error!(%error, "failed to issue recording playback token");
                return StatusCode::UNAUTHORIZED.into_response();
            }
        };
    let client = match build_upstream_http_client(&catalog, &manifest).await {
        Ok(client) => client,
        Err(error) => {
            tracing::error!(?error, "failed to build recording playback client");
            return StatusCode::BAD_GATEWAY.into_response();
        }
    };
    drop(catalog);

    let mut url = match url::Url::parse(manifest.upstream.url.as_str()) {
        Ok(url) => url,
        Err(error) => {
            tracing::error!(%error, "invalid recording upstream URL");
            return StatusCode::BAD_GATEWAY.into_response();
        }
    };
    let path = match &segment_id {
        Some(segment_id) => {
            format!("/recordings/{recording_id}/segments/{segment_id}")
        }
        None => format!("/recordings/{recording_id}/playback"),
    };
    url.set_path(&path);
    url.set_query(None);
    let mut request = client
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
        if let Some(value) = headers.get(&name) {
            request = request.header(name, value);
        }
    }
    let upstream = match request.send().await {
        Ok(response) => response,
        Err(error) => {
            tracing::error!(%error, %recording_id, "recording playback upstream failed");
            return StatusCode::BAD_GATEWAY.into_response();
        }
    };
    proxy_response(upstream)
}

fn proxy_response(upstream: reqwest::Response) -> Response {
    let status = upstream.status();
    let mut headers = HeaderMap::new();
    for name in [
        header::CONTENT_TYPE,
        header::CONTENT_LENGTH,
        header::CONTENT_RANGE,
        header::ACCEPT_RANGES,
        header::CACHE_CONTROL,
        header::ETAG,
        header::LAST_MODIFIED,
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
