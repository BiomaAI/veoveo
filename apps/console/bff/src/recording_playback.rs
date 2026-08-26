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
const PLAYBACK_MANIFEST_SCHEMA: &str = "veoveo.io/recording-playback/v8";
const PLAYBACK_SESSION_HEADER: &str = "x-veoveo-playback-session";
const LIVE_RRD_START_HEADER: &str = "x-veoveo-rerun-live-start";
const LIVE_RRD_STREAM_CONTENT_TYPE: &str =
    "application/vnd.veoveo.rerun.rrd-stream; framing=be32; version=2";
pub(crate) const MANIFEST_PATH: &str = "/console/api/recordings/{recording_id}/playback";
pub(crate) const LIVE_RECORDING_PATH: &str =
    "/console/api/recordings/{recording_id}/live/rrd-stream";
pub(crate) const BLUEPRINT_PATH: &str =
    "/console/api/recordings/{recording_id}/blueprints/{revision}/data.rrd";

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
    blueprint: Option<PlaybackBlueprint>,
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
    transport: PlaybackLiveTransport,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum PlaybackLiveTransport {
    RerunRrdChannelV2,
}

#[derive(Debug, Deserialize, Serialize)]
struct PlaybackBlueprint {
    blueprint_id: String,
    revision: u64,
    sha256: String,
    byte_len: u64,
    map_provider: PlaybackMapProvider,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
enum PlaybackMapProvider {
    None,
    OpenStreetMap,
    Mapbox,
    Mixed,
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

pub(crate) async fn live_recording(
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
    let session_headers = match response_session_headers(&state, &session) {
        Ok(headers) => headers,
        Err(status) => return status.into_response(),
    };
    let Some(start) = live_rrd_start(&request_headers) else {
        return (session_headers, StatusCode::BAD_REQUEST).into_response();
    };
    let upstream = match state
        .live_http
        .get(
            state
                .config
                .recording_live_rrd_stream_url(&recording_id.to_string()),
        )
        .header(HOST, state.config.gateway_host())
        .header(axum::http::header::ACCEPT, LIVE_RRD_STREAM_CONTENT_TYPE)
        .header(LIVE_RRD_START_HEADER, start)
        .bearer_auth(session.session.access_token)
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            tracing::error!(%error, "console live RRD stream upstream failed");
            return (session_headers, StatusCode::BAD_GATEWAY).into_response();
        }
    };
    let status = upstream.status();
    if status.is_success() && !headers_are_live_rrd_stream(upstream.headers()) {
        tracing::error!("console live RRD stream returned an invalid content type");
        return (session_headers, StatusCode::BAD_GATEWAY).into_response();
    }
    let headers = live_rrd_stream_headers(upstream.headers(), session_headers);
    let mut response = Response::new(Body::from_stream(upstream.bytes_stream()));
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    response
}

fn live_rrd_start(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(LIVE_RRD_START_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| matches!(*value, "bootstrap" | "resume-head"))
}

fn headers_are_live_rrd_stream(headers: &HeaderMap) -> bool {
    headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        == Some(LIVE_RRD_STREAM_CONTENT_TYPE)
}

fn live_rrd_stream_headers(upstream: &HeaderMap, mut headers: HeaderMap) -> HeaderMap {
    for name in [
        CONTENT_TYPE,
        X_CONTENT_TYPE_OPTIONS,
        axum::http::HeaderName::from_static("x-accel-buffering"),
    ] {
        if let Some(value) = upstream.get(&name) {
            headers.insert(name, value.clone());
        }
    }
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    headers.insert(
        axum::http::HeaderName::from_static("x-accel-buffering"),
        HeaderValue::from_static("no"),
    );
    headers
}

