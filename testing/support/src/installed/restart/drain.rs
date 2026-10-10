//! Selected process drain: Kubernetes watches qualify exit, never deletion alone.
use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct DrainProfile {
    pub(super) container: String,
    pub(super) deadline: Duration,
    resource: DrainResourceExpectation,
}
#[derive(Clone, Copy, Debug)]
enum DrainResourceExpectation {
    Server,
    NvidiaGpu,
}
impl DrainProfile {
    /// The selected NVIDIA container must request and limit at least one GPU.
    pub fn nvidia(container: &str, deadline: Duration) -> Result<Self> {
        let mut profile = Self::server(container, deadline)?;
        profile.resource = DrainResourceExpectation::NvidiaGpu;
        Ok(profile)
    }
    /// Ordinary server-process drain makes no GPU execution claim.
    pub fn server(container: &str, deadline: Duration) -> Result<Self> {
        name(container)?;
        ensure!(
            !deadline.is_zero() && deadline <= Duration::from_secs(300),
            "drain deadline must be between one nanosecond and 300 seconds"
        );
        Ok(Self {
            container: container.into(),
            deadline,
            resource: DrainResourceExpectation::Server,
        })
    }
}
#[derive(Clone)]
pub struct SelectedDrainTarget {
    pub(super) namespace: String,
    pub(super) namespace_uid: Uuid,
    pub(super) deployment: String,
    pub(super) deployment_uid: Uuid,
    pub(super) deployment_version: String,
    pub(super) generation: u64,
    pub(super) pod: String,
    pub(super) pod_uid: Uuid,
    pub(super) pod_version: String,
    pub(super) container_id: String,
    pub(super) restart_count: u32,
    pub(super) profile: DrainProfile,
    pub(super) grace: u64,
    pub(super) annotations: BTreeMap<String, String>,
}
/// Immutable, secret-free observation of the selected lifecycle identities.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedDrainIdentity {
    namespace: String,
    namespace_uid: Uuid,
    deployment: String,
    deployment_uid: Uuid,
    deployment_resource_version: String,
    pod: String,
    pod_uid: Uuid,
    pod_resource_version: String,
    container: String,
    grace_seconds: u64,
    container_instance_sha256: String,
    restart_count: u32,
}
impl SelectedDrainIdentity {
    pub fn namespace_uid(&self) -> Uuid {
        self.namespace_uid
    }
    pub fn deployment_uid(&self) -> Uuid {
        self.deployment_uid
    }
    pub fn pod_uid(&self) -> Uuid {
        self.pod_uid
    }
    pub fn deployment_resource_version(&self) -> &str {
        &self.deployment_resource_version
    }
    pub fn pod_resource_version(&self) -> &str {
        &self.pod_resource_version
    }
}
impl SelectedDrainTarget {
    pub fn identity(&self) -> SelectedDrainIdentity {
        SelectedDrainIdentity {
            namespace: self.namespace.clone(),
            namespace_uid: self.namespace_uid,
            deployment: self.deployment.clone(),
            deployment_uid: self.deployment_uid,
            deployment_resource_version: self.deployment_version.clone(),
            pod: self.pod.clone(),
            pod_uid: self.pod_uid,
            pod_resource_version: self.pod_version.clone(),
            container: self.profile.container.clone(),
            grace_seconds: self.grace,
            container_instance_sha256: instance_digest(&self.container_id),
            restart_count: self.restart_count,
        }
    }
    pub fn pod_name(&self) -> &str {
        &self.pod
    }
    pub fn container_name(&self) -> &str {
        &self.profile.container
    }
    pub fn grace_seconds(&self) -> u64 {
        self.grace
    }
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DrainReceipt {
    pub namespace_uid: Uuid,
    pub deployment_uid: Uuid,
    pub old_pod_uid: Uuid,
    pub container_name: String,
    pub exit_code: i32,
    pub finished_at: DateTime<Utc>,
    pub deletion_timestamp: DateTime<Utc>,
    pub grace_seconds: u64,
    pub replacement_generation: u64,
    pub container_instance_sha256: String,
    pub restart_count: u32,
}
pub(super) fn instance_digest(value: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(value.as_bytes()))
}
pub(super) fn name(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 253
            && value
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'-' | b'.'))
            && value.starts_with(|c: char| c.is_ascii_alphanumeric())
            && value.ends_with(|c: char| c.is_ascii_alphanumeric()),
        "invalid selected Kubernetes name"
    );
    Ok(())
}
pub(super) fn version(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.')),
        "invalid selected Kubernetes resource version"
    );
    Ok(())
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Metadata {
    pub name: String,
    pub namespace: Option<String>,
    pub uid: Uuid,
    pub resource_version: String,
    pub deletion_timestamp: Option<DateTime<Utc>>,
    pub deletion_grace_period_seconds: Option<u64>,
    #[serde(default)]
    pub owner_references: Vec<Owner>,
}
#[derive(Deserialize)]
pub(super) struct Owner {
    pub uid: Uuid,
    pub kind: String,
    pub name: String,
    pub controller: Option<bool>,
}
#[derive(Deserialize)]
pub(super) struct Namespace {
    pub metadata: Metadata,
}
#[derive(Deserialize)]
pub(super) struct ReplicaSet {
    pub metadata: Metadata,
}
#[derive(Deserialize)]
pub(super) struct Pod {
    pub metadata: Metadata,
    pub spec: PodSpec,
    pub status: PodStatus,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PodSpec {
    pub termination_grace_period_seconds: Option<u64>,
    pub containers: Vec<Container>,
}
#[derive(Deserialize)]
pub(super) struct Container {
    pub name: String,
    pub resources: Resources,
}
#[derive(Deserialize)]
pub(super) struct Resources {
    #[serde(default)]
    pub requests: BTreeMap<String, String>,
    #[serde(default)]
    pub limits: BTreeMap<String, String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PodStatus {
    #[serde(default)]
    pub container_statuses: Vec<ContainerStatus>,
    #[serde(default)]
    pub conditions: Vec<Condition>,
}
#[derive(Deserialize)]
pub(super) struct Condition {
    pub r#type: String,
    pub status: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ContainerStatus {
    pub name: String,
    #[serde(rename = "containerID")]
    #[serde(default)]
    pub container_id: String,
    #[serde(default, rename = "imageID")]
    pub image_id: String,
    #[serde(default)]
    pub last_state: ContainerState,
    pub restart_count: u32,
    pub state: ContainerState,
    pub ready: bool,
}
#[derive(Default, Deserialize)]
pub(super) struct ContainerState {
    pub terminated: Option<Terminated>,
    pub running: Option<Running>,
    pub waiting: Option<serde::de::IgnoredAny>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Running {
    pub started_at: DateTime<Utc>,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Terminated {
    pub exit_code: i32,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub signal: Option<i32>,
    pub finished_at: DateTime<Utc>,
    #[serde(rename = "containerID")]
    pub container_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(tag = "type", content = "object")]
pub(super) enum WatchEvent {
    #[serde(rename = "ADDED")]
    Added(Pod),
    #[serde(rename = "MODIFIED")]
    Modified(Pod),
    #[serde(rename = "DELETED")]
    Deleted(Pod),
    #[serde(rename = "ERROR")]
    Error(serde::de::IgnoredAny),
}
impl Pod {
    pub(super) fn selected_status(&self, profile: &DrainProfile) -> Result<&ContainerStatus> {
        let mut matching = self
            .status
            .container_statuses
            .iter()
            .filter(|status| status.name == profile.container);
        let status = matching
            .next()
            .context("selected container status is absent")?;
        ensure!(
            matching.next().is_none()
                && !status.container_id.is_empty()
                && status.container_id.len() <= 1024,
            "selected container instance is ambiguous or absent"
        );
        Ok(status)
    }
    pub(super) fn admit(&self, namespace: &str, profile: &DrainProfile) -> Result<u64> {
        ensure!(!self.metadata.uid.is_nil(), "selected Pod UID is absent");
        name(&self.metadata.name)?;
        version(&self.metadata.resource_version)?;
        ensure!(
            self.metadata.namespace.as_deref() == Some(namespace)
                && self.metadata.deletion_timestamp.is_none(),
            "selected pod namespace or deletion state differs"
        );
        let grace = self
            .spec
            .termination_grace_period_seconds
            .context("selected pod has no declared termination grace")?;
        ensure!(
            grace > 0 && profile.deadline <= Duration::from_secs(grace),
            "drain deadline exceeds configured termination grace"
        );
        let container = self
            .spec
            .containers
            .iter()
            .find(|c| c.name == profile.container)
            .context("selected container absent")?;
        if matches!(profile.resource, DrainResourceExpectation::NvidiaGpu) {
            for quantities in [&container.resources.requests, &container.resources.limits] {
                ensure!(
                    quantities
                        .get("nvidia.com/gpu")
                        .and_then(|n| n.parse::<u32>().ok())
                        .is_some_and(|n| n > 0),
                    "selected container must request and limit NVIDIA hardware"
                );
            }
        }
        let admitted_status = self.selected_status(profile)?;
        ensure!(
            admitted_status.state.waiting.is_none(),
            "selected old container is waiting"
        );
        ensure!(
            self.status
                .conditions
                .iter()
                .any(|c| c.r#type == "Ready" && c.status == "True")
                && self
                    .status
                    .container_statuses
                    .iter()
                    .any(|c| c.name == profile.container
                        && c.ready
                        && c.state.terminated.is_none()
                        && c.state
                            .running
                            .as_ref()
                            .is_some_and(|running| running.started_at <= Utc::now())),
            "selected old container is not ready and running"
        );
        Ok(grace)
    }
}
pub(super) struct DrainObservation {
    selected: SelectedDrainTarget,
    dispatched_at: Option<DateTime<Utc>>,
    versions: BTreeSet<String>,
    deletion: Option<DateTime<Utc>>,
    terminal_seen: bool,
    qualified: Option<(Terminated, DateTime<Utc>)>,
}
impl DrainObservation {
    pub(super) fn new(selected: SelectedDrainTarget) -> Self {
        Self {
            versions: BTreeSet::from([selected.pod_version.clone()]),
            selected,
            dispatched_at: None,
            deletion: None,
            terminal_seen: false,
            qualified: None,
        }
    }
    pub(super) fn dispatched(&mut self, at: DateTime<Utc>) {
        self.dispatched_at = Some(at);
    }
    pub(super) fn initial(&self, event: WatchEvent) -> Result<()> {
        let WatchEvent::Added(pod) = event else {
            anyhow::bail!("selected old pod watch did not establish initial state")
        };
        self.initial_pod(&pod)
    }
    pub(super) fn initial_pod(&self, pod: &Pod) -> Result<()> {
        self.identity(pod)?;
        ensure!(
            pod.metadata.resource_version == self.selected.pod_version,
            "old pod changed before watch admission; select it again before effects"
        );
        pod.admit(&self.selected.namespace, &self.selected.profile)?;
        Ok(())
    }
    fn identity(&self, pod: &Pod) -> Result<()> {
        ensure!(
            pod.metadata.uid == self.selected.pod_uid
                && pod.metadata.name == self.selected.pod
                && pod.metadata.namespace.as_deref() == Some(&self.selected.namespace),
            "old pod watch observed a different resource"
        );
        let status = pod.selected_status(&self.selected.profile)?;
        ensure!(
            status.container_id == self.selected.container_id
                && status.restart_count == self.selected.restart_count,
            "selected container instance or restart count changed"
        );
        Ok(())
    }
    pub(super) fn observe(&mut self, event: WatchEvent) -> Result<bool> {
        let dispatched = self
            .dispatched_at
            .context("old Pod observation precedes restart dispatch")?;
        let deleted = matches!(&event, WatchEvent::Deleted(_));
        let pod = match event {
            WatchEvent::Modified(pod) | WatchEvent::Deleted(pod) => pod,
            _ => anyhow::bail!("old pod watch gap or unexpected event"),
        };
        self.observe_pod(&pod, deleted, dispatched)
    }
    pub(super) fn observe_pod(
        &mut self,
        pod: &Pod,
        deleted: bool,
        dispatched: DateTime<Utc>,
    ) -> Result<bool> {
        self.identity(pod)?;
        version(&pod.metadata.resource_version)?;
        ensure!(
            self.versions.insert(pod.metadata.resource_version.clone()),
            "old Pod watch repeated an admitted resource version"
        );
        let status = pod.selected_status(&self.selected.profile)?;
        ensure!(
            status.state.waiting.is_none(),
            "selected container is waiting; drain is unqualified"
        );
        ensure!(
            !(status.state.running.is_some() && status.state.terminated.is_some()),
            "selected container has contradictory process states"
        );
        ensure!(
            status.state.running.is_some() || status.state.terminated.is_some(),
            "selected container process state is absent"
        );
        ensure!(
            !self.terminal_seen || status.state.terminated.is_some(),
            "selected container regressed from terminated state"
        );
        ensure!(
            self.deletion.is_none() || pod.metadata.deletion_timestamp == self.deletion,
            "old Pod deletion metadata disappeared or changed"
        );
        if let Some(deletion) = pod.metadata.deletion_timestamp {
            self.deletion = Some(deletion);
        }
        if let Some(terminated) = &status.state.terminated {
            self.terminal_seen = true;
            ensure!(
                terminated.exit_code == 0
                    && terminated
                        .container_id
                        .as_deref()
                        .is_none_or(|id| id == self.selected.container_id),
                "selected old container termination differs from admitted instance"
            );
            if let Some(deletion) = pod.metadata.deletion_timestamp {
                let seconds = pod
                    .metadata
                    .deletion_grace_period_seconds
                    .context("old pod deletion has no declared grace")?;
                ensure!(
                    seconds > 0 && seconds <= self.selected.grace,
                    "old pod deletion grace is missing or forced"
                );
                let start = deletion - chrono::Duration::seconds(i64::try_from(seconds)?);
                ensure!(
                    start >= dispatched
                        && terminated.finished_at >= start
                        && terminated.finished_at <= deletion
                        && terminated.finished_at
                            <= start + chrono::Duration::from_std(self.selected.profile.deadline)?,
                    "old container exit timestamp is outside the admitted deletion window"
                );
                self.qualified = Some((terminated.clone(), deletion));
                return Ok(true);
            }
        }
        ensure!(
            !deleted,
            "old Pod deleted without a full same-instance terminal snapshot"
        );
        Ok(false)
    }
    pub(super) fn qualified(&self) -> Option<(&Terminated, DateTime<Utc>)> {
        self.qualified
            .as_ref()
            .map(|(exit, deletion)| (exit, *deletion))
    }
    pub(super) fn receipt(self, generation: u64) -> Result<DrainReceipt> {
        let (terminated, deletion) = self
            .qualified
            .context("full same-instance termination/deletion evidence missing")?;
        Ok(DrainReceipt {
            namespace_uid: self.selected.namespace_uid,
            deployment_uid: self.selected.deployment_uid,
            old_pod_uid: self.selected.pod_uid,
            container_name: self.selected.profile.container,
            exit_code: terminated.exit_code,
            finished_at: terminated.finished_at,
            deletion_timestamp: deletion,
            grace_seconds: self.selected.grace,
            replacement_generation: generation,
            container_instance_sha256: instance_digest(&self.selected.container_id),
            restart_count: self.selected.restart_count,
        })
    }
}

/// Native watch output is bounded and parsed without exposing provider payloads.
pub(super) struct PodWatch {
    _child: crate::AsyncChild,
    stdout: tokio::process::ChildStdout,
    pending: Vec<u8>,
    bytes: usize,
}
impl PodWatch {
    pub(super) fn start(context: &str, namespace: &str, pod: &str) -> Result<Self> {
        let mut endpoint = url::Url::parse("https://kubernetes.invalid/")?;
        endpoint
            .path_segments_mut()
            .map_err(|_| anyhow::anyhow!("invalid Kubernetes API origin"))?
            .extend(["api", "v1", "namespaces", namespace, "pods"]);
        endpoint
            .query_pairs_mut()
            .append_pair("watch", "true")
            .append_pair("resourceVersion", "0")
            .append_pair("fieldSelector", &format!("metadata.name={pod}"));
        let raw_path = &endpoint[url::Position::BeforePath..];
        let mut command = tokio::process::Command::new("kubectl");
        command
            .args([
                "--context",
                context,
                "--namespace",
                namespace,
                "get",
                "--raw",
                raw_path,
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        Self::from_child(crate::spawn_async(command)?)
    }
    pub(super) fn from_child(mut child: crate::AsyncChild) -> Result<Self> {
        let stdout = child
            .stdout
            .take()
            .context("selected Pod watch output unavailable")?;
        Ok(Self {
            _child: child,
            stdout,
            pending: Vec::new(),
            bytes: 0,
        })
    }
    pub(super) async fn close(mut self) -> bool {
        let _ = self._child.start_kill();
        matches!(
            tokio::time::timeout(Duration::from_secs(5), self._child.wait()).await,
            Ok(Ok(_))
        )
    }
    pub(super) async fn close_until(mut self, deadline: std::time::Instant) -> Result<()> {
        self._child.cleanup_until(deadline).await?;
        Ok(())
    }
    pub(super) async fn next(&mut self) -> Result<WatchEvent> {
        use tokio::io::AsyncReadExt;
        loop {
            let mut events =
                serde_json::Deserializer::from_slice(&self.pending).into_iter::<WatchEvent>();
            match events.next() {
                Some(Ok(event)) => {
                    let used = events.byte_offset();
                    self.pending.drain(..used);
                    return Ok(event);
                }
                Some(Err(error)) if !error.is_eof() => anyhow::bail!(
                    "selected Pod watch returned an invalid event; drain is unqualified"
                ),
                _ => (),
            }
            let mut buffer = [0_u8; 8192];
            let count = self
                .stdout
                .read(&mut buffer)
                .await
                .context("read selected Pod watch")?;
            ensure!(
                count > 0,
                "selected Pod watch ended before terminal evidence; drain is unqualified"
            );
            self.bytes += count;
            ensure!(
                self.bytes <= 2 * 1024 * 1024,
                "selected Pod watch exceeded two MiB"
            );
            self.pending.extend_from_slice(&buffer[..count]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn selected() -> SelectedDrainTarget {
        SelectedDrainTarget {
            namespace: "fixture".into(),
            namespace_uid: Uuid::from_u128(1),
            deployment: "view".into(),
            deployment_uid: Uuid::from_u128(2),
            deployment_version: "11".into(),
            generation: 1,
            pod: "view-old".into(),
            pod_uid: Uuid::from_u128(3),
            pod_version: "12".into(),
            container_id: "containerd://selected-instance".into(),
            restart_count: 0,
            profile: DrainProfile::nvidia("server", Duration::from_secs(20)).unwrap(),
            grace: 30,
            annotations: BTreeMap::new(),
        }
    }
    fn pod(deletion: bool, terminated: bool) -> Pod {
        serde_json::from_value(json!({
            "metadata":{"name":"view-old","namespace":"fixture","uid":Uuid::from_u128(3),"resourceVersion":"opaque-update","deletionTimestamp":if deletion { Some("2026-10-07T00:00:30Z") } else { None },"deletionGracePeriodSeconds":if deletion {Some(30)} else {None}},
            "spec":{"terminationGracePeriodSeconds":30,"containers":[{"name":"server","resources":{"requests":{"nvidia.com/gpu":"1"},"limits":{"nvidia.com/gpu":"1"}}}]},
            "status":{"conditions":[{"type":"Ready","status":"True"}],"containerStatuses":[{"name":"server","containerID":"containerd://selected-instance","restartCount":0,"ready":!terminated,"state":{"running":if !terminated {Some(json!({"startedAt":"2026-10-06T00:00:00Z"}))} else {None}, "terminated":if terminated {Some(json!({"exitCode":0,"finishedAt":"2026-10-07T00:00:10+00:00"}))} else {None}}}]}
        })).unwrap()
    }
    #[test]
    fn identity_receipt_excludes_private_annotations_and_cannot_change_target() {
        let mut target = selected();
        target
            .annotations
            .insert("private-annotation".into(), "secret-sentinel".into());
        let identity = target.identity();
        assert_eq!(identity.pod_uid(), Uuid::from_u128(3));
        for representation in [
            format!("{identity:?}"),
            serde_json::to_string(&identity).unwrap(),
        ] {
            assert!(!representation.contains("private-annotation"));
            assert!(!representation.contains("secret-sentinel"));
        }
        assert_eq!(target.pod_version, "12");
    }
    #[test]
    fn dropped_watch_cancels_owned_native_process_without_exposing_output() -> Result<()> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(async {
            let directory = tempfile::tempdir()?;
            let marker = directory.path().join("late-effect");
            let mut command = tokio::process::Command::new("sh");
            command
                .args([
                    "-c",
                    "sleep 1; echo secret-sentinel; touch \"$1\"",
                    "fixture",
                ])
                .arg(&marker)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(true);
            let mut watch = PodWatch::from_child(crate::spawn_async(command)?)?;
            assert!(
                tokio::time::timeout(Duration::from_millis(50), watch.next())
                    .await
                    .is_err()
            );
            drop(watch);
            tokio::time::sleep(Duration::from_millis(1200)).await;
            ensure!(!marker.exists(), "cancelled watch left its child alive");
            Ok::<_, anyhow::Error>(())
        })
    }
    #[test]
    fn admission_refuses_wrong_resource_not_ready_missing_gpu_and_excess_deadline() {
        let selected = selected();
        let observation = DrainObservation::new(selected.clone());
        let mut initial = pod(false, false);
        initial.metadata.resource_version = "12".into();
        observation.initial(WatchEvent::Added(initial)).unwrap();
        let mut wrong = pod(false, false);
        wrong.metadata.uid = Uuid::from_u128(99);
        assert!(observation.initial(WatchEvent::Added(wrong)).is_err());
        let mut stale = pod(false, false);
        stale.metadata.resource_version = "13".into();
        assert!(observation.initial(WatchEvent::Added(stale)).is_err());
        let mut not_ready = pod(false, false);
        not_ready.status.container_statuses[0].ready = false;
        assert!(not_ready.admit("fixture", &selected.profile).is_err());
        let mut no_gpu = pod(false, false);
        no_gpu.spec.containers[0].resources.requests.clear();
        assert!(no_gpu.admit("fixture", &selected.profile).is_err());
        assert!(
            pod(false, false)
                .admit(
                    "fixture",
                    &DrainProfile::nvidia("server", Duration::from_secs(31)).unwrap()
                )
                .is_err()
        );
    }
    #[test]
    fn contradictory_full_snapshots_cannot_merge_into_process_exit() {
        let mut observation = DrainObservation::new(selected());
        observation.dispatched("2026-10-07T00:00:00Z".parse().unwrap());
        assert!(
            !observation
                .observe(WatchEvent::Modified(pod(false, true)))
                .unwrap()
        );
        let mut next = pod(true, false);
        next.metadata.resource_version = "contradictory-next".into();
        assert!(
            observation.observe(WatchEvent::Modified(next)).is_err(),
            "a later running full snapshot invalidates retained terminal evidence"
        );
        let mut observation = DrainObservation::new(selected());
        observation.dispatched("2026-10-07T00:00:00Z".parse().unwrap());
        assert!(
            !observation
                .observe(WatchEvent::Modified(pod(true, false)))
                .unwrap()
        );
        let mut next = pod(false, true);
        next.metadata.resource_version = "contradictory-next".into();
        assert!(
            observation.observe(WatchEvent::Modified(next)).is_err(),
            "deletion metadata cannot disappear across full snapshots"
        );
    }
    #[test]
    fn changed_instance_restart_count_and_opaque_version_replay_are_refused() {
        for change in 0..4 {
            let mut observation = DrainObservation::new(selected());
            observation.dispatched("2026-10-07T00:00:00Z".parse().unwrap());
            let mut first = pod(true, false);
            first.metadata.resource_version = "opaque-a".into();
            assert!(!observation.observe(WatchEvent::Modified(first)).unwrap());
            let mut next = pod(true, true);
            next.metadata.resource_version = "opaque-b".into();
            match change {
                0 => {
                    next.status.container_statuses[0].container_id =
                        "secret-different-instance".into()
                }
                1 => next.status.container_statuses[0].restart_count = 1,
                2 => {
                    next.status.container_statuses[0]
                        .state
                        .terminated
                        .as_mut()
                        .unwrap()
                        .container_id = Some("secret-different-instance".into())
                }
                _ => next.metadata.resource_version = "opaque-a".into(),
            }
            let diagnostic = format!(
                "{:#}",
                observation.observe(WatchEvent::Modified(next)).unwrap_err()
            );
            assert!(!diagnostic.contains("secret-different-instance"));
        }
    }
    #[test]
    fn same_instance_running_with_deletion_then_full_terminal_snapshot_qualifies() {
        let mut observation = DrainObservation::new(selected());
        observation.dispatched("2026-10-07T00:00:00Z".parse().unwrap());
        let mut first = pod(true, false);
        first.metadata.resource_version = "opaque-draining".into();
        assert!(!observation.observe(WatchEvent::Modified(first)).unwrap());
        let mut final_state = pod(true, true);
        final_state.metadata.resource_version = "opaque-exit".into();
        assert!(
            observation
                .observe(WatchEvent::Modified(final_state))
                .unwrap()
        );
        let receipt = observation.receipt(2).unwrap();
        assert_eq!(receipt.exit_code, 0);
        assert_eq!(
            receipt.finished_at.to_rfc3339(),
            "2026-10-07T00:00:10+00:00"
        );
    }
    #[test]
    fn deleted_ready_pod_missing_terminal_and_watch_error_cannot_prove_drain() {
        let mut observation = DrainObservation::new(selected());
        observation.dispatched("2026-10-07T00:00:00Z".parse().unwrap());
        assert!(
            observation
                .observe(WatchEvent::Deleted(pod(true, false)))
                .is_err()
        );
        assert!(observation.receipt(2).is_err());
        let mut observation = DrainObservation::new(selected());
        observation.dispatched("2026-10-07T00:00:00Z".parse().unwrap());
        let error: WatchEvent =
            serde_json::from_value(json!({"type":"ERROR","object":{"message":"secret-sentinel"}}))
                .unwrap();
        let diagnostic = format!("{:#}", observation.observe(error).unwrap_err());
        assert!(!diagnostic.contains("secret-sentinel"));
    }
    #[test]
    fn grace_overrun_and_forced_exit_are_unqualified() {
        for (time, exit) in [
            ("2026-10-07T00:00:31Z", 0),
            ("2026-10-07T00:00:21Z", 0),
            ("2026-10-07T00:00:10Z", 137),
            ("2026-10-06T23:59:59Z", 0),
        ] {
            let mut observation = DrainObservation::new(selected());
            observation.dispatched("2026-10-07T00:00:00Z".parse().unwrap());
            let mut event = pod(true, true);
            let termination = event.status.container_statuses[0]
                .state
                .terminated
                .as_mut()
                .unwrap();
            termination.finished_at = time.parse().unwrap();
            termination.exit_code = exit;
            assert!(observation.observe(WatchEvent::Modified(event)).is_err());
        }
    }
}
