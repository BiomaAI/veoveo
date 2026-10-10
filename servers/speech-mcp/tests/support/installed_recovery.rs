//! Selected installed crash qualification; no mutation or inference delay is invented here.
use super::*;
use rmcp::service::Subscription;
use veoveo_testing_support::installed::restart::{CrashReceipt, CrashTarget, DeploymentRestart};

// The maintained domain-services chart uses this owner role for both the
// Deployment component selector and its Rust server container. Deployment names
// still come from the installation; custom container-role renaming is unsupported.
const SERVER_ROLE: &str = "speech-mcp";

fn server_component(target: &CrashTarget) -> Result<&'static str> {
    ensure!(
        target.container == SERVER_ROLE,
        "Speech recovery requires the maintained speech-mcp Rust server container; worker sidecars and custom container-role names are unsupported"
    );
    Ok(SERVER_ROLE)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Fixture {
    pub target: CrashTarget,
    replacement_timeout_seconds: u64,
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Evidence {
    pub admitted_target: Option<CrashTarget>,
    pub before_replacement: Option<Observation>,
    pub working_delivery: Option<Observation>,
    pub arm_intent: bool,
    pub armed: bool,
    pub watch: Option<CrashReceipt>,
    pub after_replacement: Option<Observation>,
    pub replacement_subscription_id: Option<rmcp::model::RequestId>,
    pub final_target_checked: bool,
    pub watch_closed: bool,
    pub watch_close_failed: bool,
    pub listener_closed: bool,
    pub listener_close_failed: bool,
}
pub(super) fn admit_pairing(mode: Mode, deployment: &str, fixture: Option<&Fixture>) -> Result<()> {
    match (mode, fixture) {
        (Mode::Recover, Some(fixture)) => {
            fixture.target.validate()?;
            server_component(&fixture.target)?;
            ensure!(
                fixture.target.deployment == deployment,
                "Speech recovery selects another Deployment"
            );
            ensure!(
                (1..=300).contains(&fixture.replacement_timeout_seconds),
                "Speech replacement timeout must be1..300seconds"
            );
        }
        (Mode::Recover, None) => anyhow::bail!("Speech recover requires a selected crash fixture"),
        (_, Some(_)) => anyhow::bail!("Speech crash fixture requires recover mode"),
        (_, None) => (),
    }
    Ok(())
}
pub(super) async fn admit(
    input: &Input,
    caller: &SmokeMcpClient,
    journal: &mut Journal<'_>,
    file: &mut fs::File,
) -> Result<Option<DeploymentRestart>> {
    let Some(fixture) = &input.recovery else {
        return Ok(None);
    };
    journal.phase = Phase::RecoveryAdmission;
    journal.persist(file)?;
    let target = input.installation.validate()?;
    let driver = DeploymentRestart::new(
        &target,
        &input.installation.deployment,
        server_component(&fixture.target)?,
        caller.peer().clone(),
        veoveo_speech_contract::SpeechResource::Capabilities.to_uri(),
    )?;
    tokio::time::timeout(
        Duration::from_secs(30),
        driver.admit_crash_target(&fixture.target),
    )
    .await
    .context("Speech crash target admission deadline")??;
    journal.recovery.admitted_target = Some(fixture.target.clone());
    journal.persist(file)?;
    Ok(Some(driver))
}
/// Working is deliberately stricter than the cancellation profile's unfinished state.
fn working(id: &CanonicalTaskId, created: &Task, current: &DetailedTask) -> Result<Observation> {
    assertions::same(id, created, current)?;
    ensure!(
        current.status() == TaskStatus::Working,
        "Speech recovery requires current Working; early terminal or input-required work is unqualified"
    );
    Observation::admit(&current.task)
}
async fn working_delivery(
    listener: &mut Subscription,
    id: &CanonicalTaskId,
    created: &Task,
) -> Result<Observation> {
    let notification = listener
        .next()
        .await?
        .context("Speech recovery Task listener ended")?;
    ensure!(
        notification.get_meta().subscription_id() == Some(listener.id().clone()),
        "Speech recovery Working subscription identity differs"
    );
    let ServerNotification::TaskStatusNotification(update) = notification else {
        anyhow::bail!("Speech recovery filter delivered another notification");
    };
    working(id, created, &update.params.task)
}
pub(super) async fn observe(
    input: &Input,
    driver: &DeploymentRestart,
    created: &Task,
    handles: &mut Handles,
    journal: &mut Journal<'_>,
    file: &mut fs::File,
) -> Result<()> {
    let fixture = input
        .recovery
        .as_ref()
        .context("Speech recovery fixture missing")?;
    let id = journal
        .task_id
        .clone()
        .context("Speech recovery Task identity missing")?;
    let caller = handles
        .caller
        .handle
        .as_ref()
        .context("Speech recovery caller missing")?;
    journal.phase = Phase::RecoveryWorking;
    journal.persist(file)?;
    let listener = handles
        .listener
        .handle
        .as_mut()
        .context("Speech recovery listener missing")?;
    journal.recovery.working_delivery = Some(
        tokio::time::timeout(
            Duration::from_secs(15),
            working_delivery(listener, &id, created),
        )
        .await
        .context("Speech Working delivery deadline")??,
    );
    let delivered = journal.recovery.working_delivery.as_ref().unwrap();
    journal
        .first_delivered_status
        .get_or_insert(delivered.status);
    journal.delivered.push(delivered.clone());
    journal.working_delivered = true;
    journal.persist(file)?;
    let current = tokio::time::timeout(
        Duration::from_secs(10),
        caller.get_task(GetTaskParams::new(id.as_str())),
    )
    .await
    .context("Speech pre-replacement Task read deadline")??
    .task;
    journal.recovery.before_replacement = Some(Observation::admit(&current.task)?);
    journal.persist(file)?;
    working(&id, created, &current)?;
    // The maintained API assigns the actual watch into this retained slot before
    // awaiting its initial observation. Cleanup retains its original consuming close.
    journal.recovery.arm_intent = true;
    journal.persist(file)?;
    handles.crash_watch.closed = false;
    let armed = driver
        .arm_crash_watch(&fixture.target, &mut handles.crash_watch.handle)
        .await;
    if let Some(watch) = &handles.crash_watch.handle {
        journal.recovery.watch = watch.snapshot();
        handles.crash_receipt = journal.recovery.watch.clone();
    }
    journal.persist(file)?;
    armed?;
    journal.recovery.armed = true;
    journal.phase = Phase::RecoveryArmed;
    // An external operator may signal only the selected container after this marker.
    journal.persist(file)?;
    let watch = handles
        .crash_watch
        .handle
        .as_mut()
        .context("Speech retained crash watch missing")?;
    let replaced = watch
        .wait(
            tokio::time::Instant::now() + Duration::from_secs(fixture.replacement_timeout_seconds),
        )
        .await;
    journal.recovery.watch = watch.snapshot();
    handles.crash_receipt = journal.recovery.watch.clone();
    journal.phase = Phase::RecoveryReplacement;
    journal.persist(file)?;
    replaced?;
    // Do not infer unfinished recovery from a pre-crash observation or from a
    // completed Task surviving replacement. Require Working after the old exit.
    let current = tokio::time::timeout(
        Duration::from_secs(10),
        caller.get_task(GetTaskParams::new(id.as_str())),
    )
    .await
    .context("Speech post-replacement Task read deadline")??
    .task;
    journal.recovery.after_replacement = Some(Observation::admit(&current.task)?);
    journal.persist(file)?;
    working(&id, created, &current)?;
    let filter = SubscriptionFilter::builder()
        .task_id(id.to_string())
        .build();
    // This listener is deliberate after the observed process replacement. It is
    // separately owned, never a silent retry after a failed creation/cancellation.
    handles.recovery_listener.retain(
        tokio::time::timeout(Duration::from_secs(15), caller.listen(filter.clone()))
            .await
            .context("Speech replacement listener deadline")??,
    );
    let listener = handles.recovery_listener.handle.as_ref().unwrap();
    ensure!(
        listener.acknowledged() == &filter,
        "Speech replacement Task subscription differs"
    );
    journal.recovery.replacement_subscription_id = Some(listener.id().clone());
    journal.phase = Phase::RecoveryListening;
    journal.persist(file)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(deployment: &str) -> Result<Fixture> {
        Ok(serde_json::from_value(serde_json::json!({
            "target":{"deployment":deployment,"pod":"selected-pod","container":"speech-mcp",
                "namespaceUid":uuid::Uuid::now_v7(),"deploymentUid":uuid::Uuid::now_v7(),
                "replicaSetUid":uuid::Uuid::now_v7(),"podUid":uuid::Uuid::now_v7(),
                "containerId":"containerd://selected","imageId":"sha256:selected","restartCount":0},
            "replacementTimeoutSeconds":120
        }))?)
    }
    #[test]
    fn recovery_fixture_requires_explicit_mode_matching_target_and_bounded_timeout() -> Result<()> {
        let deployment = "selected-speech";
        let mut fixture = fixture(deployment)?;
        admit_pairing(Mode::Recover, deployment, Some(&fixture))?;
        ensure!(admit_pairing(Mode::Recover, deployment, None).is_err());
        ensure!(admit_pairing(Mode::Complete, deployment, Some(&fixture)).is_err());
        ensure!(admit_pairing(Mode::Cancel, deployment, Some(&fixture)).is_err());
        ensure!(admit_pairing(Mode::Recover, "another-deployment", Some(&fixture)).is_err());
        for timeout in [0, 301] {
            fixture.replacement_timeout_seconds = timeout;
            ensure!(admit_pairing(Mode::Recover, deployment, Some(&fixture)).is_err());
        }
        let mut raw = serde_json::to_value(&fixture.target)?;
        raw["unknown"] = true.into();
        ensure!(serde_json::from_value::<CrashTarget>(raw).is_err());
        ensure!(
            serde_json::from_value::<Fixture>(serde_json::json!({"target":fixture.target,
            "replacementTimeoutSeconds":120,"unknown":true}))
            .is_err()
        );
        Ok(())
    }
    #[test]
    fn recovery_role_binds_server_container_to_component() -> Result<()> {
        let deployment = "installation-selected-speech";
        let mut fixture = fixture(deployment)?;
        ensure!(server_component(&fixture.target)? == fixture.target.container);
        admit_pairing(Mode::Recover, deployment, Some(&fixture))?;
        for unsupported in ["inference-worker", "renamed-rust-server"] {
            fixture.target.container = unsupported.into();
            ensure!(server_component(&fixture.target).is_err());
            ensure!(admit_pairing(Mode::Recover, deployment, Some(&fixture)).is_err());
        }
        Ok(())
    }
    #[test]
    fn recovery_requires_working_same_identity_before_and_after_replacement() -> Result<()> {
        use rmcp::model::TaskPayload;
        let id = CanonicalTaskId::parse("speech.opaque-routed-recovery")?;
        let created = Task::new(
            id.as_str(),
            TaskStatus::Working,
            "2026-10-09T00:00:00Z",
            "2026-10-09T00:00:01Z",
        );
        let current = DetailedTask::new(created.clone(), TaskPayload::Working);
        working(&id, &created, &current)?;
        for payload in [
            TaskPayload::Cancelled,
            TaskPayload::Completed {
                result: Default::default(),
            },
        ] {
            let mut terminal = created.clone();
            terminal.status = payload.status();
            ensure!(working(&id, &created, &DetailedTask::new(terminal, payload)).is_err());
        }
        let mut other = current.clone();
        other.task.task_id = "speech.another-routed-task".into();
        ensure!(working(&id, &created, &other).is_err());
        other = current;
        other.task.created_at = "2026-10-09T00:01:00Z".into();
        ensure!(working(&id, &created, &other).is_err());
        Ok(())
    }
}
