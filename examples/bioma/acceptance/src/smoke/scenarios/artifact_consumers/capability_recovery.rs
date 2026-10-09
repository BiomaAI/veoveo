//! One opt-in Artifact SERVICE replacement; the Python SDK retains the bearer.
//! The operator reserves the admitted Work Context against concurrent fixture writes.
use super::*;
use std::{
    collections::BTreeMap,
    future::Future,
    io::Write,
    num::NonZeroU64,
    pin::Pin,
    sync::{Arc, Mutex},
};
use tokio::io::{AsyncBufReadExt, BufReader};
use veoveo_artifact_contract::{
    ArtifactId, ArtifactMetadata, ArtifactPage, ArtifactTaskId, ArtifactWriteCapabilityId,
    ArtifactWriteIdempotencyKey, IssueArtifactWriteCapabilityRequest, PutArtifactRequest,
};
use veoveo_artifact_mcp::contract::ArtifactResource;
use veoveo_testing_support::installed::restart::{
    DeploymentRestart, DrainProfile, DrainReceipt, SelectedDrainIdentity,
};
use veoveo_testing_support::lifecycle::owner::{self, CleanupKind, CleanupRegistration};

const BYTES: &[u8] = b"artifact-service retained capability fixture\n";
const DEADLINE: Duration = Duration::from_secs(300);

fn journal_path(output: &Path) -> std::path::PathBuf {
    output.with_extension("service-recovery.jsonl")
}

pub(super) fn admit(installation: &InstalledTarget, output: &Path) -> Result<()> {
    installation.target.validate()?;
    ensure!(
        installation
            .target
            .artifact_consumer
            .as_ref()
            .and_then(|c| c.service_recovery.as_ref())
            .is_some(),
        "--service-recovery requires explicit artifactConsumer.serviceRecovery and a reserved stable operator Work Context"
    );
    admit_output_policy(&installation.operator.work_context.output_policy)?;
    ensure!(
        !journal_path(output).exists(),
        "service recovery journal already exists"
    );
    Ok(())
}

fn admit_output_policy(policy: &veoveo_types::WorkContextOutputPolicy) -> Result<()> {
    // Canonical issue_write_capability inherits classification and output labels.
    // The unchanged fixture actor has no clearance; Viewer itself is admitted.
    let mut inherited = policy.data_labels.clone();
    inherited.extend(policy.classification.iter().cloned());
    let fixture_clearance = BTreeSet::new();
    ensure!(
        inherited.is_subset(&fixture_clearance),
        "service recovery requires a reserved operator Work Context with no output sensitivity labels; the fixture actor has no clearance"
    );
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Input<'a> {
    artifact_service_url: &'a str,
    caller: python::CallerInput,
    issue: IssueArtifactWriteCapabilityRequest,
    artifact: PutArtifactRequest,
    idempotency_key: ArtifactWriteIdempotencyKey,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "phase", rename_all = "camelCase", deny_unknown_fields)]