pub(crate) async fn blueprint(
    State(state): State<AppState>,
    Path((recording_id, revision)): Path<(String, u64)>,
    request_headers: HeaderMap,
) -> Response {
    let Some(recording_id) = parse_uuid_v7(&recording_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if revision == 0 {
        return StatusCode::NOT_FOUND.into_response();
    }
    let session = match upstream_session_for_apps(&state, &request_headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let session_headers = match response_session_headers(&state, &session) {
        Ok(headers) => headers,
        Err(status) => return status.into_response(),
    };
    let upstream = match state
        .live_http
        .get(
            state
                .config
                .recording_blueprint_url(&recording_id.to_string(), revision),
        )
        .header(HOST, state.config.gateway_host())
        .bearer_auth(session.session.access_token)
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            tracing::error!(%error, %recording_id, revision, "console recording Blueprint upstream failed");
            return (session_headers, StatusCode::BAD_GATEWAY).into_response();
        }
    };
    binary_rrd_response(upstream, session_headers)
}

fn binary_rrd_response(upstream: reqwest::Response, session_headers: HeaderMap) -> Response {
    let status = upstream.status();
    let headers = binary_rrd_headers(upstream.headers(), session_headers);
    let mut response = Response::new(Body::from_stream(upstream.bytes_stream()));
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    response
}

fn binary_rrd_headers(upstream: &HeaderMap, mut headers: HeaderMap) -> HeaderMap {
    for name in [
        CONTENT_TYPE,
        CONTENT_LENGTH,
        CONTENT_DISPOSITION,
        X_CONTENT_TYPE_OPTIONS,
    ] {
        if let Some(value) = upstream.get(&name) {
            headers.insert(name, value.clone());
        }
    }
    let buffering = axum::http::HeaderName::from_static("x-accel-buffering");
    if let Some(value) = upstream.get(&buffering) {
        headers.insert(buffering, value.clone());
    }
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    headers
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
    if let Some(blueprint) = &manifest.blueprint {
        ensure!(
            blueprint.revision > 0
                && blueprint.byte_len > 0
                && !blueprint.blueprint_id.trim().is_empty()
                && blueprint.sha256.len() == 64
                && blueprint
                    .sha256
                    .bytes()
                    .all(|value| value.is_ascii_hexdigit()),
            "manifest Blueprint descriptor is invalid"
        );
    }
    serde_json::to_vec(&manifest).context("serializing validated playback manifest")
}

#[cfg(test)]
mod tests {
    use axum::{
        Router,
        http::{HeaderMap, HeaderName, HeaderValue, header},
        routing::get,
    };
    use serde_json::json;

    use super::{
        BLUEPRINT_PATH, LIVE_RECORDING_PATH, MANIFEST_PATH, PLAYBACK_MANIFEST_SCHEMA, blueprint,
        live_recording, live_rrd_start, live_rrd_stream_headers, manifest,
        validated_manifest_bytes,
    };

