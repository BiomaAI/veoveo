//! Qualify a compiler candidate through the installed Stream runtime and GPU.
//!
//! The additional server is a normal task-runtime replica. Its HTTP listener is
//! private to this harness; Kubernetes workload specifications stay untouched.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use veoveo_stream_mcp::contract::RunRecordingOutput;

pub(super) const PORT: u16 = 18797;
const CONTAINER: &str = "stream-mcp";

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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Receipt {
    schema: &'static str,
    pod_uid: String,
    runtime_image: String,
    runtime_image_id: String,
    candidate_sha256: String,
    app_sha256: String,
    gpu: String,
    processed_frames: u64,
    detection_count: u64,
    original_restart_count: u64,
    candidate_removed: bool,
    outcome: ProbeOutcome,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum ProbeOutcome {
    Started,
    StartupVerified,
    GpuQualified { result: Box<RunRecordingOutput> },
    WorkloadFailed { message: String },
}

pub(super) struct Candidate {
    namespace: String,
    pod: String,
    remote: String,
    remote_app: String,
    process: Option<Child>,
    process_group: Option<u32>,
    receipt: Receipt,
    deployment_spec: Vec<u8>,
    removed: bool,
}

impl Candidate {
    pub(super) fn start(
        namespace: &str,
        binary: &Path,
        app: &Path,
        work_dir: &Path,
    ) -> Result<Self> {
        let deployment: Deployment =
            serde_json::from_slice(&checked(Command::new("kubectl").args([
                "-n",
                namespace,
                "get",
                "deployment/stream-mcp",
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
            "Stream Deployment has no label selector"
        );
        let pods: Pods = serde_json::from_slice(&checked(Command::new("kubectl").args([
            "-n", namespace, "get", "pods", "-l", &selector, "-o", "json",
        ]))?)?;
        let pod = pods
            .items
            .into_iter()
            .find(|pod| {
                pod.status
                    .container_statuses
                    .iter()
                    .any(|container| container.name == CONTAINER && container.ready)
            })
            .context("Stream has no ready runtime container")?;
        let container = pod
            .spec
            .containers
            .iter()
            .find(|container| container.name == CONTAINER)
            .context("Stream runtime container is missing")?;
        let status = pod
            .status
            .container_statuses
            .iter()
            .find(|container| container.name == CONTAINER)
            .context("Stream runtime status is missing")?;
        ensure!(
            container
                .resources
                .limits
                .get("nvidia.com/gpu")
                .and_then(|value| value.parse::<u32>().ok())
                .is_some_and(|count| count > 0),
            "candidate qualification requires the Stream container's NVIDIA GPU resource"
        );
        let candidate_sha256 = format!("sha256:{}", hex::encode(Sha256::digest(fs::read(binary)?)));
        let mut arguments = candidate_arguments(&container.args)?;
        let deployment_spec = deployment_spec(namespace)?;
        let remote = format!("/tmp/veoveo-compiler-{}", uuid::Uuid::new_v4().simple());
        let remote_app = format!("{remote}-app.html");
        arguments.extend(["--live-app".to_owned(), remote_app.clone()]);
        let mut candidate = Self {
            namespace: namespace.to_owned(),
            pod: pod.metadata.name,
            remote,
            remote_app,
            process: None,
            process_group: None,
            receipt: Receipt {
                schema: "veoveo.io/stream-compiler-acceptance/v1",
                pod_uid: pod.metadata.uid,
                runtime_image: container.image.clone(),
                runtime_image_id: status.image_id.clone(),
                candidate_sha256,
                app_sha256: format!("sha256:{}", hex::encode(Sha256::digest(fs::read(app)?))),
                gpu: String::new(),
                processed_frames: 0,
                detection_count: 0,
                original_restart_count: status.restart_count,
                candidate_removed: false,
                outcome: ProbeOutcome::Started,
            },
            deployment_spec,
            removed: false,
        };
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
        let mut copy = Command::new("kubectl");
        copy.args([
            "-n",
            namespace,
            "exec",
            "-i",
            &candidate.pod,
            "-c",
            CONTAINER,
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
        let mut copy_app = Command::new("kubectl");
        copy_app
            .args([
                "-n",
                namespace,
                "exec",
                "-i",
                &candidate.pod,
                "-c",
                CONTAINER,
                "--",
                "tee",
                &candidate.remote_app,
            ])
            .stdin(File::open(app)?)
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        checked(&mut copy_app)?;
        let app_hash = String::from_utf8(checked(
            candidate.exec().args(["sha256sum", &candidate.remote_app]),
        )?)?;
        ensure!(
            app_hash.split_whitespace().next()
                == candidate.receipt.app_sha256.strip_prefix("sha256:"),
            "candidate App copy differs from local asset"
        );
        fs::create_dir_all(work_dir)?;
        let log = File::create(work_dir.join("compiler-candidate.log"))?;
        candidate.process = Some(
            candidate
                .exec()
                .arg("setsid")
                .arg("--wait")
                .arg(&candidate.remote)
                .args(arguments)
                .stdout(log.try_clone()?)
                .stderr(log)
                .spawn()?,
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        let pattern = format!("^{}( |$)", candidate.remote);
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
            let running = candidate.exec().args(["pgrep", "-f", &pattern]).output()?;
            if running.status.success() {
                let pid = String::from_utf8(running.stdout)?
                    .trim()
                    .parse::<u32>()
                    .context("candidate must have exactly one process")?;
                let group = String::from_utf8(checked(candidate.exec().args([
                    "ps",
                    "-o",
                    "pgid=",
                    "-p",
                    &pid.to_string(),
                ]))?)?
                .trim()
                .parse::<u32>()?;
                ensure!(pid == group, "candidate has no isolated process group");
                candidate.process_group = Some(group);
                break;
            }
            ensure!(
                running.status.code() == Some(1),
                "could not observe compiler candidate process"
            );
            ensure!(
                Instant::now() < deadline,
                "compiler candidate process did not start"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok(candidate)
    }

    pub(super) fn resource(&self) -> String {
        format!("pod/{}", self.pod)
    }

    pub(super) fn check_running(&mut self, work_dir: &Path) -> Result<()> {
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

    pub(super) fn verify_listener(&self) -> Result<()> {
        let pid = self
            .process_group
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
            owns_listener(&sockets, &descriptors, PORT),
            "the candidate process does not own the accepted HTTP listener"
        );
        Ok(())
    }

    fn exec(&self) -> Command {
        let mut command = Command::new("kubectl");
        command.args([
            "-n",
            &self.namespace,
            "exec",
            &self.pod,
            "-c",
            CONTAINER,
            "--",
        ]);
        command
    }

    fn remove(&mut self) -> Result<()> {
        if self.removed {
            return Ok(());
        }
        if let Some(group) = self.process_group {
            // Stop native children while the server is alive to reap them. A
            // failed or timed-out task must not leave a GPU runner behind.
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let members = self
                    .exec()
                    .args(["pgrep", "-g", &group.to_string()])
                    .output()?;
                ensure!(
                    members.status.success() || members.status.code() == Some(1),
                    "could not inspect candidate process group"
                );
                let children = String::from_utf8(members.stdout)?
                    .split_whitespace()
                    .map(str::parse::<u32>)
                    .collect::<std::result::Result<Vec<_>, _>>()?
                    .into_iter()
                    .filter(|pid| *pid != group)
                    .map(|pid| pid.to_string())
                    .collect::<Vec<_>>();
                if children.is_empty() {
                    break;
                }
                let stopped = self
                    .exec()
                    .args(["kill", "-TERM", "--"])
                    .args(&children)
                    .output()?;
                ensure!(
                    stopped.status.success(),
                    "could not stop candidate GPU children"
                );
                ensure!(
                    Instant::now() < deadline,
                    "candidate GPU children did not stop"
                );
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        // This generated path contains no regular-expression metacharacters.
        let pattern = format!("^{}( |$)", self.remote);
        let killed = self
            .exec()
            .args(["pkill", "-TERM", "-f", &pattern])
            .output()?;
        ensure!(
            killed.status.success() || killed.status.code() == Some(1),
            "could not terminate compiler candidate"
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let running = self.exec().args(["pgrep", "-f", &pattern]).output()?;
            if running.status.code() == Some(1) {
                break;
            }
            ensure!(
                running.status.success(),
                "could not inspect compiler candidate cleanup"
            );
            ensure!(Instant::now() < deadline, "compiler candidate did not stop");
            std::thread::sleep(Duration::from_millis(100));
        }
        if let Some(mut process) = self.process.take() {
            let _ = process.kill();
            process.wait()?;
        }
        checked(
            self.exec()
                .args(["rm", "-f", &self.remote, &self.remote_app]),
        )?;
        let absent = self
            .exec()
            .args(["test", "!", "-e", &self.remote])
            .output()?;
        ensure!(
            absent.status.success(),
            "compiler candidate file survived cleanup"
        );
        ensure!(
            self.exec()
                .args(["test", "!", "-e", &self.remote_app])
                .output()?
                .status
                .success(),
            "candidate App file survived cleanup"
        );
        self.removed = true;
        self.receipt.candidate_removed = true;
        Ok(())
    }

    pub(super) fn finish(&mut self, work_dir: &Path, outcome: ProbeOutcome) -> Result<()> {
        self.verify_listener()?;
        self.remove()?;
        ensure!(
            deployment_spec(&self.namespace)? == self.deployment_spec,
            "Stream Deployment changed during qualification"
        );
        let pod: Pod = serde_json::from_slice(&checked(Command::new("kubectl").args([
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
            .find(|container| container.name == CONTAINER)
            .context("Stream runtime status disappeared")?;
        ensure!(
            pod.metadata.uid == self.receipt.pod_uid
                && status.ready
                && status.restart_count == self.receipt.original_restart_count
                && status.image_id == self.receipt.runtime_image_id,
            "original Stream runtime changed during qualification"
        );
        if let ProbeOutcome::GpuQualified { result } = &outcome {
            self.receipt.processed_frames = result.summary.processed_frames;
            self.receipt.detection_count = result.summary.detection_count;
        }
        self.receipt.outcome = outcome;
        fs::write(
            work_dir.join("compiler-candidate.json"),
            serde_json::to_vec_pretty(&self.receipt)?,
        )?;
        Ok(())
    }
}

impl Drop for Candidate {
    fn drop(&mut self) {
        if let Err(error) = self.remove() {
            eprintln!(
                "compiler candidate cleanup failed for {}:{}: {error:#}",
                self.pod, self.remote
            );
        }
    }
}

fn checked(command: &mut Command) -> Result<Vec<u8>> {
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

fn deployment_spec(namespace: &str) -> Result<Vec<u8>> {
    checked(Command::new("kubectl").args([
        "-n",
        namespace,
        "get",
        "deployment/stream-mcp",
        "-o",
        "jsonpath={.spec}",
    ]))
}

fn candidate_arguments(original: &[String]) -> Result<Vec<String>> {
    let mut arguments = Vec::new();
    let mut original = original.iter();
    while let Some(argument) = original.next() {
        if argument == "--port" || argument == "--live-app" {
            original
                .next()
                .context("installed Stream port argument has no value")?;
        } else if !argument.starts_with("--port=") && !argument.starts_with("--live-app=") {
            arguments.push(argument.clone());
        }
    }
    arguments.extend(["--port".to_owned(), PORT.to_string()]);
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

    #[test]
    fn candidate_changes_only_the_listener() {
        let args = [
            "--port",
            "8797",
            "--allowed-host",
            "stream-mcp:8797",
            "--pipeline-catalog",
            "/site/catalog.json",
        ]
        .map(str::to_owned);
        assert_eq!(
            candidate_arguments(&args).unwrap(),
            [
                "--allowed-host",
                "stream-mcp:8797",
                "--pipeline-catalog",
                "/site/catalog.json",
                "--port",
                "18797"
            ]
        );
        assert_eq!(
            candidate_arguments(&["--port=8797".into()]).unwrap(),
            ["--port", "18797"]
        );
        assert!(candidate_arguments(&["--port".into()]).is_err());
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
}
