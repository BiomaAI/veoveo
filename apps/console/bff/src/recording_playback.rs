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
use veoveo_recording_mcp::contract::{
    MAX_PROJECTION_BYTES, PlaybackManifest, RecordingId, RecordingProjectionId,
};

use crate::{
    AppState,
    api::{response_session_headers, upstream_session},
};

const MAX_MANIFEST_BYTES: u64 = 8 * 1024 * 1024;
const ARROW_STREAM_CONTENT_TYPE: &str = "application/vnd.apache.arrow.stream";
const RECORDING_GRANT_HEADER: &str = "x-veoveo-recording-grant";
const LIVE_RRD_START_HEADER: &str = "x-veoveo-rerun-live-start";
const LIVE_RRD_STREAM_CONTENT_TYPE: &str =
    "application/vnd.veoveo.rerun.rrd-stream; framing=be32; version=2";
pub(crate) const MANIFEST_PATH: &str = "/console/api/recordings/{recording_id}/playback";
pub(crate) const LIVE_RECORDING_PATH: &str =
    "/console/api/recordings/{recording_id}/live/rrd-stream";
pub(crate) const BLUEPRINT_PATH: &str =
    "/console/api/recordings/{recording_id}/blueprints/{revision}/data.rrd";
pub(crate) const PROJECTION_PATH: &str =
    "/console/api/recordings/{recording_id}/projections/{projection_id}/data.arrow";