    fn manifest_value(recording_id: uuid::Uuid) -> serde_json::Value {
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
                "rrd_version": "0.36.3",
                "optimization_profile": "object-store",
                "byte_len": 42,
                "layer_count": 1
            },
            "live": null,
            "blueprint": {
                "blueprint_id": "producer-default",
                "revision": 3,
                "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                "byte_len": 512,
                "map_provider": "mapbox"
            }
        })
    }

    #[test]
    fn manifest_v8_is_canonicalized_after_identity_validation() {
        let recording_id = uuid::Uuid::now_v7();
        let mut manifest = manifest_value(recording_id);
        manifest["live"] = json!({
            "segment_id": uuid::Uuid::now_v7(),
            "ordinal": 0,
            "current_byte_len": 1024,
            "history_seconds": 1,
            "video_preroll_seconds": 2,
            "transport": "rerun_rrd_channel_v2"
        });
        let body = serde_json::to_vec(&manifest).unwrap();
        let validated = validated_manifest_bytes(&body, recording_id).unwrap();
        let decoded: serde_json::Value = serde_json::from_slice(&validated).unwrap();
        assert_eq!(decoded["schema"], PLAYBACK_MANIFEST_SCHEMA);
        assert_eq!(decoded["recording_id"], recording_id.to_string());
        assert_eq!(decoded["blueprint"]["map_provider"], "mapbox");
        assert_eq!(decoded["live"]["transport"], "rerun_rrd_channel_v2");
    }

    #[test]
    fn manifest_rejects_an_unknown_live_transport() {
        let recording_id = uuid::Uuid::now_v7();
        let mut manifest = manifest_value(recording_id);
        manifest["live"] = json!({
            "segment_id": uuid::Uuid::now_v7(),
            "ordinal": 0,
            "current_byte_len": 1024,
            "history_seconds": 1,
            "video_preroll_seconds": 2,
            "transport": "http_rrd"
        });
        assert!(
            validated_manifest_bytes(&serde_json::to_vec(&manifest).unwrap(), recording_id)
                .is_err()
        );
    }

    #[test]
    fn canonical_playback_routes_register_with_axum() {
        let _: Router<crate::AppState> = Router::new()
            .route(MANIFEST_PATH, get(manifest))
            .route(LIVE_RECORDING_PATH, get(live_recording))
            .route(BLUEPRINT_PATH, get(blueprint));
    }

    #[test]
    fn framed_rrd_stream_preserves_rotated_console_session_headers() {
        let mut session = HeaderMap::new();
        session.insert(
            header::SET_COOKIE,
            HeaderValue::from_static("veoveo_console=rotated; HttpOnly; Secure"),
        );
        session.insert(
            HeaderName::from_static("x-veoveo-csrf"),
            HeaderValue::from_static("rotated-csrf"),
        );
        let mut upstream = HeaderMap::new();
        upstream.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static(super::LIVE_RRD_STREAM_CONTENT_TYPE),
        );
        upstream.insert(
            HeaderName::from_static("x-accel-buffering"),
            HeaderValue::from_static("no"),
        );

        let headers = live_rrd_stream_headers(&upstream, session);

        assert_eq!(
            headers.get(header::SET_COOKIE).unwrap(),
            "veoveo_console=rotated; HttpOnly; Secure"
        );
        assert_eq!(headers.get("x-veoveo-csrf").unwrap(), "rotated-csrf");
        assert_eq!(
            headers.get(header::CONTENT_TYPE).unwrap(),
            super::LIVE_RRD_STREAM_CONTENT_TYPE
        );
        assert_eq!(headers.get("x-accel-buffering").unwrap(), "no");
        assert_eq!(
            headers.get(header::CACHE_CONTROL).unwrap(),
            "private, no-store"
        );
    }

    #[test]
    fn live_rrd_start_accepts_only_the_two_channel_states() {
        for value in ["bootstrap", "resume-head"] {
            let mut headers = HeaderMap::new();
            headers.insert(
                HeaderName::from_static(super::LIVE_RRD_START_HEADER),
                HeaderValue::from_str(value).unwrap(),
            );
            assert_eq!(live_rrd_start(&headers), Some(value));
        }
        for value in ["", "resume", "bootstrap, resume-head"] {
            let mut headers = HeaderMap::new();
            headers.insert(
                HeaderName::from_static(super::LIVE_RRD_START_HEADER),
                HeaderValue::from_str(value).unwrap(),
            );
            assert_eq!(live_rrd_start(&headers), None);
        }
        assert_eq!(live_rrd_start(&HeaderMap::new()), None);
    }

    #[test]
    fn obsolete_or_cross_recording_manifests_are_rejected() {
        let recording_id = uuid::Uuid::now_v7();
        let mut obsolete = manifest_value(recording_id);
        obsolete["schema"] = json!("veoveo.io/recording-playback/v1");
        assert!(
            validated_manifest_bytes(&serde_json::to_vec(&obsolete).unwrap(), recording_id)
                .is_err()
        );

        let other_recording_id = uuid::Uuid::now_v7();
        assert!(
            validated_manifest_bytes(
                &serde_json::to_vec(&manifest_value(other_recording_id)).unwrap(),
                recording_id
            )
            .is_err()
        );

        let mut unknown_provider = manifest_value(recording_id);
        unknown_provider["blueprint"]["map_provider"] = json!("silentFallback");
        assert!(
            validated_manifest_bytes(
                &serde_json::to_vec(&unknown_provider).unwrap(),
                recording_id
            )
            .is_err()
        );
    }
}
