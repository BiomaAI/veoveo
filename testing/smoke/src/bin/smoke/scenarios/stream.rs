use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chrono::{TimeDelta, Utc};
use re_sdk::RecordingStreamBuilder;
use re_sdk_types::archetypes::VideoStream;
use re_sdk_types::components::VideoCodec;
use secrecy::SecretString;
use serde_json::json;
use veoveo_mcp_contract::{
    AccessSubject, GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalSigningKey,
    GatewayInternalTokenIssuer, GatewayProfileId, InvocationAuthority, InvocationProvenance,
    PolicyVersion, Principal, PrincipalId, PrincipalKind, ScopeName, ServerSlug, TenantId,
    TokenIssuer, TokenSubject, WorkContextId, WorkContextMembershipLevel, WorkContextOutputPolicy,
};
use veoveo_platform_store::{
    PlatformStore, RecordIdKey, RecordingId, StoreConfig, StoreCredentials, deterministic_tenant_id,
};

use super::*;

const SAMPLE_H264_NAME: &str = "sample_720p.h264";
const SAMPLE_FRAME_COUNT: usize = 90;
const RECORDING_PROXY: &str = "rerun+http://127.0.0.1:9876/proxy";
pub(crate) const RECORDING_FORWARDER: &str = "target/debug/recording-forwarder";
const STREAM_MCP_URL: &str = "http://127.0.0.1:8797/stream/mcp";
const STREAM_READY_URL: &str = "http://127.0.0.1:8797/stream/readyz";
const STREAM_HOST: &str = "stream-mcp:8797";
const DEFAULT_KUBERNETES_NAMESPACE: &str = "veoveo";

#[path = "stream/candidate.rs"]
mod candidate;

pub(crate) async fn stream_compiler_startup(
    namespace: &str,
    binary: &Path,
    app: &Path,
    work_dir: &Path,
) -> Result<()> {
    let mut candidate = Some(candidate::Candidate::start(
        namespace, binary, app, work_dir,
    )?);
    let resource = candidate.as_ref().context("candidate missing")?.resource();
    let _forward = PortForwardGuard::spawn(namespace, &resource, 8797, candidate::PORT)?;
    wait_for_stream(namespace, &mut candidate, work_dir).await?;
    candidate
        .as_mut()
        .context("candidate missing")?
        .finish(work_dir, candidate::ProbeOutcome::StartupVerified)?;
    println!(
        "Stream compiler service startup verified; GPU workload qualification remains separate"
    );
    Ok(())
}

