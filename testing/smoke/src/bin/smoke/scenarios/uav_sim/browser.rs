use std::{collections::VecDeque, path::Path, sync::Arc};

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
    RerunCameraRenderEvidence, RerunRenderEvidence, analyze_rerun_camera_render,
    analyze_rerun_render,
};

#[path = "browser/recording_acceptance.rs"]
mod recording_acceptance;

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

#[allow(dead_code)] // Viewer-slot identity is consumed by the focused browser binary.
impl ConsoleLiveCaptureEvidence {
    pub(crate) fn viewer_instance_id(&self) -> &str {
        &self.video.viewer_instance_id
    }

    pub(crate) fn live_view_id(&self) -> &str {
        &self.video.live_view_id
    }

    pub(crate) fn stream_product_id(&self) -> &str {
        &self.video.stream_product_id
    }

    pub(crate) fn capacity_slot(&self) -> u16 {
        self.video.capacity_slot
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
        ),
    );
    let (first, second) = tokio::join!(first, second);
    Ok((
        first.with_context(|| format!("first Console live-App capture exceeded {timeout:?}"))??,
        second
            .with_context(|| format!("second Console live-App capture exceeded {timeout:?}"))??,
    ))
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

async fn preflight_console_live_app_inner(cdp_base: &str, page_url: &str) -> Result<()> {
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
            "UAV live cameras",
        )
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

