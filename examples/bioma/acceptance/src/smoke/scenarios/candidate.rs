//! Qualify a compiler candidate through its installed NVIDIA runtime and GPU.
//!
//! The additional server is a normal task-runtime replica. Its HTTP listener is
//! private to this harness; Kubernetes workload specifications stay untouched.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use veoveo_testing_support::ChildGuard;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use veoveo_stream_mcp::contract::RunRecordingOutput;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Service {
    Stream,
    Reason,
}

impl Service {
    fn container(self) -> &'static str {
        match self {
            Self::Stream => "stream-mcp",
            Self::Reason => "reason-mcp",
        }
    }
    pub(crate) fn port(self) -> u16 {
        match self {
            Self::Stream => 18797,
            Self::Reason => 18803,
        }
    }
    fn asset_flag(self) -> &'static str {
        match self {
            Self::Stream => "--live-app",
            Self::Reason => "--reason-runner",
        }
    }
}

#[derive(Deserialize)]
struct Deployment {
    spec: DeploymentSpec,
}

#[derive(Deserialize)]
struct DeploymentSpec {
    selector: Selector,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Selector {
    match_labels: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct Pods {
    items: Vec<Pod>,
}

#[derive(Deserialize)]
struct Pod {
    metadata: Metadata,
    spec: PodSpec,
    status: PodStatus,
}

#[derive(Deserialize)]
struct Metadata {
    name: String,
    uid: String,
}

#[derive(Deserialize)]
struct PodSpec {
    containers: Vec<Container>,
}

#[derive(Deserialize)]
struct Container {
    name: String,
    image: String,
    #[serde(default)]
    args: Vec<String>,
    resources: Resources,
}

#[derive(Deserialize)]
struct Resources {
    limits: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PodStatus {
    #[serde(default)]
    container_statuses: Vec<ContainerStatus>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContainerStatus {
    name: String,
    ready: bool,
    restart_count: u64,
    #[serde(rename = "imageID")]
    image_id: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
enum CandidateReceiptFormat {
    #[vocabulary(rename = "veoveo.ai/compiler-acceptance/v4")]
    V4,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Receipt {
    schema: CandidateReceiptFormat,
    pod_uid: String,
    runtime_image: String,
    runtime_image_id: String,
    candidate_sha256: String,
    payload_sha256: String,
    service: Service,
    gpu: String,
    original_restart_count: u64,
    candidate_removed: bool,
    cache_removed: bool,
    outcome: ProbeOutcome,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ProbeOutcome {
    Started,
    StartupVerified,
    GpuQualified {
        result: Box<RunRecordingOutput>,
    },
    ReasonGpuQualified {
        result: Box<veoveo_reason_mcp::contract::AnalyzeRecordingOutput>,
    },
    WorkloadFailed {
        message: String,
    },
}

impl Receipt {
    fn validate(&self) -> Result<()> {
        ensure!(
            matches!(
                (&self.service, &self.outcome),
                (Service::Stream, ProbeOutcome::GpuQualified { .. })
                    | (Service::Reason, ProbeOutcome::ReasonGpuQualified { .. })
                    | (
                        _,
                        ProbeOutcome::Started
                            | ProbeOutcome::StartupVerified
                            | ProbeOutcome::WorkloadFailed { .. }
                    )
            ),
            "candidate outcome does not belong to the selected service"
        );
        Ok(())
    }
    #[cfg(test)]
    fn decode(bytes: &[u8]) -> Result<Self> {
        let receipt: Self = serde_json::from_slice(bytes)?;
        receipt.validate()?;
        Ok(receipt)
    }
}

pub(crate) struct Candidate {
    context: String,
    service: Service,
    namespace: String,
    pod: String,
    remote: String,
    remote_app: String,
    remote_cache: String,
    process: Option<ChildGuard>,
    cleanup: Arc<RemoteCandidate>,
    cleanup_registration: Option<veoveo_testing_support::lifecycle::owner::CleanupRegistration>,
    receipt: Receipt,
    deployment_spec: Vec<u8>,
    removed: bool,
}

impl Candidate {
    pub(crate) fn start(
        context: &str,
        namespace: &str,
        service: Service,
        binary: &Path,
        asset: &Path,
        work_dir: &Path,
    ) -> Result<Self> {
        let deployment: Deployment =
            serde_json::from_slice(&checked(CandidateCommand::new("kubectl").args([
                "--context",
                context,
                "--request-timeout=30s",
                "-n",
                namespace,
                "get",
                &format!("deployment/{}", service.container()),
                "-o",
                "json",
            ]))?)?;
        let selector = deployment
            .spec
            .selector
            .match_labels
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join(",");
        ensure!(
            !selector.is_empty(),
            "candidate Deployment has no label selector"
        );
        let pods: Pods =
            serde_json::from_slice(&checked(CandidateCommand::new("kubectl").args([
                "--context",
                context,
                "--request-timeout=30s",
                "-n",
                namespace,
                "get",
                "pods",
                "-l",
                &selector,
                "-o",
                "json",
            ]))?)?;
        let pod = pods
            .items
            .into_iter()
            .find(|pod| {
                pod.status
                    .container_statuses
                    .iter()
                    .any(|container| container.name == service.container() && container.ready)
            })
            .context("candidate has no ready runtime container")?;
        let container = pod
            .spec
            .containers
            .iter()
            .find(|container| container.name == service.container())
            .context("candidate runtime container is missing")?;
        let status = pod
            .status
            .container_statuses
            .iter()
            .find(|container| container.name == service.container())
            .context("candidate runtime status is missing")?;
        ensure!(
            container
                .resources
                .limits
                .get("nvidia.com/gpu")
                .and_then(|value| value.parse::<u32>().ok())
                .is_some_and(|count| count > 0),
            "candidate qualification requires the candidate container's NVIDIA GPU resource"
        );
        let candidate_sha256 = format!("sha256:{}", hex::encode(Sha256::digest(fs::read(binary)?)));
        let deployment_spec = deployment_spec(context, namespace, service)?;
        let remote = format!("/tmp/veoveo-compiler-{}", uuid::Uuid::new_v4().simple());
        let remote_app = format!("{remote}-payload");
        let remote_cache = format!("{remote}-cache");
        let mut arguments = candidate_arguments(service, &container.args, &remote_cache)?;
        arguments.extend([service.asset_flag().to_owned(), remote_app.clone()]);
        let remote_cleanup = Arc::new(RemoteCandidate {
            context: context.to_owned(),
            namespace: namespace.to_owned(),
            service,
            pod: pod.metadata.name.clone(),
            pod_uid: pod.metadata.uid.clone(),
            remote: remote.clone(),
            remote_app: remote_app.clone(),
            remote_cache: remote_cache.clone(),
            group: Mutex::new(None),
            settled: AtomicBool::new(false),
            launched: AtomicBool::new(false),
        });
        let mut candidate = Self {
            context: context.to_owned(),
            service,
            namespace: namespace.to_owned(),
            pod: pod.metadata.name,
            remote,
            remote_app,
            remote_cache,
            process: None,
            cleanup: Arc::clone(&remote_cleanup),
            cleanup_registration: None,
            receipt: Receipt {
                schema: CandidateReceiptFormat::V4,
                pod_uid: pod.metadata.uid,
                runtime_image: container.image.clone(),
                runtime_image_id: status.image_id.clone(),
                candidate_sha256,
                payload_sha256: format!("sha256:{}", hex::encode(Sha256::digest(fs::read(asset)?))),
                service,
                gpu: String::new(),
                original_restart_count: status.restart_count,
                candidate_removed: false,
                cache_removed: false,
                outcome: ProbeOutcome::Started,
            },
            deployment_spec,
            removed: false,
        };
        let cleanup = Arc::clone(&remote_cleanup);
        candidate.cleanup_registration =
            Some(veoveo_testing_support::lifecycle::owner::register_cleanup(
                veoveo_testing_support::lifecycle::owner::CleanupKind::Remote,
                "compiler_candidate",
                &serde_json::to_string(&CandidateCleanupIdentity {
                    context,
                    namespace,
                    pod: &candidate.pod,
                    pod_uid: &candidate.receipt.pod_uid,
                    executable: &candidate.remote,
                    payload: &candidate.remote_app,
                    cache: &candidate.cleanup.remote_cache,
                })?,
                move || async move { cleanup.remove() },
            )?);
        // Verify the actual target Python/kernel cleanup API before any candidate
        // files or process effects exist. Post-dispatch loss stays unresolved.
        checked(candidate.exec().args([
            "python3",
            "-c",
            include_str!("candidate/launch.py"),
            "--prerequisite",
        ]))?;
        candidate.receipt.gpu = String::from_utf8(checked(candidate.exec().args([
            "nvidia-smi",
            "--query-gpu=name,uuid,driver_version",
            "--format=csv,noheader",
        ]))?)?
        .trim()
        .to_owned();
        ensure!(
            !candidate.receipt.gpu.is_empty(),
            "NVIDIA hardware probe returned no GPU"
        );
        checked(
            candidate
                .exec()
                .args(["mkdir", "-m", "0700", &candidate.remote_cache]),
        )?;
        let mut copy = CandidateCommand::new("kubectl");
        copy.args([
            "--context",
            context,
            "--request-timeout=30s",
            "-n",
            namespace,
            "exec",
            "-i",
            &candidate.pod,
            "-c",
            service.container(),
            "--",
            "tee",
            &candidate.remote,
        ])
        .stdin(File::open(binary)?)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
        checked(&mut copy)?;
        checked(candidate.exec().args(["chmod", "0700", &candidate.remote]))?;
        let remote_hash = String::from_utf8(checked(
            candidate.exec().args(["sha256sum", &candidate.remote]),
        )?)?;
        ensure!(
            remote_hash.split_whitespace().next()
                == candidate.receipt.candidate_sha256.strip_prefix("sha256:"),
            "candidate copy differs from local binary"
        );
        let mut copy_app = CandidateCommand::new("kubectl");
        copy_app
            .args([
                "--context",
                context,
                "--request-timeout=30s",
                "-n",
                namespace,
                "exec",
                "-i",
                &candidate.pod,
                "-c",
                service.container(),
                "--",
                "tee",
                &candidate.remote_app,
            ])
            .stdin(File::open(asset)?)
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        checked(&mut copy_app)?;
        let app_hash = String::from_utf8(checked(
            candidate.exec().args(["sha256sum", &candidate.remote_app]),
        )?)?;
        ensure!(
            app_hash.split_whitespace().next()
                == candidate.receipt.payload_sha256.strip_prefix("sha256:"),
            "candidate payload copy differs from local asset"
        );
        if matches!(service, Service::Reason) {
            checked(
                candidate
                    .exec()
                    .args(["chmod", "0700", &candidate.remote_app]),
            )?;
        }
        fs::create_dir_all(work_dir)?;
        let log = File::create(work_dir.join("compiler-candidate.log"))?;
        let launch_receipt = format!("{}/launch.json", candidate.remote_cache);
        let invocation = uuid::Uuid::new_v4().to_string();
        candidate.cleanup.launched.store(true, Ordering::Release);
        candidate.process = Some(
            candidate
                .exec()
                .arg("python3")
                .arg("-c")
                .arg(include_str!("candidate/launch.py"))
                .arg(&launch_receipt)
                .arg(&invocation)
                .arg(&candidate.remote)
                .args(arguments)
                .stdout(log.try_clone()?)
                .stderr(log)
                .spawn()?,
        );
        let deadline = Instant::now() + Duration::from_secs(10);

        loop {
            if let Some(exit) = candidate
                .process
                .as_mut()
                .context("candidate process missing")?
                .try_wait()?
            {
                anyhow::bail!(
                    "compiler candidate exited ({exit}): {}",
                    fs::read_to_string(work_dir.join("compiler-candidate.log"))?
                );
            }
            let running = candidate.exec().args(["cat", &launch_receipt]).output()?;
            if running.status.success() {
                let receipt: CandidateLaunch = serde_json::from_slice(&running.stdout)?;
                ensure!(
                    receipt.format == CandidateLaunchFormat::V1 && receipt.invocation == invocation,
                    "candidate launch receipt belongs to another invocation"
                );
                ensure!(
                    receipt.process_group > 0 && receipt.start_ticks > 0,
                    "candidate launch identity missing"
                );
                let group = receipt.process_group;
                let start_ticks = receipt.start_ticks;
                *candidate
                    .cleanup
                    .group
                    .lock()
                    .expect("candidate group identity") = Some((group, start_ticks));
                candidate
                    .cleanup_registration
                    .as_ref()
                    .context("missing candidate cleanup registration")?
                    .observed_identity(&serde_json::to_string(&CandidateProcessIdentity {
                        process_group: group,
                        start_ticks,
                    })?)?;
                break;
            }
            ensure!(
                Instant::now() < deadline,
                "compiler candidate process did not start"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok(candidate)
    }

    pub(crate) fn resource(&self) -> String {
        format!("pod/{}", self.pod)
    }

    pub(crate) fn check_running(&mut self, work_dir: &Path) -> Result<()> {
        if let Some(exit) = self
            .process
            .as_mut()
            .context("candidate process missing")?
            .try_wait()?
        {
            anyhow::bail!(
                "compiler candidate exited ({exit}): {}",
                fs::read_to_string(work_dir.join("compiler-candidate.log"))?
            );
        }
        Ok(())
    }

    pub(crate) fn verify_listener(&self) -> Result<()> {
        let (pid, _) = self
            .cleanup
            .group
            .lock()
            .expect("candidate group identity")
            .context("candidate process identity is missing")?;
        let hash = String::from_utf8(checked(
            self.exec().args(["sha256sum", &format!("/proc/{pid}/exe")]),
        )?)?;
        ensure!(
            hash.split_whitespace().next() == self.receipt.candidate_sha256.strip_prefix("sha256:"),
            "listening candidate executable differs from the admitted binary"
        );
        let sockets = String::from_utf8(checked(self.exec().args([
            "cat",
            &format!("/proc/{pid}/net/tcp"),
            &format!("/proc/{pid}/net/tcp6"),
        ]))?)?;
        let descriptors = String::from_utf8(checked(self.exec().args([
            "ls",
            "-l",
            &format!("/proc/{pid}/fd"),
        ]))?)?;
        ensure!(
            owns_listener(&sockets, &descriptors, self.service.port()),
            "the candidate process does not own the accepted HTTP listener"
        );
        Ok(())
    }

    fn exec(&self) -> CandidateCommand {
        let mut command = CandidateCommand::new("kubectl");
        command.args([
            "--context",
            &self.context,
            "--request-timeout=30s",
            "-n",
            &self.namespace,
            "exec",
            &self.pod,
            "-c",
            self.service.container(),
            "--",
        ]);
        command
    }

    fn remove(&mut self) -> Result<()> {
        if self.removed {
            return Ok(());
        }
        self.cleanup.remove()?;
        if let Some(mut process) = self.process.take() {
            let end = veoveo_testing_support::lifecycle::owner::cleanup_deadline()?;
            loop {
                if process.try_wait()?.is_some() {
                    break;
                }
                ensure!(
                    Instant::now() < end,
                    "candidate local execution drain unresolved"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        self.cleanup_registration
            .as_ref()
            .context("missing candidate cleanup registration")?
            .settled()?;
        self.removed = true;
        self.receipt.candidate_removed = true;
        self.receipt.cache_removed = true;
        Ok(())
    }

    pub(crate) fn finish(&mut self, work_dir: &Path, outcome: ProbeOutcome) -> Result<()> {
        self.verify_listener()?;
        self.remove()?;
        ensure!(
            deployment_spec(&self.context, &self.namespace, self.service)? == self.deployment_spec,
            "candidate Deployment changed during qualification"
        );
        let pod: Pod = serde_json::from_slice(&checked(CandidateCommand::new("kubectl").args([
            "--context",
            &self.context,
            "--request-timeout=30s",
            "-n",
            &self.namespace,
            "get",
            &self.resource(),
            "-o",
            "json",
        ]))?)?;
        let status = pod
            .status
            .container_statuses
            .iter()
            .find(|container| container.name == self.service.container())
            .context("candidate runtime status disappeared")?;
        ensure!(
            pod.metadata.uid == self.receipt.pod_uid
                && status.ready
                && status.restart_count == self.receipt.original_restart_count
                && status.image_id == self.receipt.runtime_image_id,
            "original candidate runtime changed during qualification"
        );
        self.receipt.outcome = outcome;
        self.receipt.validate()?;
        fs::write(
            work_dir.join("compiler-candidate.json"),
            serde_json::to_vec_pretty(&self.receipt)?,
        )?;
        Ok(())
    }
}

impl Drop for Candidate {
    fn drop(&mut self) {
        // The registered owner action reconciles remote effects after an interrupted
        // operation. ChildGuard independently retains local group ownership.
        if !self.removed && !veoveo_testing_support::lifecycle::owner::has_active_owner() {
            eprintln!(
                "compiler candidate cleanup unresolved for {}:{}",
                self.pod, self.remote
            );
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CandidateCleanupIdentity<'a> {
    context: &'a str,
    namespace: &'a str,
    pod: &'a str,
    pod_uid: &'a str,
    executable: &'a str,
    payload: &'a str,
    cache: &'a str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CandidateProcessIdentity {
    process_group: u32,
    start_ticks: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
enum CandidateLaunchFormat {
    #[vocabulary(rename = "veoveo.ai/candidate-launch/v1")]
    V1,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CandidateLaunch {
    format: CandidateLaunchFormat,
    invocation: String,
    process_group: u32,
    start_ticks: u64,
}

struct RemoteCandidate {
    context: String,
    namespace: String,
    service: Service,
    pod: String,
    pod_uid: String,
    remote: String,
    remote_app: String,
    remote_cache: String,
    group: Mutex<Option<(u32, u64)>>,
    settled: AtomicBool,
    launched: AtomicBool,
}
impl RemoteCandidate {
    fn cleanup_exec(&self) -> CandidateCommand {
        let mut command = CandidateCommand::new("kubectl");
        command.cleanup = true;
        command.args([
            "--context",
            &self.context,
            "--request-timeout=30s",
            "-n",
            &self.namespace,
            "exec",
            &self.pod,
            "-c",
            self.service.container(),
            "--",
        ]);
        command
    }
    fn remove(&self) -> Result<()> {
        if self.settled.load(Ordering::Acquire) {
            return Ok(());
        }
        let cleanup_end = veoveo_testing_support::lifecycle::owner::cleanup_deadline()?;
        let mut observed = CandidateCommand::new("kubectl");
        observed.cleanup = true;
        let pod: Pod = serde_json::from_slice(&checked(observed.args([
            "--context",
            &self.context,
            "--request-timeout=30s",
            "-n",
            &self.namespace,
            "get",
            "pod",
            &self.pod,
            "-o",
            "json",
        ]))?)?;
        ensure!(
            pod.metadata.uid == self.pod_uid,
            "candidate runtime identity changed; refuse remote cleanup"
        );
        if self.launched.load(Ordering::Acquire)
            && self
                .group
                .lock()
                .expect("candidate group identity")
                .is_none()
        {
            bail!("candidate dispatch lacks a confirmed process identity; retain remote intent");
        }
        let original_group = *self.group.lock().expect("candidate group identity");
        if let Some((group, start_ticks)) = original_group {
            let stat = String::from_utf8(checked(
                self.cleanup_exec()
                    .args(["cat", &format!("/proc/{group}/stat")]),
            )?)?;
            ensure!(
                process_start_ticks(&stat)? == start_ticks,
                "candidate remote process identity changed; refuse cleanup"
            );
            let remaining = cleanup_end.saturating_duration_since(Instant::now());
            ensure!(!remaining.is_zero(), "candidate cleanup deadline expired");
            // The helper holds Linux pidfds through verification and signalling.
            // Neither member nor leader signals use reusable numeric PID targets.
            checked(self.cleanup_exec().args([
                "python3",
                "-c",
                include_str!("candidate/cleanup.py"),
                &group.to_string(),
                &start_ticks.to_string(),
                &remaining.as_secs_f64().to_string(),
            ]))?;
        }
        checked(
            self.cleanup_exec()
                .args(["rm", "-f", &self.remote, &self.remote_app]),
        )?;
        let absent = self
            .cleanup_exec()
            .args(["test", "!", "-e", &self.remote])
            .output()?;
        ensure!(
            absent.status.success(),
            "compiler candidate file survived cleanup"
        );
        ensure!(
            self.cleanup_exec()
                .args(["test", "!", "-e", &self.remote_app])
                .output()?
                .status
                .success(),
            "candidate payload file survived cleanup"
        );
        checked(
            self.cleanup_exec()
                .args(["rm", "-rf", "--", &self.remote_cache]),
        )?;
        ensure!(
            self.cleanup_exec()
                .args(["test", "!", "-e", &self.remote_cache])
                .output()?
                .status
                .success(),
            "candidate recording cache survived cleanup"
        );
        self.settled.store(true, Ordering::Release);
        Ok(())
    }
}

fn process_start_ticks(stat: &str) -> Result<u64> {
    stat.rsplit_once(") ")
        .context("malformed candidate process identity")?
        .1
        .split_whitespace()
        .nth(19)
        .context("missing candidate process start time")?
        .parse()
        .context("invalid candidate process start time")
}

/// One-shot owner command builder; the configured native Command moves into the shared gate.
struct CandidateCommand {
    command: Option<Command>,
    cleanup: bool,
    configured_stdout: bool,
}
impl CandidateCommand {
    fn new(program: &str) -> Self {
        Self {
            command: Some(Command::new(program)),
            cleanup: false,
            configured_stdout: false,
        }
    }
    fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        self.command
            .as_mut()
            .expect("unconsumed candidate command")
            .args(args);
        self
    }
    fn arg(&mut self, arg: impl AsRef<std::ffi::OsStr>) -> &mut Self {
        self.command
            .as_mut()
            .expect("unconsumed candidate command")
            .arg(arg);
        self
    }
    fn stdin(&mut self, value: impl Into<Stdio>) -> &mut Self {
        self.command
            .as_mut()
            .expect("unconsumed candidate command")
            .stdin(value);
        self
    }
    fn stdout(&mut self, value: impl Into<Stdio>) -> &mut Self {
        self.configured_stdout = true;
        self.command
            .as_mut()
            .expect("unconsumed candidate command")
            .stdout(value);
        self
    }
    fn stderr(&mut self, value: impl Into<Stdio>) -> &mut Self {
        self.command
            .as_mut()
            .expect("unconsumed candidate command")
            .stderr(value);
        self
    }
    fn output(&mut self) -> Result<Output> {
        let command = self
            .command
            .take()
            .context("candidate command already consumed")?;
        if self.configured_stdout {
            veoveo_testing_support::output_command_with_configured_stdout(command, self.cleanup)
        } else if self.cleanup {
            veoveo_testing_support::output_cleanup_command(command)
        } else {
            veoveo_testing_support::output_command(command)
        }
    }
    fn spawn(&mut self) -> Result<ChildGuard> {
        ChildGuard::from_command(
            self.command
                .take()
                .context("candidate command already consumed")?,
        )
    }
}

fn checked(command: &mut CandidateCommand) -> Result<Vec<u8>> {
    let Output {
        status,
        stdout,
        stderr,
    } = command.output()?;
    ensure!(
        status.success(),
        "runtime qualification command failed ({status}): {}",
        String::from_utf8_lossy(&stderr)
    );
    Ok(stdout)
}

fn deployment_spec(context: &str, namespace: &str, service: Service) -> Result<Vec<u8>> {
    checked(CandidateCommand::new("kubectl").args([
        "--context",
        context,
        "--request-timeout=30s",
        "-n",
        namespace,
        "get",
        &format!("deployment/{}", service.container()),
        "-o",
        "jsonpath={.spec}",
    ]))
}

fn candidate_arguments(service: Service, original: &[String], cache: &str) -> Result<Vec<String>> {
    let mut arguments = Vec::new();
    let mut original = original.iter();
    while let Some(argument) = original.next() {
        if ["--port", service.asset_flag(), "--catalog-cache-dir"].contains(&argument.as_str()) {
            original
                .next()
                .with_context(|| format!("installed candidate {argument} argument has no value"))?;
        } else if ![
            "--port=".to_owned(),
            format!("{}=", service.asset_flag()),
            "--catalog-cache-dir=".to_owned(),
        ]
        .iter()
        .any(|prefix| argument.starts_with(prefix.as_str()))
        {
            arguments.push(argument.clone());
        }
    }
    arguments.extend([
        "--port".to_owned(),
        service.port().to_string(),
        "--catalog-cache-dir".to_owned(),
        cache.to_owned(),
    ]);
    Ok(arguments)
}

fn owns_listener(sockets: &str, descriptors: &str, port: u16) -> bool {
    sockets.lines().any(|line| {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        let Some(address) = fields.get(1) else {
            return false;
        };
        let Some((_, encoded_port)) = address.rsplit_once(':') else {
            return false;
        };
        fields.get(3) == Some(&"0A")
            && u16::from_str_radix(encoded_port, 16).ok() == Some(port)
            && fields.get(9).is_some_and(|inode| {
                descriptors
                    .lines()
                    .any(|descriptor| descriptor.ends_with(&format!(" -> socket:[{inode}]")))
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const PORT: u16 = 18797;

    #[test]
    fn remote_cleanup_rejects_replaced_pod_or_process_before_any_mutation() {
        const CHILD: &str = "VEOVEO_TEST_CANDIDATE_REPLACEMENT";
        if let Ok(case) = std::env::var(CHILD) {
            let root =
                std::path::PathBuf::from(std::env::var_os("VEOVEO_SMOKE_LOCAL_GROUPS").unwrap());
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime
                .block_on(veoveo_testing_support::lifecycle::owner::run(async {
                    let candidate = RemoteCandidate {
                        context: "fixture".into(),
                        namespace: "fixture".into(),
                        service: Service::Stream,
                        pod: "candidate-pod".into(),
                        pod_uid: "original-pod".into(),
                        remote: "/tmp/private-candidate".into(),
                        remote_app: "/tmp/private-app".into(),
                        remote_cache: "/tmp/private-cache".into(),
                        group: Mutex::new(Some((42, 10))),
                        settled: AtomicBool::new(false),
                        launched: AtomicBool::new(true),
                    };
                    let error = candidate.remove().unwrap_err();
                    ensure!(
                        error.to_string().contains(if case == "pod" {
                            "runtime identity changed"
                        } else {
                            "process identity changed"
                        }),
                        "wrong refusal: {error}"
                    );
                    ensure!(
                        !candidate.settled.load(Ordering::Acquire),
                        "replacement became settled"
                    );
                    ensure!(
                        !root.join("mutated").exists(),
                        "replacement received cleanup mutation"
                    );
                    Ok(())
                }))
                .unwrap();
            return;
        }
        for case in ["pod", "process"] {
            let root = tempfile::tempdir().unwrap();
            let bin = root.path().join("bin");
            fs::create_dir(&bin).unwrap();
            let kubectl = bin.join("kubectl");
            let stat = format!(
                "42 (candidate) {}",
                [vec!["S"], vec!["0"; 18], vec!["99"]].concat().join(" ")
            );
            let script = format!(
                r#"#!/bin/sh
case " $* " in
*" get pod "*) if [ "$VEOVEO_TEST_CANDIDATE_REPLACEMENT" = pod ]; then uid=replaced-pod; else uid=original-pod; fi; printf '{{"metadata":{{"name":"candidate-pod","uid":"%s"}},"spec":{{"containers":[]}},"status":{{}}}}\n' "$uid";;
*" cat /proc/42/stat "*) printf '%s\n' '{stat}';;
*" kill "*|*" pkill "*|*" rm "*) printf mutation > "$VEOVEO_SMOKE_LOCAL_GROUPS/mutated"; exit 0;;
*) exit 2;;
esac
"#
            );
            fs::write(&kubectl, script).unwrap();
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&kubectl, fs::Permissions::from_mode(0o700)).unwrap();
            let mut paths = vec![bin];
            paths.extend(std::env::split_paths(
                &std::env::var_os("PATH").unwrap_or_default(),
            ));
            let mut command = Command::new(std::env::current_exe().unwrap());
            command.args(["--exact", "candidate::tests::remote_cleanup_rejects_replaced_pod_or_process_before_any_mutation", "--nocapture"])
                .env(CHILD, case).env("PATH", std::env::join_paths(paths).unwrap())
                .env("VEOVEO_SMOKE_LOCAL_GROUPS", root.path());
            let mut child = ChildGuard::from_command(command)
                .unwrap()
                .with_drain_on_drop(Duration::ZERO);
            let end = Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(status) = child.try_wait().unwrap() {
                    assert!(
                        status.success(),
                        "Candidate {case} replacement control failed"
                    );
                    break;
                }
                assert!(
                    Instant::now() < end,
                    "Candidate {case} control did not finish"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }

    #[test]
    fn candidate_isolates_listener_and_cache_preserving_site_limits() {
        let args = [
            "--port",
            "8797",
            "--allowed-host",
            "stream-mcp:8797",
            "--pipeline-catalog",
            "/site/catalog.json",
            "--catalog-cache-dir",
            "/recording-cache",
            "--catalog-cache-managed-bytes",
            "1073741824",
        ]
        .map(str::to_owned);
        assert_eq!(
            candidate_arguments(Service::Stream, &args, "/tmp/candidate-cache").unwrap(),
            [
                "--allowed-host",
                "stream-mcp:8797",
                "--pipeline-catalog",
                "/site/catalog.json",
                "--catalog-cache-managed-bytes",
                "1073741824",
                "--port",
                "18797",
                "--catalog-cache-dir",
                "/tmp/candidate-cache"
            ]
        );
        assert_eq!(
            candidate_arguments(
                Service::Stream,
                &[
                    "--port=8797".into(),
                    "--catalog-cache-dir=/recording-cache".into()
                ],
                "/tmp/candidate-cache"
            )
            .unwrap(),
            [
                "--port",
                "18797",
                "--catalog-cache-dir",
                "/tmp/candidate-cache"
            ]
        );
        for flag in ["--port", "--live-app", "--catalog-cache-dir"] {
            assert!(
                candidate_arguments(Service::Stream, &[flag.into()], "/tmp/candidate-cache")
                    .is_err()
            );
        }
    }

    #[test]
    fn reason_candidate_replaces_runner_and_isolates_its_listener() {
        let args = [
            "--port=8803",
            "--reason-runner=/usr/local/bin/reason-runner",
            "--pipeline-catalog",
            "/site/catalog.json",
            "--catalog-cache-dir",
            "/recording-cache",
            "--catalog-cache-managed-bytes=8589934592",
        ]
        .map(str::to_owned);
        assert_eq!(
            candidate_arguments(Service::Reason, &args, "/tmp/candidate-cache").unwrap(),
            [
                "--pipeline-catalog",
                "/site/catalog.json",
                "--catalog-cache-managed-bytes=8589934592",
                "--port",
                "18803",
                "--catalog-cache-dir",
                "/tmp/candidate-cache",
            ]
        );
        assert!(
            candidate_arguments(Service::Reason, &["--reason-runner".into()], "/tmp/cache")
                .is_err()
        );
    }

    #[test]
    fn another_process_listener_cannot_accept_the_candidate() {
        let sockets = "0: 00000000:496D 00000000:0000 0A 0:0 00:0 00000000 10001 0 12345";
        let owned = "lrwx------ 1 user user 64 Sep 8 12:00 42 -> socket:[12345]";
        assert!(owns_listener(sockets, owned, PORT));
        assert!(!owns_listener(sockets, "42 -> socket:[98765]", PORT));
        assert!(!owns_listener(sockets, owned, 8797));
        assert!(!owns_listener(
            &sockets.replace(" 0A ", " 01 "),
            owned,
            PORT
        ));
    }
    #[test]
    fn retained_pidfd_refuses_a_foreign_child_and_missing_platform_support() {
        // Real local process handles exercise the production cleanup helper.
        // The injected member listing models replacement between enumeration
        // and handle admission; the foreign process must receive no signal.
        let script = format!(
            r#"
namespace = {{"__name__": "candidate_control"}}
exec({helper:?}, namespace)
import subprocess, os, signal
leader = subprocess.Popen(["sleep", "30"], start_new_session=True)
foreign = subprocess.Popen(["sleep", "30"], start_new_session=True)
try:
    ticks = namespace["identity"](leader.pid)[1]
    namespace["members"] = lambda group: [leader.pid, foreign.pid]
    try:
        namespace["stop"](leader.pid, ticks, 1.0)
    except RuntimeError as error:
        assert "child identity changed" in str(error), str(error)
    else:
        raise AssertionError("foreign child was admitted")
    assert leader.poll() is None
    assert foreign.poll() is None
    original = os.pidfd_open
    del os.pidfd_open
    try:
        try:
            namespace["stop"](leader.pid, ticks, 1.0)
        except RuntimeError as error:
            assert "requires Linux pidfd support" in str(error), str(error)
        else:
            raise AssertionError("missing pidfd support admitted")
    finally:
        os.pidfd_open = original
finally:
    for process in [foreign, leader]:
        process.terminate()
        process.wait(timeout=2)
"#,
            helper = include_str!("candidate/cleanup.py")
        );
        let output = std::process::Command::new("python3")
            .args(["-c", &script])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "pidfd control failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    #[test]
    fn candidate_launch_receipt_binds_the_exact_exec_process_before_cleanup() {
        let script = format!(
            r#"
import subprocess, tempfile, json, sys, os, time
namespace = {{"__name__": "candidate_control"}}
exec({cleanup:?}, namespace)
launch = {launch:?}
subprocess.run([sys.executable, "-c", launch, "--prerequisite"], check=True, timeout=2)
with tempfile.TemporaryDirectory() as root:
    path = root + "/launch.json"
    process = subprocess.Popen([sys.executable, "-c", launch, path, "owned-invocation", "/bin/sleep", "30"])
    try:
        end = time.monotonic() + 2
        while not os.path.exists(path):
            assert process.poll() is None, "launch failed before receipt"
            assert time.monotonic() < end, "launch receipt deadline"
            time.sleep(0.01)
        with open(path, encoding="utf-8") as source:
            receipt = json.load(source)
        assert set(receipt) == {{"format", "invocation", "processGroup", "startTicks"}}
        assert receipt["format"] == "veoveo.ai/candidate-launch/v1"
        assert receipt["invocation"] == "owned-invocation"
        assert receipt["processGroup"] == process.pid
        assert namespace["identity"](process.pid)[:2] == (process.pid, receipt["startTicks"])
        namespace["stop"](receipt["processGroup"], receipt["startTicks"], 2)
        process.wait(timeout=2)
    finally:
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=2)
"#,
            cleanup = include_str!("candidate/cleanup.py"),
            launch = include_str!("candidate/launch.py")
        );
        let output = std::process::Command::new("python3")
            .args(["-c", &script])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "launch ownership control failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    #[test]
    fn candidate_receipt_current_complete_outcomes_refuse_retired_children() {
        let stream: RunRecordingOutput = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../servers/stream-mcp/testdata/run-output.json"
        )))
        .unwrap();
        let reason: veoveo_reason_mcp::contract::AnalyzeRecordingOutput = serde_json::from_slice(
            &fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../servers/reason-mcp/testdata/analysis-output-v1.json"
            ))
            .unwrap(),
        )
        .unwrap();
        for (service, outcome) in [
            (
                Service::Stream,
                ProbeOutcome::GpuQualified {
                    result: Box::new(stream),
                },
            ),
            (
                Service::Reason,
                ProbeOutcome::ReasonGpuQualified {
                    result: Box::new(reason),
                },
            ),
        ] {
            let receipt = Receipt {
                schema: CandidateReceiptFormat::V4,
                pod_uid: "selected-pod".into(),
                runtime_image: "selected-runtime".into(),
                runtime_image_id: "selected-image-id".into(),
                candidate_sha256: "a".repeat(64),
                payload_sha256: "b".repeat(64),
                service,
                gpu: "selected-gpu".into(),
                original_restart_count: 0,
                candidate_removed: true,
                cache_removed: true,
                outcome,
            };
            let current = serde_json::to_value(&receipt).unwrap();
            Receipt::decode(&serde_json::to_vec(&current).unwrap()).unwrap();
            let mut retired = current.clone();
            retired["schema"] = serde_json::json!("veoveo.ai/compiler-acceptance/v3");
            assert!(Receipt::decode(&serde_json::to_vec(&retired).unwrap()).is_err());
            for (path, old) in [
                ("/podUid", "pod_uid"),
                ("/outcome/result/resultUri", "result_uri"),
                ("/outcome/result/resultsArtifact/byteLen", "byte_len"),
            ] {
                for mixed in [false, true] {
                    let mut bad = current.clone();
                    let (parent, field) = path.rsplit_once('/').unwrap();
                    let object = bad.pointer_mut(parent).unwrap().as_object_mut().unwrap();
                    let value = object[field].clone();
                    if !mixed {
                        object.remove(field);
                    }
                    object.insert(old.to_owned(), value);
                    assert!(
                        Receipt::decode(&serde_json::to_vec(&bad).unwrap()).is_err(),
                        "{path} mixed={mixed}"
                    );
                }
            }
            if service == Service::Stream {
                let mut wrong_parent = current.clone();
                wrong_parent["outcome"]["result"]["resultUri"] =
                    serde_json::to_value(veoveo_stream_mcp::contract::RunResultsUri::new(
                        veoveo_stream_mcp::contract::RunId::try_from(veoveo_types::TaskId::new())
                            .unwrap(),
                    ))
                    .unwrap();
                assert!(Receipt::decode(&serde_json::to_vec(&wrong_parent).unwrap()).is_err());
            }
            for outcome in [
                ProbeOutcome::Started,
                ProbeOutcome::StartupVerified,
                ProbeOutcome::WorkloadFailed {
                    message: "bounded diagnostic".into(),
                },
            ] {
                let report = Receipt {
                    schema: CandidateReceiptFormat::V4,
                    pod_uid: "selected-pod".into(),
                    runtime_image: "selected-runtime".into(),
                    runtime_image_id: "selected-image-id".into(),
                    candidate_sha256: "a".repeat(64),
                    payload_sha256: "b".repeat(64),
                    service,
                    gpu: "selected-gpu".into(),
                    original_restart_count: 0,
                    candidate_removed: true,
                    cache_removed: true,
                    outcome,
                };
                Receipt::decode(&serde_json::to_vec(&report).unwrap()).unwrap();
            }
            let mut wrong_service = current;
            wrong_service["service"] = serde_json::json!(if service == Service::Stream {
                "reason"
            } else {
                "stream"
            });
            assert!(Receipt::decode(&serde_json::to_vec(&wrong_service).unwrap()).is_err());
        }
    }
}
