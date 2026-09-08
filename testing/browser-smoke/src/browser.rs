use std::{
    collections::{BTreeSet, VecDeque},
    future::Future,
    path::Path,
    sync::Arc,
};

#[cfg(test)]
use anyhow::anyhow;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Message, http::StatusCode},
};
use url::Url;

use super::*;
use recording_acceptance::{
    ElementBounds, RecordingPlaybackMode, RecordingPlaybackNetworkEvidence,
    RerunCameraRenderEvidence, RerunRenderEvidence, analyze_rerun_camera_frame,
    analyze_rerun_camera_render, analyze_rerun_render,
};

#[path = "browser/recording_acceptance.rs"]
mod recording_acceptance;

const SIMULTANEOUS_VIEW_BARRIER_TIMEOUT: Duration = Duration::from_secs(15);
const MINIMUM_DELIVERED_FRAME_RATE_HZ: f64 = 12.0;
const MAXIMUM_SOURCE_TO_RENDER_P95_MS: f64 = 85.0;
const MAXIMUM_MOTION_TO_PHOTON_P95_MS: f64 = 250.0;
const MINIMUM_MEAN_LUMA: f64 = 25.0;
const MAXIMUM_MEAN_LUMA: f64 = 225.0;

#[derive(Clone, Copy, Debug)]
#[allow(dead_code, reason = "used by the focused browser-smoke binary")]
pub(crate) struct ConsoleAppExpectation {
    pub(crate) server: &'static str,
    pub(crate) resource_uri: &'static str,
    pub(crate) marker: &'static str,
    pub(crate) settled_state: ConsoleAppSettledState,
    pub(crate) required_selector: Option<&'static str>,
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code, reason = "used by the focused browser-smoke binary")]
pub(crate) enum ConsoleAppSettledState {
    Exact(&'static [&'static str]),
    Prefix(&'static [&'static str]),
    MapViewport,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code, reason = "used by the focused browser-smoke binary")]
pub(crate) struct ConsoleAppsCatalogEvidence {
    schema: &'static str,
    captured_at: chrono::DateTime<chrono::Utc>,
    page_url: String,
    screenshot_path: String,
    screenshot_sha256: String,
    hardware: HardwareIdentity,
    apps: Vec<ConsoleAppCatalogEntry>,
    server_groups: Vec<String>,
    rendered_apps: Vec<RenderedConsoleAppEvidence>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code, reason = "used by the focused browser-smoke binary")]
struct ConsoleAppCatalogEntry {
    server: String,
    resource_uri: String,
    standalone_path: String,
    name: String,
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code, reason = "used by the focused browser-smoke binary")]
struct ConsoleAppCatalogDegradation {
    server: String,
    surface: String,
    code: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code, reason = "used by the focused browser-smoke binary")]
struct ConsoleAppCatalogProbe {
    status: u16,
    apps: Vec<ConsoleAppCatalogEntry>,
    degradations: Vec<ConsoleAppCatalogDegradation>,
    server_groups: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code, reason = "used by the focused browser-smoke binary")]
struct ConsoleAppRenderState {
    title: String,
    heading: String,
    status: String,
    body: String,
    host_display_mode: String,
    host_bridge_error: String,
    visible_errors: Vec<String>,
    required_selector_found: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code, reason = "used by the focused browser-smoke binary")]
struct RenderedConsoleAppEvidence {
    server: String,
    resource_uri: String,
    page_url: String,
    screenshot_path: String,
    screenshot_sha256: String,
    hardware: HardwareIdentity,
    render: ConsoleAppRenderState,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConsoleLiveCaptureEvidence {
    schema: &'static str,
    captured_at: chrono::DateTime<chrono::Utc>,
    page_url: String,
    screenshot_path: String,
    screenshot_sha256: String,
    hardware: HardwareIdentity,
    video: AppVideoState,
    decode: DecodeIdentity,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code, reason = "shared by the focused browser-smoke binary")]
pub(crate) struct MapWorkspaceCaptureEvidence {
    schema: &'static str,
    captured_at: chrono::DateTime<chrono::Utc>,
    page_url: String,
    screenshot_path: String,
    screenshot_sha256: String,
    hardware: HardwareIdentity,
    bridge: MapWorkspaceBridgeEvidence,
    theme: Value,
    render: MapWorkspaceRenderEvidence,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConsoleMapWorkspaceCaptureEvidence {
    schema: &'static str,
    captured_at: chrono::DateTime<chrono::Utc>,
    page_url: String,
    workspace_screenshot_path: String,
    workspace_screenshot_sha256: String,
    source_screenshot_path: String,
    source_screenshot_sha256: String,
    feature_screenshot_path: String,
    feature_screenshot_sha256: String,
    guided_screenshot_path: String,
    guided_screenshot_sha256: String,
    management_screenshot_path: String,
    management_screenshot_sha256: String,
    hardware: HardwareIdentity,
    workspace: Value,
    source: Value,
    feature: Value,
    guided: Value,
    management: Value,
    render: MapWorkspaceRenderEvidence,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code, reason = "shared by the focused browser-smoke binary")]
struct MapWorkspaceBridgeEvidence {
    initialized: bool,
    query_calls: u64,
    query_responses: u64,
    authored_query_calls: u64,
    source_query_calls: u64,
    size_height: f64,
    last_query: Value,
    last_authored_query: Value,
    last_source_query: Value,
    errors: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MapWorkspaceRenderEvidence {
    screenshot_width: u32,
    screenshot_height: u32,
    feature_colored_pixels: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConsoleLiveGridEvidence {
    schema: &'static str,
    captured_at: chrono::DateTime<chrono::Utc>,
    page_url: String,
    screenshot_path: String,
    screenshot_sha256: String,
    hardware: HardwareIdentity,
    videos: Vec<GridVideoState>,
    decode: DecodeIdentity,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConsoleLiveRestartEvidence {
    schema: &'static str,
    captured_at: chrono::DateTime<chrono::Utc>,
    component: String,
    page_url: String,
    screenshot_path: String,
    screenshot_sha256: String,
    hardware: HardwareIdentity,
    before: RestartVideoIdentity,
    after: RestartVideoIdentity,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RestartVideoIdentity {
    document_epoch_ms: f64,
    camera_id: String,
    viewer_instance_id: String,
    live_view_id: String,
    stream_product_id: String,
    current_time: f64,
}

impl From<&AppVideoState> for RestartVideoIdentity {
    fn from(state: &AppVideoState) -> Self {
        Self {
            document_epoch_ms: state.document_epoch_ms,
            camera_id: state.camera_id.clone(),
            viewer_instance_id: state.viewer_instance_id.clone(),
            live_view_id: state.live_view_id.clone(),
            stream_product_id: state.stream_product_id.clone(),
            current_time: state.current_time,
        }
    }
}

impl ConsoleLiveGridEvidence {
    #[allow(dead_code)] // Consumed by the focused browser binary that includes this module.
    pub(crate) fn products(&self) -> Vec<String> {
        self.videos
            .iter()
            .map(|video| video.stream_product_id.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}

#[allow(dead_code)] // Viewer authorization identity is consumed by the focused browser binary.
impl ConsoleLiveCaptureEvidence {
    pub(crate) fn camera_id(&self) -> &str {
        &self.video.camera_id
    }

    pub(crate) fn viewer_instance_id(&self) -> &str {
        &self.video.viewer_instance_id
    }

    pub(crate) fn live_view_id(&self) -> &str {
        &self.video.live_view_id
    }

    pub(crate) fn stream_product_id(&self) -> &str {
        &self.video.stream_product_id
    }

    pub(crate) fn observed_frame_rate_hz(&self) -> f64 {
        self.video.observed_frame_rate_hz
    }

    pub(crate) fn cadence_dropped_frames(&self) -> u64 {
        self.video.cadence_dropped_frames
    }

    pub(crate) fn source_to_render_p95_ms(&self) -> f64 {
        self.video.source_to_render_p95_ms
    }

    pub(crate) fn composed_motion_to_photon_upper_bound_p95_ms(&self) -> f64 {
        self.video.composed_motion_to_photon_upper_bound_p95_ms
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConsoleRecordingCaptureEvidence {
    schema: &'static str,
    captured_at: chrono::DateTime<chrono::Utc>,
    page_url: String,
    recording_id: String,
    screenshot_path: String,
    screenshot_sha256: String,
    hardware: HardwareIdentity,
    live_follow: RerunLiveFollowEvidence,
    network: RecordingPlaybackNetworkEvidence,
    render: RerunRenderEvidence,
    camera_render: RerunCameraRenderEvidence,
    initial_responsiveness: RerunResponsivenessEvidence,
    final_responsiveness: RerunResponsivenessEvidence,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub(crate) struct ConsoleRecordingArchiveCaptureEvidence {
    schema: &'static str,
    captured_at: chrono::DateTime<chrono::Utc>,
    page_url: String,
    recording_id: String,
    screenshot_path: String,
    screenshot_sha256: String,
    hardware: HardwareIdentity,
    network: RecordingPlaybackNetworkEvidence,
    timeline: RecordingArchiveTimelineEvidence,
    render: RerunRenderEvidence,
    camera_render: RerunRenderEvidence,
    responsiveness: RerunResponsivenessEvidence,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordingArchiveTimelineEvidence {
    recording_id: String,
    timeline: String,
    current_time: f64,
    newest_time: f64,
}

impl ConsoleRecordingCaptureEvidence {
    #[allow(dead_code)] // Used by the focused browser binary that includes this module.
    pub(crate) fn final_timeline_seconds(&self) -> f64 {
        self.live_follow.final_time / 1_000_000_000.0
    }

    #[allow(dead_code)] // Used by the focused browser binary that includes this module.
    pub(crate) fn captured_at(&self) -> chrono::DateTime<chrono::Utc> {
        self.captured_at
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConsoleStreamCaptureEvidence {
    schema: &'static str,
    captured_at: chrono::DateTime<chrono::Utc>,
    page_url: String,
    screenshot_path: String,
    screenshot_sha256: String,
    hardware: HardwareIdentity,
    stream: StreamAppState,
    decode: StreamDecodeIdentity,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // Used by the focused browser binary that includes this module.
pub(crate) struct ConsoleAgentInstructionEvidence {
    schema: &'static str,
    captured_at: chrono::DateTime<chrono::Utc>,
    page_url: String,
    screenshot_path: String,
    screenshot_sha256: String,
    agent_id: String,
    message: String,
    hardware: HardwareIdentity,
    app_result: String,
    operator_entry: Value,
    agent_reply: Value,
}

#[allow(dead_code)] // Used by the focused browser binary that includes this module.
pub(crate) async fn send_console_uav_agent_instruction(
    cdp_base: &str,
    public_base_url: &str,
    agent_id: &str,
    message: &str,
    screenshot_path: &Path,
    timeout: Duration,
) -> Result<ConsoleAgentInstructionEvidence> {
    let page_url = console_acceptance_url(public_base_url, "/apps/uav-sim/live.html");
    tokio::time::timeout(
        timeout,
        send_console_uav_agent_instruction_inner(
            cdp_base,
            &page_url,
            agent_id,
            message,
            screenshot_path,
            timeout,
        ),
    )
    .await
    .with_context(|| format!("Console UAV agent instruction exceeded {timeout:?}"))?
}

pub(crate) async fn capture_console_live_app(
    cdp_base: &str,
    public_base_url: &str,
    expected_camera_id: &str,
    screenshot_path: &Path,
    timeout: Duration,
) -> Result<ConsoleLiveCaptureEvidence> {
    let page_url = console_acceptance_url(public_base_url, "/apps/uav-sim/live.html");
    tokio::time::timeout(
        timeout,
        capture_console_live_app_inner(
            cdp_base,
            &page_url,
            expected_camera_id,
            screenshot_path,
            None,
            false,
        ),
    )
    .await
    .with_context(|| format!("Console live-App capture exceeded {timeout:?}"))?
}

#[allow(dead_code)] // Multi-tab acceptance is owned by the focused browser binary.
pub(crate) async fn capture_console_live_app_pair(
    cdp_base: &str,
    public_base_url: &str,
    expected_camera_id: &str,
    first_screenshot_path: &Path,
    second_screenshot_path: &Path,
    timeout: Duration,
) -> Result<(ConsoleLiveCaptureEvidence, ConsoleLiveCaptureEvidence)> {
    let page_url = console_acceptance_url(public_base_url, "/apps/uav-sim/live.html");
    let simultaneous = Arc::new(tokio::sync::Barrier::new(2));
    let first = tokio::time::timeout(
        timeout,
        capture_console_live_app_inner(
            cdp_base,
            &page_url,
            expected_camera_id,
            first_screenshot_path,
            Some(Arc::clone(&simultaneous)),
            true,
        ),
    );
    let second = tokio::time::timeout(
        timeout,
        capture_console_live_app_inner(
            cdp_base,
            &page_url,
            expected_camera_id,
            second_screenshot_path,
            Some(simultaneous),
            true,
        ),
    );
    let (first, second) = tokio::join!(first, second);
    Ok((
        first.with_context(|| format!("first Console live-App capture exceeded {timeout:?}"))??,
        second
            .with_context(|| format!("second Console live-App capture exceeded {timeout:?}"))??,
    ))
}

#[allow(dead_code)] // Multi-camera acceptance is owned by the focused browser binary.
pub(crate) async fn capture_console_live_app_grid(
    cdp_base: &str,
    public_base_url: &str,
    expected_camera_ids: &[&str],
    screenshot_path: &Path,
    timeout: Duration,
) -> Result<ConsoleLiveGridEvidence> {
    let page_url = console_acceptance_url(public_base_url, "/apps/uav-sim/live.html");
    tokio::time::timeout(
        timeout,
        capture_console_live_app_grid_inner(
            cdp_base,
            &page_url,
            expected_camera_ids,
            screenshot_path,
            None,
        ),
    )
    .await
    .with_context(|| format!("Console live-App grid capture exceeded {timeout:?}"))?
}

#[allow(dead_code)] // Five-user acceptance is owned by the focused browser binary.
pub(crate) async fn capture_console_live_app_five_user_grid(
    cdp_base: &str,
    public_base_url: &str,
    expected_camera_ids: &[&str],
    screenshot_directory: &Path,
    timeout: Duration,
) -> Result<Vec<ConsoleLiveGridEvidence>> {
    const USER_COUNT: usize = 5;
    ensure!(
        expected_camera_ids.len() == 5,
        "five-user acceptance requires the complete five-camera collection"
    );
    let page_url = console_acceptance_url(public_base_url, "/apps/uav-sim/live.html");
    let barrier = Arc::new(tokio::sync::Barrier::new(USER_COUNT));
    let screenshot_paths = (1..=USER_COUNT)
        .map(|user| screenshot_directory.join(format!("uav-live-view-user-{user}.png")))
        .collect::<Vec<_>>();
    futures::future::try_join_all(screenshot_paths.iter().enumerate().map(|(index, path)| {
        let barrier = Arc::clone(&barrier);
        let page_url = &page_url;
        async move {
            tokio::time::timeout(
                timeout,
                capture_console_live_app_grid_inner(
                    cdp_base,
                    page_url,
                    expected_camera_ids,
                    path,
                    Some(barrier),
                ),
            )
            .await
            .with_context(|| {
                format!(
                    "Console five-user live-App grid {} exceeded {timeout:?}",
                    index + 1
                )
            })?
        }
    }))
    .await
}

#[allow(dead_code)] // Component-restart acceptance is owned by the focused browser binary.
pub(crate) async fn capture_console_live_app_restart<F, Fut, T>(
    cdp_base: &str,
    public_base_url: &str,
    expected_camera_id: &str,
    component: &str,
    screenshot_path: &Path,
    timeout: Duration,
    restart: F,
) -> Result<(ConsoleLiveRestartEvidence, T)>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let page_url = console_acceptance_url(public_base_url, "/apps/uav-sim/live.html");
    tokio::time::timeout(
        timeout,
        capture_console_live_app_restart_inner(
            cdp_base,
            &page_url,
            expected_camera_id,
            component,
            screenshot_path,
            timeout,
            restart,
        ),
    )
    .await
    .with_context(|| format!("Console live-App {component} restart capture exceeded {timeout:?}"))?
}

pub(crate) async fn preflight_console_live_app(
    cdp_base: &str,
    public_base_url: &str,
    timeout: Duration,
) -> Result<()> {
    let page_url = console_acceptance_url(public_base_url, "/apps/uav-sim/live.html");
    tokio::time::timeout(
        timeout,
        preflight_console_live_app_inner(cdp_base, &page_url),
    )
    .await
    .with_context(|| format!("Console live-App preflight exceeded {timeout:?}"))?
}

pub(crate) async fn preflight_standalone_live_app(
    cdp_base: &str,
    public_base_url: &str,
    timeout: Duration,
) -> Result<()> {
    let page_url = standalone_acceptance_url(public_base_url, "/apps/uav-sim/live.html");
    tokio::time::timeout(
        timeout,
        preflight_standalone_live_app_inner(cdp_base, &page_url),
    )
    .await
    .with_context(|| format!("standalone UAV App preflight exceeded {timeout:?}"))?
}

#[allow(dead_code, reason = "used by the focused browser-smoke binary")]
pub(crate) async fn capture_console_apps_catalog(
    cdp_base: &str,
    public_base_url: &str,
    expectations: &[ConsoleAppExpectation],
    evidence_directory: &Path,
    timeout: Duration,
) -> Result<ConsoleAppsCatalogEvidence> {
    let page_url = console_acceptance_url(public_base_url, "/apps");
    tokio::time::timeout(
        timeout,
        capture_console_apps_catalog_inner(
            cdp_base,
            public_base_url,
            &page_url,
            expectations,
            evidence_directory,
        ),
    )
    .await
    .with_context(|| format!("Console Apps catalog acceptance exceeded {timeout:?}"))?
}

pub(crate) async fn capture_console_recording(
    cdp_base: &str,
    public_base_url: &str,
    recording_id: &str,
    screenshot_path: &Path,
    timeout: Duration,
) -> Result<ConsoleRecordingCaptureEvidence> {
    let page_url = console_acceptance_url(public_base_url, &format!("/recordings/{recording_id}"));
    tokio::time::timeout(
        timeout,
        capture_console_recording_inner(cdp_base, &page_url, recording_id, screenshot_path),
    )
    .await
    .with_context(|| format!("Console Rerun capture exceeded {timeout:?}"))?
}

#[allow(dead_code)]
pub(crate) async fn capture_console_recording_archive(
    cdp_base: &str,
    public_base_url: &str,
    recording_id: &str,
    screenshot_path: &Path,
    timeout: Duration,
) -> Result<ConsoleRecordingArchiveCaptureEvidence> {
    let page_url = console_acceptance_url(public_base_url, &format!("/recordings/{recording_id}"));
    tokio::time::timeout(
        timeout,
        capture_console_recording_archive_inner(cdp_base, &page_url, recording_id, screenshot_path),
    )
    .await
    .with_context(|| format!("Console archived Rerun capture exceeded {timeout:?}"))?
}

pub(crate) async fn capture_console_stream_app(
    cdp_base: &str,
    public_base_url: &str,
    screenshot_path: &Path,
    timeout: Duration,
) -> Result<ConsoleStreamCaptureEvidence> {
    let page_url = console_acceptance_url(public_base_url, "/apps/stream/live.html");
    tokio::time::timeout(
        timeout,
        capture_console_stream_app_inner(cdp_base, &page_url, screenshot_path),
    )
    .await
    .with_context(|| format!("Console Stream-App capture exceeded {timeout:?}"))?
}

fn console_acceptance_url(public_base_url: &str, route: &str) -> String {
    format!(
        "{}/console/?veoveo-acceptance={}#{route}",
        public_base_url.trim_end_matches('/'),
        uuid::Uuid::now_v7(),
    )
}

fn standalone_acceptance_url(public_base_url: &str, route: &str) -> String {
    format!("{}{}", public_base_url.trim_end_matches('/'), route)
}

async fn preflight_console_live_app_inner(cdp_base: &str, page_url: &str) -> Result<()> {
    let (mut cdp, target_id, session_id) = open_headed_target(cdp_base, page_url).await?;
    let acceptance: Result<()> = async {
        wait_for_document(&mut cdp, &session_id).await?;
        assert_page_visible(&mut cdp, &session_id).await?;
        let hardware: HardwareIdentity =
            cdp.evaluate(&session_id, HARDWARE_PREFLIGHT, true).await?;
        hardware.validate()?;
        wait_for_console_app_body(&mut cdp, &target_id, &session_id, "uav-sim", "Live Cameras")
            .await?;
        cdp.assert_no_software_renderer_events()?;
        Ok(())
    }
    .await;
    let close = close_target(&mut cdp, &target_id).await;
    acceptance?;
    close?;
    Ok(())
}

async fn preflight_standalone_live_app_inner(cdp_base: &str, page_url: &str) -> Result<()> {
    let (mut cdp, target_id, session_id) = open_headed_target(cdp_base, page_url).await?;
    let acceptance: Result<()> = async {
        wait_for_document(&mut cdp, &session_id).await?;
        assert_page_visible(&mut cdp, &session_id).await?;
        let hardware: HardwareIdentity =
            cdp.evaluate(&session_id, HARDWARE_PREFLIGHT, true).await?;
        hardware.validate()?;
        wait_for_console_app_body(
            &mut cdp,
            &target_id,
            &session_id,
            "uav-sim",
            "Live Cameras",
        )
        .await?;
        let host: Value = cdp
            .evaluate(
                &session_id,
                r#"(async () => {
                  const frame = document.querySelector("iframe.app-frame");
                  const response = frame ? await fetch(frame.src, {credentials:"same-origin"}) : null;
                  return {
                    path:location.pathname,
                    title:document.querySelector(".standalone-app-header h1")?.textContent ?? "",
                    returnHref:document.querySelector(".standalone-app-header a")?.getAttribute("href") ?? "",
                    frameTitle:frame?.title ?? "",
                    sandbox:frame?.getAttribute("sandbox") ?? "",
                    referrerPolicy:frame?.referrerPolicy ?? "",
                    frameStatus:response?.status ?? 0,
                    frameCsp:response?.headers.get("content-security-policy") ?? ""
                  };
                })()"#,
                true,
            )
            .await?;
        ensure!(
            host.get("path").and_then(Value::as_str) == Some("/apps/uav-sim/live.html")
                && host
                    .get("title")
                    .and_then(Value::as_str)
                    .is_some_and(|title| title.contains("Live Cameras"))
                && host.get("returnHref").and_then(Value::as_str) == Some("/console/#/apps")
                && host
                    .get("frameTitle")
                    .and_then(Value::as_str)
                    .is_some_and(|title| title.contains("Live Cameras"))
                && host.get("sandbox").and_then(Value::as_str) == Some("allow-scripts")
                && host.get("referrerPolicy").and_then(Value::as_str) == Some("no-referrer")
                && host.get("frameStatus").and_then(Value::as_u64) == Some(200)
                && host
                    .get("frameCsp")
                    .and_then(Value::as_str)
                    .is_some_and(|csp| csp.contains("frame-ancestors 'self'")),
            "standalone App did not retain the shared authorized frame boundary: {host}"
        );
        cdp.assert_no_software_renderer_events()?;
        Ok(())
    }
    .await;
    let close = close_target(&mut cdp, &target_id).await;
    acceptance?;
    close?;
    Ok(())
}

#[allow(dead_code, reason = "used by the focused browser-smoke binary")]
async fn capture_console_apps_catalog_inner(
    cdp_base: &str,
    public_base_url: &str,
    page_url: &str,
    expectations: &[ConsoleAppExpectation],
    evidence_directory: &Path,
) -> Result<ConsoleAppsCatalogEvidence> {
    ensure!(
        !expectations.is_empty(),
        "Console Apps acceptance requires at least one expected App"
    );
    let expected_servers = expectations
        .iter()
        .map(|expectation| expectation.server)
        .collect::<BTreeSet<_>>();
    let expected_uris = expectations
        .iter()
        .map(|expectation| expectation.resource_uri)
        .collect::<BTreeSet<_>>();
    ensure!(
        expected_uris.len() == expectations.len(),
        "Console Apps acceptance contains duplicate resource URIs"
    );

    let (mut cdp, target_id, session_id) = open_headed_target(cdp_base, page_url).await?;
    let catalog_acceptance: Result<_> = async {
        wait_for_document(&mut cdp, &session_id).await?;
        assert_page_visible(&mut cdp, &session_id).await?;
        let hardware: HardwareIdentity =
            cdp.evaluate(&session_id, HARDWARE_PREFLIGHT, true).await?;
        hardware.validate()?;

        let deadline = tokio::time::Instant::now() + Duration::from_secs(90);
        let probe = loop {
            let probe: ConsoleAppCatalogProbe = cdp
                .evaluate(
                    &session_id,
                    r#"(async () => {
                      const response=await fetch("/console/api/apps",{credentials:"same-origin",headers:{Accept:"application/json"}});
                      let catalog={apps:[],degradations:[]};
                      try{catalog=await response.json()}catch{}
                      return {
                        status:response.status,
                        apps:(catalog.apps||[]).map(app=>({
                          server:app.server,
                          resourceUri:app.resourceUri,
                          standalonePath:app.standalonePath,
                          name:app.name,
                          title:app.title??null
                        })),
                        degradations:catalog.degradations||[],
                        serverGroups:[...document.querySelectorAll(".app-catalog-group .mono")]
                          .map(node=>(node.textContent||"").trim()).filter(Boolean)
                      };
                    })()"#,
                    true,
                )
                .await?;
            let discovered = probe
                .apps
                .iter()
                .map(|app| app.resource_uri.as_str())
                .collect::<BTreeSet<_>>();
            let groups = probe
                .server_groups
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>();
            if probe.status == 200
                && probe.degradations.is_empty()
                && expected_uris.is_subset(&discovered)
                && expected_servers.is_subset(&groups)
            {
                break probe;
            }
            ensure!(
                tokio::time::Instant::now() < deadline,
                "Console did not project the complete grouped App catalog: HTTP {}, Apps {:?}, groups {:?}, degradations {:?}",
                probe.status,
                discovered,
                groups,
                probe
                    .degradations
                    .iter()
                    .map(|failure| format!("{}/{}/{}", failure.server, failure.surface, failure.code))
                    .collect::<Vec<_>>()
            );
            tokio::time::sleep(Duration::from_millis(250)).await;
        };

        for expectation in expectations {
            let expected_path = format!(
                "/apps/{}",
                expectation
                    .resource_uri
                    .strip_prefix("ui://")
                    .context("expected Console App URI must use ui://")?
            );
            let app = probe
                .apps
                .iter()
                .find(|app| app.resource_uri == expectation.resource_uri)
                .expect("complete expected catalog set");
            ensure!(
                app.server == expectation.server && app.standalone_path == expected_path,
                "Console App {} has inconsistent identity or standalone path: {:?}",
                expectation.resource_uri,
                app
            );
        }

        let screenshot_path = evidence_directory.join("catalog.png");
        let screenshot_sha256 =
            capture_screenshot(&mut cdp, &session_id, &screenshot_path).await?;
        cdp.assert_no_software_renderer_events()?;
        Ok((hardware, probe, screenshot_path, screenshot_sha256))
    }
    .await;
    let close = close_target(&mut cdp, &target_id).await;
    let (hardware, probe, screenshot_path, screenshot_sha256) = catalog_acceptance?;
    close?;

    let mut rendered_apps = Vec::with_capacity(expectations.len());
    for expectation in expectations {
        rendered_apps.push(
            capture_one_console_app(cdp_base, public_base_url, expectation, evidence_directory)
                .await
                .with_context(|| {
                    format!("rendering {} through Console", expectation.resource_uri)
                })?,
        );
    }

    Ok(ConsoleAppsCatalogEvidence {
        schema: "veoveo.io/console-apps-browser-evidence/v1",
        captured_at: chrono::Utc::now(),
        page_url: page_url.to_owned(),
        screenshot_path: screenshot_path.display().to_string(),
        screenshot_sha256,
        hardware,
        apps: probe.apps,
        server_groups: probe.server_groups,
        rendered_apps,
    })
}

#[allow(dead_code, reason = "used by the focused browser-smoke binary")]
async fn capture_one_console_app(
    cdp_base: &str,
    public_base_url: &str,
    expectation: &ConsoleAppExpectation,
    evidence_directory: &Path,
) -> Result<RenderedConsoleAppEvidence> {
    let route = format!(
        "/apps/{}",
        expectation
            .resource_uri
            .strip_prefix("ui://")
            .context("expected Console App URI must use ui://")?
    );
    let page_url = console_acceptance_url(public_base_url, &route);
    let (mut cdp, target_id, session_id) = open_headed_target(cdp_base, &page_url).await?;
    let acceptance: Result<_> = async {
        wait_for_document(&mut cdp, &session_id).await?;
        assert_page_visible(&mut cdp, &session_id).await?;
        let hardware: HardwareIdentity =
            cdp.evaluate(&session_id, HARDWARE_PREFLIGHT, true).await?;
        hardware.validate()?;
        wait_for_console_app_body(
            &mut cdp,
            &target_id,
            &session_id,
            expectation.server,
            expectation.marker,
        )
        .await?;
        let render =
            wait_for_console_app_settled(&mut cdp, &target_id, &session_id, expectation).await?;
        let screenshot_name = expectation
            .resource_uri
            .trim_start_matches("ui://")
            .replace(['/', '.'], "-");
        let screenshot_path = evidence_directory.join(format!("{screenshot_name}.png"));
        let screenshot_sha256 = capture_screenshot(&mut cdp, &session_id, &screenshot_path).await?;
        cdp.assert_no_software_renderer_events()?;
        Ok((hardware, render, screenshot_path, screenshot_sha256))
    }
    .await;
    let close = close_target(&mut cdp, &target_id).await;
    let (hardware, render, screenshot_path, screenshot_sha256) = acceptance?;
    close?;
    Ok(RenderedConsoleAppEvidence {
        server: expectation.server.to_owned(),
        resource_uri: expectation.resource_uri.to_owned(),
        page_url,
        screenshot_path: screenshot_path.display().to_string(),
        screenshot_sha256,
        hardware,
        render,
    })
}

async fn wait_for_console_app_settled(
    cdp: &mut Cdp,
    parent_target_id: &str,
    session_id: &str,
    expectation: &ConsoleAppExpectation,
) -> Result<ConsoleAppRenderState> {
    let required_selector = serde_json::to_string(expectation.required_selector.unwrap_or(""))?;
    let expression = format!(
        r#"(async () => {{
          const visible = node => {{
            if (!node || !node.textContent?.trim()) return false;
            const style = getComputedStyle(node);
            return !node.hidden && style.display !== "none" && style.visibility !== "hidden"
              && node.getClientRects().length > 0;
          }};
          const requestId = `veoveo-acceptance-${{crypto.randomUUID()}}`;
          let hostDisplayMode = "";
          let hostBridgeError = "";
          try {{
            hostDisplayMode = await new Promise((resolve, reject) => {{
              const timer = setTimeout(() => {{
                removeEventListener("message", receive);
                reject(new Error("host bridge display-mode probe timed out"));
              }}, 2000);
              const receive = event => {{
                const message = event.data;
                if (event.source !== parent || !message || message.id !== requestId) return;
                clearTimeout(timer);
                removeEventListener("message", receive);
                if (message.error) reject(new Error(message.error.message || "host bridge rejected probe"));
                else resolve(message.result?.mode || "");
              }};
              addEventListener("message", receive);
              parent.postMessage({{
                jsonrpc:"2.0",
                id:requestId,
                method:"ui/request-display-mode",
                params:{{mode:"inline"}}
              }}, "*");
            }});
          }} catch (error) {{
            hostBridgeError = error instanceof Error ? error.message : String(error);
          }}
          const requiredSelector = {required_selector};
          return {{
            title:document.title||"",
            heading:document.querySelector("h1")?.textContent?.trim()||"",
            status:document.getElementById("status")?.textContent?.trim()||"",
            body:document.body?.innerText||"",
            hostDisplayMode,
            hostBridgeError,
            visibleErrors:[...document.querySelectorAll('[id*="error" i]')]
              .filter(visible).map(node=>node.textContent.trim()),
            requiredSelectorFound:!requiredSelector || Boolean(document.querySelector(requiredSelector))
          }};
        }})()"#
    );
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        let render: ConsoleAppRenderState = evaluate_console_app(
            cdp,
            parent_target_id,
            session_id,
            expectation.server,
            &expression,
            true,
        )
        .await?;
        if console_app_has_settled(expectation, &render) {
            return Ok(render);
        }
        ensure!(
            tokio::time::Instant::now() < deadline,
            "Console App {} did not reach its declared settled state: {:?}",
            expectation.resource_uri,
            render
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn console_app_has_settled(
    expectation: &ConsoleAppExpectation,
    render: &ConsoleAppRenderState,
) -> bool {
    if !render.heading.contains(expectation.marker)
        || render.host_display_mode != "inline"
        || !render.host_bridge_error.is_empty()
        || !render.visible_errors.is_empty()
        || !render.required_selector_found
    {
        return false;
    }
    let status = render.status.trim();
    match expectation.settled_state {
        ConsoleAppSettledState::Exact(values) => values.contains(&status),
        ConsoleAppSettledState::Prefix(values) => {
            values.iter().any(|prefix| status.starts_with(prefix))
        }
        ConsoleAppSettledState::MapViewport => {
            status == "No layers are visible. Enable a layer in the catalog."
                || status.split_once(" feature").is_some_and(|(count, rest)| {
                    count.parse::<u64>().is_ok() && rest.contains(" visible across ")
                })
        }
    }
}

async fn wait_for_console_app_body(
    cdp: &mut Cdp,
    parent_target_id: &str,
    session_id: &str,
    app_server: &str,
    marker: &str,
) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        let app: Value = evaluate_console_app(
            cdp,
            parent_target_id,
            session_id,
            app_server,
            r#"({
              readyState: document.readyState,
              body: document.body?.innerText ?? ""
            })"#,
            false,
        )
        .await?;
        if console_app_body_ready(&app, marker) {
            return Ok(());
        }
        ensure!(
            tokio::time::Instant::now() < deadline,
            "authenticated Console did not finish loading the {app_server} App: {app}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn console_app_body_ready(app: &Value, marker: &str) -> bool {
    app.get("readyState").and_then(Value::as_str) == Some("complete")
        && app
            .get("body")
            .and_then(Value::as_str)
            .is_some_and(|body| body.contains(marker))
}

#[allow(dead_code)] // Used by the focused browser binary that includes this module.
async fn send_console_uav_agent_instruction_inner(
    cdp_base: &str,
    page_url: &str,
    agent_id: &str,
    message: &str,
    screenshot_path: &Path,
    timeout: Duration,
) -> Result<ConsoleAgentInstructionEvidence> {
    let (mut cdp, target_id, session_id) =
        open_headed_target_in_window(cdp_base, page_url, true).await?;
    let acceptance = async {
        wait_for_document(&mut cdp, &session_id).await?;
        assert_page_visible(&mut cdp, &session_id).await?;
        let hardware: HardwareIdentity =
            cdp.evaluate(&session_id, HARDWARE_PREFLIGHT, true).await?;
        hardware.validate()?;
        wait_for_console_app_body(
            &mut cdp,
            &target_id,
            &session_id,
            "uav-sim",
            "Live Cameras",
        )
        .await?;

        let agent = serde_json::to_string(agent_id)?;
        let instruction = serde_json::to_string(message)?;
        let app_result: Value = evaluate_console_app(
            &mut cdp,
            &target_id,
            &session_id,
            "uav-sim",
            &format!(
                r#"(async () => {{
                  const agentId={agent},message={instruction};
                  const select=document.getElementById("agent"),input=document.getElementById("command"),output=document.getElementById("command-result");
                  if (![...select.options].some((option)=>option.value===agentId)) return {{error:`agent ${{agentId}} is unavailable`,result:""}};
                  select.value=agentId;input.value=message;
                  await sendInstruction({{preventDefault(){{}}}});
                  const result=output.textContent?.trim() ?? "";
                  return result.startsWith("Instruction queued for ")
                    ? {{error:"",result}}
                    : {{error:result||"agent instruction did not return a receipt",result:""}};
                }})()"#
            ),
            true,
        )
        .await?;
        ensure!(
            app_result.get("error").and_then(Value::as_str) == Some("")
                && app_result.get("result").and_then(Value::as_str)
                    == Some(format!("Instruction queued for {agent_id}.").as_str()),
            "UAV App did not queue the exact agent instruction: {app_result}"
        );

        let deadline = tokio::time::Instant::now() + timeout.saturating_sub(Duration::from_secs(5));
        let (operator_entry, agent_reply) = loop {
            let conversation: Value = cdp
                .evaluate(
                    &session_id,
                    &format!(
                        r#"(async () => {{
                          const response=await fetch(`/console/api/agents/${{encodeURIComponent({agent})}}/conversation`,{{credentials:"same-origin",headers:{{Accept:"application/json"}}}});
                          return {{status:response.status,body:await response.json()}};
                        }})()"#
                    ),
                    true,
                )
                .await?;
            ensure!(
                conversation.get("status").and_then(Value::as_u64) == Some(200),
                "Console agent conversation returned a failure: {conversation}"
            );
            let entries = conversation
                .pointer("/body/entries")
                .and_then(Value::as_array)
                .context("Console agent conversation omitted entries")?;
            let operator = entries.iter().find(|entry| {
                entry.get("role").and_then(Value::as_str) == Some("operator")
                    && entry.get("content").and_then(Value::as_str) == Some(message)
                    && entry.get("request_id").and_then(Value::as_str).is_some()
                    && entry.get("wake_id").and_then(Value::as_str).is_some()
            });
            let request_id = operator
                .and_then(|entry| entry.get("request_id"))
                .and_then(Value::as_str);
            let reply = request_id.and_then(|request_id| {
                entries.iter().find(|entry| {
                    entry.get("role").and_then(Value::as_str) == Some("agent")
                        && entry
                            .get("in_reply_to_request_ids")
                            .and_then(Value::as_array)
                            .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(request_id)))
                })
            });
            if let (Some(operator), Some(reply)) = (operator, reply) {
                break (operator.clone(), reply.clone());
            }
            ensure!(
                tokio::time::Instant::now() < deadline,
                "agent {agent_id} did not durably reply to the queued App instruction: {conversation}"
            );
            cdp.assert_no_software_renderer_events()?;
            assert_page_visible(&mut cdp, &session_id).await?;
            tokio::time::sleep(Duration::from_secs(1)).await;
        };
        let screenshot_sha256 = capture_screenshot(&mut cdp, &session_id, screenshot_path).await?;
        cdp.assert_no_software_renderer_events()?;
        Ok(ConsoleAgentInstructionEvidence {
            schema: "veoveo.io/uav-console-agent-instruction/v1",
            captured_at: chrono::Utc::now(),
            page_url: page_url.to_owned(),
            screenshot_path: screenshot_path.display().to_string(),
            screenshot_sha256,
            agent_id: agent_id.to_owned(),
            message: message.to_owned(),
            hardware,
            app_result: app_result
                .get("result")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            operator_entry,
            agent_reply,
        })
    }
    .await;
    let close = close_target(&mut cdp, &target_id).await;
    match acceptance {
        Ok(evidence) => {
            close.context("closing headed browser target after agent instruction")?;
            Ok(evidence)
        }
        Err(error) => {
            if let Err(close_error) = close {
                Err(error.context(format!(
                    "agent-instruction target cleanup also failed: {close_error:#}"
                )))
            } else {
                Err(error)
            }
        }
    }
}

