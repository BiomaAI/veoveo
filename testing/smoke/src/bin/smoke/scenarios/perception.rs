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
use serde_json::{Value, json};
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
const PERCEPTION_MCP_URL: &str = "http://127.0.0.1:8797/perception/mcp";
const PERCEPTION_READY_URL: &str = "http://127.0.0.1:8797/perception/readyz";

pub(crate) async fn perception_gpu(env_file: &Path, work_dir: &Path) -> Result<()> {
    ensure!(
        env_file.is_file(),
        "environment file is missing: {}",
        env_file.display()
    );
    let environment = load_environment(env_file)?;
    validate_perception_workspace(&environment)?;
    let signing_key = required_environment(&environment, "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64")?;
    let signing_key_id = required_environment(&environment, "VEOVEO_INTERNAL_SIGNING_KEY_ID")?;
    let sample_h264 = prepare_sample_h264(work_dir, &environment)?;
    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    let producer_key = tmpdir.join("recording-producer.pem");
    let queue_dir = tmpdir.join("forwarder-queue");
    let forwarder_log = tmpdir.join("recording-forwarder.log");
    std::fs::create_dir_all(&queue_dir)?;
    std::fs::write(
        &producer_key,
        required_environment(&environment, "VEOVEO_RECORDING_PRODUCER_PRIVATE_KEY_PEM")?,
    )?;
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
            "veoveo".into(),
            "rollout".into(),
            "status".into(),
            "deployment/perception-mcp".into(),
            "--timeout=300s".into(),
        ],
        [],
    )
    .context("perception GPU smoke requires the active k3d perception profile")?;
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
            producer_key.as_os_str().to_os_string(),
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
    let _perception_forward = PortForwardGuard::spawn("perception-mcp", 8797, 8797)?;
    let _surreal_forward = PortForwardGuard::spawn("surrealdb", 8000, 8000)?;
    wait_for_perception().await?;

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
        "pipeline_id": "detect-objects",
        "sampling": {"mode": "every_nth", "step": 3},
        "include_source_clip": true
    });

    let bearer_token = issue_internal_token(
        signing_key,
        signing_key_id,
        "perception",
        "perception-gpu-smoke",
    )?;
    let task_client = FinalTaskSmokeClient::new(PERCEPTION_MCP_URL, bearer_token);
    let task = task_client
        .run_tool_structured("analyze_recording", arguments, Duration::from_secs(300))
        .await;
    let task = match task {
        Ok(output) => output,
        Err(error) => {
            let logs = kubernetes_logs("deployment/perception-mcp")
                .unwrap_or_else(|log_error| format!("failed to collect logs: {log_error:#}"));
            bail!("perception MCP task failed: {error:#}\nKubernetes logs:\n{logs}");
        }
    };
    let output = task;
    let summary = output
        .get("summary")
        .and_then(Value::as_object)
        .context("perception task output omitted its typed summary")?;
    let processed_frames = summary
        .get("processed_frames")
        .and_then(Value::as_u64)
        .context("perception task summary omitted processed_frames")?;
    ensure!(
        processed_frames > 0,
        "perception task processed no GPU frames: {output}"
    );
    for artifact in [
        "results_artifact",
        "annotations_artifact",
        "source_clip_artifact",
    ] {
        ensure!(
            output.get(artifact).is_some_and(Value::is_object),
            "perception task omitted {artifact}: {output}"
        );
    }

    let detection_count = summary
        .get("detection_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    ensure!(
        detection_count > 0,
        "perception task returned no detections: {output}"
    );
    println!(
        "perception GPU smoke ok: recording {recording_id}, {processed_frames} frames, {detection_count} detections, typed artifacts published"
    );
    cleanup.remove_on_drop();
    Ok(())
}

