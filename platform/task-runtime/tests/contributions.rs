//! Native atomicity of pure owner contributions, including independent recovery settlement.
#[path = "../../../testing/fixtures/store.rs"]
mod store;
use chrono::{DateTime, Utc};
use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use store::module_lanes;
use surrealdb::types::{RecordId, SurrealValue};
use veoveo_modules::*;
use veoveo_platform_store::task_record_id;
use veoveo_task_runtime::{
    CreateTask, OwnedTaskTable, RecoveryClass, TaskContribution, TaskContributions, TaskCreation,
    TaskError, TaskOwner, TaskRuntime, TaskSettlement, TaskSnapshot, TaskStatus, TaskTransition,
};
use veoveo_types::{TaskId, TaskTypeName};
const KIND: TaskTypeName = TaskTypeName::from_static("native");

fn ownership() -> ModuleOwnership {
    ModuleOwnership::new(
        ModuleName::new("contribution-test").unwrap(),
        ModuleLayer::Optional,
        vec![OwnershipClaim::Table(
            TableName::new("contribution_probe").unwrap(),
        )],
    )
    .unwrap()
}
async fn install(db: &store::TestDb) {
    let setup = ModuleSetup::from_ownership(ownership())
        .lane(
            MigrationLane::new(vec![
                Migration::new(
                    MigrationVersion::new(0),
                    MigrationName::new("probe").unwrap(),
                    include_str!("fixtures/migrations/0000_contribution_probe.surql"),
                )
                .unwrap()
                .with_requirements(vec![LaneRequirement::AtLeast {
                    module: ModuleName::new("tasks").unwrap(),
                    version: MigrationVersion::new(0),
                }])
                .unwrap(),
            ])
            .unwrap(),
        )
        .execution(module_lanes::execution("contribution-test").unwrap())
        .requires(vec![LaneRequirement::AtLeast {
            module: ModuleName::new("tasks").unwrap(),
            version: MigrationVersion::new(0),
        }])
        .build()
        .unwrap();
    module_lanes::install(&db.a, vec![setup]).await.unwrap();
}
fn draft(revision: i64) -> CreateTask {
    let owner: TaskOwner = serde_json::from_value(serde_json::json!({
        "principal_key":"pilot","principal_kind":"service","issuer":"https://native.example",
        "subject":"pilot","profile":"operator","tenant_key":"test","data_labels":[],
        "authority":{"work_context":"operations","tenant":"test","membership":"owner",
            "policy_revision":"native","output_policy":{"owner":{"kind":"principal","id":"pilot"}},
            "provenance":{"mode":"automated"}}
    }))
    .unwrap();
    CreateTask {
        task_id: TaskId::new(),
        owner,
        server: "contribution-test".into(),
        task_type: KIND,
        request: serde_json::json!({"revision":revision}),
        recovery_class: RecoveryClass::InterruptedIndeterminate,
        idempotency_key: None,
        ttl_ms: Some(60_000),
        poll_interval_ms: None,
        retention_pins: BTreeSet::new(),
    }
}
#[derive(Clone, SurrealValue)]
struct ProbeIdentity {
    revision: i64,
}
#[derive(SurrealValue)]
struct ProbeSettlement {
    #[surreal(wrap)]
    status: TaskStatus,
    completed_at: DateTime<Utc>,
    marker: String,
}
struct Probe {
    table: OwnedTaskTable,
    calls: Arc<AtomicUsize>,
    operations: [TaskTypeName; 1],
}
impl Probe {
    fn new(calls: Arc<AtomicUsize>) -> Self {
        Self {
            table: OwnedTaskTable::new(&ownership(), TableName::new("contribution_probe").unwrap())
                .unwrap(),
            calls,
            operations: [KIND],
        }
    }
}
impl TaskContributions for Probe {
    fn task_types(&self) -> &[TaskTypeName] {
        &self.operations
    }
    fn table(&self) -> &OwnedTaskTable {
        &self.table
    }
    fn created(&self, creation: TaskCreation<'_>) -> Result<TaskContribution, TaskError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        TaskContribution::create(
            self.table.clone(),
            ProbeIdentity {
                revision: creation.draft.request["revision"].as_i64().unwrap(),
            },
        )
    }
    fn settled(
        &self,
        current: &TaskSnapshot,
        settlement: TaskSettlement<'_>,
        completed_at: DateTime<Utc>,
    ) -> Result<TaskContribution, TaskError> {
        let (status, marker) = match settlement {
            TaskSettlement::Succeeded { result } => (
                TaskStatus::Succeeded,
                if result["reject"] == true {
                    "reject"
                } else {
                    "success"
                },
            ),
            TaskSettlement::Failed { .. } => (TaskStatus::Failed, "failure"),
            TaskSettlement::Cancelled => (TaskStatus::Cancelled, "cancelled"),
        };
        TaskContribution::settle(
            self.table.clone(),
            ProbeIdentity {
                revision: current.request["revision"].as_i64().unwrap(),
            },
            ProbeSettlement {
                status,
                completed_at,
                marker: marker.into(),
            },
        )
    }
}
fn runtime(db: &store::TestDb, worker: &str, calls: Arc<AtomicUsize>) -> TaskRuntime {
    TaskRuntime::new(db.a.clone(), "contribution-test", worker)
        .requiring_contributions([KIND])
        .unwrap()
        .bind_contributions(Arc::new(Probe::new(calls)))
        .unwrap()
}
async fn rows(db: &store::TestDb, task: TaskId) -> Vec<surrealdb::types::Value> {
    let mut response =
        db.b.client()
            .query(include_str!("queries/contributions/rows/statement_1.surql"))
            .bind(("task", task_record_id(task)))
            .await
            .unwrap()
            .check()
            .unwrap();
    response.take(0).unwrap()
}
async fn count(db: &store::TestDb, table: &str) -> usize {
    let mut response =
        db.b.client()
            .query(include_str!(
                "queries/contributions/count/statement_1.surql"
            ))
            .bind(("table", table.to_owned()))
            .await
            .unwrap()
            .check()
            .unwrap();
    response.take::<Option<usize>>(0).unwrap().unwrap_or(0)
}