async fn capture_console_live_app_inner(
    cdp_base: &str,
    page_url: &str,
    expected_camera_id: &str,
    screenshot_path: &Path,
    simultaneous: Option<Arc<tokio::sync::Barrier>>,
    new_window: bool,
) -> Result<ConsoleLiveCaptureEvidence> {
    let (mut cdp, target_id, session_id) =
        open_headed_target_in_window(cdp_base, page_url, new_window).await?;
    let acceptance = async {
        wait_for_document(&mut cdp, &session_id).await?;
        assert_page_visible(&mut cdp, &session_id).await?;
        let hardware: HardwareIdentity =
            cdp.evaluate(&session_id, HARDWARE_PREFLIGHT, true).await?;
        hardware.validate()?;
        let ready = wait_for_console_app_camera(
            &mut cdp,
            &target_id,
            &session_id,
            expected_camera_id,
        )
        .await?;
        ensure!(
            ready,
            "Console UAV live view App exposed no camera {expected_camera_id:?}"
        );
        isolate_console_app_camera(
            &mut cdp,
            &target_id,
            &session_id,
            expected_camera_id,
        )
        .await?;
        let first = match wait_for_console_video(
            &mut cdp,
            &target_id,
            &session_id,
            expected_camera_id,
        )
        .await
        {
            Ok(state) => state,
            Err(error) => {
                let diagnostics = cdp.stream_diagnostics(&session_id).await?;
                return Err(error.context(diagnostics));
            }
        };
        let second = match wait_for_console_video_advance(
            &mut cdp,
            &target_id,
            &session_id,
            expected_camera_id,
            first,
        )
        .await
        {
            Ok(state) => state,
            Err(error) => {
                let app_state: Value = evaluate_console_app(
                    &mut cdp,
                    &target_id,
                    &session_id,
                    "uav-sim",
                    APP_FRAME_VIDEO_STATE,
                    false,
                )
                .await
                .unwrap_or_else(|state_error| {
                    serde_json::json!({"diagnosticError": format!("{state_error:#}")})
                });
                let diagnostics = cdp.stream_diagnostics(&session_id).await?;
                return Err(error.context(format!(
                    "current App state: {app_state}; {diagnostics}"
                )));
            }
        };
        let decode: DecodeIdentity = evaluate_console_app(
            &mut cdp,
            &target_id,
            &session_id,
            "uav-sim",
            APP_FRAME_DECODE_IDENTITY,
            true,
        )
        .await?;
        decode.validate()?;
        if let Some(simultaneous) = simultaneous {
            tokio::time::timeout(SIMULTANEOUS_VIEW_BARRIER_TIMEOUT, simultaneous.wait())
                .await
                .context("the peer live-view window did not reach advancing video")?;
        }
        let snapshot: Value = cdp
            .evaluate(
                &session_id,
                r#"(async () => {
                  const snapshot = await fetch("/console/api/snapshot", {credentials:"same-origin"});
                  const apps = await fetch("/console/api/apps", {credentials:"same-origin"});
                  return {
                    snapshotStatus:snapshot.status,
                    appsStatus:apps.status,
                    appFrameTitle:document.querySelector("iframe.app-frame")?.title ?? "",
                    bodyText:document.body?.innerText ?? ""
                  };
                })()"#,
                true,
            )
            .await?;
        ensure!(
            snapshot.get("snapshotStatus").and_then(Value::as_u64) == Some(200)
                && snapshot.get("appsStatus").and_then(Value::as_u64) == Some(200)
                && snapshot
                    .get("appFrameTitle")
                    .and_then(Value::as_str)
                    .is_some_and(|title| title.contains("Live Cameras"))
                && snapshot
                    .get("bodyText")
                    .and_then(Value::as_str)
                    .is_some_and(|body| body.contains("Live Cameras")),
            "real Console did not load its snapshot, App catalog, and UAV live view frame: \
             {snapshot}"
        );
        let screenshot_sha256 =
            capture_screenshot(&mut cdp, &session_id, screenshot_path).await?;
        cdp.assert_no_software_renderer_events()?;
        Ok(ConsoleLiveCaptureEvidence {
            schema: "veoveo.io/uav-console-live-capture/v1",
            captured_at: chrono::Utc::now(),
            page_url: page_url.to_owned(),
            screenshot_path: screenshot_path.display().to_string(),
            screenshot_sha256,
            hardware,
            video: second,
            decode,
        })
    }
    .await;
    let release =
        close_console_live_view(&mut cdp, &target_id, &session_id, expected_camera_id).await;
    let close = close_target(&mut cdp, &target_id).await;
    finish_live_capture(acceptance, release, close)
}

