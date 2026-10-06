use std::collections::BTreeSet;
use std::num::{NonZeroU32, NonZeroU64};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, ensure};
use chrono::{TimeDelta, Utc};
use rmcp::model::{CallToolResult, ContentBlock};
use tokio_util::sync::CancellationToken;
use veoveo_artifact_contract::{
    ArtifactTaskId, IssueArtifactReadCapabilityRequest, IssueArtifactWriteCapabilityRequest,
};
use veoveo_mcp_contract::{ArtifactReadAuthority, GatewayInternalIdentity, PlaneCaller};
use veoveo_recording_video::contract::validate_video_selection;
use veoveo_recording_video::runtime::{materialize_video, timeline_kind};
use veoveo_stream_mcp::{
    annotation::write_annotation_rrd,
    contract::{RunId, SamplingPolicy},
};
use veoveo_task_runtime::{
    CreateTask as DurableCreateTask, TaskFailure, TaskRetentionPin, TaskSnapshot, TaskTransition,
};
use veoveo_types::TaskId;

use super::app_state::{AppState, update_task};
use super::outputs::{AnalysisProducts, publish_analysis};
use super::ownership::{
    recording_authority_from_identity, recording_authority_from_runtime, runtime_owner,
};

pub(super) const MCP_TASK_POLL_INTERVAL_MS: u64 = 3_000;
pub(super) const MCP_TASK_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
const TASK_LEASE_DURATION: Duration = Duration::from_secs(120);
const TASK_LEASE_HEARTBEAT: Duration = Duration::from_secs(40);
const ARTIFACT_CAPABILITY_TTL: TimeDelta = TimeDelta::hours(24);
pub(super) const SERVER_SLUG: &str = "stream";

pub(super) use veoveo_stream_mcp::task_request::{DurableStreamRequest, StreamTaskInput};

pub(super) struct TaskProgress {
    pub(super) peer: rmcp::service::Peer<rmcp::RoleServer>,
    pub(super) token: Option<rmcp::model::ProgressToken>,
}

pub(super) async fn start_stream_task(
    state: Arc<AppState>,
    identity: GatewayInternalIdentity,
    caller: PlaneCaller,
    input: StreamTaskInput,
    progress: Option<TaskProgress>,
    retention_pins: BTreeSet<TaskRetentionPin>,
) -> Result<TaskSnapshot, String> {
    validate_input(&state, &input).map_err(|error| error.to_string())?;
    let task_id = TaskId::new();
    let capability = state
        .artifacts
        .issue_write_capability(
            &caller,
            &IssueArtifactWriteCapabilityRequest {
                required_data_labels: Default::default(),
                task_id: veoveo_artifact_contract::ArtifactTaskId::try_from(task_id.as_uuid())
                    .map_err(|error| error.to_string())?,
                expires_at: Utc::now() + ARTIFACT_CAPABILITY_TTL,
                max_artifact_count: input.artifact_count(),
                max_total_bytes: NonZeroU64::new(state.max_artifact_bytes)
                    .ok_or_else(|| "max artifact bytes must be non-zero".to_owned())?,
            },
        )
        .await
        .map_err(|error| error.to_string())?;
    let read_capability = state
        .artifacts
        .issue_read_capability(
            &caller,
            &IssueArtifactReadCapabilityRequest {
                task_id: ArtifactTaskId::parse(task_id.to_string())
                    .map_err(|error| error.to_string())?,
                expires_at: Utc::now() + ARTIFACT_CAPABILITY_TTL,
                max_artifact_count: NonZeroU32::new(veoveo_recording_reader::MAX_LAYERS)
                    .expect("positive recording layer limit"),
                max_total_bytes: NonZeroU64::new(state.source_limits.max_segment_bytes)
                    .ok_or_else(|| "max segment bytes must be non-zero".to_owned())?,
            },
        )
        .await
        .map_err(|error| error.to_string())?;
    let recovery_class = input.recovery_class();
    let task_type = input.task_type();
    let request = DurableStreamRequest {
        input,
        artifact_write_capability: capability,
        artifact_read_capability: read_capability,
    };
    let created = state
        .tasks
        .create(DurableCreateTask {
            task_id,
            owner: runtime_owner(&identity),
            server: SERVER_SLUG.to_owned(),
            task_type,
            request: serde_json::to_value(&request).map_err(|error| error.to_string())?,
            recovery_class,
            idempotency_key: None,
            ttl_ms: Some(MCP_TASK_TTL_MS),
            poll_interval_ms: Some(MCP_TASK_POLL_INTERVAL_MS),
            retention_pins,
        })
        .await
        .map_err(|error| error.to_string())?;
    schedule_task(
        state,
        created.snapshot,
        request,
        recording_authority_from_identity(&identity),
        progress,
    )
    .await
    .map_err(|error| error.to_string())
}

