//! One retained Frames fixture across one externally authorized process crash.
use super::*;
use crate::case_5::{
    TaskNotificationObservation, TaskNotificationState, complete_tool_with_notification,
};
use anyhow::ensure;
use chrono::{DateTime, Utc};
use rmcp::model::{CallToolResult, DetailedTask, GetTaskParams, TaskPayload, TaskStatus};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::{MetadataExt, PermissionsExt},
};
use veoveo_frames_mcp::contract::*;
use veoveo_testing_support::{
    SmokeMcpClient, connect_mcp_client,
    installed::restart::{CrashIdentity, CrashReceipt, CrashTarget, CrashWatch, DeploymentRestart},
};
use veoveo_types::{CanonicalTaskId, ResourceAddress, Sha256Digest};

const OPERATION_BUDGET: Duration = Duration::from_secs(300);
const RECEIPT_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, Eq, PartialEq, veoveo_types::Vocabulary)]
enum FixtureSchema {
    #[vocabulary(rename = "veoveo.ai/frames-crash-fixture/v1")]
    V1,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CrashFixture {
    schema: FixtureSchema,
    context: String,
    namespace: String,
    control_plane_sha256: Sha256Digest,
    target: CrashTarget,
}
impl CrashFixture {
    fn admit(&self, installation: &support::InstalledTarget, control_bytes: &[u8]) -> Result<()> {
        self.target.validate()?;
        ensure!(
            self.schema == FixtureSchema::V1
                && self.context == installation.target.kubernetes.context
                && self.namespace == installation.target.kubernetes.namespace,
            "Frames crash fixture belongs to another installation"
        );
        ensure!(
            self.control_plane_sha256
                == Sha256Digest::from_bytes(Sha256::digest(control_bytes).into()),
            "Frames crash fixture control-plane digest differs"
        );
        ensure!(
            self.target.deployment == "frames-mcp"
                && self.target.container == "frames-mcp"
                && installation
                    .target
                    .expected_deployments
                    .contains(&self.target.deployment),
            "Frames crash fixture must select its declared server Deployment/container"
        );
        Ok(())
    }
}

#[derive(Clone, Copy, Eq, PartialEq, veoveo_types::Vocabulary)]
enum Stage {
    #[vocabulary(rename = "admitted")]
    Admitted,
    #[vocabulary(rename = "live_target_admitted")]
    LiveTargetAdmitted,
    #[vocabulary(rename = "create")]
    Create,
    #[vocabulary(rename = "publish")]
    Publish,
    #[vocabulary(rename = "convert")]
    Convert,
    #[vocabulary(rename = "batch")]
    Batch,
    #[vocabulary(rename = "before_verified")]
    BeforeVerified,
    #[vocabulary(rename = "watch_armed")]
    WatchArmed,
    #[vocabulary(rename = "ready_for_crash")]
    ReadyForCrash,
    #[vocabulary(rename = "crash_observed")]
    CrashObserved,
    #[vocabulary(rename = "after_verified")]
    AfterVerified,
}
#[derive(Clone, Copy, Eq, PartialEq, veoveo_types::Vocabulary)]
enum Outcome {
    #[vocabulary(rename = "pending")]
    Pending,
    #[vocabulary(rename = "passed")]
    Passed,
    #[vocabulary(rename = "failed_before_mutation")]
    FailedBeforeMutation,
    #[vocabulary(rename = "mutation_unresolved")]
    MutationUnresolved,
    #[vocabulary(rename = "crash_unresolved")]
    CrashUnresolved,
    #[vocabulary(rename = "fixture_failed_settled")]
    FailedSettled,
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Cleanup {
    task_listener: bool,
    original_client: bool,
    fresh_client: bool,
    foreign_client: bool,
    watch: bool,
}
impl Cleanup {
    fn closed(&self) -> bool {
        self.task_listener
            && self.original_client
            && self.fresh_client
            && self.foreign_client
            && self.watch
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateMutation {
    request: CreateWorldRequest,
    observed: Option<CreateWorldOutput>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PublishMutation {
    request: PublishWorldRequest,
    observed: Option<PublishWorldOutput>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConvertMutation {
    request: ConvertFrameRequest,
    observed: Option<ConvertFrameOutput>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchMutation {
    request: BatchTransformRequest,
    task_id: Option<CanonicalTaskId>,
    payload: Option<CallToolResult>,
    current: Option<DetailedTask>,
    output: Option<BatchTransformOutput>,
}
#[derive(Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    world: FrameWorldSummary,
    revision: FrameWorldRevision,
    direct_operation: CoordinateOperationProvenance,
    task_operation: CoordinateOperationProvenance,
    task: DetailedTask,
    payload: CallToolResult,
    batch: BatchTransformOutput,
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotProgress {
    world: Option<FrameWorldSummary>,
    revision: Option<FrameWorldRevision>,
    direct_operation: Option<CoordinateOperationProvenance>,
    task_operation: Option<CoordinateOperationProvenance>,
    task: Option<DetailedTask>,
    payload: Option<CallToolResult>,
    batch: Option<BatchTransformOutput>,
}
impl SnapshotProgress {
    fn complete(&self) -> Result<Snapshot> {
        Ok(Snapshot {
            world: self
                .world
                .clone()
                .context("Frames snapshot world missing")?,
            revision: self
                .revision
                .clone()
                .context("Frames snapshot revision missing")?,
            direct_operation: self
                .direct_operation
                .clone()
                .context("Frames snapshot direct operation missing")?,
            task_operation: self
                .task_operation
                .clone()
                .context("Frames snapshot Task operation missing")?,
            task: self.task.clone().context("Frames snapshot Task missing")?,
            payload: self
                .payload
                .clone()
                .context("Frames snapshot payload missing")?,
            batch: self
                .batch
                .clone()
                .context("Frames snapshot batch missing")?,
        })
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Receipt {
    schema: &'static str,
    started_at: DateTime<Utc>,
    fixture: Option<CrashFixture>,
    stage: Stage,
    outcome: Outcome,
    world_id: FrameWorldId,
    create: Option<CreateMutation>,
    publish: Option<PublishMutation>,
    convert: Option<ConvertMutation>,
    batch: Option<BatchMutation>,
    before: SnapshotProgress,
    after: SnapshotProgress,
    foreign: Vec<super::coverage::ForeignProbe>,
    selected: Option<CrashIdentity>,
    crash: Option<CrashReceipt>,
    cleanup: Cleanup,
    failed_stage: Option<Stage>,
}
impl Receipt {
    fn new(world_id: FrameWorldId) -> Self {
        Self {
            schema: "veoveo.ai/frames-recovery-evidence/v1",
            started_at: Utc::now(),
            fixture: None,
            stage: Stage::Admitted,
            outcome: Outcome::Pending,
            world_id,
            create: None,
            publish: None,
            convert: None,
            batch: None,
            before: SnapshotProgress::default(),
            after: SnapshotProgress::default(),
            foreign: Vec::new(),
            selected: None,
            crash: None,
            cleanup: Cleanup::default(),
            failed_stage: None,
        }
    }
    fn settled(&self) -> bool {
        self.create
            .as_ref()
            .is_some_and(|step| step.observed.is_some())
            && self
                .publish
                .as_ref()
                .is_some_and(|step| step.observed.is_some())
            && self
                .convert
                .as_ref()
                .is_some_and(|step| step.observed.is_some())
            && self
                .batch
                .as_ref()
                .is_some_and(|step| step.task_id.is_some() && step.payload.is_some())
    }
}

fn persist(file: &mut File, receipt: &Receipt) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(receipt)?;
    ensure!(
        bytes.len() <= RECEIPT_BYTES,
        "Frames recovery receipt exceeds 2 MiB"
    );
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&bytes)?;
    file.sync_data()?;
    Ok(())
}
fn mutation_stage(file: &mut File, receipt: &mut Receipt, stage: Stage) -> Result<()> {
    receipt.stage = stage;
    receipt.outcome = Outcome::MutationUnresolved;
    persist(file, receipt)
}

async fn prepare(
    client: &SmokeMcpClient,
    file: &mut File,
    receipt: &mut Receipt,
    task_state: &mut TaskNotificationState,
    deadline: tokio::time::Instant,
) -> Result<()> {
    let request = CreateWorldRequest {
        world_id: receipt.world_id.clone(),
        display_name: "Frames process recovery fixture".into(),
        description: Some("Retained completed-state crash fixture".into()),
    };
    receipt.create = Some(CreateMutation {
        request: request.clone(),
        observed: None,
    });
    mutation_stage(file, receipt, Stage::Create)?;
    let created: CreateWorldOutput =
        installed_frames_call(client, "create_world", &request).await?;
    receipt.create.as_mut().expect("declared create").observed = Some(created.clone());
    persist(file, receipt)?;
    super::coverage::require_created(&request, &created)?;

    let request = PublishWorldRequest {
        world_id: receipt.world_id.clone(),
        expected_head_revision_id: None,
        tree: installed_frames_tree()?,
    };
    receipt.publish = Some(PublishMutation {
        request: request.clone(),
        observed: None,
    });
    mutation_stage(file, receipt, Stage::Publish)?;
    let published: PublishWorldOutput =
        installed_frames_call(client, "publish_world", &request).await?;
    receipt
        .publish
        .as_mut()
        .expect("declared publication")
        .observed = Some(published.clone());
    persist(file, receipt)?;
    ensure!(
        published.created,
        "fresh Frames recovery publication did not create a revision"
    );
    super::coverage::require_publication(&request, &created.world, &published, 1)?;

    let request = ConvertFrameRequest {
        target: CoordinateSpace::WorldFrame {
            frame_uri: published
                .revision
                .revision_uri()
                .frame(&FrameId::parse("robot-world")?),
        },
        points: vec![CoordinatePoint::Wgs84(Wgs84Position {
            latitude_degrees: 37.4220999,
            longitude_degrees: -122.0840575,
            ellipsoid_height_m: 12.0,
        })],
        allow_approximation: false,
    };
    receipt.convert = Some(ConvertMutation {
        request: request.clone(),
        observed: None,
    });
    mutation_stage(file, receipt, Stage::Convert)?;
    let direct: ConvertFrameOutput =
        installed_frames_call(client, "convert_frame", &request).await?;
    receipt
        .convert
        .as_mut()
        .expect("declared conversion")
        .observed = Some(direct.clone());
    persist(file, receipt)?;
    super::coverage::require_conversion(&direct, &published.revision, &request)?;

    let request = BatchTransformRequest {
        convert: request,
        artifact: false,
    };
    receipt.batch = Some(BatchMutation {
        request: request.clone(),
        task_id: None,
        payload: None,
        current: None,
        output: None,
    });
    mutation_stage(file, receipt, Stage::Batch)?;
    let completed = complete_tool_with_notification(
        client,
        "frames__batch_transform",
        serde_json::to_value(&request)?,
        deadline.min(tokio::time::Instant::now() + Duration::from_secs(30)),
        Duration::from_secs(30),
        |event| {
            let batch = receipt.batch.as_mut().expect("declared batch");
            match event {
                TaskNotificationObservation::Admitted(id) => batch.task_id = Some(id.clone()),
                TaskNotificationObservation::Completed { task_id, payload } => {
                    batch.task_id = Some(task_id.clone());
                    batch.payload = Some(payload.clone());
                }
            }
            persist(file, receipt)
        },
        task_state,
    )
    .await;
    receipt.cleanup.task_listener = task_state.listener_closed;
    persist(file, receipt)?;
    let (id, payload) = completed?;
    let output = admit_batch(&payload)?;
    receipt.batch.as_mut().expect("declared batch").output = Some(output.clone());
    persist(file, receipt)?;
    super::coverage::require_conversion(
        &output.result,
        &published.revision,
        &receipt
            .batch
            .as_ref()
            .expect("declared batch")
            .request
            .convert,
    )?;
    ensure!(
        direct.provenance.operation.operation_id()
            != output.result.provenance.operation.operation_id(),
        "Frames direct and Task conversions reused an operation identity"
    );
    let current = current_task(client, &id, deadline).await?;
    receipt.batch.as_mut().expect("declared batch").current = Some(current.clone());
    persist(file, receipt)?;
    require_completed_task(&id, &current, &payload)?;
    snapshot(client, file, receipt, false, deadline).await?;
    validate_before(receipt)?;
    receipt.stage = Stage::BeforeVerified;
    receipt.outcome = Outcome::FailedSettled;
    persist(file, receipt)
}

fn admit_batch(payload: &CallToolResult) -> Result<BatchTransformOutput> {
    ensure!(
        payload.is_error != Some(true),
        "Frames recovery Task completed with a tool error"
    );
    let value = payload
        .structured_content
        .clone()
        .context("Frames recovery Task omitted structured output")?;
    let _: BatchTransformTaskOutput = serde_json::from_value(value.clone())
        .map_err(|_| anyhow!("Frames recovery Task wrapper failed owner admission"))?;
    let output: BatchTransformOutput = serde_json::from_value(value)
        .map_err(|_| anyhow!("Frames recovery batch failed owner admission"))?;
    ensure!(
        output.artifact.is_none(),
        "Frames recovery artifact:false batch created an Artifact"
    );
    Ok(output)
}
async fn current_task(
    client: &SmokeMcpClient,
    id: &CanonicalTaskId,
    deadline: tokio::time::Instant,
) -> Result<DetailedTask> {
    let current = tokio::time::timeout_at(
        deadline.min(tokio::time::Instant::now() + Duration::from_secs(15)),
        client.get_task(GetTaskParams::new(id.to_string())),
    )
    .await
    .map_err(|_| anyhow!("Frames recovery current Task read deadline"))?
    .map_err(|_| anyhow!("Frames recovery current Task read failed"))?;
    Ok(current.task)
}
fn task_payload(task: &DetailedTask) -> Result<CallToolResult> {
    let TaskPayload::Completed { result } = &task.payload else {
        bail!("Frames recovery Task is not completed");
    };
    serde_json::from_value(Value::Object(result.clone()))
        .map_err(|_| anyhow!("Frames recovery Task payload failed MCP admission"))
}
fn require_completed_task(
    id: &CanonicalTaskId,
    task: &DetailedTask,
    payload: &CallToolResult,
) -> Result<()> {
    ensure!(
        task.task.task_id == id.as_str()
            && task.status() == TaskStatus::Completed
            && task_payload(task)? == *payload,
        "Frames recovery exact Task identity/status/payload changed"
    );
    Ok(())
}

impl Receipt {
    fn progress(&mut self, after: bool) -> &mut SnapshotProgress {
        if after {
            &mut self.after
        } else {
            &mut self.before
        }
    }
}
async fn snapshot(
    client: &SmokeMcpClient,
    file: &mut File,
    receipt: &mut Receipt,
    after: bool,
    deadline: tokio::time::Instant,
) -> Result<()> {
    let published = receipt
        .publish
        .as_ref()
        .and_then(|v| v.observed.as_ref())
        .context("publication absent")?;
    let revision_uri = published.revision.revision_uri().clone();
    let world_uri = FrameWorldUri::new(&receipt.world_id);
    let direct_uri = receipt
        .convert
        .as_ref()
        .and_then(|v| v.observed.as_ref())
        .context("direct conversion absent")?
        .provenance
        .operation
        .operation_uri()
        .clone();
    let batch = receipt.batch.as_ref().context("batch absent")?;
    let task_uri = batch
        .output
        .as_ref()
        .context("batch output absent")?
        .result
        .provenance
        .operation
        .operation_uri()
        .clone();
    let task_id = batch.task_id.clone().context("batch Task absent")?;
    receipt.progress(after).world = Some(installed_frames_read(client, world_uri.as_str()).await?);
    persist(file, receipt)?;
    receipt.progress(after).revision =
        Some(installed_frames_read(client, revision_uri.as_str()).await?);
    persist(file, receipt)?;
    receipt.progress(after).direct_operation =
        Some(installed_frames_read(client, direct_uri.as_str()).await?);
    persist(file, receipt)?;
    receipt.progress(after).task_operation =
        Some(installed_frames_read(client, task_uri.as_str()).await?);
    persist(file, receipt)?;
    let task = current_task(client, &task_id, deadline).await?;
    receipt.progress(after).task = Some(task.clone());
    persist(file, receipt)?;
    let payload = task_payload(&task)?;
    receipt.progress(after).payload = Some(payload.clone());
    persist(file, receipt)?;
    receipt.progress(after).batch = Some(admit_batch(&payload)?);
    persist(file, receipt)
}
fn validate_before(receipt: &Receipt) -> Result<()> {
    let before = receipt.before.complete()?;
    let published = receipt
        .publish
        .as_ref()
        .and_then(|v| v.observed.as_ref())
        .context("publication absent")?;
    let direct = receipt
        .convert
        .as_ref()
        .and_then(|v| v.observed.as_ref())
        .context("direct output absent")?;
    let batch = receipt.batch.as_ref().context("batch absent")?;
    ensure!(
        before.world == published.world && before.revision == published.revision,
        "Frames current world/head/revision differs from settled publication"
    );
    ensure!(
        before.direct_operation == direct.provenance
            && before.task_operation
                == batch
                    .output
                    .as_ref()
                    .context("batch output absent")?
                    .result
                    .provenance
            && before.batch == *batch.output.as_ref().context("batch output absent")?,
        "Frames operation provenance differs from settled outputs"
    );
    require_completed_task(
        batch.task_id.as_ref().context("Task identity absent")?,
        &before.task,
        batch
            .payload
            .as_ref()
            .context("Task completion payload absent")?,
    )?;
    ensure!(
        before.payload
            == *batch
                .payload
                .as_ref()
                .context("Task completion payload absent")?,
        "Frames Task result read differs from completion notification"
    );
    Ok(())
}
fn validate_after(before: &Snapshot, after: &Snapshot) -> Result<()> {
    ensure!(
        before.world == after.world && before.revision == after.revision,
        "Frames retained world/head/revision changed after process crash"
    );
    ensure!(
        before.direct_operation == after.direct_operation
            && before.task_operation == after.task_operation,
        "Frames retained direct/Task operation provenance changed after process crash"
    );
    ensure!(
        before.payload == after.payload && before.batch == after.batch,
        "Frames retained completed Task payload changed after process crash"
    );
    let id = CanonicalTaskId::parse(&before.task.task.task_id)?;
    require_completed_task(&id, &after.task, &before.payload)
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReadyForCrash {
    schema: &'static str,
    state: &'static str,
    identity: CrashIdentity,
    world_uri: FrameWorldUri,
    revision_uri: FrameWorldRevisionUri,
    direct_operation_uri: FrameOperationUri,
    task_id: CanonicalTaskId,
    task_operation_uri: FrameOperationUri,
    evidence_sha256: Sha256Digest,
}
fn require_ready(receipt: &Receipt) -> Result<()> {
    ensure!(
        receipt.stage == Stage::WatchArmed && receipt.settled() && receipt.cleanup.task_listener,
        "Frames crash marker requires settled mutations, acknowledged Task cleanup and an armed watch"
    );
    validate_before(receipt)?;
    Ok(())
}
fn ready_marker(receipt: &Receipt, identity: CrashIdentity) -> Result<ReadyForCrash> {
    require_ready(receipt)?;
    let before = receipt.before.complete()?;
    Ok(ReadyForCrash {
        schema: "veoveo.ai/frames-ready-for-crash/v1",
        state: "ready_for_crash",
        identity,
        world_uri: FrameWorldUri::new(&receipt.world_id),
        revision_uri: before.revision.revision_uri().clone(),
        direct_operation_uri: before.direct_operation.operation.operation_uri().clone(),
        task_id: CanonicalTaskId::parse(&before.task.task.task_id)?,
        task_operation_uri: before.task_operation.operation.operation_uri().clone(),
        evidence_sha256: Sha256Digest::from_bytes(
            Sha256::digest(serde_json::to_vec_pretty(receipt)?).into(),
        ),
    })
}
async fn foreign_reads(
    client: &SmokeMcpClient,
    file: &mut File,
    receipt: &mut Receipt,
) -> Result<()> {
    use super::coverage::{
        ForeignMethod, ForeignObservation, ForeignProbe, ForeignTarget, TransportClass,
        expected_denial, observe_peer_failure,
    };
    let before = receipt.before.complete()?;
    for uri in [
        before.direct_operation.operation.operation_uri(),
        before.task_operation.operation.operation_uri(),
    ] {
        let target = ForeignTarget::Operation { uri: uri.clone() };
        let expected = expected_denial(&target, ForeignMethod::ResourceRead);
        receipt.foreign.push(ForeignProbe {
            method: ForeignMethod::ResourceRead,
            target,
            expected,
            observed: ForeignObservation::AwaitingResponse,
        });
        persist(file, receipt)?;
        let observed = match tokio::time::timeout(
            Duration::from_secs(15),
            client.read_resource(rmcp::model::ReadResourceRequestParams::new(uri.to_string())),
        )
        .await
        {
            Ok(Err(error)) => observe_peer_failure(error),
            Ok(Ok(_)) => ForeignObservation::UnexpectedSuccess,
            Err(_) => ForeignObservation::Transport {
                class: TransportClass::Timeout,
            },
        };
        let probe = receipt.foreign.last_mut().expect("declared foreign read");
        probe.observed = observed;
        let denied = matches!(&probe.observed, ForeignObservation::PeerFailure { observed } if *observed == probe.expected);
        persist(file, receipt)?;
        ensure!(
            denied,
            "Frames foreign operation read did not return its owner denial"
        );
    }
    Ok(())
}
async fn connect(
    installation: &support::InstalledTarget,
    deadline: tokio::time::Instant,
) -> Result<SmokeMcpClient> {
    let token = tokio::time::timeout_at(frames_admission_deadline(deadline), installation.token())
        .await
        .map_err(|_| anyhow!("Frames recovery OAuth admission deadline"))?
        .map_err(|_| anyhow!("Frames recovery OAuth admission failed"))?;
    tokio::time::timeout_at(
        frames_admission_deadline(deadline),
        connect_mcp_client(installation.operator.resource.as_str(), &token),
    )
    .await
    .map_err(|_| anyhow!("Frames recovery MCP admission deadline"))?
    .map_err(|_| anyhow!("Frames recovery MCP admission failed"))
}
fn load_fixture(path: &Path) -> Result<CrashFixture> {
    ensure!(
        path.is_absolute(),
        "Frames crash fixture path must be absolute"
    );
    let entry = std::fs::symlink_metadata(path)?;
    ensure!(
        entry.is_file(),
        "Frames crash fixture must not be a symlink"
    );
    let mut file = File::open(path).context("opening Frames crash fixture")?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.dev() == entry.dev() && metadata.ino() == entry.ino(),
        "Frames crash fixture changed during admission"
    );
    ensure!(
        metadata.is_file()
            && metadata.permissions().mode() & 0o777 == 0o600
            && metadata.len() <= 64 * 1024,
        "Frames crash fixture must be a private regular file <=64 KiB"
    );
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take(64 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 64 * 1024,
        "Frames crash fixture exceeds 64 KiB"
    );
    serde_json::from_slice(&bytes)
        .map_err(|_| anyhow!("Frames crash fixture failed typed admission"))
}

/// The fixture dispatch future starts only after read-only live admission settles.
async fn after_admission<A, D, F, T>(admission: A, dispatch: D) -> Result<()>
where
    A: std::future::Future<Output = Result<T>>,
    D: FnOnce(T) -> F,
    F: std::future::Future<Output = Result<()>>,
{
    let admitted = admission.await?;
    dispatch(admitted).await
}

pub(crate) async fn run(
    installation_path: &Path,
    crash_fixture_path: &Path,
    marker_path: &Path,
    evidence_path: &Path,
) -> Result<()> {
    ensure!(
        marker_path != evidence_path
            && marker_path != crash_fixture_path
            && evidence_path != crash_fixture_path,
        "Frames crash input/marker/evidence paths must differ"
    );
    let installation = support::InstalledTarget::load(installation_path)?;
    let administrator = installation.administrator()?;
    ensure!(
        administrator.principal != installation.operator.principal,
        "foreign Frames identity is not distinct"
    );
    let fixture = load_fixture(crash_fixture_path)?;
    fixture.admit(
        &installation,
        &std::fs::read(installation.target.control_plane_path(installation_path))?,
    )?;
    // Exclusive handles are admitted before any domain mutation. An empty marker is not ReadyForCrash.
    let mut file = admit_frames_evidence(evidence_path)?;
    let mut marker_file = admit_frames_evidence(marker_path)?;
    let mut receipt = Receipt::new(FrameWorldId::parse(format!(
        "acceptance-recovery-{}",
        uuid::Uuid::new_v4()
    ))?);
    receipt.fixture = Some(fixture.clone());
    persist(&mut file, &receipt)?;
    let mut original = None;
    let mut fresh = None;
    let mut foreign = None;
    let mut watch: Option<CrashWatch> = None;
    let mut task_state = TaskNotificationState::default();
    let deadline = tokio::time::Instant::now() + OPERATION_BUDGET;
    let result = tokio::time::timeout_at(deadline, async {
        original = Some(connect(&installation, deadline).await?);
        let client = original.as_ref().context("original Frames client absent")?;
        let token =
            tokio::time::timeout_at(frames_admission_deadline(deadline), administrator.token())
                .await
                .map_err(|_| anyhow!("foreign Frames OAuth deadline"))?
                .map_err(|_| anyhow!("foreign Frames OAuth admission failed"))?;
        foreign = Some(
            tokio::time::timeout_at(
                frames_admission_deadline(deadline),
                connect_mcp_client(administrator.resource.as_str(), &token),
            )
            .await
            .map_err(|_| anyhow!("foreign Frames MCP deadline"))?
            .map_err(|_| anyhow!("foreign Frames MCP admission failed"))?,
        );
        let restart = DeploymentRestart::new(
            &installation.target,
            &fixture.target.deployment,
            "frames-mcp",
            client.peer().clone(),
            FramesResource::Contract.to_uri()?,
        )?;
        after_admission(
            async {
                tokio::time::timeout_at(
                    deadline.min(tokio::time::Instant::now() + Duration::from_secs(30)),
                    restart.admit_crash_target(&fixture.target),
                )
                .await
                .map_err(|_| anyhow!("Frames live crash target admission deadline"))??;
                Ok(())
            },
            |()| async {
                receipt.stage = Stage::LiveTargetAdmitted;
                persist(&mut file, &receipt)?;
                prepare(client, &mut file, &mut receipt, &mut task_state, deadline).await
            },
        )
        .await?;
        // Settlement can take time; arm repeats live admission before publishing the marker.
        restart.arm_crash_watch(&fixture.target, &mut watch).await?;
        let identity = watch
            .as_ref()
            .context("Frames crash watch absent")?
            .identity()?;
        receipt.selected = Some(identity.clone());
        receipt.stage = Stage::WatchArmed;
        persist(&mut file, &receipt)?;
        let mut marker = ready_marker(&receipt, identity)?;
        receipt.stage = Stage::ReadyForCrash;
        receipt.outcome = Outcome::CrashUnresolved;
        persist(&mut file, &receipt)?;
        marker.evidence_sha256 =
            Sha256Digest::from_bytes(Sha256::digest(serde_json::to_vec_pretty(&receipt)?).into());
        // Ops alone performs the separately authorized kill after observing this complete marker.
        marker_file.write_all(&serde_json::to_vec_pretty(&marker)?)?;
        marker_file.sync_data()?;
        receipt.crash = Some(
            watch
                .as_mut()
                .context("Frames crash watch absent")?
                .wait(deadline)
                .await?,
        );
        receipt.stage = Stage::CrashObserved;
        persist(&mut file, &receipt)?;
        fresh = Some(connect(&installation, deadline).await?);
        snapshot(
            fresh.as_ref().context("fresh Frames client absent")?,
            &mut file,
            &mut receipt,
            true,
            deadline,
        )
        .await?;
        validate_after(&receipt.before.complete()?, &receipt.after.complete()?)?;
        foreign_reads(
            foreign.as_ref().context("foreign Frames client absent")?,
            &mut file,
            &mut receipt,
        )
        .await?;
        after_admission(
            async {
                let active = watch.as_mut().context("Frames crash watch absent")?;
                tokio::time::timeout_at(
                    deadline.min(tokio::time::Instant::now() + Duration::from_secs(30)),
                    active.admit_recovered_target(),
                )
                .await
                .map_err(|_| anyhow!("Frames final live identity fence deadline"))??;
                active
                    .snapshot()
                    .context("Frames final identity proof absent")
            },
            |crash| {
                receipt.crash = Some(crash);
                async {
                    persist(&mut file, &receipt)?;
                    receipt.stage = Stage::AfterVerified;
                    persist(&mut file, &receipt)
                }
            },
        )
        .await
    })
    .await;
    if !matches!(&result, Ok(Ok(()))) {
        receipt.failed_stage = Some(receipt.stage);
    }
    if let Some(active) = &watch {
        receipt.crash = active.snapshot();
    }
    // Receipt I/O failure must not bypass cancellation of caller-owned resources.
    let mut persistence_failed = persist(&mut file, &receipt).is_err();
    receipt.cleanup.task_listener = task_state.close().await.is_ok();
    persistence_failed |= persist(&mut file, &receipt).is_err();
    receipt.cleanup.watch = match watch.take() {
        Some(active) => active.close().await,
        None => true,
    };
    persistence_failed |= persist(&mut file, &receipt).is_err();
    receipt.cleanup.original_client = close_client(original.take()).await;
    persistence_failed |= persist(&mut file, &receipt).is_err();
    receipt.cleanup.fresh_client = close_client(fresh.take()).await;
    persistence_failed |= persist(&mut file, &receipt).is_err();
    receipt.cleanup.foreign_client = close_client(foreign.take()).await;
    receipt.outcome = final_outcome(&receipt, matches!(&result, Ok(Ok(()))), persistence_failed);
    persist(&mut file, &receipt)?;
    ensure!(
        receipt.outcome == Outcome::Passed,
        "Frames completed-state process-crash qualification failed; private receipt records settled/unknown outcomes"
    );
    Ok(())
}
fn final_outcome(receipt: &Receipt, operation_passed: bool, persistence_failed: bool) -> Outcome {
    if operation_passed
        && receipt.stage == Stage::AfterVerified
        && receipt
            .crash
            .as_ref()
            .is_some_and(|crash| crash.final_fenced_at.is_some())
        && receipt.cleanup.closed()
        && !persistence_failed
    {
        Outcome::Passed
    } else if receipt.stage == Stage::ReadyForCrash {
        Outcome::CrashUnresolved
    } else if receipt.settled() {
        Outcome::FailedSettled
    } else if receipt.create.is_none() {
        Outcome::FailedBeforeMutation
    } else {
        Outcome::MutationUnresolved
    }
}
async fn close_client(client: Option<SmokeMcpClient>) -> bool {
    match client {
        Some(client) => matches!(
            tokio::time::timeout(Duration::from_secs(5), client.cancel()).await,
            Ok(Ok(_))
        ),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn retained() -> Result<Snapshot> {
        let world_id = FrameWorldId::parse("recovery-control")?;
        let revision_id = FrameWorldRevisionId::parse("revision-control")?;
        let at = Utc::now();
        let revision = FrameWorldRevision::new(
            FrameWorldRevisionUri::new(&world_id, &revision_id),
            1.try_into()?,
            ValidatedWorldTree::new(installed_frames_tree()?)?,
            at,
        );
        let world = FrameWorldSummary::new(world_id, "recovery control".into(), at)
            .with_head(revision_id, 1.try_into()?);
        let target = CoordinateSpace::WorldFrame {
            frame_uri: revision
                .revision_uri()
                .frame(&FrameId::parse("robot-world")?),
        };
        let operation = |name: &str| -> Result<CoordinateOperationProvenance> {
            Ok(CoordinateOperationProvenance {
                operation: CoordinateOperationRef::new(CoordinateOperationId::parse(name)?, at)
                    .with_frames(Some(CoordinateSpace::Wgs84), Some(target.clone())),
                kind: CoordinateOperationKind::FrameConversion,
                source_crs: None,
                target_crs: None,
                engine: None,
                grid_packages: vec![],
                approximation_used: false,
                accuracy_m: None,
                warnings: vec![],
            })
        };
        let direct_operation = operation("op-direct-control")?;
        let task_operation = operation("op-task-control")?;
        let batch = BatchTransformOutput {
            result: ConvertFrameOutput {
                points: vec![CoordinatePoint::WorldFrame(WorldFramePosition {
                    frame_uri: revision
                        .revision_uri()
                        .frame(&FrameId::parse("robot-world")?),
                    x_m: 1.0,
                    y_m: 2.0,
                    z_m: 3.0,
                })],
                provenance: task_operation.clone(),
                sources: vec![FrameSourceReference::new(
                    revision.revision_uri().clone(),
                    revision.spec_digest().clone(),
                )],
            },
            artifact: None,
        };
        let mut payload = CallToolResult::default();
        payload.structured_content = Some(serde_json::to_value(&batch)?);
        let Value::Object(result) = serde_json::to_value(&payload)? else {
            unreachable!()
        };
        let task = DetailedTask::new(
            rmcp::model::Task::new(
                "gtr_recovery_control",
                TaskStatus::Completed,
                at.to_rfc3339(),
                at.to_rfc3339(),
            ),
            TaskPayload::Completed { result },
        );
        Ok(Snapshot {
            world,
            revision,
            direct_operation,
            task_operation,
            task,
            payload,
            batch,
        })
    }
    fn prepared() -> Result<Receipt> {
        let snapshot = retained()?;
        let world_id = snapshot.world.world_id();
        let mut receipt = Receipt::new(world_id.clone());
        receipt.create = Some(CreateMutation {
            request: CreateWorldRequest {
                world_id: world_id.clone(),
                display_name: snapshot.world.display_name.clone(),
                description: None,
            },
            observed: Some(CreateWorldOutput {
                world: snapshot.world.clone(),
            }),
        });
        receipt.publish = Some(PublishMutation {
            request: PublishWorldRequest {
                world_id,
                expected_head_revision_id: None,
                tree: installed_frames_tree()?,
            },
            observed: Some(PublishWorldOutput {
                world: snapshot.world.clone(),
                revision: snapshot.revision.clone(),
                created: true,
            }),
        });
        let direct = ConvertFrameOutput {
            provenance: snapshot.direct_operation.clone(),
            ..snapshot.batch.result.clone()
        };
        let convert = ConvertFrameRequest {
            target: direct
                .provenance
                .operation
                .target_frame
                .clone()
                .context("target")?,
            points: vec![CoordinatePoint::Wgs84(Wgs84Position {
                latitude_degrees: 1.0,
                longitude_degrees: 2.0,
                ellipsoid_height_m: 3.0,
            })],
            allow_approximation: false,
        };
        receipt.convert = Some(ConvertMutation {
            request: convert.clone(),
            observed: Some(direct),
        });
        receipt.batch = Some(BatchMutation {
            request: BatchTransformRequest {
                convert,
                artifact: false,
            },
            task_id: Some(CanonicalTaskId::parse(&snapshot.task.task.task_id)?),
            payload: Some(snapshot.payload.clone()),
            current: Some(snapshot.task.clone()),
            output: Some(snapshot.batch.clone()),
        });
        receipt.before = SnapshotProgress {
            world: Some(snapshot.world),
            revision: Some(snapshot.revision),
            direct_operation: Some(snapshot.direct_operation),
            task_operation: Some(snapshot.task_operation),
            task: Some(snapshot.task),
            payload: Some(snapshot.payload),
            batch: Some(snapshot.batch),
        };
        receipt.stage = Stage::WatchArmed;
        receipt.cleanup.task_listener = true;
        Ok(receipt)
    }
    #[test]
    fn crash_marker_requires_settled_outputs_reads_and_acknowledged_listener_cleanup() -> Result<()>
    {
        let mut receipt = prepared()?;
        require_ready(&receipt)?;
        receipt.cleanup.task_listener = false;
        assert!(require_ready(&receipt).is_err());
        receipt.cleanup.task_listener = true;
        receipt.stage = Stage::BeforeVerified;
        assert!(require_ready(&receipt).is_err());
        receipt.stage = Stage::WatchArmed;
        let payload = receipt.batch.as_mut().unwrap().payload.take();
        assert!(require_ready(&receipt).is_err());
        receipt.batch.as_mut().unwrap().payload = payload;
        receipt.before.direct_operation = None;
        assert!(require_ready(&receipt).is_err());
        Ok(())
    }
    #[test]
    fn recovery_rejects_changed_world_operation_task_identity_or_payload() -> Result<()> {
        let before = retained()?;
        validate_after(&before, &before)?;
        let mut after = before.clone();
        after.world.display_name.push('x');
        assert!(validate_after(&before, &after).is_err());
        after = before.clone();
        after.direct_operation = before.task_operation.clone();
        assert!(validate_after(&before, &after).is_err());
        after = before.clone();
        after.task.task.task_id = "gtr_other_control".into();
        assert!(validate_after(&before, &after).is_err());
        after = before.clone();
        after.payload.is_error = Some(true);
        assert!(validate_after(&before, &after).is_err());
        after = before.clone();
        after.task.task.ttl_ms = Some(60000);
        after.task.task.poll_interval_ms = Some(1000);
        validate_after(&before, &after)?;
        Ok(())
    }
    #[test]
    fn known_conversion_provenance_must_match_retained_read_before_crash() -> Result<()> {
        let mut receipt = prepared()?;
        validate_before(&receipt)?;
        receipt.before.task_operation = receipt.before.direct_operation.clone();
        assert!(validate_before(&receipt).is_err());
        Ok(())
    }
    #[tokio::test]
    async fn cancelled_next_read_retains_persisted_settled_snapshot() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("receipt.json");
        let mut file = admit_frames_evidence(&path)?;
        let mut receipt = prepared()?;
        let outcome = tokio::time::timeout(Duration::from_millis(10), async {
            receipt.stage = Stage::BeforeVerified;
            persist(&mut file, &receipt)?;
            std::future::pending::<()>().await;
            Ok::<(), anyhow::Error>(())
        })
        .await;
        assert!(outcome.is_err());
        assert!(receipt.settled());
        validate_before(&receipt)?;
        let stored: Value = serde_json::from_slice(&std::fs::read(path)?)?;
        assert_eq!(stored["stage"], "before_verified");
        assert!(stored["before"]["payload"].is_object());
        assert!(!receipt.cleanup.closed());
        Ok(())
    }
    #[tokio::test]
    async fn rejected_or_cancelled_live_admission_dispatches_no_domain_fixture_operations()
    -> Result<()> {
        use std::cell::Cell;
        let dispatches = Cell::new(0);
        // The shared selector's owning controls cover actual stale UID/CID/image/count and
        // wrong-target rejection. This production gate proves such failures never invoke
        // the fixture dispatch closure, including cancellation while admission is pending.
        for rejected in [
            "stale live container identity",
            "wrong live Deployment identity",
        ] {
            let result = after_admission(async { Err(anyhow!(rejected)) }, |()| async {
                dispatches.set(dispatches.get() + 1);
                Ok(())
            })
            .await;
            assert!(result.is_err());
            assert_eq!(dispatches.get(), 0);
        }
        let result = tokio::time::timeout(
            Duration::from_millis(10),
            after_admission(std::future::pending::<Result<()>>(), |()| async {
                dispatches.set(dispatches.get() + 1);
                Ok(())
            }),
        )
        .await;
        assert!(result.is_err());
        assert_eq!(dispatches.get(), 0);
        after_admission(async { Ok(()) }, |()| async {
            dispatches.set(dispatches.get() + 1);
            Ok(())
        })
        .await?;
        assert_eq!(dispatches.get(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn final_identity_fence_failure_cannot_qualify_completed_reads() -> Result<()> {
        let mut receipt = prepared()?;
        receipt.stage = Stage::CrashObserved;
        receipt.cleanup = Cleanup {
            task_listener: true,
            original_client: true,
            fresh_client: true,
            foreign_client: true,
            watch: true,
        };
        let result = after_admission(
            async { Err(anyhow!("replacement restarted again during retained reads")) },
            |()| async {
                receipt.stage = Stage::AfterVerified;
                Ok(())
            },
        )
        .await;
        assert!(result.is_err());
        assert!(receipt.stage == Stage::CrashObserved);
        assert!(final_outcome(&receipt, result.is_ok(), false) == Outcome::FailedSettled);
        // A successful read outcome alone cannot qualify without the final live identity proof.
        receipt.stage = Stage::AfterVerified;
        assert!(final_outcome(&receipt, true, false) != Outcome::Passed);
        receipt.cleanup.watch = false;
        assert!(final_outcome(&receipt, true, false) != Outcome::Passed);
        Ok(())
    }
}