pub(crate) fn load_environment(path: &Path) -> Result<BTreeMap<String, String>> {
    dotenvy::from_path_iter(path)
        .with_context(|| format!("opening environment file {}", path.display()))?
        .collect::<std::result::Result<_, _>>()
        .with_context(|| format!("parsing environment file {}", path.display()))
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

fn validate_perception_workspace(environment: &BTreeMap<String, String>) -> Result<()> {
    let config_dir = PathBuf::from(required_environment(environment, "PERCEPTION_CONFIG_DIR")?);
    let model_dir = PathBuf::from(required_environment(environment, "PERCEPTION_MODEL_DIR")?);
    let catalog_path = config_dir.join("catalog.json");
    ensure!(
        catalog_path.is_file(),
        "perception catalog is missing: {}",
        catalog_path.display()
    );
    let catalog: Value = serde_json::from_slice(&std::fs::read(&catalog_path)?)?;
    let model_path = catalog
        .pointer("/models/0/model_path")
        .and_then(Value::as_str)
        .context("perception catalog has no first model_path")?;
    let model_name = Path::new(model_path)
        .file_name()
        .context("catalog model_path has no file name")?;
    let host_model = model_dir.join(model_name);
    ensure!(
        host_model.is_file(),
        "TensorRT engine is missing: {}",
        host_model.display()
    );
    Ok(())
}

pub(crate) async fn wait_for_recording_catalog(
    environment: &BTreeMap<String, String>,
    recording_key: &str,
) -> Result<RecordingId> {
    let username = required_environment(environment, "VEOVEO_SURREAL_RUNTIME_USERNAME")?;
    let password = required_environment(environment, "VEOVEO_SURREAL_RUNTIME_PASSWORD")?;
    let namespace = required_environment(environment, "VEOVEO_SURREAL_NAMESPACE")?;
    let database = required_environment(environment, "VEOVEO_SURREAL_DATABASE")?;
    let store = PlatformStore::connect(
        StoreConfig::builder(
            "ws://127.0.0.1:8000",
            namespace,
            database,
            StoreCredentials::database(username, SecretString::from(password.to_owned())),
        )
        .build()?,
    )
    .await?;
    let tenant_id = deterministic_tenant_id("enterprise")?;
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

async fn wait_for_perception() -> Result<()> {
    let client = reqwest::Client::new();
    for _ in 0..90 {
        if client
            .get(PERCEPTION_READY_URL)
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    let logs = kubernetes_logs("deployment/perception-mcp")
        .unwrap_or_else(|error| format!("failed to collect logs: {error:#}"));
    bail!("perception MCP did not become ready\n{logs}")
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
    let work_dir = work_dir.canonicalize()?;
    let image_tag = environment
        .get("VEOVEO_IMAGE_TAG")
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("0.1.0");
    run_checked(
        Path::new("docker"),
        [
            "run".into(),
            "--rm".into(),
            "--entrypoint".into(),
            "gst-launch-1.0".into(),
            "-v".into(),
            format!("{}:/work", work_dir.display()).into(),
            format!("veoveo/perception-mcp:{image_tag}").into(),
            "-q".into(),
            "filesrc".into(),
            "location=/opt/nvidia/deepstream/deepstream/samples/streams/sample_720p.mp4".into(),
            "!".into(),
            "qtdemux".into(),
            "!".into(),
            "h264parse".into(),
            "config-interval=-1".into(),
            "!".into(),
            "video/x-h264,stream-format=byte-stream,alignment=au".into(),
            "!".into(),
            "filesink".into(),
            format!("location=/work/{SAMPLE_H264_NAME}").into(),
        ],
        [],
    )?;
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

pub(crate) fn issue_internal_token(
    private_key_der_b64: &str,
    key_id: &str,
    server: &str,
    subject: &str,
) -> Result<String> {
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
        tenant: Some(TenantId::new("enterprise")?),
        groups: Default::default(),
        group_roles: Default::default(),
        roles: Default::default(),
        scopes: [ScopeName::new("operator:use")?].into_iter().collect(),
        data_labels: Default::default(),
        assurances: Default::default(),
        authenticated_at: Some(Utc::now()),
    };
    let authority = InvocationAuthority {
        work_context: WorkContextId::new("smoke")?,
        tenant: TenantId::new("enterprise")?,
        membership: WorkContextMembershipLevel::Owner,
        policy_revision: PolicyVersion::new("r1")?,
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
    pub(crate) fn spawn(resource: &str, local_port: u16, remote_port: u16) -> Result<Self> {
        let resource = if resource.contains('/') {
            resource.to_owned()
        } else {
            format!("service/{resource}")
        };
        let child = Command::new("kubectl")
            .args([
                "-n",
                "veoveo",
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

pub(crate) fn kubernetes_logs(primary_workload: &str) -> Result<String> {
    let mut output = String::new();
    for (workload, container) in [
        (primary_workload, None),
        ("deployment/recording", Some("recording-hub")),
        ("deployment/artifact-service", None),
    ] {
        let mut arguments = vec![
            "-n".into(),
            "veoveo".into(),
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
