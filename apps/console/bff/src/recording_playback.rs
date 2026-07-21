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
const PLAYBACK_SESSION_HEADER: &str = "x-veoveo-playback-session";
const PLAYBACK_SESSION_TTL_SECONDS: i64 = 300;
const MAX_PLAYBACK_SESSIONS: usize = 1_024;

#[derive(Clone)]
struct PlaybackSession {
    recording_id: uuid::Uuid,
    access_token: String,
    expires_at: i64,
}

#[derive(Clone, Default)]
pub(crate) struct PlaybackSessionStore(Arc<Mutex<HashMap<String, PlaybackSession>>>);

impl PlaybackSessionStore {
    fn renew_or_issue(
        &self,
        requested_session: Option<&str>,
        recording_id: uuid::Uuid,
        access_token: String,
        access_expires_at: i64,
    ) -> anyhow::Result<String> {
        let now = Utc::now().timestamp();
        let expires_at = access_expires_at.min(now.saturating_add(PLAYBACK_SESSION_TTL_SECONDS));
        ensure!(expires_at > now, "playback access token is expired");
        let mut sessions = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("playback session store is poisoned"))?;
        sessions.retain(|_, session| session.expires_at > now);
        if let Some(session_id) = requested_session
            && let Some(session) = sessions
                .get_mut(session_id)
                .filter(|session| session.recording_id == recording_id)
        {
            session.access_token = access_token;
            session.expires_at = expires_at;
            return Ok(session_id.to_owned());
        }

        if sessions.len() >= MAX_PLAYBACK_SESSIONS {
            if let Some(oldest) = sessions
                .iter()
                .min_by_key(|(_, session)| session.expires_at)
                .map(|(session_id, _)| session_id.clone())
            {
                sessions.remove(&oldest);
            }
        }
        let session_id = random_token()?;
        sessions.insert(
            session_id.clone(),
            PlaybackSession {
                recording_id,
                access_token,
                expires_at,
            },
        );
        Ok(session_id)
    }

    fn authorize(&self, session_id: &str, recording_id: uuid::Uuid) -> Option<String> {
        let now = Utc::now().timestamp();
        let mut sessions = self.0.lock().ok()?;
        sessions.retain(|_, session| session.expires_at > now);
        sessions
            .get(session_id)
            .filter(|session| session.recording_id == recording_id)
            .map(|session| session.access_token.clone())
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
    playback_session: Option<String>,
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
    let requested_session = request_headers
        .get(PLAYBACK_SESSION_HEADER)
        .and_then(|value| value.to_str().ok());
    let playback_session = match state.playback_sessions.renew_or_issue(
        requested_session,
        recording_id,
        session.session.access_token.clone(),
        session.session.access_expires_at,
    ) {
        Ok(session_id) => session_id,
        Err(error) => {
            tracing::error!(%error, %recording_id, "playback session issuance failed");
            return (headers, StatusCode::INTERNAL_SERVER_ERROR).into_response();
        }
    };
    manifest.playback_session = Some(playback_session);
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
    Path((recording_id, playback_session, segment_id)): Path<(String, String, String)>,
    request_headers: HeaderMap,
) -> Response {
    let (Some(recording_id), Some(segment_id)) =
        (parse_uuid_v7(&recording_id), parse_uuid_v7(&segment_id))
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(access_token) = state
        .playback_sessions
        .authorize(&playback_session, recording_id)
    else {
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
    Path((recording_id, playback_session, segment_id)): Path<(String, String, String)>,
) -> Response {
    let (Some(recording_id), Some(segment_id)) =
        (parse_uuid_v7(&recording_id), parse_uuid_v7(&segment_id))
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(access_token) = state
        .playback_sessions
        .authorize(&playback_session, recording_id)
    else {
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
    fn playback_session_is_stable_and_scoped_to_one_recording() {
        let sessions = PlaybackSessionStore::default();
        let recording_id = uuid::Uuid::now_v7();
        let other_recording_id = uuid::Uuid::now_v7();
        let session_id = sessions
            .renew_or_issue(
                None,
                recording_id,
                "access-token".to_owned(),
                Utc::now().timestamp() + 60,
            )
            .unwrap();
        assert_eq!(
            sessions.authorize(&session_id, recording_id).as_deref(),
            Some("access-token")
        );
        let renewed_session_id = sessions
            .renew_or_issue(
                Some(&session_id),
                recording_id,
                "renewed-token".to_owned(),
                Utc::now().timestamp() + 120,
            )
            .unwrap();
        assert_eq!(renewed_session_id, session_id);
        assert_eq!(
            sessions.authorize(&session_id, recording_id).as_deref(),
            Some("renewed-token")
        );
        assert_eq!(sessions.authorize(&session_id, other_recording_id), None);
        let other_session_id = sessions
            .renew_or_issue(
                Some(&session_id),
                other_recording_id,
                "other-token".to_owned(),
                Utc::now().timestamp() + 60,
            )
            .unwrap();
        assert_ne!(other_session_id, session_id);
        assert!(
            sessions
                .renew_or_issue(
                    None,
                    recording_id,
                    "expired".to_owned(),
                    Utc::now().timestamp(),
                )
                .is_err()
        );
    }
}
