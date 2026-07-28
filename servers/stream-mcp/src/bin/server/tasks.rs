use std::collections::BTreeSet;
use std::num::{NonZeroU32, NonZeroU64};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, ensure};
use chrono::{TimeDelta, Utc};
use rmcp::model::{CallToolResult, ContentBlock};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::{
    GatewayInternalIdentity, IssueArtifactWriteCapabilityRequest, IssuedArtifactWriteCapability,
    PlaneCaller,
};
use veoveo_recording_video::{
    materialize_video, recording_id_from_uri, timeline_kind, validate_video_selection,
};
use veoveo_stream_mcp::{
    annotation::write_annotation_rrd,
    contract::{RecordingVideoSelection, RunRecordingRequest, SamplingPolicy},
};
use veoveo_task_runtime::{
    CreateTask as DurableCreateTask, RecoveryClass, TaskFailure, TaskId, TaskPayloadState,
    TaskRetentionPin, TaskSnapshot, TaskTransition,
};

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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub(super) enum StreamTaskInput {
    RunRecording(RunRecordingRequest),
}

impl StreamTaskInput {
    fn video(&self) -> &RecordingVideoSelection {
        match self {
            Self::RunRecording(request) => &request.video,
        }
    }

    fn task_type(&self) -> &'static str {
        "run_recording"
    }

    fn artifact_count(&self) -> NonZeroU32 {
        NonZeroU32::new(match self {
            Self::RunRecording(request) if request.include_source_clip => 3,
            Self::RunRecording(_) => 2,
        })
        .expect("stream recording runs always publish artifacts")
    }

    fn recovery_class(&self) -> RecoveryClass {
        RecoveryClass::Resume
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct DurableStreamRequest {
    pub(super) input: StreamTaskInput,
    pub(super) artifact_write_capability: IssuedArtifactWriteCapability,
}

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
                task_id: task_id.to_string(),
                expires_at: Utc::now() + ARTIFACT_CAPABILITY_TTL,
                max_artifact_count: input.artifact_count(),
                max_total_bytes: NonZeroU64::new(state.max_artifact_bytes)
                    .ok_or_else(|| "max artifact bytes must be non-zero".to_owned())?,
            },
        )
        .await
        .map_err(|error| error.to_string())?;
    let recovery_class = input.recovery_class();
    let task_type = input.task_type().to_owned();
    let request = DurableStreamRequest {
        input,
        artifact_write_capability: capability,
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
    let request: DurableStreamRequest = serde_json::from_value(snapshot.request.clone())?;
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
    authority: veoveo_recording_mcp::RecordingReadAuthority,
    progress: Option<TaskProgress>,
) -> Result<TaskSnapshot> {
    let task_id = snapshot.task_id.to_string();
    let claimed = state.tasks.claim(&task_id, TASK_LEASE_DURATION).await?;
    let cancellation = CancellationToken::new();
    let join = tokio::spawn(run_task(
        state.clone(),
        task_id.clone(),
        request,
        authority,
        progress,
        cancellation.clone(),
    ));
    state
        .tasks
        .register_worker(&task_id, cancellation, join)
        .await?;
    Ok(claimed.snapshot)
}

async fn run_task(
    state: Arc<AppState>,
    task_id: String,
    request: DurableStreamRequest,
    authority: veoveo_recording_mcp::RecordingReadAuthority,
    progress: Option<TaskProgress>,
    cancellation: CancellationToken,
) {
    let work = run_task_inner(
        state.clone(),
        task_id.clone(),
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
                if let Err(error) = state.tasks.renew_lease(&task_id, TASK_LEASE_DURATION).await {
                    tracing::warn!(task_id, "stream task lease heartbeat failed: {error}");
                    cancellation.cancel();
                    break;
                }
            }
        }
    }
}

