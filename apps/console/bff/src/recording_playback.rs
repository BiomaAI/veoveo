use anyhow::{Context as _, ensure};
use axum::{
    body::Body,
    extract::{Path, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{
            CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE, HOST,
            X_CONTENT_TYPE_OPTIONS,
        },
    },
    response::{IntoResponse as _, Response},
};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    api::{response_session_headers, upstream_session_for_apps},
};

const MAX_MANIFEST_BYTES: u64 = 8 * 1024 * 1024;
const PLAYBACK_MANIFEST_SCHEMA: &str = "veoveo.io/recording-playback/v2";
const PLAYBACK_SESSION_HEADER: &str = "x-veoveo-playback-session";

#[derive(Debug, Deserialize, Serialize)]
struct PlaybackManifest {
    schema: String,
    recording_id: String,
    application_id: String,
    recording_key: String,
    state: String,
    started_at: String,
    ended_at: Option<String>,
    access: PlaybackAccess,
    archive: Option<PlaybackArchive>,
    live: Option<PlaybackLiveSegment>,
}

#[derive(Debug, Deserialize, Serialize)]
struct PlaybackAccess {
    session_id: String,
    redap_token: String,
    expires_at: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct PlaybackArchive {
    uri: String,
    dataset_id: String,
    segment_id: String,
    revision: String,
    rrd_version: String,
    optimization_profile: String,
    byte_len: u64,
    layer_count: usize,
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
    let mut request = state
        .stream_http
        .get(
            state
                .config
                .recording_playback_url(&recording_id.to_string()),
        )
        .header(HOST, state.config.gateway_host())
        .bearer_auth(&session.session.access_token);
    if let Some(value) = request_headers.get(PLAYBACK_SESSION_HEADER) {
        request = request.header(PLAYBACK_SESSION_HEADER, value);
    }
    let upstream = match request.send().await {
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
    let body = match validated_manifest_bytes(&body, recording_id) {
        Ok(body) => body,
        Err(error) => {
            tracing::error!(%error, %recording_id, "recording manifest contract is invalid");
            return (headers, StatusCode::BAD_GATEWAY).into_response();
        }
    };
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    (headers, body).into_response()
}

pub(crate) async fn live_segment(
    State(state): State<AppState>,
    Path((recording_id, segment_id)): Path<(String, String)>,
    request_headers: HeaderMap,
) -> Response {
    let (Some(recording_id), Some(segment_id)) =
        (parse_uuid_v7(&recording_id), parse_uuid_v7(&segment_id))
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let session = match upstream_session_for_apps(&state, &request_headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let upstream = match state
        .live_http
        .get(
            state
                .config
                .recording_live_segment_url(&recording_id.to_string(), &segment_id.to_string()),
        )
        .header(HOST, state.config.gateway_host())
        .bearer_auth(session.session.access_token)
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            tracing::error!(%error, "console live recording source upstream failed");
            return StatusCode::BAD_GATEWAY.into_response();
        }
    };
    let status = upstream.status();
    let mut headers = HeaderMap::new();
    for name in [
        CONTENT_TYPE,
        CONTENT_LENGTH,
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

fn validated_manifest_bytes(body: &[u8], recording_id: uuid::Uuid) -> anyhow::Result<Vec<u8>> {
    let manifest = serde_json::from_slice::<PlaybackManifest>(body)
        .context("manifest is not valid playback JSON")?;
    ensure!(
        manifest.schema == PLAYBACK_MANIFEST_SCHEMA,
        "manifest schema must be {PLAYBACK_MANIFEST_SCHEMA}"
    );
    ensure!(
        manifest.recording_id == recording_id.to_string(),
        "manifest recording identity does not match its request"
    );
    serde_json::to_vec(&manifest).context("serializing validated playback manifest")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{PLAYBACK_MANIFEST_SCHEMA, validated_manifest_bytes};

    fn manifest(recording_id: uuid::Uuid) -> serde_json::Value {
        json!({
            "schema": PLAYBACK_MANIFEST_SCHEMA,
            "recording_id": recording_id,
            "application_id": "veoveo-uav-sim",
            "recording_key": "inspection-flight",
            "state": "recording",
            "started_at": "2026-07-28T20:00:00Z",
            "ended_at": null,
            "access": {
                "session_id": uuid::Uuid::now_v7(),
                "redap_token": "scoped-token",
                "expires_at": "2026-07-28T20:05:00Z"
            },
            "archive": {
                "uri": "rerun://veoveo.example:443/dataset/00000000000000000000000000000001?segment_id=inspection-flight",
                "dataset_id": "00000000000000000000000000000001",
                "segment_id": "inspection-flight",
                "revision": "sha256:abc",
                "rrd_version": "0.35.0",
                "optimization_profile": "object-store",
                "byte_len": 42,
                "layer_count": 1
            },
            "live": null
        })
    }

    #[test]
    fn manifest_v2_is_canonicalized_after_identity_validation() {
        let recording_id = uuid::Uuid::now_v7();
        let body = serde_json::to_vec(&manifest(recording_id)).unwrap();
        let validated = validated_manifest_bytes(&body, recording_id).unwrap();
        let decoded: serde_json::Value = serde_json::from_slice(&validated).unwrap();
        assert_eq!(decoded["schema"], PLAYBACK_MANIFEST_SCHEMA);
        assert_eq!(decoded["recording_id"], recording_id.to_string());
    }

    #[test]
    fn obsolete_or_cross_recording_manifests_are_rejected() {
        let recording_id = uuid::Uuid::now_v7();
        let mut obsolete = manifest(recording_id);
        obsolete["schema"] = json!("veoveo.io/recording-playback/v1");
        assert!(
            validated_manifest_bytes(&serde_json::to_vec(&obsolete).unwrap(), recording_id)
                .is_err()
        );

        let other_recording_id = uuid::Uuid::now_v7();
        assert!(
            validated_manifest_bytes(
                &serde_json::to_vec(&manifest(other_recording_id)).unwrap(),
                recording_id
            )
            .is_err()
        );
    }
}
