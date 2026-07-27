use std::{collections::BTreeMap, path::Path, sync::Arc};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Message, http::StatusCode},
};
use url::Url;

use super::*;

pub(super) struct BrowserFixture {
    pub app_html: String,
    pub resources: BTreeMap<String, Value>,
    pub connection: Value,
    pub expected_camera_id: String,
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
pub(crate) struct ConsoleRecordingCaptureEvidence {
    schema: &'static str,
    captured_at: chrono::DateTime<chrono::Utc>,
    page_url: String,
    recording_id: String,
    screenshot_path: String,
    screenshot_sha256: String,
    hardware: HardwareIdentity,
}

struct AppHost {
    url: String,
    shutdown: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<Result<()>>,
}

impl AppHost {
    async fn start(fixture: BrowserFixture) -> Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let address = listener.local_addr()?;
        let parent = Arc::<str>::from(parent_html(&fixture)?);
        let app = Arc::<str>::from(fixture.app_html);
        let (shutdown, mut stopping) = oneshot::channel();
        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut stopping => return Ok(()),
                    accepted = listener.accept() => {
                        let (stream, _) = accepted?;
                        let parent = parent.clone();
                        let app = app.clone();
                        tokio::spawn(async move {
                            if let Err(error) = serve_request(stream, parent, app).await {
                                eprintln!("Simulation View App host request failed: {error:#}");
                            }
                        });
                    }
                }
            }
        });
        Ok(Self {
            url: format!("http://{address}/"),
            shutdown: Some(shutdown),
            task,
        })
    }

    async fn close(mut self) -> Result<()> {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        self.task.await?
    }
}

pub(super) async fn verify_live_app_in_hardware_browser(
    cdp_base: &str,
    fixture: BrowserFixture,
    timeout: Duration,
) -> Result<()> {
    let expected_camera_id = fixture.expected_camera_id.clone();
    let host = AppHost::start(fixture).await?;
    let result = tokio::time::timeout(
        timeout,
        verify_browser_inner(cdp_base, &host.url, &expected_camera_id),
    )
    .await
    .with_context(|| format!("hardware browser acceptance exceeded {timeout:?}"))?;
    let host_result = host.close().await;
    result?;
    host_result
}

pub(crate) async fn capture_console_live_app(
    cdp_base: &str,
    public_base_url: &str,
    expected_camera_id: &str,
    screenshot_path: &Path,
    timeout: Duration,
) -> Result<ConsoleLiveCaptureEvidence> {
    let page_url = format!(
        "{}/console/#/apps/simulation-view/live.html",
        public_base_url.trim_end_matches('/')
    );
    tokio::time::timeout(
        timeout,
        capture_console_live_app_inner(cdp_base, &page_url, expected_camera_id, screenshot_path),
    )
    .await
    .with_context(|| format!("Console live-App capture exceeded {timeout:?}"))?
}

pub(crate) async fn capture_console_recording(
    cdp_base: &str,
    public_base_url: &str,
    recording_id: &str,
    screenshot_path: &Path,
    timeout: Duration,
) -> Result<ConsoleRecordingCaptureEvidence> {
    let page_url = format!(
        "{}/console/#/recordings/{}",
        public_base_url.trim_end_matches('/'),
        recording_id
    );
    tokio::time::timeout(
        timeout,
        capture_console_recording_inner(cdp_base, &page_url, recording_id, screenshot_path),
    )
    .await
    .with_context(|| format!("Console Rerun capture exceeded {timeout:?}"))?
}

