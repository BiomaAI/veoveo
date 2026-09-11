use super::*;
use veoveo_computers::{
    api::ResumeUpdateInput,
    maintenance::{MaintenanceObservationAdmission, MaintenanceOperation, MaintenanceRecovery},
};
use veoveo_task_runtime::TaskStatus;

pub(super) fn input(operation: &MaintenanceOperation) -> ResumeUpdateInput {
    ResumeUpdateInput {
        computer_id: operation.computer_id,
        task_id: operation.operation_id,
        request_id: Uuid::now_v7(),
        expected_updated_at: operation.updated_at,
        acknowledged_cancellation_at: None,
    }
}

#[tokio::test]
async fn explicit_windows_retain_dispatch_history_and_exact_retries_never_renew_twice() {
    let db = TestDb::new().await;
    let (a, b, tasks, claim) = journal::queued(&db).await;
    let ticket = a.begin_maintenance_step(&claim).await.unwrap();
    let original = ticket.operation().clone();
    drop(ticket); // Lost dispatch response must be observed, never dispatched again.
    db.a.client().query("UPDATE ONLY $operation SET progress.steps[0].observation_reads = 8, progress.steps[0].last_observation_id = $id, progress.steps[0].next_observation_at = <string>time::now();")
        .bind(("operation",RecordId::new("computer_maintenance",StoreUuid::from(original.operation_id))))
        .bind(("id",Uuid::now_v7().to_string())).await.unwrap().check().unwrap();
    assert!(matches!(
        a.observe_maintenance_step(&claim).await.unwrap(),
        MaintenanceObservationAdmission::RecoveryRequired
    ));
    let paused = a.maintenance_for_claim(&claim).await.unwrap();
    let actor = support::authenticated(&paused.actor);
    let input = input(&paused);
    assert!(a.resume_maintenance(&actor, &input).await.is_err()); // Active worker owns lease.
    tasks.release_observation(&claim).await.unwrap();
    let (one, two) = tokio::join!(
        a.resume_maintenance(&actor, &input),
        b.resume_maintenance(&actor, &input)
    );
    let resumed = one.or(two).unwrap();
    let retried = b.resume_maintenance(&actor, &input).await.unwrap();
    assert_eq!(retried.updated_at, resumed.updated_at);
    assert_eq!(retried.target_instance_id, original.target_instance_id);
    assert_eq!(
        resumed.steps()[0].dispatch_id,
        original.steps()[0].dispatch_id
    );
    assert_eq!(
        resumed.steps()[0].dispatched_at,
        original.steps()[0].dispatched_at
    );
    assert_eq!(resumed.steps()[0].observation_reads, 0);
    assert_eq!(
        resumed.steps()[0].observation_deadline - resumed.steps()[0].observation_started_at,
        chrono::TimeDelta::seconds(180)
    );
    let changed = ResumeUpdateInput {
        expected_updated_at: resumed.updated_at,
        ..input.clone()
    };
    assert!(matches!(
        a.resume_maintenance(&actor, &changed).await,
        Err(ComputerError::RequestConflict)
    ));
    let claim = tasks
        .claim_observation(&resumed.task_id().to_string(), Duration::from_secs(60))
        .await
        .unwrap();
    assert!(b.begin_maintenance_step(&claim).await.is_err());
    let MaintenanceObservationAdmission::Read(read) =
        b.observe_maintenance_step(&claim).await.unwrap()
    else {
        panic!("finite read")
    };
    assert!(!read.is_dispatch());
    assert_eq!(
        read.operation().steps()[0].dispatch_id,
        original.steps()[0].dispatch_id
    );
    assert_eq!(read.operation().steps()[0].observation_reads, 1);
    let before_retry = read.operation().clone();
    drop(read);
    let retry = a.resume_maintenance(&actor, &input).await.unwrap();
    assert_eq!(retry.updated_at, before_retry.updated_at);
    assert_eq!(retry.steps()[0].observation_reads, 1);
    // A second explicit window gets a new request and archives the charged first.
    let paused_again = a
        .pause_maintenance(&claim, MaintenanceRecovery::AuthorityDenied)
        .await
        .unwrap();
    tasks.release_observation(&claim).await.unwrap();
    assert!(
        a.resume_maintenance(
            &actor,
            &ResumeUpdateInput {
                request_id: Uuid::now_v7(),
                ..input.clone()
            }
        )
        .await
        .is_err()
    );
    let second = a
        .resume_maintenance(&actor, &self::input(&paused_again))
        .await
        .unwrap();
    assert_eq!(
        second.steps()[0].dispatch_id,
        original.steps()[0].dispatch_id
    );
    let mut query = db.a.client().query("SELECT VALUE previous_progress.steps[0].observation_reads FROM computer_maintenance_resume ORDER BY created_at; SELECT VALUE retained FROM computer_usage;")
        .await.unwrap().check().unwrap();
    assert_eq!(query.take::<Vec<i64>>(0).unwrap(), vec![8, 1]);
    assert_eq!(query.take::<Vec<i64>>(1).unwrap(), vec![1, 1, 1]);
    let computer = a.get(actor.owner(), input.computer_id).await.unwrap();
    assert_eq!(computer.instance_id(), original.source_instance_id);
    assert_eq!(computer.active_operation, Some(original.operation_id));
}