pub(super) async fn resume_task(state: Arc<AppState>, snapshot: TaskSnapshot) -> Result<()> {
    let request: DurableStreamRequest = match serde_json::from_value(snapshot.request.clone()) {
        Ok(request) => request,
        Err(error) => {
            let task_id = RunId::try_from(snapshot.task_id)?;
            state
                .tasks
                .claim(task_id.task_id(), TASK_LEASE_DURATION)
                .await?;
            state
                .tasks
                .transition(
                    task_id.task_id(),
                    TaskTransition::Failed(TaskFailure::new(
                        "invalid_task_request",
                        error.to_string(),
                    )),
                )
                .await?;
            return Ok(());
        }
    };
    let authority =
        recording_authority_from_runtime(&snapshot.owner).map_err(anyhow::Error::msg)?;
    schedule_task(state, snapshot, request, authority, None)
        .await
        .map(|_| ())
}

async fn schedule_task(
    state: Arc<AppState>,
    snapshot: TaskSnapshot,
    request: DurableStreamRequest,
    authority: veoveo_recording_reader::RecordingReadAuthority,
    progress: Option<TaskProgress>,
) -> Result<TaskSnapshot> {
    let task_id = RunId::try_from(snapshot.task_id)?;
    let claimed = state
        .tasks
        .claim(task_id.task_id(), TASK_LEASE_DURATION)
        .await?;
    let cancellation = CancellationToken::new();
    let join = tokio::spawn(run_task(
        state.clone(),
        task_id,
        request,
        authority,
        progress,
        cancellation.clone(),
    ));
    state
        .tasks
        .register_worker(task_id.task_id(), cancellation, join)
        .await?;
    Ok(claimed.snapshot)
}

async fn run_task(
    state: Arc<AppState>,
    task_id: RunId,
    request: DurableStreamRequest,
    authority: veoveo_recording_reader::RecordingReadAuthority,
    progress: Option<TaskProgress>,
    cancellation: CancellationToken,
) {
    let work = run_task_inner(
        state.clone(),
        task_id,
        request,
        authority,
        progress,
        cancellation.clone(),
    );
    tokio::pin!(work);
    let mut heartbeat = tokio::time::interval(TASK_LEASE_HEARTBEAT);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    heartbeat.tick().await;
    loop {
        tokio::select! {
            () = &mut work => break,
            _ = heartbeat.tick() => {
                if let Err(error) = state.tasks.renew_lease(task_id.task_id(), TASK_LEASE_DURATION).await {
                    tracing::warn!(%task_id, "stream task lease heartbeat failed: {error}");
                    cancellation.cancel();
                    break;
                }
            }
        }
    }
}

