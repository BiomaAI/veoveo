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

#[tokio::test]
async fn task_pages_filter_before_limit_and_resume_creation_time_ties() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        let runtime = TaskRuntime::new(db.a.clone(), "integration-server", "reader");
        let writer = TaskRuntime::new(db.b.clone(), "integration-server", "writer");
        // These records sort before the requested page. They must not consume
        // its limit or be decoded, even when their input envelope is malformed.
        for exclusion in ["labels", "principal", "profile", "tenant", "type"] {
            let mut input = draft("analysis", RecoveryClass::Resume);
            match exclusion {
                "labels" => {
                    input.owner.data_labels.insert("restricted".into());
                }
                "principal" => {
                    input.owner.principal_key = "another-principal".into();
                    input.owner.subject = "another-subject".into();
                }
                "profile" => {
                    input.owner.profile = "another-profile".into();
                }
                "tenant" => {
                    input.owner.tenant_key = Some("another-tenant".into());
                    input.owner.authority.tenant = TenantId::new("another-tenant").unwrap();
                }
                "type" => {
                    input.task_type = "unrelated".into();
                }
                _ => unreachable!(),
            }
            let task = writer
                .create(input)
                .await
                .unwrap_or_else(|error| panic!("creating excluded {exclusion} task: {error}"))
                .snapshot;
            db.b.client()
                .query("UPDATE ONLY $id SET request.input = NONE;")
                .bind(("id", task.task_id.record_id()))
                .await
                .unwrap()
                .check()
                .unwrap();
        }
        let at = chrono::Utc::now();
        let mut expected = Vec::new();
        for _ in 0..5 {
            let task = writer
                .create(draft("analysis", RecoveryClass::Resume))
                .await
                .unwrap()
                .snapshot;
            db.b.client()
                .query("UPDATE ONLY $id SET created_at = $at;")
                .bind(("id", task.task_id.record_id()))
                .bind(("at", at))
                .await
                .unwrap()
                .check()
                .unwrap();
            expected.push(task.task_id);
        }
        expected.sort();
        let mut after = None;
        let mut actual = Vec::new();
        for length in [2, 2, 1] {
            let page = runtime
                .list_page_for_owner(&owner(), &["analysis"], after.as_ref(), 2)
                .await
                .unwrap();
            assert_eq!(page.items.len(), length);
            actual.extend(page.items.into_iter().map(|task| task.task_id));
            after = page.next_cursor;
        }
        assert_eq!(actual, expected);
        assert!(after.is_none());
        let first = runtime
            .list_page_for_owner(&owner(), &["analysis"], None, 2)
            .await
            .unwrap();
        let mut stranger = owner();
        stranger.principal_key = "no-records".into();
        assert!(
            runtime
                .list_page_for_owner(&stranger, &["analysis"], first.next_cursor.as_ref(), 2)
                .await
                .unwrap()
                .items
                .is_empty()
        );
        assert!(
            runtime
                .list_page_for_owner(&owner(), &[], None, 2)
                .await
                .is_err()
        );
        assert!(
            runtime
                .list_page_for_owner(&owner(), &["analysis"], None, 1001)
                .await
                .is_err()
        );
        // Both forms share the deterministic installation tenant record, but
        // TaskOwner::allows distinguishes absence from an explicit tenant.
        let mut installation_owner = owner();
        installation_owner.tenant_key = None;
        installation_owner.authority.tenant = TenantId::new("installation").unwrap();
        let mut input = draft("analysis", RecoveryClass::Resume);
        input.owner = installation_owner.clone();
        let absent = writer.create(input.clone()).await.unwrap().snapshot.task_id;
        input.task_id = veoveo_task_runtime::TaskId::new();
        input.owner.tenant_key = Some("installation".into());
        let explicit_owner = input.owner.clone();
        let explicit = writer.create(input).await.unwrap().snapshot.task_id;
        for (caller, expected) in [(installation_owner, absent), (explicit_owner, explicit)] {
            let page = runtime
                .list_page_for_owner(&caller, &["analysis"], None, 2)
                .await
                .unwrap();
            assert_eq!(page.items.len(), 1);
            assert_eq!(page.items[0].task_id, expected);
        }
    })
    .await
    .expect("task pagination qualification exceeded 60 seconds");
}
