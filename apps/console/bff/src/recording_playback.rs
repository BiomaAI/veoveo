use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use anyhow::ensure;
use axum::{
    body::Body,
    extract::{Path, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{
            ACCEPT_RANGES, CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE,
            CONTENT_TYPE, ETAG, HOST, IF_MATCH, IF_MODIFIED_SINCE, IF_NONE_MATCH, IF_RANGE,
            IF_UNMODIFIED_SINCE, LAST_MODIFIED, RANGE, X_CONTENT_TYPE_OPTIONS,
        },
    },
    response::{IntoResponse as _, Response},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    api::{response_session_headers, upstream_session_for_apps},
    session::random_token,
};

const MAX_MANIFEST_BYTES: u64 = 8 * 1024 * 1024;
const TICKET_TTL_SECONDS: i64 = 300;
const MAX_TICKETS: usize = 1_024;

#[derive(Clone)]
struct PlaybackGrant {
    recording_id: uuid::Uuid,
    access_token: String,
    expires_at: i64,
}

#[derive(Clone, Default)]
pub(crate) struct PlaybackTicketStore(Arc<Mutex<HashMap<String, PlaybackGrant>>>);

impl PlaybackTicketStore {
    fn issue(
        &self,
        recording_id: uuid::Uuid,
        access_token: String,
        access_expires_at: i64,
    ) -> anyhow::Result<String> {
        let now = Utc::now().timestamp();
        let expires_at = access_expires_at.min(now.saturating_add(TICKET_TTL_SECONDS));
        ensure!(expires_at > now, "playback access token is expired");
        let ticket = random_token()?;
        let mut grants = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("playback ticket store is poisoned"))?;
        grants.retain(|_, grant| grant.expires_at > now);
        if grants.len() >= MAX_TICKETS {
            if let Some(oldest) = grants
                .iter()
                .min_by_key(|(_, grant)| grant.expires_at)
                .map(|(ticket, _)| ticket.clone())
            {
                grants.remove(&oldest);
            }
        }
        grants.insert(
            ticket.clone(),
            PlaybackGrant {
                recording_id,
                access_token,
                expires_at,
            },
        );
        Ok(ticket)
    }

    fn authorize(&self, ticket: &str, recording_id: uuid::Uuid) -> Option<String> {
        let now = Utc::now().timestamp();
        let mut grants = self.0.lock().ok()?;
        grants.retain(|_, grant| grant.expires_at > now);
        grants
            .get(ticket)
            .filter(|grant| grant.recording_id == recording_id)
            .map(|grant| grant.access_token.clone())
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct PlaybackManifest {
    recording_id: String,
    application_id: String,
    recording_key: String,
    state: String,
    started_at: String,
    ended_at: Option<String>,
    archive: PlaybackArchive,
    live: Option<PlaybackLiveSegment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    playback_ticket: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct PlaybackArchive {
    rrd_version: String,
    optimization_profile: String,
    segments: Vec<PlaybackSegment>,
}

#[derive(Debug, Deserialize, Serialize)]
struct PlaybackSegment {
    segment_id: String,
    ordinal: i64,
    byte_len: u64,
    sha256: String,
    started_at: Option<String>,
    ended_at: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct PlaybackLiveSegment {
    segment_id: String,
    ordinal: i64,
    current_byte_len: u64,
    history_seconds: u64,
    video_preroll_seconds: u64,
}

pub(crate) async fn manifest(
    State(state): State<AppState>,
    Path(recording_id): Path<String>,
    request_headers: HeaderMap,
) -> Response {
    let Some(recording_id) = parse_uuid_v7(&recording_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let session = match upstream_session_for_apps(&state, &request_headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let mut headers = match response_session_headers(&state, &session) {
        Ok(headers) => headers,
        Err(status) => return status.into_response(),
    };
    let upstream = match state
        .stream_http
        .get(
            state
                .config
                .recording_playback_url(&recording_id.to_string()),
        )
        .header(HOST, state.config.gateway_host())
        .bearer_auth(&session.session.access_token)
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            tracing::error!(%error, %recording_id, "console recording manifest upstream failed");
            return (headers, StatusCode::BAD_GATEWAY).into_response();
        }
    };
    let status = upstream.status();
    if !status.is_success() {
        return (headers, status).into_response();
    }
    if upstream
        .content_length()
        .is_some_and(|length| length > MAX_MANIFEST_BYTES)
    {
        return (headers, StatusCode::BAD_GATEWAY).into_response();
    }
    let body = match upstream.bytes().await {
        Ok(body) if body.len() as u64 <= MAX_MANIFEST_BYTES => body,
        _ => return (headers, StatusCode::BAD_GATEWAY).into_response(),
    };
    let mut manifest = match serde_json::from_slice::<PlaybackManifest>(&body) {
        Ok(manifest) => manifest,
        Err(error) => {
            tracing::error!(%error, %recording_id, "recording manifest contract is invalid");
            return (headers, StatusCode::BAD_GATEWAY).into_response();
        }
    };
    if manifest.recording_id != recording_id.to_string() {
        return (headers, StatusCode::BAD_GATEWAY).into_response();
    }
    let ticket = match state.playback_tickets.issue(
        recording_id,
        session.session.access_token.clone(),
        session.session.access_expires_at,
    ) {
        Ok(ticket) => ticket,
        Err(error) => {
            tracing::error!(%error, %recording_id, "playback ticket issuance failed");
            return (headers, StatusCode::INTERNAL_SERVER_ERROR).into_response();
        }
    };
    manifest.playback_ticket = Some(ticket);
    let body = match serde_json::to_vec(&manifest) {
        Ok(body) => body,
        Err(error) => {
            tracing::error!(%error, %recording_id, "recording manifest serialization failed");
            return (headers, StatusCode::INTERNAL_SERVER_ERROR).into_response();
        }
    };
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    (headers, body).into_response()
}

pub(crate) async fn segment(
    State(state): State<AppState>,
    Path((recording_id, ticket, segment_id)): Path<(String, String, String)>,
    request_headers: HeaderMap,
) -> Response {
    let (Some(recording_id), Some(segment_id)) =
        (parse_uuid_v7(&recording_id), parse_uuid_v7(&segment_id))
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(access_token) = state.playback_tickets.authorize(&ticket, recording_id) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    proxy_source(
        &state,
        access_token,
        state
            .config
            .recording_segment_url(&recording_id.to_string(), &segment_id.to_string()),
        false,
        &request_headers,
    )
    .await
}

pub(crate) async fn live_segment(
    State(state): State<AppState>,
    Path((recording_id, ticket, segment_id)): Path<(String, String, String)>,
) -> Response {
    let (Some(recording_id), Some(segment_id)) =
        (parse_uuid_v7(&recording_id), parse_uuid_v7(&segment_id))
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(access_token) = state.playback_tickets.authorize(&ticket, recording_id) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    proxy_source(
        &state,
        access_token,
        state
            .config
            .recording_live_segment_url(&recording_id.to_string(), &segment_id.to_string()),
        true,
        &HeaderMap::new(),
    )
    .await
}

async fn proxy_source(
    state: &AppState,
    access_token: String,
    url: url::Url,
    live: bool,
    request_headers: &HeaderMap,
) -> Response {
    let client = if live {
        &state.live_http
    } else {
        &state.stream_http
    };
    let mut request = client
        .get(url)
        .header(HOST, state.config.gateway_host())
        .bearer_auth(access_token);
    for name in [
        RANGE,
        IF_RANGE,
        IF_MATCH,
        IF_NONE_MATCH,
        IF_MODIFIED_SINCE,
        IF_UNMODIFIED_SINCE,
    ] {
        if let Some(value) = request_headers.get(&name) {
            request = request.header(name, value);
        }
    }
    let upstream = match request.send().await {
        Ok(response) => response,
        Err(error) => {
            tracing::error!(%error, live, "console recording source upstream failed");
            return StatusCode::BAD_GATEWAY.into_response();
        }
    };
    let status = upstream.status();
    let mut headers = HeaderMap::new();
    for name in [
        CONTENT_TYPE,
        CONTENT_LENGTH,
        CONTENT_RANGE,
        ACCEPT_RANGES,
        ETAG,
        LAST_MODIFIED,
        CACHE_CONTROL,
        CONTENT_DISPOSITION,
        X_CONTENT_TYPE_OPTIONS,
    ] {
        if let Some(value) = upstream.headers().get(&name) {
            headers.insert(name, value.clone());
        }
    }
    let buffering = axum::http::HeaderName::from_static("x-accel-buffering");
    if let Some(value) = upstream.headers().get(&buffering) {
        headers.insert(buffering, value.clone());
    }
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    let mut response = Response::new(Body::from_stream(upstream.bytes_stream()));
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    response
}

fn parse_uuid_v7(value: &str) -> Option<uuid::Uuid> {
    let id = uuid::Uuid::parse_str(value).ok()?;
    (id.get_version_num() == 7).then_some(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticket_is_scoped_to_recording_and_expiry() {
        let tickets = PlaybackTicketStore::default();
        let recording_id = uuid::Uuid::now_v7();
        let other_recording_id = uuid::Uuid::now_v7();
        let ticket = tickets
            .issue(
                recording_id,
                "access-token".to_owned(),
                Utc::now().timestamp() + 60,
            )
            .unwrap();
        assert_eq!(
            tickets.authorize(&ticket, recording_id).as_deref(),
            Some("access-token")
        );
        assert_eq!(tickets.authorize(&ticket, other_recording_id), None);
        assert!(
            tickets
                .issue(recording_id, "expired".to_owned(), Utc::now().timestamp(),)
                .is_err()
        );
    }
}
