use std::{fs, path::Path, process::Stdio, time::Duration};

use anyhow::{Context as _, Result, ensure};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use veoveo_uav_sim_mcp::contract::{
    LiveCameraHealth, LiveCameraId, SimulationLifecycle, SimulationState,
};

use super::InstalledTarget;
use veoveo_testing_support::{
    connect_mcp_client,
    installed::restart::{DeploymentRestart, DrainGroupReceipt, DrainProfile, SelectedDrainGroup},
    lifecycle::owner,
};
use veoveo_types::ResourceAddress;
use veoveo_uav_sim_mcp::contract::{SessionId, UavResource};

use super::{
    FocusedScenario, OperatorClient, PRIMARY_CAMERA_ID,
    browser::{
        ConsoleLiveRestartEvidence, capture_console_live_app_restart, preflight_console_live_app,
    },
    git_revision, simulation_state,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RestartAcceptanceEvidence {
    schema: &'static str,
    completed_at: chrono::DateTime<Utc>,
    source_revision: String,
    run_id: String,
    scenario_path: String,
    session_id: String,
    lifecycle_after_mcp_restart: SimulationLifecycle,
    lifecycle_after_simulator_restart: SimulationLifecycle,
    mcp_container: KubernetesRestartEvidence,
    simulator_container: DrainGroupReceipt,
    mcp: ConsoleLiveRestartEvidence,
    simulator: ConsoleLiveRestartEvidence,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RestartStageEvidence<'a, T> {
    schema: &'static str,
    source_revision: &'a str,
    run_id: &'a str,
    lifecycle: &'a SimulationLifecycle,
    container: &'a T,
    browser: &'a ConsoleLiveRestartEvidence,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct KubernetesRestartEvidence {
    pod_name: String,
    pod_uid: String,
    container_name: String,
    status_collection: &'static str,
    restart_count_before: u32,
    restart_count_after: u32,
    container_id_before: String,
    container_id_after: String,
    image_id: String,
}

#[derive(Debug, Deserialize)]
struct KubernetesPodList {
    items: Vec<KubernetesPod>,
}

#[derive(Debug, Deserialize)]
struct KubernetesPod {
    metadata: KubernetesMetadata,
    status: KubernetesPodStatus,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KubernetesMetadata {
    name: String,
    uid: String,
    #[serde(default)]
    deletion_timestamp: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KubernetesPodStatus {
    phase: String,
    #[serde(default)]
    container_statuses: Vec<KubernetesContainerStatus>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KubernetesContainerStatus {
    name: String,
    ready: bool,
    restart_count: u32,
    #[serde(default, rename = "imageID")]
    image_id: String,
    #[serde(default, rename = "containerID")]
    container_id: String,
}

#[derive(Clone, Copy)]
enum KubernetesStatusCollection {
    Application,
}

impl KubernetesStatusCollection {
    fn field(self) -> &'static str {
        match self {
            Self::Application => "containerStatuses",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Application => "application",
        }
    }

    fn statuses(self, pod: &KubernetesPod) -> &[KubernetesContainerStatus] {
        match self {
            Self::Application => &pod.status.container_statuses,
        }
    }
}

#[derive(Clone, Copy)]
enum UavWorkload {
    Runtime,
    Mcp,
}

impl UavWorkload {
    fn selector(self) -> &'static str {
        match self {
            Self::Runtime => "app.kubernetes.io/name=uav-sim,app.kubernetes.io/component=uav-sim",
            Self::Mcp => {
                "app.kubernetes.io/name=uav-sim-mcp,app.kubernetes.io/component=uav-sim-mcp"
            }
        }
    }
}

pub struct RestartVerification<'a> {
    pub conformance: &'a Path,
    pub scenario_path: &'a Path,
    pub installation: &'a InstalledTarget,
    pub chrome_cdp_url: &'a str,
    pub restart_timeout: Duration,
    pub evidence_root: &'a Path,
}

type CloseFuture = std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>>;
struct AcquiredCaller {
    peer: rmcp::Peer<rmcp::RoleClient>,
    close: CloseFuture,
}
type AcquireFuture =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<AcquiredCaller>> + Send>>;
#[derive(Default)]
struct CallerClose {
    acquisition: Option<AcquireFuture>,
    peer: Option<rmcp::Peer<rmcp::RoleClient>>,
    future: Option<CloseFuture>,
    deadline: Option<tokio::time::Instant>,
    failed: bool,
}
impl CallerClose {
    fn connecting(endpoint: String, token: String) -> Self {
        Self {
            acquisition: Some(Box::pin(async move {
                let client = connect_mcp_client(&endpoint, &token).await?;
                // No await between acknowledged acquisition and owned shutdown transfer.
                Ok(AcquiredCaller {
                    peer: client.peer().clone(),
                    close: Box::pin(async move { client.cancel().await }),
                })
            })),
            ..Default::default()
        }
    }
    async fn acquire(&mut self) -> Result<rmcp::Peer<rmcp::RoleClient>> {
        if let Some(acquisition) = self.acquisition.as_mut() {
            let result = acquisition.await;
            self.acquisition = None;
            let acquired = result?;
            self.peer = Some(acquired.peer);
            self.future = Some(acquired.close);
        }
        self.peer.clone().context("restart caller was not acquired")
    }
    async fn finish(&mut self) -> Result<()> {
        let end = match self.deadline {
            Some(end) => end,
            None => {
                let end = tokio::time::Instant::from_std(owner::cleanup_deadline()?);
                self.deadline = Some(end);
                end
            }
        };
        if self.acquisition.is_some() {
            match tokio::time::timeout_at(end, self.acquire()).await {
                Ok(Ok(_)) => {}
                Ok(Err(error)) => {
                    self.failed = true;
                    return Err(error);
                }
                Err(error) => {
                    self.failed = true;
                    return Err(error.into());
                }
            }
        }
        if let Some(close) = self.future.as_mut() {
            match tokio::time::timeout_at(end, close).await {
                Ok(result) => {
                    self.future = None;
                    if result.is_err() || tokio::time::Instant::now() >= end {
                        self.failed = true;
                    }
                    result?;
                }
                Err(error) => {
                    self.failed = true;
                    return Err(error.into());
                }
            }
        }
        ensure!(!self.failed, "restart caller cleanup remains unqualified");
        Ok(())
    }
}
fn retain_caller(
    caller: CallerClose,
) -> Result<(
    std::sync::Arc<tokio::sync::Mutex<CallerClose>>,
    owner::CleanupRegistration,
)> {
    let retained = std::sync::Arc::new(tokio::sync::Mutex::new(caller));
    let cleanup = retained.clone();
    let registration = owner::register_cleanup(
        owner::CleanupKind::Remote,
        "uav_restart_sdk",
        "normal_oauth",
        move || async move { cleanup.lock().await.finish().await },
    )?;
    Ok((retained, registration))
}

async fn current_state(
    peer: &rmcp::Peer<rmcp::RoleClient>,
    uri: &veoveo_types::ResourceUri,
    expected_session: &SessionId,
) -> Result<SimulationState> {
    use rmcp::model::{
        ClientRequest, ReadResourceRequest, ReadResourceRequestParams, ServerResult,
    };
    let request = ClientRequest::ReadResourceRequest(ReadResourceRequest::new(
        ReadResourceRequestParams::new(uri.as_str()),
    ));
    let ServerResult::ReadResourceResult(result) = peer.send_request(request).await? else {
        anyhow::bail!("UAV current state returned another result type");
    };
    ensure!(
        result.contents.len() == 1,
        "UAV state requires one resource body"
    );
    let rmcp::model::ResourceContents::TextResourceContents {
        uri: actual, text, ..
    } = &result.contents[0]
    else {
        anyhow::bail!("UAV state requires text");
    };
    ensure!(
        actual == uri.as_str(),
        "UAV state returned another resource"
    );
    let state: SimulationState = serde_json::from_str(text)?;
    ensure!(
        &state.session_id == expected_session,
        "UAV current state returned another session"
    );
    Ok(state)
}
fn require_same_runtime(before: &SimulationState, after: &SimulationState) -> Result<()> {
    ensure!(
        before.session_id == after.session_id
            && before.world.is_some()
            && before.world == after.world,
        "runtime replacement changed selected session or world"
    );
    let ids = |state: &SimulationState| {
        state
            .vehicles
            .iter()
            .map(|v| v.vehicle_id.clone())
            .collect::<std::collections::BTreeSet<_>>()
    };
    ensure!(
        !before.vehicles.is_empty() && ids(before) == ids(after),
        "runtime replacement changed fleet identities"
    );
    ensure!(
        before.live_cameras.len() == after.live_cameras.len(),
        "runtime replacement changed camera count"
    );
    for camera in &before.live_cameras {
        let current = after
            .live_cameras
            .iter()
            .find(|v| v.camera_id == camera.camera_id)
            .context("runtime replacement lost selected camera")?;
        ensure!(
            current.session_id == camera.session_id
                && current.rig == camera.rig
                && current.width_px == camera.width_px
                && current.height_px == camera.height_px
                && current.frame_rate_millihertz == camera.frame_rate_millihertz
                && current.stream_policy == camera.stream_policy
                && current.vertical_fov_degrees == camera.vertical_fov_degrees
                && current.near_clip_m == camera.near_clip_m
                && current.far_clip_m == camera.far_clip_m,
            "runtime replacement changed camera configuration"
        );
    }
    Ok(())
}
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum RestartProgressStage {
    Selected,
    DispatchIntent,
    DrainReturned,
    Verified,
    Failed,
    CleanupObservation,
    CallerClosed,
    CallerCloseFailed,
}

fn append_progress(
    file: &std::sync::Mutex<fs::File>,
    stage: RestartProgressStage,
    selected: &SelectedDrainGroup,
    failure: Option<&anyhow::Error>,
) -> Result<()> {
    use sha2::{Digest, Sha256};
    use std::io::Write;
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Progress {
        schema: &'static str,
        stage: RestartProgressStage,
        observed_at: chrono::DateTime<Utc>,
        progress: veoveo_testing_support::installed::restart::DrainGroupProgress,
        failure_sha256: Option<String>,
    }
    let record = Progress {
        schema: "veoveo.ai/uav-live-view-restart-progress/v2",
        stage,
        observed_at: Utc::now(),
        progress: selected.progress(),
        failure_sha256: failure
            .map(|error| hex::encode(Sha256::digest(format!("{error:#}").as_bytes()))),
    };
    let mut file = file
        .lock()
        .map_err(|_| anyhow::anyhow!("restart progress lock poisoned"))?;
    serde_json::to_writer(&mut *file, &record)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

pub async fn verify_live_view_restarts(config: RestartVerification<'_>) -> Result<()> {
    let RestartVerification {
        conformance,
        scenario_path,
        installation,
        chrome_cdp_url,
        restart_timeout,
        evidence_root,
    } = config;
    let context = &installation.target.kubernetes.context;
    let namespace = &installation.target.kubernetes.namespace;
    let public_base_url = installation.public_base();
    ensure!(
        conformance.is_file(),
        "required binary does not exist: {}",
        conformance.display()
    );
    ensure!(
        !context.trim().is_empty() && !namespace.trim().is_empty(),
        "Kubernetes context and namespace are required"
    );
    ensure!(
        (Duration::from_secs(1)..=Duration::from_secs(1800)).contains(&restart_timeout),
        "replacement timeout must be between one and 1800 seconds"
    );
    let scenario: FocusedScenario = serde_json::from_slice(
        &fs::read(scenario_path)
            .with_context(|| format!("reading scenario {}", scenario_path.display()))?,
    )
    .with_context(|| format!("decoding scenario {}", scenario_path.display()))?;
    let public_base_url = public_base_url.trim_end_matches('/');
    ensure!(
        url::Url::parse(public_base_url)?.scheme() == "https",
        "focused browser acceptance requires public HTTPS"
    );
    preflight_console_live_app(
        chrome_cdp_url,
        public_base_url,
        Duration::from_secs(scenario.view.timeout_seconds),
    )
    .await?;
    let token = installation.token().await?;
    let operator = OperatorClient {
        conformance,
        installation,
        token: &token,
    };
    wait_for_running_live_camera(
        &operator,
        &scenario.session_id,
        PRIMARY_CAMERA_ID,
        Duration::from_secs(scenario.view.timeout_seconds),
    )
    .await?;

    let source_revision = git_revision()?;
    let run_id = uuid::Uuid::now_v7().to_string();
    let evidence_directory = evidence_root.join(&source_revision).join(&run_id);
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    fs::create_dir_all(&evidence_directory).with_context(|| {
        format!(
            "creating focused restart evidence directory {}",
            evidence_directory.display()
        )
    })?;

    fs::set_permissions(&evidence_directory, fs::Permissions::from_mode(0o700))?;

    let (mcp, mcp_container) = capture_console_live_app_restart(
        chrome_cdp_url,
        public_base_url,
        PRIMARY_CAMERA_ID,
        "uav-sim-mcp",
        &evidence_directory.join("uav-live-view-mcp-restart.png"),
        restart_timeout,
        || async {
            restart_kubernetes_container(
                context,
                namespace,
                UavWorkload::Mcp,
                "uav-sim-mcp",
                KubernetesStatusCollection::Application,
                restart_timeout,
            )
            .await
        },
    )
    .await?;
    let state_after_mcp = simulation_state(&operator, &scenario.session_id).await?;
    require_running_live_camera(&state_after_mcp, PRIMARY_CAMERA_ID)?;
    let lifecycle_after_mcp_restart = state_after_mcp.lifecycle.clone();
    write_restart_stage(
        &evidence_directory.join("mcp-restart.json"),
        &source_revision,
        &run_id,
        &lifecycle_after_mcp_restart,
        &mcp_container,
        &mcp,
    )?;

    let (close, close_registration) = retain_caller(CallerClose::connecting(
        installation.operator.resource.as_str().to_owned(),
        token.clone(),
    ))?;
    let peer = close.lock().await.acquire().await?;
    let session_id = SessionId::parse(&scenario.session_id)?;
    let state_uri = UavResource::Session(session_id.clone()).to_uri()?;
    let baseline = current_state(&peer, &state_uri, &session_id).await?;
    require_running_live_camera(&baseline, PRIMARY_CAMERA_ID)?;

    let pod = current_uav_pod(context, namespace, UavWorkload::Runtime, restart_timeout).await?;
    let restart = DeploymentRestart::new(
        &installation.target,
        "uav-sim",
        "uav-sim",
        peer.clone(),
        state_uri.clone(),
    )?;
    let selected = restart
        .select_drain_group(
            &pod.metadata.name,
            vec![
                DrainProfile::nvidia("isaac-sim", Duration::from_secs(120))?,
                DrainProfile::server("recording-forwarder", Duration::from_secs(120))?,
            ],
            restart_timeout,
        )
        .await?;
    let progress = std::sync::Arc::new(std::sync::Mutex::new(
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(evidence_directory.join("runtime-progress.jsonl"))?,
    ));
    append_progress(&progress, RestartProgressStage::Selected, &selected, None)?;
    let retained_progress = progress.clone();
    let retained_selection = selected.clone();
    let progress_registration = owner::register_cleanup(
        owner::CleanupKind::Remote,
        "uav_runtime_progress",
        "selected_group",
        move || async move {
            append_progress(
                &retained_progress,
                RestartProgressStage::CleanupObservation,
                &retained_selection,
                None,
            )
        },
    )?;
    let runtime_result = async {
        let (simulator, simulator_container) = capture_console_live_app_restart(
            chrome_cdp_url,
            public_base_url,
            PRIMARY_CAMERA_ID,
            "runtime-and-forwarder",
            &evidence_directory.join("uav-live-view-simulator-restart.png"),
            restart_timeout + Duration::from_secs(140),
            || async {
                append_progress(
                    &progress,
                    RestartProgressStage::DispatchIntent,
                    &selected,
                    None,
                )?;
                let result = restart.restart_with_drain_group(&selected).await;
                append_progress(
                    &progress,
                    RestartProgressStage::DrainReturned,
                    &selected,
                    result.as_ref().err(),
                )?;
                result
            },
        )
        .await?;
        let final_state = current_state(&peer, &state_uri, &session_id).await?;
        require_running_live_camera(&final_state, PRIMARY_CAMERA_ID)?;
        require_same_runtime(&baseline, &final_state)?;
        restart
            .verify_drain_group_replacement(&selected, &simulator_container)
            .await?;
        Ok::<_, anyhow::Error>((simulator, simulator_container, final_state.lifecycle))
    }
    .await;
    append_progress(
        &progress,
        if runtime_result.is_ok() {
            RestartProgressStage::Verified
        } else {
            RestartProgressStage::Failed
        },
        &selected,
        runtime_result.as_ref().err(),
    )?;
    let (simulator, simulator_container, lifecycle_after_simulator_restart) = runtime_result?;
    let close_result = close.lock().await.finish().await;
    append_progress(
        &progress,
        if close_result.is_ok() {
            RestartProgressStage::CallerClosed
        } else {
            RestartProgressStage::CallerCloseFailed
        },
        &selected,
        close_result.as_ref().err(),
    )?;
    close_result?;
    close_registration.settled()?;
    progress_registration.settled()?;
    write_restart_stage(
        &evidence_directory.join("simulator-restart.json"),
        &source_revision,
        &run_id,
        &lifecycle_after_simulator_restart,
        &simulator_container,
        &simulator,
    )?;

    let evidence = RestartAcceptanceEvidence {
        schema: "veoveo.ai/uav-live-view-restart-evidence/v2",
        completed_at: Utc::now(),
        source_revision,
        run_id,
        scenario_path: scenario_path.display().to_string(),
        session_id: scenario.session_id,
        lifecycle_after_mcp_restart,
        lifecycle_after_simulator_restart,
        mcp_container,
        simulator_container,
        mcp,
        simulator,
    };
    let manifest = evidence_directory.join("evidence.json");
    write_private_report(&manifest, &evidence)
        .with_context(|| format!("writing restart evidence {}", manifest.display()))?;
    println!(
        "Focused native live-view restart acceptance passed. Evidence: {}",
        manifest.display()
    );
    Ok(())
}

fn write_restart_stage<T: Serialize>(
    path: &Path,
    source_revision: &str,
    run_id: &str,
    lifecycle: &SimulationLifecycle,
    container: &T,
    browser: &ConsoleLiveRestartEvidence,
) -> Result<()> {
    let stage = RestartStageEvidence {
        schema: "veoveo.ai/uav-live-view-restart-stage/v2",
        source_revision,
        run_id,
        lifecycle,
        container,
        browser,
    };
    write_private_report(path, &stage)
        .with_context(|| format!("writing accepted restart stage {}", path.display()))
}

fn write_private_report(path: &Path, report: &impl Serialize) -> Result<()> {
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(&serde_json::to_vec_pretty(report)?)?;
    file.sync_all()?;
    Ok(())
}

async fn wait_for_running_live_camera(
    operator: &OperatorClient<'_>,
    session: &str,
    camera: &str,
    timeout: Duration,
) -> Result<()> {
    let mut last_state = None;
    tokio::time::timeout(timeout, async {
        loop {
            last_state = Some(simulation_state(operator, session).await?);
            match live_camera_health(last_state.as_ref().unwrap(), camera)? {
                LiveCameraHealth::Healthy => return Ok(()),
                LiveCameraHealth::Warming => tokio::time::sleep(Duration::from_secs(1)).await,
                LiveCameraHealth::Stale | LiveCameraHealth::Failed => {
                    return Err(anyhow::anyhow!(
                        "live camera failed during startup: {last_state:?}"
                    ));
                }
            }
        }
    })
    .await
    .with_context(|| format!("live camera startup exceeded {timeout:?}: {last_state:?}"))?
}

fn require_running_live_camera(state: &SimulationState, camera_id: &str) -> Result<()> {
    ensure!(
        live_camera_health(state, camera_id)? == LiveCameraHealth::Healthy,
        "authoritative live camera {camera_id} is not healthy: {state:?}"
    );
    Ok(())
}

fn live_camera_health(state: &SimulationState, camera_id: &str) -> Result<LiveCameraHealth> {
    ensure!(
        state.lifecycle == SimulationLifecycle::Running,
        "restart acceptance requires the authoritative simulation to be running: {state:?}"
    );
    let camera_id = LiveCameraId::parse(camera_id)?;
    state
        .live_cameras
        .iter()
        .find(|camera| camera.camera_id == camera_id)
        .map(|camera| camera.health)
        .with_context(|| format!("authoritative simulator omitted live camera {camera_id}"))
}

async fn restart_kubernetes_container(
    context: &str,
    namespace: &str,
    workload: UavWorkload,
    container_name: &str,
    collection: KubernetesStatusCollection,
    timeout: Duration,
) -> Result<KubernetesRestartEvidence> {
    let before_pod = current_uav_pod(context, namespace, workload, timeout).await?;
    let runtime_before = if matches!(workload, UavWorkload::Mcp) {
        Some(current_uav_pod(context, namespace, UavWorkload::Runtime, timeout).await?)
    } else {
        None
    };
    ensure!(
        before_pod.status.phase == "Running",
        "authoritative simulator pod is not Running: {} is {}",
        before_pod.metadata.name,
        before_pod.status.phase
    );
    let before = container_status(&before_pod, container_name, collection)?;
    ensure!(
        before.ready && !before.container_id.is_empty() && !before.image_id.is_empty(),
        "container {container_name} is not ready for restart evidence"
    );
    let expected_restart_count = before
        .restart_count
        .checked_add(1)
        .context("container restart count overflowed")?;
    let pod_name = before_pod.metadata.name.clone();
    let pod_uid = before_pod.metadata.uid.clone();
    let before_container_id = before.container_id.clone();
    let before_image_id = before.image_id.clone();

    request_container_restart(context, namespace, &pod_name, container_name).await?;
    let restart_condition = format!(
        "--for=jsonpath={{.status.{}[?(@.name==\"{}\")].restartCount}}={expected_restart_count}",
        collection.field(),
        container_name
    );
    let timeout_argument = format!("--timeout={}s", timeout.as_secs());
    let pod_argument = format!("pod/{pod_name}");
    kubectl_checked(
        context,
        namespace,
        [
            "wait",
            restart_condition.as_str(),
            pod_argument.as_str(),
            timeout_argument.as_str(),
        ],
        timeout + Duration::from_secs(5),
    )
    .await
    .with_context(|| format!("waiting for {container_name} restart count"))?;
    let ready_condition = format!(
        "--for=jsonpath={{.status.{}[?(@.name==\"{}\")].ready}}=true",
        collection.field(),
        container_name
    );
    kubectl_checked(
        context,
        namespace,
        [
            "wait",
            ready_condition.as_str(),
            pod_argument.as_str(),
            timeout_argument.as_str(),
        ],
        timeout + Duration::from_secs(5),
    )
    .await
    .with_context(|| format!("waiting for {container_name} readiness"))?;

    let after_pod = current_uav_pod(context, namespace, workload, timeout).await?;
    ensure!(
        after_pod.metadata.name == pod_name && after_pod.metadata.uid == pod_uid,
        "container restart unexpectedly replaced the authoritative simulator pod"
    );
    let after = container_status(&after_pod, container_name, collection)?;
    ensure!(
        after.ready
            && after.restart_count == expected_restart_count
            && after.container_id != before_container_id
            && after.image_id == before_image_id,
        "container {container_name} did not restart once on the same immutable image"
    );
    if let Some(runtime_before) = runtime_before {
        let runtime_after =
            current_uav_pod(context, namespace, UavWorkload::Runtime, timeout).await?;
        ensure!(
            runtime_after.metadata.name == runtime_before.metadata.name
                && runtime_after.metadata.uid == runtime_before.metadata.uid,
            "UAV MCP restart replaced the independent authoritative simulator pod"
        );
        let simulator_before = container_status(
            &runtime_before,
            "isaac-sim",
            KubernetesStatusCollection::Application,
        )?;
        let simulator_after = container_status(
            &runtime_after,
            "isaac-sim",
            KubernetesStatusCollection::Application,
        )?;
        ensure!(
            simulator_after.container_id == simulator_before.container_id
                && simulator_after.restart_count == simulator_before.restart_count,
            "UAV MCP restart disturbed the authoritative simulator container"
        );
    }
    Ok(KubernetesRestartEvidence {
        pod_name,
        pod_uid,
        container_name: container_name.to_owned(),
        status_collection: collection.label(),
        restart_count_before: before.restart_count,
        restart_count_after: after.restart_count,
        container_id_before: before_container_id,
        container_id_after: after.container_id.clone(),
        image_id: before_image_id,
    })
}

async fn current_uav_pod(
    context: &str,
    namespace: &str,
    workload: UavWorkload,
    timeout: Duration,
) -> Result<KubernetesPod> {
    let selector_argument = format!("--selector={}", workload.selector());
    let output = kubectl_checked(
        context,
        namespace,
        ["get", "pods", selector_argument.as_str(), "--output=json"],
        timeout,
    )
    .await?;
    let pods: KubernetesPodList =
        serde_json::from_slice(&output).context("decoding authoritative simulator pod list")?;
    select_current_uav_pod(pods)
}

fn select_current_uav_pod(mut pods: KubernetesPodList) -> Result<KubernetesPod> {
    pods.items
        .retain(|pod| pod.metadata.deletion_timestamp.is_none());
    ensure!(
        pods.items.len() == 1,
        "restart acceptance requires exactly one non-terminating authoritative simulator pod, found {}",
        pods.items.len()
    );
    Ok(pods.items.remove(0))
}

fn container_status<'a>(
    pod: &'a KubernetesPod,
    container_name: &str,
    collection: KubernetesStatusCollection,
) -> Result<&'a KubernetesContainerStatus> {
    collection
        .statuses(pod)
        .iter()
        .find(|status| status.name == container_name)
        .with_context(|| {
            format!(
                "pod {} omitted {container_name} from {} statuses",
                pod.metadata.name,
                collection.label()
            )
        })
}