async fn capture_console_live_app_inner(
    cdp_base: &str,
    page_url: &str,
    expected_camera_id: &str,
    screenshot_path: &Path,
    simultaneous: Option<Arc<tokio::sync::Barrier>>,
) -> Result<ConsoleLiveCaptureEvidence> {
    let (mut cdp, target_id, session_id) = open_headed_target(cdp_base, page_url).await?;
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
        let second = wait_for_console_video_advance(
            &mut cdp,
            &target_id,
            &session_id,
            expected_camera_id,
            first,
        )
        .await?;
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
            simultaneous.wait().await;
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
                    .is_some_and(|title| title.contains("UAV live cameras"))
                && snapshot
                    .get("bodyText")
                    .and_then(Value::as_str)
                    .is_some_and(|body| body.contains("UAV live cameras")),
            "real Console did not load its snapshot, App catalog, and UAV live view frame: \
             {snapshot}"
        );
        let screenshot_sha256 =
            capture_screenshot(&mut cdp, &session_id, screenshot_path).await?;
        close_console_live_view(&mut cdp, &target_id, &session_id, expected_camera_id).await?;
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
    let close = close_target(&mut cdp, &target_id).await;
    let evidence = acceptance?;
    close?;
    Ok(evidence)
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
                    .is_some_and(|title| title.contains("Stream"))
                && console
                    .get("bodyText")
                    .and_then(Value::as_str)
                    .is_some_and(|body| body.contains("Stream")),
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
        let deadline = tokio::time::Instant::now() + Duration::from_secs(300);
        loop {
            let state: Value = cdp
                .evaluate(
                    &session_id,
                    r#"(() => ({
                      recordingVisible:document.body?.innerText?.includes("recording://recordings/") ?? false,
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
                state.get("error").and_then(Value::as_str).unwrap_or("").is_empty(),
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
                && state.get("canvasCount").and_then(Value::as_u64).unwrap_or(0) > 0
                && state.get("loading").and_then(Value::as_bool) == Some(false)
                && body.contains(recording_id)
            {
                break;
            }
            ensure!(
                tokio::time::Instant::now() < deadline,
                "Console did not render governed recording {recording_id}: {state}"
            );
            assert_page_visible(&mut cdp, &session_id).await?;
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
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
        let viewer_bounds: ElementBounds = cdp
            .evaluate(
                &session_id,
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
            .context("Console did not expose the Rerun viewport bounds")?;
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

async fn open_headed_target(cdp_base: &str, page_url: &str) -> Result<(Cdp, String, String)> {
    let mut cdp = connect_headed_browser(cdp_base, "visual acceptance").await?;
    let target = cdp
        .command(
            "Target.createTarget",
            serde_json::json!({"url": page_url, "newWindow": false}),
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
        .map(str::to_owned)
        .or(find_app_frame_id_from_dom(cdp, session_id, app_server).await?);
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
    format!("{error:#}").contains("No frame for given id found")
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
    mut first: AppVideoState,
) -> Result<AppVideoState> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(180);
    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let second = wait_for_console_video(cdp, target_id, session_id, expected_camera_id).await?;
        if second.document_epoch_ms == first.document_epoch_ms
            && second.current_time > first.current_time + 0.25
        {
            return Ok(second);
        }
        ensure!(
            tokio::time::Instant::now() < deadline,
            "Console H.264 video did not remain mounted and advance: {:?} -> {:?}",
            first,
            second
        );
        first = second;
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
    let expected = serde_json::to_string(expected_camera_id)?;
    let requested: bool = evaluate_console_app(
        cdp,
        target_id,
        session_id,
        "uav-sim",
        &format!(
            r#"(() => {{
                  const input=[...document.querySelectorAll('#choices input[type="checkbox"]')]
                    .find((candidate)=>candidate.parentElement?.textContent?.trim().startsWith({expected}));
                  if (!input) return false;
                  if (input.checked) input.click();
                  return true;
                }})()"#
        ),
        false,
    )
    .await?;
    ensure!(
        requested,
        "Console could not close follow-camera stream {expected_camera_id}"
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
                  videoCount:document.querySelectorAll("video").length,
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
            "Console failed to close follow-camera stream: {state}"
        );
        if state.get("checked").and_then(Value::as_bool) == Some(false)
            && state.get("videoCount").and_then(Value::as_u64) == Some(0)
        {
            return Ok(());
        }
        ensure!(
            tokio::time::Instant::now() < deadline,
            "Console did not close follow-camera stream within 30 seconds: {state}"
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
    capacity_slot: u16,
    ready_state: u16,
    video_width: u32,
    video_height: u32,
    current_time: f64,
    decode_label: String,
    status: String,
    error: String,
    body_text: String,
    rtc_states: Vec<String>,
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
            "authoritative live-view App omitted its isolated viewer-slot identity: {self:?}"
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
            "browser does not report supported, smooth H.264 WebRTC decode: {self:?}"
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
  const video=document.querySelector("video");
  const view=video?.closest(".view");
  const camera=document.querySelector(`#choices input[type="checkbox"]:checked`);
  return {
    documentEpochMs:performance.timeOrigin,
    cameraId:camera?.parentElement?.textContent?.split(" · ")[0]?.trim() ?? "",
    viewerInstanceId:view?.dataset.viewerInstanceId ?? "",
    liveViewId:view?.dataset.liveViewId ?? "",
    streamProductId:view?.dataset.streamProductId ?? "",
    capacitySlot:Number(view?.dataset.capacitySlot ?? 0),
    readyState:video?.readyState ?? 0,
    videoWidth:video?.videoWidth ?? 0,
    videoHeight:video?.videoHeight ?? 0,
    currentTime:video?.currentTime ?? 0,
    decodeLabel:document.getElementById("decode")?.textContent ?? "",
    status:document.getElementById("status")?.textContent ?? "",
    error:document.getElementById("error")?.hidden === false
      ? document.getElementById("error").textContent : "",
    bodyText:document.body?.innerText ?? "",
    rtcStates:[]
  };
})()"#;

const APP_FRAME_DECODE_IDENTITY: &str = r#"(async () => {
  const video=document.querySelector("video");
  const result=await navigator.mediaCapabilities.decodingInfo({
    type:"webrtc",
    video:{
      contentType:'video/H264; codecs="avc1.42E01E"',
      width:video.videoWidth,
      height:video.videoHeight,
      bitrate:8000000,
      framerate:30
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
    fn console_app_body_must_finish_after_the_transient_empty_document() {
        assert!(!console_app_body_ready(
            &serde_json::json!({"readyState": "complete", "body": ""}),
            "UAV live cameras"
        ));
        assert!(console_app_body_ready(
            &serde_json::json!({
                "readyState": "complete",
                "body": "UAV live cameras\nNVIDIA NVENC · H.264"
            }),
            "UAV live cameras"
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
}
