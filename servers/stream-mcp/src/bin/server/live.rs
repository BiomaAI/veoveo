use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::UnixListener;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use veoveo_mcp_contract::{SubscriptionHub, WorkContextMembershipLevel};
use veoveo_stream_mcp::catalog::{
    GStreamerGraphConfig, ModelConfig, PipelineCatalog, PipelineProfileConfig, TrackerConfig,
};
use veoveo_stream_mcp::contract::{
    EncodedVideoChunk, FrameDetections, LivePreviewView, LiveResultFrame, LiveResultsView,
    LiveSessionLifecycle, LiveSessionView, LiveVideoView, StartLiveSessionOutput,
    StopLiveSessionOutput,
};
use veoveo_stream_mcp::{executor::validate_frame, uris};
use veoveo_task_runtime::TaskOwner;

use super::recording_output::LiveRecordingOutput;

const LIVE_RUNNER_REQUEST_SCHEMA: &str = "veoveo.stream-live-runner-request/v1";
const LIVE_RESULTS_SCHEMA: &str = "veoveo.stream-live-results/v1";
const LIVE_PREVIEW_SCHEMA: &str = "veoveo.stream-live-preview/v1";

pub(super) struct LiveSessionManager {
    catalog: Arc<PipelineCatalog>,
    runner: PathBuf,
    startup_timeout: Duration,
    max_result_frames: usize,
    max_preview_chunks: usize,
    max_detections_per_frame: usize,
    max_event_bytes: usize,
    max_video_chunk_bytes: usize,
    subscribers: Arc<SubscriptionHub>,
    sessions: Mutex<BTreeMap<String, Arc<LiveSession>>>,
    active_pipelines: Mutex<BTreeSet<String>>,
}

struct LiveSession {
    session_id: String,
    pipeline_id: String,
    pipeline_uri: String,
    ingress: veoveo_stream_mcp::contract::LiveIngressView,
    video: LiveVideoView,
    input_width: u16,
    input_height: u16,
    owner: TaskOwner,
    recording_output: Option<LiveRecordingOutput>,
    _work: tempfile::TempDir,
    state: Mutex<LiveSessionState>,
}

struct LiveSessionState {
    lifecycle: LiveSessionLifecycle,
    started_at: DateTime<Utc>,
    stopped_at: Option<DateTime<Utc>>,
    processed_frames: u64,
    dropped_result_frames: u64,
    newest_result_at: Option<DateTime<Utc>>,
    error: Option<String>,
    frames: VecDeque<LiveResultFrame>,
    video_chunks: VecDeque<EncodedVideoChunk>,
    dropped_video_chunks: u64,
    received_video_frames: u64,
    last_video_sequence: Option<u64>,
    child: Option<Child>,
}

