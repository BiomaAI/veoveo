//! Opt-in raw installed Speech Tasks in the existing owner Task harness.
#[path = "installed_assertions.rs"]
mod assertions;
#[path = "installed_cleanup.rs"]
mod cleanup;
#[path = "installed_recovery.rs"]
mod recovery;
use anyhow::{Context, Result, ensure};
use cleanup::Handles;
use rmcp::model::{
    CancelTaskParams, DetailedTask, GetMeta, GetTaskParams, ReadResourceRequestParams,
    ResourceContents, ServerNotification, SubscriptionFilter, Task, TaskStatus,
};
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::Path, sync::Arc, time::Duration};
use tokio::sync::Mutex;
use veoveo_speech_contract::{TranscribeRequest, TranscriptionOutput};
use veoveo_testing_support::{
    SmokeMcpClient, await_task_terminal_with_timeout, call_tool_as_task, installed::knowledge,
    lifecycle::owner,
};
use veoveo_types::{CanonicalTaskId, ResourceUri};

#[derive(Clone, Copy, Debug, PartialEq, Eq, veoveo_types::Vocabulary)]
enum Mode {
    Complete,
    Cancel,
    Recover,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, veoveo_types::Vocabulary)]
enum Phase {
    Admitted,
    Connecting,
    DispatchIntent,
    Created,
    Listening,
    BeforeCancel,
    CancelIntent,
    CancelAcknowledged,
    Delivery,
    Current,
    Output,
    ResourceIntent,
    ResourceListening,
    ResourceDelivery,
    RecoveryAdmission,
    RecoveryWorking,
    RecoveryArmed,
    RecoveryReplacement,
    RecoveryListening,
    RecoveryFence,
    Passed,
    Failed,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    mode: Mode,
    installation: knowledge::InstalledSource,
    request: TranscribeRequest,
    expected: assertions::Expected,
    #[serde(default)]
    recovery: Option<recovery::Fixture>,
}
impl Input {
    fn admit(&self, mode: Mode) -> Result<()> {
        ensure!(self.mode == mode, "Speech fixture profile differs");
        self.request.source()?;
        ensure!(
            self.request.artifact_uri == self.request.source()?.plane_uri(),
            "Speech fixture requires a neutral governed source Artifact"
        );
        self.expected.admit()?;
        recovery::admit_pairing(
            self.mode,
            &self.installation.deployment,
            self.recovery.as_ref(),
        )?;
        let endpoint = reqwest::Url::parse(self.installation.endpoint.as_str())?;
        ensure!(
            endpoint
                .path_segments()
                .is_some_and(|mut parts| parts.next() == Some("mcp")
                    && parts.next().is_some_and(|profile| !profile.is_empty())
                    && parts.next().is_none())
                && endpoint.query().is_none()
                && endpoint.fragment().is_none(),
            "Speech fixture must use a public Gateway profile MCP endpoint"
        );
        Ok(())
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Observation {
    task_id: CanonicalTaskId,
    status: TaskStatus,
    created_at: chrono::DateTime<chrono::Utc>,
    last_updated_at: chrono::DateTime<chrono::Utc>,
}
impl Observation {
    fn admit(task: &Task) -> Result<Self> {
        Ok(Self {
            task_id: CanonicalTaskId::parse(&task.task_id)?,
            status: task.status,
            created_at: task.created_at.parse()?,
            last_updated_at: task.last_updated_at.parse()?,
        })
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Journal<'a> {
    format: &'static str,
    mode: Mode,
    request: &'a TranscribeRequest,
    expected: &'a assertions::Expected,
    phase: Phase,
    dispatch_intent: bool,
    cancel_intent: bool,
    cancel_acknowledged: bool,
    task_id: Option<CanonicalTaskId>,
    created: Option<Observation>,
    before_cancel: Option<Observation>,
    delivered: Vec<Observation>,
    first_delivered_status: Option<TaskStatus>,
    working_delivered: bool,
    current: Option<Observation>,
    result: Option<TranscriptionOutput>,
    output_checked: bool,
    resource_uri: Option<veoveo_speech_contract::TranscriptionUri>,
    resource_subscription_id: Option<rmcp::model::RequestId>,
    resource_snapshot_delivered: bool,
    resource_current_checked: bool,
    resource_listener_closed: bool,
    resource_listener_close_failed: bool,
    terminal_settled: bool,
    listener_closed: bool,
    caller_closed: bool,
    listener_close_failed: bool,
    caller_close_failed: bool,
    failure: bool,
    failure_phase: Option<Phase>,
    mcp_error_code: Option<i32>,
    http_status: Option<u16>,
    active_resource: Option<ResourceUri>,
    recovery: recovery::Evidence,
    remaining_gates: Vec<&'static str>,
}
impl<'a> Journal<'a> {
    fn new(input: &'a Input) -> Self {
        Self {
            format: "veoveo.ai/speech-installed-task/v1",
            mode: input.mode,
            request: &input.request,
            expected: &input.expected,
            phase: Phase::Admitted,
            dispatch_intent: false,
            cancel_intent: false,
            cancel_acknowledged: false,
            task_id: None,
            created: None,
            before_cancel: None,
            delivered: vec![],
            first_delivered_status: None,
            working_delivered: false,
            current: None,
            result: None,
            output_checked: false,
            resource_uri: None,
            resource_subscription_id: None,
            resource_snapshot_delivered: false,
            resource_current_checked: false,
            resource_listener_closed: true,
            resource_listener_close_failed: false,
            terminal_settled: false,
            listener_closed: true,
            caller_closed: true,
            listener_close_failed: false,
            caller_close_failed: false,
            failure: false,
            failure_phase: None,
            mcp_error_code: None,
            http_status: None,
            active_resource: None,
            recovery: recovery::Evidence::default(),
            remaining_gates: vec![
                "unfinished_process_restart_recovery",
                "guaranteed_working_to_completed_delivery",
                "performance_and_concurrent_capacity",
            ],
        }
    }
    fn persist(&self, file: &mut fs::File) -> Result<()> {
        serde_json::to_writer(&mut *file, self)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        Ok(())
    }
    fn error(&mut self, error: &anyhow::Error) {
        self.failure = true;
        self.failure_phase = Some(self.phase);
        // Only typed status/code cross this boundary; raw SDK/provider diagnostics stay private in memory.
        for cause in error.chain() {
            if let Some(rmcp::ServiceError::McpError(error)) =
                cause.downcast_ref::<rmcp::ServiceError>()
            {
                self.mcp_error_code = Some(error.code.0);
            }
            if let Some(error) = cause.downcast_ref::<reqwest::Error>() {
                self.http_status = error.status().map(|value| value.as_u16());
            }
        }
    }
}
fn receipt(path: &Path) -> Result<fs::File> {
    ensure!(path.is_absolute(), "Speech receipt path must be absolute");
    let parent = path.parent().context("Speech receipt has no parent")?;
    let metadata = fs::metadata(parent)?;
    ensure!(
        metadata.is_dir(),
        "Speech receipt parent is not a directory"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
        ensure!(
            metadata.permissions().mode() & 0o077 == 0,
            "Speech receipt parent must be private"
        );
        ensure!(
            metadata.uid() == fs::metadata("/proc/self")?.uid(),
            "Speech receipt parent must belong to the caller"
        );
        Ok(fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(path)?)
    }
    #[cfg(not(unix))]
    {
        Ok(fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)?)
    }
}

#[tokio::test]
#[ignore = "requires installed Speech CUDA, normal Task-enabled OAuth, selected governed recording, independent expectations and a new private receipt"]
async fn raw_transcription_completion_through_public_gateway() -> Result<()> {
    run(Mode::Complete).await
}
#[tokio::test]
#[ignore = "requires installed Speech CUDA, normal OAuth and a genuine recording that remains unfinished at the cancellation fence"]
async fn raw_transcription_cancellation_through_public_gateway() -> Result<()> {
    run(Mode::Cancel).await
}

#[tokio::test]
#[ignore = "requires installed Speech CUDA, OAuth, exact crash target and an externally signalled process replacement while this Task stays Working"]
async fn raw_transcription_recovery_through_public_gateway() -> Result<()> {
    run(Mode::Recover).await
}

async fn run(mode: Mode) -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let input: Input = knowledge::input_from("VEOVEO_SPEECH_TASK_INPUT")
        .map_err(|_| anyhow::anyhow!("Speech fixture input admission failed"))?;
    input
        .admit(mode)
        .map_err(|_| anyhow::anyhow!("Speech fixture preconditions failed"))?;
    let target = input
        .installation
        .validate()
        .map_err(|_| anyhow::anyhow!("Speech installation admission failed"))?;
    ensure!(
        target
            .expected_deployments
            .contains(&input.installation.deployment),
        "Speech fixture deployment is not declared by the installation"
    );
    // Admit private credentials locally before creating the intent journal; no external auth calls.
    knowledge::bearer_header(&input.installation.caller_token_file)
        .map_err(|_| anyhow::anyhow!("Speech private token-file admission failed"))?;
    let mut file = receipt(&input.installation.output)
        .map_err(|_| anyhow::anyhow!("Speech private receipt admission failed"))?;
    let mut journal = Journal::new(&input);
    journal
        .persist(&mut file)
        .map_err(|_| anyhow::anyhow!("Speech receipt persistence failed"))?;
    let handles = Arc::new(Mutex::new(Handles::default()));
    let result = owner::run(async {
        let closing = handles.clone();
        owner::register_cleanup(
            owner::CleanupKind::Remote,
            "Speech SDK handles",
            &uuid::Uuid::now_v7().to_string(),
            move || async move { closing.lock().await.close().await },
        )?;
        journal.phase = Phase::Connecting;
        journal.persist(&mut file)?;
        // Acquire ownership before connection: there is no await between success and retain.
        let mut retained = handles.lock().await;
        let caller =
            tokio::time::timeout(Duration::from_secs(60), input.installation.task_caller())
                .await
                .context("Speech connection deadline")??;
        retained.caller.retain(caller);
        tokio::time::timeout(
            Duration::from_secs(if mode == Mode::Recover { 1350 } else { 180 }),
            exercise(&input, &mut retained, &mut journal, &mut file),
        )
        .await
        .context("Speech Task operation deadline")?
    })
    .await;
    if let Err(error) = &result {
        journal.error(error);
    }
    // Owner actions finish after work is dropped, inside the same captured grace.
    let retained = handles.lock().await;
    journal.recovery.watch = retained.crash_receipt.clone();
    journal.recovery.watch_closed = retained.crash_watch.closed;
    journal.recovery.watch_close_failed = retained.crash_watch.failed;
    journal.recovery.listener_closed = retained.recovery_listener.closed;
    journal.recovery.listener_close_failed = retained.recovery_listener.failed;
    journal.caller_closed = retained.caller.closed;
    journal.listener_closed = retained.listener.closed;
    journal.resource_listener_closed = retained.resource_listener.closed;
    journal.resource_listener_close_failed = retained.resource_listener.failed;
    journal.caller_close_failed = retained.caller.failed;
    journal.listener_close_failed = retained.listener.failed;
    journal.failure |= result.is_err()
        || !journal.recovery.watch_closed
        || journal.recovery.watch_close_failed
        || !journal.recovery.listener_closed
        || journal.recovery.listener_close_failed
        || !journal.caller_closed
        || !journal.resource_listener_closed
        || journal.resource_listener_close_failed
        || !journal.listener_closed
        || journal.caller_close_failed
        || journal.listener_close_failed;
    journal.failure |= !journal.terminal_settled
        || match input.mode {
            Mode::Complete | Mode::Recover => {
                !journal.output_checked
                    || !journal.resource_snapshot_delivered
                    || !journal.resource_current_checked
                    || (input.mode == Mode::Recover && !journal.recovery.final_target_checked)
            }
            Mode::Cancel => !journal.cancel_intent || !journal.cancel_acknowledged,
        };
    if !journal.failure && input.mode == Mode::Recover {
        journal
            .remaining_gates
            .retain(|gate| *gate != "unfinished_process_restart_recovery");
    }
    journal.phase = if journal.failure {
        Phase::Failed
    } else {
        Phase::Passed
    };
    journal
        .persist(&mut file)
        .map_err(|_| anyhow::anyhow!("Speech final receipt persistence failed"))?;
    ensure!(
        !journal.failure,
        "Speech installed Task acceptance failed; inspect the private journal and ownership lease"
    );
    Ok(())
}

async fn exercise(
    input: &Input,
    handles: &mut Handles,
    journal: &mut Journal<'_>,
    file: &mut fs::File,
) -> Result<()> {
    let caller = handles
        .caller
        .handle
        .as_ref()
        .context("Speech caller missing")?;
    ensure!(
        caller
            .peer_info()
            .is_some_and(|info| info.capabilities.supports_tasks()),
        "Speech Gateway does not advertise Tasks"
    );
    let driver = recovery::admit(input, caller, journal, file).await?;
    journal.phase = Phase::DispatchIntent;
    journal.dispatch_intent = true;
    journal.persist(file)?;
    let created = call_tool_as_task(
        caller,
        "speech__transcribe",
        serde_json::to_value(&input.request)?,
    )
    .await?;
    journal.task_id = Some(CanonicalTaskId::parse(&created.task_id)?);
    journal.phase = Phase::Created;
    journal.persist(file)?;
    journal.created = Some(Observation::admit(&created)?);
    journal.persist(file)?;
    let id = journal.task_id.as_ref().unwrap().clone();
    let filter = SubscriptionFilter::builder()
        .task_id(id.to_string())
        .build();
    handles
        .listener
        .retain(caller.listen(filter.clone()).await?);
    ensure!(
        handles.listener.handle.as_ref().unwrap().acknowledged() == &filter,
        "Speech exact Task subscription differs"
    );
    journal.phase = Phase::Listening;
    journal.persist(file)?;
    if input.mode == Mode::Cancel {
        journal.phase = Phase::BeforeCancel;
        journal.persist(file)?;
        let current = caller.get_task(GetTaskParams::new(id.as_str())).await?.task;
        assertions::same(&id, &created, &current)?;
        journal.before_cancel = Some(Observation::admit(&current.task)?);
        journal.persist(file)?;
        assertions::unfinished(&current)?;
        journal.phase = Phase::CancelIntent;
        journal.cancel_intent = true;
        journal.persist(file)?;
        owner::check_effect()?;
        caller
            .cancel_task(CancelTaskParams::new(id.as_str()))
            .await?;
        journal.cancel_acknowledged = true;
        journal.phase = Phase::CancelAcknowledged;
        journal.persist(file)?;
    }
    if let Some(driver) = &driver {
        recovery::observe(input, driver, &created, handles, journal, file).await?;
    }
    let caller = handles
        .caller
        .handle
        .as_ref()
        .context("Speech caller missing")?;
    let listener = if input.mode == Mode::Recover {
        handles.recovery_listener.handle.as_mut().unwrap()
    } else {
        handles.listener.handle.as_mut().unwrap()
    };
    journal.phase = Phase::Delivery;
    journal.persist(file)?;
    let delivered = tokio::time::timeout(
        Duration::from_secs(if input.mode == Mode::Recover {
            900
        } else {
            120
        }),
        async {
            for _ in 0..128 {
                let notification = listener
                    .next()
                    .await?
                    .context("Speech Task subscription ended")?;
                ensure!(
                    notification.get_meta().subscription_id() == Some(listener.id().clone()),
                    "Speech delivery subscription identity differs"
                );
                let ServerNotification::TaskStatusNotification(update) = notification else {
                    anyhow::bail!("Speech exact Task filter delivered a different notification");
                };
                let task = update.params.task;
                assertions::same(&id, &created, &task)?;
                journal.first_delivered_status.get_or_insert(task.status());
                journal.working_delivered |= task.status() == TaskStatus::Working;
                journal.delivered.push(Observation::admit(&task.task)?);
                journal.persist(file)?;
                if !matches!(
                    task.status(),
                    TaskStatus::Working | TaskStatus::InputRequired
                ) {
                    return Ok(task);
                }
            }
            anyhow::bail!("Speech Task delivery exceeded 128 observations")
        },
    )
    .await
    .context("Speech terminal delivery deadline")??;
    journal.phase = Phase::Current;
    journal.persist(file)?;
    let current =
        await_task_terminal_with_timeout(caller, id.as_str(), Duration::from_secs(10)).await?;
    journal.terminal_settled = true;
    journal.current = Some(Observation::admit(&current.task)?);
    journal.persist(file)?;
    assertions::terminal(input.mode, &id, &created, &delivered, &current)?;
    if matches!(input.mode, Mode::Complete | Mode::Recover) {
        journal.phase = Phase::Output;
        journal.persist(file)?;
        let output = assertions::output(&current)?;
        journal.result = Some(output.clone());
        journal.persist(file)?;
        // Persist the admitted domain identity before opening its listener. The Gateway
        // Task ID is opaque and is never used to construct this owner address.
        journal.resource_uri = Some(output.result_uri);
        journal.phase = Phase::ResourceIntent;
        journal.persist(file)?;
        let filter = SubscriptionFilter::builder()
            .resource_subscription(output.result_uri.to_uri().as_str())
            .build();
        handles.resource_listener.retain(
            tokio::time::timeout(Duration::from_secs(15), caller.listen(filter.clone()))
                .await
                .context("Speech resource subscription deadline")??,
        );
        let listener = handles.resource_listener.handle.as_mut().unwrap();
        ensure!(
            listener.acknowledged() == &filter,
            "Speech exact resource subscription differs"
        );
        journal.resource_subscription_id = Some(listener.id().clone());
        journal.phase = Phase::ResourceListening;
        journal.persist(file)?;
        let notification = tokio::time::timeout(Duration::from_secs(15), listener.next())
            .await
            .context("Speech current resource delivery deadline")??
            .context("Speech resource listener ended before current snapshot")?;
        assertions::resource_delivery(&notification, listener.id(), &output.result_uri)?;
        journal.resource_snapshot_delivered = true;
        journal.phase = Phase::ResourceDelivery;
        journal.persist(file)?;
        // A delivered invalidation carries no transcript. Re-read the owner resource
        // and all governed outputs to establish agreement with the completed Task.
        assertions::check_output(caller, input, &output, journal, file).await?;
        journal.resource_current_checked = true;
        journal.output_checked = true;
        journal.persist(file)?;
    }
    if input.mode == Mode::Recover {
        journal.phase = Phase::RecoveryFence;
        journal.persist(file)?;
        let watch = handles
            .crash_watch
            .handle
            .as_mut()
            .context("Speech crash watch missing")?;
        let checked = watch.admit_recovered_target().await;
        journal.recovery.watch = watch.snapshot();
        handles.crash_receipt = journal.recovery.watch.clone();
        journal.persist(file)?;
        checked?;
        journal.recovery.final_target_checked = true;
        journal.persist(file)?;
    }
    Ok(())
}

async fn text(caller: &SmokeMcpClient, uri: &ResourceUri, mime: &str) -> Result<String> {
    let reply = caller
        .read_resource(ReadResourceRequestParams::new(uri.as_str()))
        .await?;
    let [
        ResourceContents::TextResourceContents {
            uri: actual,
            text,
            mime_type,
            ..
        },
    ] = reply.contents.as_slice()
    else {
        anyhow::bail!("Speech output resource must contain one text body");
    };
    ensure!(
        actual == uri.as_str() && mime_type.as_deref() == Some(mime) && text.len() <= 512 * 1024,
        "Speech output resource identity/MIME/size differs"
    );
    Ok(text.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn input() -> Result<Input> {
        let source = veoveo_artifact_contract::ArtifactId::new();
        Ok(serde_json::from_value(serde_json::json!({
            "mode":"complete",
            "installation":{"installationTarget":"/private/installation.json","endpoint":"https://installation.test/mcp/operator", "callerTokenFile":"/private/token", "deployment":"selected-speech-gpu-worker", "output":"/private/receipt.jsonl"},
            "request":{"artifactUri":source.plane_uri()},
            "expected":{"sourceSha256":"a".repeat(64),"text":"independent fixture text", "model":"fixture-model", "modelRevision":"fixture-revision", "minimumDurationSeconds":1,"maximumDurationSeconds":2,
                "compliance":{"owner":{"kind":"principal","id":"https://installation.test#operator"},"tenantId":"fixture-tenant", "workContext":"fixture-context", "provenance":{"invocationMode":"direct", "producer":"https://installation.test#operator","initiator":"https://installation.test#operator","policyRevision":"fixture-policy"}}}
        }))?)
    }
    #[test]
    fn fixture_admission_rejects_wrong_mode_source_and_endpoint() -> Result<()> {
        let mut fixture = input()?;
        fixture.admit(Mode::Complete)?;
        ensure!(fixture.admit(Mode::Cancel).is_err());
        fixture.installation.endpoint =
            veoveo_types::HttpsUrl::parse("https://installation.test/speech/mcp")?;
        ensure!(fixture.admit(Mode::Complete).is_err());
        fixture.installation.endpoint =
            veoveo_types::HttpsUrl::parse("https://installation.test/mcp/operator")?;
        fixture.request.artifact_uri = veoveo_artifact_contract::ArtifactUri::presented(
            &veoveo_speech_contract::ARTIFACT_SCHEME,
            fixture.request.source()?,
        );
        ensure!(fixture.admit(Mode::Complete).is_err());
        let value =
            serde_json::json!({"mode":"restart","installation":{},"request":{},"expected":{}});
        ensure!(serde_json::from_value::<Input>(value).is_err());
        Ok(())
    }
    #[test]
    fn private_journal_retains_intent_identity_and_sanitizes_error() -> Result<()> {
        let directory = tempfile::tempdir()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700))?;
        }
        let path = directory.path().join("receipt.jsonl");
        let mut file = receipt(&path)?;
        let fixture = input()?;
        let mut journal = Journal::new(&fixture);
        journal.phase = Phase::DispatchIntent;
        journal.dispatch_intent = true;
        journal.persist(&mut file)?;
        let id = CanonicalTaskId::parse("speech.opaque-fixture")?;
        journal.task_id = Some(id.clone());
        journal.phase = Phase::Created;
        journal.persist(&mut file)?;
        journal.error(&anyhow::anyhow!(
            "synthetic-credential-bearing-provider-error"
        ));
        journal.phase = Phase::Failed;
        journal.persist(&mut file)?;
        journal.resource_uri = Some(veoveo_speech_contract::TranscriptionUri::new(
            veoveo_speech_contract::TranscriptionId::new(),
        ));
        journal.phase = Phase::ResourceIntent;
        journal.persist(&mut file)?;
        journal.resource_subscription_id = Some(rmcp::model::RequestId::Number(7));
        journal.phase = Phase::ResourceListening;
        journal.persist(&mut file)?;
        journal.resource_snapshot_delivered = true;
        journal.phase = Phase::ResourceDelivery;
        journal.persist(&mut file)?;
        let bytes = fs::read_to_string(&path)?;
        let rows = bytes
            .lines()
            .map(serde_json::from_str::<serde_json::Value>)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ensure!(
            rows.len() == 6 && rows[0]["dispatchIntent"] == true && rows[0]["taskId"].is_null()
        );
        ensure!(rows[1]["taskId"] == id.as_str() && rows[2]["failurePhase"] == "created");
        ensure!(
            rows[3]["phase"] == "resource_intent"
                && !rows[3]["resourceUri"].is_null()
                && rows[3]["resourceSubscriptionId"].is_null()
                && rows[3]["resourceSnapshotDelivered"] == false
        );
        ensure!(
            rows[4]["resourceSubscriptionId"] == 7
                && rows[4]["resourceSnapshotDelivered"] == false
                && rows[5]["resourceSnapshotDelivered"] == true
        );
        ensure!(!bytes.contains("synthetic-credential") && !bytes.contains("callerTokenFile"));
        ensure!(receipt(&path).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            ensure!(fs::metadata(&path)?.permissions().mode() & 0o077 == 0);
            let public = directory.path().join("public");
            fs::create_dir(&public)?;
            fs::set_permissions(&public, fs::Permissions::from_mode(0o755))?;
            ensure!(receipt(&public.join("receipt.jsonl")).is_err());
        }
        Ok(())
    }
}