async fn capture_console_live_app_inner(
    cdp_base: &str,
    page_url: &str,
    expected_camera_id: &str,
    screenshot_path: &Path,
) -> Result<ConsoleLiveCaptureEvidence> {
    let (mut cdp, target_id, session_id) = open_headed_target(cdp_base, page_url).await?;
    let acceptance = async {
        wait_for_document(&mut cdp, &session_id).await?;
        assert_page_visible(&mut cdp, &session_id).await?;
        let hardware: HardwareIdentity =
            cdp.evaluate(&session_id, HARDWARE_PREFLIGHT, true).await?;
        hardware.validate()?;
        let app_context = wait_for_console_app_context(&mut cdp, &session_id).await?;
        let ready = wait_for_console_app_camera(
            &mut cdp,
            &session_id,
            app_context,
            expected_camera_id,
        )
        .await?;
        ensure!(
            ready,
            "Console Simulation View App exposed no camera {expected_camera_id:?}"
        );
        let first = wait_for_console_video(
            &mut cdp,
            &session_id,
            app_context,
            expected_camera_id,
        )
        .await?;
        tokio::time::sleep(Duration::from_secs(2)).await;
        let second: AppVideoState = cdp
            .evaluate_context(&session_id, app_context, APP_FRAME_VIDEO_STATE, false)
            .await?;
        second.validate(expected_camera_id)?;
        ensure!(
            second.current_time > first.current_time + 0.25,
            "Console H.264 video did not advance: {} -> {}",
            first.current_time,
            second.current_time
        );
        let decode: DecodeIdentity = cdp
            .evaluate_context(&session_id, app_context, APP_FRAME_DECODE_IDENTITY, true)
            .await?;
        decode.validate()?;
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
                    .is_some_and(|title| title.contains("Simulation"))
                && snapshot
                    .get("bodyText")
                    .and_then(Value::as_str)
                    .is_some_and(|body| body.contains("Simulation live views")),
            "real Console did not load its snapshot, App catalog, and Simulation View frame: \
             {snapshot}"
        );
        let screenshot_sha256 =
            capture_screenshot(&mut cdp, &session_id, screenshot_path).await?;
        close_console_live_view(&mut cdp, &session_id, app_context, expected_camera_id).await?;
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
        assert_page_visible(&mut cdp, &session_id).await?;
        let final_hardware: HardwareIdentity =
            cdp.evaluate(&session_id, HARDWARE_PREFLIGHT, true).await?;
        final_hardware.validate()?;
        let screenshot_sha256 =
            capture_screenshot(&mut cdp, &session_id, screenshot_path).await?;
        cdp.assert_no_software_renderer_events()?;
        Ok(ConsoleRecordingCaptureEvidence {
            schema: "veoveo.io/uav-console-recording-capture/v1",
            captured_at: chrono::Utc::now(),
            page_url: page_url.to_owned(),
            recording_id: recording_id.to_owned(),
            screenshot_path: screenshot_path.display().to_string(),
            screenshot_sha256,
            hardware: final_hardware,
        })
    }
    .await;
    let close = close_target(&mut cdp, &target_id).await;
    let evidence = acceptance?;
    close?;
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

async fn wait_for_console_app_context(cdp: &mut Cdp, session_id: &str) -> Result<u64> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        let tree = cdp
            .command("Page.getFrameTree", serde_json::json!({}), Some(session_id))
            .await?;
        if let Some(frame_id) = find_app_frame_id(
            tree.get("frameTree")
                .context("Chrome frame tree omitted its root")?,
        ) {
            let isolated = cdp
                .command(
                    "Page.createIsolatedWorld",
                    serde_json::json!({
                        "frameId": frame_id,
                        "worldName": "veoveo-uav-acceptance",
                        "grantUniveralAccess": false
                    }),
                    Some(session_id),
                )
                .await?;
            return isolated
                .get("executionContextId")
                .and_then(Value::as_u64)
                .context("Chrome did not create an App-frame execution context");
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
            "authenticated Console did not load the Simulation View App frame: {state}"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

fn find_app_frame_id(frame_tree: &Value) -> Option<&str> {
    let frame = frame_tree.get("frame")?;
    let url = frame.get("url").and_then(Value::as_str).unwrap_or_default();
    if url.contains("/console/api/apps/frame") && url.contains("simulation-view") {
        return frame.get("id").and_then(Value::as_str);
    }
    frame_tree
        .get("childFrames")
        .and_then(Value::as_array)?
        .iter()
        .find_map(find_app_frame_id)
}

async fn wait_for_console_app_camera(
    cdp: &mut Cdp,
    session_id: &str,
    context_id: u64,
    expected_camera_id: &str,
) -> Result<bool> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    let expected = serde_json::to_string(expected_camera_id)?;
    loop {
        let expression = format!(
            r#"(() => {{
              const input=[...document.querySelectorAll('#cameras input[type="checkbox"]')]
                .find((candidate)=>candidate.parentElement?.textContent?.trim().startsWith({expected}));
              if (!input) return {{found:false,error:document.getElementById("error")?.hidden === false
                ? document.getElementById("error").textContent : "",body:document.body?.innerText ?? ""}};
              if (!input.checked) input.click();
              return {{found:true,error:"",body:document.body?.innerText ?? ""}};
            }})()"#
        );
        let state: Value = cdp
            .evaluate_context(session_id, context_id, &expression, false)
            .await?;
        if state.get("found").and_then(Value::as_bool) == Some(true) {
            return Ok(true);
        }
        ensure!(
            state
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("")
                .is_empty(),
            "Console Simulation View App failed during camera discovery: {state}"
        );
        ensure!(
            tokio::time::Instant::now() < deadline,
            "Console Simulation View App did not discover camera {expected_camera_id}: {state}"
        );
        cdp.assert_no_software_renderer_events()?;
        tokio::time::sleep(Duration::from_millis(250)).await;
        assert_page_visible(cdp, session_id).await?;
    }
}