async fn capture_console_live_app_grid_inner(
    cdp_base: &str,
    page_url: &str,
    expected_camera_ids: &[&str],
    screenshot_path: &Path,
    simultaneous: Option<Arc<tokio::sync::Barrier>>,
) -> Result<ConsoleLiveGridEvidence> {
    ensure!(
        expected_camera_ids.len() >= 2,
        "live-view grid acceptance requires at least two cameras"
    );
    let (mut cdp, target_id, session_id) =
        open_headed_target_in_window(cdp_base, page_url, simultaneous.is_some()).await?;
    let acceptance = async {
        wait_for_document(&mut cdp, &session_id).await?;
        assert_page_visible(&mut cdp, &session_id).await?;
        let hardware: HardwareIdentity =
            cdp.evaluate(&session_id, HARDWARE_PREFLIGHT, true).await?;
        hardware.validate()?;
        for camera_id in expected_camera_ids {
            ensure!(
                wait_for_console_app_camera(&mut cdp, &target_id, &session_id, camera_id).await?,
                "Console UAV live view App exposed no camera {camera_id:?}"
            );
        }
        let first = wait_for_console_grid_videos(
            &mut cdp,
            &target_id,
            &session_id,
            expected_camera_ids,
        )
        .await?;
        tokio::time::sleep(Duration::from_secs(2)).await;
        let second = wait_for_console_grid_videos(
            &mut cdp,
            &target_id,
            &session_id,
            expected_camera_ids,
        )
        .await?;
        for before in &first {
            let after = second
                .iter()
                .find(|video| video.camera_id == before.camera_id)
                .with_context(|| format!("grid lost camera {}", before.camera_id))?;
            ensure!(
                after.document_epoch_ms == before.document_epoch_ms
                    && after.viewer_instance_id == before.viewer_instance_id
                    && after.live_view_id == before.live_view_id
                    && after.stream_product_id == before.stream_product_id
                    && after.current_time > before.current_time + 0.25,
                "grid camera did not remain mounted on one advancing native product: {before:?} -> {after:?}"
            );
        }
        let viewer_instances = second
            .iter()
            .map(|video| video.viewer_instance_id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        let live_views = second
            .iter()
            .map(|video| video.live_view_id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        let products = second
            .iter()
            .map(|video| video.stream_product_id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        ensure!(
            viewer_instances.len() == 1
                && live_views.len() == 1
                && products.len() == 1,
            "one App grid did not share one tiled native product across every selected camera: {second:?}"
        );
        if let Some(simultaneous) = simultaneous {
            tokio::time::timeout(SIMULTANEOUS_VIEW_BARRIER_TIMEOUT, simultaneous.wait())
                .await
                .context("five-user camera grids did not become live concurrently")?;
        }
        let decode: DecodeIdentity = evaluate_console_app(
            &mut cdp,
            &target_id,
            &session_id,
            "uav-sim",
            APP_FRAME_DECODE_IDENTITY,
            true,
        )
        .await?;
        decode.validate()?;
        let screenshot_sha256 =
            capture_screenshot(&mut cdp, &session_id, screenshot_path).await?;
        cdp.assert_no_software_renderer_events()?;
        Ok(ConsoleLiveGridEvidence {
            schema: "veoveo.io/uav-console-live-grid/v1",
            captured_at: chrono::Utc::now(),
            page_url: page_url.to_owned(),
            screenshot_path: screenshot_path.display().to_string(),
            screenshot_sha256,
            hardware,
            videos: second,
            decode,
        })
    }
    .await;
    let release =
        close_console_live_views(&mut cdp, &target_id, &session_id, expected_camera_ids).await;
    let close = close_target(&mut cdp, &target_id).await;
    finish_live_capture(acceptance, release, close)
}

async fn capture_console_live_app_restart_inner<F, Fut, T>(
    cdp_base: &str,
    page_url: &str,
    expected_camera_id: &str,
    component: &str,
    screenshot_path: &Path,
    recovery_timeout: Duration,
    restart: F,
) -> Result<(ConsoleLiveRestartEvidence, T)>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let (mut cdp, target_id, session_id) = open_headed_target(cdp_base, page_url).await?;
    let acceptance = async {
        wait_for_document(&mut cdp, &session_id).await?;
        assert_page_visible(&mut cdp, &session_id).await?;
        let hardware: HardwareIdentity =
            cdp.evaluate(&session_id, HARDWARE_PREFLIGHT, true).await?;
        hardware.validate()?;
        ensure!(
            wait_for_console_app_camera(&mut cdp, &target_id, &session_id, expected_camera_id,)
                .await?,
            "Console UAV live view App exposed no camera {expected_camera_id:?}"
        );
        isolate_console_app_camera(&mut cdp, &target_id, &session_id, expected_camera_id).await?;
        let before =
            wait_for_console_video(&mut cdp, &target_id, &session_id, expected_camera_id).await?;
        let restart_evidence = restart()
            .await
            .with_context(|| format!("restarting live-view component {component}"))?;
        let after = wait_for_console_video_replacement(
            &mut cdp,
            &target_id,
            &session_id,
            expected_camera_id,
            &before,
            recovery_timeout,
        )
        .await?;
        let after = wait_for_console_video_advance(
            &mut cdp,
            &target_id,
            &session_id,
            expected_camera_id,
            after,
        )
        .await?;
        let final_hardware: HardwareIdentity =
            cdp.evaluate(&session_id, HARDWARE_PREFLIGHT, true).await?;
        final_hardware.validate()?;
        let screenshot_sha256 = capture_screenshot(&mut cdp, &session_id, screenshot_path).await?;
        cdp.assert_no_software_renderer_events()?;
        Ok((
            ConsoleLiveRestartEvidence {
                schema: "veoveo.io/uav-console-live-restart/v1",
                captured_at: chrono::Utc::now(),
                component: component.to_owned(),
                page_url: page_url.to_owned(),
                screenshot_path: screenshot_path.display().to_string(),
                screenshot_sha256,
                hardware: final_hardware,
                before: RestartVideoIdentity::from(&before),
                after: RestartVideoIdentity::from(&after),
            },
            restart_evidence,
        ))
    }
    .await;
    let release =
        close_console_live_view(&mut cdp, &target_id, &session_id, expected_camera_id).await;
    let close = close_target(&mut cdp, &target_id).await;
    finish_live_capture(acceptance, release, close)
}

fn finish_live_capture<T>(
    acceptance: Result<T>,
    release: Result<()>,
    close: Result<()>,
) -> Result<T> {
    match acceptance {
        Ok(evidence) => {
            release.context("closing live-view authorization after browser capture")?;
            close.context("closing headed browser target after live capture")?;
            Ok(evidence)
        }
        Err(error) => {
            let mut cleanup_failures = Vec::new();
            if let Err(release_error) = release {
                cleanup_failures.push(format!("live-view authorization: {release_error:#}"));
            }
            if let Err(close_error) = close {
                cleanup_failures.push(format!("browser target: {close_error:#}"));
            }
            if cleanup_failures.is_empty() {
                Err(error)
            } else {
                Err(error.context(format!(
                    "live-capture cleanup also failed: {}",
                    cleanup_failures.join("; ")
                )))
            }
        }
    }
}

async fn capture_console_stream_app_inner(
    cdp_base: &str,
    page_url: &str,
    screenshot_path: &Path,
) -> Result<ConsoleStreamCaptureEvidence> {
    let (mut cdp, target_id, session_id) = open_headed_target(cdp_base, page_url).await?;
    let acceptance = async {
        wait_for_document(&mut cdp, &session_id).await?;
        assert_page_visible(&mut cdp, &session_id).await?;
        let hardware: HardwareIdentity =
            cdp.evaluate(&session_id, HARDWARE_PREFLIGHT, true).await?;
        hardware.validate()?;
        ensure_console_stream_session(&mut cdp, &target_id, &session_id).await?;
        let first = wait_for_console_stream_video(&mut cdp, &target_id, &session_id).await?;
        let second =
            wait_for_console_stream_advance(&mut cdp, &target_id, &session_id, first).await?;
        let decode: StreamDecodeIdentity = evaluate_console_app(
            &mut cdp,
            &target_id,
            &session_id,
            "stream",
            STREAM_APP_DECODE_IDENTITY,
            true,
        )
        .await?;
        decode.validate(&second.decode_label)?;
        let console: Value = cdp
            .evaluate(
                &session_id,
                r#"(async () => {
                  const snapshot = await fetch("/console/api/snapshot", {credentials:"same-origin"});
                  const apps = await fetch("/console/api/apps", {credentials:"same-origin"});
                  return {
                    snapshotStatus:snapshot.status,
                    appsStatus:apps.status,
                    appFrameTitle:document.querySelector("iframe.app-frame")?.title ?? "",
                    bodyText:document.body?.innerText ?? ""
                  };
                })()"#,
                true,
            )
            .await?;
        ensure!(
            console.get("snapshotStatus").and_then(Value::as_u64) == Some(200)
                && console.get("appsStatus").and_then(Value::as_u64) == Some(200)
                && console
                    .get("appFrameTitle")
                    .and_then(Value::as_str)
                    .is_some_and(|title| title.contains("Live Monitor"))
                && console
                    .get("bodyText")
                    .and_then(Value::as_str)
                    .is_some_and(|body| body.contains("Live Monitor")),
            "real Console did not load its snapshot, App catalog, and Stream frame: {console}"
        );
        let screenshot_sha256 =
            capture_screenshot(&mut cdp, &session_id, screenshot_path).await?;
        cdp.assert_no_software_renderer_events()?;
        Ok(ConsoleStreamCaptureEvidence {
            schema: "veoveo.io/uav-console-stream-capture/v1",
            captured_at: chrono::Utc::now(),
            page_url: page_url.to_owned(),
            screenshot_path: screenshot_path.display().to_string(),
            screenshot_sha256,
            hardware,
            stream: second,
            decode,
        })
    }
    .await;
    let close = close_target(&mut cdp, &target_id).await;
    let evidence = acceptance?;
    close?;
    Ok(evidence)
}

async fn ensure_console_stream_session(
    cdp: &mut Cdp,
    target_id: &str,
    session_id: &str,
) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        let action: String = evaluate_console_app(
            cdp,
            target_id,
            session_id,
            "stream",
            STREAM_APP_ENSURE_SESSION,
            false,
        )
        .await?;
        match action.as_str() {
            "available" | "started" | "starting" => return Ok(()),
            "waiting" if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            "waiting" => bail!("Console Stream App did not load a live-input pipeline"),
            unexpected => {
                bail!("Console Stream App returned an invalid start state {unexpected:?}")
            }
        }
    }
}

async fn capture_console_recording_inner(
    cdp_base: &str,
    page_url: &str,
    recording_id: &str,
    screenshot_path: &Path,
) -> Result<ConsoleRecordingCaptureEvidence> {
    let (mut cdp, target_id, session_id) = open_headed_target(cdp_base, page_url).await?;
    let acceptance = async {
        wait_for_document(&mut cdp, &session_id).await?;
        assert_page_visible(&mut cdp, &session_id).await?;
        let hardware: HardwareIdentity =
            cdp.evaluate(&session_id, HARDWARE_PREFLIGHT, true).await?;
        hardware.validate()?;
        wait_for_console_recording_surface(&mut cdp, &session_id, recording_id).await?;
        let (initial_live_follow, initial_follow_seconds) =
            wait_for_rerun_live_follow(&mut cdp, &session_id).await?;
        let initial_responsiveness = sample_rerun_responsiveness(&mut cdp, &session_id).await?;
        initial_responsiveness.validate()?;
        let mut live_follow = verify_rerun_live_stability(
            &mut cdp,
            &session_id,
            initial_live_follow.clone(),
            initial_follow_seconds,
            Duration::from_secs(120),
        )
        .await?;
        let reconnected = verify_rerun_live_reconnect(
            &mut cdp,
            &session_id,
            &initial_live_follow,
        )
        .await?;
        live_follow.update_from(&reconnected)?;
        let final_responsiveness = sample_rerun_responsiveness(&mut cdp, &session_id).await?;
        final_responsiveness.validate()?;
        ensure!(
            final_responsiveness.js_heap_used_bytes
                <= initial_responsiveness
                    .js_heap_used_bytes
                    .saturating_add(512 * 1024 * 1024),
            "Rerun browser heap grew without a bound during live playback: {initial_responsiveness:?} -> {final_responsiveness:?}"
        );
        assert_page_visible(&mut cdp, &session_id).await?;
        let final_hardware: HardwareIdentity =
            cdp.evaluate(&session_id, HARDWARE_PREFLIGHT, true).await?;
        final_hardware.validate()?;
        let viewer_bounds = rerun_viewer_bounds(&mut cdp, &session_id).await?;
        let before_path = screenshot_path.with_extension("camera-before.png");
        capture_screenshot(&mut cdp, &session_id, &before_path).await?;
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert_page_visible(&mut cdp, &session_id).await?;
        let screenshot_sha256 = capture_screenshot(&mut cdp, &session_id, screenshot_path).await?;
        let render = analyze_rerun_render(screenshot_path, viewer_bounds)?;
        render.validate()?;
        let camera_render =
            analyze_rerun_camera_render(&before_path, screenshot_path, viewer_bounds)?;
        camera_render.validate()?;
        let final_live_state: RerunLiveFollowState = cdp
            .evaluate(&session_id, RERUN_LIVE_FOLLOW_STATE, false)
            .await?;
        final_live_state.validate_surface()?;
        live_follow.update_from(&final_live_state)?;
        let network = cdp.recording_playback_network_evidence(
            recording_id,
            RecordingPlaybackMode::Live,
        )?;
        cdp.assert_no_software_renderer_events()?;
        Ok(ConsoleRecordingCaptureEvidence {
            schema: "veoveo.io/uav-console-recording-capture/v6",
            captured_at: chrono::Utc::now(),
            page_url: page_url.to_owned(),
            recording_id: recording_id.to_owned(),
            screenshot_path: screenshot_path.display().to_string(),
            screenshot_sha256,
            hardware: final_hardware,
            live_follow,
            network,
            render,
            camera_render,
            initial_responsiveness,
            final_responsiveness,
        })
    }
    .await;
    let close = close_target(&mut cdp, &target_id).await;
    let evidence = acceptance?;
    close?;
    Ok(evidence)
}

#[allow(dead_code)]
async fn capture_console_recording_archive_inner(
    cdp_base: &str,
    page_url: &str,
    recording_id: &str,
    screenshot_path: &Path,
) -> Result<ConsoleRecordingArchiveCaptureEvidence> {
    let (mut cdp, target_id, session_id) = open_headed_target(cdp_base, page_url).await?;
    let acceptance = async {
        wait_for_document(&mut cdp, &session_id).await?;
        assert_page_visible(&mut cdp, &session_id).await?;
        let hardware: HardwareIdentity =
            cdp.evaluate(&session_id, HARDWARE_PREFLIGHT, true).await?;
        hardware.validate()?;
        wait_for_console_recording_surface(&mut cdp, &session_id, recording_id).await?;

        let network_deadline = tokio::time::Instant::now() + Duration::from_secs(120);
        let network = loop {
            match cdp
                .recording_playback_network_evidence(recording_id, RecordingPlaybackMode::Archive)
            {
                Ok(network) => break network,
                Err(_) if tokio::time::Instant::now() < network_deadline => {
                    assert_page_visible(&mut cdp, &session_id).await?;
                    cdp.assert_no_software_renderer_events()?;
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
                Err(error) => return Err(error).context("archived Rerun transport did not settle"),
            }
        };
        let timeline = wait_for_rerun_archive_latest(&mut cdp, &session_id).await?;
        let responsiveness = sample_rerun_responsiveness(&mut cdp, &session_id).await?;
        responsiveness.validate()?;
        let viewer_bounds = rerun_viewer_bounds(&mut cdp, &session_id).await?;
        let screenshot_sha256 = capture_screenshot(&mut cdp, &session_id, screenshot_path).await?;
        let render = analyze_rerun_render(screenshot_path, viewer_bounds)?;
        render.validate()?;
        let camera_render = analyze_rerun_camera_frame(screenshot_path, viewer_bounds)?;
        let final_hardware: HardwareIdentity =
            cdp.evaluate(&session_id, HARDWARE_PREFLIGHT, true).await?;
        final_hardware.validate()?;
        cdp.assert_no_software_renderer_events()?;
        Ok(ConsoleRecordingArchiveCaptureEvidence {
            schema: "veoveo.io/uav-console-recording-archive-capture/v2",
            captured_at: chrono::Utc::now(),
            page_url: page_url.to_owned(),
            recording_id: recording_id.to_owned(),
            screenshot_path: screenshot_path.display().to_string(),
            screenshot_sha256,
            hardware: final_hardware,
            network,
            timeline,
            render,
            camera_render,
            responsiveness,
        })
    }
    .await;
    let close = close_target(&mut cdp, &target_id).await;
    let evidence = acceptance?;
    close?;
    Ok(evidence)
}

async fn wait_for_rerun_archive_latest(
    cdp: &mut Cdp,
    session_id: &str,
) -> Result<RecordingArchiveTimelineEvidence> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let state: Value = cdp
            .evaluate(
                session_id,
                r#"(() => {
                  const host = document.querySelector(".rerun-web-viewer-host");
                  return {
                    archiveState: host?.dataset.rerunArchiveState ?? "",
                    recordingId: host?.dataset.rerunRecordingId ?? "",
                    timeline: host?.dataset.rerunTimeline ?? "",
                    currentTime: Number(host?.dataset.rerunCurrentTime ?? NaN),
                    newestTime: Number(host?.dataset.rerunNewestTime ?? NaN),
                    error: document.querySelector(".recording-viewer-error")?.textContent ?? ""
                  };
                })()"#,
                false,
            )
            .await?;
        ensure!(
            state
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("")
                .is_empty(),
            "Console Rerun viewer failed while selecting archive time: {state}"
        );
        if state.get("archiveState").and_then(Value::as_str) == Some("latest") {
            let evidence = serde_json::from_value::<RecordingArchiveTimelineEvidence>(state)?;
            ensure!(
                !evidence.recording_id.is_empty()
                    && evidence.timeline == "simulation_time"
                    && evidence.current_time.is_finite()
                    && evidence.newest_time.is_finite()
                    && evidence.current_time > 0.0
                    && (evidence.current_time - evidence.newest_time).abs() <= 1.0,
                "Rerun archive did not select its latest simulation time: {evidence:?}"
            );
            return Ok(evidence);
        }
        ensure!(
            tokio::time::Instant::now() < deadline,
            "Rerun archive did not select its latest simulation time: {state}"
        );
        assert_page_visible(cdp, session_id).await?;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn wait_for_console_recording_surface(
    cdp: &mut Cdp,
    session_id: &str,
    recording_id: &str,
) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(300);
    loop {
        let state: Value = cdp
            .evaluate(
                session_id,
                r#"(() => ({
                  recordingVisible:document.body?.innerText?.toLowerCase()
                    .includes("recording://recordings/") ?? false,
                  canvasCount:document.querySelectorAll(".rerun-web-viewer-host canvas").length,
                  loading:Boolean(document.querySelector(".recording-viewer-state")),
                  error:document.querySelector(".recording-viewer-error")?.textContent ?? "",
                  mapError:document.querySelector(".recording-viewer-map-error")?.textContent ?? "",
                  bodyText:document.body?.innerText ?? ""
                }))()"#,
                false,
            )
            .await?;
        let body = state
            .get("bodyText")
            .and_then(Value::as_str)
            .unwrap_or_default();
        ensure!(
            state
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("")
                .is_empty(),
            "Console Rerun viewer failed: {state}"
        );
        ensure!(
            state
                .get("mapError")
                .and_then(Value::as_str)
                .unwrap_or("")
                .is_empty(),
            "Console Rerun map provider failed: {state}"
        );
        ensure!(
            !software_renderer(&body.to_ascii_lowercase()),
            "Console Rerun viewer exposed a software-renderer warning"
        );
        if state.get("recordingVisible").and_then(Value::as_bool) == Some(true)
            && state
                .get("canvasCount")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                > 0
            && state.get("loading").and_then(Value::as_bool) == Some(false)
            && body.contains(recording_id)
        {
            return Ok(());
        }
        ensure!(
            tokio::time::Instant::now() < deadline,
            "Console did not render governed recording {recording_id}: {state}"
        );
        assert_page_visible(cdp, session_id).await?;
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn rerun_viewer_bounds(cdp: &mut Cdp, session_id: &str) -> Result<ElementBounds> {
    cdp.evaluate(
        session_id,
        r#"(() => {
            const bounds = document.querySelector(".rerun-web-viewer-host")
              ?.getBoundingClientRect();
            if (!bounds) return null;
            return {
              x: bounds.x,
              y: bounds.y,
              width: bounds.width,
              height: bounds.height,
              viewportWidth: window.innerWidth,
              viewportHeight: window.innerHeight
            };
        })()"#,
        false,
    )
    .await
    .context("Console did not expose the Rerun viewport bounds")
}

async fn wait_for_rerun_live_follow(
    cdp: &mut Cdp,
    session_id: &str,
) -> Result<(RerunLiveFollowState, f64)> {
    let started = tokio::time::Instant::now();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let state: RerunLiveFollowState = cdp
            .evaluate(session_id, RERUN_LIVE_FOLLOW_STATE, false)
            .await?;
        state.validate_transport_surface()?;
        if state.has_live_timeline() && state.is_current() {
            state.validate_surface()?;
            let elapsed = started.elapsed().as_secs_f64();
            ensure!(
                elapsed <= 2.0,
                "Rerun needed {elapsed:.3}s to enter native Following mode after its live surface opened"
            );
            return Ok((state, elapsed));
        }
        ensure!(
            tokio::time::Instant::now() < deadline,
            "Rerun did not reach the newest live recording time: {state:?}"
        );
        assert_page_visible(cdp, session_id).await?;
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn verify_rerun_live_stability(
    cdp: &mut Cdp,
    session_id: &str,
    initial: RerunLiveFollowState,
    initial_follow_seconds: f64,
    duration: Duration,
) -> Result<RerunLiveFollowEvidence> {
    let deadline = tokio::time::Instant::now() + duration;
    let final_state = loop {
        assert_page_visible(cdp, session_id).await?;
        cdp.assert_no_software_renderer_events()?;
        let state: RerunLiveFollowState = cdp
            .evaluate(session_id, RERUN_LIVE_FOLLOW_STATE, false)
            .await?;
        state.validate_surface()?;
        ensure!(
            state.document_epoch_ms == initial.document_epoch_ms
                && state.viewer_instance == initial.viewer_instance
                && state.recording_id == initial.recording_id,
            "Rerun remounted or replaced its live recording while following it: \
             {initial:?} -> {state:?}"
        );
        if tokio::time::Instant::now() >= deadline {
            break state;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    };
    ensure!(
        final_state.is_current()
            && final_state.time_update_count > initial.time_update_count
            && final_state.current_time > initial.current_time
            && final_state.newest_time > initial.newest_time,
        "Rerun did not remain current and advancing through the live stability window: \
         {initial:?} -> {final_state:?}"
    );
    Ok(RerunLiveFollowEvidence {
        initial_follow_seconds,
        stability_seconds: duration.as_secs(),
        viewer_instance: final_state.viewer_instance,
        recording_id: final_state.recording_id,
        timeline: final_state.timeline,
        initial_time: initial.current_time,
        final_time: final_state.current_time,
        final_newest_time: final_state.newest_time,
        final_lag_seconds: final_state.lag_seconds,
        initial_time_update_count: initial.time_update_count,
        final_time_update_count: final_state.time_update_count,
        final_live_connection_count: final_state.live_connection_count,
        initial_live_frame_count: initial.live_frame_count,
        final_live_frame_count: final_state.live_frame_count,
        newest_frame_bytes: final_state.newest_frame_bytes,
    })
}

async fn verify_rerun_live_reconnect(
    cdp: &mut Cdp,
    session_id: &str,
    identity: &RerunLiveFollowState,
) -> Result<RerunLiveFollowState> {
    let connected_before: RerunLiveFollowState = cdp
        .evaluate(session_id, RERUN_LIVE_FOLLOW_STATE, false)
        .await?;
    connected_before.validate_surface()?;
    ensure!(
        connected_before.document_epoch_ms == identity.document_epoch_ms
            && connected_before.viewer_instance == identity.viewer_instance
            && connected_before.recording_id == identity.recording_id,
        "Rerun replaced its viewer before reconnect acceptance: {identity:?} -> {connected_before:?}"
    );
    let _: Value = cdp
        .evaluate(
            session_id,
            "(() => { window.dispatchEvent(new Event('online')); return true; })()",
            false,
        )
        .await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let healthy_signal: RerunLiveFollowState = cdp
        .evaluate(session_id, RERUN_LIVE_FOLLOW_STATE, false)
        .await?;
    healthy_signal.validate_surface()?;
    ensure!(
        healthy_signal.live_connection_count == connected_before.live_connection_count,
        "a healthy network signal churned the Rerun live connection: {connected_before:?} -> {healthy_signal:?}"
    );

    cdp.command(
        "Network.emulateNetworkConditions",
        serde_json::json!({
            "offline": true,
            "latency": 0,
            "downloadThroughput": -1,
            "uploadThroughput": -1,
        }),
        Some(session_id),
    )
    .await?;
    let disconnect_result = async {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        loop {
            let state: RerunLiveFollowState = cdp
                .evaluate(session_id, RERUN_LIVE_FOLLOW_STATE, false)
                .await?;
            if state.live_state == "error" && !state.error.is_empty() {
                break Ok::<(), anyhow::Error>(());
            }
            ensure!(
                tokio::time::Instant::now() < deadline,
                "Rerun live transport did not expose the network loss: {connected_before:?} -> {state:?}"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    .await;
    let restore_result = cdp
        .command(
            "Network.emulateNetworkConditions",
            serde_json::json!({
                "offline": false,
                "latency": 0,
                "downloadThroughput": -1,
                "uploadThroughput": -1,
            }),
            Some(session_id),
        )
        .await;
    restore_result?;
    disconnect_result?;
    let _: Value = cdp
        .evaluate(
            session_id,
            "(() => { window.dispatchEvent(new Event('online')); return true; })()",
            false,
        )
        .await?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let state: RerunLiveFollowState = cdp
            .evaluate(session_id, RERUN_LIVE_FOLLOW_STATE, false)
            .await?;
        ensure!(
            state.document_epoch_ms == connected_before.document_epoch_ms
                && state.viewer_instance == connected_before.viewer_instance
                && state.recording_id == connected_before.recording_id,
            "Rerun rebuilt its viewer while reconnecting the live transport: {connected_before:?} -> {state:?}"
        );
        if state.live_connection_count > connected_before.live_connection_count
            && state.live_frame_count > connected_before.live_frame_count
            && state.time_update_count > connected_before.time_update_count
            && state.is_current()
            && state.live_state == "connected"
        {
            state.validate_surface()?;
            return Ok(state);
        }
        ensure!(
            tokio::time::Instant::now() < deadline,
            "Rerun did not reactively reconnect its persistent live channel: {connected_before:?} -> {state:?}"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RerunResponsivenessEvidence {
    animation_frames: u64,
    mean_frame_ms: f64,
    maximum_frame_ms: f64,
    js_heap_used_bytes: u64,
}

impl RerunResponsivenessEvidence {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.animation_frames == 120
                && self.mean_frame_ms <= 50.0
                && self.maximum_frame_ms <= 250.0
                && self.js_heap_used_bytes > 0,
            "Rerun live viewer is not interactively responsive: {self:?}"
        );
        Ok(())
    }
}

async fn sample_rerun_responsiveness(
    cdp: &mut Cdp,
    session_id: &str,
) -> Result<RerunResponsivenessEvidence> {
    let evidence: RerunResponsivenessEvidence = cdp
        .evaluate(
            session_id,
            r#"(async () => {
              const gaps=[];
              await new Promise((resolve) => {
                let previous;
                const frame=(now) => {
                  if (previous !== undefined) gaps.push(now-previous);
                  previous=now;
                  if (gaps.length >= 120) resolve(); else requestAnimationFrame(frame);
                };
                requestAnimationFrame(frame);
              });
              const heap=performance.memory?.usedJSHeapSize ?? 0;
              return {
                animationFrames:gaps.length,
                meanFrameMs:gaps.reduce((sum,value)=>sum+value,0)/gaps.length,
                maximumFrameMs:Math.max(...gaps),
                jsHeapUsedBytes:heap
              };
            })()"#,
            true,
        )
        .await?;
    Ok(evidence)
}

/// Exercise the exact generated Map MCP App in the same opaque-origin sandbox
/// and explicit tile-origin CSP used by Console. The local parent implements
/// only the MCP App bridge calls needed to render one immutable composition.
#[allow(dead_code, reason = "shared by the focused browser-smoke binary")]
pub(crate) async fn capture_map_workspace_app(
    cdp_base: &str,
    app_html_path: &Path,
    screenshot_path: &Path,
    timeout: Duration,
) -> Result<MapWorkspaceCaptureEvidence> {
    ensure!(
        app_html_path.is_file(),
        "generated Map workspace App does not exist: {}",
        app_html_path.display()
    );
    let app_html = fs::read(app_html_path)
        .with_context(|| format!("reading Map workspace App {}", app_html_path.display()))?;
    ensure!(
        app_html
            .windows("maplibre-gl@6.6.0".len())
            .any(|window| window == b"maplibre-gl@6.6.0"),
        "Map workspace acceptance requires the pinned MapLibre 6.6.0 artifact"
    );
    let (page_url, server) = serve_map_workspace_acceptance(app_html).await?;
    let (mut cdp, target_id, session_id) = open_headed_target(cdp_base, &page_url).await?;
    let acceptance: Result<(
        HardwareIdentity,
        MapWorkspaceBridgeEvidence,
        Value,
        MapWorkspaceRenderEvidence,
        String,
    )> = async {
        wait_for_document(&mut cdp, &session_id).await?;
        assert_page_visible(&mut cdp, &session_id).await?;
        let hardware: HardwareIdentity =
            cdp.evaluate(&session_id, HARDWARE_PREFLIGHT, true).await?;
        hardware.validate()?;

        let deadline = tokio::time::Instant::now() + timeout;
        let bridge = loop {
            let bridge: MapWorkspaceBridgeEvidence = cdp
                .evaluate(&session_id, "window.__mapWorkspaceEvidence", false)
                .await?;
            ensure!(
                bridge.errors.is_empty(),
                "Map workspace bridge failed: {:?}",
                bridge.errors
            );
            if bridge.initialized
                && bridge.authored_query_calls > 0
                && bridge.source_query_calls > 0
                && bridge.query_responses >= 2
            {
                break bridge;
            }
            ensure!(
                tokio::time::Instant::now() < deadline,
                "Map workspace did not initialize and complete a viewport query within {timeout:?}: {bridge:?}"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        };
        ensure!(
            bridge.size_height >= 500.0,
            "Map workspace did not report a usable App height: {}",
            bridge.size_height
        );
        ensure!(
            bridge
                .last_authored_query
                .get("publication_id")
                .and_then(Value::as_str)
                == Some("publication-0198-map-workspace"),
            "Map workspace did not query the immutable publication pin: {}",
            bridge.last_authored_query
        );
        ensure!(
            bridge
                .last_authored_query
                .get("limit")
                .and_then(Value::as_u64)
                == Some(1_000),
            "Map workspace viewport query must use the bounded 1,000-feature page: {}",
            bridge.last_authored_query
        );
        let bbox = bridge
            .last_authored_query
            .get("bbox")
            .context("Map workspace viewport query omitted bbox")?;
        for coordinate in ["west", "south", "east", "north"] {
            ensure!(
                bbox.get(coordinate).and_then(Value::as_f64).is_some_and(f64::is_finite),
                "Map workspace viewport query had an invalid {coordinate} bound: {bbox}"
            );
        }
        ensure!(
            bridge
                .last_source_query
                .get("limit")
                .and_then(Value::as_u64)
                == Some(500),
            "Map workspace source preview must use the bounded 500-feature page: {}",
            bridge.last_source_query
        );
        ensure!(
            bridge.last_source_query.pointer("/spatial/kind").and_then(Value::as_str)
                == Some("bounding_box"),
            "Map workspace source preview omitted its indexed bounding-box query: {}",
            bridge.last_source_query
        );

        let light = loop {
            let value: Value = evaluate_console_app(
                &mut cdp,
                &target_id,
                &session_id,
                "map",
                r#"({
                  theme:document.documentElement.dataset.theme ?? '',
                  basemapTheme:document.getElementById('map')?.dataset.basemapTheme ?? '',
                  basemapAvailable:document.getElementById('map')?.dataset.basemapAvailable ?? '',
                  viewport:document.getElementById('viewport')?.textContent ?? '',
                  renderedFeatureCount:Number(document.getElementById('map')?.dataset.renderedFeatureCount ?? 0)
                })"#,
                false,
            )
            .await?;
            if value.get("theme").and_then(Value::as_str) == Some("light")
                && value.get("basemapTheme").and_then(Value::as_str) == Some("light")
                && value.get("basemapAvailable").and_then(Value::as_str) == Some("true")
                && value
                    .get("renderedFeatureCount")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    > 0
            {
                break value;
            }
            ensure!(
                tokio::time::Instant::now() < deadline,
                "Map workspace did not initialize its light basemap and overlays: {value}"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        };
        let switched: bool = cdp
            .evaluate(
                &session_id,
                r#"(() => {
                  const frame=document.getElementById('app');
                  frame?.contentWindow?.postMessage({
                    jsonrpc:'2.0',method:'ui/notifications/host-context-changed',
                    params:{hostContext:{theme:'dark'}}
                  },'*');
                  return Boolean(frame);
                })()"#,
                false,
            )
            .await?;
        ensure!(switched, "Map workspace acceptance frame disappeared before theme switching");
        let dark = loop {
            let value: Value = evaluate_console_app(
                &mut cdp,
                &target_id,
                &session_id,
                "map",
                r#"({
                  theme:document.documentElement.dataset.theme ?? '',
                  basemapTheme:document.getElementById('map')?.dataset.basemapTheme ?? '',
                  basemapAvailable:document.getElementById('map')?.dataset.basemapAvailable ?? '',
                  viewport:document.getElementById('viewport')?.textContent ?? '',
                  renderedFeatureCount:Number(document.getElementById('map')?.dataset.renderedFeatureCount ?? 0)
                })"#,
                false,
            )
            .await?;
            if value.get("theme").and_then(Value::as_str) == Some("dark")
                && value.get("basemapTheme").and_then(Value::as_str) == Some("dark")
                && value.get("basemapAvailable").and_then(Value::as_str) == Some("true")
                && value
                    .get("renderedFeatureCount")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    > 0
            {
                break value;
            }
            ensure!(
                tokio::time::Instant::now() < deadline,
                "Map workspace did not reactively restore its overlays on the dark basemap: {value}"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        };
        ensure!(
            dark.get("viewport") == light.get("viewport"),
            "Map workspace changed its camera while switching basemap theme: {light} -> {dark}"
        );

        // GeoJSON source updates are worker-driven. Give MapLibre enough time
        // to place and paint the returned geometries before capture.
        tokio::time::sleep(Duration::from_secs(3)).await;
        assert_page_visible(&mut cdp, &session_id).await?;
        cdp.assert_no_software_renderer_events()?;
        let screenshot_sha256 =
            capture_screenshot(&mut cdp, &session_id, screenshot_path).await?;
        let render = analyze_map_workspace_render(screenshot_path)?;
        let bridge: MapWorkspaceBridgeEvidence = cdp
            .evaluate(&session_id, "window.__mapWorkspaceEvidence", false)
            .await?;
        ensure!(
            bridge.authored_query_calls >= 2 && bridge.source_query_calls >= 2,
            "Map workspace did not refresh both spatial previews after its theme switch: {bridge:?}"
        );
        Ok((hardware, bridge, dark, render, screenshot_sha256))
    }
    .await;
    let acceptance = match acceptance {
        Ok(evidence) => Ok(evidence),
        Err(error) => {
            let diagnostics = cdp.stream_diagnostics(&session_id).await?;
            Err(error.context(diagnostics))
        }
    };
    let close = close_target(&mut cdp, &target_id).await;
    server.abort();
    let (hardware, bridge, theme, render, screenshot_sha256) = acceptance?;
    close?;
    Ok(MapWorkspaceCaptureEvidence {
        schema: "veoveo.io/map-workspace-capture-evidence/v2",
        captured_at: chrono::Utc::now(),
        page_url,
        screenshot_path: screenshot_path.display().to_string(),
        screenshot_sha256,
        hardware,
        bridge,
        theme,
        render,
    })
}

