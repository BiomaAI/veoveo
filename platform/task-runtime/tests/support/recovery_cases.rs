//! Restart controls use live leases and independent replacement workers.
use super::*;
use veoveo_task_runtime::{RecoveryReport, TaskRecoveryStream};

async fn next_resume(stream: &mut TaskRecoveryStream) -> RecoveryReport {
    loop {
        let report = stream
            .next()
            .await
            .expect("retained recovery ended early")
            .unwrap();
        if !report.resumable.is_empty() {
            return report;
        }
    }
}

#[tokio::test]
async fn replacement_revisits_live_startup_lease_and_replicas_dispatch_once() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (db, old) = runtime("old").await;
        let id = old
            .create(draft("forecast", RecoveryClass::Resume))
            .await
            .unwrap()
            .snapshot
            .task_id;
        let lease = old.claim(id, Duration::from_secs(2)).await.unwrap();
        let a = TaskRuntime::new(db.a.clone(), "integration-server", "replacement-a");
        let b = TaskRuntime::new(db.b.clone(), "integration-server", "replacement-b");
        let mut sa = a.observe_startup_recovery().await.unwrap();
        let mut sb = b.observe_startup_recovery().await.unwrap();
        assert!(sa.next().await.unwrap().unwrap().resumable.is_empty());
        assert!(sb.next().await.unwrap().unwrap().resumable.is_empty());
        assert!(chrono::Utc::now() < lease.lease_expires_at);
        assert!(matches!(
            a.claim(id, Duration::from_secs(5)).await,
            Err(TaskError::LeaseHeld(_))
        ));
        let run = |runtime: TaskRuntime, mut stream: TaskRecoveryStream| async move {
            let mut dispatches = 0;
            while let Some(report) = stream.next().await {
                for task in report.unwrap().resumable {
                    match runtime.claim(task.task_id, Duration::from_secs(5)).await {
                        Ok(_) => {
                            assert!(chrono::Utc::now() >= lease.lease_expires_at);
                            dispatches += 1;
                            runtime
                                .transition(
                                    task.task_id,
                                    TaskTransition::Succeeded {
                                        result_uri: None,
                                        message: "done".into(),
                                        result: json!({"done":true}),
                                    },
                                )
                                .await
                                .unwrap();
                        }
                        Err(
                            TaskError::LeaseHeld(_)
                            | TaskError::Conflict(_)
                            | TaskError::InvalidTransition { .. },
                        ) => {}
                        Err(error) => panic!("unexpected claim failure: {error}"),
                    }
                }
            }
            dispatches
        };
        let (a, b) = tokio::join!(run(a, sa), run(b, sb));
        assert_eq!(a + b, 1);
        assert_eq!(
            old.get(id).await.unwrap().unwrap().status,
            TaskStatus::Succeeded
        );
    })
    .await
    .expect("deferred replica recovery exceeded 60 seconds");
}

#[tokio::test]
async fn renewal_postpones_recovery_and_fresh_tasks_do_not_reenter_startup_set() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (db, old) = runtime("old-renew").await;
        let id = old
            .create(draft("forecast", RecoveryClass::Resume))
            .await
            .unwrap()
            .snapshot
            .task_id;
        old.claim(id, Duration::from_secs(2)).await.unwrap();
        let replacement = TaskRuntime::new(db.b.clone(), "integration-server", "replacement");
        let mut stream = replacement.observe_startup_recovery().await.unwrap();
        assert!(stream.next().await.unwrap().unwrap().resumable.is_empty());
        let renewed = old.renew_lease(id, Duration::from_secs(4)).await.unwrap();
        let fresh = replacement
            .create(draft("forecast", RecoveryClass::Resume))
            .await
            .unwrap()
            .snapshot
            .task_id;
        replacement
            .claim(fresh, Duration::from_secs(10))
            .await
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(2500), next_resume(&mut stream))
                .await
                .is_err()
        );
        assert_eq!(
            old.get(id).await.unwrap().unwrap().lease_owner.as_deref(),
            Some("old-renew")
        );
        let report = next_resume(&mut stream).await;
        assert!(chrono::Utc::now() >= renewed.lease_expires_at.unwrap());
        assert_eq!(
            report
                .resumable
                .iter()
                .map(|s| s.task_id)
                .collect::<Vec<_>>(),
            vec![id]
        );
        assert!(stream.next().await.is_none());
        assert_eq!(
            replacement
                .get(fresh)
                .await
                .unwrap()
                .unwrap()
                .lease_owner
                .as_deref(),
            Some("replacement")
        );
    })
    .await
    .expect("deferred renewal recovery exceeded 60 seconds");
}

