use super::*;

fn installation() -> Result<veoveo_deploy_contract::InstallationTarget> {
    let mut target = veoveo_deploy_contract::InstallationTarget::decode(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../testing/fixtures/catalog-installation/installation-target.json"
    )))?;
    target.expected_deployments.push("reason-mcp".into());
    Ok(target)
}
fn route(target: &veoveo_deploy_contract::InstallationTarget) -> Result<ProtectedResourceId> {
    let mut url = target.public_base_url.clone();
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("fixture origin"))?
        .clear()
        .extend(["mcp", target.operator.profile.as_str()]);
    Ok(ProtectedResourceId::parse(url.as_str())?)
}
fn fixture(target: &veoveo_deploy_contract::InstallationTarget) -> Fixture {
    Fixture {
        schema: Schema::V1,
        context: target.kubernetes.context.clone(),
        namespace: target.kubernetes.namespace.clone(),
        operator_resource: route(target).expect("admitted fixture origin"),
        target: CrashTarget {
            deployment: "reason-mcp".into(),
            pod: "reason-selected".into(),
            container: "reason-mcp".into(),
            namespace_uid: uuid::Uuid::now_v7(),
            deployment_uid: uuid::Uuid::now_v7(),
            replica_set_uid: uuid::Uuid::now_v7(),
            pod_uid: uuid::Uuid::now_v7(),
            container_id: "containerd://selected".into(),
            image_id: format!("registry.test/reason@sha256:{}", "a".repeat(64)),
            restart_count: 0,
        },
    }
}
#[test]
fn reason_recovery_requires_public_owner_and_exact_installation_role() -> Result<()> {
    let target = installation()?;
    let valid = fixture(&target);
    let resource = route(&target)?;
    valid.admit(&target, &resource, true, false)?;
    ensure!(valid.admit(&target, &resource, false, false).is_err());
    ensure!(valid.admit(&target, &resource, true, true).is_err());
    let mut wrong = valid.clone();
    wrong.context.push_str("-foreign");
    ensure!(wrong.admit(&target, &resource, true, false).is_err());
    let mut wrong = valid.clone();
    wrong.namespace.push_str("-foreign");
    ensure!(wrong.admit(&target, &resource, true, false).is_err());
    let mut wrong = valid.clone();
    wrong.target.deployment = "not-declared".into();
    ensure!(wrong.admit(&target, &resource, true, false).is_err());
    let mut wrong = valid.clone();
    wrong.target.container = "worker-sidecar".into();
    ensure!(wrong.admit(&target, &resource, true, false).is_err());
    let mut wrong = valid.clone();
    wrong.target.pod_uid = uuid::Uuid::nil();
    ensure!(wrong.admit(&target, &resource, true, false).is_err());
    let mut wire = serde_json::to_value(&valid)?;
    wire["mode"] = serde_json::json!("completed-retention");
    ensure!(serde_json::from_value::<Fixture>(wire).is_err());
    Ok(())
}
#[tokio::test]
async fn reason_recovery_expired_cleanup_never_polls_late_success() -> Result<()> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let root = tempfile::tempdir()?;
    let polls = Arc::new(AtomicUsize::new(0));
    let counted = polls.clone();
    let mut handles = Handles {
        task_facts: Vec::new(),
        watch: None,
        closing: Some(Box::pin(async move {
            counted.fetch_add(1, Ordering::SeqCst);
            true
        })),
        receipt: None,
        cleanup_cap: Some(Instant::now() - Duration::from_secs(1)),
        operation_end: None,
        failed: false,
        persistence_failed: false,
        closed: false,
        journal: Arc::new(PrivateCallerJournal::create(
            &root.path().join("outcome.jsonl"),
        )?),
    };
    ensure!(handles.close(None).await.is_err());
    ensure!(
        handles
            .close(Some(Instant::now() + Duration::from_secs(60)))
            .await
            .is_err()
    );
    ensure!(polls.load(Ordering::SeqCst) == 0 && handles.failed && handles.closing.is_some());
    Ok(())
}

#[tokio::test]
async fn reason_recovery_fresh_instance_refusal_never_publishes_crash_permission() -> Result<()> {
    use std::cell::Cell;
    let permissions = Cell::new(0);
    let created = rmcp::model::Task::new(
        "reason-recovery-task",
        rmcp::model::TaskStatus::Working,
        "2026-10-10T00:00:00Z",
        "2026-10-10T00:00:00Z",
    );
    let id = veoveo_types::CanonicalTaskId::parse(&created.task_id)?;
    let completed = rmcp::model::DetailedTask::new(
        created.clone(),
        rmcp::model::TaskPayload::Completed {
            result: Default::default(),
        },
    );
    ensure!(
        ready_after_fence(async { require_working(&id, &created, &completed) }, || {
            permissions.set(permissions.get() + 1);
            Ok(())
        })
        .await
        .is_err()
    );

    ensure!(
        ready_after_fence(
            async { Err(anyhow::anyhow!("selected original container was replaced")) },
            || {
                permissions.set(permissions.get() + 1);
                Ok(())
            }
        )
        .await
        .is_err()
    );
    ensure!(permissions.get() == 0);
    ready_after_fence(async { Ok(()) }, || {
        permissions.set(permissions.get() + 1);
        Ok(())
    })
    .await?;
    ensure!(permissions.get() == 1);
    Ok(())
}