/// Exercise the deployed Map workspace through the authenticated public Console.
/// The caller supplies stable live fixture titles created through the public MCP
/// surface so the browser proof cannot silently render unrelated tenant data.
#[allow(dead_code)]
pub(crate) async fn capture_console_map_workspace_app(
    cdp_base: &str,
    public_base_url: &str,
    expected_composition_title: &str,
    expected_layer_title: &str,
    screenshot_directory: &Path,
    timeout: Duration,
) -> Result<ConsoleMapWorkspaceCaptureEvidence> {
    let page_url = console_acceptance_url(public_base_url, "/apps/map/workspace.html");
    tokio::time::timeout(
        timeout,
        capture_console_map_workspace_app_inner(
            cdp_base,
            &page_url,
            expected_composition_title,
            expected_layer_title,
            screenshot_directory,
            timeout,
        ),
    )
    .await
    .with_context(|| format!("Console Map workspace capture exceeded {timeout:?}"))?
}

async fn capture_console_map_workspace_app_inner(
    cdp_base: &str,
    page_url: &str,
    expected_composition_title: &str,
    expected_layer_title: &str,
    screenshot_directory: &Path,
    timeout: Duration,
) -> Result<ConsoleMapWorkspaceCaptureEvidence> {
    let (mut cdp, target_id, session_id) = open_headed_target(cdp_base, page_url).await?;
    let acceptance = async {
        wait_for_document(&mut cdp, &session_id).await?;
        assert_page_visible(&mut cdp, &session_id).await?;
        let hardware: HardwareIdentity =
            cdp.evaluate(&session_id, HARDWARE_PREFLIGHT, true).await?;
        hardware.validate()?;
        wait_for_console_app_body(
            &mut cdp,
            &target_id,
            &session_id,
            "map",
            "Workspace",
        )
        .await?;

        let composition = serde_json::to_string(expected_composition_title)?;
        let layer = serde_json::to_string(expected_layer_title)?;
        let deadline = tokio::time::Instant::now() + timeout;
        let workspace_script = || {
            format!(
                r#"(() => {{
                  const select=document.getElementById('composition-select');
                  const option=[...(select?.options ?? [])].find(item=>item.textContent?.includes({composition}));
                  if(option && select.value!==option.value){{select.value=option.value;select.dispatchEvent(new Event('change',{{bubbles:true}}));}}
                  const layerRow=[...document.querySelectorAll('#authored-list .layer-row')].find(item=>item.textContent?.includes({layer}));
                  if(layerRow && !layerRow.classList.contains('selected')) layerRow.click();
                  const canvas=document.querySelector('#map canvas');
                  const gl=canvas?.getContext('webgl2');
                  const debug=gl?.getExtension('WEBGL_debug_renderer_info');
                  const mapRect=document.querySelector('.map-stage')?.getBoundingClientRect();
                  return {{
                    compositionFound:Boolean(option),selectedTitle:option?.textContent?.trim() ?? '',
                    layerFound:Boolean(layerRow),layerTitle:layerRow?.textContent?.trim() ?? '',
                    status:document.getElementById('status-message')?.textContent?.trim() ?? '',
                    gpuBadge:document.getElementById('gpu-badge')?.textContent?.trim() ?? '',
                    mapFailureHidden:Boolean(document.getElementById('map-failure')?.hidden),
                    canvasWidth:canvas?.width ?? 0,canvasHeight:canvas?.height ?? 0,
                    mapWidth:mapRect?.width ?? 0,mapHeight:mapRect?.height ?? 0,
                    webglRenderer:debug ? gl.getParameter(debug.UNMASKED_RENDERER_WEBGL) : '',
                    authoredRows:document.querySelectorAll('#authored-list .layer-row').length,
                    sourceRows:document.querySelectorAll('#source-layer-list .layer-row').length,
                    previewRows:document.querySelectorAll('#preview-rows tr').length,
                    previewText:document.getElementById('preview-rows')?.textContent?.trim() ?? '',
                    renderedFeatureCount:Number(document.getElementById('map')?.dataset.renderedFeatureCount ?? 0),
                    attribution:document.querySelector('.maplibregl-ctrl-attrib')?.textContent?.trim() ?? '',
                    viewerInteracted:document.documentElement.dataset.veoveoAcceptanceMapInteracted === 'true'
                  }};
                }})()"#
            )
        };
        let workspace_ready = |value: &Value| {
            value.get("compositionFound").and_then(Value::as_bool) == Some(true)
                && value.get("layerFound").and_then(Value::as_bool) == Some(true)
                && value.get("mapFailureHidden").and_then(Value::as_bool) == Some(true)
                && value.get("canvasWidth").and_then(Value::as_u64).unwrap_or(0) >= 600
                && value.get("canvasHeight").and_then(Value::as_u64).unwrap_or(0) >= 360
                && value.get("mapWidth").and_then(Value::as_f64).unwrap_or(0.0) >= 600.0
                && value.get("mapHeight").and_then(Value::as_f64).unwrap_or(0.0) >= 360.0
                && value
                    .get("webglRenderer")
                    .and_then(Value::as_str)
                    .is_some_and(|renderer| {
                        renderer.to_ascii_lowercase().contains("nvidia")
                            && !software_renderer(&renderer.to_ascii_lowercase())
                    })
                && value.get("previewRows").and_then(Value::as_u64).unwrap_or(0) > 0
                && value
                    .get("previewText")
                    .and_then(Value::as_str)
                    .is_some_and(|text| text.contains("ALPHA-7"))
                && value
                    .get("renderedFeatureCount")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    > 0
                && value
                    .get("status")
                    .and_then(Value::as_str)
                    .is_some_and(|status| status.contains("visible across"))
                && !value
                    .get("attribution")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .is_empty()
        };
        loop {
            let value: Value = evaluate_console_app(
                &mut cdp,
                &target_id,
                &session_id,
                "map",
                &workspace_script(),
                false,
            )
            .await?;
            if workspace_ready(&value) {
                break;
            }
            ensure!(
                tokio::time::Instant::now() < deadline,
                "live unified Map workspace did not render its composition, basemap, and synchronized preview: {value}"
            );
            tokio::time::sleep(Duration::from_millis(250)).await;
        };
        let toggled: bool = evaluate_console_app(
            &mut cdp,
            &target_id,
            &session_id,
            "map",
            &format!(
                r#"(() => {{
                  const findVisibility=()=>[...document.querySelectorAll('#authored-list .layer-row')]
                    .find(item=>item.textContent?.includes({layer}))?.querySelector('input[type="checkbox"]');
                  const visibility=findVisibility();
                  if(!visibility?.checked) return false;
                  visibility.click();
                  const hiddenVisibility=findVisibility();
                  if(!hiddenVisibility || hiddenVisibility.checked) return false;
                  hiddenVisibility.click();
                  const restoredVisibility=findVisibility();
                  document.documentElement.dataset.veoveoAcceptanceMapInteracted='true';
                  return Boolean(restoredVisibility?.checked);
                }})()"#
            ),
            false,
        )
        .await?;
        ensure!(toggled, "live unified Map workspace did not hide and restore its authored layer");
        let workspace = loop {
            let value = evaluate_console_app(
                &mut cdp,
                &target_id,
                &session_id,
                "map",
                &workspace_script(),
                false,
            )
            .await?;
            if workspace_ready(&value)
                && value.get("viewerInteracted").and_then(Value::as_bool) == Some(true)
            {
                break value;
            }
            ensure!(
                tokio::time::Instant::now() < deadline,
                "live unified Map workspace did not recover after the visibility interaction: {value}"
            );
            tokio::time::sleep(Duration::from_millis(250)).await;
        };
        cdp.assert_no_software_renderer_events()?;
        let workspace_screenshot = screenshot_directory.join("workspace.png");
        let workspace_screenshot_sha256 =
            capture_screenshot(&mut cdp, &session_id, &workspace_screenshot).await?;
        let render = analyze_map_workspace_render(&workspace_screenshot)?;

        let source = loop {
            let value: Value = evaluate_console_app(
                &mut cdp,
                &target_id,
                &session_id,
                "map",
                r#"(() => {
                  const row=document.querySelector('#source-layer-list .layer-row');
                  if(row && !row.classList.contains('selected')) row.click();
                  const zoom=[...document.querySelectorAll('#inspector-body button')]
                    .find(item=>item.textContent?.trim()==='Zoom to data');
                  if(zoom && document.documentElement.dataset.veoveoAcceptanceSourceZoomed!=='true'){
                    zoom.click();
                    document.documentElement.dataset.veoveoAcceptanceSourceZoomed='true';
                  }
                  const mapRect=document.querySelector('.map-stage')?.getBoundingClientRect();
                  return {
                    rowFound:Boolean(row),rowSelected:Boolean(row?.classList.contains('selected')),
                    layerTitle:row?.textContent?.trim() ?? '',
                    inspectorTitle:document.getElementById('inspector-title')?.textContent?.trim() ?? '',
                    inspectorText:document.getElementById('inspector-body')?.textContent?.trim() ?? '',
                    previewRows:document.querySelectorAll('#preview-rows tr').length,
                    previewText:document.getElementById('preview-rows')?.textContent?.trim() ?? '',
                    mapWidth:mapRect?.width ?? 0,mapHeight:mapRect?.height ?? 0,
                    renderedFeatureCount:Number(document.getElementById('map')?.dataset.renderedFeatureCount ?? 0)
                  };
                })()"#,
                false,
            )
            .await?;
            if value.get("rowFound").and_then(Value::as_bool) == Some(true)
                && value.get("rowSelected").and_then(Value::as_bool) == Some(true)
                && value.get("inspectorTitle").and_then(Value::as_str) == Some("Source release")
                && value
                    .get("inspectorText")
                    .and_then(Value::as_str)
                    .is_some_and(|text| text.contains("Active governed release"))
                && value.get("previewRows").and_then(Value::as_u64).unwrap_or(0) > 0
                && !value
                    .get("previewText")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .is_empty()
                && value.get("mapWidth").and_then(Value::as_f64).unwrap_or(0.0) >= 600.0
                && value.get("mapHeight").and_then(Value::as_f64).unwrap_or(0.0) >= 360.0
                && value
                    .get("renderedFeatureCount")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    > 0
            {
                break value;
            }
            ensure!(
                tokio::time::Instant::now() < deadline,
                "live unified Map workspace did not render and preview an active source release: {value}"
            );
            tokio::time::sleep(Duration::from_millis(250)).await;
        };
        let source_screenshot = screenshot_directory.join("source-release.png");
        let source_screenshot_sha256 =
            capture_screenshot(&mut cdp, &session_id, &source_screenshot).await?;

        let feature = loop {
            let value: Value = evaluate_console_app(
                &mut cdp,
                &target_id,
                &session_id,
                "map",
                &format!(r#"(() => {{
                  const layer=[...document.querySelectorAll('#authored-list .layer-row')]
                    .find(item=>item.textContent?.includes({layer}));
                  if(layer && !layer.classList.contains('selected')) layer.click();
                  if(layer && document.documentElement.dataset.veoveoAcceptanceAuthoredReset!=='true'){{
                    document.getElementById('reset-view')?.click();
                    document.documentElement.dataset.veoveoAcceptanceAuthoredReset='true';
                  }}
                  const row=[...document.querySelectorAll('#preview-rows tr')].find(item=>item.textContent?.includes('ALPHA-7'));
                  if(row && !row.classList.contains('selected')) row.click();
                  const mapRect=document.querySelector('.map-stage')?.getBoundingClientRect();
                  return {{
                    layerFound:Boolean(layer),rowFound:Boolean(row),rowSelected:Boolean(row?.classList.contains('selected')),
                    inspectorTitle:document.getElementById('inspector-title')?.textContent?.trim() ?? '',
                    inspectorText:document.getElementById('inspector-body')?.textContent?.trim() ?? '',
                    mapWidth:mapRect?.width ?? 0,mapHeight:mapRect?.height ?? 0,
                    previewVisible:Boolean(document.querySelector('.preview-body')?.getBoundingClientRect().height)
                  }};
                }})()"#),
                false,
            )
            .await?;
            if value.get("layerFound").and_then(Value::as_bool) == Some(true)
                && value.get("rowFound").and_then(Value::as_bool) == Some(true)
                && value.get("rowSelected").and_then(Value::as_bool) == Some(true)
                && value.get("inspectorTitle").and_then(Value::as_str) == Some("Feature")
                && value
                    .get("inspectorText")
                    .and_then(Value::as_str)
                    .is_some_and(|text| text.contains("ALPHA-7"))
                && value.get("mapWidth").and_then(Value::as_f64).unwrap_or(0.0) >= 600.0
                && value.get("previewVisible").and_then(Value::as_bool) == Some(true)
            {
                break value;
            }
            ensure!(
                tokio::time::Instant::now() < deadline,
                "live unified Map workspace did not synchronize table selection with its feature inspector: {value}"
            );
            tokio::time::sleep(Duration::from_millis(250)).await;
        };
        let feature_screenshot = screenshot_directory.join("feature-inspector.png");
        let feature_screenshot_sha256 =
            capture_screenshot(&mut cdp, &session_id, &feature_screenshot).await?;

        let guided: Value = evaluate_console_app(
            &mut cdp,
            &target_id,
            &session_id,
            "map",
            r#"(() => {
              const mapSizes=[];
              const rememberMap=()=>{
                const bounds=document.querySelector('.map-stage')?.getBoundingClientRect();
                mapSizes.push({width:bounds?.width ?? 0,height:bounds?.height ?? 0});
              };
              const choose=(action)=>{
                const button=document.querySelector(`#inspector-body [data-action="${action}"]`);
                button?.click();
                rememberMap();
                return Boolean(button);
              };
              document.getElementById('add-data')?.click();
              rememberMap();
              const pickerText=[...document.querySelectorAll('#inspector-body .choice:not([hidden]) strong')]
                .map(item=>item.textContent?.trim());
              const picker=pickerText.includes('New layer') && pickerText.includes('Draw feature')
                && pickerText.includes('Import artifact') && pickerText.includes('Acquire source');

              const createOpened=choose('create-layer');
              const create=Boolean(document.getElementById('new-layer-title')
                && document.getElementById('new-layer-description')
                && document.getElementById('new-layer-class'));
              const createAdvancedClosed=Boolean(document.querySelector('#create-layer-form details:not([open])'));

              choose('picker');
              const drawOpened=choose('add-feature');
              const draw=Boolean(document.getElementById('feature-layer')
                && document.querySelectorAll('#add-feature-form [data-draw]').length===3
                && document.getElementById('feature-title')
                && document.getElementById('feature-semantic-type')
                && document.getElementById('property-rows'));
              const drawAdvancedClosed=Boolean(document.querySelector('#add-feature-form details:not([open])'));

              choose('picker');
              const importOpened=choose('import-artifact');
              const format=document.getElementById('import-format');
              const formats=[...(format?.options ?? [])].map(option=>option.value);
              const importFormats=formats.includes('geo_json_feature_collection')
                && formats.includes('geo_json_text_sequence') && formats.includes('geo_package');
              if(format){format.value='geo_package';format.dispatchEvent(new Event('change',{bubbles:true}));}
              const geopackage=Boolean(document.getElementById('inspect-geopackage')
                && !document.getElementById('geopackage-fields')?.hidden
                && document.getElementById('geopackage-table')
                && document.getElementById('geopackage-identity'));
              const authorizedArtifactCopy=document.getElementById('inspector-body')?.textContent
                ?.includes('artifact already authorized in this Work Context') ?? false;

              choose('picker');
              const acquireOpened=choose('acquire-source');
              const acquire=Boolean(document.getElementById('acquire-source')
                && document.getElementById('draw-coverage')
                && document.getElementById('bbox-west')
                && document.getElementById('bbox-north'));
              const acquireAdvancedClosed=Boolean(document.querySelector('#acquire-source-form details:not([open])'));

              document.getElementById('save-view')?.click();
              rememberMap();
              const save=Boolean(document.getElementById('save-view-title')
                && document.getElementById('save-view-summary'));

              document.getElementById('add-data')?.click();
              choose('import-artifact');
              const finalFormat=document.getElementById('import-format');
              if(finalFormat){finalFormat.value='geo_package';finalFormat.dispatchEvent(new Event('change',{bubbles:true}));}
              rememberMap();
              return {
                picker,createOpened,create,createAdvancedClosed,
                drawOpened,draw,drawAdvancedClosed,
                importOpened,importFormats,geopackage,authorizedArtifactCopy,
                acquireOpened,acquire,acquireAdvancedClosed,save,
                finalTitle:document.getElementById('inspector-title')?.textContent?.trim() ?? '',
                finalText:document.getElementById('inspector-body')?.textContent?.trim() ?? '',
                minimumMapWidth:Math.min(...mapSizes.map(size=>size.width)),
                minimumMapHeight:Math.min(...mapSizes.map(size=>size.height))
              };
            })()"#,
            false,
        )
        .await?;
        for field in [
            "picker",
            "createOpened",
            "create",
            "createAdvancedClosed",
            "drawOpened",
            "draw",
            "drawAdvancedClosed",
            "importOpened",
            "importFormats",
            "geopackage",
            "authorizedArtifactCopy",
            "acquireOpened",
            "acquire",
            "acquireAdvancedClosed",
            "save",
        ] {
            ensure!(
                guided.get(field).and_then(Value::as_bool) == Some(true),
                "live unified Map workspace guided workflow `{field}` failed: {guided}"
            );
        }
        ensure!(
            guided.get("finalTitle").and_then(Value::as_str) == Some("Import artifact")
                && guided
                    .get("finalText")
                    .and_then(Value::as_str)
                    .is_some_and(|text| {
                        text.contains("GeoJSON FeatureCollection")
                            && text.contains("GeoJSON Text Sequence")
                            && text.contains("GeoPackage")
                            && text.contains("Inspect GeoPackage")
                    })
                && guided
                    .get("minimumMapWidth")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0)
                    >= 600.0
                && guided
                    .get("minimumMapHeight")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0)
                    >= 360.0,
            "live unified Map workspace did not retain its map through every guided workflow: {guided}"
        );
        let guided_screenshot = screenshot_directory.join("guided-geopackage-import.png");
        let guided_screenshot_sha256 =
            capture_screenshot(&mut cdp, &session_id, &guided_screenshot).await?;

        let management = loop {
            let value: Value = evaluate_console_app(
                &mut cdp,
                &target_id,
                &session_id,
                "map",
                r#"(() => {
                  document.getElementById('add-data')?.click();
                  const disclosure=document.querySelector('#inspector-body details[data-admin]');
                  if(disclosure) disclosure.open=true;
                  document.querySelector('#inspector-body [data-action="manage-data"]')?.click();
                  const mapRect=document.querySelector('.map-stage')?.getBoundingClientRect();
                  return {
                    inspectorTitle:document.getElementById('inspector-title')?.textContent?.trim() ?? '',
                    inspectorText:document.getElementById('inspector-body')?.textContent?.trim() ?? '',
                    mapWidth:mapRect?.width ?? 0,mapHeight:mapRect?.height ?? 0,
                    catalogVisible:Boolean(document.querySelector('.catalog')?.getBoundingClientRect().width),
                    acquisitionRecords:document.querySelectorAll('#manage-acquisitions .record').length,
                    releaseRecords:document.querySelectorAll('#manage-releases .record').length
                  };
                })()"#,
                false,
            )
            .await?;
            if value.get("inspectorTitle").and_then(Value::as_str) == Some("Governed data")
                && value
                    .get("inspectorText")
                    .and_then(Value::as_str)
                    .is_some_and(|text| text.contains("Acquisitions and releases"))
                && value.get("mapWidth").and_then(Value::as_f64).unwrap_or(0.0) >= 600.0
                && value.get("catalogVisible").and_then(Value::as_bool) == Some(true)
            {
                break value;
            }
            ensure!(
                tokio::time::Instant::now() < deadline,
                "live unified Map workspace did not retain the map and catalog while opening governed data management: {value}"
            );
            tokio::time::sleep(Duration::from_millis(250)).await;
        };
        let management_screenshot = screenshot_directory.join("governed-data.png");
        let management_screenshot_sha256 =
            capture_screenshot(&mut cdp, &session_id, &management_screenshot).await?;
        cdp.assert_no_software_renderer_events()?;

        Ok(ConsoleMapWorkspaceCaptureEvidence {
            schema: "veoveo.io/console-map-workspace-capture-evidence/v3",
            captured_at: chrono::Utc::now(),
            page_url: page_url.to_owned(),
            workspace_screenshot_path: workspace_screenshot.display().to_string(),
            workspace_screenshot_sha256,
            source_screenshot_path: source_screenshot.display().to_string(),
            source_screenshot_sha256,
            feature_screenshot_path: feature_screenshot.display().to_string(),
            feature_screenshot_sha256,
            guided_screenshot_path: guided_screenshot.display().to_string(),
            guided_screenshot_sha256,
            management_screenshot_path: management_screenshot.display().to_string(),
            management_screenshot_sha256,
            hardware,
            workspace,
            source,
            feature,
            guided,
            management,
            render,
        })
    }
    .await;
    let acceptance = match acceptance {
        Ok(evidence) => Ok(evidence),
        Err(error) => {
            let diagnostics = cdp.stream_diagnostics(&session_id).await?;
            Err(error.context(diagnostics))
        }
    };
    let close = close_target(&mut cdp, &target_id).await;
    let evidence = acceptance?;
    close?;
    Ok(evidence)
}