#[tokio::test]
async fn current_cancellation_terminal_and_provider_observation_survive_deferred_recovery() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (db, old) = runtime("old-controls").await;
        let mut ids = vec![];
        for class in [
            RecoveryClass::Resume,
            RecoveryClass::Resume,
            RecoveryClass::ProviderWait,
        ] {
            let id = old
                .create(draft("forecast", class))
                .await
                .unwrap()
                .snapshot
                .task_id;
            if class == RecoveryClass::ProviderWait {
                old.claim_observation(id, Duration::from_secs(2))
                    .await
                    .unwrap();
            } else {
                old.claim(id, Duration::from_secs(2)).await.unwrap();
            }
            ids.push(id);
        }
        let replacement = TaskRuntime::new(db.b.clone(), "integration-server", "replacement");
        let mut stream = replacement.observe_startup_recovery().await.unwrap();
        assert!(stream.next().await.unwrap().unwrap().resumable.is_empty());
        old.cancel(ids[0]).await.unwrap();
        old.transition(
            ids[1],
            TaskTransition::Succeeded {
                result_uri: None,
                message: "already done".into(),
                result: json!(1),
            },
        )
        .await
        .unwrap();
        let mut cancelled = vec![];
        let mut observing = vec![];
        while let Some(report) = stream.next().await {
            let report = report.unwrap();
            assert!(report.resumable.is_empty());
            cancelled.extend(report.cancelled.iter().map(|s| s.task_id));
            observing.extend(report.provider_waiting.iter().map(|s| s.task_id));
        }
        assert_eq!(cancelled, vec![ids[0]]);
        assert_eq!(observing, vec![ids[2]]);
        assert_eq!(
            old.get(ids[0]).await.unwrap().unwrap().status,
            TaskStatus::Cancelled
        );
        assert!(
            replacement
                .claim(ids[2], Duration::from_secs(5))
                .await
                .is_err()
        );
    })
    .await
    .expect("deferred current-state controls exceeded 60 seconds");
}

#[tokio::test]
async fn excluded_server_terminal_and_live_lease_payloads_never_enter_decode() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (db, old) = runtime("old-exclusions").await;
        let live = old
            .create(draft("forecast", RecoveryClass::Resume))
            .await
            .unwrap()
            .snapshot
            .task_id;
        old.claim(live, Duration::from_secs(30)).await.unwrap();
        let terminal = old
            .create(draft("forecast", RecoveryClass::Resume))
            .await
            .unwrap()
            .snapshot
            .task_id;
        old.claim(terminal, Duration::from_secs(30)).await.unwrap();
        old.transition(
            terminal,
            TaskTransition::Succeeded {
                result_uri: None,
                message: "done".into(),
                result: json!(1),
            },
        )
        .await
        .unwrap();
        let foreign = TaskRuntime::new(db.b.clone(), "foreign-server", "foreign");
        let mut request = draft("forecast", RecoveryClass::Resume);
        request.server = "foreign-server".into();
        let foreign_id = foreign.create(request).await.unwrap().snapshot.task_id;
        db.b.client()
            .query(include_str!("../queries/recovery/excluded_payloads.surql"))
            .bind((
                "records",
                vec![live, terminal, foreign_id]
                    .into_iter()
                    .map(task_record_id)
                    .collect::<Vec<_>>(),
            ))
            .await
            .unwrap()
            .check()
            .unwrap();
        let eligible = old
            .create(draft("forecast", RecoveryClass::Resume))
            .await
            .unwrap()
            .snapshot
            .task_id;
        let replacement = TaskRuntime::new(db.b.clone(), "integration-server", "replacement");
        let one_shot = replacement.recover().await.unwrap();
        assert_eq!(
            one_shot
                .resumable
                .iter()
                .map(|s| s.task_id)
                .collect::<Vec<_>>(),
            vec![eligible]
        );
        let mut stream = replacement.observe_startup_recovery().await.unwrap();
        let initial = stream.next().await.unwrap().unwrap();
        assert_eq!(
            initial
                .resumable
                .iter()
                .map(|s| s.task_id)
                .collect::<Vec<_>>(),
            vec![eligible]
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(400), next_resume(&mut stream))
                .await
                .is_err()
        );
        drop(stream);
    })
    .await
    .expect("recovery SQL exclusion exceeded 60 seconds");
}

#[tokio::test]
async fn simultaneous_queued_startup_claims_have_one_dispatch_authority() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (db, old) = runtime("queued-owner").await;
        let id = old.create(draft("forecast", RecoveryClass::Resume)).await.unwrap().snapshot.task_id;
        let a = TaskRuntime::new(db.a.clone(), "integration-server", "queued-a");
        let b = TaskRuntime::new(db.b.clone(), "integration-server", "queued-b");
        let mut sa = a.observe_startup_recovery().await.unwrap();
        let mut sb = b.observe_startup_recovery().await.unwrap();
        assert_eq!(sa.next().await.unwrap().unwrap().resumable[0].task_id, id);
        assert_eq!(sb.next().await.unwrap().unwrap().resumable[0].task_id, id);
        let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(2));
        let claim = |runtime: TaskRuntime, barrier: std::sync::Arc<tokio::sync::Barrier>| async move {
            barrier.wait().await;
            runtime.claim(id, Duration::from_secs(10)).await
        };
        let (a, b) = tokio::join!(claim(a, barrier.clone()), claim(b, barrier));
        assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
        for result in [a, b] {
            if let Err(error) = result {
                assert!(matches!(error, TaskError::Conflict(_) | TaskError::LeaseHeld(_)), "{error}");
            }
        }
        assert!(sa.next().await.is_none());
        assert!(sb.next().await.is_none());
    }).await.expect("simultaneous queued recovery exceeded 60 seconds");
}