#[tokio::test]
async fn cancellation_requires_exact_consent_current_recovery_policy_and_private_ownership() {
    let db = TestDb::new().await;
    let (a, b, tasks, claim) = journal::queued(&db).await;
    let _ticket = a.begin_maintenance_step(&claim).await.unwrap();
    let paused = a
        .pause_maintenance(&claim, MaintenanceRecovery::CancellationRequested)
        .await
        .unwrap();
    let cancelled = tasks.cancel(&paused.task_id().to_string()).await.unwrap();
    tasks.release_observation(&claim).await.unwrap();
    let actor = support::authenticated(&paused.actor);
    let mut input = input(&paused);
    assert!(a.resume_maintenance(&actor, &input).await.is_err());
    input.acknowledged_cancellation_at = Some(chrono::Utc::now());
    assert!(a.resume_maintenance(&actor, &input).await.is_err());
    input.acknowledged_cancellation_at = cancelled.cancel_requested_at;
    assert!(
        b.resume_maintenance(&support::authenticated(&owner("bob")), &input)
            .await
            .is_err()
    );
    let mut denied = control();
    denied.policies[0].rules[0]
        .tools
        .remove(&LocalToolName::new("resume_update").unwrap());
    support::policy::install(&db.a, denied).await;
    assert!(matches!(
        b.resume_maintenance(&actor, &input).await,
        Err(ComputerError::Forbidden)
    ));
    support::policy::install(&db.a, control()).await;
    let resumed = b.resume_maintenance(&actor, &input).await.unwrap();
    assert_eq!(resumed.stage, MaintenanceStage::Stopping);
    let task = tasks
        .get(&resumed.task_id().to_string())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(task.status, TaskStatus::Waiting);
    assert_eq!(task.cancel_requested_at, cancelled.cancel_requested_at);
    let recancelled = tasks.cancel(&resumed.task_id().to_string()).await.unwrap();
    let retry = b.resume_maintenance(&actor, &input).await.unwrap();
    assert_eq!(retry.updated_at, resumed.updated_at);
    assert_eq!(
        tasks
            .get(&resumed.task_id().to_string())
            .await
            .unwrap()
            .unwrap()
            .status,
        TaskStatus::CancelRequested
    );
    assert!(recancelled.cancel_requested_at > cancelled.cancel_requested_at);
    let count: Vec<Uuid> =
        db.a.client()
            .query("SELECT VALUE request_id FROM computer_maintenance_resume;")
            .await
            .unwrap()
            .check()
            .unwrap()
            .take(0)
            .unwrap();
    assert_eq!(count, vec![input.request_id]);
}

#[tokio::test]
async fn drained_migration_preserves_original_window_and_all_dispatch_metadata() {
    let db = TestDb::new().await;
    let (a, _b, tasks, claim) = journal::queued(&db).await;
    let ticket = a.begin_maintenance_step(&claim).await.unwrap();
    let original = ticket.operation().clone();
    drop(ticket);
    tasks.release_observation(&claim).await.unwrap();
    // Construct a pre-0071 row in this owned isolated fixture, then apply the exact migration.
    let mut step = serde_json::to_value(&original.steps()[0]).unwrap();
    step.as_object_mut()
        .unwrap()
        .remove("observation_started_at");
    let step: veoveo_platform_store::OpenObject = serde_json::from_value(step).unwrap();
    db.a.client().query("UPDATE ONLY $operation SET progress.steps = [$step]; REMOVE TABLE computer_maintenance_resume;")
        .bind(("operation",RecordId::new("computer_maintenance",StoreUuid::from(original.operation_id))))
        .bind(("step",step)).await.unwrap().check().unwrap();
    assert!(
        a.maintenance(&original.actor, original.operation_id)
            .await
            .is_err()
    );
    db.a.client()
        .query(include_str!(
            "../../../store/migrations/0071_computer_maintenance_resumption.surql"
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    let migrated = a
        .maintenance(&original.actor, original.operation_id)
        .await
        .unwrap();
    assert_eq!(migrated.updated_at, original.updated_at);
    assert_eq!(
        serde_json::to_value(migrated.steps()).unwrap(),
        serde_json::to_value(original.steps()).unwrap()
    );
    assert_eq!(migrated.target_instance_id, original.target_instance_id);
}
