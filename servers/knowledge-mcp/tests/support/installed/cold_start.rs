//! Observation-only startup: an operator launches only after the persisted watch handshake.
use super::*;
use tokio::io::AsyncReadExt;
use veoveo_embedding_contract::QualifiedEmbeddingRuntime;
#[path = "cold_start/embedding.rs"]
mod embedding;
use veoveo_testing_support::installed::knowledge::InstalledSource;

fn probe_client() -> Result<reqwest::Client> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    Ok(reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(3))
        .redirect(reqwest::redirect::Policy::none())
        .build()?)
}

#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum Schema {
    #[vocabulary(rename = "veoveo.ai/knowledge-cold-start-input/v2")]
    V2,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    schema: Schema,
    source: InstalledSource,
    profile: GatewayProfileId,
    context: String,
    namespace: String,
    release: String,
    deployment_uid: uuid::Uuid,
    namespace_uid: uuid::Uuid,
    image: String,
    embedding: embedding::Selection,
    launch_generation: u64,
    startup_seconds: u64,
    stability_seconds: u64,
}
impl Input {
    fn admit(&self) -> Result<()> {
        ensure!(
            self.schema == Schema::V2,
            "unsupported Knowledge cold-start profile"
        );
        for value in [
            &self.source.deployment,
            &self.namespace,
            &self.release,
            &self.embedding.deployment,
        ] {
            name(value)?;
        }
        ensure!(
            !self.context.is_empty() && !self.context.chars().any(char::is_control),
            "invalid Kubernetes context"
        );
        ensure!(
            !self.deployment_uid.is_nil() && !self.namespace_uid.is_nil(),
            "cold-start identities must be nonnil"
        );
        ensure!(
            (30..=1800).contains(&self.startup_seconds)
                && (5..=120).contains(&self.stability_seconds),
            "cold-start startup must be30..1800seconds and stability5..120seconds"
        );
        ensure!(
            self.launch_generation > 1,
            "cold-start launch generation must follow admitted baseline"
        );
        ensure!(
            self.source.caller_token_file.is_absolute(),
            "cold-start token path must be absolute"
        );
        image(&self.image, "knowledge-mcp")?;
        self.embedding.admit()?;
        Ok(())
    }
    fn admit_installation(
        &self,
        context: &str,
        namespace: &str,
        deployments: &[String],
        profile: &str,
    ) -> Result<()> {
        ensure!(
            context == self.context
                && namespace == self.namespace
                && deployments.contains(&self.source.deployment)
                && profile == self.profile.as_str(),
            "Knowledge cold-start fixture differs from InstallationTarget"
        );
        Ok(())
    }
}
fn name(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.len() <= 63
            && value
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            && value.as_bytes()[0].is_ascii_alphanumeric()
            && value.as_bytes()[value.len() - 1].is_ascii_alphanumeric(),
        "invalid cold-start Kubernetes name"
    );
    Ok(())
}
fn image(value: &str, role: &str) -> Result<()> {
    let (repository, digest) = value
        .rsplit_once("@sha256:")
        .context("cold-start image must be digest-pinned")?;
    ensure!(
        repository.ends_with(&format!("/{role}"))
            && !repository.chars().any(char::is_whitespace)
            && digest.len() == 64
            && digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "cold-start image differs from owner role or digest profile"
    );
    Ok(())
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(super) struct Instance {
    pod: String,
    pod_uid: uuid::Uuid,
    container_id: String,
    image_id: String,
    restart_count: u32,
}
#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub(super) struct Observation {
    stage: Stage,
    startup_deadline_expired: bool,
    watch_armed_resource_version: Option<String>,
    deployment_uid: Option<uuid::Uuid>,
    namespace_uid: Option<uuid::Uuid>,
    launch_generation: Option<u64>,
    image: Option<String>,
    selected_resource_version: Option<String>,
    startup_pods: BTreeSet<uuid::Uuid>,
    startup_restart_counts: BTreeMap<uuid::Uuid, u32>,
    unavailable: Option<Instance>,
    http_succeeded: Option<Instance>,
    http_fence_resource_version: Option<String>,
    http_fence_observed: bool,
    #[serde(skip)]
    watched_pods: BTreeMap<uuid::Uuid, WatchedPod>,
    ready: Option<Instance>,
    generation: Option<GenerationId>,
    stable: bool,
}
#[derive(Clone, Copy, Default, PartialEq, Eq, veoveo_types::Vocabulary)]
enum Stage {
    #[default]
    Admission,
    InitialWatchHandshake,
    AwaitingPod,
    MissingContainerStatus,
    MissingContainerIds,
    AwaitingHttp503,
    AwaitingHttp200,
    ReadinessFence,
    McpVerification,
    Stability,
    Complete,
}
#[derive(Clone)]
struct WatchedPod {
    resource_version: String,
    instance: Option<Instance>,
    ready: bool,
    deleted: bool,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Metadata {
    name: String,
    #[serde(default)]
    namespace: String,
    uid: uuid::Uuid,
    resource_version: String,
    #[serde(default)]
    labels: BTreeMap<String, String>,
    #[serde(default)]
    generation: Option<u64>,
    #[serde(default)]
    owner_references: Vec<Owner>,
    #[serde(default)]
    deletion_timestamp: Option<String>,
}
#[derive(Clone, Deserialize)]
struct Owner {
    kind: String,
    name: String,
    uid: uuid::Uuid,
    controller: Option<bool>,
}
#[derive(Clone, Deserialize)]
struct Container {
    name: String,
    image: String,
    #[serde(default)]
    resources: Resources,
    #[serde(default)]
    env: Vec<Environment>,
    #[serde(default)]
    ports: Vec<ContainerPort>,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Environment {
    name: String,
    value: Option<String>,
    value_from: Option<serde::de::IgnoredAny>,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContainerPort {
    name: Option<String>,
    container_port: u16,
    protocol: Option<String>,
}
#[derive(Clone, Default, Deserialize)]
struct Resources {
    #[serde(default)]
    requests: BTreeMap<String, String>,
    #[serde(default)]
    limits: BTreeMap<String, String>,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContainerStatus {
    name: String,
    #[serde(default, rename = "containerID")]
    container_id: String,
    #[serde(default, rename = "imageID")]
    image_id: String,
    restart_count: u32,
    ready: bool,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PodStatus {
    #[serde(default, rename = "podIP")]
    pod_ip: Option<std::net::IpAddr>,
    #[serde(default)]
    phase: Option<PodPhase>,
    #[serde(default)]
    container_statuses: Vec<ContainerStatus>,
    #[serde(default)]
    conditions: Vec<PodCondition>,
}
#[derive(Clone, Copy, Deserialize)]
enum PodPhase {
    Succeeded,
    Failed,
    #[serde(other)]
    ActiveOrUnknown,
}
#[derive(Clone, Deserialize)]
struct PodCondition {
    #[serde(rename = "type")]
    kind: PodConditionKind,
    status: PodConditionStatus,
}
#[derive(Clone, PartialEq, Eq, Deserialize)]
enum PodConditionKind {
    Ready,
    #[serde(other)]
    Other,
}
#[derive(Clone, PartialEq, Eq, Deserialize)]
enum PodConditionStatus {
    True,
    False,
    Unknown,
}
#[derive(Clone, Deserialize)]
struct PodSpec {
    containers: Vec<Container>,
}
#[derive(Clone, Deserialize)]
struct Pod {
    metadata: Metadata,
    spec: PodSpec,
    status: Option<PodStatus>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Selector {
    match_labels: BTreeMap<String, String>,
}
#[derive(Deserialize)]
struct Template {
    spec: PodSpec,
}
#[derive(Deserialize)]
struct DeploymentSpec {
    replicas: u32,
    selector: Selector,
    template: Template,
}
#[derive(Deserialize)]
struct Deployment {
    metadata: Metadata,
    spec: DeploymentSpec,
}
#[derive(Deserialize)]
struct Object {
    metadata: Metadata,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BookmarkMetadata {
    resource_version: String,
    #[serde(default)]
    annotations: BTreeMap<String, String>,
}
#[derive(Deserialize)]
struct Bookmark {
    metadata: BookmarkMetadata,
}
#[derive(Deserialize)]
struct WatchError {
    code: u16,
}
#[derive(Deserialize)]
#[serde(tag = "type", content = "object")]
enum Event {
    #[serde(rename = "ADDED")]
    Added(Pod),
    #[serde(rename = "MODIFIED")]
    Modified(Pod),
    #[serde(rename = "DELETED")]
    Deleted(Pod),
    #[serde(rename = "BOOKMARK")]
    Bookmark(Bookmark),
    #[serde(rename = "ERROR")]
    Error(WatchError),
}
impl Pod {
    fn instance(&self, expected_image: &str, namespace: &str) -> Result<Option<Instance>> {
        ensure!(
            !self.status.as_ref().is_some_and(|status| matches!(
                status.phase,
                Some(PodPhase::Succeeded | PodPhase::Failed)
            )),
            "Knowledge observed candidate Pod is terminal"
        );
        name(&self.metadata.name)?;
        ensure!(
            self.metadata.namespace == namespace
                && !self.metadata.uid.is_nil()
                && !self.metadata.resource_version.is_empty(),
            "Knowledge Pod identity invalid"
        );
        let selected = self
            .spec
            .containers
            .iter()
            .filter(|c| c.name == "knowledge-mcp")
            .collect::<Vec<_>>();
        ensure!(
            selected.len() == 1 && selected[0].image == expected_image,
            "Knowledge Pod container role/image differs"
        );
        let statuses = self
            .status
            .as_ref()
            .map(|s| s.container_statuses.as_slice())
            .unwrap_or_default();
        let selected = statuses
            .iter()
            .filter(|s| s.name == "knowledge-mcp")
            .collect::<Vec<_>>();
        ensure!(selected.len() <= 1, "duplicate Knowledge container status");
        let Some(status) = selected.first() else {
            return Ok(None);
        };
        ensure!(
            status.restart_count <= 64,
            "Knowledge startup retry budget exhausted"
        );
        if status.container_id.is_empty() {
            return Ok(None);
        }
        ensure!(
            !status.image_id.is_empty()
                && status.container_id.len() <= 256
                && status.image_id.len() <= 256,
            "Knowledge container instance absent or oversized"
        );
        Ok(Some(Instance {
            pod: self.metadata.name.clone(),
            pod_uid: self.metadata.uid,
            container_id: status.container_id.clone(),
            image_id: status.image_id.clone(),
            restart_count: status.restart_count,
        }))
    }
    fn ready(&self) -> bool {
        self.status.as_ref().is_some_and(|s| {
            s.container_statuses
                .iter()
                .any(|c| c.name == "knowledge-mcp" && c.ready)
                && s.conditions.iter().any(|c| {
                    c.kind == PodConditionKind::Ready && c.status == PodConditionStatus::True
                })
        })
    }
}
impl Observation {
    fn pod(
        &mut self,
        pod: &Pod,
        expected_image: &str,
        namespace: &str,
        initial: bool,
    ) -> Result<Option<Instance>> {
        let instance = pod.instance(expected_image, namespace)?;
        if self.http_succeeded.is_none() {
            self.stage = if instance.is_some() {
                if self.unavailable.as_ref() == instance.as_ref() {
                    Stage::AwaitingHttp200
                } else {
                    Stage::AwaitingHttp503
                }
            } else if pod.status.as_ref().is_some_and(|status| {
                status
                    .container_statuses
                    .iter()
                    .any(|container| container.name == "knowledge-mcp")
            }) {
                Stage::MissingContainerIds
            } else {
                Stage::MissingContainerStatus
            };
        }
        ensure!(
            !initial || !pod.ready(),
            "Knowledge cold-start baseline already Ready"
        );
        if self.http_fence_observed
            && let Some(successful) = &self.http_succeeded
        {
            if pod.metadata.uid != successful.pod_uid {
                ensure!(
                    self.startup_pods.contains(&pod.metadata.uid),
                    "unadmitted competing Knowledge Pod appeared after startup fence"
                );
                return Ok(None);
            }
            self.qualify_fenced_pod(
                &instance,
                pod.ready(),
                pod.metadata.deletion_timestamp.is_some(),
            )?;
        }
        if self
            .http_succeeded
            .as_ref()
            .is_none_or(|successful| successful.pod_uid == pod.metadata.uid)
        {
            self.selected_resource_version = Some(pod.metadata.resource_version.clone());
        }
        self.startup_pods.insert(pod.metadata.uid);
        ensure!(
            self.startup_pods.len() <= 16,
            "Knowledge startup Pod budget exhausted"
        );
        if let Some(instance) = &instance {
            self.startup_restart_counts
                .insert(instance.pod_uid, instance.restart_count);
        }
        self.watched_pods.insert(
            pod.metadata.uid,
            WatchedPod {
                resource_version: pod.metadata.resource_version.clone(),
                instance: instance.clone(),
                ready: pod.ready(),
                deleted: pod.metadata.deletion_timestamp.is_some(),
            },
        );
        if !self.http_fence_observed
            && self
                .http_succeeded
                .as_ref()
                .is_some_and(|successful| successful.pod_uid == pod.metadata.uid)
            && self.http_fence_resource_version.as_ref() == Some(&pod.metadata.resource_version)
        {
            self.http_fence_observed = true;
            self.qualify_fenced_pod(
                &instance,
                pod.ready(),
                pod.metadata.deletion_timestamp.is_some(),
            )?;
        }
        Ok(instance)
    }
    fn qualify_fenced_pod(
        &mut self,
        instance: &Option<Instance>,
        ready: bool,
        deleted: bool,
    ) -> Result<()> {
        ensure!(
            !deleted && instance.as_ref() == self.http_succeeded.as_ref(),
            "Knowledge HTTP-success Pod/container changed at or after startup fence"
        );
        ensure!(
            self.ready.is_none() || ready,
            "Knowledge post-Ready readiness regressed"
        );
        if ready {
            self.ready = self.http_succeeded.clone();
        }
        Ok(())
    }
    fn admit_http_fence(&mut self, pod: &Pod, expected_image: &str, namespace: &str) -> Result<()> {
        let instance = pod.instance(expected_image, namespace)?;
        ensure!(
            self.http_succeeded.is_some()
                && pod.metadata.deletion_timestamp.is_none()
                && instance.as_ref() == self.http_succeeded.as_ref(),
            "Knowledge post-200 Pod GET differs from successful container"
        );
        self.http_fence_resource_version = Some(pod.metadata.resource_version.clone());
        if let Some(watched) = self.watched_pods.get(&pod.metadata.uid).cloned()
            && watched.resource_version == pod.metadata.resource_version
        {
            ensure!(
                watched.instance == instance
                    && watched.ready == pod.ready()
                    && watched.deleted == pod.metadata.deletion_timestamp.is_some(),
                "Knowledge Pod snapshot differs at the same resourceVersion"
            );
            self.http_fence_observed = true;
            self.selected_resource_version = Some(watched.resource_version.clone());
            self.qualify_fenced_pod(&watched.instance, watched.ready, watched.deleted)?;
        }
        Ok(())
    }
    fn status(&mut self, instance: &Instance, status: u16) -> Result<bool> {
        match status {
            503 if self.http_succeeded.is_none() => {
                self.unavailable = Some(instance.clone());
                self.stage = Stage::AwaitingHttp200;
            }
            200 => {
                ensure!(
                    self.unavailable.as_ref() == Some(instance),
                    "Knowledge readiness200 requires503 from the same container instance"
                );
                ensure!(
                    self.http_succeeded
                        .as_ref()
                        .is_none_or(|ready| ready == instance),
                    "Knowledge ready instance changed"
                );
                self.http_succeeded = Some(instance.clone());
                self.stage = Stage::ReadinessFence;
                return Ok(true);
            }
            _ => anyhow::bail!("Knowledge readyz returned unexpected status or regressed"),
        }
        Ok(false)
    }
}

fn command(context: &str, namespace: &str) -> tokio::process::Command {
    let mut command = tokio::process::Command::new("kubectl");
    command
        .args(["--context", context, "--namespace", namespace])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    command
}
async fn get<T: DeserializeOwned>(
    context: &str,
    namespace: &str,
    kind: &str,
    name: &str,
) -> Result<T> {
    let mut command = command(context, namespace);
    command.args(["get", kind, name, "-o", "json"]);
    let output = veoveo_testing_support::output_async(command, Duration::from_secs(10))
        .await
        .map_err(|_| anyhow::anyhow!("Knowledge Kubernetes read failed"))?;
    ensure!(
        output.status.success() && output.stdout.len() <= 1024 * 1024,
        "Knowledge Kubernetes read failed or exceeded1MiB"
    );
    serde_json::from_slice(&output.stdout)
        .map_err(|_| anyhow::anyhow!("Knowledge Kubernetes identity response invalid"))
}
struct Watch {
    stdout: tokio::process::ChildStdout,
    pending: Vec<u8>,
    bytes: usize,
    events: usize,
    // A select arm may drop ownership admission; the consumed event stays here.
    in_progress: Option<Event>,
}
impl Watch {
    fn url(namespace: &str, timeout_seconds: u64) -> Result<reqwest::Url> {
        let mut url = reqwest::Url::parse("https://kubernetes.invalid/")?;
        url.path_segments_mut()
            .map_err(|_| anyhow::anyhow!("Kubernetes API URL invalid"))?
            .extend(["api", "v1", "namespaces", namespace, "pods"]);
        url.query_pairs_mut()
            .append_pair("watch", "true")
            .append_pair("sendInitialEvents", "true")
            .append_pair("allowWatchBookmarks", "true")
            .append_pair("resourceVersion", "")
            .append_pair("resourceVersionMatch", "NotOlderThan")
            .append_pair("labelSelector", "app.kubernetes.io/component=knowledge-mcp")
            // Exclude only historical completed Pods. Leaving this selection
            // produces DELETED, which still passes through the refusal below.
            .append_pair(
                "fieldSelector",
                "status.phase!=Succeeded,status.phase!=Failed",
            )
            .append_pair("timeoutSeconds", &timeout_seconds.to_string());
        Ok(url)
    }
    fn start(input: &Input, handles: &mut cleanup::Handles) -> Result<Self> {
        let url = Self::url(
            &input.namespace,
            input.startup_seconds + input.stability_seconds + 300,
        )?;
        let raw = &url[url::Position::BeforePath..];
        let mut command = command(&input.context, &input.namespace);
        command
            .args(["get", "--raw", raw])
            .stdout(std::process::Stdio::piped());
        let mut child = veoveo_testing_support::spawn_async(command)?;
        let stdout = child
            .stdout
            .take()
            .context("Knowledge watch stdout absent")?;
        handles.cold_watch = Some(child);
        Ok(Self {
            stdout,
            pending: vec![],
            bytes: 0,
            events: 0,
            in_progress: None,
        })
    }
    async fn next(&mut self) -> Result<Event> {
        loop {
            let mut values =
                serde_json::Deserializer::from_slice(&self.pending).into_iter::<Event>();
            if let Some(value) = values.next() {
                match value {
                    Ok(event) => {
                        let used = values.byte_offset();
                        self.pending.drain(..used);
                        self.events += 1;
                        ensure!(
                            self.events <= 4096,
                            "Knowledge watch event budget exhausted"
                        );
                        return Ok(event);
                    }
                    Err(error) if error.is_eof() => {}
                    Err(_) => anyhow::bail!(
                        "Knowledge watch event malformed; streaming initial events required"
                    ),
                }
            }
            let mut chunk = [0; 4096];
            let n = self.stdout.read(&mut chunk).await?;
            ensure!(
                n > 0,
                "Knowledge watch gap/EOF; streaming initial events and bookmarks required"
            );
            self.bytes += n;
            ensure!(
                self.bytes <= 8 * 1024 * 1024 && self.pending.len() + n <= 256 * 1024,
                "Knowledge watch byte budget exhausted"
            );
            self.pending.extend_from_slice(&chunk[..n]);
        }
    }
}
async fn owned_pod(input: &Input, pod: &Pod) -> Result<()> {
    input
        .embedding
        .admit_endpoint(&pod.spec, &input.namespace)?;
    ensure!(
        pod.metadata.labels.get("app.kubernetes.io/instance") == Some(&input.release)
            && pod
                .metadata
                .labels
                .get("app.kubernetes.io/component")
                .map(String::as_str)
                == Some("knowledge-mcp"),
        "Knowledge Pod release or role differs"
    );
    let owners = pod
        .metadata
        .owner_references
        .iter()
        .filter(|o| o.kind == "ReplicaSet" && o.controller == Some(true))
        .collect::<Vec<_>>();
    ensure!(owners.len() == 1, "Knowledge Pod ReplicaSet owner absent");
    name(&owners[0].name)?;
    let rs: Object = get(
        &input.context,
        &input.namespace,
        "replicaset",
        &owners[0].name,
    )
    .await?;
    ensure!(
        rs.metadata.uid == owners[0].uid
            && rs.metadata.namespace == input.namespace
            && rs
                .metadata
                .owner_references
                .iter()
                .any(|o| o.kind == "Deployment"
                    && o.controller == Some(true)
                    && o.uid == input.deployment_uid
                    && o.name == input.source.deployment),
        "Knowledge Pod does not belong to admitted Deployment"
    );
    Ok(())
}
async fn observe(
    input: &Input,
    watch: &mut Watch,
    state: &mut Observation,
    journal: &receipt::Journal,
    initial: bool,
) -> Result<Option<Instance>> {
    observe_using(
        watch,
        state,
        journal,
        &input.image,
        &input.namespace,
        initial,
        async |pod| owned_pod(input, pod).await,
    )
    .await
}
async fn observe_using(
    watch: &mut Watch,
    state: &mut Observation,
    journal: &receipt::Journal,
    image: &str,
    namespace: &str,
    initial: bool,
    admit_owner: impl AsyncFn(&Pod) -> Result<()>,
) -> Result<Option<Instance>> {
    if watch.in_progress.is_none() {
        // No suspension between consuming the frame and retaining it.
        watch.in_progress = Some(watch.next().await?);
    }
    if let Event::Added(pod) | Event::Modified(pod) | Event::Deleted(pod) =
        watch.in_progress.as_ref().unwrap()
    {
        admit_owner(pod).await?;
    }
    // Ownership admission is the last await. State, journal and event settlement
    // commit in the same poll, so cancellation cannot split these operations.
    let value = match watch.in_progress.as_mut().unwrap() {
        Event::Added(pod) | Event::Modified(pod) => {
            let value = state.pod(pod, image, namespace, initial)?;
            ensure!(
                !initial,
                "Knowledge launch must follow persisted watch handshake; baseline Pod exists"
            );
            journal.cold_start(state.clone())?;
            value
        }
        Event::Deleted(pod) => {
            pod.metadata
                .deletion_timestamp
                .get_or_insert_with(|| "watch-deletion".into());
            state.pod(pod, image, namespace, initial)?;
            journal.cold_start(state.clone())?;
            None
        }
        Event::Bookmark(bookmark) => {
            ensure!(
                !bookmark.metadata.resource_version.is_empty(),
                "Knowledge watch bookmark missing version"
            );
            if initial
                && bookmark
                    .metadata
                    .annotations
                    .get("k8s.io/initial-events-end")
                    .map(String::as_str)
                    == Some("true")
            {
                state.watch_armed_resource_version =
                    Some(bookmark.metadata.resource_version.clone());
                journal.cold_start(state.clone())?;
            }
            None
        }
        Event::Error(error) => anyhow::bail!(
            "Knowledge watch API error code{}; no gap recovery permitted",
            error.code
        ),
    };
    watch.in_progress.take();
    Ok(value)
}

pub(super) struct ForwardOutput {
    stdout: Option<tokio::process::ChildStdout>,
    drain: Option<tokio::task::JoinHandle<Result<()>>>,
    failed: bool,
    deadline: Option<tokio::time::Instant>,
}
impl ForwardOutput {
    fn start_drain(&mut self) {
        let mut stdout = self.stdout.take().expect("retained forward stdout");
        self.drain = Some(tokio::spawn(async move {
            let mut bytes = 0usize;
            let mut buffer = [0; 4096];
            loop {
                let count = stdout.read(&mut buffer).await?;
                if count == 0 {
                    return Ok(());
                }
                bytes += count;
                ensure!(
                    bytes <= 1024 * 1024,
                    "Knowledge portforward output budget exceeded"
                );
            }
        }));
    }
    pub(super) async fn close(&mut self, end: tokio::time::Instant) -> Result<()> {
        let end = *self.deadline.get_or_insert(end);
        ensure!(
            !self.failed,
            "Knowledge prior portforward output cleanup unproven"
        );
        // The child/group is settled first; an interrupted handshake still owns
        // its pipe here and has no separate drain task to await.
        self.stdout.take();
        if let Some(drain) = self.drain.as_mut() {
            match tokio::time::timeout_at(end, drain).await {
                Ok(Ok(Ok(()))) => {
                    self.drain.take();
                }
                _ => {
                    self.failed = true;
                    anyhow::bail!("Knowledge portforward output cleanup unproven");
                }
            }
        }
        Ok(())
    }
}
impl Drop for ForwardOutput {
    fn drop(&mut self) {
        if let Some(drain) = &self.drain {
            drain.abort();
        }
    }
}

async fn forward(
    input: &Input,
    instance: &Instance,
    port: u16,
    handles: &mut cleanup::Handles,
) -> Result<u16> {
    // Hand the old exact process back only after its original cleanup completed.
    handles
        .close_forward(tokio::time::Instant::from_std(
            veoveo_testing_support::lifecycle::owner::cleanup_deadline()?,
        ))
        .await?;
    let mut command = command(&input.context, &input.namespace);
    command
        .args([
            "port-forward",
            "--address",
            "127.0.0.1",
            &format!("pod/{}", instance.pod),
            &format!(":{port}"),
        ])
        .stdout(std::process::Stdio::piped());
    retain_forward(command, port, handles).await
}

async fn retain_forward(
    command: tokio::process::Command,
    port: u16,
    handles: &mut cleanup::Handles,
) -> Result<u16> {
    let mut child = veoveo_testing_support::spawn_async(command)?;
    let stdout = child
        .stdout
        .take()
        .context("Knowledge portforward stdout absent")?;
    // Retain synchronously before waiting for the actual listening handshake.
    handles.cold_forward = Some(child);
    handles.cold_forward_output = Some(ForwardOutput {
        stdout: Some(stdout),
        drain: None,
        failed: false,
        deadline: None,
    });
    let output = handles.cold_forward_output.as_mut().unwrap();
    let line = tokio::time::timeout(Duration::from_secs(10), async {
        let mut bytes = Vec::new();
        loop {
            ensure!(
                bytes.len() < 256,
                "Knowledge portforward handshake oversized"
            );
            let byte = output.stdout.as_mut().unwrap().read_u8().await?;
            bytes.push(byte);
            if byte == b'\n' {
                break;
            }
        }
        String::from_utf8(bytes).map_err(anyhow::Error::from)
    })
    .await
    .context("Knowledge portforward handshake deadline")??;
    let value = line
        .strip_prefix("Forwarding from 127.0.0.1:")
        .and_then(|s| s.split_once(" -> "))
        .context("Knowledge portforward handshake invalid")?;
    ensure!(
        value.1.trim().parse::<u16>()? == port,
        "Knowledge portforward targets wrong port"
    );
    let local = value.0.parse::<u16>()?;
    output.start_drain();
    Ok(local)
}

fn admit_embedding(deployment: &Deployment, namespace: &str, digest: &str) -> Result<()> {
    ensure!(
        deployment.metadata.namespace == namespace
            && !deployment.metadata.uid.is_nil()
            && deployment.spec.replicas == 1,
        "qualified embedding Deployment identity invalid"
    );
    let selected = deployment
        .spec
        .template
        .spec
        .containers
        .iter()
        .filter(|c| c.name == "embedding")
        .collect::<Vec<_>>();
    ensure!(
        selected.len() == 1,
        "qualified embedding container role absent"
    );
    let selected = selected[0];
    ensure!(
        selected
            .image
            .rsplit_once("@sha256:")
            .is_some_and(|(_, actual)| actual == digest)
            && selected
                .resources
                .requests
                .get("nvidia.com/gpu")
                .map(String::as_str)
                == Some("1")
            && selected
                .resources
                .limits
                .get("nvidia.com/gpu")
                .map(String::as_str)
                == Some("1"),
        "embedding image or required NVIDIA allocation differs from qualified input"
    );
    Ok(())
}
fn admit_deployment(input: &Input, deployment: &Deployment, initial: bool) -> Result<()> {
    let generation = if initial {
        input.launch_generation.checked_sub(1)
    } else {
        Some(input.launch_generation)
    };
    let selected = deployment
        .spec
        .template
        .spec
        .containers
        .iter()
        .filter(|c| c.name == "knowledge-mcp")
        .collect::<Vec<_>>();
    ensure!(
        deployment.metadata.labels.get("app.kubernetes.io/instance") == Some(&input.release)
            && deployment.metadata.uid == input.deployment_uid
            && deployment.metadata.namespace == input.namespace
            && deployment.metadata.deletion_timestamp.is_none()
            && deployment.metadata.generation == generation
            && deployment.spec.replicas == if initial { 0 } else { 1 }
            && deployment
                .spec
                .selector
                .match_labels
                .get("app.kubernetes.io/component")
                .map(String::as_str)
                == Some("knowledge-mcp")
            && selected.len() == 1
            && selected[0].image == input.image,
        "Knowledge launch generation or workload admission failed"
    );
    input
        .embedding
        .admit_endpoint(&deployment.spec.template.spec, &input.namespace)?;
    Ok(())
}
async fn verify_deployment(input: &Input) -> Result<()> {
    let deployment: Deployment = get(
        &input.context,
        &input.namespace,
        "deployment",
        &input.source.deployment,
    )
    .await?;
    admit_deployment(input, &deployment, false)?;
    Ok(())
}
async fn same_instance(input: &Input, instance: &Instance) -> Result<bool> {
    let pod: Pod = get(&input.context, &input.namespace, "pod", &instance.pod).await?;
    owned_pod(input, &pod).await?;
    Ok(pod.metadata.deletion_timestamp.is_none()
        && pod.instance(&input.image, &input.namespace)?.as_ref() == Some(instance))
}
async fn probe(
    input: &Input,
    http: &reqwest::Client,
    port: u16,
    path: &str,
    host: &str,
    state: &mut Observation,
) -> Result<()> {
    let mut url = reqwest::Url::parse("http://127.0.0.1/")?;
    url.set_port(Some(port))
        .map_err(|_| anyhow::anyhow!("Knowledge probe port invalid"))?;
    url.set_path(path);
    let ready = state
        .ready
        .clone()
        .context("Knowledge ready instance absent")?;
    ensure!(
        same_instance(input, &ready).await?,
        "Knowledge post-Ready container changed"
    );
    let response = http
        .get(url)
        .header(reqwest::header::HOST, host)
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("Knowledge post-Ready probe unavailable"))?;
    ensure!(
        same_instance(input, &ready).await?,
        "Knowledge post-Ready container changed"
    );
    state.status(&ready, response.status().as_u16())?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires prearmed native watch, externally authorized launch, normal OAuth and qualified NVIDIA embeddings"]
async fn unattended_cold_start_converges_then_preserves_ready_generation() -> Result<()> {
    let input: Input = veoveo_testing_support::final_tasks::public_caller::read_private_input(
        &env_path("VEOVEO_KNOWLEDGE_COLD_START_INPUT")?,
    )?;
    input.admit()?;
    let target = input.source.validate()?;
    input.admit_installation(
        &target.kubernetes.context,
        &target.kubernetes.namespace,
        &target.expected_deployments,
        &target.operator.profile,
    )?;
    let control = load_control(&target.control_plane_path(&input.source.installation_target))?;
    ensure!(
        endpoint(&control, &input.profile)? == input.source.endpoint,
        "Knowledge OAuth endpoint differs from selected profile"
    );
    let server = control
        .servers
        .iter()
        .find(|s| s.slug.as_str() == "knowledge")
        .context("Knowledge server absent")?;
    let health = reqwest::Url::parse(server.upstream.health_url.as_str())?;
    let host = health[url::Position::BeforeHost..url::Position::AfterPort].to_owned();
    ensure!(
        health.host_str() == Some("knowledge-mcp")
            && health.scheme() == "http"
            && health.username().is_empty()
            && health.password().is_none()
            && health.query().is_none()
            && health.fragment().is_none()
            && health.path() == "/knowledge/readyz",
        "Knowledge owner health route differs"
    );
    let port = health
        .port_or_known_default()
        .context("Knowledge health port absent")?;
    let journal = receipt::Journal::open(&input.source.output, input.profile.clone())?;
    journal.cold_start(Observation::default())?;
    let result = veoveo_testing_support::lifecycle::owner::run(async {
        let owned = cleanup::register(&journal)?;
        let result = tokio::time::timeout(Duration::from_secs(input.startup_seconds+input.stability_seconds+300),async {
            let mut handles = owned.lock().await;
            let namespace: Object = get(&input.context,&input.namespace,"namespace",&input.namespace).await?;
            let deployment: Deployment = get(&input.context,&input.namespace,"deployment",&input.source.deployment).await?;
            ensure!(namespace.metadata.uid == input.namespace_uid,"Knowledge namespace identity changed");
            embedding::admit_caller_namespace(&input, &namespace.metadata)?;
            admit_deployment(&input,&deployment,true)?;
            embedding::observe(&input).await?;
            let mut state = Observation { deployment_uid:Some(input.deployment_uid), namespace_uid:Some(input.namespace_uid),launch_generation:Some(input.launch_generation),image:Some(input.image.clone()), ..Observation::default() };
            journal.cold_start(state.clone())?;
            state.stage = Stage::InitialWatchHandshake;
            journal.cold_start(state.clone())?;
            let mut watch = Watch::start(&input,&mut handles)?;
            journal.native_observers(receipt::Close::Open)?;
            tokio::time::timeout(Duration::from_secs(15),async {
                while state.watch_armed_resource_version.is_none() { observe(&input,&mut watch,&mut state,&journal,true).await?; }
                Ok::<_,anyhow::Error>(())
            }).await.context("Knowledge streaming initial-event handshake missing; API support required")??;
            state.stage = Stage::AwaitingPod;
            journal.cold_start(state.clone())?;
            let deadline = tokio::time::Instant::now()+Duration::from_secs(input.startup_seconds);
            let http = probe_client()?;
            let mut current: Option<Instance> = None;
            let mut local = None;
            let mut probes = tokio::time::interval(Duration::from_secs(1));
            probes.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let startup = tokio::time::timeout_at(deadline, async { loop {
                tokio::select! {
                    value = observe(&input,&mut watch,&mut state,&journal,false) => {
                        if let Some(instance) = value? && current.as_ref() != Some(&instance) {
                            let port = forward(&input,&instance,port,&mut handles).await?;
                            current = Some(instance); local = Some(port);
                        }
                    }
                    _ = probes.tick(), if local.is_some() => {
                        let mut url = reqwest::Url::parse("http://127.0.0.1/")?;
                        url.set_port(local).map_err(|_| anyhow::anyhow!("Knowledge probe port invalid"))?;
                        url.set_path(health.path());
                        if same_instance(&input,current.as_ref().unwrap()).await? && let Ok(response) = http.get(url).header(reqwest::header::HOST,&host).send().await && same_instance(&input,current.as_ref().unwrap()).await? {
                            let ready = state.status(current.as_ref().unwrap(),response.status().as_u16())?;
                            journal.cold_start(state.clone())?;
                            if ready {
                                let successful = state.http_succeeded.as_ref().context("Knowledge HTTP-success instance absent")?;
                                let pod: Pod = get(&input.context,&input.namespace,"pod",&successful.pod).await?;
                                owned_pod(&input,&pod).await?;
                                state.admit_http_fence(&pod,&input.image,&input.namespace)?;
                                journal.cold_start(state.clone())?;
                                while state.ready.is_none() {
                                    observe(&input,&mut watch,&mut state,&journal,false).await?;
                                }
                                break;
                            }
                        }
                    }

                }
            }
            Ok::<_,anyhow::Error>(())
            }).await;
            match startup {
                Ok(result) => result?,
                Err(_) => {
                    state.startup_deadline_expired = true;
                    journal.cold_start(state.clone())?;
                    anyhow::bail!("Knowledge startup deadline expired; inspect typed cold-start stage");
                }
            }
            state.stage = Stage::McpVerification;
            journal.cold_start(state.clone())?;
            verify_deployment(&input).await?;
            journal.acquire(&mut handles.caller,connect(&input.source.endpoint,&input.source.caller_token_file)).await?;
            handles.opened_caller(&journal)?;
            let peer = handles.caller.as_ref().unwrap().peer().clone();
            let report = {
                let verify = verify(&peer,&control,input.profile.clone(),&mut handles,&journal);
                tokio::pin!(verify);
                loop { tokio::select! {
                    value = &mut verify => break value?,
                    event = observe(&input,&mut watch,&mut state,&journal,false) => { event?; }
                    _ = probes.tick() => { probe(&input,&http,local.context("Knowledge probe absent")?,health.path(),&host,&mut state).await?; }
                }}
            };
            ensure!(&report.embedding_space == input.embedding.runtime.space(),"Knowledge generation embedding space differs from qualified installation runtime");
            state.stage = Stage::Stability;
            state.generation = Some(report.generation);
            journal.cold_start(state.clone())?;
            let stable = tokio::time::Instant::now()+Duration::from_secs(input.stability_seconds);
            loop { tokio::select! {
                event = observe(&input,&mut watch,&mut state,&journal,false) => { event?; }
                _ = probes.tick() => {
                    probe(&input,&http,local.context("Knowledge probe absent")?,health.path(),&host,&mut state).await?;
                }
                _ = tokio::time::sleep_until(stable) => break,
            }}
            let ready = state.ready.as_ref().context("Knowledge ready instance absent")?;
            let pod: Pod = get(&input.context,&input.namespace,"pod",&ready.pod).await?;
            owned_pod(&input,&pod).await?;
            ensure!(pod.instance(&input.image,&input.namespace)?.as_ref() == state.ready.as_ref() && pod.ready() && pod.metadata.deletion_timestamp.is_none(), "Knowledge Ready Pod changed during verification");
            for collection in report.statistics.keys() {
                let entry: CollectionCatalogEntry = read_json(&peer,&journal,&KnowledgeResource::Collection(collection.clone()).to_uri()?).await?;
                ensure!(entry.generation.as_ref() == state.generation.as_ref(),"Knowledge current generation changed during stability");
            }
            probe(&input,&http,local.context("Knowledge probe absent")?,health.path(),&host,&mut state).await?;
            verify_deployment(&input).await?;
            embedding::observe(&input).await?;
            let ready = state.ready.as_ref().context("Knowledge ready instance absent")?;
            let pod: Pod = get(&input.context,&input.namespace,"pod",&ready.pod).await?;
            owned_pod(&input,&pod).await?;
            ensure!(pod.instance(&input.image,&input.namespace)?.as_ref() == state.ready.as_ref() && pod.ready() && pod.metadata.deletion_timestamp.is_none(), "Knowledge final Ready Pod changed");
            let final_version = pod.metadata.resource_version.clone();
            tokio::time::timeout(Duration::from_secs(10), async {
                while watch.in_progress.is_some() || state.selected_resource_version.as_ref() != Some(&final_version) {
                    observe(&input,&mut watch,&mut state,&journal,false).await?;
                }
                Ok::<_,anyhow::Error>(())
            }).await.context("Knowledge final Pod watch fence missing")??;
            state.stage = Stage::Complete;
            state.stable = true;
            journal.cold_start(state)?;
            handles.close(&journal).await?;
            Ok(report)
        }).await.context("Knowledge cold-start total deadline expired").and_then(|r| r);
        journal.operation(&result)?;
        result
    }).await;
    match result {
        Ok(report) => journal.finish(true, Some(report)),
        Err(error) => {
            journal.failure(&error)?;
            journal.finish(false, None)?;
            anyhow::bail!("Knowledge cold-start unqualified; inspect private outcome");
        }
    }
}

#[cfg(test)]
#[tokio::test]
async fn cold_start_portforward_drains_later_output_and_owns_cancelled_cleanup() -> Result<()> {
    const MODE: &str = "VEOVEO_KNOWLEDGE_FORWARD_CONTROL";
    if let Ok(mode) = std::env::var(MODE) {
        let directory = super::controls::Scratch::new()?;
        let journal =
            receipt::Journal::open(&directory.0.join("outcome.json"), "operator".parse()?)?;
        let mut retained = None;
        let result: Result<()> = veoveo_testing_support::lifecycle::owner::run(async {
            let owned = cleanup::register(&journal)?;
            retained = Some(owned.clone());
            let mut handles = owned.lock().await;
            let marker = directory.0.join("drained");
            let mut command = tokio::process::Command::new("sh");
            if mode == "handshake-drop" {
                command.args(["-c", "exec sleep 30"]);
            } else {
                command.args(["-c", "printf 'Forwarding from 127.0.0.1:12345 -> 8080\\n'; sleep 0.05; head -c 786432 /dev/zero; printf 'Handling connection for 12345\\n'; printf drained > \"$1\"; exec sleep 30", "forward-control"]).arg(&marker);
            }
            command.stdout(std::process::Stdio::piped());
            if mode == "handshake-drop" {
                ensure!(tokio::time::timeout(Duration::from_millis(50), retain_forward(command, 8080, &mut handles)).await.is_err());
                anyhow::bail!("controlled handshake interruption");
            }
            ensure!(retain_forward(command, 8080, &mut handles).await? == 12345);
            tokio::time::timeout(Duration::from_secs(1), async {
                while !marker.exists() { tokio::time::sleep(Duration::from_millis(5)).await; }
            }).await.context("later portforward output was not drained")?;
            ensure!(handles.cold_forward_output.as_ref().unwrap().drain.as_ref().is_some_and(|drain| !drain.is_finished()));
            ensure!(tokio::time::timeout(Duration::from_millis(20), handles.cold_forward.as_mut().unwrap().wait()).await.is_err(), "connection notice terminated forward child");
            if mode == "owner-drop" {
                std::future::pending::<()>().await;
            }
            handles.close_forward(tokio::time::Instant::from_std(veoveo_testing_support::lifecycle::owner::cleanup_deadline()?)).await?;
            ensure!(handles.cold_forward.is_none() && handles.cold_forward_output.is_none());
            // Replacement also owns its original stdout through owner cleanup.
            let mut replacement = tokio::process::Command::new("sh");
            replacement.args(["-c", "printf 'Forwarding from 127.0.0.1:12346 -> 8080\\n'; exec sleep 30"]).stdout(std::process::Stdio::piped());
            ensure!(retain_forward(replacement, 8080, &mut handles).await? == 12346);
            Ok(())
        }).await;
        ensure!(result.is_ok() == (mode == "replacement"));
        let handles = retained.unwrap();
        let handles = handles.lock().await;
        ensure!(
            handles.cold_forward.is_none() && handles.cold_forward_output.is_none(),
            "owned forward cleanup did not settle child and drain"
        );
        return Ok(());
    }
    for mode in ["replacement", "owner-drop", "handshake-drop"] {
        let directory = super::controls::Scratch::new()?;
        let deadline = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis()
            + 2500;
        let mut command = tokio::process::Command::new(std::env::current_exe()?);
        command.args(["installed::cold_start::cold_start_portforward_drains_later_output_and_owns_cancelled_cleanup", "--exact", "--nocapture"])
            .env(MODE, mode).env("VEOVEO_SMOKE_DEADLINE_UNIX_MS", deadline.to_string())
            .env("VEOVEO_SMOKE_CLEANUP_SECONDS", "4").env("VEOVEO_SMOKE_LOCAL_GROUPS", &directory.0);
        let output = veoveo_testing_support::output_async(command, Duration::from_secs(10)).await?;
        ensure!(
            output.status.success(),
            "Knowledge owned portforward control failed: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

#[cfg(test)]
#[tokio::test]
async fn cold_start_probe_client_initializes_crypto_before_installed_connect() -> Result<()> {
    // Run this selector alone in a fresh test process to qualify the cold-case
    // construction order without installed connect or another TLS fixture.
    let client = probe_client()?;
    ensure!(rustls::crypto::CryptoProvider::get_default().is_some());
    let request = client.get("https://probe.invalid/readyz").build()?;
    ensure!(request.url().scheme() == "https");
    Ok(())
}

#[cfg(test)]
fn control_pod(ready: bool, restarts: u32) -> Pod {
    Pod {
        metadata: Metadata {
            name: "knowledge-1".into(),
            namespace: "fixture".into(),
            uid: uuid::Uuid::from_u128(1),
            resource_version: "10".into(),
            labels: BTreeMap::from([
                ("app.kubernetes.io/instance".into(), "fixture".into()),
                ("app.kubernetes.io/component".into(), "knowledge-mcp".into()),
            ]),
            generation: None,
            owner_references: vec![],
            deletion_timestamp: None,
        },
        spec: PodSpec {
            containers: vec![Container {
                name: "knowledge-mcp".into(),
                image: format!("registry.invalid/knowledge-mcp@sha256:{}", "a".repeat(64)),
                resources: Resources::default(),
                ports: vec![],
                env: vec![Environment {
                    name: "VEOVEO_EMBEDDING_ENDPOINT".into(),
                    value: Some("http://embedding.fixture.svc.cluster.local:8000/".into()),
                    value_from: None,
                }],
            }],
        },
        status: Some(PodStatus {
            pod_ip: None,
            phase: Some(PodPhase::ActiveOrUnknown),
            conditions: vec![PodCondition {
                kind: PodConditionKind::Ready,
                status: if ready {
                    PodConditionStatus::True
                } else {
                    PodConditionStatus::False
                },
            }],
            container_statuses: vec![ContainerStatus {
                name: "knowledge-mcp".into(),
                container_id: format!("containerd://instance-{restarts}"),
                image_id: format!("sha256:{}", "a".repeat(64)),
                restart_count: restarts,
                ready,
            }],
        }),
    }
}

#[test]
fn cold_start_decodes_kubernetes_container_ids_and_reports_missing_instance_stage() -> Result<()> {
    let mut pod = control_pod(false, 0);
    let image = pod.spec.containers[0].image.clone();
    let wire = serde_json::json!({
        "metadata": {"name":"knowledge-1", "namespace":"fixture", "uid":pod.metadata.uid, "resourceVersion":"native-wire"},
        "spec":{"containers":[{"name":"knowledge-mcp","image":image}]},
        "status":{"phase":"Running","containerStatuses":[{
            "name":"knowledge-mcp","containerID":"containerd://actual-instance",
            "imageID":"sha256:actual-image","restartCount":0,"ready":false
        }],"conditions":[{"type":"Ready","status":"False"}]}
    });
    let decoded: Pod = serde_json::from_value(wire)?;
    let mut state = Observation::default();
    let instance = state.pod(&decoded, &image, "fixture", false)?.unwrap();
    ensure!(
        instance.container_id == "containerd://actual-instance"
            && instance.image_id == "sha256:actual-image"
    );
    ensure!(state.startup_restart_counts.get(&pod.metadata.uid) == Some(&0));
    ensure!(state.stage == Stage::AwaitingHttp503);
    state.status(&instance, 503)?;
    ensure!(state.stage == Stage::AwaitingHttp200);
    state.status(&instance, 200)?;
    ensure!(state.stage == Stage::ReadinessFence);
    // Kubernetes may initially omit status, then publish a waiting container.
    pod.status = None;
    let mut waiting = Observation::default();
    ensure!(waiting.pod(&pod, &image, "fixture", false)?.is_none());
    ensure!(waiting.stage == Stage::MissingContainerStatus);
    let misspelled: ContainerStatus = serde_json::from_value(serde_json::json!({
        "name":"knowledge-mcp","containerId":"obsolete","imageId":"obsolete","restartCount":0,"ready":false
    }))?;
    ensure!(
        misspelled.container_id.is_empty() && misspelled.image_id.is_empty(),
        "misspelled wire aliases admitted"
    );
    pod.status = Some(PodStatus {
        pod_ip: None,
        phase: Some(PodPhase::ActiveOrUnknown),
        container_statuses: vec![misspelled],
        conditions: vec![],
    });
    ensure!(waiting.pod(&pod, &image, "fixture", false)?.is_none());
    ensure!(waiting.stage == Stage::MissingContainerIds);
    Ok(())
}

#[test]
fn cold_start_watch_selects_active_candidates_and_refuses_terminal_instances() -> Result<()> {
    let url = Watch::url("fixture", 335)?;
    let pairs = url.query_pairs().collect::<BTreeMap<_, _>>();
    ensure!(
        pairs.get("fieldSelector").map(|s| s.as_ref())
            == Some("status.phase!=Succeeded,status.phase!=Failed")
    );
    ensure!(pairs.get("sendInitialEvents").map(|s| s.as_ref()) == Some("true"));
    ensure!(pairs.get("resourceVersionMatch").map(|s| s.as_ref()) == Some("NotOlderThan"));
    // The native API excludes both historical terminal phases before emitting
    // initial events; no owner/image mismatch is ignored by the local parser.
    for phase in ["Pending", "Running", "Unknown", "Succeeded", "Failed"] {
        let mut pod = control_pod(false, 0);
        pod.status.as_mut().unwrap().phase =
            Some(serde_json::from_value(serde_json::json!(phase))?);
        let image = pod.spec.containers[0].image.clone();
        let terminal = matches!(phase, "Succeeded" | "Failed");
        ensure!(pod.instance(&image, "fixture").is_err() == terminal);
        pod.spec.containers[0].image = "wrong-image".into();
        ensure!(
            pod.instance(&image, "fixture").is_err(),
            "active wrong-image candidate admitted"
        );
    }
    Ok(())
}

#[test]
fn cold_start_requires_same_instance_503_then_200_and_freezes_post_ready() -> Result<()> {
    let pod = control_pod(false, 0);
    let image = pod.spec.containers[0].image.clone();
    let mut state = Observation::default();
    let first = state.pod(&pod, &image, "fixture", false)?.unwrap();
    ensure!(state.status(&first, 200).is_err(), "missing503 accepted");
    state.status(&first, 503)?;
    let restarted = control_pod(false, 1);
    let next = state.pod(&restarted, &image, "fixture", false)?.unwrap();
    ensure!(
        state.status(&next, 200).is_err(),
        "503 from old container admitted"
    );
    state.status(&next, 503)?;
    ensure!(state.status(&next, 200)?);
    state.pod(&restarted, &image, "fixture", false)?;
    let mut ready = control_pod(true, 1);
    ready.metadata.resource_version = "11".into();
    state.admit_http_fence(&ready, &image, "fixture")?;
    state.pod(&ready, &image, "fixture", false)?;
    ensure!(state.http_fence_observed && state.ready.as_ref() == Some(&next));
    ensure!(
        state
            .pod(&control_pod(true, 2), &image, "fixture", false)
            .is_err(),
        "postReady restart accepted"
    );
    ready.metadata.deletion_timestamp = Some("observed".into());
    ensure!(
        state.pod(&ready, &image, "fixture", false).is_err(),
        "postReady deletion accepted"
    );
    ensure!(state.status(&next, 503).is_err(), "postReady503 accepted");
    ensure!(
        Observation::default()
            .pod(&control_pod(true, 0), &image, "fixture", true)
            .is_err(),
        "alreadyReady baseline accepted"
    );
    ensure!(
        Observation::default()
            .pod(&pod, &image, "foreign", false)
            .is_err(),
        "foreign namespace accepted"
    );
    ensure!(
        Observation::default()
            .pod(&pod, "wrong-image", "fixture", false)
            .is_err(),
        "wrong image accepted"
    );
    Ok(())
}

#[test]
fn cold_start_orders_lagging_readiness_with_exact_watch_fence() -> Result<()> {
    let mut selected = control_pod(false, 0);
    let image = selected.spec.containers[0].image.clone();
    let mut prior = control_pod(false, 0);
    prior.metadata.uid = uuid::Uuid::from_u128(2);
    prior.metadata.name = "knowledge-prior".into();
    let mut state = Observation::default();
    state.pod(&prior, &image, "fixture", false)?;
    let instance = state.pod(&selected, &image, "fixture", false)?.unwrap();
    state.status(&instance, 503)?;
    state.status(&instance, 200)?;
    selected.metadata.resource_version = "opaque-fence".into();
    selected.status.as_mut().unwrap().container_statuses[0].ready = true;
    state.admit_http_fence(&selected, &image, "fixture")?;
    let mut queued = selected.clone();
    queued.metadata.resource_version = "queued-before-fence".into();
    state.pod(&queued, &image, "fixture", false)?;
    state.pod(&prior, &image, "fixture", false)?;
    ensure!(
        !state.http_fence_observed && state.ready.is_none(),
        "queued observation prematurely settled Ready"
    );
    state.pod(&selected, &image, "fixture", false)?;
    ensure!(
        state.http_fence_observed && state.ready.is_none(),
        "kubelet readiness delay rejected or bypassed"
    );
    state.pod(&prior, &image, "fixture", false)?;
    ensure!(
        state.selected_resource_version.as_deref() == Some("opaque-fence"),
        "stale other Pod advanced selected fence"
    );
    let mut competing = prior.clone();
    competing.metadata.uid = uuid::Uuid::from_u128(3);
    ensure!(
        state.pod(&competing, &image, "fixture", false).is_err(),
        "new competing Pod admitted after fence"
    );
    let mut changed = control_pod(false, 1);
    changed.metadata.resource_version = "after-fence-restart".into();
    ensure!(
        state.pod(&changed, &image, "fixture", false).is_err(),
        "restart after HTTP fence admitted before kubeletReady"
    );
    let mut ready = selected.clone();
    ready.metadata.resource_version = "first-ready".into();
    ready.status.as_mut().unwrap().container_statuses[0].ready = true;
    ready.status.as_mut().unwrap().conditions[0].status = PodConditionStatus::True;
    state.pod(&ready, &image, "fixture", false)?;
    ensure!(state.ready.as_ref() == Some(&instance));
    state.pod(&prior, &image, "fixture", false)?;
    ensure!(
        state.pod(&selected, &image, "fixture", false).is_err(),
        "actual postReady regression accepted"
    );
    ensure!(
        state.pod(&changed, &image, "fixture", false).is_err(),
        "actual postReady restart accepted"
    );
    let mut deleted = ready;
    deleted.metadata.deletion_timestamp = Some("observed".into());
    ensure!(state.pod(&deleted, &image, "fixture", false).is_err());
    let mut wrong_fence = control_pod(false, 1);
    wrong_fence.metadata.resource_version = "another-fence".into();
    ensure!(
        state
            .admit_http_fence(&wrong_fence, &image, "fixture")
            .is_err(),
        "post200GET accepted different container"
    );
    let pending = control_pod(false, 0);
    let mut already_seen = Observation::default();
    let instance = already_seen
        .pod(&pending, &image, "fixture", false)?
        .unwrap();
    already_seen.status(&instance, 503)?;
    already_seen.status(&instance, 200)?;
    already_seen.admit_http_fence(&pending, &image, "fixture")?;
    ensure!(
        already_seen.http_fence_observed && already_seen.ready.is_none(),
        "already-observed exact RV required a new event or bypassed kubeletReady"
    );
    Ok(())
}

#[test]
fn cold_start_closed_input_and_qualified_embedding_admission() -> Result<()> {
    let runtime = crate::indexing::SyntheticEmbeddings::new().runtime;
    let mut input = Input {
        schema: Schema::V2,
        source: InstalledSource {
            installation_target: "/private/installation.json".into(),
            endpoint: "https://gateway.invalid/operator/mcp".parse()?,
            caller_token_file: "/private/token".into(),
            deployment: "knowledge-mcp".into(),
            output: "/private/new-outcome.json".into(),
        },
        profile: "operator".parse()?,
        context: "fixture".into(),
        namespace: "fixture".into(),
        release: "fixture".into(),
        deployment_uid: uuid::Uuid::from_u128(1),
        namespace_uid: uuid::Uuid::from_u128(2),
        image: format!("registry.invalid/knowledge-mcp@sha256:{}", "a".repeat(64)),
        embedding: embedding::Selection {
            namespace: "fixture".into(),
            namespace_uid: uuid::Uuid::from_u128(2),
            deployment: "embedding".into(),
            deployment_uid: uuid::Uuid::from_u128(3),
            service: "embedding".into(),
            service_uid: uuid::Uuid::from_u128(4),
            port: 8000,
            runtime,
        },
        launch_generation: 2,
        startup_seconds: 30,
        stability_seconds: 5,
    };
    input.admit()?;
    let mut caller = control_pod(false, 0).metadata;
    caller.name = "fixture".into();
    caller.uid = input.namespace_uid;
    embedding::admit_caller_namespace(&input, &caller)?;
    input.embedding.namespace = "shared".into();
    ensure!(embedding::admit_caller_namespace(&input, &caller).is_err());
    caller.labels.insert(
        "veoveo.ai/embedding-consumer".into(),
        input.namespace_uid.to_string(),
    );
    embedding::admit_caller_namespace(&input, &caller)?;
    caller.labels.insert(
        "veoveo.ai/embedding-consumer".into(),
        uuid::Uuid::from_u128(99).to_string(),
    );
    ensure!(embedding::admit_caller_namespace(&input, &caller).is_err());
    input.embedding.namespace = "fixture".into();
    let deployments = vec!["knowledge-mcp".into(), "embedding".into()];
    input.admit_installation("fixture", "fixture", &deployments, "operator")?;
    ensure!(
        input
            .admit_installation("foreign", "fixture", &deployments, "operator")
            .is_err()
    );
    ensure!(
        input
            .admit_installation("fixture", "foreign", &deployments, "operator")
            .is_err()
    );
    ensure!(
        input
            .admit_installation("fixture", "fixture", &[], "operator")
            .is_err()
    );
    ensure!(
        input
            .admit_installation("fixture", "fixture", &deployments, "foreign")
            .is_err()
    );
    let pod = control_pod(false, 0);
    let mut deployment = Deployment {
        metadata: pod.metadata,
        spec: DeploymentSpec {
            replicas: 0,
            selector: Selector {
                match_labels: BTreeMap::from([(
                    "app.kubernetes.io/component".into(),
                    "knowledge-mcp".into(),
                )]),
            },
            template: Template { spec: pod.spec },
        },
    };
    deployment.metadata.generation = Some(1);
    admit_deployment(&input, &deployment, true)?;
    input.release = "foreign".into();
    ensure!(admit_deployment(&input, &deployment, true).is_err());
    input.release = "fixture".into();
    deployment.spec.replicas = 1;
    ensure!(admit_deployment(&input, &deployment, true).is_err());
    deployment.metadata.generation = Some(2);
    admit_deployment(&input, &deployment, false)?;
    deployment.metadata.generation = Some(3);
    ensure!(admit_deployment(&input, &deployment, false).is_err());
    input.startup_seconds = 29;
    ensure!(input.admit().is_err());
    input.startup_seconds = 30;
    input.launch_generation = 0;
    ensure!(input.admit().is_err());
    input.launch_generation = 2;
    input.image = format!("registry.invalid/sidecar@sha256:{}", "a".repeat(64));
    ensure!(input.admit().is_err());
    ensure!(serde_json::from_value::<Input>(serde_json::json!({"schema":"veoveo.ai/knowledge-cold-start-input/v2","unexpected":true})).is_err());
    let mut embedding = Deployment {
        metadata: Metadata {
            name: "embedding".into(),
            namespace: "fixture".into(),
            uid: uuid::Uuid::from_u128(3),
            resource_version: "20".into(),
            labels: BTreeMap::new(),
            generation: Some(1),
            owner_references: vec![],
            deletion_timestamp: None,
        },
        spec: DeploymentSpec {
            replicas: 1,
            selector: Selector {
                match_labels: BTreeMap::new(),
            },
            template: Template {
                spec: PodSpec {
                    containers: vec![Container {
                        name: "embedding".into(),
                        image: format!(
                            "registry.invalid/embedding@sha256:{}",
                            input
                                .embedding
                                .runtime
                                .profile()
                                .contents()
                                .runtime_image
                                .hex()
                        ),
                        env: vec![],
                        ports: vec![],
                        resources: Resources {
                            requests: BTreeMap::from([("nvidia.com/gpu".into(), "1".into())]),
                            limits: BTreeMap::from([("nvidia.com/gpu".into(), "1".into())]),
                        },
                    }],
                },
            },
        },
    };
    let digest = input
        .embedding
        .runtime
        .profile()
        .contents()
        .runtime_image
        .hex();
    admit_embedding(&embedding, "fixture", digest)?;
    embedding.spec.template.spec.containers[0]
        .resources
        .limits
        .clear();
    ensure!(
        admit_embedding(&embedding, "fixture", digest).is_err(),
        "missingGPU accepted"
    );
    embedding.spec.template.spec.containers[0]
        .resources
        .limits
        .insert("nvidia.com/gpu".into(), "1".into());
    ensure!(
        admit_embedding(&embedding, "fixture", &"b".repeat(64)).is_err(),
        "foreign runtime digest accepted"
    );
    Ok(())
}

#[tokio::test]
async fn cold_start_watch_framing_refuses_gaps_and_preserves_initial_bookmark() -> Result<()> {
    // A real pipe exercises framing and EOF admission; it never contacts Kubernetes.
    let mut command = tokio::process::Command::new("printf");
    command.arg("%s%s").arg(r#"{"type":"BOOKMARK","object":{"metadata":{"resourceVersion":"123","annotations":{"k8s.io/initial-events-end":"true"}}}}"#).arg(r#"{"type":"ERROR","object":{"code":410}}"#).stdout(std::process::Stdio::piped());
    let mut child = veoveo_testing_support::spawn_async(command)?;
    let mut watch = Watch {
        stdout: child.stdout.take().unwrap(),
        pending: vec![],
        bytes: 0,
        events: 0,
        in_progress: None,
    };
    let Event::Bookmark(bookmark) = watch.next().await? else {
        anyhow::bail!("initial bookmark lost");
    };
    ensure!(
        bookmark
            .metadata
            .annotations
            .get("k8s.io/initial-events-end")
            .map(String::as_str)
            == Some("true")
    );
    ensure!(matches!(
        watch.next().await?,
        Event::Error(WatchError { code: 410 })
    ));
    ensure!(watch.next().await.is_err(), "watchEOF accepted");
    ensure!(
        child.wait().await?.success(),
        "controlled watch process failed"
    );
    Ok(())
}

#[tokio::test]
async fn cold_start_retains_watch_event_when_ownership_admission_is_cancelled() -> Result<()> {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    for case in ["fence", "unready", "deleted", "terminal", "restart"] {
        let directory = super::controls::Scratch::new()?;
        let journal =
            receipt::Journal::open(&directory.0.join("outcome.json"), "operator".parse()?)?;
        let mut pod = control_pod(true, 0);
        let image = pod.spec.containers[0].image.clone();
        let mut state = Observation::default();
        let instance = state.pod(&pod, &image, "fixture", false)?.unwrap();
        state.status(&instance, 503)?;
        state.status(&instance, 200)?;
        if case == "fence" {
            pod.metadata.resource_version = "exact-fence".into();
            state.admit_http_fence(&pod, &image, "fixture")?;
            ensure!(state.ready.is_none());
        } else {
            state.admit_http_fence(&pod, &image, "fixture")?;
            ensure!(state.ready.is_some());
            pod.metadata.resource_version = "after-ready".into();
            if case == "unready" {
                pod.status.as_mut().unwrap().conditions[0].status = PodConditionStatus::False;
            } else if case == "restart" {
                pod.status.as_mut().unwrap().container_statuses[0].restart_count = 1;
            }
        }
        // Native pipe supplies an ordinary API frame, not a prefilled event slot.
        let status = &pod.status.as_ref().unwrap().container_statuses[0];
        let event = serde_json::json!({"type":if matches!(case, "deleted" | "terminal") {"DELETED"} else {"MODIFIED"},"object":{
            "metadata":{"name":pod.metadata.name,"namespace":pod.metadata.namespace,"uid":pod.metadata.uid,"resourceVersion":pod.metadata.resource_version},
            "spec":{"containers":[{"name":"knowledge-mcp","image":image}]},
            "status":{"phase":if case=="terminal" {"Succeeded"} else {"Running"},"containerStatuses":[{"name":"knowledge-mcp","containerID":status.container_id,"imageID":status.image_id,"restartCount":status.restart_count,"ready":status.ready}],"conditions":[{"type":"Ready","status":if case=="unready" {"False"} else {"True"}}]}
        }});
        let mut command = tokio::process::Command::new("printf");
        command.arg("%s%s").arg(serde_json::to_string(&event)?).arg(r#"{"type":"BOOKMARK","object":{"metadata":{"resourceVersion":"following-event"}}}"#).stdout(std::process::Stdio::piped());
        let mut child = veoveo_testing_support::spawn_async(command)?;
        let mut watch = Watch {
            stdout: child.stdout.take().unwrap(),
            pending: vec![],
            bytes: 0,
            events: 0,
            in_progress: None,
        };
        let started = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let completed = Arc::new(AtomicUsize::new(0));
        let admit = async |_: &Pod| -> Result<()> {
            started.notify_one();
            release.notified().await;
            completed.fetch_add(1, Ordering::SeqCst);
            Ok(())
        };
        tokio::time::timeout(Duration::from_secs(2),async {
            tokio::select! {
                result=observe_using(&mut watch,&mut state,&journal,&image,"fixture",false,&admit) => anyhow::bail!("ownership did not suspend: {}",result.is_ok()),
                _=started.notified()=>Ok::<_,anyhow::Error>(()),
            }
        }).await??;
        ensure!(
            watch.in_progress.is_some()
                && watch.events == 1
                && completed.load(Ordering::SeqCst) == 0,
            "cancel discarded or prematurely admitted watch event"
        );
        release.notify_one();
        let resumed = tokio::time::timeout(
            Duration::from_secs(2),
            observe_using(
                &mut watch, &mut state, &journal, &image, "fixture", false, &admit,
            ),
        )
        .await?;
        ensure!(
            completed.load(Ordering::SeqCst) == 1 && watch.events == 1,
            "event was reread or admitted twice after cancellation"
        );
        if case == "fence" {
            resumed?;
            ensure!(
                state.http_fence_observed
                    && state.ready.as_ref() == Some(&instance)
                    && watch.in_progress.is_none(),
                "resumed exact fence was lost"
            );
            observe_using(
                &mut watch,
                &mut state,
                &journal,
                &image,
                "fixture",
                false,
                async |_| Ok(()),
            )
            .await?;
            ensure!(
                watch.events == 2 && completed.load(Ordering::SeqCst) == 1,
                "settled fence replayed during following event"
            );
        } else {
            ensure!(
                resumed.is_err() && watch.in_progress.is_some(),
                "cancelled ownership hid postReady regression"
            );
            ensure!(
                state.selected_resource_version.as_deref() == Some("10"),
                "rejected event advanced stable watch fence"
            );
        }
        ensure!(child.wait().await?.success(), "native watch control failed");
    }
    Ok(())
}