pub(crate) async fn manifest(
    State(state): State<AppState>,
    Path(recording_id): Path<String>,
    request_headers: HeaderMap,
) -> Response {
    let Ok(recording_id) = RecordingId::parse(&recording_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let session = match upstream_session(&state, &request_headers).await {
        Ok(session) => session,
        Err(response) => return *response,
    };
    let mut headers = match response_session_headers(&state, &session) {
        Ok(headers) => headers,
        Err(status) => return status.into_response(),
    };
    let mut request = state
        .stream_http
        .get(state.config.recording_playback_url(recording_id))
        .header(HOST, state.config.gateway_host())
        .bearer_auth(&session.session.access_token);
    if let Some(value) = request_headers.get(RECORDING_GRANT_HEADER) {
        request = request.header(RECORDING_GRANT_HEADER, value);
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
    let Ok(recording_id) = RecordingId::parse(&recording_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let session = match upstream_session(&state, &request_headers).await {
        Ok(session) => session,
        Err(response) => return *response,
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
        .get(state.config.recording_live_rrd_stream_url(recording_id))
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
    let Ok(recording_id) = RecordingId::parse(&recording_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if revision == 0 {
        return StatusCode::NOT_FOUND.into_response();
    }
    let session = match upstream_session(&state, &request_headers).await {
        Ok(session) => session,
        Err(response) => return *response,
    };
    let session_headers = match response_session_headers(&state, &session) {
        Ok(headers) => headers,
        Err(status) => return status.into_response(),
    };
    let upstream = match state
        .live_http
        .get(state.config.recording_blueprint_url(recording_id, revision))
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

pub(crate) async fn projection(
    State(state): State<AppState>,
    Path((recording_id, projection_id)): Path<(String, String)>,
    request_headers: HeaderMap,
) -> Response {
    let Ok(recording_id) = RecordingId::parse(&recording_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(projection_id) = RecordingProjectionId::parse(&projection_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let session = match upstream_session(&state, &request_headers).await {
        Ok(session) => session,
        Err(response) => return *response,
    };
    let session_headers = match response_session_headers(&state, &session) {
        Ok(headers) => headers,
        Err(status) => return status.into_response(),
    };
    let upstream = match state
        .stream_http
        .get(
            state
                .config
                .recording_projection_url(recording_id, projection_id),
        )
        .header(HOST, state.config.gateway_host())
        .bearer_auth(session.session.access_token)
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            tracing::error!(%error, %recording_id, %projection_id, "console recording projection upstream failed");
            return (session_headers, StatusCode::BAD_GATEWAY).into_response();
        }
    };
    if upstream.status().is_success() && !headers_are_bounded_arrow(upstream.headers()) {
        tracing::error!(%recording_id, %projection_id, "console recording projection returned an invalid Arrow contract");
        return (session_headers, StatusCode::BAD_GATEWAY).into_response();
    }
    let status = upstream.status();
    let headers = binary_projection_headers(upstream.headers(), session_headers);
    let mut response = Response::new(Body::from_stream(upstream.bytes_stream()));
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    response
}

fn headers_are_bounded_arrow(headers: &HeaderMap) -> bool {
    headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        == Some(ARROW_STREAM_CONTENT_TYPE)
        && headers
            .get(CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .is_some_and(|value| value <= MAX_PROJECTION_BYTES)
        && headers
            .get("x-veoveo-payload-sha256")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
}

fn binary_projection_headers(upstream: &HeaderMap, mut headers: HeaderMap) -> HeaderMap {
    for name in [
        CONTENT_TYPE,
        CONTENT_LENGTH,
        CONTENT_DISPOSITION,
        X_CONTENT_TYPE_OPTIONS,
        axum::http::HeaderName::from_static("x-veoveo-payload-sha256"),
    ] {
        if let Some(value) = upstream.get(&name) {
            headers.insert(name, value.clone());
        }
    }
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    headers
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

fn validated_manifest_bytes(body: &[u8], recording_id: RecordingId) -> anyhow::Result<Vec<u8>> {
    let manifest = serde_json::from_slice::<PlaybackManifest>(body)
        .context("manifest is not valid playback JSON")?;
    ensure!(
        manifest.recording_segment_id == recording_id,
        "manifest recording identity does not match its request"
    );
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
    use veoveo_recording_mcp::contract::{
        PLAYBACK_MANIFEST_SCHEMA, PlaybackArchiveUri, RecordingDatasetId, RecordingRedapOrigin,
    };

    use super::{
        BLUEPRINT_PATH, LIVE_RECORDING_PATH, MANIFEST_PATH, PROJECTION_PATH, RecordingId,
        blueprint, live_recording, live_rrd_start, live_rrd_stream_headers, manifest, projection,
        validated_manifest_bytes,
    };

    #[test]
    fn playback_browser_fixture_comes_from_current_owner_builders() {
        use veoveo_recording_mcp::contract::{
            PlaybackAccess, PlaybackArchive, PlaybackLiveReceiver, PlaybackLiveTransport,
            PlaybackManifestBuilder, PlaybackManifestSchema, RecordingReadGrantId, RecordingState,
        };
        let dataset = RecordingDatasetId::parse("019fa000-0000-7000-8000-000000000002").unwrap();
        let recording = RecordingId::parse("019fa000-0000-7000-8000-000000000001").unwrap();
        let origin = RecordingRedapOrigin::from_http("https://archive.example").unwrap();
        let live = PlaybackManifestBuilder {
            schema: PlaybackManifestSchema::V11,
            dataset_id: dataset,
            recording_segment_id: recording,
            application_id: "fixture".into(),
            recording_key: "rollover".into(),
            state: RecordingState::Live,
            started_at: "2026-10-03T00:00:00Z".parse().unwrap(),
            ended_at: None,
            catalog_revision: "r1".into(),
            access: PlaybackAccess {
                grant_id: RecordingReadGrantId::parse("019fa000-0000-7000-8000-000000000003")
                    .unwrap(),
                redap_token: "fixture-0".into(),
                expires_at: "2099-10-03T00:00:00Z".parse().unwrap(),
            },
            archive: None,
            live: Some(PlaybackLiveReceiver {
                history_seconds: 1,
                video_preroll_seconds: 2,
                transport: PlaybackLiveTransport::RerunRrdChannelV2,
            }),
            blueprint: None,
        };
        let mut sealed = live.clone();
        sealed.state = RecordingState::Sealed;
        sealed.live = None;
        sealed.ended_at = Some("2026-10-03T00:01:00Z".parse().unwrap());
        sealed.catalog_revision = "r4".into();
        sealed.archive = Some(PlaybackArchive {
            uri: PlaybackArchiveUri::new(&origin, dataset, recording),
            dataset_id: dataset,
            recording_segment_id: recording,
            catalog_revision: sealed.catalog_revision.clone(),
            rrd_version: "0.38.1".into(),
            optimization_profile: "object-store".into(),
            byte_len: 256,
            layer_count: 2,
        });
        let sealed = sealed.build().unwrap();
        let base = sealed.archive.as_ref().unwrap().uri.as_str();
        let mut refused = Vec::new();
        for change in [
            "dataset",
            "recording",
            "duplicate",
            "unsupported",
            "escaped_key",
            "escaped_value",
            "encoded_path",
            "port_zero",
            "unspecified_v4",
            "unspecified_v6",
            "loopback_domain",
            "loopback_v4",
            "loopback_v6",
            "loopback_http",
        ] {
            let mut uri = url::Url::parse(base).unwrap();
            match change {
                "dataset" => uri.set_path("/dataset/019FA000000070008000000000000004"),
                "recording" => {
                    uri.query_pairs_mut()
                        .clear()
                        .append_pair("segment_id", "019fa000-0000-7000-8000-000000000004");
                }
                "duplicate" => {
                    uri.query_pairs_mut()
                        .append_pair("segment_id", &recording.to_string());
                }
                "unsupported" => {
                    uri.query_pairs_mut().append_pair("extra", "1");
                }
                "escaped_key" => {
                    let query = uri.query().unwrap().replace("segment_id", "%73egment_id");
                    uri.set_query(Some(&query));
                }
                "escaped_value" => {
                    let query = uri.query().unwrap().replacen("=0", "=%30", 1);
                    uri.set_query(Some(&query));
                }
                "encoded_path" => uri.set_path(&uri.path().replace("dataset", "%64ataset")),
                "port_zero" => {
                    uri.set_port(Some(0)).unwrap();
                }
                "unspecified_v4" => {
                    uri.set_host(Some("0.0.0.0")).unwrap();
                }
                "unspecified_v6" => {
                    uri.set_host(Some("[::]")).unwrap();
                }
                "loopback_domain" => {
                    uri.set_host(Some("localhost")).unwrap();
                    uri.set_port(Some(443)).unwrap();
                }
                "loopback_v4" => {
                    uri.set_host(Some("127.0.0.1")).unwrap();
                    uri.set_port(Some(443)).unwrap();
                }
                "loopback_v6" => {
                    uri.set_host(Some("[::1]")).unwrap();
                    uri.set_port(Some(443)).unwrap();
                }
                _ => {
                    uri.set_scheme("rerun+http").unwrap();
                    uri.set_host(Some("localhost")).unwrap();
                    uri.set_port(Some(80)).unwrap();
                }
            }
            let mut receiver = serde_json::to_value(&sealed).unwrap();
            *receiver.pointer_mut("/archive/uri").unwrap() = json!(uri.as_str());
            assert!(
                validated_manifest_bytes(&serde_json::to_vec(&receiver).unwrap(), recording)
                    .is_err(),
                "{change}"
            );
            refused.push(uri.to_string());
        }
        let produced =
            json!({"live": live.build().unwrap(), "sealed": sealed, "archiveRefusals": refused});
        for value in [produced["live"].clone(), produced["sealed"].clone()] {
            validated_manifest_bytes(&serde_json::to_vec(&value).unwrap(), recording).unwrap();
        }
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../web/testdata/recording-playback.json");
        if std::env::var_os("UPDATE_RECORDING_APP_FIXTURES").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(
                &path,
                format!("{}\n", serde_json::to_string_pretty(&produced).unwrap()),
            )
            .unwrap();
        }
        let captured: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(produced, captured);
    }

    fn manifest_value(recording_id: RecordingId) -> serde_json::Value {
        let dataset_id = RecordingDatasetId::new();
        let origin = RecordingRedapOrigin::from_http("https://veoveo.example").unwrap();
        json!({
            "schema": PLAYBACK_MANIFEST_SCHEMA,
            "datasetId": dataset_id,
            "recordingSegmentId": recording_id,
            "applicationId": "veoveo-uav-sim",
            "recordingKey": "inspection-flight",
            "state": "sealed",
            "startedAt": "2026-07-28T20:00:00Z",
            "endedAt": null,
            "catalogRevision": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "access": {
                "grantId": uuid::Uuid::now_v7(),
                "redapToken": "scoped-token",
                "expiresAt": "2026-07-28T20:05:00Z"
            },
            "archive": {
                "uri": PlaybackArchiveUri::new(&origin, dataset_id, recording_id),
                "datasetId": dataset_id,
                "recordingSegmentId": recording_id,
                "catalogRevision": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                "rrdVersion": "0.38.1",
                "optimizationProfile": "object-store",
                "byteLen": 42,
                "layerCount": 1
            },
            "live": null,
            "blueprint": {
                "blueprintId": "producer-default",
                "revision": 3,
                "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                "byteLen": 512,
                "mapProvider": "mapbox"
            }
        })
    }

    #[test]
    fn manifest_v11_is_canonicalized_after_identity_validation() {
        let recording_id = RecordingId::new();
        let mut manifest = manifest_value(recording_id);
        manifest["state"] = json!("live");
        manifest["archive"] = serde_json::Value::Null;
        manifest["live"] = json!({
            "historySeconds": 1,
            "videoPrerollSeconds": 2,
            "transport": "rerun_rrd_channel_v2"
        });
        let body = serde_json::to_vec(&manifest).unwrap();
        let validated = validated_manifest_bytes(&body, recording_id).unwrap();
        let decoded: serde_json::Value = serde_json::from_slice(&validated).unwrap();
        assert_eq!(decoded["schema"], PLAYBACK_MANIFEST_SCHEMA);
        assert_eq!(decoded["recordingSegmentId"], recording_id.to_string());
        assert_eq!(decoded["blueprint"]["mapProvider"], "mapbox");
        assert_eq!(decoded["live"]["transport"], "rerun_rrd_channel_v2");
    }

    #[test]
    fn manifest_rejects_an_unknown_live_transport() {
        let recording_id = RecordingId::new();
        let mut manifest = manifest_value(recording_id);
        manifest["state"] = json!("live");
        manifest["archive"] = serde_json::Value::Null;
        manifest["live"] = json!({
            "historySeconds": 1,
            "videoPrerollSeconds": 2,
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
            .route(BLUEPRINT_PATH, get(blueprint))
            .route(PROJECTION_PATH, get(projection));
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
        let recording_id = RecordingId::new();
        let mut obsolete = manifest_value(recording_id);
        obsolete["schema"] = json!("veoveo.ai/recording-playback/v9");
        assert!(
            validated_manifest_bytes(&serde_json::to_vec(&obsolete).unwrap(), recording_id)
                .is_err()
        );

        let other_recording_id = RecordingId::new();
        assert!(
            validated_manifest_bytes(
                &serde_json::to_vec(&manifest_value(other_recording_id)).unwrap(),
                recording_id
            )
            .is_err()
        );

        let mut unknown_provider = manifest_value(recording_id);
        unknown_provider["blueprint"]["mapProvider"] = json!("silentFallback");
        assert!(
            validated_manifest_bytes(
                &serde_json::to_vec(&unknown_provider).unwrap(),
                recording_id
            )
            .is_err()
        );
    }

    #[test]
    fn shared_playback_contract_rejects_invalid_identities_and_unknown_fields() {
        let recording = RecordingId::new();
        for pointer in [
            "/datasetId",
            "/recordingSegmentId",
            "/access/grantId",
            "/archive/datasetId",
            "/archive/recordingSegmentId",
        ] {
            for invalid in [
                "private-id",
                "01983da0-0000-4000-8000-000000000001",
                "01983DA0-0000-7000-8000-000000000001",
                "01983da0-0000-7000-c000-000000000001",
            ] {
                let mut value = manifest_value(recording);
                *value.pointer_mut(pointer).unwrap() = json!(invalid);
                assert!(
                    validated_manifest_bytes(&serde_json::to_vec(&value).unwrap(), recording)
                        .is_err()
                );
            }
        }
        for pointer in ["", "/access", "/archive", "/blueprint"] {
            let mut value = manifest_value(recording);
            value.pointer_mut(pointer).unwrap()["unknown"] = json!(true);
            assert!(
                validated_manifest_bytes(&serde_json::to_vec(&value).unwrap(), recording).is_err()
            );
        }
    }
}
