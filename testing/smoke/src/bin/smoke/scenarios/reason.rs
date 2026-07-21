use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};

use super::perception::{
    PortForwardGuard, RECORDING_FORWARDER, issue_internal_token, kubernetes_logs, load_environment,
    optional_environment, prepare_sample_h264, publish_h264_recording, required_environment,
    wait_for_recording_catalog, wait_for_recording_forwarder,
};
use super::*;

const REASON_MCP_URL: &str = "http://127.0.0.1:8803/reason/mcp";
const REASON_READY_URL: &str = "http://127.0.0.1:8803/reason/readyz";

pub(crate) async fn reason_gpu(env_file: &Path, work_dir: &Path) -> Result<()> {
    ensure!(
        env_file.is_file(),
        "environment file is missing: {}",
        env_file.display()
    );
    let environment = load_environment(env_file)?;
    validate_reason_workspace(&environment)?;
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
            "deployment/reason-mcp".into(),
            "--timeout=300s".into(),
        ],
        [],
    )
    .context("reason GPU smoke requires the reason workload with its runner image and engine")?;
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
    let _reason_forward = PortForwardGuard::spawn("reason-mcp", 8803, 8803)?;
    let _surreal_forward = PortForwardGuard::spawn("surrealdb", 8000, 8000)?;
    wait_for_reason().await?;
    assert_unauthenticated_rejected().await?;

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
        "pipeline_id": "video-reasoning",
        "task": {
            "kind": "describe_segment",
            "prompt": "Describe the road scene and any moving vehicles."
        },
        "sampling": {"max_frames": 16}
    });

    let bearer_token =
        issue_internal_token(signing_key, signing_key_id, "reason", "reason-gpu-smoke")?;
    let task_client = FinalTaskSmokeClient::new(REASON_MCP_URL, bearer_token);
    let task = task_client
        .run_tool_structured("analyze_recording", arguments, Duration::from_secs(600))
        .await;
    let output = match task {
        Ok(output) => output,
        Err(error) => {
            let logs = kubernetes_logs("deployment/reason-mcp")
                .unwrap_or_else(|log_error| format!("failed to collect logs: {log_error:#}"));
            bail!("reason MCP task failed: {error:#}\nKubernetes logs:\n{logs}");
        }
    };
    let summary = output
        .get("summary")
        .and_then(Value::as_object)
        .context("reason task output omitted its typed summary")?;
    let observed_frames = summary
        .get("observed_frames")
        .and_then(Value::as_u64)
        .context("reason task summary omitted observed_frames")?;
    ensure!(
        observed_frames > 0,
        "reason task observed no GPU frames: {output}"
    );
    for artifact in ["results_artifact", "annotations_artifact"] {
        ensure!(
            output.get(artifact).is_some_and(Value::is_object),
            "reason task omitted {artifact}: {output}"
        );
    }
    println!(
        "reason GPU smoke ok: recording {recording_id}, {observed_frames} observed frames, typed artifacts published"
    );
    cleanup.remove_on_drop();
    Ok(())
}

fn validate_reason_workspace(
    environment: &std::collections::BTreeMap<String, String>,
) -> Result<()> {
    let config_dir = PathBuf::from(required_environment(environment, "REASON_CONFIG_DIR")?);
    let model_dir = PathBuf::from(required_environment(environment, "REASON_MODEL_DIR")?);
    let catalog_path = config_dir.join("catalog.json");
    ensure!(
        catalog_path.is_file(),
        "reason catalog is missing: {}",
        catalog_path.display()
    );
    let catalog: Value = serde_json::from_slice(&std::fs::read(&catalog_path)?)?;
    let model_path = catalog
        .pointer("/models/0/model_path")
        .and_then(Value::as_str)
        .context("reason catalog has no first model_path")?;
    let model_name = Path::new(model_path)
        .file_name()
        .context("catalog model_path has no file name")?;
    let host_model = model_dir.join(model_name);
    ensure!(
        host_model.exists(),
        "world-model checkpoint is missing: {}",
        host_model.display()
    );
    Ok(())
}

async fn wait_for_reason() -> Result<()> {
    let client = reqwest::Client::new();
    for _ in 0..90 {
        if client
            .get(REASON_READY_URL)
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    let logs = kubernetes_logs("deployment/reason-mcp")
        .unwrap_or_else(|error| format!("failed to collect logs: {error:#}"));
    bail!("reason MCP did not become ready\n{logs}")
}

/// The MCP route must reject callers without a gateway-signed internal token.
async fn assert_unauthenticated_rejected() -> Result<()> {
    let response = reqwest::Client::new()
        .post(REASON_MCP_URL)
        .header("content-type", "application/json")
        .body(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#)
        .send()
        .await
        .context("probing reason MCP without authorization")?;
    ensure!(
        response.status() == reqwest::StatusCode::UNAUTHORIZED,
        "reason MCP accepted an unauthenticated request: {}",
        response.status()
    );
    Ok(())
}