async fn run_task_inner(
    state: Arc<AppState>,
    task_id: String,
    request: DurableStreamRequest,
    authority: veoveo_recording_mcp::RecordingReadAuthority,
    progress: Option<TaskProgress>,
    cancellation: CancellationToken,
) {
    macro_rules! fail {
        ($message:expr) => {{
            let message: String = $message;
            tracing::warn!(task_id, "stream task failed: {message}");
            complete_tool_error(&state, &task_id, message).await;
            return;
        }};
    }

    set_progress(
        &state,
        &task_id,
        &progress,
        0.02,
        "waiting for local stream capacity",
    )
    .await;
    let work_slot = tokio::select! {
        permit = state.work_slots.clone().acquire_owned() => match permit {
            Ok(permit) => permit,
            Err(error) => fail!(format!("stream work queue closed: {error}")),
        },
        () = cancellation.cancelled() => {
            update_task(&state, &task_id, TaskTransition::Cancelled).await;
            return;
        }
    };
    let _work_slot = work_slot;
    set_progress(
        &state,
        &task_id,
        &progress,
        0.1,
        "resolving governed recording",
    )
    .await;
    let video = request.input.video().clone();
    let materialize = materialize_video(
        state.recordings.clone(),
        authority,
        video.clone(),
        state.source_limits.clone(),
    );
    let source = tokio::select! {
        result = materialize => match result {
            Ok(source) => source,
            Err(error) => fail!(format!("video materialization failed: {error:#}")),
        },
        () = cancellation.cancelled() => {
            update_task(&state, &task_id, TaskTransition::Cancelled).await;
            return;
        }
    };
    set_progress(&state, &task_id, &progress, 0.35, "video clip materialized").await;
    let result = match request.input {
        StreamTaskInput::RunRecording(input) => {
            let Some(pipeline) = state.catalog.pipeline(&input.pipeline_id).cloned() else {
                fail!(format!("unknown pipeline `{}`", input.pipeline_id));
            };
            let Some(model_id) = pipeline.profile.model_id() else {
                fail!(format!(
                    "pipeline `{}` does not expose typed perception results",
                    pipeline.id
                ));
            };
            let Some(model) = state.catalog.model(model_id).cloned() else {
                fail!(format!("pipeline model `{}` disappeared", model_id));
            };
            let work = match tempfile::Builder::new()
                .prefix("veoveo-stream-recording-task-")
                .tempdir()
            {
                Ok(work) => work,
                Err(error) => fail!(format!("creating task workspace failed: {error}")),
            };
            let input_path = work.path().join("input.mp4");
            if let Err(error) = tokio::fs::write(&input_path, &source.mp4).await {
                fail!(format!("writing runner input failed: {error}"));
            }
            set_progress(
                &state,
                &task_id,
                &progress,
                0.45,
                "running DeepStream inference",
            )
            .await;
            let timeline_kind = match timeline_kind(&source.clip) {
                Ok(kind) => kind,
                Err(error) => fail!(format!("{error:#}")),
            };
            let execute =
                state
                    .executor
                    .analyze(veoveo_stream_mcp::executor::StreamAnalysisRequest {
                        task_id: &task_id,
                        input_mp4: &input_path,
                        decode_start_index: source.clip.decode_start_index,
                        input_width: source.clip.width,
                        input_height: source.clip.height,
                        timeline_kind,
                        video: &input.video,
                        pipeline: &pipeline,
                        model: &model,
                        sampling: input.sampling,
                    });
            let analysis = tokio::select! {
                result = execute => match result {
                    Ok(result) => result,
                    Err(error) => fail!(format!("DeepStream analysis failed: {error:#}")),
                },
                () = cancellation.cancelled() => {
                    update_task(&state, &task_id, TaskTransition::Cancelled).await;
                    return;
                }
            };
            set_progress(
                &state,
                &task_id,
                &progress,
                0.8,
                "writing derived annotation layer",
            )
            .await;
            let annotation_task_id = task_id.clone();
            let annotation_results = analysis.clone();
            let annotations_rrd = match tokio::task::spawn_blocking(move || {
                write_annotation_rrd(&annotation_task_id, &annotation_results)
            })
            .await
            {
                Ok(Ok(bytes)) => bytes,
                Ok(Err(error)) => fail!(format!("annotation RRD failed: {error:#}")),
                Err(error) => fail!(format!("annotation worker failed: {error}")),
            };
            publish_analysis(
                &state,
                &request.artifact_write_capability,
                &task_id,
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
        update_task(&state, &task_id, TaskTransition::Cancelled).await;
        return;
    }
    let result = match result {
        Ok(result) => result,
        Err(error) => fail!(format!("publishing stream artifacts failed: {error:#}")),
    };
    notify_progress(&progress, 1.0, "completed").await;
    let payload = match serde_json::to_value(result) {
        Ok(payload) => payload,
        Err(error) => fail!(format!("serializing stream result failed: {error}")),
    };
    update_task(
        &state,
        &task_id,
        TaskTransition::Succeeded {
            message: "completed; stream artifacts available".to_owned(),
            result: payload,
        },
    )
    .await;
}

async fn set_progress(
    state: &AppState,
    task_id: &str,
    progress: &Option<TaskProgress>,
    value: f64,
    message: &str,
) {
    if let Err(error) = state
        .tasks
        .transition(
            task_id,
            TaskTransition::Running {
                message: message.to_owned(),
                progress: value,
            },
        )
        .await
    {
        tracing::warn!(task_id, "failed to persist stream progress: {error}");
    }
    state
        .subscribers
        .notify_resource_updated(veoveo_stream_mcp::uris::run_uri(task_id))
        .await;
    notify_progress(progress, value, message).await;
}

async fn notify_progress(progress: &Option<TaskProgress>, value: f64, message: &str) {
    if let Some(progress) = progress {
        veoveo_mcp_contract::notify_progress(&progress.peer, &progress.token, value, message).await;
    }
}

async fn complete_tool_error(state: &AppState, task_id: &str, message: String) {
    let result = CallToolResult::error(vec![ContentBlock::text(message.clone())]);
    let transition = match serde_json::to_value(result) {
        Ok(result) => TaskTransition::Succeeded { message, result },
        Err(error) => TaskTransition::Failed(TaskFailure::new(
            "result_serialization_failed",
            error.to_string(),
        )),
    };
    update_task(state, task_id, transition).await;
}

fn validate_input(state: &AppState, input: &StreamTaskInput) -> Result<()> {
    recording_id_from_uri(&input.video().recording_uri)?;
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

pub(super) async fn completed_payload(
    state: &AppState,
    task_id: &str,
) -> Result<CallToolResult, rmcp::ErrorData> {
    match state
        .tasks
        .await_payload_state(task_id)
        .await
        .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None))?
    {
        TaskPayloadState::Completed(payload) => serde_json::from_value(payload).map_err(|error| {
            rmcp::ErrorData::internal_error(
                format!("invalid persisted stream result: {error}"),
                None,
            )
        }),
        TaskPayloadState::Failed(error) => Err(rmcp::ErrorData::internal_error(
            error.message,
            error.details,
        )),
        TaskPayloadState::Cancelled => Err(rmcp::ErrorData::invalid_request(
            "stream task was cancelled",
            None,
        )),
        TaskPayloadState::Running => Err(rmcp::ErrorData::internal_error(
            "stream task wait ended while still running",
            None,
        )),
        TaskPayloadState::Unknown => Err(rmcp::ErrorData::internal_error(
            "stream task disappeared before completion",
            None,
        )),
    }
}
