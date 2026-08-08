use std::{fs, path::Path, process::Stdio, time::Duration};

use anyhow::{Context as _, Result, ensure};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    FocusedScenario, OperatorClient, PRIMARY_CAMERA_ID,
    browser::{
        ConsoleLiveRestartEvidence, capture_console_live_app_restart, preflight_console_live_app,
    },
    gateway_token, git_revision, json_string, simulation_state,
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
    lifecycle_after_mcp_restart: String,
    lifecycle_after_simulator_restart: String,
    mcp_container: KubernetesRestartEvidence,
    simulator_container: KubernetesRestartEvidence,
    mcp: ConsoleLiveRestartEvidence,
    simulator: ConsoleLiveRestartEvidence,
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
    #[serde(default)]
    init_container_statuses: Vec<KubernetesContainerStatus>,
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
    RestartableInit,
}

impl KubernetesStatusCollection {
    fn field(self) -> &'static str {
        match self {
            Self::Application => "containerStatuses",
            Self::RestartableInit => "initContainerStatuses",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Application => "application",
            Self::RestartableInit => "restartable_init",
        }
    }

    fn statuses(self, pod: &KubernetesPod) -> &[KubernetesContainerStatus] {
        match self {
            Self::Application => &pod.status.container_statuses,
            Self::RestartableInit => &pod.status.init_container_statuses,
        }
    }
}

pub(super) struct RestartVerification<'a> {
    pub(super) conformance: &'a Path,
    pub(super) scenario_path: &'a Path,
    pub(super) context: &'a str,
    pub(super) namespace: &'a str,
    pub(super) public_base_url: &'a str,
    pub(super) chrome_cdp_url: &'a str,
    pub(super) restart_timeout: Duration,
    pub(super) evidence_root: &'a Path,
}

pub(super) async fn verify_live_view_restarts(config: RestartVerification<'_>) -> Result<()> {
    let RestartVerification {
        conformance,
        scenario_path,
        context,
        namespace,
        public_base_url,
        chrome_cdp_url,
        restart_timeout,
        evidence_root,
    } = config;
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
        !restart_timeout.is_zero(),
        "container restart timeout must be positive"
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
    let token = gateway_token(conformance, public_base_url).await?;
    let operator = OperatorClient {
        conformance,
        base: public_base_url,
        token: &token,
    };
    let initial_state = simulation_state(&operator, &scenario.session_id).await?;
    require_running_live_camera(&initial_state, PRIMARY_CAMERA_ID)?;

    let source_revision = git_revision()?;
    let run_id = uuid::Uuid::now_v7().to_string();
    let evidence_directory = evidence_root.join(&source_revision).join(&run_id);
    fs::create_dir_all(&evidence_directory).with_context(|| {
        format!(
            "creating focused restart evidence directory {}",
            evidence_directory.display()
        )
    })?;

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
    let lifecycle_after_mcp_restart = json_string(&state_after_mcp, "/lifecycle")?.to_owned();

    let (simulator, simulator_container) = capture_console_live_app_restart(
        chrome_cdp_url,
        public_base_url,
        PRIMARY_CAMERA_ID,
        "isaac-sim",
        &evidence_directory.join("uav-live-view-simulator-restart.png"),
        restart_timeout,
        || async {
            restart_kubernetes_container(
                context,
                namespace,
                "isaac-sim",
                KubernetesStatusCollection::RestartableInit,
                restart_timeout,
            )
            .await
        },
    )
    .await?;
    let final_state = simulation_state(&operator, &scenario.session_id).await?;
    require_running_live_camera(&final_state, PRIMARY_CAMERA_ID)?;
    let lifecycle_after_simulator_restart = json_string(&final_state, "/lifecycle")?.to_owned();

    let evidence = RestartAcceptanceEvidence {
        schema: "veoveo.io/uav-live-view-restart-evidence/v1",
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
    fs::write(&manifest, serde_json::to_vec_pretty(&evidence)?)
        .with_context(|| format!("writing restart evidence {}", manifest.display()))?;
    println!(
        "Focused native live-view restart acceptance passed. Evidence: {}",
        manifest.display()
    );
    Ok(())
}

fn require_running_live_camera(state: &Value, camera_id: &str) -> Result<()> {
    ensure!(
        json_string(state, "/lifecycle")? == "running",
        "restart acceptance requires the authoritative simulation to be running: {state}"
    );
    let camera = state
        .get("live_cameras")
        .and_then(Value::as_array)
        .and_then(|cameras| {
            cameras
                .iter()
                .find(|camera| camera.get("cameraId").and_then(Value::as_str) == Some(camera_id))
        })
        .with_context(|| format!("authoritative simulator omitted live camera {camera_id}"))?;
    ensure!(
        camera.get("health").and_then(Value::as_str) == Some("healthy"),
        "authoritative live camera {camera_id} is not healthy: {camera}"
    );
    Ok(())
}

async fn restart_kubernetes_container(
    context: &str,
    namespace: &str,
    container_name: &str,
    collection: KubernetesStatusCollection,
    timeout: Duration,
) -> Result<KubernetesRestartEvidence> {
    let before_pod = current_uav_pod(context, namespace, timeout).await?;
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

    let after_pod = current_uav_pod(context, namespace, timeout).await?;
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
    timeout: Duration,
) -> Result<KubernetesPod> {
    let output = kubectl_checked(
        context,
        namespace,
        [
            "get",
            "pods",
            "--selector=app.kubernetes.io/name=uav-sim,app.kubernetes.io/component=uav-sim",
            "--output=json",
        ],
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
    let output = tokio::time::timeout(Duration::from_secs(30), command.output())
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
    let output = tokio::time::timeout(timeout, command.output())
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
