//! An externally signalled process replacement while an existing replay Task is unfinished.
use super::*;
use rmcp::model::{DetailedTask, GetTaskParams, Task, TaskStatus};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex as SyncMutex},
};
use tokio::sync::Mutex;
use veoveo_stream_mcp::contract::{
    AnalysisResults, ModelId, PipelineId, RecordingSourceSnapshot, RecordingVideoSelection,
    RunRecordingOutput, RunRecordingRequest, StreamResource,
};
use veoveo_testing_support::{
    final_tasks::{WorkingCheckpoint, WorkingTaskRecovery},
    installed::restart::{CrashIdentity, CrashReceipt, CrashTarget, CrashWatch, DeploymentRestart},
    lifecycle::owner::{self, CleanupKind},
};
use veoveo_types::{CanonicalTaskId, ResourceAddress};

const OPERATION: Duration = Duration::from_secs(600);
const CLEANUP_RESERVE: Duration = Duration::from_secs(20);
#[cfg(test)]
#[path = "recovery/tests.rs"]
mod tests;
#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum Schema {
    #[vocabulary(rename = "veoveo.ai/stream-recovery/v1")]
    V1,
}
#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum Mode {
    Recover,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Input {
    schema: Schema,
    mode: Mode,
    pub caller:
        veoveo_testing_support::final_tasks::public_caller::PublicCallerInput<public::Schema>,
    pub fixture: Fixture,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Fixture {
    pub target: CrashTarget,
    replacement_timeout_seconds: u64,
    request: RunRecordingRequest,
    expected: Expected,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Expected {
    video: RecordingVideoSelection,
    pipeline_id: PipelineId,
    model_id: ModelId,
    source_snapshot: RecordingSourceSnapshot,
    processed_frames: u64,
    minimum_detections: u64,
}
impl Input {
    pub(super) fn admit(&self, installation: &InstalledTarget) -> Result<()> {
        ensure!(
            self.schema == Schema::V1 && self.mode == Mode::Recover,
            "unsupported Stream recovery profile"
        );
        self.fixture.admit(installation)
    }
}
impl Fixture {
    pub(super) fn admit(&self, installation: &InstalledTarget) -> Result<()> {
        self.admit_target(&installation.target)
    }
    fn admit_target(&self, target: &veoveo_deploy_contract::InstallationTarget) -> Result<()> {
        self.target.validate()?;
        ensure!(
            self.target.container == "stream-mcp"
                && target
                    .expected_deployments
                    .contains(&self.target.deployment),
            "Stream recovery requires the selected installed Rust server"
        );
        ensure!(
            (1..=300).contains(&self.replacement_timeout_seconds),
            "Stream replacement wait must be 1..300 seconds"
        );
        ensure!(
            self.request.pipeline_id == self.expected.pipeline_id
                && same(&self.request.video, &self.expected.video)?
                && self.expected.source_snapshot.recording_id
                    == self.request.video.recording_uri.id()
                && self.request.include_source_clip
                && (1..=1_000_000).contains(&self.expected.processed_frames)
                && self.expected.minimum_detections > 0,
            "Stream recovery request and independent expectations disagree"
        );
        Ok(())
    }
    fn product(&self, output: &RunRecordingOutput, results: &AnalysisResults) -> Result<()> {
        results.validate()?;
        let expected = &self.expected;
        ensure!(
            output.pipeline_uri.id() == &expected.pipeline_id
                && output.model_uri.id() == &expected.model_id
                && results.pipeline_id == expected.pipeline_id
                && results.model_id == expected.model_id
                && results.recording_uri == expected.video.recording_uri
                && results.entity_path == expected.video.entity_path
                && results.timeline == expected.video.timeline
                && results.requested_range == expected.video.range
                && results.source_snapshot.recording_id == expected.source_snapshot.recording_id
                && results.source_snapshot.dataset_id == expected.source_snapshot.dataset_id
                && results.source_snapshot.sources == expected.source_snapshot.sources
                && results.processed_frames == expected.processed_frames
                && output.summary.processed_frames == expected.processed_frames
                && output.summary.requested_start_index == expected.video.range.start
                && output.summary.requested_end_index == expected.video.range.end
                && output.summary.detection_count >= expected.minimum_detections
                && output.source_clip_artifact.is_some(),
            "Stream recovered product differs from independent fixture"
        );
        // Capture time is chosen by the run, while ordered immutable source facts
        // are independently supplied. Bind that actual snapshot to product provenance.
        let metadata: veoveo_stream_mcp::contract::StreamArtifactMetadata =
            serde_json::from_value(output.results_artifact.metadata.clone())?;
        let veoveo_stream_mcp::contract::StreamArtifactProvenance::Results {
            run_id,
            recording_id,
            pipeline_id,
            model_id,
            source_snapshot_sha256,
        } = metadata.provenance
        else {
            bail!("Stream recovered Artifact has wrong provenance kind");
        };
        ensure!(
            run_id == output.run_id()
                && recording_id == expected.video.recording_uri.id()
                && pipeline_id == expected.pipeline_id
                && model_id == expected.model_id
                && source_snapshot_sha256 == results.source_snapshot.digest_sha256()?,
            "Stream recovered source provenance differs from its result"
        );
        ensure!(
            results
                .frames
                .iter()
                .map(|frame| frame.detections.len() as u64)
                .sum::<u64>()
                == output.summary.detection_count,
            "Stream recovered detection summary disagrees with results"
        );
        Ok(())
    }
}
fn same(a: &impl Serialize, b: &impl Serialize) -> Result<bool> {
    Ok(serde_json::to_value(a)? == serde_json::to_value(b)?)
}
fn safe<T>(value: Result<T>) -> Result<T> {
    value.map_err(|error| {
        anyhow::anyhow!(
            "Stream recovery operation failed (diagnostic sha256:{})",
            hex::encode(Sha256::digest(error.to_string().as_bytes()))
        )
    })
}
#[derive(Clone, Copy, Default, PartialEq, Eq, veoveo_types::Vocabulary)]
enum Step {
    #[default]
    Prepared,
    DispatchIntent,
    Acknowledged,
    Working,
    WatchArmed,
    ReadyForCrash,
    ReplacementObserved,
    ReplacementWorking,
    Completed,
    ResourceSnapshot,
    Cleanup,
    Failed,
}
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Facts {
    step: Step,
    task_id: Option<CanonicalTaskId>,
    created_at: Option<chrono::DateTime<Utc>>,
    created_task: Option<Task>,
    replacement_working: Option<DetailedTask>,
    output: Option<RunRecordingOutput>,
    statuses: Vec<TaskStatus>,
    resource: Option<veoveo_types::ResourceUri>,
    crash_identity: Option<CrashIdentity>,
    crash_receipt: Option<CrashReceipt>,
    watch_closed: bool,
    preflight_client_closed: bool,
    cleanup_failed: bool,
    failed: bool,
    domain_checks_complete: bool,
    failure_sha256: Option<String>,
}
struct Evidence {
    journal: Arc<veoveo_testing_support::final_tasks::public_caller::PrivateCallerJournal>,
    facts: SyncMutex<Facts>,
}
impl Evidence {
    fn update(&self, change: impl FnOnce(&mut Facts)) -> Result<()> {
        let mut facts = self
            .facts
            .lock()
            .map_err(|_| anyhow::anyhow!("Stream recovery facts lock failed"))?;
        change(&mut facts);
        self.journal.append(&*facts)
    }
    fn created(&self, id: &CanonicalTaskId, task: &Task) -> Result<()> {
        // Identity is retained synchronously before journal persistence or another await.
        let created_at = task.created_at.parse::<chrono::DateTime<Utc>>();
        self.update(|facts| {
            facts.step = Step::Acknowledged;
            facts.task_id = Some(id.clone());
            facts.created_at = created_at.as_ref().ok().copied();
            facts.created_task = Some(task.clone());
        })?;
        created_at.context("Stream acknowledged Task has invalid creation time")?;
        Ok(())
    }
}
type WatchClose = Pin<Box<dyn Future<Output = bool> + Send>>;
type ClientClose = Pin<Box<dyn Future<Output = Result<()>> + Send>>;
#[derive(Default)]
struct Handles {
    watch: Option<CrashWatch>,
    client: Option<SmokeMcpClient>,
    watch_close: Option<WatchClose>,
    client_close: Option<ClientClose>,
    end: Option<tokio::time::Instant>,
    total_end: Option<tokio::time::Instant>,
    failed: bool,
}
impl Handles {
    fn capture(&self, evidence: &Evidence) -> Result<()> {
        if let Some(watch) = &self.watch {
            let identity = watch.identity().ok();
            let receipt = watch.snapshot().map(|mut receipt| {
                receipt.termination_reason = None;
                receipt
            });
            evidence.update(|facts| {
                facts.crash_identity = identity;
                facts.crash_receipt = receipt;
            })?;
        }
        Ok(())
    }
    async fn close(&mut self, evidence: &Evidence) -> Result<()> {
        let admitted = tokio::time::Instant::from_std(owner::cleanup_deadline()?);
        self.close_until(evidence, admitted).await
    }
    async fn close_until(
        &mut self,
        evidence: &Evidence,
        admitted: tokio::time::Instant,
    ) -> Result<()> {
        let cap = self.total_end.map_or(admitted, |total| total.min(admitted));
        let end = *self.end.get_or_insert(cap);
        let persistence = self.capture(evidence);
        if self.watch_close.is_none()
            && let Some(watch) = self.watch.take()
        {
            self.watch_close = Some(Box::pin(watch.close()));
        }
        if self.client_close.is_none()
            && let Some(client) = self.client.take()
        {
            self.client_close = Some(Box::pin(async move { safe(client.cancel().await) }));
        }
        let watch_ok = if let Some(close) = self.watch_close.as_mut() {
            if tokio::time::Instant::now() >= end {
                false
            } else {
                match tokio::time::timeout_at(end, close).await {
                    Ok(success) => {
                        // A consuming close future must never be polled after Ready,
                        // including a completed failure. Keep failure in `self.failed`.
                        self.watch_close = None;
                        success
                    }
                    Err(_) => false,
                }
            }
        } else {
            true
        };
        if watch_ok {
            self.watch_close = None;
        }
        // Retain a completed watch failure before awaiting the remaining close.
        self.failed |= !watch_ok;
        let client_ok = if let Some(close) = self.client_close.as_mut() {
            if tokio::time::Instant::now() >= end {
                false
            } else {
                match tokio::time::timeout_at(end, close).await {
                    Ok(result) => {
                        self.client_close = None;
                        result.is_ok()
                    }
                    Err(_) => false,
                }
            }
        } else {
            true
        };
        if client_ok {
            self.client_close = None;
        }
        self.failed |= !watch_ok || !client_ok || persistence.is_err();
        if let Err(error) = evidence.update(|facts| {
            facts.failed |= !facts.domain_checks_complete;
            facts.step = Step::Cleanup;
            facts.watch_closed = watch_ok && !self.failed;
            facts.preflight_client_closed = client_ok && !self.failed;
            facts.cleanup_failed |= self.failed;
        }) {
            // Empty close slots prove completion, not successful persistence.
            // Retain this failure before a registered cleanup can re-enter.
            self.failed = true;
            if let Ok(mut facts) = evidence.facts.lock() {
                facts.cleanup_failed = true;
                facts.failed = true;
            }
            return Err(error);
        }
        ensure!(
            !self.failed,
            "Stream recovery native/client cleanup unresolved"
        );
        persistence
    }
}
struct Recovery<'a> {
    installation: &'a InstalledTarget,
    fixture: &'a Fixture,
    handles: Arc<Mutex<Handles>>,
    evidence: Arc<Evidence>,
}
impl WorkingTaskRecovery for Recovery<'_> {
    fn replacement_working(
        &mut self,
        id: &CanonicalTaskId,
        created: &Task,
        current: &DetailedTask,
    ) -> Result<()> {
        working(created, current, id)?;
        self.evidence.update(|facts| {
            facts.step = Step::ReplacementWorking;
            facts.replacement_working = Some(current.clone());
        })
    }
    fn checkpoint<'a>(
        &'a mut self,
        context: WorkingCheckpoint<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            working(
                context.created_task(),
                context.current_working(),
                context.task_id(),
            )?;
            self.evidence.update(|facts| facts.step = Step::Working)?;
            let driver = driver(self.installation, self.fixture, context.client())?;
            let mut handles = self.handles.lock().await;
            // The watch slot belongs to registered cleanup before its initial admission awaits.
            driver
                .arm_crash_watch(&self.fixture.target, &mut handles.watch)
                .await?;
            handles.capture(&self.evidence)?;
            self.evidence
                .update(|facts| facts.step = Step::WatchArmed)?;
            driver.admit_crash_target(&self.fixture.target).await?;
            let current = context
                .client()
                .get_task(GetTaskParams::new(context.task_id().as_str()))
                .await?
                .task;
            working(context.created_task(), &current, context.task_id())?;
            // Ops may signal only after this persisted Task/watch/live-target handshake.
            self.evidence
                .update(|facts| facts.step = Step::ReadyForCrash)?;
            let end = context.deadline().min(
                tokio::time::Instant::now()
                    + Duration::from_secs(self.fixture.replacement_timeout_seconds),
            );
            let watch = handles
                .watch
                .as_mut()
                .context("Stream crash watch not retained")?;
            watch.wait(end).await?;
            watch.admit_recovered_target().await?;
            handles.capture(&self.evidence)?;
            self.evidence
                .update(|facts| facts.step = Step::ReplacementObserved)?;
            // Shared delivery now reconnects once and requires same-ID/current Working.
            Ok(())
        })
    }
}
fn driver(
    installation: &InstalledTarget,
    fixture: &Fixture,
    client: &SmokeMcpClient,
) -> Result<DeploymentRestart> {
    DeploymentRestart::new(
        &installation.target,
        &fixture.target.deployment,
        "stream-mcp",
        client.peer().clone(),
        StreamResource::Contract.to_uri()?,
    )
}
fn working(created: &Task, current: &DetailedTask, id: &CanonicalTaskId) -> Result<()> {
    ensure!(
        created.task_id == id.as_str()
            && current.task.task_id == id.as_str()
            && current.task.created_at == created.created_at
            && current.status() == TaskStatus::Working,
        "Stream Task must retain original identity and remain unfinished"
    );
    Ok(())
}
pub(super) async fn run(
    installation: &InstalledTarget,
    profile: &mut public::Profile,
    fixture: Fixture,
) -> Result<()> {
    let total_end = tokio::time::Instant::now() + OPERATION;
    let end = total_end - CLEANUP_RESERVE;
    fixture.admit(installation)?;
    let evidence = Arc::new(Evidence {
        journal: profile.journal.clone(),
        facts: SyncMutex::new(Facts::default()),
    });
    // Retain the selected immutable request/expectations before network or Task effects.
    evidence.journal.append(&fixture)?;
    let handles = Arc::new(Mutex::new(Handles {
        total_end: Some(total_end),
        ..Handles::default()
    }));
    let owned = handles.clone();
    let cleanup_evidence = evidence.clone();
    let registration = owner::register_cleanup(
        CleanupKind::Remote,
        "Stream recovery watch/preflight client",
        &uuid::Uuid::now_v7().to_string(),
        move || async move { owned.lock().await.close(&cleanup_evidence).await },
    )?;
    let result = tokio::time::timeout_at(end, async {
        let mut retained = handles.lock().await;
        retained.client = Some(safe(profile.client.connect().await)?);
        driver(installation, &fixture, retained.client.as_ref().unwrap())?
            .admit_crash_target(&fixture.target)
            .await?;
        drop(retained);
        evidence.update(|facts| facts.step = Step::DispatchIntent)?;
        let mut recovery = Recovery {
            installation,
            fixture: &fixture,
            handles: handles.clone(),
            evidence: evidence.clone(),
        };
        let delivered = profile
            .client
            .run_tool_delivered_recovered(
                "stream__run_recording",
                serde_json::to_value(&fixture.request)?,
                end.saturating_duration_since(tokio::time::Instant::now()),
                |id, task| evidence.created(id, task),
                &mut recovery,
            )
            .await?;
        working(
            &delivered.created_task,
            &delivered.replacement_working,
            &delivered.task_id,
        )?;
        evidence.update(|facts| facts.step = Step::ReplacementWorking)?;
        let output: RunRecordingOutput = serde_json::from_value(
            delivered
                .result
                .structured_content
                .context("Stream recovered Task lacks typed output")?,
        )?;
        evidence.update(|facts| {
            facts.step = Step::Completed;
            facts.output = Some(output.clone());
            facts.statuses = delivered.statuses.clone();
        })?;
        let uri = public::resource_identity(&output)?;
        let results: AnalysisResults = profile.client.read_resource(&uri).await?;
        fixture.product(&output, &results)?;
        let snapshot = profile
            .client
            .resource_snapshot_delivery(
                &uri,
                end.saturating_duration_since(tokio::time::Instant::now()),
            )
            .await?;
        ensure!(
            snapshot.uri == uri,
            "Stream recovery resource snapshot identity differs"
        );
        evidence.update(|facts| {
            facts.step = Step::ResourceSnapshot;
            facts.resource = Some(uri.clone());
        })?;
        handles
            .lock()
            .await
            .watch
            .as_mut()
            .context("Stream recovered watch absent")?
            .admit_recovered_target()
            .await?;
        evidence.update(|facts| facts.domain_checks_complete = true)?;
        Ok::<_, anyhow::Error>(())
    })
    .await
    .map_err(|_| anyhow::anyhow!("Stream original recovery operation deadline"))
    .and_then(|value| value);
    let closed = handles.lock().await.close(&evidence).await;
    if closed.is_ok() {
        registration.settled()?;
    }
    if let Err(error) = &result {
        evidence.update(|facts| {
            facts.step = Step::Failed;
            facts.failed = true;
            facts.failure_sha256 = Some(hex::encode(Sha256::digest(error.to_string().as_bytes())));
        })?;
    }
    safe(result)?;
    safe(closed)?;
    profile.complete()?;
    Ok(())
}
