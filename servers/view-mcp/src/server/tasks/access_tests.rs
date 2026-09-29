use super::*;
use veoveo_task_runtime::{
    TaskRuntime, cancel_durable_task, get_durable_task, subscribe_durable_tasks,
};
use veoveo_types::{TenantId, WorkContextId};

use super::test_support::{fixture, identity};

#[tokio::test]
async fn view_task_selection_uses_the_authenticated_work_context() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        let runtime = TaskRuntime::new(db.a.clone(), SERVER_SLUG, "reader");
        let writer = TaskRuntime::new(db.b.clone(), SERVER_SLUG, "writer");
        let caller = identity();
        let mut foreign = caller.clone();
        foreign.authority.work_context = WorkContextId::new("another-context").unwrap();
        let mut ids = Vec::new();
        for identity in [&caller, &foreign] {
            ids.push(
                writer
                    .create(CreateTask {
                        task_id: TaskId::new(),
                        owner: runtime_owner(identity),
                        server: SERVER_SLUG.into(),
                        task_type: ViewTaskKind::CaptureFrame.name(),
                        request: serde_json::json!({"fixture": true}),
                        recovery_class: RecoveryClass::Resume,
                        idempotency_key: None,
                        ttl_ms: Some(60_000),
                        poll_interval_ms: None,
                        retention_pins: BTreeSet::new(),
                    })
                    .await
                    .unwrap()
                    .snapshot
                    .task_id,
            );
        }
        // This qualifies caller-to-query wiring after authentication. It does
        // not execute a renderer, decode a capture or validate a gateway JWT.
        let query = task_query(&runtime, &caller).unwrap();
        assert!(query.get(ids[0]).await.unwrap().is_some());
        assert!(query.get(ids[1]).await.unwrap().is_none());
        assert!(
            task_query(&runtime, &foreign)
                .unwrap()
                .get(ids[1])
                .await
                .unwrap()
                .is_some()
        );
        let subscription =
            subscribe_durable_tasks(&query, ids.iter().map(ToString::to_string).collect())
                .await
                .unwrap();
        assert_eq!(subscription.accepted_task_ids, vec![ids[0].to_string()]);
        assert_eq!(
            get_durable_task(&query, rmcp::model::GetTaskParams::new(ids[1].to_string()))
                .await
                .unwrap_err()
                .message,
            "unknown task id"
        );
        assert_eq!(
            cancel_durable_task(&query, ids[1].to_string())
                .await
                .unwrap_err()
                .message,
            "unknown task id"
        );
        assert!(
            runtime
                .get(&ids[1].to_string())
                .await
                .unwrap()
                .expect("created Task exists")
                .cancel_requested_at
                .is_none()
        );
        let mut invalid = caller;
        invalid.authority.tenant = TenantId::new("another-tenant").unwrap();
        assert_eq!(
            task_query(&runtime, &invalid).err().unwrap().message,
            "invalid View Task authority"
        );
    })
    .await
    .expect("View Task context selection exceeded 60 seconds");
}