pub(crate) async fn stream_gpu(
    env_file: &Path,
    work_dir: &Path,
    candidate_inputs: Option<(&Path, &Path)>,
    pipeline_id: &str,
    producer_key_secret: &str,
) -> Result<()> {
    ensure!(
        env_file.is_file(),
        "environment file is missing: {}",
        env_file.display()
    );
    let environment = load_environment(env_file)?;
    let namespace = kubernetes_namespace(&environment);
    let signing_key = required_environment(&environment, "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64")?;
    let signing_key_id = required_environment(&environment, "VEOVEO_INTERNAL_SIGNING_KEY_ID")?;
    let sample_h264 = prepare_sample_h264(work_dir, &environment)?;
    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    let producer_key = recording_producer_key(namespace, producer_key_secret, &tmpdir)?;
    let queue_dir = tmpdir.join("forwarder-queue");
    let forwarder_log = tmpdir.join("recording-forwarder.log");
    std::fs::create_dir_all(&queue_dir)?;
    let gateway_url = required_environment(&environment, "PUBLIC_BASE_URL")?.trim_end_matches('/');
    let producer_client_id = optional_environment(
        &environment,
        "VEOVEO_RECORDING_PRODUCER_CLIENT_ID",
        "recording-producer",
    );
    let producer_key_id = required_environment(&environment, "VEOVEO_RECORDING_PRODUCER_KEY_ID")?;

    run_checked(
        Path::new("kubectl"),
        [
            "-n".into(),
            namespace.into(),
            "rollout".into(),
            "status".into(),
            "deployment/stream-mcp".into(),
            "--timeout=300s".into(),
        ],
        [],
    )
    .context("Stream GPU smoke requires the active k3d Stream profile")?;
    let mut candidate = candidate_inputs
        .map(|(binary, app)| candidate::Candidate::start(namespace, binary, app, work_dir))
        .transpose()?;
    let _recording_forwarder = ChildGuard::spawn(
        Path::new(RECORDING_FORWARDER),
        [
            "--gateway-url".into(),
            format!("{gateway_url}/").into(),
            "--protected-resource".into(),
            format!("{gateway_url}/ingest/recordings").into(),
            "--client-id".into(),
            producer_client_id.into(),
            "--key-id".into(),
            producer_key_id.into(),
            "--private-key-pem-file".into(),
            producer_key.path().as_os_str().to_os_string(),
            "--queue-dir".into(),
            queue_dir.as_os_str().to_os_string(),
        ],
        [],
        &forwarder_log,
    )
    .with_context(|| {
        format!(
            "starting authenticated recording forwarder; logs: {}",
            forwarder_log.display()
        )
    })?;
    wait_for_recording_forwarder(&forwarder_log).await?;
    let remote_port = if candidate.is_some() {
        candidate::PORT
    } else {
        8797
    };
    let resource = candidate
        .as_ref()
        .map(candidate::Candidate::resource)
        .unwrap_or_else(|| "service/stream-mcp".to_owned());
    let _stream_forward = PortForwardGuard::spawn(namespace, &resource, 8797, remote_port)?;
    let _surreal_forward = PortForwardGuard::spawn(namespace, "surrealdb", 8000, 8000)?;
    wait_for_stream(namespace, &mut candidate, work_dir).await?;
    if let Some(candidate) = &candidate {
        candidate.verify_listener()?;
    }

    let recording_key = uuid::Uuid::now_v7().to_string();
    publish_h264_recording(&recording_key, &sample_h264).await?;
    let recording_id = wait_for_recording_catalog(&environment, &recording_key).await?;
    let arguments = json!({
        "video": {
            "recording_uri": format!("recording://recordings/{recording_id}"),
            "entity_path": "/world/camera/front",
            "timeline": "sensor_time",
            "range": {"start": 0, "end": 3_000_000_000_i64}
        },
        "pipeline_id": pipeline_id,
        "sampling": {"mode": "every_nth", "step": 3},
        "include_source_clip": true
    });

    let bearer_token = issue_internal_token(
        signing_key,
        signing_key_id,
        "stream",
        "stream-gpu-smoke",
        &environment,
    )
    .await?;
    let task_client =
        FinalTaskSmokeClient::new(STREAM_MCP_URL, bearer_token).with_host(STREAM_HOST);
    let task = task_client
        .run_tool_structured("run_recording", arguments, Duration::from_secs(300))
        .await;
    let task = match task {
        Ok(output) => output,
        Err(error) => {
            if let Some(candidate) = candidate.as_mut() {
                candidate.finish(
                    work_dir,
                    candidate::ProbeOutcome::WorkloadFailed {
                        message: format!("{error:#}"),
                    },
                )?;
                bail!(
                    "Stream compiler workload failed: {error:#}; receipt and candidate logs: {}",
                    work_dir.display()
                );
            }
            let logs = kubernetes_logs(namespace, "deployment/stream-mcp")
                .unwrap_or_else(|log_error| format!("failed to collect logs: {log_error:#}"));
            bail!("Stream MCP recording run failed: {error:#}\nKubernetes logs:\n{logs}");
        }
    };
    let output: veoveo_stream_mcp::contract::RunRecordingOutput = serde_json::from_value(task)
        .context("Stream recording run did not return its typed contract")?;
    let processed_frames = output.summary.processed_frames;
    ensure!(
        processed_frames > 0,
        "Stream recording run processed no GPU frames: {output:?}"
    );
    ensure!(
        output.source_clip_artifact.is_some(),
        "Stream recording run omitted its requested source clip"
    );

    let detection_count = output.summary.detection_count;
    ensure!(
        detection_count > 0,
        "Stream recording run returned no detections: {output:?}"
    );
    println!(
        "Stream GPU smoke ok: recording {recording_id}, {processed_frames} frames, {detection_count} detections, typed artifacts published"
    );
    if let Some(candidate) = candidate.as_mut() {
        candidate.finish(
            work_dir,
            candidate::ProbeOutcome::GpuQualified {
                result: Box::new(output),
            },
        )?;
    }
    cleanup.remove_on_drop();
    Ok(())
}

