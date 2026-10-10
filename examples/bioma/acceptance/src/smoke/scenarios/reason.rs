use super::candidate;
use super::stream::{
    PortForwardGuard, issue_internal_token, kubernetes_logs, load_environment,
    optional_environment, prepare_sample_h264, publish_h264_recording, recording_forwarder,
    recording_producer_key, required_environment, wait_for_recording_forwarder,
    wait_for_recording_source,
};
use super::*;
use anyhow::{Context, Result, bail, ensure};
use rmcp::model::ContentBlock;
use serde_json::json;
use std::path::Path;
use std::time::Duration;
use veoveo_reason_mcp::contract::{AnalyzeRecordingOutput, ReasoningResults};
#[path = "reason/public.rs"]
mod public;

const REASON_MCP_URL: &str = "http://127.0.0.1:8803/reason/mcp";

const REASON_READY_URL: &str = "http://127.0.0.1:8803/reason/readyz";

const REASON_HOST: &str = "reason-mcp:8803";

pub(crate) async fn reason_gpu(
    installation: &InstalledTarget,
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
    let context = &installation.target.kubernetes.context;
    let namespace = &installation.target.kubernetes.namespace;
    let public_input = std::env::var_os("VEOVEO_REASON_PUBLIC_CALLER_INPUT")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            environment
                .get("VEOVEO_REASON_PUBLIC_CALLER_INPUT")
                .map(std::path::PathBuf::from)
        });
    let mut public = public_input
        .as_deref()
        .map(|path| public::Profile::load(path, installation, candidate_inputs.is_some()))
        .transpose()?;
    let sample_h264 = prepare_sample_h264(work_dir, installation)?;
    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    let producer_key = recording_producer_key(context, namespace, producer_key_secret, &tmpdir)?;
    let queue_dir = tmpdir.join("forwarder-queue");
    let forwarder_log = tmpdir.join("recording-forwarder.log");
    std::fs::create_dir_all(&queue_dir)?;
    let gateway_url = installation.public_base();
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
                context,
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
            "--context".into(),
            context.into(),
            "--request-timeout=30s".into(),
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
        &recording_forwarder()?,
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
    let _reason_forward =
        PortForwardGuard::spawn(context, namespace, &resource, 8803, remote_port)?;
    let _surreal_forward = PortForwardGuard::spawn(context, namespace, "surrealdb", 8000, 8000)?;
    wait_for_reason(context, namespace, &mut candidate, work_dir).await?;
    if let Some(candidate) = &candidate {
        candidate.verify_listener()?;
    }
    assert_unauthenticated_rejected().await?;

    let recording_key = uuid::Uuid::now_v7().to_string();
    publish_h264_recording(&recording_key, &sample_h264).await?;
    let recording_id =
        wait_for_recording_source(&environment, installation, &recording_key, &queue_dir).await?;
    let arguments = json!({
        "video": {
            "recordingUri": veoveo_recording_contract::RecordingUri::new(recording_id),
            "entityPath": "/world/camera/front",
            "timeline": "sensor_time",
            "range": {"start": 0, "end": 3_000_000_000_i64}
        },
        "pipelineId": "video-reasoning",
        "task": {
            "kind": "describe_segment",
            "prompt": "Describe the road scene and any moving vehicles."
        },
        "sampling": {"maxFrames": 16}
    });

    let request: veoveo_reason_mcp::contract::AnalyzeRecordingRequest =
        serde_json::from_value(arguments.clone()).context("admitting Reason owner request")?;
    let mut direct_client = None;
    let task_client = if let Some(public) = &public {
        &public.client
    } else {
        // This profile deliberately addresses the direct or candidate owner endpoint.
        let signing_key =
            required_environment(&environment, "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64")?;
        let signing_key_id = required_environment(&environment, "VEOVEO_INTERNAL_SIGNING_KEY_ID")?;
        let bearer_token = issue_internal_token(
            signing_key,
            signing_key_id,
            "reason",
            "reason-gpu-smoke",
            &environment,
            installation,
        )
        .await?;
        // Keep the owned direct client alive across all owner resource assertions.
        direct_client
            .insert(FinalTaskSmokeClient::new(REASON_MCP_URL, bearer_token).with_host(REASON_HOST))
    };
    let tool = if public.is_some() {
        "reason__analyze_recording"
    } else {
        "analyze_recording"
    };
    if let Some(public) = &public {
        public.record(public::ObservationKind::TaskDispatchIntent, None, &[], None)?;
    }
    let task = task_client
        .run_tool_delivered_observed(tool, arguments, Duration::from_secs(600), |task| {
            if let Some(public) = &public {
                public.record(
                    public::ObservationKind::TaskAcknowledged,
                    Some(task),
                    &[],
                    None,
                )?;
            }
            Ok(())
        })
        .await;
    let output = match task {
        Ok(output) => output,
        Err(error) => {
            if public.is_some() {
                bail!("public Reason Task delivery failed; private outcome retained");
            }
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
            let logs = kubernetes_logs(context, namespace, "deployment/reason-mcp")
                .unwrap_or_else(|log_error| format!("failed to collect logs: {log_error:#}"));
            bail!("reason MCP task failed: {error:#}\nKubernetes logs:\n{logs}");
        }
    };
    ensure!(
        output.statuses.last() == Some(&rmcp::model::TaskStatus::Completed),
        "Reason delivery did not complete"
    );
    if let Some(public) = &public {
        public.record(
            public::ObservationKind::DeliveredTaskCompleted,
            Some(&output.task_id),
            &output.statuses,
            None,
        )?;
    }
    let delivered_id = output.task_id;
    let delivered_statuses = output.statuses;
    let output = output.result;
    let result: AnalyzeRecordingOutput = serde_json::from_value(
        output
            .structured_content
            .context("Reason task omitted structured content")?,
    )
    .context("Reason task returned an invalid canonical output")?;
    ensure!(
        matches!(output.content.as_slice(), [ContentBlock::Text(status), ContentBlock::ResourceLink(link)]
            if status.text == "Analysis completed." && link.uri == result.result_uri().to_string()),
        "Reason terminal content must contain the status and one canonical result link"
    );
    result.check_request(&request)?;
    let selected_resource = public::resource_identity(&result)?;
    let observed_frames = result.summary.observed_frames;
    ensure!(observed_frames > 0, "Reason task observed no GPU frames");
    let resource: ReasoningResults = task_client
        .read_resource(&result.result_uri().to_uri())
        .await?;
    ensure!(
        resource.pipeline_id == *result.pipeline_uri.id()
            && resource.model_id == *result.model_uri.id()
            && resource.observed_frames == observed_frames
            && resource.elapsed_ms == result.summary.elapsed_ms
            && resource.answer.event_count() == result.summary.event_count
            && resource.requested_range.start == result.summary.requested_start_index
            && resource.requested_range.end == result.summary.requested_end_index,
        "Reason canonical resource disagrees with the terminal completion"
    );
    result.check_results(&request, &result.results_artifact, &resource)?;
    let snapshot = if let Some(public) = &public {
        public.record(
            public::ObservationKind::ResourceSnapshotIntent,
            Some(&delivered_id),
            &delivered_statuses,
            Some(&selected_resource),
        )?;
        Some(
            task_client
                .resource_snapshot_delivery(&selected_resource, Duration::from_secs(45))
                .await?,
        )
    } else {
        None
    };
    if let Some(public) = &mut public {
        public.record(
            public::ObservationKind::DeliveredTaskCompleted,
            Some(&delivered_id),
            &delivered_statuses,
            Some(&selected_resource),
        )?;
        let snapshot = snapshot.context("public Reason snapshot was not observed")?;
        ensure!(
            snapshot.uri == selected_resource,
            "Reason update delivered a different resource"
        );
        public.record(
            public::ObservationKind::AcknowledgedInitialCurrentResourceSnapshot,
            Some(&delivered_id),
            &delivered_statuses,
            Some(&snapshot.uri),
        )?;
    }
    if let Some(candidate) = candidate.as_mut() {
        candidate.finish(
            work_dir,
            candidate::ProbeOutcome::ReasonGpuQualified {
                result: Box::new(result),
            },
        )?;
    }
    println!(
        "reason GPU smoke ok: recording {recording_id}, {observed_frames} observed frames, typed artifacts published"
    );
    recording_forwarder.drain(Duration::from_secs(40)).await?;
    if let Some(public) = &mut public {
        public.complete()?;
    }
    cleanup.remove_on_drop();
    Ok(())
}

async fn wait_for_reason(
    context: &str,
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
    let logs = kubernetes_logs(context, namespace, "deployment/reason-mcp")
        .unwrap_or_else(|error| format!("failed to collect logs: {error:#}"));
    bail!("reason MCP did not become ready\n{logs}")
}

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
