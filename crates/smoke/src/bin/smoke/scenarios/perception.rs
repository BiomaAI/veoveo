use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chrono::{TimeDelta, Utc};
use re_sdk::RecordingStreamBuilder;
use re_sdk_types::archetypes::VideoStream;
use re_sdk_types::components::VideoCodec;
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use rmcp::model::CallToolResult;
use secrecy::SecretString;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Map, Value, json};
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalSigningKey, GatewayInternalTokenIssuer,
    GatewayProfileId, Principal, PrincipalId, PrincipalKind, ScopeName, ServerSlug, TenantId,
    TokenIssuer, TokenSubject,
};
use veoveo_mcp_task_extension::{
    CreateTaskResult, DISCOVER_METHOD, DetailedTask, DiscoverParams, DiscoverResult, EXTENSION_ID,
    GET_TASK_METHOD, GetTaskParams, GetTaskResult, HEADER_MCP_METHOD, HEADER_MCP_NAME,
    HEADER_MCP_PROTOCOL_VERSION, PROTOCOL_VERSION, RequestMeta, ToolCallParams,
};
use veoveo_platform_store::{
    PlatformStore, RecordIdKey, RecordingId, StoreConfig, StoreCredentials, deterministic_tenant_id,
};

use super::*;

const SAMPLE_H264_NAME: &str = "sample_720p.h264";
const SAMPLE_FRAME_COUNT: usize = 90;
const RECORDING_PROXY: &str = "rerun+http://127.0.0.1:9876/proxy";
const PERCEPTION_MCP_URL: &str = "http://127.0.0.1:8797/perception/mcp";
const PERCEPTION_READY_URL: &str = "http://127.0.0.1:8797/perception/readyz";