pub(crate) fn load_environment(path: &Path) -> Result<BTreeMap<String, String>> {
    dotenvy::from_path_iter(path)
        .with_context(|| format!("opening environment file {}", path.display()))?
        .collect::<std::result::Result<_, _>>()
        .with_context(|| format!("parsing environment file {}", path.display()))
}

/// Keep the installed producer credential in a private file owned by the harness.
/// `NamedTempFile` removes it on every return path, including failed acceptance.
pub(crate) fn recording_producer_key(
    namespace: &str,
    secret_name: &str,
    directory: &Path,
) -> Result<tempfile::NamedTempFile> {
    #[derive(serde::Deserialize)]
    struct Secret {
        data: KeyData,
    }
    #[derive(serde::Deserialize)]
    struct KeyData {
        #[serde(rename = "private-key.pem")]
        private_key: String,
    }

    let document = run_checked(
        Path::new("kubectl"),
        [
            "-n".into(),
            namespace.into(),
            "get".into(),
            "secret".into(),
            secret_name.into(),
            "-o".into(),
            "json".into(),
        ],
        [],
    )?;
    let secret: Secret = serde_json::from_str(&document)
        .context("recording producer Secret must contain private-key.pem")?;
    let key = BASE64_STANDARD
        .decode(secret.data.private_key)
        .context("recording producer Secret contains invalid base64")?;
    ensure!(
        !key.is_empty(),
        "recording producer Secret contains an empty key"
    );
    let mut file = tempfile::NamedTempFile::new_in(directory)?;
    std::io::Write::write_all(&mut file, &key)?;
    Ok(file)
}

pub(crate) fn required_environment<'a>(
    environment: &'a BTreeMap<String, String>,
    name: &str,
) -> Result<&'a str> {
    environment
        .get(name)
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
        .with_context(|| format!("environment file does not define {name}"))
}

pub(crate) fn optional_environment<'a>(
    environment: &'a BTreeMap<String, String>,
    name: &str,
    default: &'a str,
) -> &'a str {
    environment
        .get(name)
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(default)
}

pub(crate) fn kubernetes_namespace(environment: &BTreeMap<String, String>) -> &str {
    optional_environment(
        environment,
        "VEOVEO_KUBERNETES_NAMESPACE",
        DEFAULT_KUBERNETES_NAMESPACE,
    )
}