async fn wait_for_console_video(
    cdp: &mut Cdp,
    session_id: &str,
    context_id: u64,
    expected_camera_id: &str,
) -> Result<AppVideoState> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(180);
    loop {
        let state: AppVideoState = cdp
            .evaluate_context(session_id, context_id, APP_FRAME_VIDEO_STATE, false)
            .await?;
        if state.ready_state >= 2 && state.video_width > 0 && state.current_time > 0.0 {
            state.validate(expected_camera_id)?;
            return Ok(state);
        }
        ensure!(
            state.error.is_empty(),
            "Console Simulation View App failed while opening the real stream: {state:?}"
        );
        ensure!(
            tokio::time::Instant::now() < deadline,
            "Console Simulation View App did not display real H.264 video: {state:?}"
        );
        cdp.assert_no_software_renderer_events()?;
        assert_page_visible(cdp, session_id).await?;
        tokio::time::sleep(Duration::from_millis(500)).await;
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
    session_id: &str,
    context_id: u64,
    expected_camera_id: &str,
) -> Result<()> {
    let expected = serde_json::to_string(expected_camera_id)?;
    let requested: bool = cdp
        .evaluate_context(
            session_id,
            context_id,
            &format!(
                r#"(() => {{
                  const input=[...document.querySelectorAll('#cameras input[type="checkbox"]')]
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
        let state: Value = cdp
            .evaluate_context(
                session_id,
                context_id,
                r#"(() => ({
                  checked:document.querySelector('#cameras input[type="checkbox"]:checked') !== null,
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

async fn verify_browser_inner(
    cdp_base: &str,
    page_url: &str,
    expected_camera_id: &str,
) -> Result<()> {
    let mut cdp = connect_headed_browser(cdp_base, "Simulation View acceptance").await?;
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

    let acceptance = async {
        for method in [
            "Runtime.enable",
            "Page.enable",
            "Log.enable",
            "Network.enable",
        ] {
            cdp.command(method, serde_json::json!({}), Some(&session_id))
                .await?;
        }
        wait_for_document(&mut cdp, &session_id).await?;
        let hardware: HardwareIdentity =
            cdp.evaluate(&session_id, HARDWARE_PREFLIGHT, true).await?;
        hardware.validate()?;

        wait_for_app_ready(&mut cdp, &session_id).await?;
        let diagnostics_installed: bool = cdp
            .evaluate(&session_id, INSTALL_RTC_DIAGNOSTICS, false)
            .await?;
        ensure!(
            diagnostics_installed,
            "generic App peer-connection diagnostics could not be installed"
        );
        let selected: bool = cdp
            .evaluate(
                &session_id,
                r#"(() => {
                    const frame = document.getElementById("app-frame");
                    const input = frame?.contentDocument?.querySelector(
                      `#cameras input[type="checkbox"]`
                    );
                    if (!input) return false;
                    if (!input.checked) input.click();
                    return true;
                })()"#,
                false,
            )
            .await?;
        ensure!(
            selected,
            "generic Simulation View App exposed no camera selector"
        );

        let first = match wait_for_video(&mut cdp, &session_id, expected_camera_id).await {
            Ok(state) => state,
            Err(error) => return Err(error.context(cdp.stream_diagnostics()?)),
        };
        tokio::time::sleep(Duration::from_secs(2)).await;
        let second: AppVideoState = cdp.evaluate(&session_id, VIDEO_STATE, false).await?;
        second.validate(expected_camera_id)?;
        ensure!(
            second.current_time > first.current_time + 0.25,
            "real H.264 video did not advance: {} -> {}",
            first.current_time,
            second.current_time
        );

        let decode: DecodeIdentity = cdp
            .evaluate(
                &session_id,
                r#"(async () => {
                    const frame = document.getElementById("app-frame");
                    const doc = frame?.contentDocument;
                    const video = doc?.querySelector("video");
                    const result = await navigator.mediaCapabilities.decodingInfo({
                      type: "webrtc",
                      video: {
                        contentType: 'video/H264; codecs="avc1.42E01E"',
                        width: video.videoWidth,
                        height: video.videoHeight,
                        bitrate: 8000000,
                        framerate: 30
                      }
                    });
                    return {
                      supported: result.supported,
                      smooth: result.smooth,
                      powerEfficient: result.powerEfficient,
                      label: doc.getElementById("decode")?.textContent ?? ""
                    };
                })()"#,
                true,
            )
            .await?;
        decode.validate()?;

        let teardown_started: bool = cdp
            .evaluate(
                &session_id,
                r#"(() => {
                    const frame = document.getElementById("app-frame");
                    if (!frame?.contentWindow) return false;
                    frame.contentWindow.postMessage({
                      jsonrpc: "2.0",
                      id: 9999,
                      method: "ui/resource-teardown",
                      params: {}
                    }, "*");
                    return true;
                })()"#,
                false,
            )
            .await?;
        ensure!(
            teardown_started,
            "generic App teardown could not be delivered"
        );
        wait_for_teardown(&mut cdp, &session_id).await?;
        cdp.assert_no_software_renderer_events()?;
        Ok(())
    }
    .await;

    let close = cdp
        .command(
            "Target.closeTarget",
            serde_json::json!({"targetId": target_id}),
            None,
        )
        .await;
    acceptance?;
    close?;
    Ok(())
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