fn analyze_map_workspace_render(screenshot_path: &Path) -> Result<MapWorkspaceRenderEvidence> {
    let image = image::open(screenshot_path)
        .with_context(|| {
            format!(
                "decoding Map workspace screenshot {}",
                screenshot_path.display()
            )
        })?
        .to_rgb8();
    let (width, height) = image.dimensions();
    ensure!(
        width >= 1_280 && height >= 720,
        "Map workspace screenshot is below the acceptance viewport: {width}x{height}"
    );
    let left = width * 35 / 100;
    let right = width * 75 / 100;
    let top = height * 20 / 100;
    let bottom = height * 60 / 100;
    let mut feature_colored_pixels = 0_u64;
    for y in top..bottom {
        for x in left..right {
            let [red, green, blue] = image.get_pixel(x, y).0;
            let minimum = red.min(green).min(blue);
            let maximum = red.max(green).max(blue);
            if red < 210 && maximum.saturating_sub(minimum) > 20 {
                feature_colored_pixels += 1;
            }
        }
    }
    ensure!(
        feature_colored_pixels >= 500,
        "Map workspace screenshot did not contain the expected rendered feature colors in the central map viewport ({feature_colored_pixels} qualifying pixels)"
    );
    Ok(MapWorkspaceRenderEvidence {
        screenshot_width: width,
        screenshot_height: height,
        feature_colored_pixels,
    })
}

#[allow(dead_code, reason = "shared by the focused browser-smoke binary")]
async fn serve_map_workspace_acceptance(
    app_html: Vec<u8>,
) -> Result<(String, tokio::task::JoinHandle<Result<()>>)> {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    let address = listener.local_addr()?;
    let harness = Arc::<[u8]>::from(map_workspace_acceptance_harness(address).into_bytes());
    let app_html = Arc::<[u8]>::from(app_html);
    let light_style = Arc::<[u8]>::from(serde_json::to_vec(&serde_json::json!({
        "version": 8,
        "sources": {},
        "layers": [{
            "id": "acceptance-light",
            "type": "background",
            "paint": {"background-color": "#dfe8ee"}
        }]
    }))?);
    let dark_style = Arc::<[u8]>::from(serde_json::to_vec(&serde_json::json!({
        "version": 8,
        "sources": {},
        "layers": [{
            "id": "acceptance-dark",
            "type": "background",
            "paint": {"background-color": "#111820"}
        }]
    }))?);
    let app_csp = format!(
        "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; \
         img-src data: blob: http://{address}; media-src blob:; connect-src data: http://{address}; \
         worker-src blob:; frame-ancestors 'self'; object-src 'none'; base-uri 'none'"
    );
    let server = tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await?;
            let mut request = [0_u8; 8_192];
            let count = stream.read(&mut request).await?;
            if count == 0 {
                continue;
            }
            let target = std::str::from_utf8(&request[..count])?
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/");
            let (status, content_type, csp, body): (&str, &str, Option<&str>, &[u8]) =
                if target.starts_with("/app") || target.starts_with("/console/api/apps/frame") {
                    (
                        "200 OK",
                        "text/html; charset=utf-8",
                        Some(app_csp.as_str()),
                        &app_html,
                    )
                } else if target.starts_with("/style-light.json") {
                    ("200 OK", "application/json", None, &light_style)
                } else if target.starts_with("/style-dark.json") {
                    ("200 OK", "application/json", None, &dark_style)
                } else if target == "/" || target.starts_with("/?") {
                    ("200 OK", "text/html; charset=utf-8", None, &harness)
                } else {
                    ("204 No Content", "text/plain", None, &[])
                };
            let mut headers = format!(
                "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n",
                body.len()
            );
            if let Some(csp) = csp {
                headers.push_str("Content-Security-Policy: ");
                headers.push_str(csp);
                headers.push_str("\r\n");
            }
            headers.push_str("\r\n");
            stream.write_all(headers.as_bytes()).await?;
            stream.write_all(body).await?;
            stream.shutdown().await?;
        }
    });
    Ok((format!("http://{address}/"), server))
}

#[allow(dead_code, reason = "shared by the focused browser-smoke binary")]
fn map_workspace_acceptance_harness(address: std::net::SocketAddr) -> String {
    let resources = serde_json::json!({
        "map://workspace": {
            "administration": true,
            "dataset_read": true,
            "feature_read": true,
            "feature_write": true,
            "feature_publish": true,
                "basemap": {
                    "id": "acceptance_basemap",
                    "title": "Acceptance basemap",
                    "light_style_url": format!("http://{address}/style-light.json"),
                    "dark_style_url": format!("http://{address}/style-dark.json")
                }
        },
        "map://feature-layers": [{
            "layer_id": "layer-0198-map-workspace",
            "title": "San Salvador operations",
            "content_class": "operational",
            "schema": {"version": 1},
            "revision": 3
        }],
        "map://publications": [{
            "publication_id": "publication-0198-map-workspace",
            "layer_id": "layer-0198-map-workspace",
            "layer_revision": 3,
            "schema_version": 1,
            "style_revision_id": "style-revision-0198-map-workspace",
            "title": "Operations publication"
        }],
        "map://compositions": [{
            "composition_id": "composition-0198-map-workspace",
            "title": "San Salvador operational picture",
            "current": {
                "revision": 2,
                "layers": [{
                    "layer_id": "layer-0198-map-workspace",
                    "publication_id": "publication-0198-map-workspace",
                    "style_revision_id": "style-revision-0198-map-workspace",
                    "visible": true,
                    "opacity": 1.0
                }],
                "view": {
                    "center": {"longitude_deg": -89.2182, "latitude_deg": 13.6929},
                    "zoom": 12.5,
                    "bearing_deg": 0.0,
                    "pitch_deg": 0.0
                }
            }
        }],
        "map://feature-style/style-revision-0198-map-workspace": {
            "style_revision_id": "style-revision-0198-map-workspace",
            "layer_id": "layer-0198-map-workspace",
            "version": 1,
            "style": {"rules": [
                {"geometry_type": "Polygon", "fill_color": "#287e8e", "fill_opacity": 0.32, "line_color": "#164d59", "line_width_px": 2.0},
                {"geometry_type": "LineString", "line_color": "#b8683b", "line_width_px": 4.0},
                {"geometry_type": "Point", "circle_color": "#b34f68", "circle_radius_px": 7.0}
            ]}
        },
        "map://sources": [{
            "source_id": "source-0198-map-workspace",
            "dataset_id": "dataset-0198-map-workspace",
            "name": "San Salvador reference source",
            "enabled": true,
            "adapter_kind": "authority_vector",
            "record_version": 1
        }],
        "map://datasets": {
            "dataset-0198-map-workspace": [{
                "release_id": "release-0198-map-workspace",
                "dataset_id": "dataset-0198-map-workspace",
                "source_id": "source-0198-map-workspace",
                "version_label": "acceptance-2026-08",
                "coverage": {"west": -89.24, "south": 13.67, "east": -89.19, "north": 13.72},
                "license": {"attribution": "Veoveo acceptance source"},
                "state": "active",
                "record_version": 1,
                "updated_at": "2026-08-25T00:00:00Z"
            }]
        },
        "map://active-releases": [{
            "dataset_id": "dataset-0198-map-workspace",
            "release_id": "release-0198-map-workspace",
            "record_version": 1
        }],
        "map://acquisitions": [],
        "map://mobility-profiles": []
    });
    let features = serde_json::json!([
        {
            "type": "Feature",
            "id": "feature-0198-command",
            "geometry": {"type": "Point", "coordinates": [-89.2182, 13.6929]},
            "properties": {"role": "command"},
            "featureType": "command_post",
            "layer_id": "layer-0198-map-workspace",
            "title": "Command post"
        },
        {
            "type": "Feature",
            "id": "feature-0198-route",
            "geometry": {"type": "LineString", "coordinates": [[-89.229, 13.688], [-89.2182, 13.6929], [-89.207, 13.699]]},
            "properties": {"role": "supply_route"},
            "featureType": "route",
            "layer_id": "layer-0198-map-workspace",
            "title": "Supply route"
        },
        {
            "type": "Feature",
            "id": "feature-0198-sector",
            "geometry": {"type": "Polygon", "coordinates": [[[-89.225, 13.688], [-89.211, 13.688], [-89.211, 13.699], [-89.225, 13.699], [-89.225, 13.688]]]},
            "properties": {"role": "operating_sector"},
            "featureType": "sector",
            "layer_id": "layer-0198-map-workspace",
            "title": "Operating sector"
        }
    ]);
    let source_features = serde_json::json!([{
        "feature": {
            "feature_id": "source-feature-0198-reference",
            "source_id": "source-0198-map-workspace",
            "release_id": "release-0198-map-workspace",
            "source_element_type": "feature",
            "source_element_id": "reference-zone",
            "representation": "polygon",
            "geometry": {"type": "Polygon", "coordinates": [[[-89.232, 13.682], [-89.195, 13.682], [-89.195, 13.708], [-89.232, 13.708], [-89.232, 13.682]]]},
            "normalized_tags": {"name": "Reference coverage"}
        }
    }]);
    format!(
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Map workspace acceptance host</title>
<style>html,body{{margin:0;width:100%;height:100%;overflow:hidden;background:#d8d5cd}}iframe{{display:block;width:100%;height:100%;border:0}}</style>
<script>
const resources={resources};
const features={features};
const sourceFeatures={source_features};
window.__mapWorkspaceEvidence={{initialized:false,queryCalls:0,queryResponses:0,authoredQueryCalls:0,sourceQueryCalls:0,sizeHeight:0,lastQuery:null,lastAuthoredQuery:null,lastSourceQuery:null,errors:[]}};
addEventListener("error",event=>window.__mapWorkspaceEvidence.errors.push(String(event.message||event.error||"host error")));
addEventListener("unhandledrejection",event=>window.__mapWorkspaceEvidence.errors.push(String(event.reason||"host rejection")));
addEventListener("message",event=>{{
  const frame=document.getElementById("app");
  if(!frame||event.source!==frame.contentWindow) return;
  const message=event.data;
  if(!message||message.jsonrpc!=="2.0") return;
  const reply=(result)=>event.source.postMessage({{jsonrpc:"2.0",id:message.id,result}},"*");
  const fail=(detail)=>{{window.__mapWorkspaceEvidence.errors.push(detail);event.source.postMessage({{jsonrpc:"2.0",id:message.id,error:{{code:-32601,message:detail}}}},"*");}};
  if(message.method==="ui/initialize") return reply({{protocolVersion:"2026-01-26",hostContext:{{theme:"light"}}}});
  if(message.method==="ui/notifications/initialized"){{window.__mapWorkspaceEvidence.initialized=true;return;}}
  if(message.method==="ui/notifications/size-changed"){{window.__mapWorkspaceEvidence.sizeHeight=Number(message.params?.height||0);return;}}
  if(message.method==="resources/read"){{
    const uri=message.params?.uri;
    if(!Object.hasOwn(resources,uri)) return fail(`unexpected resource ${{uri}}`);
    return reply({{contents:[{{uri,mimeType:"application/json",text:JSON.stringify(resources[uri])}}]}});
  }}
  if(message.method==="tools/call"){{
    const name=message.params?.name;
    const args=message.params?.arguments||{{}};
    window.__mapWorkspaceEvidence.queryCalls+=1;
    window.__mapWorkspaceEvidence.lastQuery=args;
    if(name==="query_features"){{
      window.__mapWorkspaceEvidence.authoredQueryCalls+=1;
      window.__mapWorkspaceEvidence.lastAuthoredQuery=args;
      window.__mapWorkspaceEvidence.queryResponses+=1;
      return reply({{structuredContent:{{layer_id:"layer-0198-map-workspace",features,next_cursor:null,projection_sequence:3}}}});
    }}
    if(name==="query_source_features"){{
      window.__mapWorkspaceEvidence.sourceQueryCalls+=1;
      window.__mapWorkspaceEvidence.lastSourceQuery=args;
      window.__mapWorkspaceEvidence.queryResponses+=1;
      return reply({{structuredContent:{{release_id:"release-0198-map-workspace",features:sourceFeatures,next_cursor:null,query_digest_sha256:"acceptance"}}}});
    }}
    return fail(`unexpected tool ${{name}}`);
  }}
  if(message.method==="subscriptions/listen") return reply({{}});
  if(message.id!==undefined) return fail(`unexpected request ${{message.method}}`);
}});
</script></head><body><iframe id="app" class="app-frame" title="Workspace" sandbox="allow-scripts" referrerpolicy="no-referrer" src="/console/api/apps/frame?uri=ui%3A%2F%2Fmap%2Fworkspace.html"></iframe></body></html>"#,
        resources = resources,
        features = features,
        source_features = source_features,
    )
}

async fn open_headed_target(cdp_base: &str, page_url: &str) -> Result<(Cdp, String, String)> {
    open_headed_target_in_window(cdp_base, page_url, false).await
}

async fn open_headed_target_in_window(
    cdp_base: &str,
    page_url: &str,
    new_window: bool,
) -> Result<(Cdp, String, String)> {
    let mut cdp = connect_headed_browser(cdp_base, "visual acceptance").await?;
    let target = cdp
        .command(
            "Target.createTarget",
            serde_json::json!({"url": page_url, "newWindow": new_window}),
            None,
        )
        .await?;
    let target_id = value_string(&target, "/targetId")?.to_owned();
    let attached = cdp
        .command(
            "Target.attachToTarget",
            serde_json::json!({"targetId": target_id, "flatten": true}),
            None,
        )
        .await?;
    let session_id = value_string(&attached, "/sessionId")?.to_owned();
    for method in [
        "Runtime.enable",
        "Page.enable",
        "DOM.enable",
        "Log.enable",
        "Network.enable",
    ] {
        cdp.command(method, serde_json::json!({}), Some(&session_id))
            .await?;
    }
    cdp.command(
        "Emulation.setDeviceMetricsOverride",
        serde_json::json!({
            "width": 1920,
            "height": 1080,
            "deviceScaleFactor": 1,
            "mobile": false,
        }),
        Some(&session_id),
    )
    .await?;
    cdp.command(
        "Page.bringToFront",
        serde_json::json!({}),
        Some(&session_id),
    )
    .await?;
    wait_for_requested_document(&mut cdp, &session_id, page_url).await?;
    Ok((cdp, target_id, session_id))
}

async fn close_target(cdp: &mut Cdp, target_id: &str) -> Result<()> {
    cdp.command(
        "Target.closeTarget",
        serde_json::json!({"targetId": target_id}),
        None,
    )
    .await?;
    Ok(())
}

enum ConsoleAppExecution {
    ParentWorld(u64),
    TargetSession(String),
}

async fn wait_for_console_app_execution(
    cdp: &mut Cdp,
    parent_target_id: &str,
    session_id: &str,
    app_server: &str,
) -> Result<ConsoleAppExecution> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        let targets = cdp
            .command("Target.getTargets", serde_json::json!({}), None)
            .await?;
        if let Some(app_target_id) =
            find_app_target_id(&targets, parent_target_id, app_server).map(str::to_owned)
        {
            let attached = match cdp
                .command(
                    "Target.attachToTarget",
                    serde_json::json!({"targetId": app_target_id, "flatten": true}),
                    None,
                )
                .await
            {
                Ok(attached) => attached,
                Err(error)
                    if stale_app_target(&error) && tokio::time::Instant::now() < deadline =>
                {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("attaching to the {app_server} Console App target")
                    });
                }
            };
            let app_session_id = attached
                .get("sessionId")
                .and_then(Value::as_str)
                .with_context(|| {
                    format!("Chrome did not attach a session to the {app_server} Console App")
                })?
                .to_owned();
            let runtime = cdp
                .command(
                    "Runtime.enable",
                    serde_json::json!({}),
                    Some(&app_session_id),
                )
                .await;
            match runtime {
                Ok(_) => return Ok(ConsoleAppExecution::TargetSession(app_session_id)),
                Err(error) => {
                    let _ = cdp
                        .command(
                            "Target.detachFromTarget",
                            serde_json::json!({"sessionId": app_session_id}),
                            None,
                        )
                        .await;
                    if stale_app_target(&error) && tokio::time::Instant::now() < deadline {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        continue;
                    }
                    return Err(error).with_context(|| {
                        format!("enabling the {app_server} Console App target runtime")
                    });
                }
            }
        }

        let tree = cdp
            .command("Page.getFrameTree", serde_json::json!({}), Some(session_id))
            .await?;
        let frame_id = find_app_frame_id(
            tree.get("frameTree")
                .context("Chrome frame tree omitted its root")?,
            app_server,
        )
        .map(str::to_owned);
        let frame_id = if frame_id.is_some() {
            frame_id
        } else {
            match find_app_frame_id_from_dom(cdp, session_id, app_server).await {
                Ok(frame_id) => frame_id,
                Err(error) if stale_app_frame(&error) && tokio::time::Instant::now() < deadline => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
                Err(error) => return Err(error),
            }
        };
        if let Some(frame_id) = frame_id {
            let isolated = match cdp
                .command(
                    "Page.createIsolatedWorld",
                    serde_json::json!({
                        "frameId": frame_id,
                        "worldName": "veoveo-uav-acceptance",
                        "grantUniversalAccess": false
                    }),
                    Some(session_id),
                )
                .await
            {
                Ok(isolated) => isolated,
                Err(error) if stale_app_frame(&error) && tokio::time::Instant::now() < deadline => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("creating an isolated world in the {app_server} Console App frame")
                    });
                }
            };
            let context_id = isolated
                .get("executionContextId")
                .and_then(Value::as_u64)
                .context("Chrome did not create an App-frame execution context")?;
            return Ok(ConsoleAppExecution::ParentWorld(context_id));
        }
        let state: Value = cdp
            .evaluate(
                session_id,
                r#"({url:location.href,title:document.title,body:document.body?.innerText ?? ""})"#,
                false,
            )
            .await?;
        ensure!(
            tokio::time::Instant::now() < deadline,
            "authenticated Console did not load the {app_server} App frame: {state}"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn evaluate_console_app<T: serde::de::DeserializeOwned>(
    cdp: &mut Cdp,
    parent_target_id: &str,
    session_id: &str,
    app_server: &str,
    expression: &str,
    await_promise: bool,
) -> Result<T> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        let execution =
            wait_for_console_app_execution(cdp, parent_target_id, session_id, app_server).await?;
        let (evaluated, detach) = match execution {
            ConsoleAppExecution::ParentWorld(context_id) => (
                cdp.evaluate_context(session_id, context_id, expression, await_promise)
                    .await,
                None,
            ),
            ConsoleAppExecution::TargetSession(app_session_id) => {
                let evaluated = cdp
                    .evaluate(&app_session_id, expression, await_promise)
                    .await;
                let detach = cdp
                    .command(
                        "Target.detachFromTarget",
                        serde_json::json!({"sessionId": app_session_id}),
                        None,
                    )
                    .await;
                (evaluated, Some(detach))
            }
        };
        match evaluated {
            Ok(value) => {
                if let Some(Err(error)) = detach
                    && !stale_app_target(&error)
                {
                    return Err(error).with_context(|| {
                        format!("detaching from the {app_server} Console App target")
                    });
                }
                return Ok(value);
            }
            Err(error)
                if stale_execution_context(&error) && tokio::time::Instant::now() < deadline =>
            {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("evaluating the current {app_server} Console App frame")
                });
            }
        }
    }
}

fn stale_execution_context(error: &anyhow::Error) -> bool {
    let message = format!("{error:#}");
    message.contains("Cannot find context with specified id")
        || message.contains("Execution context was destroyed")
        || stale_app_target(error)
}

fn stale_app_frame(error: &anyhow::Error) -> bool {
    let message = format!("{error:#}");
    message.contains("No frame for given id found")
        || message.contains("Could not find node with given id")
}