pub(crate) async fn wait_for_recording_forwarder(log: &Path) -> Result<()> {
    for _ in 0..100 {
        if tokio::net::TcpStream::connect("127.0.0.1:9876")
            .await
            .is_ok()
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let output = std::fs::read_to_string(log)
        .unwrap_or_else(|error| format!("failed to read forwarder log: {error}"));
    bail!("recording forwarder did not accept loopback Rerun traffic\n{output}")
}

pub(crate) async fn wait_for_recording_catalog(
    environment: &BTreeMap<String, String>,
    recording_key: &str,
) -> Result<RecordingId> {
    let store = recording_store(environment).await?;
    let tenant_id =
        deterministic_tenant_id(required_environment(environment, "RECORDING_TENANT_KEY")?)?;
    for _ in 0..80 {
        if let Some(recording) = store
            .recording_by_key(tenant_id, "veoveo-video-test", recording_key)
            .await?
        {
            ensure!(
                recording.id.table.as_str() == RecordingId::TABLE,
                "catalog returned a non-recording id: {:?}",
                recording.id
            );
            let uuid = match &recording.id.key {
                RecordIdKey::Uuid(value) => **value,
                RecordIdKey::String(value) => uuid::Uuid::parse_str(value)?,
                other => bail!("catalog recording key is not a UUID: {other:?}"),
            };
            ensure!(
                uuid.get_version_num() == 7,
                "catalog recording id is not UUIDv7"
            );
            return Ok(RecordingId::from_uuid(uuid));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    bail!("Recording Hub did not catalog recording key {recording_key}")
}

async fn recording_store(environment: &BTreeMap<String, String>) -> Result<PlatformStore> {
    let username = required_environment(environment, "VEOVEO_SURREAL_RUNTIME_USERNAME")?;
    let password = required_environment(environment, "VEOVEO_SURREAL_RUNTIME_PASSWORD")?;
    let namespace = required_environment(environment, "VEOVEO_SURREAL_NAMESPACE")?;
    let database = required_environment(environment, "VEOVEO_SURREAL_DATABASE")?;
    PlatformStore::connect(
        StoreConfig::builder(
            "ws://127.0.0.1:8000",
            namespace,
            database,
            StoreCredentials::database(username, SecretString::from(password.to_owned())),
        )
        .build()?,
    )
    .await
    .context("connecting to the installed recording catalog")
}

async fn wait_for_stream(
    namespace: &str,
    candidate: &mut Option<candidate::Candidate>,
    work_dir: &Path,
) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    for _ in 0..90 {
        if let Some(candidate) = candidate {
            candidate.check_running(work_dir)?;
        }
        if client
            .get(STREAM_READY_URL)
            .header(reqwest::header::HOST, STREAM_HOST)
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    let logs = kubernetes_logs(namespace, "deployment/stream-mcp")
        .unwrap_or_else(|error| format!("failed to collect logs: {error:#}"));
    bail!("Stream MCP did not become ready\n{logs}")
}

pub(crate) fn prepare_sample_h264(
    work_dir: &Path,
    environment: &BTreeMap<String, String>,
) -> Result<PathBuf> {
    std::fs::create_dir_all(work_dir)?;
    let output = work_dir.join(SAMPLE_H264_NAME);
    if output.metadata().is_ok_and(|metadata| metadata.len() > 0) {
        return Ok(output);
    }
    // The installed digest already contains the NVIDIA sample. Demuxing needs
    // neither a separately tagged Docker image nor installation-specific host paths.
    let staging = output.with_extension("partial");
    let result = Command::new("kubectl")
        .args([
            "-n",
            kubernetes_namespace(environment),
            "exec",
            "deployment/stream-mcp",
            "-c",
            "stream-mcp",
            "--",
            "gst-launch-1.0",
            "-q",
            "filesrc",
            "location=/opt/nvidia/deepstream/deepstream/samples/streams/sample_720p.mp4",
            "!",
            "qtdemux",
            "!",
            "h264parse",
            "config-interval=-1",
            "!",
            "video/x-h264,stream-format=byte-stream,alignment=au",
            "!",
            "fdsink",
            "fd=1",
        ])
        .stdout(std::fs::File::create(&staging)?)
        .stderr(Stdio::piped())
        .output()?;
    ensure!(
        result.status.success(),
        "DeepStream sample demux failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    std::fs::rename(staging, &output)?;
    ensure!(
        output.metadata().is_ok_and(|metadata| metadata.len() > 0),
        "DeepStream sample demux did not create {}",
        output.display()
    );
    Ok(output)
}

pub(crate) async fn publish_h264_recording(recording_id: &str, sample_h264: &Path) -> Result<()> {
    let mut access_units = sample_access_units(sample_h264)?;
    ensure!(
        access_units.len() >= SAMPLE_FRAME_COUNT,
        "DeepStream sample contains only {} access units",
        access_units.len()
    );
    access_units.truncate(SAMPLE_FRAME_COUNT);
    let stream = RecordingStreamBuilder::new("veoveo-video-test")
        .recording_id(recording_id.to_owned())
        .connect_grpc_opts(RECORDING_PROXY.to_owned())
        .context("connecting the H.264 producer to Recording Hub")?;
    for (frame, bytes) in access_units.into_iter().enumerate() {
        let keyframe = access_unit_is_idr(&bytes);
        stream.set_duration_secs("sensor_time", frame as f64 / 30.0);
        let mut video = VideoStream::new(VideoCodec::H264).with_sample(bytes);
        if keyframe {
            video = video.with_is_keyframe(true);
        }
        stream.log("/world/camera/front", &video)?;
    }
    stream.flush_blocking()?;
    drop(stream);
    tokio::time::sleep(Duration::from_secs(1)).await;
    Ok(())
}

fn sample_access_units(path: &Path) -> Result<Vec<Vec<u8>>> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("reading DeepStream H.264 sample {}", path.display()))?;
    let starts = (0..bytes.len().saturating_sub(4))
        .filter(|index| bytes[*index..].starts_with(&[0, 0, 0, 1, 9]))
        .collect::<Vec<_>>();
    Ok(starts
        .iter()
        .enumerate()
        .map(|(index, start)| {
            let end = starts.get(index + 1).copied().unwrap_or(bytes.len());
            bytes[*start..end].to_vec()
        })
        .collect())
}

fn access_unit_is_idr(bytes: &[u8]) -> bool {
    (0..bytes.len().saturating_sub(4))
        .any(|index| bytes[index..].starts_with(&[0, 0, 0, 1]) && bytes[index + 4] & 0x1f == 5)
}

pub(crate) async fn issue_internal_token(
    private_key_der_b64: &str,
    key_id: &str,
    server: &str,
    subject: &str,
    environment: &BTreeMap<String, String>,
) -> Result<String> {
    let tenant = required_environment(environment, "RECORDING_TENANT_KEY")?;
    let work_context = required_environment(environment, "RECORDING_WORK_CONTEXT")?;
    let context = recording_store(environment)
        .await?
        .artifact_read_context_version(tenant, work_context)
        .await?
        .context("acceptance tenant and Work Context must exist in the installed catalog")?;
    let private_key_der = BASE64_STANDARD.decode(private_key_der_b64.trim())?;
    let issuer = GatewayInternalTokenIssuer::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        GatewayInternalSigningKey::new(key_id.to_owned(), private_key_der)?,
    );
    let principal_issuer = TokenIssuer::new("https://smoke.veoveo.local")?;
    let principal_subject = TokenSubject::new(subject)?;
    let principal = Principal {
        id: PrincipalId::new(format!("{principal_issuer}#{principal_subject}"))?,
        kind: PrincipalKind::Service,
        issuer: principal_issuer,
        subject: principal_subject,
        tenant: Some(TenantId::new(tenant)?),
        groups: Default::default(),
        group_roles: Default::default(),
        roles: Default::default(),
        scopes: [ScopeName::new("operator:use")?].into_iter().collect(),
        data_labels: Default::default(),
        assurances: Default::default(),
        authenticated_at: Some(Utc::now()),
    };
    let authority = InvocationAuthority {
        work_context: WorkContextId::new(work_context)?,
        tenant: TenantId::new(tenant)?,
        membership: WorkContextMembershipLevel::Owner,
        policy_revision: PolicyVersion::new(context.policy_revision)?,
        output_policy: WorkContextOutputPolicy {
            owner: AccessSubject::Principal(principal.id.clone()),
            initial_grants: Vec::new(),
            classification: None,
            data_labels: Default::default(),
        },
        provenance: InvocationProvenance::Automated,
    };
    Ok(issuer
        .issue(
            GatewayProfileId::new("operator")?,
            ServerSlug::new(server)?,
            principal,
            authority,
            Utc::now() + TimeDelta::minutes(30),
        )?
        .bearer_token)
}

pub(crate) struct PortForwardGuard {
    child: Child,
}

impl PortForwardGuard {
    pub(crate) fn spawn(
        namespace: &str,
        resource: &str,
        local_port: u16,
        remote_port: u16,
    ) -> Result<Self> {
        let resource = if resource.contains('/') {
            resource.to_owned()
        } else {
            format!("service/{resource}")
        };
        let child = Command::new("kubectl")
            .args([
                "-n",
                namespace,
                "port-forward",
                &resource,
                &format!("{local_port}:{remote_port}"),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("starting port-forward for {resource}"))?;
        Ok(Self { child })
    }
}

impl Drop for PortForwardGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub(crate) fn kubernetes_logs(namespace: &str, primary_workload: &str) -> Result<String> {
    let mut output = String::new();
    for (workload, container) in [
        (primary_workload, None),
        ("deployment/recording", Some("recording-hub")),
        ("deployment/artifact-service", None),
    ] {
        let mut arguments = vec![
            "-n".into(),
            namespace.into(),
            "logs".into(),
            workload.into(),
            "--tail=300".into(),
        ];
        if let Some(container) = container {
            arguments.extend(["-c".into(), container.into()]);
        }
        let logs = run_checked(Path::new("kubectl"), arguments, [])?;
        output.push_str(&format!("==> {workload}\n{logs}\n"));
    }
    Ok(output)
}