#[derive(Deserialize)]
#[serde(tag = "schema", deny_unknown_fields)]
enum LiveRunnerEvent {
    #[serde(rename = "veoveo.stream-live-frame/v1")]
    ResultFrame { frame: FrameDetections },
    #[serde(rename = "veoveo.stream-live-video-chunk/v1")]
    VideoChunk { chunk: EncodedVideoChunk },
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct LiveRunnerRequest {
    schema: &'static str,
    session_id: String,
    input_width: u16,
    input_height: u16,
    pipeline: RunnerPipeline,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<RunnerModel>,
    max_detections_per_frame: usize,
    max_event_bytes: usize,
    max_video_chunk_bytes: usize,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct RunnerPipeline {
    pipeline_id: String,
    graph: GStreamerGraphConfig,
    profile: RunnerPipelineProfile,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum RunnerPipelineProfile {
    PassThrough,
    Perception {
        operation: veoveo_stream_mcp::contract::PerceptionOperation,
        inference_config_path: PathBuf,
        tracker: Option<RunnerTracker>,
    },
}

impl From<&PipelineProfileConfig> for RunnerPipelineProfile {
    fn from(value: &PipelineProfileConfig) -> Self {
        match value {
            PipelineProfileConfig::PassThrough => Self::PassThrough,
            PipelineProfileConfig::Perception { .. } => {
                let profile = value
                    .perception()
                    .expect("perception variant must expose its typed profile");
                Self::Perception {
                    operation: profile.operation,
                    inference_config_path: profile.inference_config_path.to_path_buf(),
                    tracker: profile.tracker.map(RunnerTracker::from),
                }
            }
        }
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct RunnerTracker {
    config_path: PathBuf,
    width: u32,
    height: u32,
}

impl From<&TrackerConfig> for RunnerTracker {
    fn from(value: &TrackerConfig) -> Self {
        Self {
            config_path: value.config_path.clone(),
            width: value.width,
            height: value.height,
        }
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct RunnerModel {
    model_id: String,
    model_path: PathBuf,
    format: veoveo_stream_mcp::contract::ModelFormat,
}

impl LiveSessionManager {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        catalog: Arc<PipelineCatalog>,
        runner: PathBuf,
        startup_timeout: Duration,
        max_result_frames: usize,
        max_preview_chunks: usize,
        max_detections_per_frame: usize,
        max_event_bytes: u64,
        max_video_chunk_bytes: u64,
        subscribers: Arc<SubscriptionHub>,
    ) -> Result<Self> {
        ensure!(
            startup_timeout > Duration::ZERO,
            "live startup timeout must be positive"
        );
        ensure!(
            max_result_frames > 0,
            "max_live_result_frames must be non-zero"
        );
        ensure!(
            max_preview_chunks > 0,
            "max_live_preview_chunks must be non-zero"
        );
        let max_event_bytes = usize::try_from(max_event_bytes)
            .context("max_runner_response_bytes does not fit usize")?;
        ensure!(max_event_bytes > 0, "max_event_bytes must be non-zero");
        let max_video_chunk_bytes = usize::try_from(max_video_chunk_bytes)
            .context("max_live_video_chunk_bytes does not fit usize")?;
        ensure!(
            max_video_chunk_bytes > 0 && max_video_chunk_bytes < max_event_bytes,
            "max_live_video_chunk_bytes must be non-zero and smaller than max_live_event_bytes"
        );
        Ok(Self {
            catalog,
            runner,
            startup_timeout,
            max_result_frames,
            max_preview_chunks,
            max_detections_per_frame,
            max_event_bytes,
            max_video_chunk_bytes,
            subscribers,
            sessions: Mutex::new(BTreeMap::new()),
            active_pipelines: Mutex::new(BTreeSet::new()),
        })
    }

    pub(super) async fn start(
        self: &Arc<Self>,
        pipeline_id: &str,
        owner: TaskOwner,
    ) -> Result<StartLiveSessionOutput> {
        let pipeline = self
            .catalog
            .pipeline(pipeline_id)
            .with_context(|| format!("unknown Stream pipeline `{pipeline_id}`"))?;
        let live = pipeline
            .live
            .as_ref()
            .with_context(|| format!("pipeline `{pipeline_id}` does not admit live input"))?;
        let model = pipeline
            .profile
            .model_id()
            .map(|model_id| {
                self.catalog
                    .model(model_id)
                    .context("pipeline model disappeared from the admitted catalog")
            })
            .transpose()?;

        {
            let mut active = self.active_pipelines.lock().await;
            ensure!(
                active.insert(pipeline_id.to_owned()),
                "pipeline `{pipeline_id}` already has an active live session"
            );
        }

        let result = self
            .start_reserved(pipeline_id, live, &pipeline.profile, model, owner)
            .await;
        if result.is_err() {
            self.active_pipelines.lock().await.remove(pipeline_id);
        }
        result
    }

    async fn start_reserved(
        self: &Arc<Self>,
        pipeline_id: &str,
        live: &veoveo_stream_mcp::catalog::LivePipelineConfig,
        profile: &PipelineProfileConfig,
        model: Option<&ModelConfig>,
        owner: TaskOwner,
    ) -> Result<StartLiveSessionOutput> {
        let session_id = uuid::Uuid::now_v7().to_string();
        let work = tempfile::Builder::new()
            .prefix("veoveo-stream-live-")
            .tempdir()
            .context("creating live Stream runner workspace")?;
        let request_path = work.path().join("request.json");
        let event_socket = work.path().join("events.sock");
        let stderr_path = work.path().join("runner.stderr");
        let listener = UnixListener::bind(&event_socket)
            .context("binding the live Stream runner event socket")?;
        let request = LiveRunnerRequest {
            schema: LIVE_RUNNER_REQUEST_SCHEMA,
            session_id: session_id.clone(),
            input_width: live.input_width,
            input_height: live.input_height,
            pipeline: RunnerPipeline {
                pipeline_id: pipeline_id.to_owned(),
                graph: live.graph.clone(),
                profile: RunnerPipelineProfile::from(profile),
            },
            model: model.map(|model| RunnerModel {
                model_id: model.id.clone(),
                model_path: model.model_path.clone(),
                format: model.format,
            }),
            max_detections_per_frame: self.max_detections_per_frame,
            max_event_bytes: self.max_event_bytes,
            max_video_chunk_bytes: self.max_video_chunk_bytes,
        };
        tokio::fs::write(&request_path, serde_json::to_vec_pretty(&request)?)
            .await
            .context("writing live Stream runner request")?;
        let stderr = std::fs::File::create(&stderr_path)
            .context("creating live Stream runner diagnostics")?;
        let mut command = Command::new(&self.runner);
        command
            .arg("--request-json")
            .arg(&request_path)
            .arg("--event-socket")
            .arg(&event_socket)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(stderr))
            .kill_on_drop(true);
        let mut child = command
            .spawn()
            .with_context(|| format!("starting Stream runner {}", self.runner.display()))?;
        let stream = match tokio::time::timeout(self.startup_timeout, listener.accept()).await {
            Ok(Ok((stream, _))) => stream,
            Ok(Err(error)) => {
                let _ = child.kill().await;
                return Err(error).context("accepting the live Stream runner event channel");
            }
            Err(_) => {
                let status = child.try_wait().context("checking live Stream runner")?;
                let _ = child.kill().await;
                let diagnostics = tokio::fs::read_to_string(&stderr_path)
                    .await
                    .unwrap_or_default();
                bail!(
                    "live Stream runner did not connect within {:?}; status={status:?}: {}",
                    self.startup_timeout,
                    diagnostics.trim()
                );
            }
        };

        let started_at = Utc::now();
        let recording_output = live
            .recording_output
            .clone()
            .map(|config| LiveRecordingOutput::start(session_id.clone(), config));
        let session = Arc::new(LiveSession {
            session_id: session_id.clone(),
            pipeline_id: pipeline_id.to_owned(),
            pipeline_uri: uris::pipeline_uri(pipeline_id),
            ingress: live.ingress.view(),
            video: live.video_view(),
            input_width: live.input_width,
            input_height: live.input_height,
            owner,
            recording_output,
            _work: work,
            state: Mutex::new(LiveSessionState {
                lifecycle: LiveSessionLifecycle::Running,
                started_at,
                stopped_at: None,
                processed_frames: 0,
                dropped_result_frames: 0,
                newest_result_at: None,
                error: None,
                frames: VecDeque::new(),
                video_chunks: VecDeque::new(),
                dropped_video_chunks: 0,
                received_video_frames: 0,
                last_video_sequence: None,
                child: Some(child),
            }),
        });
        self.sessions
            .lock()
            .await
            .insert(session_id.clone(), session.clone());
        tokio::spawn(
            self.clone()
                .consume_events(session.clone(), stream, stderr_path),
        );

        Ok(StartLiveSessionOutput {
            session_id: session_id.clone(),
            session_uri: uris::session_uri(&session_id),
            results_uri: uris::session_results_uri(&session_id),
            pipeline_uri: uris::pipeline_uri(pipeline_id),
            ingress: live.ingress.view(),
            video: live.video_view(),
            preview_uri: uris::session_preview_uri(&session_id),
            recording_output: session
                .recording_output
                .as_ref()
                .map(LiveRecordingOutput::view),
            started_at: started_at.to_rfc3339(),
        })
    }

    async fn consume_events(
        self: Arc<Self>,
        session: Arc<LiveSession>,
        stream: tokio::net::UnixStream,
        stderr_path: PathBuf,
    ) {
        let mut reader = BufReader::new(stream);
        let mut line = Vec::new();
        let failure = loop {
            line.clear();
            match reader.read_until(b'\n', &mut line).await {
                Ok(0) => break None,
                Ok(_) if line.len() > self.max_event_bytes => {
                    break Some("live Stream runner event exceeds max_event_bytes".to_owned());
                }
                Ok(_) => {
                    let event = match serde_json::from_slice::<LiveRunnerEvent>(&line) {
                        Ok(event) => event,
                        Err(error) => {
                            break Some(format!(
                                "live Stream runner emitted invalid typed JSON: {error}"
                            ));
                        }
                    };
                    match event {
                        LiveRunnerEvent::ResultFrame { frame } => {
                            if let Err(error) = validate_frame(
                                &frame,
                                session.input_width,
                                session.input_height,
                                self.max_detections_per_frame,
                            ) {
                                break Some(format!(
                                    "live Stream runner event was invalid: {error}"
                                ));
                            }
                            self.record_frame(&session, frame).await;
                        }
                        LiveRunnerEvent::VideoChunk { chunk } => {
                            if let Err(error) = self.record_video_chunk(&session, chunk).await {
                                break Some(format!(
                                    "live Stream runner video event was invalid: {error}"
                                ));
                            }
                        }
                    }
                }
                Err(error) => {
                    break Some(format!("live Stream runner event channel failed: {error}"));
                }
            }
        };
        self.finish_session(&session, failure, &stderr_path).await;
    }

    async fn record_frame(&self, session: &LiveSession, frame: FrameDetections) {
        let observed_at = Utc::now();
        {
            let mut state = session.state.lock().await;
            if state.lifecycle != LiveSessionLifecycle::Running {
                return;
            }
            state.processed_frames += 1;
            state.newest_result_at = Some(observed_at);
            if state.frames.len() == self.max_result_frames {
                state.frames.pop_front();
                state.dropped_result_frames += 1;
            }
            state.frames.push_back(LiveResultFrame {
                index: frame.index,
                observed_at: observed_at.to_rfc3339(),
                detections: frame.detections,
            });
        }
        self.subscribers
            .notify_resource_updated(uris::session_uri(&session.session_id))
            .await;
        self.subscribers
            .notify_resource_updated(uris::session_results_uri(&session.session_id))
            .await;
    }

    async fn record_video_chunk(
        &self,
        session: &LiveSession,
        chunk: EncodedVideoChunk,
    ) -> Result<()> {
        let timestamp_us = chunk.timestamp_us;
        let keyframe = chunk.keyframe;
        let bytes = BASE64_STANDARD
            .decode(&chunk.data_base64)
            .context("H.264 access unit is not valid base64")?;
        ensure!(
            !bytes.is_empty() && bytes.len() <= self.max_video_chunk_bytes,
            "H.264 access unit exceeds the admitted byte bound"
        );
        ensure!(
            bytes.starts_with(&[0, 0, 0, 1]) || bytes.starts_with(&[0, 0, 1]),
            "H.264 access unit is not Annex B"
        );
        {
            let mut state = session.state.lock().await;
            if state.lifecycle != LiveSessionLifecycle::Running {
                return Ok(());
            }
            let expected = state
                .last_video_sequence
                .map_or(0, |sequence| sequence.saturating_add(1));
            ensure!(
                chunk.sequence == expected,
                "H.264 sequence is not contiguous"
            );
            state.last_video_sequence = Some(chunk.sequence);
            state.received_video_frames += 1;
            if state.video_chunks.len() == self.max_preview_chunks {
                state.video_chunks.pop_front();
                state.dropped_video_chunks += 1;
            }
            state.video_chunks.push_back(chunk);
        }
        if let Some(recording) = &session.recording_output {
            recording.try_record(timestamp_us, keyframe, bytes);
        }
        self.subscribers
            .notify_resource_updated(uris::session_uri(&session.session_id))
            .await;
        self.subscribers
            .notify_resource_updated(uris::session_preview_uri(&session.session_id))
            .await;
        Ok(())
    }

    async fn finish_session(
        &self,
        session: &LiveSession,
        failure: Option<String>,
        stderr_path: &PathBuf,
    ) {
        if let Some(recording) = &session.recording_output {
            recording.stop();
        }
        let mut state = session.state.lock().await;
        let child = state.child.take();
        drop(state);
        let exit = if let Some(child) = child {
            // The event loop owns the runner only after its private event stream has
            // failed or ended unexpectedly. A normal stop takes the child first.
            reap_runner(child, true).await
        } else {
            None
        };
        let diagnostics = tokio::fs::read_to_string(stderr_path)
            .await
            .unwrap_or_default();
        let mut state = session.state.lock().await;
        if state.lifecycle != LiveSessionLifecycle::Stopped {
            state.lifecycle = LiveSessionLifecycle::Failed;
            state.stopped_at = Some(Utc::now());
            state.error = Some(failure.unwrap_or_else(|| {
                format!(
                    "live Stream runner exited unexpectedly with {exit:?}: {}",
                    diagnostics.trim()
                )
            }));
        }
        drop(state);
        self.active_pipelines
            .lock()
            .await
            .remove(&session.pipeline_id);
        self.subscribers
            .notify_resource_updated(uris::session_uri(&session.session_id))
            .await;
    }

    pub(super) async fn stop(
        &self,
        session_id: &str,
        caller: &TaskOwner,
    ) -> Result<Option<StopLiveSessionOutput>> {
        let Some(session) = self.owned(session_id, caller).await else {
            return Ok(None);
        };
        let mut state = session.state.lock().await;
        if state.lifecycle == LiveSessionLifecycle::Stopped {
            return Ok(Some(stop_output(&session, &state)));
        }
        state.lifecycle = LiveSessionLifecycle::Stopped;
        state.stopped_at = Some(Utc::now());
        let child = state.child.take();
        drop(state);
        if let Some(mut child) = child {
            child.kill().await.context("stopping live Stream runner")?;
            let _ = child.wait().await;
        }
        if let Some(recording) = &session.recording_output {
            recording.stop();
        }
        self.active_pipelines
            .lock()
            .await
            .remove(&session.pipeline_id);
        let state = session.state.lock().await;
        let output = stop_output(&session, &state);
        drop(state);
        self.subscribers
            .notify_resource_updated(uris::session_uri(session_id))
            .await;
        Ok(Some(output))
    }

    pub(super) async fn visible(&self, caller: &TaskOwner) -> Vec<LiveSessionView> {
        let sessions = self
            .sessions
            .lock()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let mut views = Vec::new();
        for session in sessions {
            if reader_allows(&session.owner, caller) {
                let state = session.state.lock().await;
                views.push(session_view(&session, &state));
            }
        }
        views.sort_by(|left, right| left.started_at.cmp(&right.started_at));
        views
    }

    pub(super) async fn view(
        &self,
        session_id: &str,
        caller: &TaskOwner,
    ) -> Option<LiveSessionView> {
        let session = self.readable(session_id, caller).await?;
        let state = session.state.lock().await;
        Some(session_view(&session, &state))
    }

    pub(super) async fn results(
        &self,
        session_id: &str,
        caller: &TaskOwner,
    ) -> Option<LiveResultsView> {
        let session = self.readable(session_id, caller).await?;
        let state = session.state.lock().await;
        Some(LiveResultsView {
            schema: LIVE_RESULTS_SCHEMA.to_owned(),
            session_id: session.session_id.clone(),
            pipeline_id: session.pipeline_id.clone(),
            frames: state.frames.iter().cloned().collect(),
            processed_frames: state.processed_frames,
            dropped_result_frames: state.dropped_result_frames,
        })
    }

    pub(super) async fn preview(
        &self,
        session_id: &str,
        caller: &TaskOwner,
    ) -> Option<LivePreviewView> {
        let session = self.readable(session_id, caller).await?;
        let state = session.state.lock().await;
        let first_keyframe = state
            .video_chunks
            .iter()
            .rposition(|chunk| chunk.keyframe)
            .unwrap_or(state.video_chunks.len());
        Some(LivePreviewView {
            schema: LIVE_PREVIEW_SCHEMA.to_owned(),
            session_id: session.session_id.clone(),
            video: session.video.clone(),
            chunks: state
                .video_chunks
                .iter()
                .skip(first_keyframe)
                .cloned()
                .collect(),
            dropped_chunks: state.dropped_video_chunks,
            received_video_frames: state.received_video_frames,
        })
    }

    pub(super) async fn readable_by(&self, session_id: &str, caller: &TaskOwner) -> bool {
        self.readable(session_id, caller).await.is_some()
    }

    async fn readable(&self, session_id: &str, caller: &TaskOwner) -> Option<Arc<LiveSession>> {
        let session = self.sessions.lock().await.get(session_id).cloned()?;
        reader_allows(&session.owner, caller).then_some(session)
    }

    async fn owned(&self, session_id: &str, caller: &TaskOwner) -> Option<Arc<LiveSession>> {
        let session = self.sessions.lock().await.get(session_id).cloned()?;
        owner_allows(&session.owner, caller).then_some(session)
    }
}

async fn reap_runner(mut child: Child, terminate: bool) -> Option<ExitStatus> {
    if terminate {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) | Err(_) => {
                let _ = child.kill().await;
            }
        }
    }
    child.wait().await.ok()
}

fn reader_allows(owner: &TaskOwner, caller: &TaskOwner) -> bool {
    owner.tenant_key() == caller.tenant_key()
        && owner.authority.tenant == caller.authority.tenant
        && owner.authority.work_context == caller.authority.work_context
        && caller
            .authority
            .membership
            .allows(WorkContextMembershipLevel::Viewer)
        && owner.data_labels.is_subset(&caller.data_labels)
}

fn owner_allows(owner: &TaskOwner, caller: &TaskOwner) -> bool {
    owner.allows(
        &caller.principal_key,
        &caller.profile,
        caller.tenant_key.as_deref(),
        &caller.data_labels,
    )
}

fn session_view(session: &LiveSession, state: &LiveSessionState) -> LiveSessionView {
    LiveSessionView {
        session_id: session.session_id.clone(),
        session_uri: uris::session_uri(&session.session_id),
        results_uri: uris::session_results_uri(&session.session_id),
        pipeline_id: session.pipeline_id.clone(),
        pipeline_uri: session.pipeline_uri.clone(),
        ingress: session.ingress.clone(),
        video: session.video.clone(),
        preview_uri: uris::session_preview_uri(&session.session_id),
        recording_output: session
            .recording_output
            .as_ref()
            .map(LiveRecordingOutput::view),
        lifecycle: state.lifecycle,
        started_at: state.started_at.to_rfc3339(),
        stopped_at: state.stopped_at.map(|value| value.to_rfc3339()),
        processed_frames: state.processed_frames,
        received_video_frames: state.received_video_frames,
        newest_result_at: state.newest_result_at.map(|value| value.to_rfc3339()),
        error: state.error.clone(),
    }
}

fn stop_output(session: &LiveSession, state: &LiveSessionState) -> StopLiveSessionOutput {
    StopLiveSessionOutput {
        session_uri: uris::session_uri(&session.session_id),
        lifecycle: state.lifecycle,
        received_video_frames: state.received_video_frames,
        processed_frames: state.processed_frames,
        recording_output: session
            .recording_output
            .as_ref()
            .map(LiveRecordingOutput::view),
        stopped_at: state.stopped_at.unwrap_or_else(Utc::now).to_rfc3339(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_mcp_contract::{
        AccessSubject, InvocationAuthority, InvocationProvenance, PolicyVersion, PrincipalId,
        TenantId, WorkContextId, WorkContextOutputPolicy,
    };
    use veoveo_task_runtime::PrincipalKind;

    fn task_owner(
        principal: &str,
        work_context: &str,
        membership: WorkContextMembershipLevel,
        data_labels: &[&str],
    ) -> TaskOwner {
        let principal_id = PrincipalId::new(principal).unwrap();
        TaskOwner {
            principal_key: principal.to_owned(),
            principal_kind: PrincipalKind::Service,
            issuer: "https://issuer.example".to_owned(),
            subject: principal.to_owned(),
            profile: "operator".to_owned(),
            tenant_key: Some("tenant".to_owned()),
            data_labels: data_labels
                .iter()
                .map(|label| (*label).to_owned())
                .collect(),
            authority: InvocationAuthority {
                work_context: WorkContextId::new(work_context).unwrap(),
                tenant: TenantId::new("tenant").unwrap(),
                membership,
                policy_revision: PolicyVersion::new("r1").unwrap(),
                output_policy: WorkContextOutputPolicy {
                    owner: AccessSubject::Principal(principal_id),
                    initial_grants: Vec::new(),
                    classification: None,
                    data_labels: BTreeSet::new(),
                },
                provenance: InvocationProvenance::Automated,
            },
        }
    }

    #[test]
    fn work_context_viewer_can_read_but_not_control_an_automation_stream() {
        let automation = task_owner(
            "automation",
            "flight",
            WorkContextMembershipLevel::Contributor,
            &["operations"],
        );
        let operator = task_owner(
            "operator",
            "flight",
            WorkContextMembershipLevel::Viewer,
            &["operations", "reviewed"],
        );
        assert!(reader_allows(&automation, &operator));
        assert!(!owner_allows(&automation, &operator));
    }

    #[test]
    fn stream_reads_fail_closed_across_contexts_and_labels() {
        let automation = task_owner(
            "automation",
            "flight",
            WorkContextMembershipLevel::Contributor,
            &["operations"],
        );
        let other_context = task_owner(
            "operator",
            "maintenance",
            WorkContextMembershipLevel::Owner,
            &["operations"],
        );
        let missing_label =
            task_owner("operator", "flight", WorkContextMembershipLevel::Owner, &[]);
        assert!(!reader_allows(&automation, &other_context));
        assert!(!reader_allows(&automation, &missing_label));
    }

    #[tokio::test]
    async fn event_failure_terminates_the_native_runner_before_waiting() {
        let child = Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("spawn bounded native-runner fixture");
        let status = tokio::time::timeout(Duration::from_secs(2), reap_runner(child, true))
            .await
            .expect("failed runner must be reaped promptly")
            .expect("failed runner must return an exit status");
        assert!(!status.success());
    }
}