fn stale_app_target(error: &anyhow::Error) -> bool {
    let message = format!("{error:#}");
    message.contains("No target with given id found")
        || message.contains("No session with given id")
        || message.contains("Session closed")
        || message.contains("Target closed")
        || message.contains("Inspected target navigated or closed")
}

async fn find_app_frame_id_from_dom(
    cdp: &mut Cdp,
    session_id: &str,
    app_server: &str,
) -> Result<Option<String>> {
    let document = cdp
        .command(
            "DOM.getDocument",
            serde_json::json!({"depth": 0}),
            Some(session_id),
        )
        .await?;
    let root = document
        .pointer("/root/nodeId")
        .and_then(Value::as_u64)
        .context("Chrome DOM document omitted its root node")?;
    let selected = cdp
        .command(
            "DOM.querySelector",
            serde_json::json!({"nodeId": root, "selector": "iframe.app-frame"}),
            Some(session_id),
        )
        .await?;
    let Some(node_id) = selected
        .get("nodeId")
        .and_then(Value::as_u64)
        .filter(|node_id| *node_id != 0)
    else {
        return Ok(None);
    };
    let described = cdp
        .command(
            "DOM.describeNode",
            serde_json::json!({"nodeId": node_id, "depth": 0}),
            Some(session_id),
        )
        .await?;
    let node = described
        .get("node")
        .with_context(|| format!("Chrome omitted the {app_server} iframe node"))?;
    Ok(app_frame_id_from_dom_node(node, app_server).map(str::to_owned))
}

fn app_frame_id_from_dom_node<'a>(node: &'a Value, app_server: &str) -> Option<&'a str> {
    let attributes = node.get("attributes").and_then(Value::as_array)?;
    let is_expected_app = attributes.windows(2).any(|pair| {
        pair[0].as_str() == Some("src")
            && pair[1].as_str().is_some_and(|src| src.contains(app_server))
    });
    if !is_expected_app {
        return None;
    }
    node.get("frameId").and_then(Value::as_str)
}

fn find_app_frame_id<'a>(frame_tree: &'a Value, app_server: &str) -> Option<&'a str> {
    let frame = frame_tree.get("frame")?;
    let url = frame.get("url").and_then(Value::as_str).unwrap_or_default();
    if is_app_frame_url(url, app_server) {
        return frame.get("id").and_then(Value::as_str);
    }
    frame_tree
        .get("childFrames")
        .and_then(Value::as_array)?
        .iter()
        .find_map(|child| find_app_frame_id(child, app_server))
}

fn find_app_target_id<'a>(
    targets: &'a Value,
    parent_target_id: &str,
    app_server: &str,
) -> Option<&'a str> {
    targets
        .get("targetInfos")
        .and_then(Value::as_array)?
        .iter()
        .find(|target| {
            target.get("type").and_then(Value::as_str) == Some("iframe")
                && target.get("parentId").and_then(Value::as_str) == Some(parent_target_id)
                && target
                    .get("url")
                    .and_then(Value::as_str)
                    .is_some_and(|url| is_app_frame_url(url, app_server))
        })
        .and_then(|target| target.get("targetId"))
        .and_then(Value::as_str)
}

fn is_app_frame_url(url: &str, app_server: &str) -> bool {
    url.contains("/console/api/apps/frame") && url.contains(app_server)
}

async fn wait_for_console_app_camera(
    cdp: &mut Cdp,
    target_id: &str,
    session_id: &str,
    expected_camera_id: &str,
) -> Result<bool> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    let expected = serde_json::to_string(expected_camera_id)?;
    loop {
        let expression = format!(
            r#"(() => {{
              const input=[...document.querySelectorAll('#choices input[type="checkbox"]')]
                .find((candidate)=>candidate.parentElement?.textContent?.trim().startsWith({expected}));
              if (!input) return {{found:false,error:document.getElementById("error")?.hidden === false
                ? document.getElementById("error").textContent : "",body:document.body?.innerText ?? ""}};
              if (!input.checked) input.click();
              return {{found:true,error:"",body:document.body?.innerText ?? ""}};
            }})()"#
        );
        let state: Value =
            evaluate_console_app(cdp, target_id, session_id, "uav-sim", &expression, false).await?;
        if state.get("found").and_then(Value::as_bool) == Some(true) {
            return Ok(true);
        }
        ensure!(
            state
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("")
                .is_empty(),
            "Console UAV live view App failed during camera discovery: {state}"
        );
        ensure!(
            tokio::time::Instant::now() < deadline,
            "Console UAV live view App did not discover camera {expected_camera_id}: {state}"
        );
        cdp.assert_no_software_renderer_events()?;
        tokio::time::sleep(Duration::from_millis(250)).await;
        assert_page_visible(cdp, session_id).await?;
    }
}

async fn isolate_console_app_camera(
    cdp: &mut Cdp,
    target_id: &str,
    session_id: &str,
    expected_camera_id: &str,
) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let expected = serde_json::to_string(expected_camera_id)?;
    loop {
        let expression = format!(
            r#"(() => {{
              const expected={expected};
              const inputs=[...document.querySelectorAll('#choices input[type="checkbox"]')];
              const cameraId=(input)=>input.parentElement?.textContent?.split(" · ")[0]?.trim() ?? "";
              const target=inputs.find((input)=>cameraId(input)===expected);
              if (!target) return {{found:false,checkedIds:[],viewIds:[]}};
              for (const input of inputs) {{
                const shouldBeChecked=cameraId(input)===expected;
                if (input.checked!==shouldBeChecked) input.click();
              }}
              return {{
                found:true,
                checkedIds:inputs.filter((input)=>input.checked).map(cameraId),
                viewIds:[...document.querySelectorAll(".view")].map((view)=>view.id.slice(5))
              }};
            }})()"#
        );
        let state: Value =
            evaluate_console_app(cdp, target_id, session_id, "uav-sim", &expression, false).await?;
        let checked_ids = state
            .get("checkedIds")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let view_ids = state
            .get("viewIds")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let only_expected =
            |values: &[Value]| values.len() == 1 && values[0].as_str() == Some(expected_camera_id);
        if state.get("found").and_then(Value::as_bool) == Some(true)
            && only_expected(&checked_ids)
            && only_expected(&view_ids)
        {
            return Ok(());
        }
        ensure!(
            tokio::time::Instant::now() < deadline,
            "Console UAV live view App did not isolate camera {expected_camera_id}: {state}"
        );
        cdp.assert_no_software_renderer_events()?;
        assert_page_visible(cdp, session_id).await?;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn wait_for_console_video(
    cdp: &mut Cdp,
    target_id: &str,
    session_id: &str,
    expected_camera_id: &str,
) -> Result<AppVideoState> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(180);
    loop {
        wait_for_console_app_camera(cdp, target_id, session_id, expected_camera_id).await?;
        let state: AppVideoState = evaluate_console_app(
            cdp,
            target_id,
            session_id,
            "uav-sim",
            APP_FRAME_VIDEO_STATE,
            false,
        )
        .await?;
        if state.ready_state >= 2 && state.video_width > 0 && state.current_time > 0.0 {
            state.validate(expected_camera_id)?;
            return Ok(state);
        }
        ensure!(
            state.error.is_empty(),
            "Console UAV live view App failed while opening the real stream: {state:?}"
        );
        ensure!(
            tokio::time::Instant::now() < deadline,
            "Console UAV live view App did not display real H.264 video: {state:?}"
        );
        cdp.assert_no_software_renderer_events()?;
        assert_page_visible(cdp, session_id).await?;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn wait_for_console_video_advance(
    cdp: &mut Cdp,
    target_id: &str,
    session_id: &str,
    expected_camera_id: &str,
    first: AppVideoState,
) -> Result<AppVideoState> {
    let startup_cadence: VideoCadenceSample = evaluate_console_app(
        cdp,
        target_id,
        session_id,
        "uav-sim",
        APP_FRAME_VIDEO_CADENCE,
        true,
    )
    .await?;
    let cadence = if startup_cadence.needs_steady_state_sample(first.declared_frame_rate_hz) {
        eprintln!(
            "shared H.264 startup window is still warming; requiring one strict steady-state window: {startup_cadence:?}"
        );
        evaluate_console_app(
            cdp,
            target_id,
            session_id,
            "uav-sim",
            APP_FRAME_VIDEO_CADENCE,
            true,
        )
        .await?
    } else {
        startup_cadence
    };
    let mut second = wait_for_console_video(cdp, target_id, session_id, expected_camera_id).await?;
    ensure!(
        second.document_epoch_ms == first.document_epoch_ms
            && second.viewer_instance_id == first.viewer_instance_id
            && second.live_view_id == first.live_view_id
            && second.stream_product_id == first.stream_product_id
            && second.current_time > first.current_time + 0.25,
        "Console H.264 video did not remain mounted and advance: {first:?} -> {second:?}"
    );
    cadence.validate(second.declared_frame_rate_hz)?;
    second.observed_frame_rate_hz = cadence.observed_frame_rate_hz();
    second.cadence_sample_frames = cadence.presented_frame_intervals;
    second.cadence_sample_seconds = cadence.interval_seconds;
    second.cadence_dropped_frames = cadence.dropped_video_frames;
    second.source_to_render_p95_ms = cadence.source_to_render_p95_ms;
    second.source_to_render_samples = cadence.source_to_render_samples;
    second.composed_motion_to_photon_upper_bound_p95_ms =
        cadence.composed_motion_to_photon_upper_bound_p95_ms;
    second.delivery_samples = cadence.delivery_samples;
    second.receive_to_display_p95_ms = cadence.receive_to_display_p95_ms;
    Ok(second)
}

async fn wait_for_console_video_replacement(
    cdp: &mut Cdp,
    target_id: &str,
    session_id: &str,
    expected_camera_id: &str,
    before: &AppVideoState,
    timeout: Duration,
) -> Result<AppVideoState> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let state: AppVideoState = evaluate_console_app(
            cdp,
            target_id,
            session_id,
            "uav-sim",
            APP_FRAME_VIDEO_STATE,
            false,
        )
        .await?;
        if state.ready_state >= 2
            && state.video_width > 0
            && state.current_time > 0.0
            && state.live_view_id != before.live_view_id
        {
            state.validate(expected_camera_id)?;
            ensure!(
                state.document_epoch_ms == before.document_epoch_ms
                    && state.viewer_instance_id == before.viewer_instance_id
                    && state.live_view_id != before.live_view_id,
                "component restart reloaded the App or reused stale stream authorization: {before:?} -> {state:?}"
            );
            return Ok(state);
        }
        ensure!(
            tokio::time::Instant::now() < deadline,
            "Console UAV live view App did not replace its stream authorization after component restart: {before:?} -> {state:?}"
        );
        cdp.assert_no_software_renderer_events()?;
        assert_page_visible(cdp, session_id).await?;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn wait_for_console_grid_videos(
    cdp: &mut Cdp,
    target_id: &str,
    session_id: &str,
    expected_camera_ids: &[&str],
) -> Result<Vec<GridVideoState>> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(180);
    loop {
        let states: Vec<GridVideoState> = evaluate_console_app(
            cdp,
            target_id,
            session_id,
            "uav-sim",
            APP_FRAME_GRID_VIDEO_STATES,
            false,
        )
        .await?;
        let complete = expected_camera_ids.iter().all(|expected| {
            states
                .iter()
                .find(|state| state.camera_id == **expected)
                .is_some_and(GridVideoState::is_ready)
        });
        if complete && states.len() == expected_camera_ids.len() {
            for state in &states {
                state.validate()?;
            }
            return Ok(states);
        }
        ensure!(
            states.iter().all(|state| state.error.is_empty()),
            "Console UAV live-view grid failed while opening native products: {states:?}"
        );
        ensure!(
            tokio::time::Instant::now() < deadline,
            "Console UAV live-view grid did not display every selected camera: {states:?}"
        );
        cdp.assert_no_software_renderer_events()?;
        assert_page_visible(cdp, session_id).await?;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn wait_for_console_stream_video(
    cdp: &mut Cdp,
    target_id: &str,
    session_id: &str,
) -> Result<StreamAppState> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(180);
    loop {
        let state: StreamAppState = evaluate_console_app(
            cdp,
            target_id,
            session_id,
            "stream",
            STREAM_APP_STATE,
            false,
        )
        .await?;
        if state.is_ready() {
            state.validate()?;
            return Ok(state);
        }
        ensure!(
            state.error.is_empty(),
            "Console Stream App failed while opening live encoded video: {state:?}"
        );
        ensure!(
            tokio::time::Instant::now() < deadline,
            "Console Stream App did not display advancing live H.264 and typed results: {state:?}"
        );
        cdp.assert_no_software_renderer_events()?;
        assert_page_visible(cdp, session_id).await?;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn wait_for_console_stream_advance(
    cdp: &mut Cdp,
    target_id: &str,
    session_id: &str,
    mut first: StreamAppState,
) -> Result<StreamAppState> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(180);
    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let second = wait_for_console_stream_video(cdp, target_id, session_id).await?;
        if second.document_epoch_ms == first.document_epoch_ms
            && second.session_id == first.session_id
            && second.decoded_frames > first.decoded_frames
            && second.processed_frames > first.processed_frames
        {
            return Ok(second);
        }
        ensure!(
            tokio::time::Instant::now() < deadline,
            "Console Stream App did not remain mounted and process advancing live frames: \
             {:?} -> {:?}",
            first,
            second
        );
        first = second;
    }
}

async fn assert_page_visible(cdp: &mut Cdp, session_id: &str) -> Result<()> {
    let visible: bool = cdp
        .evaluate(session_id, "document.visibilityState === 'visible'", false)
        .await?;
    ensure!(
        visible,
        "visual acceptance requires the headed Console target to remain visible"
    );
    Ok(())
}

async fn close_console_live_view(
    cdp: &mut Cdp,
    target_id: &str,
    session_id: &str,
    expected_camera_id: &str,
) -> Result<()> {
    close_console_live_views(cdp, target_id, session_id, &[expected_camera_id]).await
}

async fn close_console_live_views(
    cdp: &mut Cdp,
    target_id: &str,
    session_id: &str,
    expected_camera_ids: &[&str],
) -> Result<()> {
    let expected = serde_json::to_string(expected_camera_ids)?;
    let requested: usize = evaluate_console_app(
        cdp,
        target_id,
        session_id,
        "uav-sim",
        &format!(
            r#"(() => {{
                  const expected=new Set({expected});
                  const inputs=[...document.querySelectorAll('#choices input[type="checkbox"]')];
                  const matched=inputs.filter((candidate)=>expected.has(candidate.parentElement?.textContent?.split(" · ")[0]?.trim())).length;
                  for (const input of inputs) if (input.checked) input.click();
                  return matched;
                }})()"#
        ),
        false,
    )
    .await?;
    ensure!(
        requested == expected_camera_ids.len(),
        "Console could not close every requested live camera: expected {expected_camera_ids:?}, matched {requested}"
    );
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let state: Value = evaluate_console_app(
            cdp,
            target_id,
            session_id,
            "uav-sim",
            r#"(() => ({
                  checked:document.querySelector('#choices input[type="checkbox"]:checked') !== null,
                  canvasCount:document.querySelectorAll(".view canvas").length,
                  status:document.getElementById("status")?.textContent ?? "",
                  error:document.getElementById("error")?.hidden === false
                    ? document.getElementById("error").textContent : ""
                }))()"#,
            false,
        )
        .await?;
        ensure!(
            state
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("")
                .is_empty(),
            "Console failed to close selected live cameras: {state}"
        );
        if state.get("checked").and_then(Value::as_bool) == Some(false)
            && state.get("canvasCount").and_then(Value::as_u64) == Some(0)
        {
            return Ok(());
        }
        ensure!(
            tokio::time::Instant::now() < deadline,
            "Console did not close selected live cameras within 30 seconds: {state}"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn capture_screenshot(cdp: &mut Cdp, session_id: &str, output: &Path) -> Result<String> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating screenshot directory {}", parent.display()))?;
    }
    let result = cdp
        .command(
            "Page.captureScreenshot",
            serde_json::json!({
                "format": "png",
                "fromSurface": true,
                "captureBeyondViewport": false
            }),
            Some(session_id),
        )
        .await?;
    let encoded = value_string(&result, "/data")?;
    let bytes = STANDARD
        .decode(encoded)
        .context("decoding Chrome PNG screenshot")?;
    ensure!(
        bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "Chrome screenshot was not PNG"
    );
    fs::write(output, &bytes)
        .with_context(|| format!("writing screenshot {}", output.display()))?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(&bytes))))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChromeVersion {
    web_socket_debugger_url: String,
}

