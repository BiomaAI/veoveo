//! Store-qualified Resume delivery; no provider, publication or hardware execution.
use super::*;
use veoveo_task_runtime::{ResumeCancellationPolicy as Policy, TaskSnapshot};

async fn running(runtime: &TaskRuntime) -> TaskSnapshot {
    let task = runtime
        .create(draft("calculation", RecoveryClass::Resume))
        .await
        .unwrap()
        .snapshot;
    runtime
        .claim(task.task_id, Duration::from_secs(60))
        .await
        .unwrap()
        .snapshot
}
fn success() -> TaskTransition {
    TaskTransition::Succeeded {
        result_uri: None,
        message: "calculation complete".into(),
        result: json!({"answer": 7}),
    }
}
#[tokio::test]
async fn resume_settlement_reconciles_cancellation_and_preserves_first_terminal_and_failure_policy()
{
    tokio::time::timeout(Duration::from_secs(90), async {
        let (db, a) = runtime("resume-a").await;
        let b = TaskRuntime::new(db.b.clone(), "integration-server", "resume-b");
        let selected = running(&a).await;
        b.cancel(selected.task_id).await.unwrap();
        // The old raw path rejects the selected version and leaves cancellation pending.
        assert!(matches!(
            a.transition_if_current(&selected, success()).await,
            Err(TaskError::Conflict(_))
        ));
        assert_eq!(
            b.get(selected.task_id).await.unwrap().unwrap().status,
            TaskStatus::CancelRequested
        );
        let settled = a
            .transition_resumable_if_current(&selected, success(), Policy::CancellationWins, None)
            .await
            .unwrap();
        assert_eq!(settled.status, TaskStatus::Cancelled);
        assert_eq!(settled.task_id, selected.task_id);
        assert_eq!(settled.owner, selected.owner);
        assert_eq!(settled.request, selected.request);
        assert_eq!(settled.created_at, selected.created_at);
        assert!(settled.result.is_none());

        for policy in [Policy::CancellationWins, Policy::PreserveFailure] {
            let selected = running(&a).await;
            b.cancel(selected.task_id).await.unwrap();
            let failure = TaskFailure::new("calculation_failed", "native failure");
            let settled = a
                .transition_resumable_if_current(
                    &selected,
                    TaskTransition::Failed(failure.clone()),
                    policy,
                    None,
                )
                .await
                .unwrap();
            if policy == Policy::CancellationWins {
                assert_eq!(settled.status, TaskStatus::Cancelled);
                assert!(settled.error.is_none());
            } else {
                assert_eq!(settled.status, TaskStatus::Failed);
                assert_eq!(settled.error, Some(failure));
            }
            assert!(settled.result.is_none());
        }
        let selected = running(&a).await;
        let completed = a
            .transition_resumable_if_current(&selected, success(), Policy::CancellationWins, None)
            .await
            .unwrap();
        assert_eq!(b.cancel(selected.task_id).await.unwrap(), completed);
        assert_eq!(
            a.transition_resumable_if_current(
                &selected,
                TaskTransition::Failed(TaskFailure::interrupted_indeterminate()),
                Policy::CancellationWins,
                None
            )
            .await
            .unwrap(),
            completed
        );
    })
    .await
    .expect("Resume settlement controls exceeded 90 seconds");
}

