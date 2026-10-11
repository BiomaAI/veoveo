//! Fixed-wall diagnostic sampling. Ops selects production cameras; this module selects canvases.
use super::*;
use crate::cli::CameraMeasureArgs;
use veoveo_uav_sim_mcp::contract::{LiveCameraStreamPolicy, LiveStreamProductState};

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
enum DiagnosticPhase {
    Bootstrap,
    BrowserConnect,
    Navigation,
    Hardware,
    CameraAdmission,
    StateBefore,
    DecoderBefore,
    Sampling,
    StateAfter,
    ObservationFinished,
    Cleanup,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SafeFailure {
    phase: DiagnosticPhase,
    category: &'static str,
    message: &'static str,
    error_chain_sha256: String,
    redacted_error_chain: Vec<String>,
}
impl SafeFailure {
    fn capture(phase: DiagnosticPhase, error: &anyhow::Error, secrets: &[String]) -> Self {
        let original = format!("{error:#}");
        // Only closed, owner-authored diagnostics are published. CDP and upstream
        // errors can contain URLs or provider text; preserve their digest instead.
        let (category, message) = if original.contains("deadline") || original.contains("timed out")
        {
            ("deadline", "operation or cleanup deadline elapsed")
        } else if original.contains("follow camera absent") {
            ("cameraAdmission", "follow camera absent")
        } else if original.contains("actual rendered atlas does not match") {
            (
                "topology",
                "actual rendered atlas does not match selected measurement profile",
            )
        } else if original.contains("browser is not presenting selected atlas/canvas profile") {
            (
                "decoderAdmission",
                "browser is not presenting selected atlas/canvas profile",
            )
        } else if original.contains("no frames advanced") {
            ("noFrames", "no frames advanced during measurement")
        } else {
            (
                "requestOrAdmission",
                "request or admission failed; upstream text withheld",
            )
        };
        Self {
            phase,
            category,
            message,
            error_chain_sha256: hex::encode(Sha256::digest(original.as_bytes())),
            redacted_error_chain: error
                .chain()
                .take(12)
                .map(|cause| safe_detail(&cause.to_string(), secrets))
                .collect(),
        }
    }
}
fn safe_detail(value: &str, secrets: &[String]) -> String {
    let mut value = value.to_owned();
    for secret in secrets.iter().filter(|secret| !secret.is_empty()) {
        value = value.replace(secret, "[REDACTED]");
    }
    // Remove query/fragment values before the maintained browser redactor bounds text.
    let value = value
        .split_whitespace()
        .map(|word| {
            if word.contains('?') || word.contains('#') {
                word.split(['?', '#']).next().unwrap_or("").to_owned()
            } else {
                word.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    if [
        "cookie",
        "set-cookie",
        "client_secret",
        "refresh_token",
        "id_token",
    ]
    .iter()
    .any(|key| value.to_ascii_lowercase().contains(key))
    {
        return "[redacted authentication-bearing browser diagnostic]".to_owned();
    }
    bounded_browser_diagnostic(&value)
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticState {
    schema: &'static str,
    phase: DiagnosticPhase,
    observation_failure: Option<SafeFailure>,
    cleanup_failure: Option<SafeFailure>,
    browser_diagnostics: Option<String>,
}
#[derive(Clone)]
struct DiagnosticJournal {
    state: Arc<std::sync::Mutex<DiagnosticState>>,
    path: std::path::PathBuf,
    secrets: Arc<std::sync::Mutex<Vec<String>>>,
}
impl DiagnosticJournal {
    fn create(directory: &std::path::Path) -> Result<Self> {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        fs::create_dir_all(directory)?;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
        let path = directory.join("diagnostic.json");
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
        let journal = Self {
            state: Arc::new(std::sync::Mutex::new(DiagnosticState {
                schema: "veoveo.ai/uav-camera-capacity-diagnostic/v1",
                phase: DiagnosticPhase::Bootstrap,
                observation_failure: None,
                cleanup_failure: None,
                browser_diagnostics: None,
            })),
            path,
            secrets: Arc::new(std::sync::Mutex::new(vec![])),
        };
        journal.update(|_| {})?;
        Ok(journal)
    }
    fn update(&self, change: impl FnOnce(&mut DiagnosticState)) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("measurement diagnostic lock poisoned"))?;
        change(&mut state);
        fs::write(&self.path, serde_json::to_vec_pretty(&*state)?)?;
        Ok(())
    }
    fn phase(&self, phase: DiagnosticPhase) -> Result<()> {
        self.update(|state| state.phase = phase)
    }
    fn observation_failure(&self, error: &anyhow::Error) -> Result<()> {
        self.update(|state| {
            if state.observation_failure.is_none() {
                state.observation_failure = Some(SafeFailure::capture(
                    state.phase,
                    error,
                    &self.secrets.lock().expect("diagnostic secrets"),
                ));
            }
        })
    }
    fn cleanup_failure(&self, error: &anyhow::Error) -> Result<()> {
        self.update(|state| {
            if state.cleanup_failure.is_none() {
                state.cleanup_failure = Some(SafeFailure::capture(
                    DiagnosticPhase::Cleanup,
                    error,
                    &self.secrets.lock().expect("diagnostic secrets"),
                ));
            }
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DecoderSample {
    document_epoch: f64,
    sampled_at_ms: f64,
    viewer: String,
    product: LiveStreamProductId,
    decoded_frames: u64,
    media_seconds: f64,
    queue_size: u64,
    codec: String,
    decode_label: String,
    supported: bool,
    smooth: bool,
    power_efficient: bool,
    canvas_count: usize,
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReadBracket {
    start_seconds: f64,
    end_seconds: f64,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct IntervalBounds {
    minimum_seconds: f64,
    maximum_seconds: f64,
}
impl IntervalBounds {
    fn between(before: ReadBracket, after: ReadBracket) -> Result<Self> {
        ensure!(
            before.start_seconds <= before.end_seconds
                && after.start_seconds <= after.end_seconds
                && after.start_seconds > before.end_seconds,
            "state request brackets overlap or regress"
        );
        Ok(Self {
            minimum_seconds: after.start_seconds - before.end_seconds,
            maximum_seconds: after.end_seconds - before.start_seconds,
        })
    }
    fn rate(&self, delta: f64) -> RateBounds {
        RateBounds {
            minimum_per_second: delta / self.maximum_seconds,
            maximum_per_second: delta / self.minimum_seconds,
        }
    }
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RateBounds {
    minimum_per_second: f64,
    maximum_per_second: f64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Window {
    canvases: usize,
    before_request: ReadBracket,
    after_request: ReadBracket,
    state_observation_interval: IntervalBounds,
    browser_sample_seconds: f64,
    encoded_frames_delta: u64,
    admitted_access_unit_observation_rate: RateBounds,
    decoded_and_drawn_callbacks_delta: u64,
    decoded_and_drawn_callbacks_per_second: f64,
    simulation_seconds_delta: f64,
    real_time_factor_bounds: RateBounds,
    before: SimulationState,
    after: SimulationState,
    frame_age_before_seconds: Option<f64>,
    frame_age_after_seconds: Option<f64>,
    decoder_samples: Vec<DecoderSample>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Receipt {
    schema: &'static str,
    captured_at: chrono::DateTime<Utc>,
    expected_camera_count: u8,
    requested_sample_seconds: u64,
    observation_succeeded: bool,
    cleanup_succeeded: bool,
    hardware: Option<HardwareIdentity>,
    windows: Vec<Window>,
    limitations: &'static str,
}
fn profile(count: u8) -> Result<(&'static [&'static str], u32, u32)> {
    match count {
        1 => Ok((&["follow"], 1280, 720)),
        2 => Ok((&["follow", "chase"], 2560, 720)),
        5 => Ok((&QUALIFIED_CAMERA_IDS, 3840, 1440)),
        _ => bail!("camera measurement supports only Ops-selected 1, 2 or 5 camera profiles"),
    }
}
fn topology(state: &SimulationState, count: u8) -> Result<&LiveStreamProductState> {
    let (ids, width, height) = profile(count)?;
    let expected = ids
        .iter()
        .map(LiveCameraId::parse)
        .collect::<Result<BTreeSet<_>, _>>()?;
    ensure!(
        state.lifecycle == SimulationLifecycle::Running,
        "simulation is not running"
    );
    ensure!(
        state.stream_products.len() == 1,
        "measurement requires one atlas product"
    );
    let product = &state.stream_products[0];
    let actual = product
        .camera_regions
        .iter()
        .map(|r| r.camera_id.clone())
        .collect::<BTreeSet<_>>();
    ensure!(
        actual == expected
            && product.coded_width_px == width
            && product.coded_height_px == height
            && product.nvenc_sessions == 1
            && product.lifecycle == LiveStreamProductLifecycle::Ready,
        "actual rendered atlas does not match selected measurement profile"
    );
    for (index, id) in ids.iter().enumerate() {
        let camera_id = LiveCameraId::parse(id)?;
        let region = product.region(&camera_id).context("atlas region missing")?;
        let columns = width / 1280;
        ensure!(
            region.width_px == 1280
                && region.height_px == 720
                && region.x_px == (index as u32 % columns) * 1280
                && region.y_px == (index as u32 / columns) * 720,
            "atlas camera region differs from selected profile"
        );
    }
    let active = state
        .live_cameras
        .iter()
        .filter(|c| c.stream_policy == LiveCameraStreamPolicy::Continuous)
        .map(|c| c.camera_id.clone())
        .collect::<BTreeSet<_>>();
    ensure!(
        active == expected,
        "continuous camera set differs from selected profile"
    );
    for camera in state
        .live_cameras
        .iter()
        .filter(|c| expected.contains(&c.camera_id))
    {
        ensure!(
            camera.width_px == 1280
                && camera.height_px == 720
                && camera.frame_rate_millihertz == 16000
                && camera.health == LiveCameraHealth::Healthy,
            "camera dimensions/cadence/health differ"
        );
    }
    Ok(product)
}
const SNAPSHOT: &str = r#"(async()=>{
 const canvases=[...document.querySelectorAll('.view canvas')], canvas=document.querySelector('#view-follow canvas');
 if(!canvas||!canvases.length)throw new Error('follow canvas unavailable');
 const views=canvases.map(c=>c.closest('.view'));
 if(new Set(views.map(v=>v.dataset.streamProductId)).size!==1)throw new Error('multiple products');
 const codec=canvas.dataset.codec;
 if(!codec||codec==='pending')throw new Error('codec unavailable');
 const cap=await navigator.mediaCapabilities.decodingInfo({type:'file',video:{contentType:`video/mp4; codecs="${codec}"`,width:Number(canvas.dataset.sourceWidth),height:Number(canvas.dataset.sourceHeight),bitrate:12000000,framerate:Number(canvas.dataset.frameRate)}});
 return {documentEpoch:performance.timeOrigin,sampledAtMs:performance.now(),viewer:views[0].dataset.viewerInstanceId,product:views[0].dataset.streamProductId,
 decodedFrames:Number(canvas.dataset.decodedFrames),mediaSeconds:Number(canvas.dataset.mediaTimeSeconds),queueSize:Number(canvas.dataset.decodeQueueSize??0),codec,
 decodeLabel:document.getElementById('decode')?.textContent??'',supported:cap.supported,smooth:cap.smooth,powerEfficient:cap.powerEfficient,canvasCount:canvases.length};
})()"#;
async fn snapshot(cdp: &mut Cdp, target: &str, session: &str) -> Result<DecoderSample> {
    evaluate_console_app(cdp, target, session, "uav-sim", SNAPSHOT, true).await
}
fn decoder_identity(before: &DecoderSample, after: &DecoderSample) -> Result<()> {
    ensure!(
        before.document_epoch == after.document_epoch
            && before.viewer == after.viewer
            && before.product == after.product
            && before.codec == after.codec
            && after.sampled_at_ms > before.sampled_at_ms
            && after.decoded_frames >= before.decoded_frames
            && after.media_seconds >= before.media_seconds,
        "decoder identity or monotonic counters changed during window"
    );
    ensure!(
        after.supported && after.smooth,
        "exact H264 MediaCapabilities profile is unsupported or not smooth"
    );
    Ok(())
}

fn browser_rate(before: &DecoderSample, after: &DecoderSample) -> Result<(u64, f64, f64)> {
    decoder_identity(before, after)?;
    let seconds = (after.sampled_at_ms - before.sampled_at_ms) / 1000.0;
    ensure!(
        seconds.is_finite() && seconds > 0.0,
        "invalid browser counter interval"
    );
    let delta = after.decoded_frames - before.decoded_frames;
    Ok((delta, seconds, delta as f64 / seconds))
}
async fn bracketed_state(
    operator: &OperatorClient<'_>,
    session: &str,
    origin: tokio::time::Instant,
) -> Result<(SimulationState, ReadBracket)> {
    let start_seconds = origin.elapsed().as_secs_f64();
    let state = simulation_state(operator, session).await?;
    Ok((
        state,
        ReadBracket {
            start_seconds,
            end_seconds: origin.elapsed().as_secs_f64(),
        },
    ))
}

struct MeasurementBrowser {
    cdp: Cdp,
    target: String,
    session: String,
    navigation_intent: bool,
    released: bool,
    target_closed: bool,
    cleanup_end: Option<std::time::Instant>,
    failed: bool,
}
impl MeasurementBrowser {
    async fn close(&mut self, ids: &[&str]) -> Result<()> {
        let owner_end = veoveo_testing_support::lifecycle::owner::cleanup_deadline()?;
        let end = *self.cleanup_end.get_or_insert_with(|| {
            (std::time::Instant::now() + Duration::from_secs(30)).min(owner_end)
        });
        let end = end.min(owner_end);
        let release_end = end.checked_sub(Duration::from_secs(10)).unwrap_or(end);
        let release = if !self.navigation_intent || self.released {
            Ok(())
        } else {
            tokio::time::timeout_at(
                release_end.into(),
                close_console_live_views(&mut self.cdp, &self.target, &self.session, ids),
            )
            .await
            .context("measurement authorization cleanup deadline")
            .and_then(|r| r)
        };
        if release.is_ok() {
            self.released = true;
        } else {
            self.failed = true;
        }
        let close = if self.target_closed {
            Ok(())
        } else {
            tokio::time::timeout_at(end.into(), close_target(&mut self.cdp, &self.target))
                .await
                .context("measurement target cleanup deadline")
                .and_then(|r| r)
        };
        if close.is_ok() {
            self.target_closed = true;
        } else {
            self.failed = true;
        }
        release?;
        close?;
        ensure!(
            !self.failed,
            "measurement cleanup previously failed; ownership unresolved"
        );
        Ok(())
    }
}
type RetainedBrowser = Arc<tokio::sync::Mutex<MeasurementBrowser>>;
fn retain_browser(
    cdp: Cdp,
    target: String,
    session: String,
    camera_count: u8,
    diagnostic: DiagnosticJournal,
) -> Result<(
    RetainedBrowser,
    veoveo_testing_support::lifecycle::owner::CleanupRegistration,
)> {
    let retained = Arc::new(tokio::sync::Mutex::new(MeasurementBrowser {
        cdp,
        target: target.clone(),
        session,
        navigation_intent: false,
        released: false,
        target_closed: false,
        cleanup_end: None,
        failed: false,
    }));
    let cleanup_owner = retained.clone();
    let registration = register_measurement_cleanup(&target, move || async move {
        let result = cleanup_owner
            .lock()
            .await
            .close(profile(camera_count)?.0)
            .await;
        if let Err(error) = &result {
            diagnostic.cleanup_failure(error)?;
        }
        result
    })?;
    Ok((retained, registration))
}
fn register_measurement_cleanup<F, Fut>(
    identity: &str,
    cleanup: F,
) -> Result<veoveo_testing_support::lifecycle::owner::CleanupRegistration>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<()>> + Send + 'static,
{
    use veoveo_testing_support::lifecycle::owner;
    owner::register_cleanup(
        owner::CleanupKind::Remote,
        "camera measurement live-view authorization and browser",
        identity,
        cleanup,
    )
}

pub async fn run(args: CameraMeasureArgs) -> Result<()> {
    let directory = args.evidence_root.join(uuid::Uuid::now_v7().to_string());
    let diagnostic = DiagnosticJournal::create(&directory)?;
    println!(
        "Camera capacity diagnostic journal: {}",
        diagnostic.path.display()
    );
    let result = run_observation(args, &directory, diagnostic.clone()).await;
    if let Err(error) = &result {
        diagnostic.observation_failure(error)?;
    }
    result
}
async fn run_observation(
    args: CameraMeasureArgs,
    directory: &std::path::Path,
    diagnostic: DiagnosticJournal,
) -> Result<()> {
    profile(args.camera_count)?;
    let installation = InstalledTarget::load(&args.installation)?;
    let conformance = veoveo_testing_support::artifacts::requested_executable(
        args.conformance_bin.clone(),
        "veoveo-mcp-conformance",
        "conformance",
    )?;
    let windows = if args.all_canvases { 2 } else { 1 };
    // One operation clock includes bootstrap, warm-up and both optional windows.
    let end = tokio::time::Instant::now()
        + Duration::from_secs(120 + windows * (args.warmup_seconds + args.sample_seconds));
    let token = tokio::time::timeout_at(end, installation.token())
        .await
        .context("measurement OAuth deadline")??;
    diagnostic
        .secrets
        .lock()
        .expect("diagnostic secrets")
        .push(token.clone());
    let operator = OperatorClient {
        conformance: &conformance,
        installation: &installation,
        token: &token,
    };
    let page = console_acceptance_url(installation.public_base(), "/apps/uav-sim/live");
    diagnostic.phase(DiagnosticPhase::BrowserConnect)?;
    let (cdp, target, session) =
        tokio::time::timeout_at(end, open_headed_target(&args.chrome_cdp_url, "about:blank"))
            .await
            .context("measurement browser deadline")??;
    let (retained, registration) =
        retain_browser(cdp, target, session, args.camera_count, diagnostic.clone())?;
    let mut receipt = Receipt {
        schema: "veoveo.ai/uav-camera-capacity/v1",
        captured_at: Utc::now(),
        expected_camera_count: args.camera_count,
        requested_sample_seconds: args.sample_seconds,
        observation_succeeded: false,
        cleanup_succeeded: false,
        hardware: None,
        windows: vec![],
        limitations: "encodedFrames counts admitted RTSP access units, not hardware NVENC submissions. Canvas counters share one decoder and are counted once. droppedFrames is unavailable; initialized App zero is not loss evidence. Received AU/SEI frame gaps, runtime restart and clock generation are not exposed by this App/state contract. Timing fields are Kit CPU/update wall timing, not GPU execution timing. State rates are bounds from collector request brackets; browser rates use counter-paired performance.now timestamps. Decoded/drawn callbacks do not measure physical compositor presentation. Decode powerEfficient=false identifies software H264 decode and is permitted.",
    };
    let observed: Result<()> = tokio::time::timeout_at(end, async {
        let mut browser = retained.lock().await;
        diagnostic.phase(DiagnosticPhase::Navigation)?;
        browser.navigation_intent = true;
        let MeasurementBrowser {
            cdp,
            target,
            session,
            ..
        } = &mut *browser;
        veoveo_testing_support::lifecycle::owner::check_effect()?;
        cdp.command(
            "Page.navigate",
            serde_json::json!({"url": page}),
            Some(session),
        )
        .await?;
        wait_for_document(cdp, session).await?;
        assert_page_visible(cdp, session).await?;
        diagnostic.phase(DiagnosticPhase::Hardware)?;
        let hardware: HardwareIdentity = cdp.evaluate(session, HARDWARE_PREFLIGHT, true).await?;
        hardware.validate()?;
        receipt.hardware = Some(hardware);
        diagnostic.phase(DiagnosticPhase::CameraAdmission)?;
        ensure!(
            wait_for_console_app_camera(cdp, target, session, "follow").await?,
            "follow camera absent"
        );
        isolate_console_app_camera(cdp, target, session, "follow").await?;
        for window in 0..windows {
            let ids = profile(args.camera_count)?.0;
            let canvas_count = if window == 0 { 1 } else { ids.len() };
            if window > 0 {
                for id in ids {
                    ensure!(
                        wait_for_console_app_camera(cdp, target, session, id).await?,
                        "active camera absent"
                    );
                }
            }
            tokio::time::sleep(Duration::from_secs(args.warmup_seconds)).await;
            let origin = tokio::time::Instant::now();
            diagnostic.phase(DiagnosticPhase::StateBefore)?;
            let (before, before_request) =
                bracketed_state(&operator, &args.session, origin).await?;
            let product = topology(&before, args.camera_count)?;
            diagnostic.phase(DiagnosticPhase::DecoderBefore)?;
            let first = snapshot(cdp, target, session).await?;
            ensure!(
                first.product == product.stream_product_id
                    && first.canvas_count == canvas_count
                    && first.decoded_frames > 0,
                "browser is not presenting selected atlas/canvas profile"
            );
            diagnostic.phase(DiagnosticPhase::Sampling)?;
            let browser_start = tokio::time::Instant::now();
            let sample_end = browser_start + Duration::from_secs(args.sample_seconds);
            let mut samples = vec![first];
            loop {
                tokio::time::sleep_until(
                    (tokio::time::Instant::now() + Duration::from_secs(1)).min(sample_end),
                )
                .await;
                assert_page_visible(cdp, session).await?;
                let hardware: HardwareIdentity =
                    cdp.evaluate(session, HARDWARE_PREFLIGHT, true).await?;
                hardware.validate()?;
                cdp.assert_no_software_renderer_events()?;
                let next = snapshot(cdp, target, session).await?;
                decoder_identity(samples.last().unwrap(), &next)?;
                ensure!(
                    next.canvas_count == canvas_count,
                    "display work changed during sample"
                );
                samples.push(next);
                if tokio::time::Instant::now() >= sample_end {
                    break;
                }
            }
            diagnostic.phase(DiagnosticPhase::StateAfter)?;
            let (after, after_request) = bracketed_state(&operator, &args.session, origin).await?;
            let interval = IntervalBounds::between(before_request, after_request)?;
            let last_product = topology(&after, args.camera_count)?;
            ensure!(
                last_product.stream_product_id == product.stream_product_id
                    && after.world == before.world
                    && after.simulation_time_s >= before.simulation_time_s
                    && after.physics_step >= before.physics_step,
                "source identity or clock changed during measurement"
            );
            let encoded = last_product
                .encoded_frames
                .checked_sub(product.encoded_frames)
                .context("atlas counter reset")?;
            let (frames, browser_seconds, browser_fps) =
                browser_rate(&samples[0], samples.last().unwrap())?;
            let sim = after.simulation_time_s - before.simulation_time_s;
            let age = |state: &SimulationState, product: &LiveStreamProductState| {
                product
                    .last_frame_at
                    .map(|at| (state.updated_at - at).num_milliseconds() as f64 / 1000.0)
            };
            let age_before = age(&before, product);
            let age_after = age(&after, last_product);
            receipt.windows.push(Window {
                canvases: canvas_count,
                before_request,
                after_request,
                admitted_access_unit_observation_rate: interval.rate(encoded as f64),
                real_time_factor_bounds: interval.rate(sim),
                state_observation_interval: interval,
                browser_sample_seconds: browser_seconds,
                encoded_frames_delta: encoded,
                decoded_and_drawn_callbacks_delta: frames,
                decoded_and_drawn_callbacks_per_second: browser_fps,
                simulation_seconds_delta: sim,
                before,
                after,
                frame_age_before_seconds: age_before,
                frame_age_after_seconds: age_after,
                decoder_samples: samples,
            });
            ensure!(
                encoded > 0 && frames > 0,
                "no frames advanced during measurement"
            );
        }
        Ok(())
    })
    .await
    .context("camera measurement original operation deadline")
    .and_then(|r| r);
    receipt.observation_succeeded = observed.is_ok();
    if let Err(error) = &observed {
        diagnostic.observation_failure(error)?;
        let diagnostics_end = (tokio::time::Instant::now() + Duration::from_secs(3)).min(end);
        let mut browser = retained.lock().await;
        let session = browser.session.clone();
        if let Ok(Ok(detail)) =
            tokio::time::timeout_at(diagnostics_end, browser.cdp.stream_diagnostics(&session)).await
        {
            let detail = safe_detail(
                &detail,
                &diagnostic.secrets.lock().expect("diagnostic secrets"),
            );
            diagnostic.update(|state| state.browser_diagnostics = Some(detail))?;
        }
    } else {
        diagnostic.phase(DiagnosticPhase::ObservationFinished)?;
    }
    // Persist partial observations before cleanup or outer owner aggregation can fail.
    let path = directory.join("measurement.json");
    fs::write(&path, serde_json::to_vec_pretty(&receipt)?)?;
    let cleanup = retained
        .lock()
        .await
        .close(profile(args.camera_count)?.0)
        .await;
    receipt.cleanup_succeeded = cleanup.is_ok();
    if let Err(error) = &cleanup {
        diagnostic.cleanup_failure(error)?;
    }
    if cleanup.is_ok() {
        registration.settled()?;
    }
    fs::write(&path, serde_json::to_vec_pretty(&receipt)?)?;
    println!("Camera capacity diagnostic: {}", path.display());
    observed?;
    cleanup
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> DecoderSample {
        DecoderSample {
            document_epoch: 1.0,
            sampled_at_ms: 1000.0,
            viewer: "viewer".into(),
            product: LiveStreamProductId::parse("camera-atlas").unwrap(),
            decoded_frames: 1,
            media_seconds: 1.0,
            queue_size: 0,
            codec: "avc1.640034".into(),
            decode_label: "software H.264 decode".into(),
            supported: true,
            smooth: true,
            power_efficient: false,
            canvas_count: 1,
        }
    }
    #[test]
    fn diagnostic_allows_slow_software_decode_but_refuses_counter_and_identity_reset() -> Result<()>
    {
        let before = sample();
        let mut after = sample();
        // A single frame over a long wall window is a valid low-FPS result.
        after.sampled_at_ms = 2000.0;
        after.decoded_frames = 2;
        after.media_seconds = 2.0;
        decoder_identity(&before, &after)?;
        after.document_epoch = 2.0;
        ensure!(decoder_identity(&before, &after).is_err());
        after.document_epoch = 1.0;
        after.decoded_frames = 0;
        ensure!(decoder_identity(&before, &after).is_err());
        after.decoded_frames = 2;
        after.product = LiveStreamProductId::parse("replacement-atlas")?;
        ensure!(decoder_identity(&before, &after).is_err());
        ensure!(profile(3).is_err() && profile(4).is_err());
        ensure!(profile(1)?.1 == 1280 && profile(2)?.1 == 2560 && profile(5)?.2 == 1440);
        Ok(())
    }
    #[test]
    fn topology_refuses_ui_only_camera_reduction_and_extra_encoder() -> Result<()> {
        let mut state: SimulationState = serde_json::from_str(include_str!(
            "../../../../flight/tests/fixtures/world-ready.json"
        ))?;
        // This maintained fixture has a follow-only view with a full-size atlas.
        ensure!(topology(&state, 1).is_err());
        state.stream_products[0].coded_width_px = 1280;
        state.stream_products[0].coded_height_px = 720;
        topology(&state, 1)?;
        state.stream_products[0].nvenc_sessions = 2;
        ensure!(topology(&state, 1).is_err());
        state.stream_products[0].nvenc_sessions = 1;
        let mut hidden = state.live_cameras[0].clone();
        hidden.camera_id = LiveCameraId::parse("chase")?;
        state.live_cameras.push(hidden);
        ensure!(topology(&state, 1).is_err());
        Ok(())
    }
    #[test]
    fn asymmetric_request_latency_uses_bracket_bounds_and_counter_paired_browser_clock()
    -> Result<()> {
        let bounds = IntervalBounds::between(
            ReadBracket {
                start_seconds: 0.0,
                end_seconds: 40.0,
            },
            ReadBracket {
                start_seconds: 160.0,
                end_seconds: 240.0,
            },
        )?;
        let rates = bounds.rate(1200.0);
        ensure!(bounds.minimum_seconds == 120.0 && bounds.maximum_seconds == 240.0);
        ensure!(rates.minimum_per_second == 5.0 && rates.maximum_per_second == 10.0);
        let before = sample();
        let mut after = sample();
        after.sampled_at_ms = before.sampled_at_ms + 120000.0;
        after.decoded_frames += 600;
        // Transport can return either CDP evaluation much later: the paired clocks still give 5 Hz.
        let (delta, seconds, fps) = browser_rate(&before, &after)?;
        ensure!(delta == 600 && seconds == 120.0 && fps == 5.0);
        ensure!(
            IntervalBounds::between(
                ReadBracket {
                    start_seconds: 0.0,
                    end_seconds: 80.0
                },
                ReadBracket {
                    start_seconds: 60.0,
                    end_seconds: 90.0
                }
            )
            .is_err()
        );
        Ok(())
    }

    #[tokio::test]
    async fn setup_and_cleanup_failures_survive_outer_owner_aggregation() -> Result<()> {
        let mut command = tokio::process::Command::new(std::env::current_exe()?);
        command.args(["--exact", "browser::camera_measure::tests::cancelled_owner_releases_live_authorization_then_closes_target_even_on_release_error", "--nocapture"])
            .env("VEOVEO_CAMERA_CLEANUP_CHILD", "observationFailure")
            .env("VEOVEO_SMOKE_DEADLINE_UNIX_MS", (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis() + 5000).to_string())
            .env("VEOVEO_SMOKE_CLEANUP_SECONDS", "12").kill_on_drop(true);
        let status = tokio::time::timeout(Duration::from_secs(8), command.status()).await??;
        ensure!(status.success(), "dual failure diagnostic child failed");
        Ok(())
    }
    #[tokio::test]
    async fn cancelled_owner_releases_live_authorization_then_closes_target_even_on_release_error()
    -> Result<()> {
        use futures::{SinkExt, StreamExt};
        use veoveo_testing_support::lifecycle::owner;
        if std::env::var_os("VEOVEO_CAMERA_CLEANUP_CHILD").is_none() {
            for failure in [false, true] {
                let deadline = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_millis()
                    + 1500;
                let mut command = tokio::process::Command::new(std::env::current_exe()?);
                command.args(["--exact", "browser::camera_measure::tests::cancelled_owner_releases_live_authorization_then_closes_target_even_on_release_error", "--nocapture"])
                    .env("VEOVEO_CAMERA_CLEANUP_CHILD", if failure {"failure"} else {"success"})
                    .env("VEOVEO_SMOKE_DEADLINE_UNIX_MS", deadline.to_string())
                    .env("VEOVEO_SMOKE_CLEANUP_SECONDS", "12")
                    .kill_on_drop(true);
                let status =
                    tokio::time::timeout(Duration::from_secs(8), command.status()).await??;
                ensure!(status.success(), "owned cancellation child failed");
            }
            return Ok(());
        }
        // Behavioral CDP loopback fixture. It does not claim headed/GPU visual qualification.
        let mode = std::env::var("VEOVEO_CAMERA_CLEANUP_CHILD")?;
        let observation_failure = mode == "observationFailure";
        let fail_release = mode != "success";
        let directory = std::env::temp_dir().join(uuid::Uuid::now_v7().to_string());
        let diagnostic = DiagnosticJournal::create(&directory)?;
        let events = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
        let endpoint = format!("ws://{}", listener.local_addr()?);
        let observed = events.clone();
        let server = tokio::spawn(async move {
            let mut clients = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let (socket, _) = accepted?;
                        let events = observed.clone();
                        clients.spawn(async move {
                            let mut socket = tokio_tungstenite::accept_async(socket).await?;
                            while let Some(message) = socket.next().await {
                                let message = message?;
                                let tokio_tungstenite::tungstenite::Message::Text(text) = message else { continue; };
                                let request: Value = serde_json::from_str(&text)?;
                                let method = request["method"].as_str().context("CDP method")?;
                                let expression = request["params"]["expression"].as_str().unwrap_or("");
                                let mut error = false;
                                let result = match method {
                                    "Browser.getVersion" => serde_json::json!({"product":"Chrome/behavioral-fixture"}),
                                    "Target.createTarget" => serde_json::json!({"targetId":"owned"}),
                                    "Target.attachToTarget" => serde_json::json!({"sessionId":"session"}),
                                    "Target.getTargets" => serde_json::json!({"targetInfos":[{"targetId":"app","type":"iframe","parentId":"owned","url":"https://fixture.invalid/console/api/apps/frame?uav-sim"}]}),
                                    "Target.closeTarget" => {events.lock().unwrap().push("close".into());serde_json::json!({"success":true})},
                                    "Runtime.evaluate" => {
                                        let value = if expression.contains("const expected=new Set") {
                                            events.lock().unwrap().push("release".into());
                                            error = fail_release;
                                            serde_json::json!(1)
                                        } else if expression.contains("canvasCount") {
                                            serde_json::json!({"checked":false,"canvasCount":0,"error":""})
                                        } else {serde_json::json!({"href":"about:blank","readyState":"complete"})};
                                        serde_json::json!({"result":{"value":value}})
                                    },
                                    _ => serde_json::json!({}),
                                };
                                let reply = if error {serde_json::json!({"id":request["id"],"error":{"code":-32000,"message":"fixed fixture release failure"}})}
                                    else {serde_json::json!({"id":request["id"],"result":result})};
                                socket.send(tokio_tungstenite::tungstenite::Message::Text(reply.to_string().into())).await?;
                            }
                            Ok::<(), anyhow::Error>(())
                        });
                    },
                    _ = clients.join_next(), if !clients.is_empty() => {},
                }
            }
            #[allow(unreachable_code)]
            Ok::<(), anyhow::Error>(())
        });
        let retained_slot = Arc::new(tokio::sync::Mutex::new(None::<RetainedBrowser>));
        let saved = retained_slot.clone();
        let result: Result<()> = owner::run(async {
            let (cdp, target, session) = open_headed_target(&endpoint, "about:blank").await?;
            let (retained, _registration) =
                retain_browser(cdp, target, session, 1, diagnostic.clone())?;
            *saved.lock().await = Some(retained.clone());
            let mut browser = retained.lock().await;
            browser.navigation_intent = true;
            if observation_failure {
                diagnostic.phase(DiagnosticPhase::CameraAdmission)?;
                let error = anyhow::anyhow!(
                    "fixed setup failure: HTTP 502 /console/api/apps/call?secret=hidden"
                );
                diagnostic.observation_failure(&error)?;
                return Err(error);
            }
            std::future::pending::<Result<()>>().await
        })
        .await;
        server.abort();
        let _ = server.await;
        ensure!(
            result.is_err(),
            "owner deadline did not cancel pending observation"
        );
        let retained = retained_slot
            .lock()
            .await
            .clone()
            .context("retained browser missing")?;
        let browser = retained.lock().await;
        ensure!(browser.target_closed && browser.failed == fail_release);
        ensure!(browser.released != fail_release);
        if observation_failure {
            let state = diagnostic.state.lock().unwrap();
            let observation = state
                .observation_failure
                .as_ref()
                .context("observation error lost")?;
            let cleanup = state
                .cleanup_failure
                .as_ref()
                .context("cleanup error lost")?;
            ensure!(
                observation
                    .redacted_error_chain
                    .iter()
                    .any(|detail| detail.contains("HTTP 502 /console/api/apps/call"))
            );
            ensure!(
                !observation
                    .redacted_error_chain
                    .iter()
                    .any(|detail| detail.contains("hidden"))
            );
            ensure!(
                cleanup
                    .redacted_error_chain
                    .iter()
                    .any(|detail| detail.contains("fixed fixture release failure"))
            );
            let persisted: Value = serde_json::from_slice(&fs::read(&diagnostic.path)?)?;
            ensure!(
                persisted["observationFailure"].is_object()
                    && persisted["cleanupFailure"].is_object()
            );
        }
        let events = events.lock().unwrap();
        ensure!(
            events.first().map(String::as_str) == Some("release")
                && events.iter().any(|e| e == "close"),
            "cancellation skipped authorization release or remaining target close"
        );
        Ok(())
    }
}