async fn connect_headed_browser(endpoint: &str, acceptance: &str) -> Result<Cdp> {
    let endpoint = Url::parse(endpoint).context("parsing Chrome DevTools endpoint")?;
    let browser_web_socket = match endpoint.scheme() {
        "ws" => endpoint.to_string(),
        "http" | "https" => {
            let version_url = endpoint
                .join("json/version")
                .context("Chrome CDP URL cannot resolve /json/version")?;
            let version: ChromeVersion = reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()?
                .get(version_url)
                .send()
                .await
                .context("headed Chrome DevTools endpoint is unavailable")?
                .error_for_status()?
                .json()
                .await?;
            version.web_socket_debugger_url
        }
        scheme => bail!(
            "Chrome DevTools endpoint must use http:// discovery or a direct ws:// browser endpoint, received {scheme:?}"
        ),
    };
    let mut cdp = Cdp::connect(&browser_web_socket).await?;
    let version = cdp
        .command("Browser.getVersion", serde_json::json!({}), None)
        .await
        .context("querying the attached Chrome identity")?;
    let product = value_string(&version, "/product")?;
    ensure!(
        !product.to_ascii_lowercase().contains("headless"),
        "{acceptance} requires headed Chrome; endpoint reported {product}"
    );
    Ok(cdp)
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct HardwareIdentity {
    user_agent: String,
    webgpu_vendor: String,
    webgpu_architecture: String,
    webgpu_device: String,
    webgpu_description: String,
    webgl_available: bool,
    webgl_vendor: String,
    webgl_renderer: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BrowserGpuApi {
    WebGpu,
    WebGl,
}

impl HardwareIdentity {
    fn validate(&self) -> Result<()> {
        ensure!(
            !self.user_agent.contains("HeadlessChrome"),
            "attached Chrome is headless"
        );
        let (hardware_apis, webgpu, webgl) = self.hardware_apis();
        ensure!(
            !hardware_apis.is_empty(),
            "headed Chrome requires hardware-backed NVIDIA WebGPU or WebGL; \
             received WebGPU {webgpu:?} and WebGL {webgl:?}"
        );
        Ok(())
    }

    fn hardware_apis(&self) -> (Vec<BrowserGpuApi>, String, String) {
        let webgpu = format!(
            "{} {} {} {}",
            self.webgpu_vendor,
            self.webgpu_architecture,
            self.webgpu_device,
            self.webgpu_description
        )
        .to_ascii_lowercase();
        let webgl = format!("{} {}", self.webgl_vendor, self.webgl_renderer).to_ascii_lowercase();
        let mut hardware_apis = Vec::with_capacity(2);
        if !self.webgpu_vendor.is_empty()
            && webgpu.contains("nvidia")
            && !software_renderer(&webgpu)
        {
            hardware_apis.push(BrowserGpuApi::WebGpu);
        }
        if self.webgl_available && webgl.contains("nvidia") && !software_renderer(&webgl) {
            hardware_apis.push(BrowserGpuApi::WebGl);
        }
        (hardware_apis, webgpu, webgl)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppVideoState {
    #[serde(skip_serializing)]
    document_epoch_ms: f64,
    camera_id: String,
    viewer_instance_id: String,
    live_view_id: String,
    stream_product_id: String,
    ready_state: u16,
    video_width: u32,
    video_height: u32,
    current_time: f64,
    sampled_at_ms: f64,
    total_video_frames: u64,
    dropped_video_frames: u64,
    declared_frame_rate_hz: f64,
    #[serde(default)]
    observed_frame_rate_hz: f64,
    #[serde(default)]
    cadence_sample_frames: u64,
    #[serde(default)]
    cadence_sample_seconds: f64,
    #[serde(default)]
    cadence_dropped_frames: u64,
    #[serde(default)]
    source_to_render_p95_ms: f64,
    #[serde(default)]
    source_to_render_samples: u64,
    #[serde(default)]
    composed_motion_to_photon_upper_bound_p95_ms: f64,
    #[serde(default)]
    delivery_samples: u64,
    #[serde(default)]
    receive_to_display_p95_ms: f64,
    mean_luma: f64,
    luma_standard_deviation: f64,
    minimum_luma: u8,
    maximum_luma: u8,
    pixel_sample_error: String,
    decode_label: String,
    status: String,
    error: String,
    body_text: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct GridVideoState {
    #[serde(skip_serializing)]
    document_epoch_ms: f64,
    camera_id: String,
    viewer_instance_id: String,
    live_view_id: String,
    stream_product_id: String,
    ready_state: u16,
    video_width: u32,
    video_height: u32,
    current_time: f64,
    error: String,
}

impl GridVideoState {
    fn is_ready(&self) -> bool {
        self.ready_state >= 2
            && self.video_width == 1280
            && self.video_height == 720
            && self.current_time > 0.0
            && !self.viewer_instance_id.is_empty()
            && !self.live_view_id.is_empty()
            && !self.stream_product_id.is_empty()
            && self.error.is_empty()
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            self.is_ready(),
            "grid camera did not expose one advancing 1280x720 native product: {self:?}"
        );
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VideoCadenceSample {
    interval_seconds: f64,
    media_time_seconds: f64,
    presented_frame_intervals: u64,
    decoded_video_frames: u64,
    dropped_video_frames: u64,
    source_to_render_p95_ms: f64,
    source_to_render_samples: u64,
    composed_motion_to_photon_upper_bound_p95_ms: f64,
    delivery_samples: u64,
    receive_to_display_p95_ms: f64,
}

impl VideoCadenceSample {
    fn observed_frame_rate_hz(&self) -> f64 {
        self.presented_frame_intervals as f64 / self.interval_seconds
    }

    fn needs_steady_state_sample(&self, declared_frame_rate_hz: f64) -> bool {
        self.interval_seconds > 0.0
            && self.media_time_seconds > 0.0
            && self.presented_frame_intervals >= 48
            && declared_frame_rate_hz > 0.0
            && (self.observed_frame_rate_hz() < MINIMUM_DELIVERED_FRAME_RATE_HZ
                || self.source_to_render_samples < 12
                || !self.source_to_render_p95_ms.is_finite()
                || self.source_to_render_p95_ms >= MAXIMUM_SOURCE_TO_RENDER_P95_MS
                || self.delivery_samples < 48
                || !self.receive_to_display_p95_ms.is_finite()
                || !self
                    .composed_motion_to_photon_upper_bound_p95_ms
                    .is_finite()
                || self.composed_motion_to_photon_upper_bound_p95_ms
                    >= MAXIMUM_MOTION_TO_PHOTON_P95_MS)
    }

    fn validate(&self, declared_frame_rate_hz: f64) -> Result<()> {
        ensure!(
            self.interval_seconds > 0.0
                && self.media_time_seconds > 0.0
                && self.presented_frame_intervals >= 48
                && declared_frame_rate_hz > 0.0,
            "authoritative live-view App returned an invalid reactive cadence sample: {self:?}"
        );
        let observed_frame_rate_hz = self.observed_frame_rate_hz();
        ensure!(
            observed_frame_rate_hz >= MINIMUM_DELIVERED_FRAME_RATE_HZ,
            "authoritative live-view App delivered {observed_frame_rate_hz:.2} fps, below the qualified {MINIMUM_DELIVERED_FRAME_RATE_HZ:.2} fps floor for its {declared_frame_rate_hz:.2} fps target: {self:?}"
        );
        ensure!(
            self.decoded_video_frames >= self.presented_frame_intervals
                && self.dropped_video_frames * 20 <= self.decoded_video_frames.max(1),
            "authoritative live-view App dropped more than 5% of browser video frames: {self:?}"
        );
        ensure!(
            self.source_to_render_samples >= 12
                && self.source_to_render_p95_ms.is_finite()
                && (0.0..MAXIMUM_SOURCE_TO_RENDER_P95_MS).contains(&self.source_to_render_p95_ms),
            "authoritative camera source-to-render p95 exceeded {MAXIMUM_SOURCE_TO_RENDER_P95_MS:.0} ms or lacked evidence: {self:?}"
        );
        ensure!(
            self.delivery_samples >= 48
                && self.receive_to_display_p95_ms.is_finite()
                && self.receive_to_display_p95_ms >= 0.0
                && self
                    .composed_motion_to_photon_upper_bound_p95_ms
                    .is_finite()
                && (0.0..MAXIMUM_MOTION_TO_PHOTON_P95_MS)
                    .contains(&self.composed_motion_to_photon_upper_bound_p95_ms),
            "shared H.264 composed motion-to-photon upper bound exceeded {MAXIMUM_MOTION_TO_PHOTON_P95_MS:.0} ms or lacked browser delivery evidence: {self:?}"
        );
        Ok(())
    }
}

impl AppVideoState {
    fn validate(&self, expected_camera_id: &str) -> Result<()> {
        ensure!(
            self.camera_id == expected_camera_id,
            "authoritative live-view App selected camera {:?}, expected {expected_camera_id:?}",
            self.camera_id
        );
        ensure!(
            !self.viewer_instance_id.is_empty()
                && !self.live_view_id.is_empty()
                && !self.stream_product_id.is_empty(),
            "authoritative live-view App omitted its viewer and shared-product identity: {self:?}"
        );
        ensure!(
            self.ready_state >= 2
                && self.video_width == 1280
                && self.video_height == 720
                && self.current_time.is_finite()
                && self.current_time > 0.0,
            "authoritative live-view App did not display the declared 1280x720 H.264 stream: {self:?}"
        );
        ensure!(
            self.pixel_sample_error.is_empty()
                && self.mean_luma >= MINIMUM_MEAN_LUMA
                && self.mean_luma <= MAXIMUM_MEAN_LUMA
                && self.luma_standard_deviation >= 5.0
                && self.minimum_luma < self.maximum_luma,
            "authoritative live-view App displayed a blank or uniform GPU frame: {self:?}"
        );
        ensure!(
            self.decode_label == "NVIDIA NVENC · hardware H.264 decode"
                || self.decode_label == "NVIDIA NVENC · software H.264 decode",
            "authoritative live-view App made an invalid decode-path claim: {:?}",
            self.decode_label
        );
        ensure!(
            self.status.contains("live") && self.error.is_empty(),
            "authoritative live-view App did not reach live state: {self:?}"
        );
        ensure!(
            !software_renderer(&self.body_text.to_ascii_lowercase()),
            "authoritative live-view App exposed a software-renderer warning"
        );
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DecodeIdentity {
    supported: bool,
    smooth: bool,
    power_efficient: bool,
    label: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamAppState {
    #[serde(skip_serializing)]
    document_epoch_ms: f64,
    session_id: String,
    lifecycle: String,
    status: String,
    decode_label: String,
    video_width: u32,
    video_height: u32,
    decoded_frames: u64,
    processed_frames: u64,
    detections: u64,
    dropped_results: u64,
    overlay_width: u32,
    overlay_height: u32,
    observed_at: String,
    freshness_label: String,
    error: String,
    body_text: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RerunLiveFollowState {
    document_epoch_ms: f64,
    viewer_instance: String,
    viewer_state: String,
    recording_id: String,
    timeline: String,
    current_time: f64,
    newest_time: f64,
    lag_seconds: f64,
    time_update_count: u64,
    live_connection_count: u64,
    live_state: String,
    live_frame_count: u64,
    newest_frame_bytes: u64,
    canvas_count: u64,
    loading: bool,
    error: String,
    map_error: String,
}

impl RerunLiveFollowState {
    fn validate_transport_surface(&self) -> Result<()> {
        ensure!(
            self.viewer_state == "open"
                && !self.viewer_instance.is_empty()
                && !self.recording_id.is_empty()
                && self.live_connection_count > 0
                && self.live_state == "connected"
                && self.live_frame_count > 0
                && self.newest_frame_bytes > 0
                && self.canvas_count > 0
                && !self.loading
                && self.error.is_empty()
                && self.map_error.is_empty(),
            "Rerun live surface is not healthy: {self:?}"
        );
        Ok(())
    }

    fn has_live_timeline(&self) -> bool {
        !self.timeline.is_empty() && self.time_update_count > 0
    }

    fn validate_surface(&self) -> Result<()> {
        self.validate_transport_surface()?;
        ensure!(
            self.has_live_timeline()
                && self.current_time.is_finite()
                && self.newest_time.is_finite()
                && self.lag_seconds.is_finite(),
            "Rerun live surface has not published a healthy timeline: {self:?}"
        );
        Ok(())
    }

    fn is_current(&self) -> bool {
        self.lag_seconds <= 0.25 && self.current_time <= self.newest_time
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RerunLiveFollowEvidence {
    initial_follow_seconds: f64,
    stability_seconds: u64,
    viewer_instance: String,
    recording_id: String,
    timeline: String,
    initial_time: f64,
    final_time: f64,
    final_newest_time: f64,
    final_lag_seconds: f64,
    initial_time_update_count: u64,
    final_time_update_count: u64,
    final_live_connection_count: u64,
    initial_live_frame_count: u64,
    final_live_frame_count: u64,
    newest_frame_bytes: u64,
}

impl RerunLiveFollowEvidence {
    fn update_from(&mut self, state: &RerunLiveFollowState) -> Result<()> {
        ensure!(
            state.is_current()
                && state.viewer_instance == self.viewer_instance
                && state.recording_id == self.recording_id
                && state.timeline == self.timeline
                && state.current_time >= self.final_time
                && state.newest_time >= self.final_newest_time
                && state.time_update_count >= self.final_time_update_count
                && state.live_connection_count >= self.final_live_connection_count
                && state.live_frame_count > self.final_live_frame_count
                && state.newest_frame_bytes > 0,
            "Rerun stopped following its live source during visual capture: {self:?} -> {state:?}"
        );
        self.final_time = state.current_time;
        self.final_newest_time = state.newest_time;
        self.final_lag_seconds = state.lag_seconds;
        self.final_time_update_count = state.time_update_count;
        self.final_live_connection_count = state.live_connection_count;
        self.final_live_frame_count = state.live_frame_count;
        self.newest_frame_bytes = state.newest_frame_bytes;
        Ok(())
    }
}

impl StreamAppState {
    fn is_ready(&self) -> bool {
        self.lifecycle == "running"
            && self.status == "running"
            && self.video_width > 0
            && self.video_height > 0
            && self.decoded_frames > 0
            && self.processed_frames > 0
            && self.overlay_width > 0
            && self.overlay_height > 0
            && !self.observed_at.is_empty()
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            self.is_ready() && self.error.is_empty(),
            "Stream App did not display a running live session with video and typed results: \
             {self:?}"
        );
        ensure!(
            uuid::Uuid::parse_str(&self.session_id)?.get_version_num() == 7,
            "Stream App session identity must be UUIDv7"
        );
        ensure!(
            self.decode_label == "hardware H.264 decode"
                || self.decode_label == "software H.264 decode",
            "Stream App made an invalid decode-path claim: {:?}",
            self.decode_label
        );
        let observed_at = chrono::DateTime::parse_from_rfc3339(&self.observed_at)?;
        let result_age = chrono::Utc::now()
            .signed_duration_since(observed_at.with_timezone(&chrono::Utc))
            .num_milliseconds();
        ensure!(
            (0..=5_000).contains(&result_age),
            "Stream App typed overlay result is stale by {result_age} ms: {self:?}"
        );
        ensure!(
            self.freshness_label.starts_with("frame age ")
                && !software_renderer(&self.body_text.to_ascii_lowercase()),
            "Stream App did not expose fresh results or exposed a software-renderer warning"
        );
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamDecodeIdentity {
    supported: bool,
    smooth: bool,
    power_efficient: bool,
    label: String,
}

impl StreamDecodeIdentity {
    fn validate(&self, app_label: &str) -> Result<()> {
        ensure!(
            self.supported && self.smooth,
            "browser does not report supported, smooth H.264 Stream decode: {self:?}"
        );
        let expected = if self.power_efficient {
            "hardware H.264 decode"
        } else {
            "software H.264 decode"
        };
        ensure!(
            self.label == expected && app_label == expected,
            "Stream App decode label disagrees with exact MediaCapabilities result \
             ({expected:?}): {self:?}"
        );
        Ok(())
    }
}

impl DecodeIdentity {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.supported && self.smooth,
            "browser does not report supported, smooth H.264 decode: {self:?}"
        );
        let expected = if self.power_efficient {
            "NVIDIA NVENC · hardware H.264 decode"
        } else {
            "NVIDIA NVENC · software H.264 decode"
        };
        ensure!(
            self.label == expected,
            "generic App decode label {:?} disagrees with MediaCapabilities ({expected:?})",
            self.label
        );
        Ok(())
    }
}

async fn wait_for_document(cdp: &mut Cdp, session_id: &str) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let ready: bool = cdp
            .evaluate(
                session_id,
                r#"document.readyState === "complete" || document.readyState === "interactive""#,
                false,
            )
            .await?;
        if ready {
            return Ok(());
        }
        ensure!(
            tokio::time::Instant::now() < deadline,
            "App host document did not load"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn wait_for_requested_document(
    cdp: &mut Cdp,
    session_id: &str,
    requested_url: &str,
) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let document = cdp
            .evaluate(
                session_id,
                r#"({href:location.href,readyState:document.readyState})"#,
                false,
            )
            .await?;
        if document_is_ready_at(&document, requested_url) {
            return Ok(());
        }
        ensure!(
            tokio::time::Instant::now() < deadline,
            "headed browser target did not load requested URL {requested_url:?}: {document}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn document_is_ready_at(document: &Value, requested_url: &str) -> bool {
    document.get("href").and_then(Value::as_str) == Some(requested_url)
        && document
            .get("readyState")
            .and_then(Value::as_str)
            .is_some_and(|state| state == "complete" || state == "interactive")
}

struct Cdp {
    socket: WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    next_id: u64,
    events: VecDeque<Value>,
    software_renderer_event: bool,
}

impl Cdp {
    async fn connect(url: &str) -> Result<Self> {
        ensure!(
            url.starts_with("ws://"),
            "headed Chrome DevTools WebSocket must be local plaintext ws://"
        );
        let (socket, response) = connect_async(url).await?;
        ensure!(
            response.status() == StatusCode::SWITCHING_PROTOCOLS,
            "Chrome DevTools WebSocket returned {}",
            response.status()
        );
        Ok(Self {
            socket,
            next_id: 1,
            events: VecDeque::new(),
            software_renderer_event: false,
        })
    }

    async fn command(
        &mut self,
        method: &str,
        params: Value,
        session_id: Option<&str>,
    ) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let mut request = serde_json::json!({
            "id": id,
            "method": method,
            "params": params,
        });
        if let Some(session_id) = session_id {
            request["sessionId"] = Value::String(session_id.to_owned());
        }
        self.socket
            .send(Message::Text(serde_json::to_string(&request)?.into()))
            .await?;
        loop {
            let message = self
                .socket
                .next()
                .await
                .context("Chrome DevTools WebSocket closed")??;
            match message {
                Message::Text(text) => {
                    let value: Value = serde_json::from_str(text.as_ref())?;
                    if value.get("id").and_then(Value::as_u64) == Some(id) {
                        if let Some(error) = value.get("error") {
                            bail!("Chrome DevTools `{method}` failed: {error}");
                        }
                        return Ok(value.get("result").cloned().unwrap_or(Value::Null));
                    }
                    if !self.software_renderer_event {
                        let encoded = serde_json::to_string(&value)?.to_ascii_lowercase();
                        self.software_renderer_event = software_renderer(&encoded);
                    }
                    if retain_cdp_event(&value) {
                        const MAX_RETAINED_EVENTS: usize = 512;
                        if self.events.len() == MAX_RETAINED_EVENTS {
                            self.events.pop_front();
                        }
                        self.events.push_back(value);
                    }
                }
                Message::Ping(value) => self.socket.send(Message::Pong(value)).await?,
                Message::Close(frame) => {
                    bail!("Chrome DevTools WebSocket closed unexpectedly: {frame:?}")
                }
                _ => {}
            }
        }
    }

    async fn evaluate<T: serde::de::DeserializeOwned>(
        &mut self,
        session_id: &str,
        expression: &str,
        await_promise: bool,
    ) -> Result<T> {
        let result = self
            .command(
                "Runtime.evaluate",
                serde_json::json!({
                    "expression": expression,
                    "awaitPromise": await_promise,
                    "returnByValue": true,
                    "userGesture": true,
                }),
                Some(session_id),
            )
            .await?;
        if let Some(exception) = result.get("exceptionDetails") {
            bail!("browser evaluation failed: {exception}");
        }
        let value = result
            .pointer("/result/value")
            .cloned()
            .with_context(|| format!("browser evaluation returned no value: {result}"))?;
        serde_json::from_value(value).context("decoding browser evaluation result")
    }

    async fn evaluate_context<T: serde::de::DeserializeOwned>(
        &mut self,
        session_id: &str,
        context_id: u64,
        expression: &str,
        await_promise: bool,
    ) -> Result<T> {
        let result = self
            .command(
                "Runtime.evaluate",
                serde_json::json!({
                    "expression": expression,
                    "contextId": context_id,
                    "awaitPromise": await_promise,
                    "returnByValue": true,
                    "userGesture": true,
                }),
                Some(session_id),
            )
            .await?;
        if let Some(exception) = result.get("exceptionDetails") {
            bail!("browser App-frame evaluation failed: {exception}");
        }
        let value = result
            .pointer("/result/value")
            .cloned()
            .with_context(|| format!("browser App-frame evaluation returned no value: {result}"))?;
        serde_json::from_value(value).context("decoding browser App-frame evaluation result")
    }

    fn assert_no_software_renderer_events(&self) -> Result<()> {
        ensure!(
            !self.software_renderer_event,
            "headed Chrome emitted a software-renderer event"
        );
        Ok(())
    }

    async fn stream_diagnostics(&mut self, session_id: &str) -> Result<String> {
        let mut events = Vec::new();
        for event in self.events.iter().rev() {
            let Some(summary) = stream_event_summary(event) else {
                continue;
            };
            if !events.contains(&summary) {
                events.push(summary);
            }
            if events.len() == 16 {
                break;
            }
        }
        let exception_object_ids = self
            .events
            .iter()
            .rev()
            .filter_map(|event| {
                event
                    .pointer("/params/exceptionDetails/exception/objectId")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .take(4)
            .collect::<Vec<_>>();
        for object_id in exception_object_ids {
            let Some(summary) = self.browser_object_summary(session_id, &object_id, 0).await else {
                continue;
            };
            let summary = format!("browser exception object {summary}");
            if !events.contains(&summary) {
                events.push(summary);
            }
        }
        events.reverse();
        Ok(if events.is_empty() {
            "Chrome reported no WebSocket response or transport error".to_owned()
        } else {
            format!("Chrome WebSocket diagnostics: {}", events.join("; "))
        })
    }

    async fn browser_object_summary(
        &mut self,
        session_id: &str,
        object_id: &str,
        depth: usize,
    ) -> Option<String> {
        let result = self
            .command(
                "Runtime.getProperties",
                serde_json::json!({
                    "objectId": object_id,
                    "ownProperties": true,
                    "accessorPropertiesOnly": false,
                    "generatePreview": true,
                }),
                Some(session_id),
            )
            .await
            .ok()?;
        let mut details = Vec::new();
        for property in result.get("result")?.as_array()? {
            let name = property.get("name")?.as_str()?;
            if !matches!(
                name,
                "action"
                    | "status"
                    | "info"
                    | "name"
                    | "message"
                    | "code"
                    | "description"
                    | "cause"
                    | "reason"
            ) {
                continue;
            }
            let Some(value) = property.get("value") else {
                continue;
            };
            if let Some(primitive) = browser_remote_primitive(value) {
                details.push(format!("{name}={primitive}"));
                continue;
            }
            if depth == 0
                && matches!(name, "info" | "cause" | "reason")
                && let Some(nested_object_id) = value.get("objectId").and_then(Value::as_str)
                && let Some(nested) =
                    Box::pin(self.browser_object_summary(session_id, nested_object_id, depth + 1))
                        .await
            {
                details.push(format!("{name}=({nested})"));
            }
        }
        (!details.is_empty()).then(|| bounded_browser_diagnostic(&details.join(",")))
    }
}

fn browser_remote_primitive(value: &Value) -> Option<String> {
    let primitive = value.get("value")?;
    let rendered = match primitive {
        Value::String(value) => value.to_owned(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::Null => "null".to_owned(),
        Value::Array(_) | Value::Object(_) => return None,
    };
    Some(bounded_browser_diagnostic(&rendered))
}

fn retain_cdp_event(event: &Value) -> bool {
    match event.get("method").and_then(Value::as_str) {
        Some(
            "Network.requestWillBeSent"
            | "Network.responseReceived"
            | "Network.loadingFinished"
            | "Network.loadingFailed"
            | "Network.webSocketCreated"
            | "Network.webSocketHandshakeResponseReceived"
            | "Network.webSocketFrameError"
            | "Network.webSocketClosed",
        ) => true,
        Some("Runtime.exceptionThrown") => true,
        Some("Runtime.consoleAPICalled") => matches!(
            event.pointer("/params/type").and_then(Value::as_str),
            Some("error" | "warning")
        ),
        _ => false,
    }
}

fn stream_event_summary(event: &Value) -> Option<String> {
    match event.get("method").and_then(Value::as_str)? {
        "Network.webSocketCreated" => event
            .pointer("/params/url")
            .and_then(Value::as_str)
            .map(redacted_network_url)
            .map(|url| format!("created {url}")),
        "Network.webSocketHandshakeResponseReceived" => {
            let status = event.pointer("/params/response/status")?.as_u64()?;
            let status_text = event
                .pointer("/params/response/statusText")
                .and_then(Value::as_str)
                .unwrap_or("");
            Some(
                format!("handshake HTTP {status} {status_text}")
                    .trim()
                    .to_owned(),
            )
        }
        "Network.webSocketFrameError" => event
            .pointer("/params/errorMessage")
            .and_then(Value::as_str)
            .map(|error| format!("frame error {error}")),
        "Network.loadingFailed" => event
            .pointer("/params/errorText")
            .and_then(Value::as_str)
            .map(|error| format!("load failed {error}")),
        "Runtime.exceptionThrown" => browser_exception_summary(event),
        "Runtime.consoleAPICalled" => {
            let message = event
                .pointer("/params/args")?
                .as_array()?
                .iter()
                .filter_map(|argument| {
                    argument
                        .get("value")
                        .and_then(Value::as_str)
                        .or_else(|| argument.get("description").and_then(Value::as_str))
                })
                .collect::<Vec<_>>()
                .join(" ");
            (!message.is_empty()).then(|| {
                format!(
                    "browser {} {}",
                    event
                        .pointer("/params/type")
                        .and_then(Value::as_str)
                        .unwrap_or("console"),
                    bounded_browser_diagnostic(&message)
                )
            })
        }
        _ => None,
    }
}

fn browser_exception_summary(event: &Value) -> Option<String> {
    let exception = event.pointer("/params/exceptionDetails/exception");
    let description = exception
        .and_then(|value| value.get("description"))
        .or_else(|| event.pointer("/params/exceptionDetails/text"))
        .and_then(Value::as_str)?;
    let properties = exception
        .and_then(|value| value.pointer("/preview/properties"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|property| {
            let name = property.get("name")?.as_str()?;
            let value = property
                .get("value")
                .or_else(|| property.get("valuePreview"))?
                .as_str()?;
            Some(format!("{name}={value}"))
        })
        .collect::<Vec<_>>()
        .join(",");
    let detail = if properties.is_empty() {
        description.to_owned()
    } else {
        format!("{description} ({properties})")
    };
    Some(format!(
        "browser exception {}",
        bounded_browser_diagnostic(&detail)
    ))
}

fn bounded_browser_diagnostic(value: &str) -> String {
    let lowercase = value.to_ascii_lowercase();
    if [
        "authorization",
        "bearer",
        "access_token",
        "accesstoken",
        "jwt",
    ]
    .iter()
    .any(|needle| lowercase.contains(needle))
    {
        return "[redacted authentication-bearing browser diagnostic]".to_owned();
    }
    value.chars().take(320).collect()
}

fn redacted_network_url(value: &str) -> String {
    let Ok(mut url) = Url::parse(value) else {
        return "unparseable WebSocket URL".to_owned();
    };
    url.set_query(None);
    url.set_fragment(None);
    url.to_string()
}

fn value_string<'a>(value: &'a Value, pointer: &str) -> Result<&'a str> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .with_context(|| format!("Chrome DevTools response omitted {pointer}: {value}"))
}

fn software_renderer(value: &str) -> bool {
    [
        "swiftshader",
        "llvmpipe",
        "software rasterizer",
        "software adapter",
    ]
    .iter()
    .any(|needle| value.contains(needle))
}

const HARDWARE_PREFLIGHT: &str = r#"(async () => {
  let adapter;
  try {
    adapter = await navigator.gpu?.requestAdapter({powerPreference:"high-performance"});
  } catch {
    adapter = undefined;
  }
  const canvas = document.createElement("canvas");
  const webgl = canvas.getContext("webgl2",{failIfMajorPerformanceCaveat:true})
    ?? canvas.getContext("webgl",{failIfMajorPerformanceCaveat:true});
  const debug = webgl?.getExtension("WEBGL_debug_renderer_info");
  const webglVendor = webgl && debug
    ? webgl.getParameter(debug.UNMASKED_VENDOR_WEBGL)
    : webgl?.getParameter(webgl.VENDOR) ?? "";
  const webglRenderer = webgl && debug
    ? webgl.getParameter(debug.UNMASKED_RENDERER_WEBGL)
    : webgl?.getParameter(webgl.RENDERER) ?? "";
  webgl?.getExtension("WEBGL_lose_context")?.loseContext();
  return {
    userAgent:navigator.userAgent,
    webgpuVendor:adapter?.info?.vendor ?? "",
    webgpuArchitecture:adapter?.info?.architecture ?? "",
    webgpuDevice:adapter?.info?.device ?? "",
    webgpuDescription:adapter?.info?.description ?? "",
    webglAvailable:Boolean(webgl),
    webglVendor,
    webglRenderer
  };
})()"#;

const APP_FRAME_VIDEO_STATE: &str = r#"(() => {
  const canvas=document.querySelector(".view canvas");
  const view=canvas?.closest(".view");
  const decodedFrames=Number(canvas?.dataset.decodedFrames ?? 0);
  const frame={meanLuma:0,lumaStandardDeviation:0,minimumLuma:0,maximumLuma:0,pixelSampleError:""};
  try {
    if(!canvas?.width||!canvas?.height) throw new Error("video canvas dimensions are unavailable");
    const sample=document.createElement("canvas");
    sample.width=64;sample.height=36;
    const context=sample.getContext("2d",{willReadFrequently:true});
    if(!context) throw new Error("2D frame sampler is unavailable");
    context.drawImage(canvas,0,0,sample.width,sample.height);
    const pixels=context.getImageData(0,0,sample.width,sample.height).data;
    let sum=0,sumSquares=0,minimum=255,maximum=0,count=0;
    for(let index=0;index<pixels.length;index+=4){
      const luma=0.2126*pixels[index]+0.7152*pixels[index+1]+0.0722*pixels[index+2];
      sum+=luma;sumSquares+=luma*luma;minimum=Math.min(minimum,luma);maximum=Math.max(maximum,luma);count++;
    }
    frame.meanLuma=sum/count;
    frame.lumaStandardDeviation=Math.sqrt(Math.max(0,sumSquares/count-frame.meanLuma*frame.meanLuma));
    frame.minimumLuma=Math.round(minimum);
    frame.maximumLuma=Math.round(maximum);
  }catch(error){frame.pixelSampleError=String(error?.message||error);}
  return {
    documentEpochMs:performance.timeOrigin,
    cameraId:view?.id?.startsWith("view-") ? view.id.slice(5) : "",
    viewerInstanceId:view?.dataset.viewerInstanceId ?? "",
    liveViewId:view?.dataset.liveViewId ?? "",
    streamProductId:view?.dataset.streamProductId ?? "",
    readyState:decodedFrames>0 ? 2 : 0,
    videoWidth:canvas?.width ?? 0,
    videoHeight:canvas?.height ?? 0,
    currentTime:Number(canvas?.dataset.mediaTimeSeconds ?? 0),
    sampledAtMs:performance.now(),
    totalVideoFrames:decodedFrames,
    droppedVideoFrames:Number(canvas?.dataset.droppedFrames ?? 0),
    declaredFrameRateHz:Number(canvas?.dataset.frameRate ?? 0),
    observedFrameRateHz:0,
    ...frame,
    decodeLabel:document.getElementById("decode")?.textContent ?? "",
    status:document.getElementById("status")?.textContent ?? "",
    error:document.getElementById("error")?.hidden === false
      ? document.getElementById("error").textContent : "",
    bodyText:document.body?.innerText ?? ""
  };
})()"#;

const APP_FRAME_GRID_VIDEO_STATES: &str = r#"(() => {
  const error=document.getElementById("error")?.hidden === false
    ? document.getElementById("error").textContent : "";
  return [...document.querySelectorAll(".view")].map((view)=>{
    const canvas=view.querySelector("canvas"),decoded=Number(canvas?.dataset.decodedFrames ?? 0);
    return {
      documentEpochMs:performance.timeOrigin,
      cameraId:view.id.startsWith("view-") ? view.id.slice(5) : "",
      viewerInstanceId:view.dataset.viewerInstanceId ?? "",
      liveViewId:view.dataset.liveViewId ?? "",
      streamProductId:view.dataset.streamProductId ?? "",
      readyState:decoded>0 ? 2 : 0,
      videoWidth:canvas?.width ?? 0,
      videoHeight:canvas?.height ?? 0,
      currentTime:Number(canvas?.dataset.mediaTimeSeconds ?? 0),
      error
    };
  });
})()"#;

const APP_FRAME_VIDEO_CADENCE: &str = r#"(async()=>{
  const canvas=document.querySelector(".view canvas");
  if(!canvas)throw new Error("shared H.264 video canvas is unavailable");
  const view=canvas.closest(".view"),liveViewId=view?.dataset.liveViewId??"",minimumSourceSamples=64,warmupDeadline=Date.now()+60000;
  const waitForFrame=(deadline)=>new Promise((resolve,reject)=>{
    const before=Number(canvas.dataset.decodedFrames??0),remaining=deadline-Date.now();
    if(remaining<=0){reject(new Error("reactive video cadence sample did not complete"));return;}
    const observer=new MutationObserver(()=>{
      const decoded=Number(canvas.dataset.decodedFrames??0);
      if(decoded<=before)return;
      clearTimeout(timeout);observer.disconnect();resolve({now:performance.now(),decoded,mediaTime:Number(canvas.dataset.mediaTimeSeconds??0),receiveToDisplayMs:Number(canvas.dataset.receiveToDisplayMs??0)});
    });
    const timeout=setTimeout(()=>{observer.disconnect();reject(new Error("reactive video cadence sample did not complete"));},remaining);
    observer.observe(canvas,{attributes:true,attributeFilter:["data-decoded-frames"]});
  });
  let warmLive=null;
  while(Date.now()<warmupDeadline){
    warmLive=await read(`uav-sim://session/${sessionId}/live-view/${liveViewId}`);
    if(Number(warmLive.sourceToRenderSamples??0)>=minimumSourceSamples)break;
    await waitForFrame(Math.min(warmupDeadline,Date.now()+3000));
  }
  if(Number(warmLive?.sourceToRenderSamples??0)<minimumSourceSamples)throw new Error("native render warm-up lacked source-to-render samples");
  const warmupFrames=12,sampleIntervals=48,deadline=Date.now()+15000;
  const p95=values=>{const ordered=[...values].sort((a,b)=>a-b);return ordered.length?ordered[Math.max(0,Math.ceil(ordered.length*0.95)-1)]:-1;};
  for(let index=0;index<warmupFrames;index++)await waitForFrame(deadline);
  const first={now:performance.now(),mediaTime:Number(canvas.dataset.mediaTimeSeconds??0),decoded:Number(canvas.dataset.decodedFrames??0),dropped:Number(canvas.dataset.droppedFrames??0)};
  const delivery=[];let last=null;
  for(let index=0;index<sampleIntervals;index++){last=await waitForFrame(deadline);delivery.push(last.receiveToDisplayMs);}
  const live=await read(`uav-sim://session/${sessionId}/live-view/${liveViewId}`),intervalSeconds=(last.now-first.now)/1000;
  const sourceToRenderP95Ms=Number(live.sourceToRenderP95Microseconds??-1000)/1000,observedFps=sampleIntervals/intervalSeconds,boundedFps=Math.max(1,Math.min(Number(canvas.dataset.frameRate??observedFps),observedFps)),receiveToDisplayP95Ms=p95(delivery);
  return {
    intervalSeconds,
    mediaTimeSeconds:last.mediaTime-first.mediaTime,
    presentedFrameIntervals:sampleIntervals,
    decodedVideoFrames:last.decoded-first.decoded,
    droppedVideoFrames:Math.max(0,Number(canvas.dataset.droppedFrames??0)-first.dropped),
    sourceToRenderP95Ms,
    sourceToRenderSamples:Number(live.sourceToRenderSamples??0),
    composedMotionToPhotonUpperBoundP95Ms:sourceToRenderP95Ms+receiveToDisplayP95Ms+2000/boundedFps,
    deliverySamples:delivery.length,
    receiveToDisplayP95Ms
  };
})()"#;

const APP_FRAME_DECODE_IDENTITY: &str = r#"(async () => {
  const video=document.querySelector(".view canvas");
  const result=await navigator.mediaCapabilities.decodingInfo({
    type:"file",
    video:{
      contentType:'video/mp4; codecs="avc1.4d4032"',
      width:Number(video.dataset.sourceWidth),
      height:Number(video.dataset.sourceHeight),
      bitrate:12000000,
      framerate:Number(video.dataset.frameRate)
    }
  });
  return {
    supported:result.supported,
    smooth:result.smooth,
    powerEfficient:result.powerEfficient,
    label:document.getElementById("decode")?.textContent ?? ""
  };
})()"#;

const STREAM_APP_STATE: &str = r#"(() => {
  const video=document.getElementById("video");
  const overlay=document.getElementById("overlay");
  const sessionText=document.getElementById("session")?.textContent ?? "";
  const [sessionId,lifecycle]=sessionText.split(" · ");
  return {
    documentEpochMs:performance.timeOrigin,
    sessionId:sessionId === "no session" ? "" : sessionId,
    lifecycle:lifecycle ?? "",
    status:document.getElementById("status")?.textContent ?? "",
    decodeLabel:document.getElementById("decode")?.textContent ?? "",
    videoWidth:video?.width ?? 0,
    videoHeight:video?.height ?? 0,
    decodedFrames:Number(video?.dataset.decodedFrames ?? 0),
    processedFrames:Number(document.getElementById("frames")?.textContent ?? 0),
    detections:Number(document.getElementById("objects")?.textContent ?? 0),
    droppedResults:Number(document.getElementById("dropped")?.textContent ?? 0),
    overlayWidth:overlay?.width ?? 0,
    overlayHeight:overlay?.height ?? 0,
    observedAt:overlay?.dataset.observedAt ?? "",
    freshnessLabel:document.getElementById("freshness")?.textContent ?? "",
    error:document.getElementById("error")?.hidden === false
      ? document.getElementById("error").textContent : "",
    bodyText:document.body?.innerText ?? ""
  };
})()"#;

const STREAM_APP_ENSURE_SESSION: &str = r#"(() => {
  const session=document.getElementById("session")?.textContent ?? "";
  if(session && session !== "no session") return "available";
  const start=document.getElementById("start");
  const pipeline=document.getElementById("pipeline");
  if(!start || !pipeline || pipeline.options.length === 0) return "waiting";
  if(start.dataset.acceptanceStartRequested === "true") return "starting";
  start.dataset.acceptanceStartRequested="true";
  start.click();
  return "started";
})()"#;

