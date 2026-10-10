//! Official Tasks lifecycle against one explicitly selected real forecast workload.
use super::*;
use rmcp::model::{
    CancelTaskParams, DetailedTask, GetTaskParams, ServerNotification, SubscriptionFilter,
    TaskStatus,
};
use serde::{Deserialize, Serialize};
use veoveo_types::CanonicalTaskId;
#[path = "lifecycle/assertions.rs"]
mod output_assertions;
#[path = "lifecycle/recovery.rs"]
mod recovery;

#[derive(Debug, Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
pub(super) enum Mode {
    Cancel,
    Reconnect,
    ProcessCrash,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Input {
    mode: Mode,
    request: TimeseriesForecastRequest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    crash: Option<recovery::Fixture>,
}
impl Input {
    pub fn load(path: &Path) -> Result<Self> {
        ensure!(
            path.is_absolute(),
            "Timeseries lifecycle input requires an absolute path"
        );
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::symlink_metadata(path)?;
        ensure!(
            metadata.is_file() && metadata.permissions().mode() & 0o777 == 0o600,
            "Timeseries lifecycle input requires a private 0600 regular file"
        );
        ensure!(
            metadata.len() <= 256 * 1024,
            "Timeseries lifecycle input exceeds 256 KiB"
        );
        let bytes = std::fs::read(path)?;
        ensure!(
            bytes.len() <= 256 * 1024,
            "Timeseries lifecycle input exceeds 256 KiB"
        );
        let input: Self = serde_json::from_slice(&bytes)
            .map_err(|_| anyhow!("invalid Timeseries lifecycle input"))?;
        input.validate()?;
        Ok(input)
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            matches!(&self.request.source, DuckDbTabularSource::InlineCsv { csv, .. } if !csv.is_empty() && csv.len() <= 128*1024),
            "lifecycle workload requires nonempty inline CSV at most 128 KiB"
        );
        ensure!(
            self.request.method
                == veoveo_timeseries_mcp::contract::TimeseriesForecastMethod::NaiveTrend,
            "lifecycle fixture requires CPU NaiveTrend"
        );
        ensure!(
            (self.mode == Mode::ProcessCrash) == self.crash.is_some(),
            "process_crash requires crash input; cancel/reconnect forbid it"
        );
        if let Some(crash) = &self.crash {
            crash.validate()?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
pub(super) enum Step {
    DispatchIntent,
    Admitted,
    BeforeAction,
    CancelIntent,
    CancelAcknowledged,
    Reconnected,
    AfterAction,
    Terminal,
    WatchArmed,
    ReadyForCrash,
    CrashObserved,
    ReplacementFenced,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Observation {
    step: Step,
    task: Option<DetailedTask>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Journal {
    input: Input,
    task_id: Option<CanonicalTaskId>,
    created_at: Option<String>,
    observations: Vec<Observation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    crash: Option<recovery::Journal>,
    pub(super) cleanup: cleanup::Trace,
    output: Option<TimeseriesForecastOutput>,
    artifact_digest: Option<veoveo_types::Sha256Digest>,
    pub(super) subscription_closed: bool,
    pub(super) connection_closed: bool,
    pub(super) passed: bool,
}
fn observe(
    file: &mut std::fs::File,
    receipt: &mut Receipt,
    step: Step,
    task: Option<DetailedTask>,
) -> Result<()> {
    let journal = receipt
        .lifecycle
        .as_mut()
        .expect("lifecycle journal admitted");
    ensure!(
        journal.observations.len() < 64,
        "Timeseries lifecycle observation budget exhausted"
    );
    journal.observations.push(Observation { step, task });
    persist(file, receipt)
}
fn same_task(task: &DetailedTask, id: &CanonicalTaskId, created: &str) -> Result<()> {
    ensure!(
        task.task.task_id == id.as_str() && task.task.created_at == created,
        "Timeseries lifecycle changed Task identity/creation time"
    );
    Ok(())
}
fn nonterminal(task: &DetailedTask) -> Result<()> {
    ensure!(
        matches!(
            task.status(),
            TaskStatus::Working | TaskStatus::InputRequired
        ),
        "forecast completed before unfinished lifecycle proof; workload is unqualified"
    );
    Ok(())
}
fn before_action(mode: Mode, task: &DetailedTask) -> Result<()> {
    if mode == Mode::ProcessCrash {
        ensure!(
            task.status() == TaskStatus::Working,
            "process crash requires an actually Working forecast; workload is unqualified"
        );
    } else if mode == Mode::Cancel {
        nonterminal(task)?;
    }
    Ok(())
}
async fn current(client: &SmokeMcpClient, id: &CanonicalTaskId) -> Result<DetailedTask> {
    client
        .get_task(GetTaskParams::new(id.to_string()))
        .await
        .map(|reply| reply.task)
        .map_err(|_| anyhow!("Timeseries lifecycle current Task read failed"))
}
async fn listen(
    client: &SmokeMcpClient,
    id: &CanonicalTaskId,
    state: &mut TaskNotificationState,
) -> Result<()> {
    state.listener_closed = false;
    let filter = SubscriptionFilter::builder()
        .task_ids([id.to_string()])
        .build();
    state.subscription = Some(
        client
            .listen(filter.clone())
            .await
            .map_err(|_| anyhow!("Timeseries lifecycle listener failed"))?,
    );
    ensure!(
        state.subscription.as_ref().unwrap().acknowledged() == &filter,
        "Timeseries lifecycle filter changed"
    );
    Ok(())
}
pub(super) async fn open_listener(
    handles: &mut cleanup::Handles,
    id: &CanonicalTaskId,
) -> Result<()> {
    let result = listen(handles.client.as_ref().unwrap(), id, &mut handles.state).await;
    if handles.state.subscription.is_some() {
        handles.opened_subscription();
    }
    result
}
pub(super) async fn notification(
    state: &mut TaskNotificationState,
    id: &CanonicalTaskId,
    created: &str,
) -> Result<DetailedTask> {
    // SDK Subscription::next rejects absent/foreign subscription IDs and updates
    // outside its acknowledged filter before returning an observation.
    let next = state
        .subscription
        .as_mut()
        .context("Timeseries lifecycle listener absent")?
        .next()
        .await
        .map_err(|_| anyhow!("Timeseries lifecycle notification failed"))?
        .context("Timeseries lifecycle listener ended")?;
    let ServerNotification::TaskStatusNotification(update) = next else {
        bail!("Timeseries lifecycle received unexpected notification")
    };
    same_task(&update.params.task, id, created)?;
    Ok(update.params.task)
}
async fn connect(installation: &support::InstalledTarget) -> Result<SmokeMcpClient> {
    let token = installation
        .token()
        .await
        .map_err(|_| anyhow!("Timeseries lifecycle OAuth admission failed"))?;
    connect_mcp_client(installation.operator.resource.as_str(), &token)
        .await
        .map_err(|_| anyhow!("Timeseries lifecycle connection failed"))
}
pub(super) async fn run(
    installation: &support::InstalledTarget,
    input: Input,
    file: &mut std::fs::File,
    receipt: &mut Receipt,
) -> Result<()> {
    receipt.lifecycle = Some(Journal {
        input: input.clone(),
        task_id: None,
        created_at: None,
        observations: Vec::new(),
        crash: input.crash.as_ref().map(|_| recovery::Journal::default()),
        cleanup: cleanup::Trace::default(),
        output: None,
        artifact_digest: None,
        subscription_closed: false,
        connection_closed: false,
        passed: false,
    });
    persist(file, receipt)?;
    let (owned, registration) = cleanup::register(file, cleanup::Role::Lifecycle)?;
    let mut cleanup_file = file.try_clone()?;
    let (watch_owned, watch_registration) = recovery::register(file)?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(300);
    let result = tokio::time::timeout_at(deadline, async {
        let mut handles = owned.lock().await;
        handles.opened_client(connect(installation).await?)?;
        handles.sync(receipt);
        persist(file, receipt)?;
        if let Some(fixture) = &input.crash {
            fixture.admit(installation)?;
            let driver = recovery::driver(installation, fixture, handles.client.as_ref().unwrap())?;
            driver.admit_crash_target(&fixture.target).await?;
            let mut retained = watch_owned.lock().await;
            driver
                .arm_crash_watch(&fixture.target, &mut retained.watch)
                .await?;
            retained.sync(receipt)?;
            observe(file, receipt, Step::WatchArmed, None)?;
        }
        observe(file, receipt, Step::DispatchIntent, None)?;
        let admitted = call_tool_as_task(
            handles.client.as_ref().unwrap(),
            "timeseries__forecast",
            serde_json::to_value(&input.request)?,
        )
        .await
        .map_err(|_| anyhow!("Timeseries lifecycle dispatch unresolved"))?;
        let id = CanonicalTaskId::parse(&admitted.task_id)?;
        let created = admitted.created_at.clone();
        receipt.lifecycle.as_mut().unwrap().task_id = Some(id.clone());
        receipt.lifecycle.as_mut().unwrap().created_at = Some(created.clone());
        observe(file, receipt, Step::Admitted, None)?;
        open_listener(&mut handles, &id).await?;
        handles.sync(receipt);
        persist(file, receipt)?;
        let baseline = notification(&mut handles.state, &id, &created).await?;
        observe(file, receipt, Step::BeforeAction, Some(baseline.clone()))?;
        before_action(input.mode, &baseline)?;
        let before = current(handles.client.as_ref().unwrap(), &id).await?;
        same_task(&before, &id, &created)?;
        before_action(input.mode, &before)?;
        observe(file, receipt, Step::BeforeAction, Some(before))?;
        match input.mode {
            Mode::Cancel => {
                observe(file, receipt, Step::CancelIntent, None)?;
                handles
                    .client
                    .as_ref()
                    .unwrap()
                    .cancel_task(CancelTaskParams::new(id.to_string()))
                    .await
                    .map_err(|_| anyhow!("Timeseries lifecycle cancellation outcome unresolved"))?;
                observe(file, receipt, Step::CancelAcknowledged, None)?;
            }
            Mode::ProcessCrash => {
                // This durable handshake authorizes only the externally applied, selected crash.
                let fixture = input
                    .crash
                    .as_ref()
                    .context("Timeseries crash fixture absent")?;
                let driver =
                    recovery::driver(installation, fixture, handles.client.as_ref().unwrap())?;
                recovery::ready_for_crash(
                    driver.admit_crash_target(&fixture.target),
                    file,
                    receipt,
                )
                .await?;
                let mut retained = watch_owned.lock().await;
                let progress = retained
                    .watch
                    .as_mut()
                    .context("Timeseries crash watch absent")?
                    .wait(deadline)
                    .await;
                retained.sync(receipt)?;
                persist(file, receipt)?;
                progress?;
                observe(file, receipt, Step::CrashObserved, None)?;
                // A fresh listener snapshot cannot reuse a queued pre-crash Working update.
                let closed = handles
                    .close_until(
                        tokio::time::Instant::from_std(
                            veoveo_testing_support::lifecycle::owner::cleanup_deadline()?,
                        ),
                        &mut cleanup_file,
                    )
                    .await;
                handles.sync(receipt);
                persist(file, receipt)?;
                closed?;
                handles.opened_client(connect(installation).await?)?;
                handles.sync(receipt);
                persist(file, receipt)?;
                open_listener(&mut handles, &id).await?;
                handles.sync(receipt);
                persist(file, receipt)?;
                let delivered = notification(&mut handles.state, &id, &created).await?;
                observe(file, receipt, Step::AfterAction, Some(delivered.clone()))?;
                before_action(input.mode, &delivered)?;
                let after = current(handles.client.as_ref().unwrap(), &id).await?;
                same_task(&after, &id, &created)?;
                observe(file, receipt, Step::AfterAction, Some(after.clone()))?;
                before_action(input.mode, &after)?;
            }
            Mode::Reconnect => {
                let closed = handles
                    .close_until(
                        tokio::time::Instant::from_std(
                            veoveo_testing_support::lifecycle::owner::cleanup_deadline()?,
                        ),
                        &mut cleanup_file,
                    )
                    .await;
                handles.sync(receipt);
                persist(file, receipt)?;
                closed?;
                handles.opened_client(connect(installation).await?)?;
                handles.sync(receipt);
                persist(file, receipt)?;
                observe(file, receipt, Step::Reconnected, None)?;
                let after = current(handles.client.as_ref().unwrap(), &id).await?;
                same_task(&after, &id, &created)?;
                observe(file, receipt, Step::AfterAction, Some(after.clone()))?;
                open_listener(&mut handles, &id).await?;
                handles.sync(receipt);
                persist(file, receipt)?;
            }
        }
        let mut terminal = None;
        for _ in 0..60 {
            let task = notification(&mut handles.state, &id, &created).await?;
            if !matches!(
                task.status(),
                TaskStatus::Working | TaskStatus::InputRequired
            ) {
                terminal = Some(task);
                break;
            }
        }
        let terminal = terminal.context("Timeseries lifecycle exceeded notification budget")?;
        let expected = if input.mode == Mode::Cancel {
            TaskStatus::Cancelled
        } else {
            TaskStatus::Completed
        };
        ensure!(
            terminal.status() == expected,
            "Timeseries lifecycle ended with unexpected terminal status"
        );
        let current = current(handles.client.as_ref().unwrap(), &id).await?;
        same_task(&current, &id, &created)?;
        output_assertions::terminal_agreement(&terminal, &current, &id, &created)?;
        observe(file, receipt, Step::Terminal, Some(current))?;
        if input.mode != Mode::Cancel {
            output_assertions::verify_output(
                handles.client.as_ref().unwrap(),
                &input.request,
                &id,
                &terminal,
                file,
                receipt,
            )
            .await?;
        }
        if input.mode == Mode::ProcessCrash {
            let mut retained = watch_owned.lock().await;
            retained
                .watch
                .as_mut()
                .context("Timeseries crash watch absent")?
                .admit_recovered_target()
                .await?;
            retained.sync(receipt)?;
            observe(file, receipt, Step::ReplacementFenced, None)?;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await
    .map_err(|_| {
        anyhow!("Timeseries lifecycle exceeded 300 seconds; retained Task outcome unresolved")
    })
    .and_then(|result| result);
    let watch_closed = {
        let mut retained = watch_owned.lock().await;
        let result = retained.close(&mut cleanup_file).await;
        let sync = retained.sync(receipt);
        let settled = if retained.closed() {
            watch_registration.settled()
        } else {
            Ok(())
        };
        result.and(sync).and(settled)
    };
    let mut handles = owned.lock().await;
    let closed = handles
        .close_until(
            tokio::time::Instant::from_std(
                veoveo_testing_support::lifecycle::owner::cleanup_deadline()?,
            ),
            &mut cleanup_file,
        )
        .await;
    handles.sync(receipt);
    receipt.lifecycle.as_mut().unwrap().passed =
        result.is_ok() && closed.is_ok() && watch_closed.is_ok() && handles.closed();
    persist(file, receipt)?;
    if handles.closed() {
        registration.settled()?;
    }
    result?;
    closed?;
    watch_closed?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn task(id: &str, created: &str, payload: rmcp::model::TaskPayload) -> DetailedTask {
        DetailedTask::new(
            rmcp::model::Task::new(id, payload.status(), created, created),
            payload,
        )
    }
    #[test]
    fn unfinished_proof_rejects_completed_retention_and_replaced_identity() -> Result<()> {
        let id = CanonicalTaskId::parse("gateway-real-forecast")?;
        let working = task(
            id.as_str(),
            "2026-10-09T00:00:00Z",
            rmcp::model::TaskPayload::Working,
        );
        nonterminal(&working)?;
        same_task(&working, &id, &working.task.created_at)?;
        let completed = task(
            id.as_str(),
            &working.task.created_at,
            rmcp::model::TaskPayload::Completed {
                result: serde_json::Map::new(),
            },
        );
        assert!(before_action(Mode::Cancel, &completed).is_err());
        before_action(Mode::ProcessCrash, &working)?;
        assert!(before_action(Mode::ProcessCrash, &completed).is_err());
        before_action(Mode::Reconnect, &completed)?;
        let cancelled = task(
            id.as_str(),
            &working.task.created_at,
            rmcp::model::TaskPayload::Cancelled,
        );
        assert!(nonterminal(&cancelled).is_err());
        assert!(
            same_task(
                &working,
                &CanonicalTaskId::parse("gateway-other-forecast")?,
                &working.task.created_at
            )
            .is_err()
        );
        assert!(same_task(&working, &id, "2026-10-09T01:00:00Z").is_err());
        Ok(())
    }
    #[test]
    fn lifecycle_admission_closes_modes_and_cpu_workload() -> Result<()> {
        let mut input = Input {
            mode: Mode::Cancel,
            request: forecast_request()?,
            crash: None,
        };
        input.validate()?;
        let mut wire = serde_json::to_value(&input)?;
        wire["unexpected"] = true.into();
        assert!(serde_json::from_value::<Input>(wire).is_err());
        let mut wire = serde_json::to_value(&input)?;
        wire["mode"] = "restart".into();
        assert!(serde_json::from_value::<Input>(wire).is_err());
        let mut wire = serde_json::to_value(&input)?;
        wire["restart"] = serde_json::json!({"deployment":"timeseries-mcp"});
        assert!(serde_json::from_value::<Input>(wire).is_err());
        input.mode = Mode::Reconnect;
        input.validate()?;
        if let DuckDbTabularSource::InlineCsv { csv, .. } = &mut input.request.source {
            *csv = "x".repeat(128 * 1024 + 1);
        }
        assert!(input.validate().is_err());
        Ok(())
    }
    #[test]
    fn partial_lifecycle_journal_preserves_uncertain_dispatch_and_cancel_intents() -> Result<()> {
        let input = Input {
            mode: Mode::Cancel,
            request: forecast_request()?,
            crash: None,
        };
        let mut receipt = Receipt::new(forecast_request()?);
        receipt.lifecycle = Some(Journal {
            input,
            task_id: None,
            created_at: None,
            crash: None,
            cleanup: cleanup::Trace::default(),
            observations: vec![Observation {
                step: Step::DispatchIntent,
                task: None,
            }],
            output: None,
            artifact_digest: None,
            subscription_closed: false,
            connection_closed: false,
            passed: false,
        });
        let mut file = tempfile::tempfile()?;
        persist(&mut file, &receipt)?;
        let id = CanonicalTaskId::parse("gateway-known-forecast")?;
        receipt.lifecycle.as_mut().unwrap().task_id = Some(id.clone());
        observe(&mut file, &mut receipt, Step::CancelIntent, None)?;
        receipt.settle(false);
        let saved: Receipt = serde_json::from_slice(&serde_json::to_vec(&receipt)?)?;
        let journal = saved.lifecycle.unwrap();
        assert!(
            journal.task_id.as_ref() == Some(&id) && !journal.passed && !journal.connection_closed
        );
        assert!(journal.observations.last().unwrap().step == Step::CancelIntent);
        Ok(())
    }
}