enum Event {
    Issued {
        #[serde(rename = "capabilityId")]
        capability_id: ArtifactWriteCapabilityId,
        #[serde(rename = "taskId")]
        task_id: ArtifactTaskId,
        #[serde(rename = "expiresAt")]
        expires_at: chrono::DateTime<chrono::Utc>,
        pages: Vec<ArtifactPage>,
    },
    Redeemed {
        observation: Observation,
    },
    Replayed {
        observation: Observation,
    },
    Failed,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Observation {
    metadata: ArtifactMetadata,
    head: ArtifactMetadata,
    resolved: ArtifactMetadata,
    sha256: String,
    pages: Vec<ArtifactPage>,
}

#[derive(Serialize)]
#[serde(
    tag = "phase",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum Record<'a> {
    FixtureIntent {
        schema: &'static str,
        source_revision: String,
        payload_sha256: UploadSha256,
        transport: &'static str,
        issue: &'a IssueArtifactWriteCapabilityRequest,
        artifact: &'a PutArtifactRequest,
        idempotency_key: &'a ArtifactWriteIdempotencyKey,
        work_context: &'a veoveo_types::WorkContextId,
        tenant: &'a veoveo_types::TenantId,
        namespace: &'a str,
        selection: &'a veoveo_deploy_contract::InstallationArtifactServiceRecovery,
    },
    IssueIntent {
        selected: SelectedDrainIdentity,
    },
    RestartIntent {
        selected: SelectedDrainIdentity,
    },
    Replaced {
        receipt: DrainReceipt,
    },
    RedeemIntent,
    ReplayIntent,
    OwnerCleanup {
        operation: OperationState,
        owner_cancelled: bool,
        child: ChildSettlement,
        mcp: CloseStatus,
    },
    Outcome {
        operation_passed: bool,
        child_cleanup: ChildSettlement,
        mcp_cleanup: CloseStatus,
        capability_expires_at: chrono::DateTime<chrono::Utc>,
        requested_retention_expires_at: chrono::DateTime<chrono::Utc>,
        reconciliation: Reconciliation,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum Reconciliation {
    OneOccurrenceVerifiedNoDeletionApi,
    UnresolvedInspectIntentNoMutationRetry,
}

// Append-only, create-new and fsync before dispatch. Any unfinished intent is
// unresolved, including issuance without a response. There is no revoke/delete API.
#[derive(Clone)]
struct Journal(Arc<Mutex<fs::File>>);
impl Journal {
    fn append(&self, value: &impl Serialize) -> Result<()> {
        let mut file = self.0.lock().expect("recovery journal");
        file.write_all(&serde_json::to_vec(value)?)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        Ok(())
    }
}

#[derive(Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum CloseStatus {
    NotAcquired,
    Retained,
    Closing,
    Passed,
    Failed,
}
#[derive(Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum ChildSettlement {
    NotSpawned,
    OwnedGroupRegistration,
    ObservedExit,
}
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
enum OperationState {
    Unfinished,
    Passed,
    Failed,
}
type CloseFuture = Pin<Box<dyn Future<Output = Result<()>> + Send>>;
struct ClientClose {
    status: CloseStatus,
    future: Option<CloseFuture>,
    deadline: Option<std::time::Instant>,
}
struct CleanupFacts {
    operation: OperationState,
    child: ChildSettlement,
}
struct RetainedCleanup {
    client: tokio::sync::Mutex<ClientClose>,
    facts: Mutex<CleanupFacts>,
    journal: Journal,
}
impl RetainedCleanup {
    fn register(journal: Journal, identity: &str) -> Result<(Arc<Self>, CleanupRegistration)> {
        let retained = Arc::new(Self {
            client: tokio::sync::Mutex::new(ClientClose {
                status: CloseStatus::NotAcquired,
                future: None,
                deadline: None,
            }),
            facts: Mutex::new(CleanupFacts {
                operation: OperationState::Unfinished,
                child: ChildSettlement::NotSpawned,
            }),
            journal,
        });
        let cleanup = Arc::clone(&retained);
        let registration = owner::register_cleanup(
            CleanupKind::Remote,
            "artifact_service_recovery_client",
            identity,
            move || async move {
                // Persist global drop facts even if the original close later
                // exhausts the owner interval. Reporting failure cannot skip close.
                let before = cleanup.record().await;
                let result = cleanup.close().await;
                let after = cleanup.record().await;
                before?;
                after?;
                result
            },
        )?;
        Ok((retained, registration))
    }

    // Called synchronously after acquisition, before another cancellation point.
    // The original consuming close future owns the actual client; resumption never
    // constructs a second client or treats an emptied SDK slot as acknowledgement.
    fn retain(&self, future: impl Future<Output = Result<()>> + Send + 'static) {
        let mut client = self.client.try_lock().expect("new recovery client slot");
        assert!(client.status == CloseStatus::NotAcquired);
        client.future = Some(Box::pin(future));
        client.status = CloseStatus::Retained;
    }

    async fn close(&self) -> Result<()> {
        let end = owner::cleanup_deadline()?;
        let mut client = self.client.lock().await;
        match client.status {
            CloseStatus::NotAcquired | CloseStatus::Passed => return Ok(()),
            CloseStatus::Failed => anyhow::bail!("original recovery client close failed"),
            CloseStatus::Retained | CloseStatus::Closing => {}
        }
        let deadline = client.deadline.map_or(end, |previous| previous.min(end));
        client.deadline = Some(deadline);
        client.status = CloseStatus::Closing;
        // If the operation future is dropped here, this same future and its first
        // deadline remain in the owner-retained slot for the registered cleanup.
        let future = client
            .future
            .as_mut()
            .context("original recovery client close unavailable")?;
        let passed = matches!(
            tokio::time::timeout_at(deadline.into(), future).await,
            Ok(Ok(()))
        );
        client.status = if passed {
            CloseStatus::Passed
        } else {
            CloseStatus::Failed
        };
        client.future = None;
        ensure!(
            passed,
            "original recovery client close failed or exceeded owner cleanup deadline"
        );
        Ok(())
    }

    async fn record(&self) -> Result<()> {
        let status = self.client.lock().await.status;
        let facts = self.facts.lock().expect("recovery cleanup facts");
        self.journal.append(&Record::OwnerCleanup {
            operation: facts.operation,
            owner_cancelled: owner::check_effect().is_err(),
            child: facts.child,
            mcp: status,
        })
    }
}