#[tokio::test]
async fn creation_is_atomic_idempotent_and_requires_a_bound_owner() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = store::TestDb::new().await;
        install(&db).await;
        let calls = Arc::new(AtomicUsize::new(0));
        let runtime = runtime(&db, "worker", calls.clone());
        let mut rejected = draft(-1);
        rejected.idempotency_key = Some("rejected".into());
        assert!(runtime.create(rejected).await.is_err());
        assert_eq!(count(&db, "task").await, 0);
        assert_eq!(count(&db, "task_idempotency").await, 0);
        assert_eq!(count(&db, "contribution_probe").await, 0);
        let mut first = draft(1);
        first.idempotency_key = Some("same".into());
        let created = runtime.create(first).await.unwrap();
        assert!(created.created);
        let before = calls.load(Ordering::SeqCst);
        let mut replay = draft(2);
        replay.idempotency_key = Some("same".into());
        let replay = runtime.create(replay).await.unwrap();
        assert!(!replay.created);
        assert_eq!(replay.snapshot.task_id, created.snapshot.task_id);
        assert_eq!(calls.load(Ordering::SeqCst), before);
        assert_eq!(rows(&db, created.snapshot.task_id).await.len(), 1);
        let unbound = TaskRuntime::new(db.b.clone(), "contribution-test", "unbound")
            .requiring_contributions([KIND])
            .unwrap();
        assert!(matches!(
            unbound.create(draft(3)).await,
            Err(TaskError::ContributionUnbound(_))
        ));
        assert!(matches!(
            unbound.cancel(created.snapshot.task_id).await,
            Err(TaskError::ContributionUnbound(_))
        ));
        assert!(matches!(
            unbound.recover().await,
            Err(TaskError::ContributionUnbound(_))
        ));
        assert_eq!(
            runtime
                .get(created.snapshot.task_id)
                .await
                .unwrap()
                .unwrap()
                .status,
            TaskStatus::Queued
        );
        assert!(
            runtime
                .clone()
                .bind_contributions(Arc::new(Probe::new(calls)))
                .is_err()
        );
        let mut a = draft(7);
        a.idempotency_key = Some("race".into());
        let mut b = draft(8);
        b.idempotency_key = Some("race".into());
        let (a, b) = tokio::join!(runtime.create(a), runtime.create(b));
        let (a, b) = (a.unwrap(), b.unwrap());
        assert_ne!(a.created, b.created);
        assert_eq!(a.snapshot.task_id, b.snapshot.task_id);
        assert_eq!(rows(&db, a.snapshot.task_id).await.len(), 1);
        assert_eq!(count(&db, "task").await, 2);
        assert_eq!(count(&db, "contribution_probe").await, 2);
    })
    .await
    .expect("Task creation contribution qualification exceeded 90 seconds");
}

