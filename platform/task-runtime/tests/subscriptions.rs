//! Disposable real-store acceptance, never environment-gated.
#[path = "../../../testing/fixtures/connection_switch.rs"]
mod connection_switch;
#[path = "support/context_query_cases.rs"]
mod context_query_cases;
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;
#[path = "support/owner_query_cases.rs"]
mod owner_query_cases;
use futures::StreamExt;
use serde_json::json;
use std::{collections::BTreeSet, time::Duration};
use veoveo_platform_store::task_record_id;
use veoveo_task_runtime::{
    CreateTask, PrincipalKind, RecoveryClass, TaskOwner, TaskRuntime, TaskTransition,
    authorized_snapshot, subscribe_durable_tasks,
};
use veoveo_types::{
    AccessSubject, InvocationProvenance, PolicyVersion, PrincipalId, TaskId, TenantId,
    WorkContextId,
};
use veoveo_types::{InvocationAuthority, WorkContextMembershipLevel, WorkContextOutputPolicy};
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
        task_id: veoveo_types::TaskId::new(),
        owner: owner(),
        server: "integration-server".to_owned(),
        task_type: veoveo_types::TaskTypeName::new(task_type).unwrap(),
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
            .bind(("id", task_record_id(unrelated.task_id)))
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
        let mut first = subscribe_durable_tasks(&reader.for_owner(&owner()), vec![id.clone()])
            .await
            .unwrap()
            .updates;
        let mut second =
            subscribe_durable_tasks(&reader.clone().for_owner(&owner()), vec![id.clone()])
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
        let mut resumed = subscribe_durable_tasks(&reader.for_owner(&owner()), vec![id.clone()])
            .await
            .unwrap()
            .updates;
        assert_eq!(
            resumed.next().await.unwrap().unwrap().task.status,
            rmcp::model::TaskStatus::Completed
        );
        let mut stranger = owner();
        stranger.principal_key = "stranger".into();
        let denied = subscribe_durable_tasks(&reader.for_owner(&stranger), vec![id])
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
                    input.task_type =
                        const { veoveo_types::TaskTypeName::from_static("unrelated") };
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
                .bind(("id", task_record_id(task.task_id)))
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
                .bind(("id", task_record_id(task.task_id)))
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
                .for_owner(&owner())
                .of_type(veoveo_types::TaskTypeName::from_static("analysis"))
                .page(after.as_ref(), 2)
                .await
                .unwrap();
            assert_eq!(page.items.len(), length);
            actual.extend(page.items.into_iter().map(|task| task.task_id));
            after = page.next_cursor;
        }
        assert_eq!(actual, expected);
        assert!(after.is_none());
        let first = runtime
            .for_owner(&owner())
            .of_type(veoveo_types::TaskTypeName::from_static("analysis"))
            .page(None, 2)
            .await
            .unwrap();
        let mut stranger = owner();
        stranger.principal_key = "no-records".into();
        assert!(
            runtime
                .for_owner(&stranger)
                .of_type(veoveo_types::TaskTypeName::from_static("analysis"))
                .page(first.next_cursor.as_ref(), 2)
                .await
                .unwrap()
                .items
                .is_empty()
        );
        assert!(runtime.for_owner(&owner()).of_types([]).is_err());
        assert!(
            runtime
                .for_owner(&owner())
                .of_type(veoveo_types::TaskTypeName::from_static("analysis"))
                .page(None, 1001)
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
        input.task_id = veoveo_types::TaskId::new();
        input.owner.tenant_key = Some("installation".into());
        let explicit_owner = input.owner.clone();
        let explicit = writer.create(input).await.unwrap().snapshot.task_id;
        for (caller, expected) in [(installation_owner, absent), (explicit_owner, explicit)] {
            let page = runtime
                .for_owner(&caller)
                .of_type(veoveo_types::TaskTypeName::from_static("analysis"))
                .page(None, 2)
                .await
                .unwrap();
            assert_eq!(page.items.len(), 1);
            assert_eq!(page.items[0].task_id, expected);
        }
    })
    .await
    .expect("task pagination qualification exceeded 60 seconds");
}

