//! Per-stage restart authorization ownership and synced private progress.
use super::*;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RestartOwnedView {
    document_epoch_ms: f64,
    camera_id: veoveo_uav_sim_mcp::contract::LiveCameraId,
    viewer_instance_id: veoveo_uav_sim_mcp::contract::LiveViewerInstanceId,
    live_view_id: veoveo_uav_sim_mcp::contract::LiveViewId,
    stream_product_id: LiveStreamProductId,
}
impl RestartOwnedView {
    fn admit(state: &AppVideoState) -> Result<Option<Self>> {
        if state.live_view_id.is_empty() {
            return Ok(None);
        }
        Ok(Some(Self {
            document_epoch_ms: state.document_epoch_ms,
            camera_id: veoveo_uav_sim_mcp::contract::LiveCameraId::parse(&state.camera_id)?,
            viewer_instance_id: veoveo_uav_sim_mcp::contract::LiveViewerInstanceId::parse(
                &state.viewer_instance_id,
            )?,
            live_view_id: veoveo_uav_sim_mcp::contract::LiveViewId::parse(&state.live_view_id)?,
            stream_product_id: LiveStreamProductId::parse(&state.stream_product_id)?,
        }))
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RestartAppObservation {
    captured_at: chrono::DateTime<chrono::Utc>,
    ready_state: u16,
    decoded_and_drawn_callbacks: u64,
    presentation_frame_count: Option<u64>,
    video_width: u32,
    video_height: u32,
    media_time_seconds: Option<f64>,
    sampled_at_ms: Option<f64>,
    status: String,
    error: String,
}
impl RestartAppObservation {
    fn capture(state: &AppVideoState) -> Self {
        let finite = |value: f64| value.is_finite().then_some(value);
        Self {
            captured_at: chrono::Utc::now(),
            ready_state: state.ready_state,
            // This public canvas counter advances after drawImage, not compositor presentation.
            decoded_and_drawn_callbacks: state.total_video_frames,
            presentation_frame_count: None,
            video_width: state.video_width,
            video_height: state.video_height,
            media_time_seconds: finite(state.current_time),
            sampled_at_ms: finite(state.sampled_at_ms),
            status: camera_measure::safe_detail(&state.status, &[]),
            error: camera_measure::safe_detail(&state.error, &[]),
        }
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RestartHardwareObservation {
    validated_at: chrono::DateTime<chrono::Utc>,
    webgpu_hardware: bool,
    webgl_hardware: bool,
    webgpu: String,
    webgl: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RestartDecodeObservation {
    validated_at: chrono::DateTime<chrono::Utc>,
    supported: bool,
    smooth: bool,
    power_efficient: bool,
    label: String,
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum RestartOwnerPhase {
    Prepared,
    TargetOwned,
    NavigationIntent,
    HardwareValidated,
    DecodeValidated,
    ViewObserved,
    DispatchIntent,
    ReplacementObserved,
    ObservationFailed,
    CleanupIntent,
    AuthorizationCloseResult,
    TargetCloseResult,
    CleanupResult,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RestartOwnerRecord<'a> {
    schema: &'static str,
    observed_at: chrono::DateTime<chrono::Utc>,
    component: &'a str,
    session_id: &'a veoveo_uav_sim_mcp::contract::SessionId,
    operation_end: chrono::DateTime<chrono::Utc>,
    cleanup_end: Option<chrono::DateTime<chrono::Utc>>,
    phase: RestartOwnerPhase,
    target_id: Option<&'a str>,
    cdp_session_id: Option<&'a str>,
    view: Option<RestartOwnedView>,
    app: Option<RestartAppObservation>,
    hardware: Option<RestartHardwareObservation>,
    decode: Option<RestartDecodeObservation>,
    sequence: u64,
    authorization_outcome_unknown: bool,
    authorization_closed: bool,
    succeeded: Option<bool>,
}
struct JournalStorage {
    file: fs::File,
    failed: bool,
    #[cfg(test)]
    final_append_delay: Option<Duration>,
    latest_view: Option<RestartOwnedView>,
    latest_app: Option<RestartAppObservation>,
    hardware: Option<RestartHardwareObservation>,
    decode: Option<RestartDecodeObservation>,
    authorization_closed: bool,
    sequence: u64,
    cleanup_end: Option<chrono::DateTime<chrono::Utc>>,
}
#[derive(Clone)]
pub(super) struct RestartOwnerJournal {
    storage: Arc<std::sync::Mutex<JournalStorage>>,
    component: String,
    session: veoveo_uav_sim_mcp::contract::SessionId,
    operation_end: chrono::DateTime<chrono::Utc>,
}
impl RestartOwnerJournal {
    pub(super) fn component(&self) -> String {
        self.component.clone()
    }
    pub(super) fn create(
        path: &Path,
        component: &str,
        session: veoveo_uav_sim_mcp::contract::SessionId,
        timeout: Duration,
    ) -> Result<Self> {
        use std::os::unix::fs::OpenOptionsExt;
        let mut operation_end = chrono::Utc::now() + chrono::Duration::from_std(timeout)?;
        if let Ok(original) = std::env::var("VEOVEO_SMOKE_DEADLINE_UNIX_MS") {
            let original = chrono::DateTime::from_timestamp_millis(
                original
                    .parse()
                    .context("invalid original owner deadline")?,
            )
            .context("original owner deadline out of range")?;
            operation_end = operation_end.min(original);
        }
        let journal = Self {
            storage: Arc::new(std::sync::Mutex::new(JournalStorage {
                file: fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(path)?,
                failed: false,
                #[cfg(test)]
                final_append_delay: None,
                latest_view: None,
                latest_app: None,
                hardware: None,
                decode: None,
                authorization_closed: false,
                sequence: 0,
                cleanup_end: None,
            })),
            component: component.to_owned(),
            session,
            operation_end,
        };
        journal.append(RestartOwnerPhase::Prepared, None, None, None, None)?;
        Ok(journal)
    }
    pub(super) fn hardware_validated(
        &self,
        target: &str,
        session: &str,
        hardware: &HardwareIdentity,
    ) -> Result<()> {
        hardware.validate()?;
        let (apis, webgpu, webgl) = hardware.hardware_apis();
        self.storage
            .lock()
            .map_err(|_| anyhow::anyhow!("restart journal lock poisoned"))?
            .hardware = Some(RestartHardwareObservation {
            validated_at: chrono::Utc::now(),
            webgpu_hardware: apis.contains(&BrowserGpuApi::WebGpu),
            webgl_hardware: apis.contains(&BrowserGpuApi::WebGl),
            webgpu: camera_measure::safe_detail(&webgpu, &[]),
            webgl: camera_measure::safe_detail(&webgl, &[]),
        });
        self.append(
            RestartOwnerPhase::HardwareValidated,
            Some(target),
            Some(session),
            None,
            None,
        )
    }
    pub(super) fn decode_validated(
        &self,
        target: &str,
        session: &str,
        decode: &DecodeIdentity,
    ) -> Result<()> {
        decode.validate()?;
        self.storage
            .lock()
            .map_err(|_| anyhow::anyhow!("restart journal lock poisoned"))?
            .decode = Some(RestartDecodeObservation {
            validated_at: chrono::Utc::now(),
            supported: decode.supported,
            smooth: decode.smooth,
            power_efficient: decode.power_efficient,
            label: camera_measure::safe_detail(&decode.label, &[]),
        });
        self.append(
            RestartOwnerPhase::DecodeValidated,
            Some(target),
            Some(session),
            None,
            None,
        )
    }
    pub(super) async fn dispatch<F, Fut, T>(
        &self,
        target: &str,
        cdp_session: &str,
        state: &AppVideoState,
        effect: F,
    ) -> Result<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        self.append(
            RestartOwnerPhase::DispatchIntent,
            Some(target),
            Some(cdp_session),
            Some(state),
            None,
        )?;
        veoveo_testing_support::lifecycle::owner::check_effect()?;
        ensure!(
            chrono::Utc::now() < self.operation_end,
            "restart dispatch original deadline expired"
        );
        effect().await
    }
    pub(super) fn append(
        &self,
        phase: RestartOwnerPhase,
        target: Option<&str>,
        cdp_session: Option<&str>,
        state: Option<&AppVideoState>,
        succeeded: Option<bool>,
    ) -> Result<()> {
        use std::io::Write;
        if let Some(state) = state {
            ensure!(
                state.session_id == self.session,
                "restart App observation belongs to another session"
            );
        }
        let observed = state.map(RestartOwnedView::admit).transpose()?.flatten();
        let mut storage = self
            .storage
            .lock()
            .map_err(|_| anyhow::anyhow!("restart ownership journal lock poisoned"))?;
        if let Some(view) = observed {
            storage.latest_view = Some(view);
            storage.authorization_closed = false;
        }
        if let Some(state) = state {
            storage.latest_app = Some(RestartAppObservation::capture(state));
        }
        if matches!(phase, RestartOwnerPhase::AuthorizationCloseResult) && succeeded == Some(true) {
            storage.authorization_closed = true;
        }
        storage.sequence += 1;
        let record = RestartOwnerRecord {
            schema: "veoveo.ai/uav-restart-owner/v1",
            observed_at: chrono::Utc::now(),
            component: &self.component,
            session_id: &self.session,
            operation_end: self.operation_end,
            cleanup_end: storage.cleanup_end,
            phase,
            target_id: target,
            cdp_session_id: cdp_session,
            sequence: storage.sequence,
            authorization_outcome_unknown: !storage.authorization_closed,
            authorization_closed: storage.authorization_closed,
            view: storage.latest_view.clone(),
            app: storage.latest_app.clone(),
            hardware: storage.hardware.clone(),
            decode: storage.decode.clone(),
            succeeded,
        };
        #[cfg(test)]
        if matches!(phase, RestartOwnerPhase::CleanupResult)
            && let Some(delay) = storage.final_append_delay.take()
        {
            std::thread::sleep(delay);
        }
        let written = (|| -> Result<()> {
            let encoded = serde_json::to_vec(&record)?;
            ensure!(
                storage
                    .file
                    .metadata()?
                    .len()
                    .saturating_add(encoded.len() as u64 + 1)
                    <= 8 * 1024 * 1024,
                "restart ownership journal exceeds eight MiB"
            );
            storage.file.write_all(&encoded)?;
            storage.file.write_all(b"\n")?;
            storage.file.sync_all()?;
            Ok(())
        })();
        if written.is_err() {
            storage.failed = true;
        }
        written?;
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AppTeardownResult {
    acknowledged: bool,
    failure: Option<AppTeardownFailure>,
    error_code: Option<i64>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum AppTeardownFailure {
    FrameIdentity,
    Refused,
    InvalidResult,
    Deadline,
}
async fn teardown_owned_restart_app(
    cdp: &mut Cdp,
    session: &str,
    end: std::time::Instant,
) -> Result<()> {
    let remaining = end.saturating_duration_since(std::time::Instant::now());
    ensure!(
        !remaining.is_zero(),
        "App teardown original deadline expired"
    );
    let expression = format!(
        "({})({})",
        include_str!("app_teardown.js"),
        remaining.as_millis()
    );
    let result: AppTeardownResult =
        tokio::time::timeout_at(end.into(), cdp.evaluate(session, &expression, true))
            .await
            .context("App teardown original deadline")??;
    ensure!(
        result.acknowledged && result.failure.is_none() && result.error_code.is_none(),
        "App teardown not acknowledged: {:?}, RPC code {:?}",
        result.failure,
        result.error_code
    );
    ensure!(
        std::time::Instant::now() < end,
        "App teardown acknowledged after original deadline"
    );
    Ok(())
}

pub(super) struct RestartBrowserOwner {
    pub(super) cdp: Cdp,
    pub(super) target: String,
    pub(super) session: String,
    pub(super) journal: RestartOwnerJournal,
    pub(super) navigation_intent: bool,
    pub(super) authorization_closed: bool,
    pub(super) target_closed: bool,
    pub(super) cleanup_end: Option<std::time::Instant>,
    pub(super) failed: bool,
}
impl RestartBrowserOwner {
    pub(super) async fn close(&mut self) -> Result<()> {
        use veoveo_testing_support::lifecycle::owner;
        self.failed |= self
            .journal
            .storage
            .lock()
            .map(|storage| storage.failed)
            .unwrap_or(true);
        let owner_end = owner::cleanup_deadline()?;
        let end = *self.cleanup_end.get_or_insert_with(|| {
            (std::time::Instant::now() + Duration::from_secs(30)).min(owner_end)
        });
        let end = end.min(owner_end);
        let deadline_record = (|| -> Result<()> {
            let mut storage = self
                .journal
                .storage
                .lock()
                .map_err(|_| anyhow::anyhow!("restart journal lock poisoned"))?;
            storage.cleanup_end.get_or_insert(
                chrono::Utc::now()
                    + chrono::Duration::from_std(
                        end.saturating_duration_since(std::time::Instant::now()),
                    )?,
            );
            Ok(())
        })();
        if deadline_record.is_err() {
            self.failed = true;
        }
        let intent_record = self.journal.append(
            RestartOwnerPhase::CleanupIntent,
            Some(&self.target),
            Some(&self.session),
            None,
            None,
        );
        if intent_record.is_err() {
            self.failed = true;
        }
        let release_end = end.checked_sub(Duration::from_secs(5)).unwrap_or(end);
        let release = if !self.navigation_intent || self.authorization_closed {
            Ok(())
        } else {
            tokio::time::timeout_at(
                release_end.into(),
                teardown_owned_restart_app(&mut self.cdp, &self.session, release_end),
            )
            .await
            .context("restart authorization cleanup original deadline")
            .and_then(|result| result)
        };
        let release_record = self.journal.append(
            RestartOwnerPhase::AuthorizationCloseResult,
            Some(&self.target),
            Some(&self.session),
            None,
            Some(release.is_ok()),
        );
        if release_record.is_err() {
            self.failed = true;
        }
        if release.is_ok() {
            self.authorization_closed = true;
        } else {
            self.failed = true;
        }
        let close = if self.target_closed {
            Ok(())
        } else {
            tokio::time::timeout_at(end.into(), close_target(&mut self.cdp, &self.target))
                .await
                .context("restart target cleanup original deadline")
                .and_then(|result| result)
        };
        let close_record = self.journal.append(
            RestartOwnerPhase::TargetCloseResult,
            Some(&self.target),
            Some(&self.session),
            None,
            Some(close.is_ok()),
        );
        if close_record.is_err() {
            self.failed = true;
        }
        if close.is_ok() {
            self.target_closed = true;
        } else {
            self.failed = true;
        }
        let journal = self.journal.append(
            RestartOwnerPhase::CleanupResult,
            Some(&self.target),
            Some(&self.session),
            None,
            Some(self.authorization_closed && self.target_closed && !self.failed),
        );
        if journal.is_err() {
            self.failed = true;
        }
        let completed_in_time = std::time::Instant::now() < end;
        if !completed_in_time {
            self.failed = true;
        }
        let result = (|| -> Result<()> {
            release?;
            close?;
            deadline_record?;
            intent_record?;
            release_record?;
            close_record?;
            journal?;
            ensure!(
                !self.failed,
                "restart browser cleanup previously failed; authorization ownership unresolved"
            );
            Ok(())
        })();
        if !completed_in_time {
            return result
                .and_then(|()| anyhow::bail!("restart browser cleanup deadline elapsed"))
                .context("restart browser cleanup completed after original deadline");
        }
        result?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state(view: &str) -> Result<AppVideoState> {
        Ok(serde_json::from_value(serde_json::json!({
            "sessionId":"uav-showcase", "documentEpochMs":123.0,"cameraId":"follow",
            "viewerInstanceId":"this-stage-viewer","liveViewId":view,"streamProductId":"camera-atlas",
            "readyState":2,"videoWidth":1280,"videoHeight":720,"currentTime":1.0,"sampledAtMs":1.0,
            "totalVideoFrames":3,"droppedVideoFrames":0,"declaredFrameRateHz":16.0,
            "meanLuma":50.0,"lumaStandardDeviation":15.0,"minimumLuma":0,"maximumLuma":100,
            "lumaCells":vec![serde_json::json!({"minimum":0,"maximum":100,"standardDeviation":15.0});16],
            "pixelSampleError":"","decodeLabel":"software H.264 decode","status":"","error":"","bodyText":""
        }))?)
    }
    fn records(path: &Path) -> Result<Vec<Value>> {
        fs::read_to_string(path)?
            .lines()
            .map(|line| Ok(serde_json::from_str(line)?))
            .collect()
    }
    #[tokio::test]
    async fn public_failure_observation_survives_cancelled_wait_and_stage_cleanup() -> Result<()> {
        use veoveo_testing_support::lifecycle::owner::{self, CleanupKind};
        const CHILD: &str = "VEOVEO_RESTART_OBSERVATION_TEST_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let mut command = tokio::process::Command::new(std::env::current_exe()?);
            command.args([
                "browser::restart_journal::tests::public_failure_observation_survives_cancelled_wait_and_stage_cleanup",
                "--exact", "--nocapture",
            ]).env(CHILD, "1").kill_on_drop(true);
            let status = tokio::time::timeout(Duration::from_secs(10), command.status()).await??;
            ensure!(
                status.success(),
                "isolated owner observation control failed: {status}"
            );
            return Ok(());
        }
        let directory = tempfile::tempdir()?;
        for component in ["mcp", "runtime-and-forwarder"] {
            let path = directory.path().join(format!("{component}.jsonl"));
            let journal = RestartOwnerJournal::create(
                &path,
                component,
                veoveo_uav_sim_mcp::contract::SessionId::parse("uav-showcase")?,
                Duration::from_millis(10),
            )?;
            let retained = journal.clone();
            let mut failed = state(component)?;
            failed.ready_state = 0;
            failed.total_video_frames = 47;
            failed.current_time = 7.5;
            failed.sampled_at_ms = 1234.0;
            failed.status =
                "waiting https://example.test/live?token=QUERY_SECRET#FRAGMENT_SECRET".into();
            failed.error = "Authorization: Bearer HEADER_SECRET".into();
            failed.body_text = "OPAQUE_BODY_SECRET".into();
            let result: Result<()> = owner::run(async {
                owner::register_cleanup(
                    CleanupKind::Remote,
                    "restart observation",
                    component,
                    move || async move {
                        retained.append(
                            RestartOwnerPhase::CleanupIntent,
                            Some("owned-target"),
                            Some("owned-cdp"),
                            None,
                            None,
                        )
                    },
                )?;
                journal.append(
                    RestartOwnerPhase::ObservationFailed,
                    Some("owned-target"),
                    Some("owned-cdp"),
                    Some(&failed),
                    Some(false),
                )?;
                tokio::time::timeout(Duration::from_millis(20), std::future::pending::<()>())
                    .await?;
                Ok(())
            })
            .await;
            ensure!(result.is_err());
            let saved = records(&path)?;
            let last = saved.last().context("cleanup snapshot missing")?;
            ensure!(last["phase"] == "cleanupIntent" && last["component"] == component);
            ensure!(last["view"]["liveViewId"] == component);
            ensure!(
                last["app"]["decodedAndDrawnCallbacks"] == 47 && last["app"]["readyState"] == 0
            );
            ensure!(last["app"]["videoWidth"] == 1280 && last["app"]["mediaTimeSeconds"] == 7.5);
            ensure!(
                last["app"]["sampledAtMs"] == 1234.0
                    && last["app"]["presentationFrameCount"].is_null()
            );
            ensure!(last["hardware"].is_null() && last["decode"].is_null());
            ensure!(
                saved
                    .iter()
                    .all(|record| record["operationEnd"] == last["operationEnd"])
            );
            let text = fs::read_to_string(&path)?;
            for secret in [
                "QUERY_SECRET",
                "FRAGMENT_SECRET",
                "HEADER_SECRET",
                "OPAQUE_BODY_SECRET",
            ] {
                ensure!(!text.contains(secret), "diagnostic leaked {secret}");
            }
            let dispatched = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let effect = dispatched.clone();
            let expired: Result<()> = owner::run(async {
                journal
                    .dispatch("owned-target", "owned-cdp", &failed, || async move {
                        effect.store(true, std::sync::atomic::Ordering::SeqCst);
                        Ok(())
                    })
                    .await
            })
            .await;
            ensure!(expired.is_err() && !dispatched.load(std::sync::atomic::Ordering::SeqCst));
        }
        Ok(())
    }
    #[test]
    fn only_admitted_hardware_and_decode_identity_are_carried_into_cleanup() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("owner.jsonl");
        let journal = RestartOwnerJournal::create(
            &path,
            "mcp",
            veoveo_uav_sim_mcp::contract::SessionId::parse("uav-showcase")?,
            Duration::from_secs(30),
        )?;
        let hardware: HardwareIdentity = serde_json::from_value(serde_json::json!({
            "userAgent":"Chrome/behavioral-fixture", "webgpuVendor":"nvidia", "webgpuArchitecture":"ada",
            "webgpuDevice":"fixture", "webgpuDescription":"NVIDIA fixture", "webglAvailable":false,
            "webglVendor":"", "webglRenderer":""
        }))?;
        let invalid: HardwareIdentity = serde_json::from_value(serde_json::json!({
            "userAgent":"Chrome/behavioral-fixture", "webgpuVendor":"google", "webgpuArchitecture":"",
            "webgpuDevice":"", "webgpuDescription":"SwiftShader", "webglAvailable":false,
            "webglVendor":"", "webglRenderer":""
        }))?;
        ensure!(
            journal
                .hardware_validated("owned-target", "owned-cdp", &invalid)
                .is_err()
        );
        let mut decode = DecodeIdentity {
            supported: false,
            smooth: true,
            power_efficient: false,
            label: "NVIDIA NVENC · software H.264 decode".into(),
        };
        ensure!(
            journal
                .decode_validated("owned-target", "owned-cdp", &decode)
                .is_err()
        );
        ensure!(records(&path)?.last().context("prepared")?["hardware"].is_null());
        journal.hardware_validated("owned-target", "owned-cdp", &hardware)?;
        decode.supported = true;
        journal.decode_validated("owned-target", "owned-cdp", &decode)?;
        journal.append(
            RestartOwnerPhase::CleanupIntent,
            Some("owned-target"),
            Some("owned-cdp"),
            None,
            None,
        )?;
        let saved = records(&path)?.pop().context("cleanup")?;
        ensure!(
            saved["hardware"]["webgpuHardware"] == true
                && saved["hardware"]["webglHardware"] == false
        );
        ensure!(saved["decode"]["supported"] == true && saved["decode"]["powerEfficient"] == false);
        Ok(())
    }
    #[test]
    fn observation_redacts_credentials_bounds_data_and_latches_capacity_failure() -> Result<()> {
        for secret in [
            "access_token=SECRET",
            "accessToken=SECRET",
            "X-API-Key: SECRET",
            "api_key=SECRET",
            "private_key=SECRET",
            "password=SECRET",
            "https://user:SECRET@example.test/live",
            "('https://user:SECRET@example.test/live')",
            "\"https://user:SECRET@example.test/live\"",
            "[https://user:SECRET@example.test/live]",
            "url=(https://user:SECRET@example.test/live)",
        ] {
            ensure!(!camera_measure::safe_detail(secret, &[]).contains("SECRET"));
        }
        ensure!(
            camera_measure::safe_detail(&"界".repeat(1000), &[])
                .chars()
                .count()
                <= 320
        );
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("owner.jsonl");
        let journal = RestartOwnerJournal::create(
            &path,
            "mcp",
            veoveo_uav_sim_mcp::contract::SessionId::parse("uav-showcase")?,
            Duration::from_secs(30),
        )?;
        let mut wrong = state("wrong-session-view")?;
        wrong.session_id = veoveo_uav_sim_mcp::contract::SessionId::parse("another-session")?;
        ensure!(
            journal
                .append(
                    RestartOwnerPhase::ViewObserved,
                    None,
                    None,
                    Some(&wrong),
                    None
                )
                .is_err()
        );
        ensure!(records(&path)?.last().context("prepared")?["app"].is_null());
        journal
            .storage
            .lock()
            .unwrap()
            .file
            .set_len(8 * 1024 * 1024)?;
        ensure!(
            journal
                .append(RestartOwnerPhase::ObservationFailed, None, None, None, None)
                .is_err()
        );
        {
            use std::io::Seek;
            let mut storage = journal.storage.lock().unwrap();
            storage.file.set_len(0)?;
            storage.file.rewind()?;
        }
        journal.append(RestartOwnerPhase::CleanupIntent, None, None, None, None)?;
        ensure!(
            journal.storage.lock().unwrap().failed,
            "restored storage must not clear failed ownership"
        );
        Ok(())
    }
    #[tokio::test]
    async fn dispatch_is_synced_before_effect_and_failure_retains_only_its_stage_owner()
    -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("runtime-owner.jsonl");
        let journal = RestartOwnerJournal::create(
            &path,
            "runtime-and-forwarder",
            veoveo_uav_sim_mcp::contract::SessionId::parse("uav-showcase")?,
            Duration::from_secs(30),
        )?;
        let before = state("runtime-before")?;
        let result: Result<()> = journal
            .dispatch(
                "runtime-owned-target",
                "runtime-cdp-session",
                &before,
                || async {
                    let records = records(&path)?;
                    let intent = records
                        .last()
                        .context("dispatch intent missing before effect")?;
                    ensure!(
                        intent["phase"] == "dispatchIntent"
                            && intent["view"]["liveViewId"] == "runtime-before"
                    );
                    anyhow::bail!("fixed effect observation failure")
                },
            )
            .await;
        ensure!(result.is_err());
        journal.append(
            RestartOwnerPhase::ReplacementObserved,
            Some("runtime-owned-target"),
            Some("runtime-cdp-session"),
            Some(&state("runtime-replacement")?),
            None,
        )?;
        journal.append(
            RestartOwnerPhase::CleanupIntent,
            Some("runtime-owned-target"),
            Some("runtime-cdp-session"),
            None,
            None,
        )?;
        let pending = records(&path)?.pop().context("cleanup record")?;
        ensure!(
            pending["view"]["liveViewId"] == "runtime-replacement"
                && pending["authorizationOutcomeUnknown"] == true
        );
        journal.append(
            RestartOwnerPhase::AuthorizationCloseResult,
            Some("runtime-owned-target"),
            Some("runtime-cdp-session"),
            None,
            Some(true),
        )?;
        let closed = records(&path)?.pop().context("close record")?;
        ensure!(
            closed["authorizationClosed"] == true && closed["authorizationOutcomeUnknown"] == false
        );
        ensure!(closed["view"]["viewerInstanceId"] == "this-stage-viewer");
        use std::os::unix::fs::PermissionsExt;
        ensure!(fs::metadata(path)?.permissions().mode() & 0o777 == 0o600);
        Ok(())
    }
    #[tokio::test]
    async fn cleanup_attempts_owned_target_after_journal_or_remote_close_failure() -> Result<()> {
        use futures::{SinkExt, StreamExt};
        use veoveo_testing_support::lifecycle::owner;
        for mode in [
            "ack",
            "refuse",
            "journalFailure",
            "transientJournalFailure",
            "lateAppend",
        ] {
            let directory = tempfile::tempdir()?;
            let journal = RestartOwnerJournal::create(
                &directory.path().join("owner.jsonl"),
                "runtime-and-forwarder",
                veoveo_uav_sim_mcp::contract::SessionId::parse("uav-showcase")?,
                Duration::from_secs(5),
            )?;
            let listener =
                tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
            let endpoint = format!("ws://{}", listener.local_addr()?);
            let events = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
            let observed = events.clone();
            let server = tokio::spawn(async move {
                let (socket, _) = listener.accept().await?;
                let mut socket = tokio_tungstenite::accept_async(socket).await?;
                while let Some(message) = socket.next().await {
                    let tokio_tungstenite::tungstenite::Message::Text(text) = message? else {
                        continue;
                    };
                    let request: Value = serde_json::from_str(&text)?;
                    let method = request["method"].as_str().context("CDP method")?;
                    let result = match method {
                        "Browser.getVersion" => {
                            serde_json::json!({"product":"Chrome/behavioral-fixture"})
                        }
                        "Target.createTarget" => {
                            serde_json::json!({"targetId":"this-stage-target"})
                        }
                        "Target.attachToTarget" => {
                            serde_json::json!({"sessionId":"this-stage-cdp"})
                        }
                        "Target.closeTarget" => {
                            observed.lock().unwrap().push("target".into());
                            serde_json::json!({"success":true})
                        }
                        "Runtime.evaluate" => {
                            let expression = request["params"]["expression"].as_str().unwrap_or("");
                            let value = if expression.contains("ui/resource-teardown") {
                                observed.lock().unwrap().push("authorization".into());
                                if mode == "refuse" {
                                    serde_json::json!({"acknowledged":false,"failure":"refused","errorCode":-32000})
                                } else {
                                    serde_json::json!({"acknowledged":true,"failure":null,"errorCode":null})
                                }
                            } else {
                                serde_json::json!({"href":"about:blank","readyState":"complete"})
                            };
                            serde_json::json!({"result":{"value":value}})
                        }
                        _ => serde_json::json!({}),
                    };
                    socket
                        .send(tokio_tungstenite::tungstenite::Message::Text(
                            serde_json::json!({"id":request["id"],"result":result})
                                .to_string()
                                .into(),
                        ))
                        .await?;
                }
                Ok::<_, anyhow::Error>(())
            });
            let saved = Arc::new(tokio::sync::Mutex::new(None));
            let retained = saved.clone();
            let result: Result<()> = owner::run(async {
                let (cdp, target, session) = open_headed_target(&endpoint, "about:blank").await?;
                let mut browser = RestartBrowserOwner {
                    cdp,
                    target,
                    session,
                    journal: journal.clone(),
                    navigation_intent: true,
                    authorization_closed: false,
                    target_closed: false,
                    cleanup_end: None,
                    failed: false,
                };
                if mode == "journalFailure" || mode == "transientJournalFailure" {
                    journal.storage.lock().unwrap().file =
                        fs::File::open(directory.path().join("owner.jsonl"))?;
                    ensure!(
                        journal
                            .append(
                                RestartOwnerPhase::ObservationFailed,
                                Some("this-stage-target"),
                                Some("this-stage-cdp"),
                                None,
                                Some(false)
                            )
                            .is_err()
                    );
                    if mode == "transientJournalFailure" {
                        journal.storage.lock().unwrap().file = fs::OpenOptions::new()
                            .append(true)
                            .open(directory.path().join("owner.jsonl"))?;
                    }
                }
                let mut result = browser.close().await;
                if mode == "lateAppend" {
                    result?;
                    ensure!(browser.authorization_closed && browser.target_closed);
                    browser.cleanup_end =
                        Some(std::time::Instant::now() + Duration::from_millis(10));
                    journal.storage.lock().unwrap().final_append_delay =
                        Some(Duration::from_millis(30));
                    result = browser.close().await;
                    ensure!(result.is_err() && browser.failed);
                    ensure!(
                        browser.close().await.is_err(),
                        "retry must not promote late cleanup"
                    );
                }
                *retained.lock().await = Some((
                    browser.authorization_closed,
                    browser.target_closed,
                    browser.failed,
                    browser.cleanup_end,
                ));
                result
            })
            .await;
            let (authorization, target, failed, end) =
                saved.lock().await.take().context("cleanup facts missing")?;
            ensure!(target && authorization == (mode != "refuse") && failed == (mode != "ack"));
            ensure!(result.is_ok() == (mode == "ack"));
            ensure!(end.is_some());
            ensure!(*events.lock().unwrap() == ["authorization", "target"]);
            server.abort();
            let _ = server.await;
        }
        Ok(())
    }
}
