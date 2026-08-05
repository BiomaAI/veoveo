use std::{collections::BTreeMap, time::Instant};

use axum::{
    body::Body,
    extract::{Extension, Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::{TimeDelta, Utc};
use veoveo_mcp_contract::{
    AuditEvent, GatewayAction, GatewayProfileId, McpMethodName, PolicyEffect, PolicyTarget,
    PrincipalAuditAttributes, ResourceUri, ServerSlug, TraceId,
};
use veoveo_mcp_gateway::{AuthenticatedSubject, PolicyRequest, merge_principal_audit_metadata};

use crate::runtime::{RecordingPlaybackState, current_catalog};

const RECORDING_SERVER: &str = "recording";
const INTERNAL_PLAYBACK_TOKEN_TTL_SECONDS: i64 = 60;
const PLAYBACK_SESSION_HEADER: &str = "x-veoveo-playback-session";
const LIVE_RRD_START_HEADER: &str = "x-veoveo-rerun-live-start";

#[derive(Clone, Debug)]
enum PlaybackSource {
    Manifest,
    LiveRrdStream,
    Blueprint(u64),
}

impl PlaybackSource {
    fn mode(&self) -> &'static str {
        match self {
            Self::Manifest => "manifest",
            Self::LiveRrdStream => "live-rrd-stream",
            Self::Blueprint(_) => "blueprint",
        }
    }

    fn upstream_path(&self, recording_id: &str) -> String {
        match self {
            Self::Manifest => format!("/recordings/{recording_id}/playback"),
            Self::LiveRrdStream => format!("/recordings/{recording_id}/live/rrd-stream"),
            Self::Blueprint(revision) => {
                format!("/recordings/{recording_id}/blueprints/{revision}/data.rrd")
            }
        }
    }
}

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
        PlaybackSource::Manifest,
        subject,
        headers,
    )
    .await
}

pub(super) async fn playback_live_recording(
    State(state): State<RecordingPlaybackState>,
    Path((profile, recording_id)): Path<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    headers: HeaderMap,
) -> Response {
    proxy_playback(
        state,
        profile,
        recording_id,
        PlaybackSource::LiveRrdStream,
        subject,
        headers,
    )
    .await
}

pub(super) async fn playback_blueprint(
    State(state): State<RecordingPlaybackState>,
    Path((profile, recording_id, revision)): Path<(String, String, u64)>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    if revision == 0 {
        return StatusCode::NOT_FOUND.into_response();
    }
    proxy_playback(
        state,
        profile,
        recording_id,
        PlaybackSource::Blueprint(revision),
        subject,
        HeaderMap::new(),
    )
    .await
}

async fn proxy_playback(
    state: RecordingPlaybackState,
    profile: String,
    recording_id: String,
    source: PlaybackSource,
    subject: AuthenticatedSubject,
    headers: HeaderMap,
) -> Response {
    let started_at = Instant::now();
    let Ok(profile) = GatewayProfileId::new(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(recording_uuid) = uuid::Uuid::parse_str(&recording_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if recording_uuid.get_version_num() != 7 {
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
                ("playback_mode".to_owned(), source.mode().to_owned()),
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
    let internal_token = match state.internal_token_issuer.issue(
        profile,
        server,
        subject.actor,
        subject.authority,
        expires_at,
    ) {
        Ok(token) => token,
        Err(error) => {
            tracing::error!(%error, "failed to issue recording playback token");
            return StatusCode::UNAUTHORIZED.into_response();
        }
    };
    let client = match state.upstream_http.client(&catalog, &manifest).await {
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
    let path = source.upstream_path(&recording_id);
    url.set_path(&path);
    url.set_query(None);
    let request = forwarded_request_headers(
        client.get(url).bearer_auth(internal_token.bearer_token),
        &source,
        &headers,
    );
    let upstream = match request.send().await {
        Ok(response) => response,
        Err(error) => {
            tracing::error!(%error, %recording_id, "recording playback upstream failed");
            return StatusCode::BAD_GATEWAY.into_response();
        }
    };
    proxy_response(upstream)
}

fn forwarded_request_headers(
    mut request: reqwest::RequestBuilder,
    source: &PlaybackSource,
    headers: &HeaderMap,
) -> reqwest::RequestBuilder {
    if matches!(source, PlaybackSource::Manifest)
        && let Some(value) = headers.get(PLAYBACK_SESSION_HEADER)
    {
        request = request.header(PLAYBACK_SESSION_HEADER, value);
    }
    if matches!(source, PlaybackSource::LiveRrdStream)
        && let Some(value) = headers.get(header::ACCEPT)
    {
        request = request.header(header::ACCEPT, value);
    }
    if matches!(source, PlaybackSource::LiveRrdStream)
        && let Some(value) = headers.get(LIVE_RRD_START_HEADER)
    {
        request = request.header(LIVE_RRD_START_HEADER, value);
    }
    request
}

fn proxy_response(upstream: reqwest::Response) -> Response {
    let status = upstream.status();
    let mut headers = HeaderMap::new();
    for name in [
        axum::http::header::CONTENT_TYPE,
        axum::http::header::CONTENT_LENGTH,
        axum::http::header::CACHE_CONTROL,
        axum::http::header::CONTENT_DISPOSITION,
        axum::http::header::X_CONTENT_TYPE_OPTIONS,
        axum::http::HeaderName::from_static("grpc-encoding"),
        axum::http::HeaderName::from_static("grpc-accept-encoding"),
    ] {
        if let Some(value) = upstream.headers().get(&name) {
            headers.insert(name, value.clone());
        }
    }
    if let Some(value) = upstream
        .headers()
        .get(axum::http::HeaderName::from_static("x-accel-buffering"))
    {
        headers.insert(
            axum::http::HeaderName::from_static("x-accel-buffering"),
            value.clone(),
        );
    }
    let mut response = Response::new(Body::from_stream(upstream.bytes_stream()));
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_playback_uses_the_incremental_rrd_stream_contract() {
        let source = PlaybackSource::LiveRrdStream;

        assert_eq!(source.mode(), "live-rrd-stream");
        assert_eq!(
            source.upstream_path("019faa9f-acc8-7400-ba67-a9b022da1f63"),
            "/recordings/019faa9f-acc8-7400-ba67-a9b022da1f63/live/rrd-stream"
        );
    }

    #[test]
    fn live_playback_forwards_the_exact_channel_start_state() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ACCEPT,
            "application/vnd.veoveo.rerun.rrd-stream; framing=be32; version=2"
                .parse()
                .unwrap(),
        );
        headers.insert(LIVE_RRD_START_HEADER, "resume-head".parse().unwrap());
        let request = forwarded_request_headers(
            reqwest::Client::new().get("http://recording.example/live"),
            &PlaybackSource::LiveRrdStream,
            &headers,
        )
        .build()
        .unwrap();

        assert_eq!(
            request.headers().get(header::ACCEPT),
            headers.get(header::ACCEPT)
        );
        assert_eq!(
            request.headers().get(LIVE_RRD_START_HEADER),
            headers.get(LIVE_RRD_START_HEADER)
        );
        assert!(request.headers().get(PLAYBACK_SESSION_HEADER).is_none());
    }
}
