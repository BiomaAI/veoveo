use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use veoveo_mcp_contract::ArtifactMetadata;

use super::candidate;
use super::stream::{
    PortForwardGuard, RECORDING_FORWARDER, issue_internal_token, kubernetes_logs,
    kubernetes_namespace, load_environment, optional_environment, prepare_sample_h264,
    publish_h264_recording, recording_producer_key, required_environment,
    wait_for_recording_forwarder, wait_for_recording_source,
};
use super::*;

const REASON_MCP_URL: &str = "http://127.0.0.1:8803/reason/mcp";
const REASON_READY_URL: &str = "http://127.0.0.1:8803/reason/readyz";
const REASON_HOST: &str = "reason-mcp:8803";

#[derive(Deserialize, Serialize)]
pub(crate) struct ReasonOutput {
    analysis_uri: String,
    results_uri: String,
    pipeline_uri: String,
    model_uri: String,
    summary: ReasonSummary,
    results_artifact: ArtifactMetadata,
    annotations_artifact: ArtifactMetadata,
    source_clip_artifact: Option<ArtifactMetadata>,
}

#[derive(Deserialize, Serialize)]
struct ReasonSummary {
    observed_frames: u64,
    event_count: u64,
    elapsed_ms: u64,
    decode_start_index: i64,
    requested_start_index: i64,
    requested_end_index: i64,
}

pub(crate) async fn reason_gpu(
    env_file: &Path,
    work_dir: &Path,
    producer_key_secret: &str,
    candidate_inputs: Option<(&Path, &Path)>,
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

    let mut candidate = candidate_inputs
        .map(|(binary, runner)| {
            ensure!(
                runner.join("reason_runner/main.py").is_file(),
                "candidate runner source is missing"
            );
            std::fs::create_dir_all(work_dir)?;
            let archive = work_dir.join("candidate-runner.pyz");
            run_checked(
                Path::new("python3"),
                [
                    "-m".into(),
                    "zipapp".into(),
                    runner.as_os_str().to_owned(),
                    "--main".into(),
                    "reason_runner.main:main".into(),
                    "--python".into(),
                    "/usr/bin/env python3".into(),
                    "--output".into(),
                    archive.as_os_str().to_owned(),
                ],
                [],
            )?;
            candidate::Candidate::start(
                namespace,
                candidate::Service::Reason,
                binary,
                &archive,
                work_dir,
            )
        })
        .transpose()?;

    run_checked(
        Path::new("kubectl"),
        [
            "-n".into(),
            namespace.into(),
            "rollout".into(),
            "status".into(),
            "deployment/reason-mcp".into(),
            "--timeout=300s".into(),
        ],
        [],
    )
    .context("reason GPU smoke requires the reason workload with its runner image and engine")?;
    let mut recording_forwarder = ChildGuard::spawn(
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
        [("RUST_LOG", "veoveo_recording_forwarder=info".into())],
        &forwarder_log,
    )
    .with_context(|| {
        format!(
            "starting authenticated recording forwarder; logs: {}",
            forwarder_log.display()
        )
    })?
    .with_drain_on_drop(Duration::from_secs(40));
    wait_for_recording_forwarder(&forwarder_log).await?;
    let remote_port = if candidate.is_some() {
        candidate::Service::Reason.port()
    } else {
        8803
    };
    let resource = candidate
        .as_ref()
        .map(candidate::Candidate::resource)
        .unwrap_or_else(|| "reason-mcp".to_owned());
    let _reason_forward = PortForwardGuard::spawn(namespace, &resource, 8803, remote_port)?;
    let _surreal_forward = PortForwardGuard::spawn(namespace, "surrealdb", 8000, 8000)?;
    wait_for_reason(namespace, &mut candidate, work_dir).await?;
    if let Some(candidate) = &candidate {
        candidate.verify_listener()?;
    }
    assert_unauthenticated_rejected().await?;

    let recording_key = uuid::Uuid::now_v7().to_string();
    publish_h264_recording(&recording_key, &sample_h264).await?;
    let recording_id = wait_for_recording_source(&environment, &recording_key, &queue_dir).await?;
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

    let bearer_token = issue_internal_token(
        signing_key,
        signing_key_id,
        "reason",
        "reason-gpu-smoke",
        &environment,
    )
    .await?;
    let task_client =
        FinalTaskSmokeClient::new(REASON_MCP_URL, bearer_token).with_host(REASON_HOST);
    let task = task_client
        .run_tool_structured("analyze_recording", arguments, Duration::from_secs(600))
        .await;
    let output = match task {
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
                    "Reason compiler workload failed: {error:#}; candidate logs: {}",
                    work_dir.display()
                );
            }
            let logs = kubernetes_logs(namespace, "deployment/reason-mcp")
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
    if let Some(candidate) = candidate.as_mut() {
        candidate.finish(
            work_dir,
            candidate::ProbeOutcome::ReasonGpuQualified {
                result: Box::new(serde_json::from_value(output.clone())?),
            },
        )?;
    }
    println!(
        "reason GPU smoke ok: recording {recording_id}, {observed_frames} observed frames, typed artifacts published"
    );
    recording_forwarder.drain(Duration::from_secs(40)).await?;
    cleanup.remove_on_drop();
    Ok(())
}

async fn wait_for_reason(
    namespace: &str,
    candidate: &mut Option<candidate::Candidate>,
    work_dir: &Path,
) -> Result<()> {
    let client = reqwest::Client::new();
    for _ in 0..90 {
        if let Some(candidate) = candidate {
            candidate.check_running(work_dir)?;
        }
        if client
            .get(REASON_READY_URL)
            .header(reqwest::header::HOST, REASON_HOST)
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    let logs = kubernetes_logs(namespace, "deployment/reason-mcp")
        .unwrap_or_else(|error| format!("failed to collect logs: {error:#}"));
    bail!("reason MCP did not become ready\n{logs}")
}

/// The MCP route must reject callers without a gateway-signed internal token.
async fn assert_unauthenticated_rejected() -> Result<()> {
    let response = reqwest::Client::new()
        .post(REASON_MCP_URL)
        .header(reqwest::header::HOST, REASON_HOST)
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