pub(crate) async fn perception_gpu(
    env_file: &Path,
    compose_override: &Path,
    project_name: &str,
) -> Result<()> {
    ensure!(
        !project_name.trim().is_empty(),
        "Compose project name is empty"
    );
    ensure!(
        env_file.is_file(),
        "environment file is missing: {}",
        env_file.display()
    );
    ensure!(
        compose_override.is_file(),
        "Compose override is missing: {}",
        compose_override.display()
    );
    let environment = load_environment(env_file)?;
    validate_perception_workspace(&environment)?;
    let signing_key = required_environment(&environment, "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64")?;
    let signing_key_id = required_environment(&environment, "VEOVEO_INTERNAL_SIGNING_KEY_ID")?;
    let sample_h264 = prepare_sample_h264(compose_override, &environment)?;

    let stack = PerceptionComposeGuard::new(project_name, env_file, compose_override);
    stack.run(&["up", "-d", "--no-build", "otel-collector", "perception-mcp"])?;
    wait_for_perception(&stack).await?;

    let recording_key = uuid::Uuid::now_v7().to_string();
    publish_h264_recording(&recording_key, &sample_h264).await?;
    let recording_id = wait_for_recording_catalog(&environment, &recording_key).await?;
    let arguments = json!({
        "video": {
            "recording_uri": format!("recording://recordings/{recording_id}"),
            "entity_path": "/world/camera/front",
            "timeline": "sensor_time",
            "range": {"start": 0, "end": 3_000_000_000_i64},
            "source": {"mode": "recent_proxy", "idle_ms": 500, "capture_ms": 5_000}
        },
        "pipeline_id": "detect-objects",
        "sampling": {"mode": "every_nth", "step": 3},
        "include_source_clip": true
    });

    let bearer_token = issue_internal_token(signing_key, signing_key_id)?;
    let task_client = FinalTaskSmokeClient::new(PERCEPTION_MCP_URL, bearer_token);
    let task = task_client.run_tool("analyze_recording", arguments).await;
    let task = match task {
        Ok(output) => output,
        Err(error) => {
            let logs = stack
                .run(&[
                    "logs",
                    "--no-color",
                    "--tail",
                    "300",
                    "perception-mcp",
                    "recording-hub",
                    "artifact-service",
                ])
                .unwrap_or_else(|log_error| format!("failed to collect logs: {log_error:#}"));
            bail!("perception MCP task failed: {error:#}\nCompose logs:\n{logs}");
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
    Ok(())
}

fn load_environment(path: &Path) -> Result<BTreeMap<String, String>> {
    dotenvy::from_path_iter(path)
        .with_context(|| format!("opening environment file {}", path.display()))?
        .collect::<std::result::Result<_, _>>()
        .with_context(|| format!("parsing environment file {}", path.display()))
}

fn required_environment<'a>(
    environment: &'a BTreeMap<String, String>,
    name: &str,
) -> Result<&'a str> {
    environment
        .get(name)
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
        .with_context(|| format!("environment file does not define {name}"))
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

async fn wait_for_recording_catalog(
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

async fn wait_for_perception(stack: &PerceptionComposeGuard) -> Result<()> {
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
    let logs = stack
        .run(&[
            "logs",
            "--no-color",
            "--tail",
            "300",
            "perception-mcp",
            "recording-hub",
            "artifact-service",
        ])
        .unwrap_or_else(|error| format!("failed to collect logs: {error:#}"));
    bail!("perception MCP did not become ready\n{logs}")
}

fn prepare_sample_h264(
    compose_override: &Path,
    environment: &BTreeMap<String, String>,
) -> Result<PathBuf> {
    let perception_dir = compose_override
        .parent()
        .context("perception Compose override has no parent directory")?;
    let work_dir = perception_dir.join("work");
    std::fs::create_dir_all(&work_dir)?;
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

async fn publish_h264_recording(recording_id: &str, sample_h264: &Path) -> Result<()> {
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
        stream.log(
            "/world/camera/front",
            &VideoStream::new(VideoCodec::H264)
                .with_sample(bytes)
                .with_is_keyframe(keyframe),
        )?;
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

fn issue_internal_token(private_key_der_b64: &str, key_id: &str) -> Result<String> {
    let private_key_der = BASE64_STANDARD.decode(private_key_der_b64.trim())?;
    let issuer = GatewayInternalTokenIssuer::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        GatewayInternalSigningKey::new(key_id.to_owned(), private_key_der)?,
    );
    let principal_issuer = TokenIssuer::new("https://smoke.veoveo.local")?;
    let principal_subject = TokenSubject::new("perception-gpu-smoke")?;
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
    Ok(issuer
        .issue(
            GatewayProfileId::new("operator")?,
            ServerSlug::new("perception")?,
            principal,
            Utc::now() + TimeDelta::minutes(30),
        )?
        .bearer_token)
}

struct FinalTaskSmokeClient {
    http: reqwest::Client,
    endpoint: String,
    bearer_token: String,
    request_ids: Arc<AtomicU64>,
}

impl FinalTaskSmokeClient {
    fn new(endpoint: &str, bearer_token: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            endpoint: endpoint.to_owned(),
            bearer_token,
            request_ids: Arc::new(AtomicU64::new(1)),
        }
    }

    async fn run_tool(&self, name: &str, arguments: Value) -> Result<Value> {
        self.discover().await?;
        let arguments = arguments
            .as_object()
            .context("perception tool arguments are not an object")?
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let created: CreateTaskResult = self
            .request(
                "tools/call",
                Some(name),
                &ToolCallParams {
                    meta: RequestMeta::new().with_task_capability(),
                    name: name.to_owned(),
                    arguments,
                },
            )
            .await?;
        let task_id = created.task.task_id;
        let poll_ms = created
            .task
            .poll_interval_ms
            .unwrap_or(100)
            .clamp(10, 5_000);
        let task = tokio::time::timeout(Duration::from_secs(300), async {
            loop {
                let task = self.get_task(task_id).await?;
                println!(
                    "perception task {task_id}: {:?} {}",
                    task.status(),
                    task.metadata().status_message.as_deref().unwrap_or("")
                );
                match task {
                    DetailedTask::Working { .. } => {
                        tokio::time::sleep(Duration::from_millis(poll_ms)).await;
                    }
                    terminal => return Ok::<_, anyhow::Error>(terminal),
                }
            }
        })
        .await
        .with_context(|| format!("timed out waiting for perception task {task_id}"))??;

        match task {
            DetailedTask::Completed { result, .. } => {
                let result: CallToolResult = serde_json::from_value(Value::Object(
                    result.into_iter().collect::<Map<String, Value>>(),
                ))?;
                ensure!(
                    result.is_error != Some(true),
                    "perception tool returned an error: {:?}",
                    result.content
                );
                result
                    .structured_content
                    .context("perception task completed without structured content")
            }
            DetailedTask::Failed { error, .. } => {
                bail!("task failed ({}): {}", error.code, error.message)
            }
            DetailedTask::Cancelled { .. } => bail!("perception task was cancelled"),
            DetailedTask::InputRequired { .. } => {
                bail!("perception task unexpectedly requested input")
            }
            DetailedTask::Working { .. } => unreachable!("task wait returns a terminal state"),
        }
    }

    async fn discover(&self) -> Result<()> {
        let result: DiscoverResult = self
            .request(
                DISCOVER_METHOD,
                None,
                &DiscoverParams {
                    meta: RequestMeta::new(),
                },
            )
            .await?;
        let extensions = result
            .capabilities
            .get("extensions")
            .and_then(Value::as_object);
        ensure!(
            result
                .supported_versions
                .iter()
                .any(|version| version == PROTOCOL_VERSION)
                && extensions.is_some_and(|extensions| extensions.contains_key(EXTENSION_ID)),
            "perception server does not advertise final MCP tasks"
        );
        Ok(())
    }

    async fn get_task(
        &self,
        task_id: veoveo_mcp_task_extension::ProtocolTaskId,
    ) -> Result<DetailedTask> {
        let result: GetTaskResult = self
            .request(
                GET_TASK_METHOD,
                Some(&task_id.to_string()),
                &GetTaskParams {
                    meta: RequestMeta::new().with_task_capability(),
                    task_id,
                },
            )
            .await?;
        Ok(result.task)
    }

    async fn request<T, P>(&self, method: &str, name: Option<&str>, params: &P) -> Result<T>
    where
        T: DeserializeOwned,
        P: Serialize + ?Sized,
    {
        let mut request = self
            .http
            .post(&self.endpoint)
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json")
            .header(HEADER_MCP_PROTOCOL_VERSION, PROTOCOL_VERSION)
            .header(HEADER_MCP_METHOD, method)
            .header(AUTHORIZATION, format!("Bearer {}", self.bearer_token))
            .json(&json!({
                "jsonrpc": "2.0",
                "id": self.request_ids.fetch_add(1, Ordering::Relaxed),
                "method": method,
                "params": params,
            }));
        if let Some(name) = name {
            request = request.header(HEADER_MCP_NAME, name);
        }
        let response = request.send().await?;
        let status = response.status();
        let envelope: RpcResponse<T> = response.json().await?;
        match (envelope.result, envelope.error) {
            (Some(result), None) if status.is_success() => Ok(result),
            (_, Some(error)) => bail!(
                "task extension request `{method}` failed ({}): {}",
                error.code,
                error.message
            ),
            _ => bail!("task extension request `{method}` returned invalid HTTP {status}"),
        }
    }
}

#[derive(Deserialize)]
struct RpcResponse<T> {
    result: Option<T>,
    error: Option<veoveo_mcp_task_extension::JsonRpcErrorData>,
}

struct PerceptionComposeGuard {
    project_name: String,
    env_file: PathBuf,
    compose_override: PathBuf,
}

impl PerceptionComposeGuard {
    fn new(project_name: &str, env_file: &Path, compose_override: &Path) -> Self {
        Self {
            project_name: project_name.to_owned(),
            env_file: env_file.to_owned(),
            compose_override: compose_override.to_owned(),
        }
    }

    fn arguments(&self, command: &[&str]) -> Vec<OsString> {
        let mut arguments = vec![
            "compose".into(),
            "--project-name".into(),
            self.project_name.clone().into(),
            "--env-file".into(),
            self.env_file.as_os_str().to_owned(),
            "-f".into(),
            "compose.yaml".into(),
            "-f".into(),
            self.compose_override.as_os_str().to_owned(),
        ];
        arguments.extend(command.iter().map(OsString::from));
        arguments
    }

    fn run(&self, command: &[&str]) -> Result<String> {
        run_checked(Path::new("docker"), self.arguments(command), [])
    }
}

impl Drop for PerceptionComposeGuard {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args(self.arguments(&["down", "--volumes", "--remove-orphans"]))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}