async fn run_task_inner(
    state: Arc<AppState>,
    task_id: RunId,
    request: DurableStreamRequest,
    authority: veoveo_recording_reader::RecordingReadAuthority,
    progress: Option<TaskProgress>,
    cancellation: CancellationToken,
) {
    set_progress(
        &state,
        task_id,
        &progress,
        0.02,
        "waiting for local stream capacity",
    )
    .await;
    let work_slot = tokio::select! {
        permit = state.work_slots.clone().acquire_owned() => match permit {
            Ok(permit) => permit,
            Err(error) => {
                fail_task(&state, task_id, format!("stream work queue closed: {error}")).await;
                return;
            },
        },
        () = cancellation.cancelled() => {
            update_task(&state, task_id, TaskTransition::Cancelled).await;
            return;
        }
    };
    let _work_slot = work_slot;
    set_progress(
        &state,
        task_id,
        &progress,
        0.1,
        "resolving governed recording",
    )
    .await;
    let video = request.input.video().clone();
    let materialize = materialize_video(
        state.recordings.clone(),
        authority,
        ArtifactReadAuthority::Task {
            capability: &request.artifact_read_capability,
            task_id: ArtifactTaskId::parse(task_id.to_string()).expect("durable task ID is UUIDv7"),
        },
        video.clone(),
        state.source_limits.clone(),
    );
    let source = tokio::select! {
        result = materialize => match result {
            Ok(source) => source,
            Err(error) => {
                fail_task(&state, task_id, format!("video materialization failed: {error:#}")).await;
                return;
            },
        },
        () = cancellation.cancelled() => {
            update_task(&state, task_id, TaskTransition::Cancelled).await;
            return;
        }
    };
    set_progress(&state, task_id, &progress, 0.35, "video clip materialized").await;
    let result = match request.input {
        StreamTaskInput::RunRecording(input) => {
            let Some(pipeline) = state.catalog.pipeline(&input.pipeline_id).cloned() else {
                fail_task(
                    &state,
                    task_id,
                    format!("unknown pipeline `{}`", input.pipeline_id),
                )
                .await;
                return;
            };
            let Some(model_id) = pipeline.profile.model_id() else {
                fail_task(
                    &state,
                    task_id,
                    format!(
                        "pipeline `{}` does not expose typed perception results",
                        pipeline.id
                    ),
                )
                .await;
                return;
            };
            let Some(model) = state.catalog.model(model_id).cloned() else {
                fail_task(
                    &state,
                    task_id,
                    format!("pipeline model `{}` disappeared", model_id),
                )
                .await;
                return;
            };
            let work = match tempfile::Builder::new()
                .prefix("veoveo-stream-recording-task-")
                .tempdir()
            {
                Ok(work) => work,
                Err(error) => {
                    fail_task(
                        &state,
                        task_id,
                        format!("creating task workspace failed: {error}"),
                    )
                    .await;
                    return;
                }
            };
            let input_path = work.path().join("input.mp4");
            if let Err(error) = tokio::fs::write(&input_path, &source.mp4).await {
                fail_task(
                    &state,
                    task_id,
                    format!("writing runner input failed: {error}"),
                )
                .await;
                return;
            }
            set_progress(
                &state,
                task_id,
                &progress,
                0.45,
                "running DeepStream inference",
            )
            .await;
            let timeline_kind = match timeline_kind(&source.clip) {
                Ok(kind) => kind,
                Err(error) => {
                    fail_task(&state, task_id, format!("{error:#}")).await;
                    return;
                }
            };
            let execute =
                state
                    .executor
                    .analyze(veoveo_stream_mcp::executor::StreamAnalysisRequest {
                        task_id,
                        input_mp4: &input_path,
                        decode_start_index: source.clip.decode_start_index,
                        input_width: source.clip.width,
                        input_height: source.clip.height,
                        timeline_kind,
                        video: &input.video,
                        source_snapshot: &source.source_snapshot,
                        pipeline: &pipeline,
                        model: &model,
                        sampling: input.sampling,
                    });
            let analysis = tokio::select! {
                result = execute => match result {
                    Ok(result) => result,
                    Err(error) => {
                        fail_task(&state, task_id, format!("DeepStream analysis failed: {error:#}")).await;
                        return;
                    },
                },
                () = cancellation.cancelled() => {
                    update_task(&state, task_id, TaskTransition::Cancelled).await;
                    return;
                }
            };
            set_progress(
                &state,
                task_id,
                &progress,
                0.8,
                "writing derived annotation layer",
            )
            .await;
            let annotation_task_id = task_id;
            let annotation_results = analysis.clone();
            let annotations_rrd = match tokio::task::spawn_blocking(move || {
                write_annotation_rrd(annotation_task_id, &annotation_results)
            })
            .await
            {
                Ok(Ok(bytes)) => bytes,
                Ok(Err(error)) => {
                    fail_task(&state, task_id, format!("annotation RRD failed: {error:#}")).await;
                    return;
                }
                Err(error) => {
                    fail_task(
                        &state,
                        task_id,
                        format!("annotation worker failed: {error}"),
                    )
                    .await;
                    return;
                }
            };
            publish_analysis(
                &state,
                &request.artifact_write_capability,
                task_id,
                AnalysisProducts {
                    results: analysis,
                    annotations_rrd,
                    source,
                    include_source_clip: input.include_source_clip,
                },
            )
            .await
        }
    };
    if cancellation.is_cancelled() {
        update_task(&state, task_id, TaskTransition::Cancelled).await;
        return;
    }
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            fail_task(
                &state,
                task_id,
                format!("publishing stream artifacts failed: {error:#}"),
            )
            .await;
            return;
        }
    };
    notify_progress(&progress, 1.0, "completed").await;
    let transition = match veoveo_task_runtime::mcp_task_completion(
        veoveo_stream_mcp::task_product::RUN_COMPLETED,
        result,
    ) {
        Ok(transition) => transition,
        Err(error) => {
            fail_task(
                &state,
                task_id,
                format!("serializing stream result failed: {error}"),
            )
            .await;
            return;
        }
    };
    update_task(&state, task_id, transition).await;
}