async fn request_container_restart(
    context: &str,
    namespace: &str,
    pod_name: &str,
    container_name: &str,
) -> Result<()> {
    let mut command = tokio::process::Command::new("kubectl");
    command
        .args([
            "--context",
            context,
            "--namespace",
            namespace,
            "exec",
            pod_name,
            "--container",
            container_name,
            "--",
            "/bin/sh",
            "-c",
            "set -- $(cat /proc/1/task/1/children); [ \"$#\" -eq 1 ]; kill -TERM \"$1\"",
        ])
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = tokio::time::timeout(
        Duration::from_secs(30),
        veoveo_testing_support::output_async(command, Duration::from_secs(30)),
    )
    .await
    .context("kubectl restart request timed out")??;
    let stderr = String::from_utf8_lossy(&output.stderr);
    ensure!(
        output.status.success()
            || stderr.contains("command terminated with exit code 143")
            || stderr.contains("command terminated with exit code 137"),
        "kubectl rejected the {container_name} restart request: {stderr}"
    );
    Ok(())
}

async fn kubectl_checked<const N: usize>(
    context: &str,
    namespace: &str,
    arguments: [&str; N],
    timeout: Duration,
) -> Result<Vec<u8>> {
    let mut command = tokio::process::Command::new("kubectl");
    command
        .args(["--context", context, "--namespace", namespace])
        .args(arguments)
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = tokio::time::timeout(
        timeout,
        veoveo_testing_support::output_async(command, timeout),
    )
    .await
    .with_context(|| format!("kubectl command exceeded {timeout:?}"))??;
    ensure!(
        output.status.success(),
        "kubectl command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct StateSource {
        body: std::sync::Arc<std::sync::Mutex<SimulationState>>,
        fail: std::sync::Arc<std::sync::atomic::AtomicBool>,
        reads: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        discovering: std::sync::Arc<tokio::sync::Notify>,
        release: Option<std::sync::Arc<tokio::sync::Notify>>,
    }
    impl StateSource {
        fn new() -> Self {
            Self {
                body: std::sync::Arc::new(std::sync::Mutex::new(
                    serde_json::from_str(include_str!(
                        "../../../flight/tests/fixtures/world-ready.json"
                    ))
                    .unwrap(),
                )),
                fail: Default::default(),
                reads: Default::default(),
                discovering: Default::default(),
                release: None,
            }
        }
    }
    impl rmcp::ServerHandler for StateSource {
        fn get_info(&self) -> rmcp::model::ServerConfig {
            rmcp::model::ServerConfig::new(
                rmcp::model::ServerCapabilities::builder()
                    .enable_resources()
                    .build(),
            )
        }
        async fn discover(
            &self,
            _: rmcp::service::RequestContext<rmcp::RoleServer>,
        ) -> std::result::Result<rmcp::model::DiscoverResult, rmcp::ErrorData> {
            self.discovering.notify_one();
            if let Some(release) = &self.release {
                release.notified().await;
            }
            Ok(rmcp::model::DiscoverResult::from_server_info(
                self.supported_protocol_versions().into_owned(),
                self.get_info(),
            ))
        }
        async fn read_resource(
            &self,
            request: rmcp::model::ReadResourceRequestParams,
            _: rmcp::service::RequestContext<rmcp::RoleServer>,
        ) -> std::result::Result<rmcp::model::ReadResourceResponse, rmcp::ErrorData> {
            use std::sync::atomic::Ordering;
            self.reads.fetch_add(1, Ordering::SeqCst);
            if self.fail.load(Ordering::SeqCst) {
                return Err(rmcp::ErrorData::internal_error(
                    "native current backend unavailable",
                    None,
                ));
            }
            let text = serde_json::to_string(&*self.body.lock().unwrap()).unwrap();
            Ok(
                rmcp::model::ReadResourceResult::new(vec![rmcp::model::ResourceContents::text(
                    text,
                    request.uri,
                )])
                .with_ttl_ms(60_000)
                .with_cache_scope(rmcp::model::CacheScope::Private)
                .into(),
            )
        }
    }

    #[tokio::test]
    async fn restart_current_read_bypasses_sdk_cache_and_refuses_wrong_session() -> Result<()> {
        use rmcp::{ClientServiceExt, ServiceExt};
        use std::sync::atomic::Ordering;
        tokio::time::timeout(Duration::from_secs(5), async {
            let source = StateSource::new();
            let handler = source.clone();
            let (server_io, client_io) = tokio::io::duplex(32_768);
            let server = tokio::spawn(async move { handler.serve(server_io).await });
            let client = ()
                .serve_with_lifecycle(
                    client_io,
                    rmcp::ClientLifecycleMode::Discover {
                        preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
                    },
                )
                .await?;
            let server = server.await??;
            let expected = source.body.lock().unwrap().session_id.clone();
            let uri = UavResource::Session(expected.clone()).to_uri()?;
            let original = client
                .peer()
                .read_resource(rmcp::model::ReadResourceRequestParams::new(uri.as_str()))
                .await?;
            assert_eq!(source.reads.load(Ordering::SeqCst), 1);
            source.body.lock().unwrap().world = None;
            let cached = client
                .peer()
                .read_resource(rmcp::model::ReadResourceRequestParams::new(uri.as_str()))
                .await?;
            assert_eq!(
                serde_json::to_value(original)?,
                serde_json::to_value(cached)?
            );
            assert_eq!(source.reads.load(Ordering::SeqCst), 1);
            let current = current_state(client.peer(), &uri, &expected).await?;
            assert!(current.world.is_none());
            source.fail.store(true, Ordering::SeqCst);
            assert!(current_state(client.peer(), &uri, &expected).await.is_err());
            source.fail.store(false, Ordering::SeqCst);
            source.body.lock().unwrap().session_id = SessionId::parse("another-session")?;
            assert!(current_state(client.peer(), &uri, &expected).await.is_err());
            assert_eq!(source.reads.load(Ordering::SeqCst), 4);
            client.cancel().await?;
            server.cancel().await?;
            Ok::<_, anyhow::Error>(())
        })
        .await?
    }

    #[tokio::test]
    async fn restart_cancelled_discovery_is_resumed_and_closed_by_registered_owner() -> Result<()> {
        use rmcp::{ClientServiceExt, ServiceExt};
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        tokio::time::timeout(Duration::from_secs(5), async {
            let mut source = StateSource::new();
            let release = Arc::new(tokio::sync::Notify::new());
            source.release = Some(release.clone());
            let discovering = source.discovering.clone();
            let (server_io, client_io) = tokio::io::duplex(32_768);
            let server = tokio::spawn(async move { source.serve(server_io).await });
            let closed = Arc::new(AtomicUsize::new(0));
            let count = closed.clone();
            let retained = Arc::new(std::sync::Mutex::new(None));
            let observed = retained.clone();
            let result: Result<()> = owner::run(async move {
                let caller = CallerClose {
                    acquisition: Some(Box::pin(async move {
                        let client = ().serve_with_lifecycle(client_io, rmcp::ClientLifecycleMode::Discover {
                            preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
                        }).await?;
                        Ok(AcquiredCaller { peer: client.peer().clone(), close: Box::pin(async move {
                            client.cancel().await?; count.fetch_add(1, Ordering::SeqCst); Ok(())
                        }) })
                    })),
                    ..Default::default()
                };
                let (caller, _registration) = retain_caller(caller)?;
                *observed.lock().unwrap() = Some(caller.clone());
                {
                    let mut guard = caller.lock().await;
                    let acquire = guard.acquire();
                    tokio::pin!(acquire);
                    tokio::select! {
                        _ = discovering.notified() => {},
                        result = &mut acquire => anyhow::bail!("unexpected discovery completion: {}", result.is_ok()),
                    }
                }
                release.notify_one();
                anyhow::bail!("injected observation cancellation during discovery")
            }).await;
            assert!(result.is_err());
            assert_eq!(closed.load(Ordering::SeqCst), 1);
            let caller = retained.lock().unwrap().take().unwrap();
            let caller = caller.lock().await;
            assert!(caller.acquisition.is_none() && caller.future.is_none() && !caller.failed);
            assert!(caller.deadline.is_some());
            drop(caller);
            server.await??.cancel().await?;

            // Refused registration must leave the inert acquisition entirely unpolled.
            let polls = Arc::new(AtomicUsize::new(0));
            let count = polls.clone();
            let refused = retain_caller(CallerClose {
                acquisition: Some(Box::pin(async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    anyhow::bail!("must not acquire without an owner")
                })), ..Default::default()
            });
            assert!(refused.is_err());
            assert_eq!(polls.load(Ordering::SeqCst), 0);
            Ok::<_, anyhow::Error>(())
        }).await?
    }

    #[test]
    fn paired_restart_refuses_changed_world_fleet_and_camera_optics() {
        let state: SimulationState = serde_json::from_str(include_str!(
            "../../../flight/tests/fixtures/world-ready.json"
        ))
        .unwrap();
        require_same_runtime(&state, &state).unwrap();
        let mut changed = state.clone();
        changed.world = None;
        assert!(require_same_runtime(&state, &changed).is_err());
        changed = state.clone();
        changed.vehicles.pop();
        assert!(require_same_runtime(&state, &changed).is_err());
        changed = state.clone();
        changed.live_cameras[0].far_clip_m += 1.0;
        assert!(require_same_runtime(&state, &changed).is_err());
        // Current health/frame counters are observations, not immutable camera configuration.
        changed = state.clone();
        changed.live_cameras[0].revision += 1;
        require_same_runtime(&state, &changed).unwrap();
    }

    #[tokio::test]
    async fn restart_caller_cleanup_retains_timeout_and_never_repolls_completed_failure() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let mut close = CallerClose {
            future: Some(Box::pin(async move {
                count.fetch_add(1, Ordering::SeqCst);
                anyhow::bail!("injected SDK close failure")
            })),
            deadline: Some(tokio::time::Instant::now() + Duration::from_secs(1)),
            failed: false,
            ..Default::default()
        };
        assert!(close.finish().await.is_err());
        assert!(close.finish().await.is_err());
        assert!(close.future.is_none());
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        let end = tokio::time::Instant::now();
        let mut pending = CallerClose {
            future: Some(Box::pin(std::future::pending())),
            deadline: Some(end),
            failed: false,
            ..Default::default()
        };
        assert!(pending.finish().await.is_err());
        assert!(pending.future.is_some());
        assert_eq!(pending.deadline, Some(end));
        // Even a later successful completion cannot settle the expired original cap.
        pending.future = Some(Box::pin(async { Ok(()) }));
        assert!(pending.finish().await.is_err());
        assert!(pending.future.is_none());
        assert_eq!(pending.deadline, Some(end));
    }

    #[test]
    fn only_declared_camera_warmup_can_wait_for_startup() {
        let mut state: SimulationState = serde_json::from_str(include_str!(
            "../../../flight/tests/fixtures/world-ready.json"
        ))
        .unwrap();
        state.lifecycle = SimulationLifecycle::Running;
        state.live_cameras[0].health = LiveCameraHealth::Warming;
        assert_eq!(
            live_camera_health(&state, "follow").unwrap(),
            LiveCameraHealth::Warming
        );
        assert!(require_running_live_camera(&state, "follow").is_err());
        for health in [
            LiveCameraHealth::Healthy,
            LiveCameraHealth::Stale,
            LiveCameraHealth::Failed,
        ] {
            state.live_cameras[0].health = health;
            assert_eq!(live_camera_health(&state, "follow").unwrap(), health);
            assert_eq!(
                require_running_live_camera(&state, "follow").is_ok(),
                health == LiveCameraHealth::Healthy
            );
        }
        let mut unknown = serde_json::to_value(&state).unwrap();
        unknown["liveCameras"][0]["health"] = "unknown".into();
        assert!(serde_json::from_value::<SimulationState>(unknown).is_err());
        assert!(live_camera_health(&state, "missing").is_err());
        state.live_cameras[0].health = LiveCameraHealth::Healthy;
        state.lifecycle = SimulationLifecycle::Failed;
        assert!(live_camera_health(&state, "follow").is_err());
    }

    #[test]
    fn current_pod_selection_ignores_a_terminating_status_without_image_id() {
        let pods: KubernetesPodList = serde_json::from_value(serde_json::json!({
            "items": [
                {
                    "metadata": {
                        "name": "uav-sim-old",
                        "uid": "old",
                        "deletionTimestamp": "2026-08-08T04:00:00Z"
                    },
                    "status": {
                        "phase": "Running",
                        "containerStatuses": [{
                            "name": "uav-sim-mcp",
                            "ready": false,
                            "restartCount": 0
                        }]
                    }
                },
                {
                    "metadata": {"name": "uav-sim-current", "uid": "current"},
                    "status": {
                        "phase": "Running",
                        "containerStatuses": [{
                            "name": "uav-sim-mcp",
                            "ready": true,
                            "restartCount": 0,
                            "imageID": "registry.example/uav-sim-mcp@sha256:abc",
                            "containerID": "containerd://current"
                        }]
                    }
                }
            ]
        }))
        .unwrap();

        let selected = select_current_uav_pod(pods).unwrap();
        assert_eq!(selected.metadata.name, "uav-sim-current");
        let status = container_status(
            &selected,
            "uav-sim-mcp",
            KubernetesStatusCollection::Application,
        )
        .unwrap();
        assert_eq!(status.image_id, "registry.example/uav-sim-mcp@sha256:abc");
        assert_eq!(status.container_id, "containerd://current");
    }
}