async fn event(reader: &mut BufReader<tokio::process::ChildStdout>) -> Result<Event> {
    let mut line = Vec::new();
    loop {
        let bytes = reader.fill_buf().await?;
        ensure!(
            !bytes.is_empty(),
            "recovery SDK child closed without an observation"
        );
        let count = bytes
            .iter()
            .position(|b| *b == b'\n')
            .map_or(bytes.len(), |n| n + 1);
        ensure!(
            line.len() + count <= 4 * 1024 * 1024,
            "recovery SDK observation exceeds 4 MiB"
        );
        let finished = bytes[count - 1] == b'\n';
        line.extend_from_slice(&bytes[..count]);
        reader.consume(count);
        if finished {
            break;
        }
    }
    // Never attach decoder source: malformed peer input might contain a secret.
    serde_json::from_slice(&line).map_err(|_| anyhow::anyhow!("invalid recovery SDK observation"))
}

fn catalog(pages: &[ArtifactPage]) -> Result<BTreeMap<ArtifactId, ArtifactMetadata>> {
    ensure!(
        !pages.is_empty() && pages.len() <= 256,
        "recovery catalog traversal bound"
    );
    let mut result = BTreeMap::new();
    let mut previous = None;
    for (index, page) in pages.iter().enumerate() {
        ensure!(
            page.artifacts.len() <= 1,
            "recovery catalog ignored limit one"
        );
        for metadata in &page.artifacts {
            let id = metadata.artifact_id();
            ensure!(
                previous.is_none_or(|old| id < old),
                "recovery catalog order or duplicate changed"
            );
            previous = Some(id);
            ensure!(
                result.insert(id, metadata.clone()).is_none(),
                "duplicate recovery occurrence"
            );
        }
        if index + 1 == pages.len() {
            ensure!(
                page.next_cursor.is_none(),
                "recovery catalog did not terminate"
            );
        } else {
            ensure!(
                !page.artifacts.is_empty() && page.next_cursor == previous,
                "recovery catalog continuation mismatch"
            );
        }
    }
    Ok(result)
}

fn validate_observation(
    installation: &InstalledTarget,
    observation: &Observation,
    artifact: &PutArtifactRequest,
    caller: &GatewayInternalIdentity,
    before: &BTreeMap<ArtifactId, ArtifactMetadata>,
) -> Result<BTreeMap<ArtifactId, ArtifactMetadata>> {
    let metadata = &observation.metadata;
    ensure!(
        metadata == &observation.head && metadata == &observation.resolved,
        "recovery SDK readback changed occurrence metadata"
    );
    ensure!(
        metadata.byte_len == BYTES.len() as u64
            && observation.sha256 == hex::encode(Sha256::digest(BYTES))
            && metadata.filename == artifact.filename
            && metadata.mime_type == artifact.mime_type
            && metadata.compliance.retention_expires_at == artifact.retention_expires_at
            && metadata.compliance.work_context.as_ref()
                == Some(&installation.operator.work_context.id)
            && metadata.compliance.tenant_id.as_ref() == Some(&installation.operator.tenant),
        "recovery occurrence bytes, descriptor, tenancy or Work Context changed"
    );
    ensure!(
        metadata.compliance.owner.as_ref() == Some(&caller.authority.output_policy.owner)
            && metadata.compliance.classification == caller.authority.output_policy.classification
            && metadata.compliance.provenance.as_ref()
                == Some(&veoveo_artifact_contract::ArtifactProvenance::new(
                    caller.actor.id.clone(),
                    caller.authority.provenance.clone(),
                    caller.authority.policy_revision.clone()
                )),
        "recovery occurrence changed admitted owner or invocation provenance"
    );
    let after = catalog(&observation.pages)?;
    let mut expected = before.clone();
    ensure!(
        expected
            .insert(metadata.artifact_id(), metadata.clone())
            .is_none(),
        "capability redemption reused a preexisting occurrence"
    );
    ensure!(
        after == expected,
        "recovery context catalog changed beyond one owned occurrence; reserve the fixture Work Context"
    );
    Ok(after)
}