#[test]
fn reason_recovery_journal_failure_retains_task_facts_through_cancelled_waiter_and_owner_cleanup()
-> Result<()> {
    const CHILD: &str = "VEOVEO_REASON_RETAINED_FACTS_NATIVE";
    if std::env::var_os(CHILD).is_some() {
        return tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(async {
                // Poison the actual private journal lock before any Task fact is written.
                // This exercises its real fallible append path, without a production bypass.
                struct InterruptedWrite;
                impl Serialize for InterruptedWrite {
                    fn serialize<S: serde::Serializer>(
                        &self,
                        _: S,
                    ) -> std::result::Result<S::Ok, S::Error> {
                        panic!("controlled private journal interruption")
                    }
                }
                let root = tempfile::tempdir()?;
                let broken = Arc::new(PrivateCallerJournal::create(
                    &root.path().join("failed.jsonl"),
                )?);
                ensure!(
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                        || broken.append(&InterruptedWrite)
                    ))
                    .is_err()
                );
                let recovered_path = root.path().join("recovered.jsonl");
                let recovered = Arc::new(PrivateCallerJournal::create(&recovered_path)?);
                let close_polls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
                let counted = close_polls.clone();
                let handles = Arc::new(Mutex::new(Handles {
                    task_facts: Vec::new(),
                    watch: None,
                    closing: Some(Box::pin(async move {
                        counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        true
                    })),
                    receipt: None,
                    cleanup_cap: Some(Instant::now() + Duration::from_secs(5)),
                    operation_end: None,
                    failed: false,
                    persistence_failed: false,
                    closed: false,
                    journal: broken,
                }));
                let created = rmcp::model::Task::new(
                    "retained-reason-task",
                    rmcp::model::TaskStatus::Working,
                    "2026-10-10T01:02:03Z",
                    "2026-10-10T01:02:03Z",
                );
                let id = veoveo_types::CanonicalTaskId::parse(&created.task_id)?;
                let current = rmcp::model::DetailedTask::new(
                    created.clone(),
                    rmcp::model::TaskPayload::Working,
                );
                let retained = handles.clone();
                let result: Result<()> = owner::run(async {
                    owner::register_cleanup(
                        CleanupKind::Remote,
                        "Reason retained facts native",
                        "selected-task",
                        move || async move { retained.lock().await.close(None).await },
                    )?;
                    let mut waiter = Box::pin(async {
                        let sink = Acknowledgments {
                            handles: handles.clone(),
                        };
                        ensure!(sink.record(&id, &created).is_err());
                        let mut state = handles.lock().await;
                        ensure!(
                            state
                                .sync(&Observation::Working {
                                    id: &id,
                                    created: &created,
                                    current: &current
                                })
                                .is_err()
                        );
                        ensure!(
                            state
                                .sync(&Observation::ReplacementWorking {
                                    id: &id,
                                    created: &created,
                                    current: &current
                                })
                                .is_err()
                        );
                        drop(state);
                        std::future::pending::<Result<()>>().await
                    });
                    ensure!(
                        tokio::time::timeout(Duration::from_millis(10), &mut waiter)
                            .await
                            .is_err()
                    );
                    drop(waiter);
                    let mut state = handles.lock().await;
                    ensure!(
                        state.task_facts.len() == 3 && state.task_facts.iter().all(|f| !f.flushed)
                    );
                    // Storage becomes writable; the registered cleanup must flush its owned facts.
                    state.journal = recovered;
                    Err(anyhow::anyhow!("controlled operation cancellation"))
                })
                .await;
                ensure!(result.is_err());
                let state = handles.lock().await;
                ensure!(
                    state.closed
                        && state.persistence_failed
                        && state.task_facts.iter().all(|f| f.flushed)
                );
                ensure!(close_polls.load(std::sync::atomic::Ordering::SeqCst) == 1);
                let rows: Vec<serde_json::Value> = std::fs::read_to_string(recovered_path)?
                    .lines()
                    .map(serde_json::from_str)
                    .collect::<std::result::Result<_, _>>()?;
                let acknowledged = rows
                    .iter()
                    .find(|r| r["observation"] == "taskAcknowledged")
                    .context("retained acknowledgment not flushed")?;
                let task: rmcp::model::Task = serde_json::from_value(acknowledged["task"].clone())?;
                ensure!(task.task_id == created.task_id && task.created_at == created.created_at);
                let resumed = rows
                    .iter()
                    .find(|r| r["observation"] == "replacementWorking")
                    .context("retained replacement Working not flushed")?;
                let task: rmcp::model::DetailedTask =
                    serde_json::from_value(resumed["current"].clone())?;
                require_working(&id, &created, &task)?;
                ensure!(rows.last().is_some_and(|r| r["persistenceFailed"] == true));
                Ok(())
            });
    }
    let directory = tempfile::tempdir()?;
    std::fs::create_dir(directory.path().join("groups"))?;
    let deadline = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis()
        + 10_000;
    let output = std::process::Command::new(std::env::current_exe()?).args(["--exact","case_8::recovery::tests::reason_recovery_journal_failure_retains_task_facts_through_cancelled_waiter_and_owner_cleanup","--nocapture"])
        .env(CHILD,"1").env("VEOVEO_SMOKE_LOCAL_GROUPS",directory.path().join("groups")).env("VEOVEO_SMOKE_CLEANUP_SECONDS","2").env("VEOVEO_SMOKE_DEADLINE_UNIX_MS",deadline.to_string()).output()?;
    ensure!(
        output.status.success()
            && String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"),
        "isolated retained Reason Task facts control failed or selected no test"
    );
    Ok(())
}