#[tokio::test]
async fn resume_settlement_rejects_unrelated_conflicts_foreign_leases_and_forged_or_excluded_identity()
 {
    tokio::time::timeout(Duration::from_secs(90), async {
        let (db, a) = runtime("resume-a").await;
        let b = TaskRuntime::new(db.b.clone(), "integration-server", "resume-b");
        let selected = running(&a).await;
        a.transition(
            selected.task_id,
            TaskTransition::Running {
                message: "new progress".into(),
                progress: 0.5,
            },
        )
        .await
        .unwrap();
        assert!(matches!(
            a.transition_resumable_if_current(&selected, success(), Policy::CancellationWins, None)
                .await,
            Err(TaskError::Conflict(_))
        ));
        assert_eq!(
            a.get(selected.task_id).await.unwrap().unwrap().progress,
            0.5
        );
        b.cancel(selected.task_id).await.unwrap();
        assert!(matches!(
            b.transition_resumable(selected.task_id, success(), Policy::CancellationWins, None)
                .await,
            Err(TaskError::LeaseHeld(_))
        ));
        let original = running(&a).await;
        let mut cases = Vec::new();
        let mut changed = original.clone();
        changed.server = "other-server".into();
        cases.push(changed);
        let mut changed = original.clone();
        changed.task_id = veoveo_types::TaskId::new();
        cases.push(changed);
        let mut changed = original.clone();
        changed.lease_expires_at = Some(chrono::Utc::now() - chrono::TimeDelta::seconds(1));
        cases.push(changed);
        let mut changed = original.clone();
        changed.ttl_ms = Some(123);
        cases.push(changed);
        let mut changed = original.clone();
        changed.poll_interval_ms = Some(123);
        cases.push(changed);
        let mut changed = original.clone();
        changed.request = json!({"changed": true});
        cases.push(changed);
        let mut changed = original.clone();
        changed.owner.subject = "other".into();
        cases.push(changed);
        let mut changed = original.clone();
        changed.task_type = "other".parse().unwrap();
        cases.push(changed);
        let mut changed = original.clone();
        changed.created_at -= chrono::TimeDelta::seconds(1);
        cases.push(changed);
        let mut changed = original.clone();
        changed.recovery_class = RecoveryClass::ProviderWait;
        cases.push(changed);
        for changed in cases {
            assert!(
                a.transition_resumable_if_current(
                    &changed,
                    success(),
                    Policy::CancellationWins,
                    None
                )
                .await
                .is_err()
            );
        }
        assert_eq!(a.get(original.task_id).await.unwrap().unwrap(), original);
        assert!(
            a.transition_resumable(
                original.task_id,
                TaskTransition::CancelRequested,
                Policy::CancellationWins,
                None
            )
            .await
            .is_err()
        );
        for class in [
            RecoveryClass::ProviderWait,
            RecoveryClass::WebhookWait,
            RecoveryClass::InterruptedIndeterminate,
        ] {
            let task = a.create(draft("excluded", class)).await.unwrap().snapshot;
            assert!(matches!(
                a.transition_resumable(task.task_id, success(), Policy::CancellationWins, None)
                    .await,
                Err(TaskError::InvalidRecord(_))
            ));
            let mut forged = task.clone();
            forged.recovery_class = RecoveryClass::Resume;
            assert!(
                a.transition_resumable_if_current(
                    &forged,
                    success(),
                    Policy::CancellationWins,
                    None
                )
                .await
                .is_err()
            );
            assert_eq!(a.get(task.task_id).await.unwrap().unwrap(), task);
        }
    })
    .await
    .expect("Resume admission controls exceeded 90 seconds");
}

#[tokio::test]
async fn resume_local_stop_suppresses_success_without_inventing_cancel_and_settles_durable_intent()
{
    tokio::time::timeout(Duration::from_secs(90), async {
        let (db, a) = runtime("resume-a").await;
        let b = TaskRuntime::new(db.b.clone(), "integration-server", "resume-b");
        let selected = running(&a).await;
        let stop = CancellationToken::new();
        stop.cancel();
        assert_eq!(
            a.transition_resumable_if_current(
                &selected,
                success(),
                Policy::PreserveFailure,
                Some(&stop)
            )
            .await
            .unwrap(),
            selected
        );
        assert_eq!(a.get(selected.task_id).await.unwrap().unwrap(), selected);
        b.cancel(selected.task_id).await.unwrap();
        let cancelled = a
            .transition_resumable_if_current(
                &selected,
                success(),
                Policy::PreserveFailure,
                Some(&stop),
            )
            .await
            .unwrap();
        assert_eq!(cancelled.status, TaskStatus::Cancelled);
        assert!(cancelled.result.is_none());
        let selected = running(&a).await;
        let failed = a
            .transition_resumable(
                selected.task_id,
                TaskTransition::Failed(TaskFailure::new(
                    "real_failure",
                    "failure survives local stop",
                )),
                Policy::PreserveFailure,
                Some(&stop),
            )
            .await
            .unwrap();
        assert_eq!(failed.status, TaskStatus::Failed);
    })
    .await
    .expect("Resume local stop controls exceeded 90 seconds");
}