impl HardwareIdentity {
    fn validate(&self) -> Result<()> {
        ensure!(
            !self.user_agent.contains("HeadlessChrome"),
            "attached Chrome is headless"
        );
        let webgpu = format!(
            "{} {} {} {}",
            self.webgpu_vendor,
            self.webgpu_architecture,
            self.webgpu_device,
            self.webgpu_description
        )
        .to_ascii_lowercase();
        ensure!(
            !self.webgpu_vendor.is_empty()
                && webgpu.contains("nvidia")
                && !software_renderer(&webgpu),
            "headed Chrome requires its high-performance NVIDIA WebGPU adapter; received {webgpu:?}"
        );
        let webgl = format!("{} {}", self.webgl_vendor, self.webgl_renderer).to_ascii_lowercase();
        ensure!(
            self.webgl_available && webgl.contains("nvidia") && !software_renderer(&webgl),
            "headed Chrome requires a hardware NVIDIA WebGL context; received {webgl:?}"
        );
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppVideoState {
    camera_id: String,
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
            "generic App selected camera {:?}, expected {expected_camera_id:?}",
            self.camera_id
        );
        ensure!(
            self.ready_state >= 2
                && self.video_width == 640
                && self.video_height == 360
                && self.current_time.is_finite()
                && self.current_time > 0.0,
            "generic App did not display the real 640x360 H.264 stream: {self:?}"
        );
        ensure!(
            self.decode_label == "NVIDIA NVENC · hardware H.264 decode"
                || self.decode_label == "NVIDIA NVENC · software H.264 decode",
            "generic App made an invalid decode-path claim: {:?}",
            self.decode_label
        );
        ensure!(
            self.status.contains("live") && self.error.is_empty(),
            "generic App did not reach live state: {self:?}"
        );
        ensure!(
            !software_renderer(&self.body_text.to_ascii_lowercase()),
            "generic App exposed a software-renderer warning"
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

async fn wait_for_app_ready(cdp: &mut Cdp, session_id: &str) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let state: Value = cdp
            .evaluate(
                session_id,
                r#"(() => {
                    const doc = document.getElementById("app-frame")?.contentDocument;
                    return {
                      ready: Boolean(doc?.querySelector(`#cameras input[type="checkbox"]`)),
                      status: doc?.getElementById("status")?.textContent ?? "",
                      error: doc?.getElementById("error")?.hidden === false
                        ? doc.getElementById("error").textContent : ""
                    };
                })()"#,
                false,
            )
            .await?;
        if state.get("ready").and_then(Value::as_bool) == Some(true) {
            return Ok(());
        }
        let error = state.get("error").and_then(Value::as_str).unwrap_or("");
        ensure!(
            error.is_empty(),
            "generic App failed during discovery: {error}"
        );
        ensure!(
            tokio::time::Instant::now() < deadline,
            "generic App did not discover its Simulation View camera: {state}"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn wait_for_video(
    cdp: &mut Cdp,
    session_id: &str,
    expected_camera_id: &str,
) -> Result<AppVideoState> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(180);
    loop {
        let state: AppVideoState = cdp.evaluate(session_id, VIDEO_STATE, false).await?;
        if state.ready_state >= 2 && state.video_width > 0 && state.current_time > 0.0 {
            state.validate(expected_camera_id)?;
            return Ok(state);
        }
        ensure!(
            state.error.is_empty(),
            "generic App failed while opening the real stream: {}; peer states: {:?}",
            state.error,
            state.rtc_states
        );
        ensure!(
            tokio::time::Instant::now() < deadline,
            "generic App did not display real H.264 video: {state:?}"
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn wait_for_teardown(cdp: &mut Cdp, session_id: &str) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        let state: Value = cdp
            .evaluate(
                session_id,
                r#"({
                    closeCalls: window.__veoveoBridgeState?.closeCalls ?? 0,
                    teardownAck: window.__veoveoBridgeState?.teardownAck ?? false
                })"#,
                false,
            )
            .await?;
        if state.get("closeCalls").and_then(Value::as_u64).unwrap_or(0) >= 1
            && state.get("teardownAck").and_then(Value::as_bool) == Some(true)
        {
            return Ok(());
        }
        ensure!(
            tokio::time::Instant::now() < deadline,
            "generic App did not close its lease during teardown: {state}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

struct Cdp {
    socket: WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    next_id: u64,
    events: Vec<Value>,
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
            events: Vec::new(),
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
                    self.events.push(value);
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
        for event in &self.events {
            let encoded = serde_json::to_string(event)?.to_ascii_lowercase();
            ensure!(
                !software_renderer(&encoded),
                "headed Chrome emitted a software-renderer event"
            );
        }
        Ok(())
    }

    fn stream_diagnostics(&self) -> Result<String> {
        let events = self
            .events
            .iter()
            .filter_map(stream_event_summary)
            .collect::<Vec<_>>();
        Ok(if events.is_empty() {
            "Chrome reported no WebSocket response or transport error".to_owned()
        } else {
            format!("Chrome WebSocket diagnostics: {}", events.join("; "))
        })
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
        _ => None,
    }
}

fn redacted_network_url(value: &str) -> String {
    let Ok(mut url) = Url::parse(value) else {
        return "unparseable WebSocket URL".to_owned();
    };
    url.set_query(None);
    url.set_fragment(None);
    url.to_string()
}

async fn serve_request(mut stream: TcpStream, parent: Arc<str>, app: Arc<str>) -> Result<()> {
    let mut request = Vec::with_capacity(4096);
    loop {
        let mut chunk = [0_u8; 1024];
        let count = stream.read(&mut chunk).await?;
        if count == 0 {
            break;
        }
        request.extend_from_slice(&chunk[..count]);
        ensure!(request.len() <= 16 * 1024, "App host request was too large");
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    let request = std::str::from_utf8(&request)?;
    let first_line = request
        .lines()
        .next()
        .context("App host request was empty")?;
    let mut parts = first_line.split_whitespace();
    ensure!(parts.next() == Some("GET"), "App host only accepts GET");
    let path = parts.next().context("App host request omitted a path")?;
    let (status, content_type, body) = match path.split('?').next().unwrap_or(path) {
        "/" => ("200 OK", "text/html; charset=utf-8", parent),
        "/app" => ("200 OK", "text/html; charset=utf-8", app),
        _ => (
            "404 Not Found",
            "text/plain; charset=utf-8",
            Arc::<str>::from("not found"),
        ),
    };
    let headers = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(headers.as_bytes()).await?;
    stream.write_all(body.as_bytes()).await?;
    stream.shutdown().await?;
    Ok(())
}

fn parent_html(fixture: &BrowserFixture) -> Result<String> {
    let resources = script_json(&serde_json::to_value(&fixture.resources)?)?;
    let connection = script_json(&fixture.connection)?;
    Ok(format!(
        r#"<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><title>Simulation View hardware acceptance host</title></head>
<body style="margin:0;background:#020608">
<iframe id="app-frame" title="Simulation View" src="/app"
  sandbox="allow-scripts allow-same-origin"
  style="border:0;width:1280px;height:720px"></iframe>
<script>
"use strict";
const resources={resources};
const connection={connection};
window.__veoveoBridgeState={{closeCalls:0,teardownAck:false}};
const frame=()=>document.getElementById("app-frame").contentWindow;
window.addEventListener("message",(event)=>{{
  const message=event.data;
  if(!message||message.jsonrpc!=="2.0")return;
  if(message.id===9999&&message.result!==undefined){{
    window.__veoveoBridgeState.teardownAck=true;
    return;
  }}
  if(message.id===undefined)return;
  let result;
  if(message.method==="ui/initialize"){{
    result={{
      protocolVersion:"2026-01-26",
      hostInfo:{{name:"veoveo-smoke-host",version:"1.0.0"}},
      hostCapabilities:{{}},
      hostContext:{{theme:"dark",displayMode:"inline"}}
    }};
  }}else if(message.method==="resources/read"){{
    const value=resources[message.params?.uri];
    if(value===undefined){{
      frame().postMessage({{jsonrpc:"2.0",id:message.id,error:{{code:-32002,message:"resource not found"}}}},"*");
      return;
    }}
    result={{contents:[{{uri:message.params.uri,mimeType:"application/json",text:JSON.stringify(value)}}]}};
  }}else if(message.method==="tools/call"){{
    const name=message.params?.name;
    if(name==="open_live_view"||name==="renew_live_view"){{
      result={{content:[],structuredContent:connection,isError:false}};
    }}else if(name==="close_live_view"){{
      window.__veoveoBridgeState.closeCalls++;
      result={{content:[],structuredContent:{{resourceUri:connection.stream.resourceUri,closed:true}},isError:false}};
    }}else{{
      frame().postMessage({{jsonrpc:"2.0",id:message.id,error:{{code:-32601,message:"tool not admitted by acceptance host"}}}},"*");
      return;
    }}
  }}else{{
    result={{}};
  }}
  frame().postMessage({{jsonrpc:"2.0",id:message.id,result}},"*");
}});
</script>
</body>
</html>"#
    ))
}

fn script_json(value: &Value) -> Result<String> {
    Ok(serde_json::to_string(value)?
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026"))
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
  const adapter = await navigator.gpu?.requestAdapter({powerPreference:"high-performance"});
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

const INSTALL_RTC_DIAGNOSTICS: &str = r#"(() => {
  const frame = document.getElementById("app-frame");
  const app = frame?.contentWindow;
  const Native = app?.RTCPeerConnection;
  if (!app || !Native) return false;
  app.__veoveoRtcStates = [];
  let sequence = 0;
  const snapshot = (peer, event) => {
    const value = [
      `${++sequence}:${event}`,
      `signaling=${peer.signalingState}`,
      `gathering=${peer.iceGatheringState}`,
      `ice=${peer.iceConnectionState}`,
      `connection=${peer.connectionState}`,
      `local=${peer.localDescription?.type ?? "none"}`,
      `remote=${peer.remoteDescription?.type ?? "none"}`,
      `receivers=${peer.getReceivers().length}`
    ].join(",");
    app.__veoveoRtcStates.push(value);
    if (app.__veoveoRtcStates.length > 128) app.__veoveoRtcStates.shift();
  };
  app.RTCPeerConnection = class extends Native {
    constructor(...args) {
      super(...args);
      snapshot(this, "created");
      for (const event of [
        "signalingstatechange",
        "icegatheringstatechange",
        "iceconnectionstatechange",
        "connectionstatechange",
        "track"
      ]) {
        this.addEventListener(event, () => snapshot(this, event));
      }
      this.addEventListener("icecandidate", ({candidate}) => {
        snapshot(this, candidate ? "local-candidate" : "gathering-complete");
      });
    }
  };
  return true;
})()"#;

const VIDEO_STATE: &str = r#"(() => {
  const doc=document.getElementById("app-frame")?.contentDocument;
  const app=document.getElementById("app-frame")?.contentWindow;
  const video=doc?.querySelector("video");
  const camera=doc?.querySelector(`#cameras input[type="checkbox"]:checked`);
  return {
    cameraId:camera?.parentElement?.textContent?.split(" · ")[0]?.trim() ?? "",
    readyState:video?.readyState ?? 0,
    videoWidth:video?.videoWidth ?? 0,
    videoHeight:video?.videoHeight ?? 0,
    currentTime:video?.currentTime ?? 0,
    decodeLabel:doc?.getElementById("decode")?.textContent ?? "",
    status:doc?.getElementById("status")?.textContent ?? "",
    error:doc?.getElementById("error")?.hidden === false
      ? doc.getElementById("error").textContent : "",
    bodyText:doc?.body?.innerText ?? "",
    rtcStates:app?.__veoveoRtcStates ?? []
  };
})()"#;

const APP_FRAME_VIDEO_STATE: &str = r#"(() => {
  const video=document.querySelector("video");
  const camera=document.querySelector(`#cameras input[type="checkbox"]:checked`);
  return {
    cameraId:camera?.parentElement?.textContent?.split(" · ")[0]?.trim() ?? "",
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

#[cfg(test)]
mod tests {
    use super::*;

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
                "url": "ws://localhost:8782/simulation-view/signaling/sign_in?peer_id=peer",
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
            "created ws://localhost:8782/simulation-view/signaling/sign_in; \
             handshake HTTP 403 Forbidden"
        );
        assert!(!summaries.contains("secret"));
        assert!(!summaries.contains("peer"));
    }

