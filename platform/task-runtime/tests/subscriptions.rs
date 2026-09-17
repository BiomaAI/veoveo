//! Disposable real-store acceptance, never environment-gated.
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;
use futures::StreamExt;
use serde_json::json;
use std::{collections::BTreeSet, time::Duration};
use veoveo_mcp_contract::{
    AccessSubject, InvocationAuthority, InvocationProvenance, PolicyVersion, PrincipalId, TenantId,
    WorkContextId, WorkContextMembershipLevel, WorkContextOutputPolicy,
};
use veoveo_task_runtime::{
    CreateTask, PrincipalKind, RecoveryClass, TaskOwner, TaskRuntime, TaskTransition,
    subscribe_durable_tasks,
};
fn authority() -> InvocationAuthority {
    let principal = PrincipalId::new("integration-principal").unwrap();
    InvocationAuthority {
        work_context: WorkContextId::new("integration-mission").unwrap(),
        tenant: TenantId::new("integration-tenant").unwrap(),
        membership: WorkContextMembershipLevel::Owner,
        policy_revision: PolicyVersion::new("r1").unwrap(),
        output_policy: WorkContextOutputPolicy {
            owner: AccessSubject::Principal(principal.clone()),
            initial_grants: Vec::new(),
            classification: None,
            data_labels: BTreeSet::new(),
        },
        provenance: InvocationProvenance::Direct {
            initiator: principal,
        },
    }
}

fn owner() -> TaskOwner {
    TaskOwner {
        principal_key: "integration-principal".to_owned(),
        principal_kind: PrincipalKind::User,
        issuer: "https://issuer.integration.example".to_owned(),
        subject: "integration-subject".to_owned(),
        profile: "integration-profile".to_owned(),
        tenant_key: Some("integration-tenant".to_owned()),
        data_labels: BTreeSet::from(["internal".to_owned()]),
        authority: authority(),
    }
}

fn draft(task_type: &str, recovery_class: RecoveryClass) -> CreateTask {
    CreateTask {
        task_id: veoveo_task_runtime::TaskId::new(),
        owner: owner(),
        server: "integration-server".to_owned(),
        task_type: task_type.to_owned(),
        request: json!({"value": 7}),
        recovery_class,
        idempotency_key: None,
        ttl_ms: Some(60_000),
        poll_interval_ms: Some(100),
        retention_pins: BTreeSet::new(),
    }
}

#[tokio::test]
async fn exact_subscriptions_share_wakes_and_observe_other_replicas_without_unrelated_baselines() {
    tokio::time::timeout(Duration::from_secs(45), async {
        let db = fixture::TestDb::new().await;
        let reader = TaskRuntime::new(db.a.clone(), "integration-server", "reader");
        let writer = TaskRuntime::new(db.b.clone(), "integration-server", "writer");
        let unrelated = writer
            .create(draft("unrelated", RecoveryClass::Resume))
            .await
            .unwrap()
            .snapshot;
        // A server-wide baseline would try to decode this unrelated envelope.
        db.b.client()
            .query("UPDATE ONLY $id SET request = {};")
            .bind(("id", unrelated.task_id.record_id()))
            .await
            .unwrap()
            .check()
            .unwrap();
        let target = writer
            .create(draft("target", RecoveryClass::Resume))
            .await
            .unwrap()
            .snapshot;
        let id = target.task_id.to_string();
        let mut first = subscribe_durable_tasks(&reader, owner(), vec![id.clone()])
            .await
            .unwrap()
            .updates;
        let mut second = subscribe_durable_tasks(&reader.clone(), owner(), vec![id.clone()])
            .await
            .unwrap()
            .updates;
        for stream in [&mut first, &mut second] {
            assert_eq!(stream.next().await.unwrap().unwrap().task.task_id, id);
        }
        writer.claim(&id, Duration::from_secs(30)).await.unwrap();
        writer
            .transition(
                &id,
                TaskTransition::Succeeded {
                    message: "finished".into(),
                    result: json!({"value":42}),
                },
            )
            .await
            .unwrap();
        for stream in [&mut first, &mut second] {
            loop {
                let task = stream.next().await.unwrap().unwrap();
                assert_eq!(task.task.task_id, id);
                if task.task.status == rmcp::model::TaskStatus::Completed {
                    break;
                }
            }
        }
        drop(first);
        drop(second);
        let mut resumed = subscribe_durable_tasks(&reader, owner(), vec![id.clone()])
            .await
            .unwrap()
            .updates;
        assert_eq!(
            resumed.next().await.unwrap().unwrap().task.status,
            rmcp::model::TaskStatus::Completed
        );
        let mut stranger = owner();
        stranger.principal_key = "stranger".into();
        let denied = subscribe_durable_tasks(&reader, stranger, vec![id])
            .await
            .unwrap();
        assert!(denied.accepted_task_ids.is_empty());
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn projected_resource_sources_observe_cross_replica_changes_and_reconnect() {
    tokio::time::timeout(Duration::from_secs(30), async {
        use veoveo_platform_store::PlatformTable;
        let db = fixture::TestDb::new().await;
        let writer = TaskRuntime::new(db.b.clone(), "integration-server", "writer");
        let mut first = db.a.resource_changes(vec![PlatformTable::Task]);
        let mut second = db.b.resource_changes(vec![PlatformTable::Task]);
        assert!(first.next().await.is_some());
        assert!(second.next().await.is_some());
        writer
            .create(draft("cross-replica", RecoveryClass::Resume))
            .await
            .unwrap();
        // Each source yields a payload-free invalidation within LIVE latency,
        // independently of the 30-second reconciliation interval.
        for stream in [&mut first, &mut second] {
            assert!(
                tokio::time::timeout(Duration::from_secs(3), stream.next())
                    .await
                    .unwrap()
                    .is_some()
            );
        }
        drop(first);
        let mut reconnected = db.a.resource_changes(vec![PlatformTable::Task]);
        assert!(reconnected.next().await.is_some());
    })
    .await
    .unwrap();
}