#[tokio::test]
async fn settlement_rolls_back_and_stale_or_expired_leases_cannot_write() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = store::TestDb::new().await;
        install(&db).await;
        let runtime = runtime(&db, "worker", Arc::default());
        let id = runtime.create(draft(1)).await.unwrap().snapshot.task_id;
        let claimed = runtime
            .claim(id, Duration::from_secs(60))
            .await
            .unwrap()
            .snapshot;
        let rejected = TaskTransition::Succeeded { result_uri: Some(veoveo_types::ResourceUri::new("fixture://products/rejected").unwrap()),
            message: "rejected".into(),
            result: serde_json::json!({"reject":true}),
        };
        assert!(
            runtime
                .transition_if_current(&claimed, rejected)
                .await
                .is_err()
        );
        assert_eq!(
            runtime.get(id).await.unwrap().unwrap().status,
            TaskStatus::Running
        );
        let rolled_back = runtime.get(id).await.unwrap().unwrap();
        assert_eq!(rolled_back.result, None);
        assert_eq!(rolled_back.result_uri, None);
        db.b.client()
            .query(include_str!("queries/contributions/settlement_rolls_back_and_stale_or_expired_leases_cannot_write/statement_1.surql"))
            .bind(("row", RecordId::new("contribution_probe", id.to_string())))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(
            runtime
                .transition_if_current(&claimed, TaskTransition::Cancelled)
                .await
                .is_err()
        );
        assert_eq!(
            runtime.get(id).await.unwrap().unwrap().status,
            TaskStatus::Running
        );
        let missing_id = id;
        let id = runtime.create(draft(2)).await.unwrap().snapshot.task_id;
        let stale = runtime
            .claim(id, Duration::from_secs(60))
            .await
            .unwrap()
            .snapshot;
        runtime
            .transition(
                id,
                TaskTransition::Running {
                    message: "new revision".into(),
                    progress: 0.5,
                },
            )
            .await
            .unwrap();
        assert!(matches!(
            runtime
                .transition_if_current(&stale, TaskTransition::Cancelled)
                .await,
            Err(TaskError::Conflict(_))
        ));
        let current = runtime.get(id).await.unwrap().unwrap();
        db.b.client()
            .query(include_str!("queries/contributions/settlement_rolls_back_and_stale_or_expired_leases_cannot_write/statement_2.surql"))
            .bind(("task", task_record_id(id)))
            .bind(("expired", Utc::now() - chrono::Duration::seconds(1)))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(
            runtime
                .transition_if_current(
                    &current,
                    TaskTransition::Succeeded { result_uri: None,
                        message: "late".into(),
                        result: serde_json::json!({})
                    }
                )
                .await
                .is_err()
        );
        let recovered = runtime.recover().await.unwrap();
        assert_eq!(recovered.failed_indeterminate.len(), 1);
        assert_eq!(
            runtime.get(id).await.unwrap().unwrap().status,
            TaskStatus::Failed
        );
        db.b.client()
            .query(include_str!("queries/contributions/settlement_rolls_back_and_stale_or_expired_leases_cannot_write/statement_3.surql"))
            .bind(("task", task_record_id(missing_id)))
            .bind(("expired", Utc::now() - chrono::Duration::seconds(1)))
            .await
            .unwrap()
            .check()
            .unwrap();
        // Independent interrupted-failure recovery also rolls back when its module row is absent.
        assert!(runtime.recover().await.is_err());
        assert_eq!(
            runtime.get(missing_id).await.unwrap().unwrap().status,
            TaskStatus::Running
        );
    })
    .await
    .expect("Task settlement contribution qualification exceeded 90 seconds");
}

#[tokio::test]
async fn recovery_and_cancellation_settle_rows_and_cascade_is_transactional() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = store::TestDb::new().await;
        install(&db).await;
        let replacement = runtime(&db, "replacement", Arc::default());
        let runtime = runtime(&db, "worker", Arc::default());
        let id = runtime.create(draft(1)).await.unwrap().snapshot.task_id;
        runtime.claim(id, Duration::from_secs(2)).await.unwrap();
        let mut recovery = replacement.observe_startup_recovery().await.unwrap();
        use futures::StreamExt;
        assert!(recovery.next().await.unwrap().unwrap().failed_indeterminate.is_empty());
        let report = loop {
            let report = recovery.next().await.unwrap().unwrap();
            if !report.failed_indeterminate.is_empty() { break report; }
        };
        assert_eq!(report.failed_indeterminate.len(), 1);
        assert_eq!(
            runtime.get(id).await.unwrap().unwrap().status,
            TaskStatus::Failed
        );
        let cancelled = runtime.create(draft(2)).await.unwrap().snapshot.task_id;
        assert_eq!(
            runtime.cancel(cancelled).await.unwrap().status,
            TaskStatus::Cancelled
        );
        assert_eq!(rows(&db, cancelled).await.len(), 1);
        db.b.client()
            .query(include_str!("queries/contributions/recovery_and_cancellation_settle_rows_and_cascade_is_transactional/statement_2.surql"))
            .bind(("task", task_record_id(cancelled)))
            .await
            .unwrap();
        assert!(runtime.get(cancelled).await.unwrap().is_some());
        assert_eq!(rows(&db, cancelled).await.len(), 1);
        db.b.client()
            .query(include_str!("queries/contributions/recovery_and_cancellation_settle_rows_and_cascade_is_transactional/statement_3.surql"))
            .bind(("task", task_record_id(cancelled)))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(rows(&db, cancelled).await.is_empty());
        assert_eq!(rows(&db, id).await.len(), 1);
    })
    .await
    .expect("Task recovery contribution qualification exceeded 90 seconds");
}