    #[test]
    fn parent_bridge_never_places_tokens_in_resource_payloads() {
        let fixture = BrowserFixture {
            app_html: "<!doctype html>".to_owned(),
            resources: BTreeMap::from([(
                "simulation-view://capacity".to_owned(),
                serde_json::json!({"limits": {}}),
            )]),
            connection: serde_json::json!({
                "stream": {
                    "resourceUri": "simulation-view://session/s/stream/v"
                },
                "accessToken": "secret-token-only-in-tool-result-000000000"
            }),
            expected_camera_id: "camera-1".to_owned(),
        };
        let html = parent_html(&fixture).unwrap();
        let resources_start = html.find("const resources=").unwrap();
        let connection_start = html.find("const connection=").unwrap();
        let resources_script = &html[resources_start..connection_start];
        assert!(!resources_script.contains("secret-token"));
        assert!(html[connection_start..].contains("secret-token-only-in-tool-result"));
    }

    #[test]
    fn software_renderer_fingerprints_fail_closed() {
        assert!(software_renderer("google swiftshader"));
        assert!(software_renderer("mesa llvmpipe"));
        assert!(software_renderer("software rasterizer warning"));
        assert!(!software_renderer("nvidia geforce rtx 4090"));
    }

    #[test]
    fn finds_the_sandboxed_simulation_view_frame_in_a_cdp_tree() {
        let tree = serde_json::json!({
            "frame": {"id": "root", "url": "https://installation.example/console/"},
            "childFrames": [{
                "frame": {
                    "id": "app",
                    "url": "https://installation.example/console/api/apps/frame?uri=ui%3A%2F%2Fsimulation-view%2Flive.html"
                }
            }]
        });
        assert_eq!(find_app_frame_id(&tree), Some("app"));
    }
}