pub(super) async fn run(installation: &InstalledTarget, output: &Path) -> Result<()> {
    admit(installation, output)?;
    let consumer = installation
        .target
        .artifact_consumer
        .as_ref()
        .context("missing consumer")?;
    let recovery = consumer
        .service_recovery
        .as_ref()
        .context("missing recovery selection")?;
    let task_id = ArtifactTaskId::new();
    let expires_at = chrono::Utc::now() + chrono::Duration::minutes(10);
    let input = Input {
        artifact_service_url: consumer.artifact_service_url.as_str(),
        caller: python::recovery_caller(installation).await?,
        issue: IssueArtifactWriteCapabilityRequest {
            task_id,
            expires_at,
            max_artifact_count: NonZeroU32::new(1).context("count")?,
            max_total_bytes: NonZeroU64::new(BYTES.len() as u64).context("bytes")?,
            required_data_labels: BTreeSet::new(),
        },
        artifact: PutArtifactRequest {
            filename: Some("artifact-service-recovery.txt".into()),
            mime_type: Some("text/plain".into()),
            retention_expires_at: Some(expires_at),
            ..Default::default()
        },
        idempotency_key: ArtifactWriteIdempotencyKey::new(format!("service-recovery-{task_id}"))?,
    };
    let path = journal_path(output);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let journal = Journal(Arc::new(Mutex::new(
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?,
    )));
    journal.append(&Record::FixtureIntent {
        schema: "veoveo.ai/artifact-service-recovery/v1",
        source_revision: run_checked(Path::new("git"), ["rev-parse".into(), "HEAD".into()], [])?
            .trim()
            .to_owned(),
        payload_sha256: UploadSha256::parse(hex::encode(Sha256::digest(BYTES)))?,
        transport: "internal-delegated-sdk; public OAuth MCP used only for backend readiness",
        issue: &input.issue,
        artifact: &input.artifact,
        idempotency_key: &input.idempotency_key,
        work_context: &installation.operator.work_context.id,
        tenant: &installation.operator.tenant,
        namespace: &installation.target.kubernetes.namespace,
        selection: recovery,
    })?;
    let (cleanup, cleanup_registration) =
        RetainedCleanup::register(journal.clone(), &task_id.to_string())?;
    let token = installation.token().await?;
    let mcp = tokio::time::timeout(
        Duration::from_secs(15),
        veoveo_testing_support::connect_mcp_client(installation.operator.resource.as_str(), &token),
    )
    .await
    .context("recovery readiness client timed out")??;
    let peer = mcp.peer().clone();
    cleanup.retain(async move { mcp.cancel().await });
    let mut child = None;
    let outcome = tokio::time::timeout(DEADLINE, async {
        let restart = DeploymentRestart::new(
            &installation.target,
            &recovery.deployment,
            "artifact-service",
            peer,
            ArtifactResource::Index { cursor: None }.to_uri(),
        )?;
        let selected = restart
            .select_drain_target(
                &recovery.pod,
                DrainProfile::server(&recovery.container, Duration::from_secs(30))?,
            )
            .await?;
        journal.append(&Record::IssueIntent {
            selected: selected.identity(),
        })?;
        let mut command = tokio::process::Command::new("kubectl");
        command
            .args([
                "--request-timeout=30s",
                "--context",
                &installation.target.kubernetes.context,
                "-n",
                &installation.target.kubernetes.namespace,
                "exec",
                "-i",
                &format!("deployment/{}", consumer.python_deployment),
                "--",
                "python",
                "-c",
                CONSUMER,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        child = Some(veoveo_testing_support::spawn_async(command)?);
        cleanup.facts.lock().expect("recovery cleanup facts").child =
            ChildSettlement::OwnedGroupRegistration;
        let process = child.as_mut().context("missing recovery child")?;
        let mut stdin = process.stdin.take().context("missing recovery stdin")?;
        let mut stdout = BufReader::new(process.stdout.take().context("missing recovery stdout")?);
        stdin.write_all(&serde_json::to_vec(&input)?).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;
        let issued = event(&mut stdout).await?;
        journal.append(&issued)?;
        let Event::Issued {
            task_id: actual_task,
            expires_at: actual_expiry,
            pages,
            ..
        } = issued
        else {
            anyhow::bail!("recovery SDK did not issue a capability");
        };
        ensure!(
            actual_task == task_id && actual_expiry == expires_at,
            "issued capability changed Task or expiry"
        );
        let before = catalog(&pages)?;
        journal.append(&Record::RestartIntent {
            selected: selected.identity(),
        })?;
        let receipt = restart.restart_with_drain(&selected).await?;
        journal.append(&Record::Replaced { receipt })?;
        journal.append(&Record::RedeemIntent)?;
        stdin.write_all(b"redeem\n").await?;
        stdin.flush().await?;
        let redeemed = event(&mut stdout).await?;
        journal.append(&redeemed)?;
        let Event::Redeemed { observation: first } = redeemed else {
            anyhow::bail!("recovery SDK redemption failed");
        };
        let after = validate_observation(
            installation,
            &first,
            &input.artifact,
            &input.caller.identity,
            &before,
        )?;
        journal.append(&Record::ReplayIntent)?;
        stdin.write_all(b"replay\n").await?;
        stdin.flush().await?;
        let replayed = event(&mut stdout).await?;
        journal.append(&replayed)?;
        let Event::Replayed {
            observation: replay,
        } = replayed
        else {
            anyhow::bail!("recovery SDK replay failed");
        };
        ensure!(
            first.metadata == replay.metadata,
            "capability replay created a different occurrence"
        );
        let replay_after = validate_observation(
            installation,
            &replay,
            &input.artifact,
            &input.caller.identity,
            &before,
        )?;
        ensure!(replay_after == after, "capability replay changed catalog");
        drop(stdin);
        ensure!(
            process.wait().await?.success(),
            "recovery SDK child failed during close"
        );
        cleanup.facts.lock().expect("recovery cleanup facts").child = ChildSettlement::ObservedExit;
        Ok::<_, anyhow::Error>(())
    })
    .await
    .context("Artifact service recovery exceeded 300 seconds")
    .and_then(|result| result);
    cleanup
        .facts
        .lock()
        .expect("recovery cleanup facts")
        .operation = if outcome.is_ok() {
        OperationState::Passed
    } else {
        OperationState::Failed
    };
    // Capture one owner interval before either local close. The healthy owner's
    // accessor may advance until outer cleanup begins; the retained MCP attempt
    // must not gain another interval after waiting for the child.
    let cleanup_end = owner::cleanup_deadline()?;
    {
        let mut client = cleanup
            .client
            .try_lock()
            .expect("recovery close not started");
        client.deadline = Some(
            client
                .deadline
                .map_or(cleanup_end, |end| end.min(cleanup_end)),
        );
    }
    // AsyncChild already owns the maintained process-group registration and Drop
    // drain. Local observation uses the same owner's remaining interval; global
    // cancellation drops that guard and retains its unresolved group if needed.
    if let Some(mut process) = child {
        let _ = process.start_kill();
        if matches!(
            tokio::time::timeout_at(cleanup_end.into(), process.wait()).await,
            Ok(Ok(_))
        ) {
            cleanup.facts.lock().expect("recovery cleanup facts").child =
                ChildSettlement::ObservedExit;
        }
    }
    let mcp_result = cleanup.close().await;
    cleanup.record().await?;
    if mcp_result.is_ok() {
        cleanup_registration.settled()?;
    }
    let child_cleanup = cleanup.facts.lock().expect("recovery cleanup facts").child;
    let mcp_cleanup = cleanup.client.lock().await.status;
    journal.append(&Record::Outcome {
        operation_passed: outcome.is_ok(),
        child_cleanup,
        mcp_cleanup,
        capability_expires_at: expires_at,
        requested_retention_expires_at: expires_at,
        reconciliation: if outcome.is_ok() {
            Reconciliation::OneOccurrenceVerifiedNoDeletionApi
        } else {
            Reconciliation::UnresolvedInspectIntentNoMutationRetry
        },
    })?;
    ensure!(
        outcome.is_ok(),
        "Artifact service recovery failed; retained private journal: {}",
        path.display()
    );
    ensure!(
        matches!(
            child_cleanup,
            ChildSettlement::NotSpawned | ChildSettlement::ObservedExit
        ) && mcp_result.is_ok(),
        "recovery client cleanup failed; see private journal"
    );
    println!(
        "Artifact service recovery passed. Private journal: {}",
        path.display()
    );
    Ok(())
}

const CONSUMER: &str = r#"
import asyncio, hashlib, json, sys
from pydantic import BaseModel, ConfigDict
from veoveo_mcp.artifacts import HttpArtifactPlane
from veoveo_mcp.contract.identity import PlaneCaller, GatewayInternalIdentity
from veoveo_mcp.contract.artifacts import (IssueArtifactWriteCapabilityRequest,
    PutArtifactRequest, RedeemArtifactWriteCapabilityRequest, ArtifactWriteIdempotencyKey, ListArtifactsRequest)

class CallerInput(BaseModel):
    model_config = ConfigDict(extra='forbid', strict=True)
    bearerToken: str
    identity: GatewayInternalIdentity

class Input(BaseModel):
    model_config = ConfigDict(extra='forbid', strict=True)
    artifactServiceUrl: str
    caller: CallerInput
    issue: IssueArtifactWriteCapabilityRequest
    artifact: PutArtifactRequest
    idempotencyKey: ArtifactWriteIdempotencyKey

def emit(value):
    print(json.dumps(value), flush=True)

async def line():
    return await asyncio.to_thread(sys.stdin.buffer.readline)

async def catalog(plane, caller):
    pages, cursor = [], None
    async with asyncio.timeout(60):
        for _ in range(256):
            page = await plane.list(caller, ListArtifactsRequest(limit=1, cursor=cursor))
            pages.append(page.model_dump(mode='json'))
            if page.next_cursor is None:
                break
            cursor = page.next_cursor
    return pages

async def main():
    data = Input.model_validate_json(await line())
    caller = PlaneCaller.from_identity(data.caller.identity, data.caller.bearerToken)
    plane = HttpArtifactPlane(data.artifactServiceUrl)
    try:
        async with asyncio.timeout(290):
            pages = await catalog(plane, caller)
            issued = await plane.issue_write_capability(caller, data.issue)
            request = RedeemArtifactWriteCapabilityRequest(capabilityId=issued.capability_id,
                taskId=data.issue.task_id, idempotencyKey=data.idempotencyKey, artifact=data.artifact)
            emit({'phase':'issued', 'capabilityId':issued.capability_id, 'taskId':issued.task_id,
                'expiresAt':issued.expires_at.wire, 'pages':pages})
            for expected, phase in [(b'redeem\n','redeemed'), (b'replay\n','replayed')]:
                if await line() != expected:
                    raise ValueError('invalid recovery phase')
                metadata = await plane.redeem_write_capability(issued.secret, request,
                    b'artifact-service retained capability fixture\n')
                head = await plane.head(caller, metadata.artifact_id)
                resolved = await plane.resolve(caller, metadata.artifact_uri, max_bytes=1024)
                emit({'phase':phase, 'observation':{'metadata':metadata.model_dump(mode='json'),
                    'head':head.model_dump(mode='json'), 'resolved':resolved.metadata.model_dump(mode='json'),
                    'sha256':hashlib.sha256(resolved.bytes_).hexdigest(), 'pages':await catalog(plane, caller)}})
    finally:
        await plane.close()

if __name__ == '__main__':
    try:
        asyncio.run(main())
    except Exception:
        # No exception text or traceback: SDK errors may contain bearer material.
        emit({'phase':'failed'})
        sys.exit(1)
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn global_owner_cancellation_closes_retained_client_and_drains_child() -> Result<()> {
        const MODE: &str = "VEOVEO_ARTIFACT_RECOVERY_CLEANUP_CONTROL";
        const ROOT: &str = "VEOVEO_ARTIFACT_RECOVERY_CLEANUP_ROOT";
        if let Ok(mode) = std::env::var(MODE) {
            use rmcp::{ClientServiceExt, ServiceExt};
            use std::sync::atomic::{AtomicUsize, Ordering};
            let root = std::path::PathBuf::from(std::env::var_os(ROOT).context("control root")?);
            let journal = Journal(Arc::new(Mutex::new(
                fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(root.join("journal.jsonl"))?,
            )));
            let attempts = Arc::new(AtomicUsize::new(0));
            let counted = Arc::clone(&attempts);
            let observed = journal.clone();
            let mode_for_operation = mode.clone();
            let (server_closed_tx, server_closed_rx) = tokio::sync::oneshot::channel();
            let result: Result<()> = owner::run(async {
                let (cleanup, _registration) =
                    RetainedCleanup::register(observed, "native-close-control")?;
                let (server_io, client_io) = tokio::io::duplex(8192);
                struct Source;
                impl rmcp::ServerHandler for Source {}
                let server = tokio::spawn(async move { Source.serve(server_io).await });
                let client = ()
                    .serve_with_lifecycle(
                        client_io,
                        rmcp::ClientLifecycleMode::Discover {
                            preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
                        },
                    )
                    .await?;
                let server = server.await??;
                tokio::spawn(async move {
                    let _ = server_closed_tx.send(server.waiting().await.is_ok());
                });
                let interrupted = mode_for_operation == "interrupted";
                let failed = mode_for_operation == "failed";
                cleanup.retain(async move {
                    counted.fetch_add(1, Ordering::SeqCst);
                    if interrupted {
                        tokio::time::sleep(Duration::from_millis(250)).await;
                    }
                    client.cancel().await?;
                    ensure!(!failed, "controlled missing close acknowledgement");
                    Ok(())
                });
                if failed {
                    ensure!(
                        cleanup.close().await.is_err(),
                        "controlled close failure disappeared"
                    );
                    ensure!(
                        cleanup.close().await.is_err(),
                        "consumed client falsely settled on retry"
                    );
                    return Ok(());
                }
                let mut command = tokio::process::Command::new("sh");
                command
                    .args([
                        "-c",
                        "echo $$ > \"$1\"; sleep 0.1; kill -INT \"$2\"; exec sleep 30",
                        "fixture",
                    ])
                    .arg(root.join("child.pid"))
                    .arg(std::process::id().to_string())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                let _child = veoveo_testing_support::spawn_async(command)?;
                cleanup.facts.lock().expect("cleanup facts").child =
                    ChildSettlement::OwnedGroupRegistration;
                if interrupted {
                    // Global cancellation interrupts this await; owner cleanup
                    // must resume the original retained future, not recreate it.
                    cleanup.close().await?;
                }
                std::future::pending::<Result<()>>().await
            })
            .await;
            ensure!(
                result.is_err(),
                "owner cancellation/failed close became successful operation"
            );
            ensure!(
                attempts.load(Ordering::SeqCst) == 1,
                "client close was retried or never polled"
            );
            ensure!(
                tokio::time::timeout(Duration::from_secs(1), server_closed_rx).await??,
                "actual MCP transport remained open after cleanup"
            );
            let records = fs::read_to_string(root.join("journal.jsonl"))?;
            let last: Value =
                serde_json::from_str(records.lines().last().context("missing cleanup report")?)?;
            ensure!(
                last["phase"] == "ownerCleanup",
                "owner did not journal retained cleanup"
            );
            if mode == "failed" {
                ensure!(
                    last["mcp"] == "failed",
                    "failed close was converted to empty-slot success"
                );
            } else {
                ensure!(
                    last["mcp"] == "passed"
                        && last["operation"] == "unfinished"
                        && last["ownerCancelled"] == true,
                    "dropped operation was reported as completed"
                );
                let pid = fs::read_to_string(root.join("child.pid"))?;
                ensure!(
                    !Path::new("/proc").join(pid.trim()).exists(),
                    "actual owned child survived cancellation"
                );
                ensure!(
                    !root.join("groups").join(pid.trim()).exists(),
                    "actual child group ownership lease remained unsettled"
                );
            }
            return Ok(());
        }
        for mode in ["global", "interrupted", "failed"] {
            let directory = tempfile::tempdir()?;
            fs::create_dir(directory.path().join("groups"))?;
            let mut command = tokio::process::Command::new(std::env::current_exe()?);
            command
                .args([
                    "global_owner_cancellation_closes_retained_client_and_drains_child",
                    "--nocapture",
                ])
                .env(MODE, mode)
                .env(ROOT, directory.path())
                .env("VEOVEO_SMOKE_LOCAL_GROUPS", directory.path().join("groups"))
                .env("VEOVEO_SMOKE_CLEANUP_SECONDS", "2");
            let output =
                veoveo_testing_support::output_async(command, Duration::from_secs(15)).await?;
            ensure!(
                output.status.success(),
                "actual owner cleanup control {mode} failed: {} {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(())
    }

    #[test]
    fn recovery_preflight_refuses_inherited_policy_outside_fixture_clearance() -> Result<()> {
        let mut policy = veoveo_types::WorkContextOutputPolicy {
            owner: veoveo_types::AccessSubject::Principal(veoveo_types::PrincipalId::parse(
                "issuer#fixture",
            )?),
            initial_grants: Vec::new(),
            classification: None,
            data_labels: BTreeSet::new(),
        };
        admit_output_policy(&policy)?;
        policy.classification = Some(veoveo_types::DataLabelId::parse("internal")?);
        ensure!(
            admit_output_policy(&policy).is_err(),
            "classification bypassed fixture clearance"
        );
        policy.classification = None;
        policy
            .data_labels
            .insert(veoveo_types::DataLabelId::parse("internal")?);
        ensure!(
            admit_output_policy(&policy).is_err(),
            "inherited label bypassed fixture clearance"
        );
        Ok(())
    }

    #[tokio::test]
    async fn retained_capability_python_peer_preserves_requests_and_redacts_observations()
    -> Result<()> {
        let interpreter = std::env::var_os("VEOVEO_ARTIFACT_CONSUMER_PYTHON")
            .context("recovery peer control requires the qualified SDK Python interpreter")?;
        let task = ArtifactTaskId::new();
        let capability = ArtifactWriteCapabilityId::new();
        let metadata: ArtifactMetadata = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../platform/artifacts/contract/tests/fixtures/metadata-output.json"
        )))?;
        let issue = IssueArtifactWriteCapabilityRequest {
            task_id: task,
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(10),
            max_artifact_count: NonZeroU32::new(1).unwrap(),
            max_total_bytes: NonZeroU64::new(BYTES.len() as u64).unwrap(),
            required_data_labels: BTreeSet::new(),
        };
        // This checks the actual child/SDK transport and phase protocol offline.
        // A mocked service epoch cannot qualify Kubernetes or service recovery.
        let source = format!("__name__ = 'recovery_control'\n{CONSUMER}\n{CONTROL}");
        let mut command = tokio::process::Command::new(interpreter);
        command.args([
            "-c",
            &source,
            &serde_json::to_string(&issue)?,
            &capability.to_string(),
            &serde_json::to_string(&metadata)?,
        ]);
        let output = veoveo_testing_support::output_async(command, Duration::from_secs(30)).await?;
        ensure!(
            output.status.success(),
            "offline recovery SDK peer control failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let observations: Vec<Event> = serde_json::from_slice(&output.stdout)?;
        ensure!(
            matches!(observations.as_slice(), [Event::Issued { task_id, capability_id, .. }, Event::Redeemed { .. }, Event::Replayed { .. }]
            if *task_id == task && *capability_id == capability),
            "recovery SDK phase correlation changed"
        );
        ensure!(
            !String::from_utf8_lossy(&output.stdout).contains("private-capability-secret"),
            "recovery SDK emitted the capability bearer"
        );
        Ok(())
    }

    const CONTROL: &str = r#"
import base64, httpx
from datetime import datetime, timezone
from veoveo_mcp.contract.identity import (Principal, PrincipalKind, InvocationAuthority,
    WorkContextMembershipLevel, WorkContextOutputPolicy, PrincipalAccessSubject, DirectInvocationProvenance)

issue, capability, metadata = json.loads(sys.argv[1]), sys.argv[2], json.loads(sys.argv[3])
body = b'artifact-service retained capability fixture\n'
metadata['byteLen'] = len(body)
metadata.pop('downloadUrl', None)
secret = 'private-capability-secret-000000000000'
now = datetime.now(timezone.utc)
identity = GatewayInternalIdentity(issuer='veoveo-internal', profile='fixture', server='datasheet',
    jwt_id='test', issued_at=now, not_before=now, expires_at=now,
    actor=Principal(id='alice', kind=PrincipalKind.USER, issuer='https://idp.example', subject='alice', tenant='tenant'),
    authority=InvocationAuthority(work_context='operations', tenant='tenant',
        membership=WorkContextMembershipLevel.CONTRIBUTOR, policy_revision='p1',
        output_policy=WorkContextOutputPolicy(owner=PrincipalAccessSubject(kind='principal',id='alice'),initial_grants=(),data_labels=frozenset()),
        provenance=DirectInvocationProvenance(mode='direct',initiator='alice')))
input_value = {'artifactServiceUrl':'https://plane.example',
    'caller':{'bearerToken':'fixture-forwarded-identity','identity':identity.model_dump(mode='json')},
    'issue':issue,'artifact':{'filename':'fixture.txt'},'idempotencyKey':'same-write'}
events, redeems = [], []
epoch, issued = 0, False

class Body(httpx.AsyncByteStream):
    async def __aiter__(self):
        yield body

def respond(request):
    global issued
    if request.url.path == '/artifact-write-capabilities':
        assert epoch == 0 and not issued
        assert json.loads(request.content) == IssueArtifactWriteCapabilityRequest.model_validate(issue).model_dump(mode='json')
        assert request.headers['authorization'] == 'Bearer fixture-forwarded-identity'
        issued = True
        return httpx.Response(200,json={'capabilityId':capability,'taskId':issue['taskId'],
            'expiresAt':issue['expiresAt'],'secret':secret})
    if request.url.path.endswith('/redeem'):
        assert issued and epoch == 1 and request.content == body
        assert request.headers['authorization'] == f'Bearer {secret}'
        actual = json.loads(request.headers['x-artifact-capability-redeem'])
        assert actual == {'capabilityId':capability,'taskId':issue['taskId'],
            'idempotencyKey':'same-write','artifact':{'filename':'fixture.txt'}}
        redeems.append(actual)
        return httpx.Response(200,json=metadata)
    if request.url.path == '/artifacts':
        assert dict(request.url.params) == {'limit':'1'}
        return httpx.Response(200,json={'artifacts':[metadata] if redeems else []})
    if request.url.path.endswith('/meta'):
        return httpx.Response(200,json=metadata)
    assert request.url.path == '/resolve'
    return httpx.Response(200,stream=Body(),headers={
        'content-length':str(len(body)),
        'x-artifact-metadata':base64.b64encode(json.dumps(metadata).encode()).decode()})

original_plane = HttpArtifactPlane
HttpArtifactPlane = lambda origin: original_plane(origin,httpx.AsyncClient(transport=httpx.MockTransport(respond)))
commands = iter([json.dumps(input_value).encode(),b'redeem\n',b'replay\n'])
async def line():
    global epoch
    result = next(commands)
    if result == b'redeem\n':
        assert len(events) == 1 and issued
        epoch = 1
    elif result == b'replay\n':
        assert len(events) == 2 and len(redeems) == 1
    return result
emit = events.append
asyncio.run(main())
assert len(redeems) == 2 and redeems[0] == redeems[1]
assert events[1]['observation'] == events[2]['observation']
print(json.dumps(events))
"#;
}