const STREAM_APP_DECODE_IDENTITY: &str = r#"(async () => {
  const video=document.getElementById("video");
  const codec=video.dataset.codec;
  const result=await navigator.mediaCapabilities.decodingInfo({
    type:"file",
    video:{
      contentType:`video/mp4; codecs="${codec}"`,
      width:Number(video.dataset.sourceWidth),
      height:Number(video.dataset.sourceHeight),
      bitrate:Number(video.dataset.expectedBitrateBps),
      framerate:Number(video.dataset.frameRate)
    }
  });
  return {
    supported:result.supported,
    smooth:result.smooth,
    powerEfficient:result.powerEfficient,
    label:document.getElementById("decode")?.textContent ?? ""
  };
})()"#;

const RERUN_LIVE_FOLLOW_STATE: &str = r#"(() => {
  const host=document.querySelector(".rerun-web-viewer-host");
  return {
    documentEpochMs:performance.timeOrigin,
    viewerInstance:host?.dataset.rerunViewerInstance ?? "",
    viewerState:host?.dataset.rerunViewerState ?? "",
    recordingId:host?.dataset.rerunRecordingId ?? "",
    timeline:host?.dataset.rerunTimeline ?? "",
    currentTime:Number(host?.dataset.rerunCurrentTime ?? 0),
    newestTime:Number(host?.dataset.rerunNewestTime ?? 0),
    lagSeconds:Number(host?.dataset.rerunLiveLagSeconds ?? 0),
    timeUpdateCount:Number(host?.dataset.rerunTimeUpdateCount ?? 0),
    liveConnectionCount:Number(host?.dataset.rerunLiveConnectionCount ?? 0),
    liveState:host?.dataset.rerunLiveState ?? "",
    liveFrameCount:Number(host?.dataset.rerunLiveFrameCount ?? 0),
    newestFrameBytes:Number(host?.dataset.rerunLiveNewestFrameBytes ?? 0),
    canvasCount:host?.querySelectorAll("canvas").length ?? 0,
    loading:Boolean(document.querySelector(".recording-viewer-state")),
    error:document.querySelector(".recording-viewer-error")?.textContent ?? "",
    mapError:document.querySelector(".recording-viewer-map-error")?.textContent ?? ""
  };
})()"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn console_app_render(status: &str) -> ConsoleAppRenderState {
        ConsoleAppRenderState {
            title: "Workbench".to_owned(),
            heading: "Workbench".to_owned(),
            status: status.to_owned(),
            body: "Workbench".to_owned(),
            host_display_mode: "inline".to_owned(),
            host_bridge_error: String::new(),
            visible_errors: Vec::new(),
            required_selector_found: true,
        }
    }

    fn rerun_follow_state(timeline_ready: bool) -> RerunLiveFollowState {
        RerunLiveFollowState {
            document_epoch_ms: 1.0,
            viewer_instance: "viewer-1".to_owned(),
            viewer_state: "open".to_owned(),
            recording_id: "recording-1".to_owned(),
            timeline: if timeline_ready {
                "simulation_time"
            } else {
                ""
            }
            .to_owned(),
            current_time: 4.0,
            newest_time: 4.0,
            lag_seconds: 0.0,
            time_update_count: u64::from(timeline_ready),
            live_connection_count: 1,
            live_state: "connected".to_owned(),
            live_frame_count: 3,
            newest_frame_bytes: 128,
            canvas_count: 1,
            loading: false,
            error: String::new(),
            map_error: String::new(),
        }
    }

    #[test]
    fn reactive_video_cadence_requires_declared_delivery() {
        let accepted = VideoCadenceSample {
            interval_seconds: 2.0,
            media_time_seconds: 2.0,
            presented_frame_intervals: 48,
            decoded_video_frames: 48,
            dropped_video_frames: 0,
            source_to_render_p95_ms: 18.0,
            source_to_render_samples: 48,
            composed_motion_to_photon_upper_bound_p95_ms: 85.0,
            delivery_samples: 48,
            receive_to_display_p95_ms: 7.0,
        };
        accepted.validate(24.0).unwrap();
        assert!(!accepted.needs_steady_state_sample(24.0));

        let slow = VideoCadenceSample {
            interval_seconds: 5.0,
            ..accepted
        };
        assert!(slow.validate(24.0).is_err());
        assert!(slow.needs_steady_state_sample(24.0));

        let stale_pipeline = VideoCadenceSample {
            source_to_render_p95_ms: 85.0,
            ..accepted
        };
        assert!(stale_pipeline.validate(24.0).is_err());
        assert!(stale_pipeline.needs_steady_state_sample(24.0));

        let slow_transport = VideoCadenceSample {
            composed_motion_to_photon_upper_bound_p95_ms: 250.0,
            ..accepted
        };
        assert!(slow_transport.validate(24.0).is_err());
        assert!(slow_transport.needs_steady_state_sample(24.0));
    }

    #[test]
    fn rerun_transport_can_precede_its_first_timeline_update() {
        let state = rerun_follow_state(false);
        state.validate_transport_surface().unwrap();
        assert!(!state.has_live_timeline());
        assert!(state.validate_surface().is_err());
    }

    #[test]
    fn rerun_surface_becomes_healthy_after_its_timeline_update() {
        let state = rerun_follow_state(true);
        assert!(state.has_live_timeline());
        state.validate_surface().unwrap();
    }

    #[test]
    fn console_acceptance_url_bypasses_stale_entry_documents() {
        let page = Url::parse(&console_acceptance_url(
            "https://installation.example/",
            "/apps/uav-sim/live.html",
        ))
        .unwrap();
        let nonce = page
            .query_pairs()
            .find_map(|(key, value)| (key == "veoveo-acceptance").then_some(value.into_owned()))
            .unwrap();

        assert_eq!(page.path(), "/console/");
        assert_eq!(page.fragment(), Some("/apps/uav-sim/live.html"));
        assert!(uuid::Uuid::parse_str(&nonce).is_ok());
    }

    #[test]
    fn requested_document_readiness_rejects_the_initial_about_blank_target() {
        let requested = "https://installation.example/console/#/apps";
        assert!(!document_is_ready_at(
            &serde_json::json!({"href": "about:blank", "readyState": "complete"}),
            requested
        ));
        assert!(document_is_ready_at(
            &serde_json::json!({"href": requested, "readyState": "interactive"}),
            requested
        ));
    }

    #[test]
    fn standalone_acceptance_url_uses_the_canonical_no_store_route() {
        let page = Url::parse(&standalone_acceptance_url(
            "https://installation.example/",
            "/apps/uav-sim/live.html",
        ))
        .unwrap();
        assert_eq!(page.path(), "/apps/uav-sim/live.html");
        assert!(page.query().is_none());
        assert!(page.fragment().is_none());
    }

    #[test]
    fn console_app_body_must_finish_after_the_transient_empty_document() {
        assert!(!console_app_body_ready(
            &serde_json::json!({"readyState": "complete", "body": ""}),
            "Live Cameras"
        ));
        assert!(console_app_body_ready(
            &serde_json::json!({
                "readyState": "complete",
                "body": "Live Cameras\nNVIDIA NVENC · H.264"
            }),
            "Live Cameras"
        ));
    }

    #[test]
    fn console_app_acceptance_rejects_transient_and_visible_error_states() {
        let expectation = ConsoleAppExpectation {
            server: "duckdb",
            resource_uri: "ui://duckdb/workbench.html",
            marker: "Workbench",
            settled_state: ConsoleAppSettledState::Exact(&["ready"]),
            required_selector: None,
        };
        assert!(!console_app_has_settled(
            &expectation,
            &console_app_render("reading")
        ));
        let mut errored = console_app_render("ready");
        errored.visible_errors.push("resource failed".to_owned());
        assert!(!console_app_has_settled(&expectation, &errored));
        assert!(console_app_has_settled(
            &expectation,
            &console_app_render("ready")
        ));
    }

    #[test]
    fn console_map_acceptance_requires_a_completed_viewport_status() {
        let expectation = ConsoleAppExpectation {
            server: "map",
            resource_uri: "ui://map/workspace.html",
            marker: "Workspace",
            settled_state: ConsoleAppSettledState::MapViewport,
            required_selector: Some(".maplibregl-canvas"),
        };
        assert!(!console_app_has_settled(
            &expectation,
            &ConsoleAppRenderState {
                heading: "Workspace".to_owned(),
                ..console_app_render("Map resources loaded.")
            }
        ));
        assert!(console_app_has_settled(
            &expectation,
            &ConsoleAppRenderState {
                heading: "Workspace".to_owned(),
                ..console_app_render("0 features visible across 2 layers.")
            }
        ));
    }

    #[test]
    fn chrome_version_uses_the_cdp_websocket_wire_casing() {
        let version: ChromeVersion = serde_json::from_value(serde_json::json!({
            "Browser": "Chrome/150.0.7871.186",
            "Protocol-Version": "1.3",
            "webSocketDebuggerUrl": "ws://127.0.0.1:9227/devtools/browser/id"
        }))
        .unwrap();
        assert_eq!(
            version.web_socket_debugger_url,
            "ws://127.0.0.1:9227/devtools/browser/id"
        );
    }

    #[test]
    fn stream_diagnostics_omit_request_headers_and_url_queries() {
        let created = serde_json::json!({
            "method": "Network.webSocketCreated",
            "params": {
                "url": "ws://localhost:8782/uav-sim/signaling/sign_in?peer_id=peer",
                "initiator": {
                    "requestHeaders": {
                        "Sec-WebSocket-Protocol": "authorization.bearer.secret"
                    }
                }
            }
        });
        let response = serde_json::json!({
            "method": "Network.webSocketHandshakeResponseReceived",
            "params": {
                "response": {
                    "status": 403,
                    "statusText": "Forbidden",
                    "headers": {
                        "Sec-WebSocket-Protocol": "authorization.bearer.secret"
                    }
                }
            }
        });
        let summaries = [&created, &response]
            .into_iter()
            .filter_map(stream_event_summary)
            .collect::<Vec<_>>()
            .join("; ");
        assert_eq!(
            summaries,
            "created ws://localhost:8782/uav-sim/signaling/sign_in; \
             handshake HTTP 403 Forbidden"
        );
        assert!(!summaries.contains("secret"));
        assert!(!summaries.contains("peer"));
    }

    #[test]
    fn only_destroyed_app_execution_contexts_are_reacquired() {
        assert!(stale_execution_context(&anyhow!(
            "Chrome DevTools `Runtime.evaluate` failed: \
             {{\"code\":-32000,\"message\":\"Cannot find context with specified id\"}}"
        )));
        assert!(stale_execution_context(&anyhow!(
            "Execution context was destroyed."
        )));
        assert!(!stale_execution_context(&anyhow!(
            "browser App-frame evaluation failed"
        )));
        assert!(stale_app_frame(&anyhow!(
            "Chrome DevTools `Page.createIsolatedWorld` failed: \
             {{\"code\":-32602,\"message\":\"No frame for given id found\"}}"
        )));
        assert!(stale_app_frame(&anyhow!(
            "Chrome DevTools `DOM.describeNode` failed: \
             {{\"code\":-32000,\"message\":\"Could not find node with given id\"}}"
        )));
        assert!(stale_execution_context(&anyhow!(
            "Chrome DevTools `Runtime.evaluate` failed: \
             {{\"code\":-32001,\"message\":\"Session closed. Most likely the target has been closed.\"}}"
        )));
        assert!(!stale_app_frame(&anyhow!(
            "Chrome frame tree omitted its root"
        )));
    }

    #[test]
    fn software_renderer_fingerprints_fail_closed() {
        assert!(software_renderer("google swiftshader"));
        assert!(software_renderer("mesa llvmpipe"));
        assert!(software_renderer("software rasterizer warning"));
        assert!(!software_renderer("nvidia geforce rtx 4090"));
    }

    #[test]
    fn cdp_retains_only_events_used_by_acceptance_evidence() {
        assert!(retain_cdp_event(&serde_json::json!({
            "method": "Network.requestWillBeSent"
        })));
        assert!(retain_cdp_event(&serde_json::json!({
            "method": "Network.webSocketFrameError"
        })));
        assert!(!retain_cdp_event(&serde_json::json!({
            "method": "Runtime.consoleAPICalled"
        })));
        assert!(!retain_cdp_event(&serde_json::json!({
            "method": "Page.lifecycleEvent"
        })));
    }

    #[test]
    fn either_hardware_browser_api_satisfies_preflight() {
        let webgl_only = HardwareIdentity {
            user_agent: "Chrome".to_owned(),
            webgpu_vendor: "Google".to_owned(),
            webgpu_architecture: "SwiftShader".to_owned(),
            webgpu_device: String::new(),
            webgpu_description: String::new(),
            webgl_available: true,
            webgl_vendor: "Google Inc. (NVIDIA Corporation)".to_owned(),
            webgl_renderer: "ANGLE (NVIDIA GeForce RTX 4090)".to_owned(),
        };
        assert_eq!(webgl_only.hardware_apis().0, vec![BrowserGpuApi::WebGl]);
        webgl_only.validate().unwrap();

        let webgpu_only = HardwareIdentity {
            user_agent: "Chrome".to_owned(),
            webgpu_vendor: "NVIDIA".to_owned(),
            webgpu_architecture: "Lovelace".to_owned(),
            webgpu_device: "RTX 4090".to_owned(),
            webgpu_description: String::new(),
            webgl_available: false,
            webgl_vendor: String::new(),
            webgl_renderer: String::new(),
        };
        assert_eq!(webgpu_only.hardware_apis().0, vec![BrowserGpuApi::WebGpu]);
        webgpu_only.validate().unwrap();
    }

    #[test]
    fn browser_preflight_rejects_two_software_apis() {
        let software_only = HardwareIdentity {
            user_agent: "Chrome".to_owned(),
            webgpu_vendor: "Google".to_owned(),
            webgpu_architecture: "SwiftShader".to_owned(),
            webgpu_device: String::new(),
            webgpu_description: String::new(),
            webgl_available: true,
            webgl_vendor: "Google".to_owned(),
            webgl_renderer: "ANGLE (SwiftShader)".to_owned(),
        };
        assert!(software_only.hardware_apis().0.is_empty());
        assert!(software_only.validate().is_err());
    }

    #[test]
    fn finds_the_sandboxed_uav_live_view_frame_in_a_cdp_tree() {
        let tree = serde_json::json!({
            "frame": {"id": "root", "url": "https://installation.example/console/"},
            "childFrames": [{
                "frame": {
                    "id": "app",
                    "url": "https://installation.example/console/api/apps/frame?uri=ui%3A%2F%2Fuav-sim%2Flive.html"
                }
            }]
        });
        assert_eq!(find_app_frame_id(&tree, "uav-sim"), Some("app"));
    }

    #[test]
    fn finds_only_the_console_tabs_swapped_uav_live_view_target() {
        let targets = serde_json::json!({
            "targetInfos": [
                {
                    "targetId": "other-app",
                    "type": "iframe",
                    "parentId": "other-console",
                    "url": "https://installation.example/console/api/apps/frame?uri=ui%3A%2F%2Fuav-sim%2Flive.html"
                },
                {
                    "targetId": "stream-app",
                    "type": "iframe",
                    "parentId": "console",
                    "url": "https://installation.example/console/api/apps/frame?uri=ui%3A%2F%2Fstream%2Flive.html"
                },
                {
                    "targetId": "simulation-app",
                    "type": "iframe",
                    "parentId": "console",
                    "url": "https://installation.example/console/api/apps/frame?uri=ui%3A%2F%2Fuav-sim%2Flive.html"
                }
            ]
        });
        assert_eq!(
            find_app_target_id(&targets, "console", "uav-sim"),
            Some("simulation-app")
        );
    }

    #[test]
    fn finds_the_sandboxed_uav_live_view_frame_in_a_dom_node() {
        let node = serde_json::json!({
            "nodeName": "IFRAME",
            "attributes": [
                "class",
                "app-frame",
                "src",
                "/console/api/apps/frame?uri=ui%3A%2F%2Fuav-sim%2Flive.html"
            ],
            "frameId": "app"
        });
        assert_eq!(app_frame_id_from_dom_node(&node, "uav-sim"), Some("app"));
    }

    #[test]
    fn grid_video_requires_an_advancing_native_product_identity() {
        let ready = GridVideoState {
            document_epoch_ms: 10.0,
            camera_id: "follow".to_owned(),
            viewer_instance_id: "browser-a".to_owned(),
            live_view_id: "view-a".to_owned(),
            stream_product_id: "product-a".to_owned(),
            ready_state: 4,
            video_width: 1280,
            video_height: 720,
            current_time: 1.0,
            error: String::new(),
        };
        assert!(ready.validate().is_ok());

        let missing_product = GridVideoState {
            stream_product_id: String::new(),
            ..ready
        };
        assert!(missing_product.validate().is_err());
    }
}