#[tokio::test]
async fn owner_reads_and_subscription_baselines_filter_before_decoding() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        let reader = TaskRuntime::new(db.a.clone(), "integration-server", "reader");
        let writer = TaskRuntime::new(db.b.clone(), "integration-server", "writer");
        let mut ids = Vec::new();
        for mutation in [
            "request.owner.data_labels = ['restricted']",
            "request.owner.principal_key = 'inconsistent'",
            "request.owner.profile = 'inconsistent'",
            "request.owner.tenant_key = 'inconsistent'",
            "owner = principal:other",
            "profile = profile:other",
            "tenant = tenant:other",
            "server = mcp_server:other",
        ] {
            let task = writer
                .create(draft("selected", RecoveryClass::Resume))
                .await
                .unwrap()
                .snapshot;
            db.b.client()
                .query(format!(
                    "UPDATE ONLY $task SET {mutation}, request.input = NONE RETURN NONE;"
                ))
                .bind(("task", task_record_id(task.task_id)))
                .await
                .unwrap()
                .check()
                .unwrap();
            assert!(
                reader
                    .for_owner(&owner())
                    .get(task.task_id)
                    .await
                    .unwrap()
                    .is_none()
            );
            let error = authorized_snapshot(&reader.for_owner(&owner()), &task.task_id.to_string())
                .await
                .unwrap_err();
            assert_eq!(error.message.as_ref(), "unknown task id");
            if mutation == "server = mcp_server:other" {
                assert!(
                    reader
                        .get(&task.task_id.to_string())
                        .await
                        .unwrap()
                        .is_none()
                );
            }
            ids.push(task.task_id.to_string());
        }
        let task = writer
            .create(draft("selected", RecoveryClass::Resume))
            .await
            .unwrap()
            .snapshot;
        assert_eq!(
            reader
                .for_owner(&owner())
                .get(task.task_id)
                .await
                .unwrap()
                .unwrap()
                .task_id,
            task.task_id
        );
        ids.extend([
            task.task_id.to_string(),
            task.task_id.to_string(),
            "bad-id".into(),
            TaskId::new().to_string(),
        ]);
        let page = reader
            .for_owner(&owner())
            .of_type(veoveo_types::TaskTypeName::from_static("selected"))
            .page(None, 1)
            .await
            .unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].task_id, task.task_id);
        assert!(page.next_cursor.is_none());
        let mut subscription = subscribe_durable_tasks(&reader.for_owner(&owner()), ids)
            .await
            .unwrap();
        assert_eq!(subscription.accepted_task_ids, [task.task_id.to_string()]);
        assert_eq!(
            subscription
                .updates
                .next()
                .await
                .unwrap()
                .unwrap()
                .task
                .task_id,
            task.task_id.to_string()
        );
        assert!(
            reader
                .for_owner(&owner())
                .get(TaskId::new())
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            reader
                .for_owner(&owner())
                .subscribe(&vec![task.task_id; 257])
                .await
                .is_err()
        );
        let invalid: TaskId = "0195dabe-7777-4abc-8def-000000000001".parse().unwrap();
        assert!(reader.for_owner(&owner()).get(invalid).await.is_err());
        assert!(
            reader
                .for_owner(&owner())
                .subscribe(&[invalid])
                .await
                .is_err()
        );
        let mut implicit = owner();
        implicit.tenant_key = None;
        implicit.authority.tenant = TenantId::new("installation").unwrap();
        let mut explicit = implicit.clone();
        explicit.tenant_key = Some("installation".into());
        for (allowed, denied) in [(&implicit, &explicit), (&explicit, &implicit)] {
            let mut input = draft("optional-tenant", RecoveryClass::Resume);
            input.owner = allowed.clone();
            let task = writer.create(input).await.unwrap().snapshot;
            assert!(
                reader
                    .for_owner(denied)
                    .get(task.task_id)
                    .await
                    .unwrap()
                    .is_none()
            );
            assert!(
                reader
                    .for_owner(allowed)
                    .get(task.task_id)
                    .await
                    .unwrap()
                    .is_some()
            );
            assert!(
                reader
                    .for_owner(&denied.clone())
                    .subscribe(&[task.task_id])
                    .await
                    .unwrap()
                    .accepted_task_ids
                    .is_empty()
            );
        }
    })
    .await
    .expect("owner Task read qualification exceeded 60 seconds");
}

#[tokio::test]
async fn owner_updates_recheck_authority_and_advance_past_denied_change_pages() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        let reader = TaskRuntime::new(db.a.clone(), "integration-server", "reader");
        let writer = TaskRuntime::new(db.b.clone(), "integration-server", "writer");
        let revoked = writer.create(draft("revoked", RecoveryClass::Resume)).await.unwrap().snapshot;
        let target = writer.create(draft("target", RecoveryClass::Resume)).await.unwrap().snapshot;
        let mut stream = subscribe_durable_tasks(&reader.for_owner(&owner()), vec![revoked.task_id.to_string(), target.task_id.to_string()]).await.unwrap().updates;
        for _ in 0..2 { stream.next().await.unwrap().unwrap(); }
        db.b.client().query("UPDATE ONLY $task SET request.owner.data_labels = ['restricted'], request.input = NONE RETURN NONE;")
            .bind(("task", task_record_id(revoked.task_id))).await.unwrap().check().unwrap();
        // These native Task versions deliberately cannot decode as Task snapshots.
        // Recovery uses only their IDs before current SQL admission.
        for ordinal in 0..257 {
            db.b.client().query("UPDATE ONLY $task SET request.input = { ordinal: $ordinal } RETURN NONE;")
                .bind(("task", task_record_id(revoked.task_id))).bind(("ordinal", ordinal)).await.unwrap().check().unwrap();
        }
        writer.claim(&target.task_id.to_string(), Duration::from_secs(30)).await.unwrap();
        writer.transition(&target.task_id.to_string(), TaskTransition::Succeeded { message: "finished".into(), result: json!({"value":42}) }).await.unwrap();
        loop {
            let update = stream.next().await.unwrap().unwrap();
            assert_eq!(update.task.task_id, target.task_id.to_string());
            if let rmcp::model::TaskPayload::Completed { result } = update.payload {
                assert_eq!(result.get("value"), Some(&json!(42)));
                break;
            }
        }
        // Re-admission uses current SQL policy; an old denial is not cached authority.
        db.b.client().query("UPDATE ONLY $task SET request.owner.data_labels = ['internal'], request.input = {value:7} RETURN NONE;")
            .bind(("task", task_record_id(revoked.task_id))).await.unwrap().check().unwrap();
        loop {
            let update = stream.next().await.unwrap().unwrap();
            if update.task.task_id == revoked.task_id.to_string() { break; }
        }
    }).await.expect("owner Task update qualification exceeded 60 seconds");
}
