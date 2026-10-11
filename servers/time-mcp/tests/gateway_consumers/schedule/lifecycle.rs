//! Selected Task lifecycle observations and explicitly admitted process signalling.
#[path = "lifecycle/runtime_signal.rs"]
mod runtime_signal;
use super::*;
use rmcp::model::{CancelTaskParams, GetTaskParams};
use veoveo_testing_support::installed::restart::{CrashReceipt, CrashTarget, DeploymentRestart};
use veoveo_types::ResourceAddress;
#[derive(Debug, Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
pub(super) enum Mode {
    Complete,
    Cancel,
    Recover,
}
impl Mode {
    pub(super) fn operation_budget(self) -> Duration {
        Duration::from_secs(if self == Self::Recover { 300 } else { 120 })
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Fixture {
    target: CrashTarget,
    pub replacement_timeout_seconds: u64,
    runtime_signal: Option<runtime_signal::RuntimeSignal>,
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Evidence {
    admitted_target: Option<CrashTarget>,
    working_delivery: Option<TaskObservation>,
    before_effect: Option<TaskObservation>,
    cancel_intent: bool,
    cancel_acknowledged: bool,
    failure_mcp_code: Option<i32>,
    failure_sha256: Option<veoveo_types::Sha256Digest>,
    crash_arm_intent: bool,
    armed: bool,
    pub watch: Option<CrashReceipt>,
    after_replacement: Option<TaskObservation>,
    replacement_subscription_id: Option<rmcp::model::RequestId>,
    pub final_target_checked: bool,
    pub watch_closed: bool,
    pub replacement_listener_closed: bool,
    signal_runtime: Option<runtime_signal::RuntimeSignal>,
    signal_fenced_at: Option<chrono::DateTime<chrono::Utc>>,
    signal_dispatch_intent: bool,
    signal_command_succeeded: bool,
}
impl Evidence {
    pub(super) fn failure(&mut self, error: &anyhow::Error) {
        use sha2::{Digest, Sha256};
        self.failure_sha256 = veoveo_types::Sha256Digest::from_hex(hex::encode(Sha256::digest(
            error.to_string().as_bytes(),
        )))
        .ok();
        for cause in error.chain() {
            if let Some(rmcp::ServiceError::McpError(data)) =
                cause.downcast_ref::<rmcp::ServiceError>()
            {
                self.failure_mcp_code = Some(data.code.0);
            }
        }
    }
}
pub(super) fn admit(input: &Input) -> Result<()> {
    match (input.mode, input.recovery.as_ref()) {
        (Mode::Recover, Some(fixture)) => {
            fixture.target.validate()?;
            if let Some(runtime) = &fixture.runtime_signal {
                runtime.validate(&fixture.target)?;
            }
            ensure!(
                fixture.target.deployment == input.installation.deployment,
                "Time crash fixture selects another installation Deployment"
            );
            ensure!(
                fixture.target.container == "time-mcp",
                "Time crash fixture requires the maintained time-mcp Rust server container role"
            );
            ensure!(
                (1..=300).contains(&fixture.replacement_timeout_seconds),
                "Time replacement timeout must be1..300seconds"
            );
        }
        (Mode::Recover, None) => anyhow::bail!("Time recover requires a selected crash fixture"),
        (_, Some(_)) => anyhow::bail!("Time crash fixture requires recover mode"),
        (_, None) => (),
    }
    Ok(())
}
fn recovery_driver(
    input: &Input,
    target: &veoveo_deploy_contract::InstallationTarget,
    caller: rmcp::Peer<rmcp::RoleClient>,
) -> Result<DeploymentRestart> {
    DeploymentRestart::new(
        target,
        &input.installation.deployment,
        "time-mcp",
        caller,
        TimeResource::AuthoritiesCurrent.to_uri()?,
    )
}
pub(super) struct RecoveryDriver {
    restart: DeploymentRestart,
    signal: Option<runtime_signal::Prepared>,
}
pub(super) async fn admit_target(
    input: &Input,
    target: &veoveo_deploy_contract::InstallationTarget,
    caller: &SmokeMcpClient,
    journal: &mut Journal<'_>,
    file: &mut fs::File,
    deadline: tokio::time::Instant,
) -> Result<Option<RecoveryDriver>> {
    let Some(fixture) = &input.recovery else {
        return Ok(None);
    };
    journal.phase = Phase::CrashAdmission;
    journal.persist(file)?;
    let driver = recovery_driver(input, target, caller.peer().clone())?;
    tokio::time::timeout(
        Duration::from_secs(30),
        driver.admit_crash_target(&fixture.target),
    )
    .await
    .context("Time crash admission deadline")??;
    journal.lifecycle.admitted_target = Some(fixture.target.clone());
    journal.persist(file)?;
    let signal = match &fixture.runtime_signal {
        Some(config) => Some(
            runtime_signal::Prepared::admit(config.clone(), &fixture.target, target, deadline)
                .await?,
        ),
        None => None,
    };
    Ok(Some(RecoveryDriver {
        restart: driver,
        signal,
    }))
}
fn retain_signal_checkpoint(
    journal: &mut Journal<'_>,
    file: &mut fs::File,
    current: &DetailedTask,
    intent: bool,
) -> Result<()> {
    journal.lifecycle.before_effect = Some(TaskObservation::admit(&current.task)?);
    if intent {
        ensure!(
            !journal.lifecycle.signal_dispatch_intent,
            "Time signal intent already retained; never resend"
        );
        journal.lifecycle.armed = true;
        journal.phase = Phase::CrashArmed;
        journal.lifecycle.signal_dispatch_intent = true;
    }
    journal.persist(file)
}
fn same(id: &CanonicalTaskId, created: &Task, current: &DetailedTask) -> Result<()> {
    ensure!(
        CanonicalTaskId::parse(&created.task_id)? == *id
            && CanonicalTaskId::parse(&current.task.task_id)? == *id
            && created.created_at == current.task.created_at,
        "Time lifecycle Task identity/creation disagrees"
    );
    Ok(())
}
fn working(id: &CanonicalTaskId, created: &Task, current: &DetailedTask) -> Result<()> {
    same(id, created, current)?;
    ensure!(
        current.status() == TaskStatus::Working && matches!(current.payload, TaskPayload::Working),
        "Time lifecycle requires genuinely Working Task; early completion is unqualified"
    );
    Ok(())
}
pub(super) fn cancelled(
    id: &CanonicalTaskId,
    created: &Task,
    delivered: &DetailedTask,
    current: &DetailedTask,
) -> Result<()> {
    same(id, created, delivered)?;
    same(id, created, current)?;
    ensure!(
        delivered.status() == TaskStatus::Cancelled
            && current.status() == TaskStatus::Cancelled
            && delivered.payload == current.payload,
        "Time cancellation requires delivered and current Cancelled agreement"
    );
    ensure!(
        matches!(current.payload, TaskPayload::Cancelled),
        "Time cancellation published a successful result"
    );
    Ok(())
}
async fn next_working(
    listener: &mut Subscription,
    id: &CanonicalTaskId,
    created: &Task,
) -> Result<DetailedTask> {
    let notification = listener
        .next()
        .await?
        .context("Time Working listener ended")?;
    ensure!(
        notification.get_meta().subscription_id() == Some(listener.id().clone()),
        "Time Working delivery subscription differs"
    );
    let ServerNotification::TaskStatusNotification(update) = notification else {
        anyhow::bail!("Time Task filter delivered another notification")
    };
    working(id, created, &update.params.task)?;
    Ok(update.params.task)
}
pub(super) async fn observe(
    input: &Input,
    driver: Option<&RecoveryDriver>,
    created: &Task,
    handles: &mut Handles,
    journal: &mut Journal<'_>,
    file: &mut fs::File,
    deadline: tokio::time::Instant,
) -> Result<()> {
    let id = journal
        .task_id
        .as_ref()
        .context("Time acknowledged Task missing")?
        .clone();
    let caller = handles.caller.as_ref().context("Time caller missing")?;
    let listener = handles.listener.as_mut().context("Time listener missing")?;
    let filter = SubscriptionFilter::builder()
        .task_id(id.to_string())
        .build();
    ensure!(
        listener.acknowledged() == &filter,
        "Time Working subscription not acknowledged"
    );
    let delivered = tokio::time::timeout(
        Duration::from_secs(15),
        next_working(listener, &id, created),
    )
    .await
    .context("Time Working delivery deadline")??;
    journal
        .first_delivered_status
        .get_or_insert(delivered.status());
    journal.working_delivered = true;
    journal.lifecycle.working_delivery = Some(TaskObservation::admit(&delivered.task)?);
    journal.phase = Phase::Working;
    journal.persist(file)?;
    let current = caller.get_task(GetTaskParams::new(id.as_str())).await?.task;
    journal.lifecycle.before_effect = Some(TaskObservation::admit(&current.task)?);
    journal.persist(file)?;
    working(&id, created, &current)?;
    if input.mode == Mode::Cancel {
        journal.lifecycle.cancel_intent = true;
        journal.phase = Phase::CancelIntent;
        journal.persist(file)?;
        owner::check_effect()?;
        caller
            .cancel_task(CancelTaskParams::new(id.as_str()))
            .await?;
        journal.lifecycle.cancel_acknowledged = true;
        journal.phase = Phase::CancelAcknowledged;
        journal.persist(file)?;
        return Ok(());
    }
    let fixture = input
        .recovery
        .as_ref()
        .context("Time recovery fixture missing")?;
    let driver = driver.context("Time admitted crash driver missing")?;
    journal.lifecycle.crash_arm_intent = true;
    journal.phase = Phase::CrashArmIntent;
    journal.persist(file)?;
    let armed = driver
        .restart
        .arm_crash_watch(&fixture.target, &mut handles.crash_watch)
        .await;
    if let Some(watch) = &handles.crash_watch {
        handles.crash_receipt = watch.snapshot();
    }
    journal.lifecycle.watch = handles.crash_receipt.clone();
    journal.persist(file)?;
    armed?;
    // Finish every slow runtime/process fence before the final authenticated read.
    if let Some(signal) = &driver.signal {
        signal.verify(deadline).await?;
        journal.lifecycle.signal_runtime = Some(signal.identity());
        journal.lifecycle.signal_fenced_at = Some(chrono::Utc::now());
        journal.persist(file)?;
    }
    // There is no slow preparation after this final authenticated read.
    if let Some(signal) = &driver.signal {
        runtime_signal::dispatch_after_current(
            &id,
            created,
            deadline,
            async { Ok(caller.get_task(GetTaskParams::new(id.as_str())).await?.task) },
            |current, intent| retain_signal_checkpoint(journal, file, current, intent),
            signal.dispatch(deadline),
        )
        .await?;
        journal.lifecycle.signal_command_succeeded = true;
        journal.persist(file)?;
    } else {
        let current = caller.get_task(GetTaskParams::new(id.as_str())).await?.task;
        journal.lifecycle.before_effect = Some(TaskObservation::admit(&current.task)?);
        journal.persist(file)?;
        working(&id, created, &current)?;
        journal.lifecycle.armed = true;
        journal.phase = Phase::CrashArmed;
        journal.persist(file)?;
    }
    let watch = handles
        .crash_watch
        .as_mut()
        .context("Time crash watch missing")?;
    let replaced = watch
        .wait(deadline.min(
            tokio::time::Instant::now() + Duration::from_secs(fixture.replacement_timeout_seconds),
        ))
        .await;
    handles.crash_receipt = watch.snapshot();
    journal.lifecycle.watch = handles.crash_receipt.clone();
    journal.phase = Phase::ReplacementObserved;
    journal.persist(file)?;
    replaced?;
    let current = caller.get_task(GetTaskParams::new(id.as_str())).await?.task;
    journal.lifecycle.after_replacement = Some(TaskObservation::admit(&current.task)?);
    journal.persist(file)?;
    working(&id, created, &current)?;
    handles.replacement_listener = Some(caller.listen(filter.clone()).await?);
    let listener = handles.replacement_listener.as_ref().unwrap();
    ensure!(
        listener.acknowledged() == &filter,
        "Time replacement subscription differs"
    );
    journal.lifecycle.replacement_subscription_id = Some(listener.id().clone());
    journal.phase = Phase::ReplacementListening;
    journal.persist(file)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn target() -> Result<Fixture> {
        Ok(serde_json::from_value(serde_json::json!({
            "target":{"deployment":"time-mcp","pod":"selected-pod","container":"time-mcp",
                "namespaceUid":uuid::Uuid::now_v7(),"deploymentUid":uuid::Uuid::now_v7(),
                "replicaSetUid":uuid::Uuid::now_v7(),"podUid":uuid::Uuid::now_v7(),
                "containerId":"containerd://selected","imageId":"sha256:selected","restartCount":0},
            "replacementTimeoutSeconds":60
        }))?)
    }
    #[tokio::test]
    async fn retained_installation_admits_recovery_driver_after_private_journal_creation()
    -> Result<()> {
        use rmcp::{ClientServiceExt, ServiceExt};
        let root = tempfile::tempdir()?;
        let mut input = super::super::tests::fixture()?;
        input.mode = Mode::Recover;
        input.recovery = Some(target()?);
        input.installation.installation_target = root.path().join("target.json");
        input.installation.output = root.path().join("receipt.jsonl");
        fs::write(
            &input.installation.installation_target,
            serde_json::to_vec(&serde_json::json!({
                "schema":"veoveo.ai/installation-target/v1",
                "kubernetes":{"context":"fixture","namespace":"fixture"},
                "localBaseUrl":"http://127.0.0.1:8080",
                "publicBaseUrl":"https://installation.example",
                "controlPlane":"gateway.json","expectedDeployments":["time-mcp"],
                "minimumGpuShares":0,
                "operator":{"clientId":"fixture","profile":"operator","scopes":["time:read"],"workContext":"fixture"}
            }))?,
        )?;
        let admitted = input.installation.validate()?;
        let mut output = open_receipt(&input.installation.output)?;
        Journal::new(&input).persist(&mut output)?;
        ensure!(
            input.installation.validate().is_err(),
            "existing output must still refuse initial admission"
        );
        ensure!(open_receipt(&input.installation.output).is_err());
        struct Source;
        impl rmcp::ServerHandler for Source {}
        let (server_io, client_io) = tokio::io::duplex(8192);
        let (server, client) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(
                Source.serve(server_io),
                ().serve_with_lifecycle(
                    client_io,
                    rmcp::ClientLifecycleMode::Discover {
                        preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
                    }
                )
            )
        })
        .await
        .context("native recovery driver SDK deadline")?;
        let server = server?;
        let client = client?;
        let result = recovery_driver(&input, &admitted, client.peer().clone()).map(|_| ());
        let (client_closed, server_closed) = tokio::join!(
            tokio::time::timeout(Duration::from_secs(5), client.cancel()),
            tokio::time::timeout(Duration::from_secs(5), server.cancel()),
        );
        client_closed.context("native client cleanup deadline")??;
        server_closed.context("native server cleanup deadline")??;
        result
    }
    #[test]
    fn lifecycle_fixture_requires_mode_target_role_and_bounded_timeout() -> Result<()> {
        ensure!(
            Mode::Complete.operation_budget() == Duration::from_secs(120)
                && Mode::Cancel.operation_budget() == Duration::from_secs(120)
                && Mode::Recover.operation_budget() == Duration::from_secs(300)
        );
        let mut input = super::super::tests::fixture()?;
        input.mode = Mode::Recover;
        ensure!(input.admit().is_err());
        input.recovery = Some(target()?);
        input.admit()?;
        input.mode = Mode::Complete;
        ensure!(input.admit().is_err());
        input.mode = Mode::Cancel;
        ensure!(input.admit().is_err());
        input.mode = Mode::Recover;
        input.recovery.as_mut().unwrap().target.container = "worker".into();
        ensure!(input.admit().is_err());
        input.recovery = Some(target()?);
        input.recovery.as_mut().unwrap().target.deployment = "another".into();
        ensure!(input.admit().is_err());
        input.recovery = Some(target()?);
        input.recovery.as_mut().unwrap().replacement_timeout_seconds = 301;
        ensure!(input.admit().is_err());
        input.recovery = None;
        input.mode = Mode::Cancel;
        input.admit()?;
        Ok(())
    }
    #[test]
    fn working_and_cancelled_require_original_identity_and_unfinished_state() -> Result<()> {
        let id = CanonicalTaskId::parse("time.opaque-routed-task")?;
        let created = Task::new(
            id.to_string(),
            TaskStatus::Working,
            "2026-10-10T00:00:00Z",
            "2026-10-10T00:00:00Z",
        );
        let task = DetailedTask::new(created.clone(), TaskPayload::Working);
        working(&id, &created, &task)?;
        let mut foreign = task.clone();
        foreign.task.task_id = "time.other".into();
        ensure!(working(&id, &created, &foreign).is_err());
        foreign = task.clone();
        foreign.task.created_at = "2026-10-10T00:00:01Z".into();
        ensure!(working(&id, &created, &foreign).is_err());
        let completed =
            super::super::tests::completed(&id, &super::super::tests::fixture()?.expected)?;
        ensure!(working(&id, &created, &completed).is_err());
        let mut cancelled_task = created.clone();
        cancelled_task.status = TaskStatus::Cancelled;
        let cancelled_task = DetailedTask::new(cancelled_task, TaskPayload::Cancelled);
        cancelled(&id, &created, &cancelled_task, &cancelled_task)?;
        ensure!(working(&id, &created, &cancelled_task).is_err());
        ensure!(cancelled(&id, &created, &cancelled_task, &completed).is_err());
        ensure!(cancelled(&id, &created, &task, &task).is_err());
        Ok(())
    }
}