async fn set_progress(
    state: &AppState,
    task_id: RunId,
    progress: &Option<TaskProgress>,
    value: f64,
    message: &str,
) {
    if let Err(error) = state
        .tasks
        .transition(
            task_id.task_id(),
            TaskTransition::Running {
                message: message.to_owned(),
                progress: value,
            },
        )
        .await
    {
        tracing::warn!(%task_id, "failed to persist stream progress: {error}");
    }
    notify_progress(progress, value, message).await;
}

async fn notify_progress(progress: &Option<TaskProgress>, value: f64, message: &str) {
    if let Some(progress) = progress {
        veoveo_mcp_contract::notify_progress(&progress.peer, &progress.token, value, message).await;
    }
}

async fn complete_tool_error(state: &AppState, task_id: RunId, message: String) {
    let result = CallToolResult::error(vec![ContentBlock::text(message.clone())]);
    let transition = match veoveo_task_runtime::mcp_task_completion(message, result) {
        Ok(transition) => transition,
        Err(error) => TaskTransition::Failed(TaskFailure::new(
            "result_serialization_failed",
            error.to_string(),
        )),
    };
    update_task(state, task_id, transition).await;
}

fn validate_input(state: &AppState, input: &StreamTaskInput) -> Result<()> {
    validate_video_selection(input.video())?;
    match input {
        StreamTaskInput::RunRecording(request) => {
            ensure!(
                state
                    .catalog
                    .pipeline(&request.pipeline_id)
                    .is_some_and(|pipeline| pipeline.recording_replay.is_some()),
                "unknown pipeline `{}`",
                request.pipeline_id
            );
            match request.sampling {
                SamplingPolicy::EveryFrame => {}
                SamplingPolicy::EveryNth { step } => {
                    ensure!(step > 0, "sampling step must be non-zero");
                }
                SamplingPolicy::MaximumFrames { count } => {
                    ensure!(count > 0, "sampling count must be non-zero");
                }
            }
        }
    }
    Ok(())
}

async fn fail_task(state: &AppState, task_id: RunId, message: String) {
    tracing::warn!(%task_id, "stream task failed: {message}");
    complete_tool_error(state, task_id, message).await;
}
