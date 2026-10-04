use veoveo_task_runtime::TaskUsageAccess;
#[path = "../../../testing/fixtures/store.rs"]
mod store;

use std::{collections::BTreeSet, time::Duration};
use veoveo_platform_store::{DomainUsageDraft, DomainUsageKind, OpenObject, task_record_id};
use veoveo_task_runtime::{CreateTask, RecoveryClass, TaskOwner, TaskRuntime};
use veoveo_types::TaskId;

fn owner(tenant: Option<&str>, principal: &str, profile: &str, labels: &[&str]) -> TaskOwner {
    serde_json::from_value(serde_json::json!({
        "principal_key": principal, "principal_kind":"service", "issuer":"https://usage.test",
        "subject":principal, "profile":profile, "tenant_key":tenant, "data_labels":labels,
        "authority": {"work_context":"usage-fixture", "tenant":tenant.unwrap_or("installation"),
            "membership":"contributor", "policy_revision":"test-1",
            "output_policy":{"owner":{"kind":"principal","id":principal}},
            "provenance":{"mode":"automated"}}
    }))
    .unwrap()
}

async fn create(runtime: &TaskRuntime, owner: &TaskOwner, number: u64, rows: usize) -> TaskId {
    let id = TaskId::from_uuid(uuid::Uuid::from_u128(
        0x01950000000070008000000000000000 + u128::from(number),
    ));
    runtime
        .create(CreateTask {
            task_id: id,
            owner: owner.clone(),
            server: runtime.server().to_owned(),
            task_type: const { veoveo_types::TaskTypeName::from_static("usage-fixture") },
            request: serde_json::json!({}),
            recovery_class: RecoveryClass::Resume,
            idempotency_key: None,
            ttl_ms: None,
            poll_interval_ms: None,
            retention_pins: BTreeSet::new(),
        })
        .await
        .unwrap();
    for number in 0..rows {
        runtime
            .platform_store()
            .upsert_domain_usage(DomainUsageDraft {
                task_id: id,
                server: runtime.server().to_owned(),
                source_id: Some(format!("row-{number}")),
                provider_job_id: None,
                model_id: "usage-fixture".into(),
                kind: DomainUsageKind::Actual,
                quantity: Some(1.),
                unit: Some("point".into()),
                amount: None,
                currency: None,
                metadata: OpenObject::default(),
                recorded_at: chrono::Utc::now(),
            })
            .await
            .unwrap();
    }
    id
}

#[tokio::test]
async fn usage_pages_filter_owners_and_linked_tasks_before_grouping_and_limits() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = store::TestDb::new().await;
        let writer = TaskRuntime::new(db.a.clone(), "frames", "writer");
        let foreign_server = TaskRuntime::new(db.a.clone(), "other", "writer");
        let reader = TaskRuntime::new(db.b.clone(), "frames", "reader");
        let caller = owner(Some("tenant-a"), "owner", "operator", &["cui", "mission"]);
        let denied = [
            owner(Some("tenant-a"), "other", "operator", &["cui", "mission"]),
            owner(Some("tenant-a"), "owner", "observer", &["cui", "mission"]),
            owner(Some("tenant-b"), "owner", "operator", &["cui", "mission"]),
            owner(
                Some("tenant-a"),
                "owner",
                "operator",
                &["cui", "mission", "secret"],
            ),
        ];
        for number in 1..=140 {
            let id = if number % 5 == 4 {
                create(&foreign_server, &caller, number, 1).await
            } else {
                create(&writer, &denied[(number % 5) as usize], number, 1).await
            };
            // A denied row must never be decoded as a full Task just to test authority.
            db.a.client()
                .query("UPDATE ONLY $task SET request.input = NONE RETURN NONE;")
                .bind(("task", task_record_id(id)))
                .await
                .unwrap()
                .check()
                .unwrap();
        }
        let mut expected = Vec::new();
        for number in 200..326 {
            expected.push(create(&writer, &caller, number, 2).await);
        }
        let first = reader
            .usage_page(TaskUsageAccess::Owner(&caller), None, 100)
            .await
            .unwrap();
        assert_eq!(first.task_ids, expected[..100]);
        assert_eq!(first.next_task_id, Some(expected[99]));
        let second = reader
            .usage_page(TaskUsageAccess::Owner(&caller), first.next_task_id, 100)
            .await
            .unwrap();
        assert_eq!(second.task_ids, expected[100..]);
        assert_eq!(second.next_task_id, None);
        assert_eq!(
            reader
                .usage(TaskUsageAccess::Owner(&caller), expected[0])
                .await
                .unwrap()
                .len(),
            2
        );
        assert!(
            reader
                .task_visible(TaskUsageAccess::Owner(&caller), expected[0])
                .await
                .unwrap()
        );
        for denied in &denied[..3] {
            assert!(
                reader
                    .usage(TaskUsageAccess::Owner(denied), expected[0])
                    .await
                    .unwrap()
                    .is_empty()
            );
            assert!(
                !reader
                    .task_visible(TaskUsageAccess::Owner(denied), expected[0])
                    .await
                    .unwrap()
            );
            assert!(
                reader
                    .usage_page(TaskUsageAccess::Owner(denied), first.next_task_id, 100)
                    .await
                    .unwrap()
                    .task_ids
                    .is_empty()
            );
        }
        // A caller losing one label cannot continue the earlier page or read its entries.
        let mut partial = caller.clone();
        partial.data_labels.remove("mission");
        assert!(
            reader
                .usage_page(TaskUsageAccess::Owner(&partial), first.next_task_id, 100)
                .await
                .unwrap()
                .task_ids
                .is_empty()
        );
        assert!(
            reader
                .usage(TaskUsageAccess::Owner(&partial), expected[0])
                .await
                .unwrap()
                .is_empty()
        );

        let last = *expected.last().unwrap();
        db.a.client()
            .query("UPDATE ONLY $task SET request.owner.data_labels += 'secret' RETURN NONE;")
            .bind(("task", task_record_id(last)))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert_eq!(
            reader
                .usage_page(TaskUsageAccess::Owner(&caller), first.next_task_id, 100)
                .await
                .unwrap()
                .task_ids
                .len(),
            25
        );
        assert!(
            reader
                .usage(TaskUsageAccess::Owner(&caller), last)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            !reader
                .task_visible(TaskUsageAccess::Owner(&caller), last)
                .await
                .unwrap()
        );
        let mut cleared = caller.clone();
        cleared.data_labels.insert("secret".into());
        assert_eq!(
            reader
                .usage(TaskUsageAccess::Owner(&cleared), last)
                .await
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            reader
                .usage_page(TaskUsageAccess::Owner(&cleared), first.next_task_id, 100)
                .await
                .unwrap()
                .task_ids,
            expected[100..]
        );
        assert!(
            reader
                .usage_page(TaskUsageAccess::Owner(&caller), None, 0)
                .await
                .is_err()
        );
        assert!(
            reader
                .usage_page(TaskUsageAccess::Owner(&caller), None, 1001)
                .await
                .is_err()
        );
    })
    .await
    .expect("usage paging qualification exceeded 90 seconds");
}

#[tokio::test]
async fn usage_reads_reject_orphans_wrong_parent_metadata_and_tenant_aliases() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = store::TestDb::new().await;
        let writer = TaskRuntime::new(db.a.clone(), "frames", "writer");
        let reader = TaskRuntime::new(db.b.clone(), "frames", "reader");
        let anonymous_tenant = owner(None, "owner", "operator", &[]);
        let named_tenant = owner(Some("installation"), "owner", "operator", &[]);
        let first = create(&writer, &anonymous_tenant, 1, 1).await;
        let second = create(&writer, &named_tenant, 2, 1).await;
        assert_eq!(
            reader
                .usage_page(TaskUsageAccess::Owner(&anonymous_tenant), None, 100)
                .await
                .unwrap()
                .task_ids,
            [first]
        );
        assert_eq!(
            reader
                .usage_page(TaskUsageAccess::Owner(&named_tenant), None, 100)
                .await
                .unwrap()
                .task_ids,
            [second]
        );
        assert!(
            reader
                .usage(TaskUsageAccess::Owner(&named_tenant), first)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            reader
                .usage(TaskUsageAccess::Owner(&anonymous_tenant), second)
                .await
                .unwrap()
                .is_empty()
        );
        let pending = create(&writer, &named_tenant, 3, 0).await;
        assert!(
            reader
                .task_visible(TaskUsageAccess::Owner(&named_tenant), pending)
                .await
                .unwrap()
        );
        assert!(
            reader
                .usage(TaskUsageAccess::Owner(&named_tenant), pending)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            !reader
                .task_visible(TaskUsageAccess::Owner(&named_tenant), TaskId::new())
                .await
                .unwrap()
        );

        db.a.client()
            .query("CREATE ONLY usage_policy_probe:owner CONTENT $owner RETURN NONE;")
            .bind(("owner", serde_json::to_value(&named_tenant).unwrap()))
            .await
            .unwrap()
            .check()
            .unwrap();
        for (number, query) in [
            (
                20,
                "UPDATE ONLY $task SET request.owner = usage_policy_probe:owner RETURN NONE;",
            ),
            (
                21,
                "UPDATE ONLY $task SET request.owner.data_labels = NONE RETURN NONE;",
            ),
            (
                22,
                "UPDATE ONLY $task SET request.owner.data_labels = 'mission' RETURN NONE;",
            ),
            (
                23,
                "UPDATE ONLY $task SET request.owner.data_labels = [NONE] RETURN NONE;",
            ),
            (10, "DELETE $task RETURN NONE;"),
            (
                11,
                "UPDATE ONLY $task SET request.owner.principal_key = 'wrong-owner' RETURN NONE;",
            ),
            (
                12,
                "UPDATE ONLY $task SET request.owner.profile = 'wrong-profile' RETURN NONE;",
            ),
            (
                13,
                "UPDATE domain_usage SET server = mcp_server:other WHERE task = $task RETURN NONE;",
            ),
            (
                14,
                "UPDATE domain_usage SET tenant = tenant:wrong WHERE task = $task RETURN NONE;",
            ),
            (
                15,
                "UPDATE ONLY $task SET server = mcp_server:other RETURN NONE;",
            ),
            (
                16,
                "UPDATE ONLY $task SET tenant = tenant:wrong RETURN NONE;",
            ),
        ] {
            let id = create(&writer, &named_tenant, number, 1).await;
            db.a.client()
                .query(query)
                .bind(("task", task_record_id(id)))
                .await
                .unwrap()
                .check()
                .unwrap();
            assert!(
                reader
                    .usage(TaskUsageAccess::Owner(&named_tenant), id)
                    .await
                    .unwrap()
                    .is_empty(),
                "{query}"
            );
        }
        assert_eq!(
            reader
                .usage_page(TaskUsageAccess::Owner(&named_tenant), None, 100)
                .await
                .unwrap()
                .task_ids,
            [second]
        );
    })
    .await
    .expect("usage parent qualification exceeded 60 seconds");
}

#[tokio::test]
async fn context_policy_filters_before_limits_and_requires_all_stored_contexts_to_agree() {
    use veoveo_task_runtime::{
        TaskError,
        TaskUsageAccess::{Owner, WorkContext},
    };
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = store::TestDb::new().await;
        let writer = TaskRuntime::new(db.a.clone(), "optimization", "writer");
        let reader = TaskRuntime::new(db.b.clone(), "optimization", "reader");
        let caller = owner(Some("tenant-a"), "owner", "operator", &["mission"]);
        let mut foreign = caller.clone();
        foreign.authority.work_context = veoveo_types::WorkContextId::parse("other-context").unwrap();
        for number in 1..=105 {
            let id = create(&writer, &foreign, number, 1).await;
            db.a.client().query("UPDATE ONLY $task SET request.input = NONE RETURN NONE;")
                .bind(("task", task_record_id(id))).await.unwrap().check().unwrap();
        }
        let mut expected = Vec::new();
        for number in 200..301 {
            expected.push(create(&writer, &caller, number, 2).await);
        }
        let first = reader.usage_page(WorkContext(&caller), None, 100).await.unwrap();
        assert_eq!(first.task_ids, expected[..100]);
        assert_eq!(first.next_task_id, Some(expected[99]));
        let second = reader.usage_page(WorkContext(&caller), first.next_task_id, 100).await.unwrap();
        assert_eq!(second.task_ids, expected[100..]);
        assert_eq!(second.next_task_id, None);
        assert_eq!(reader.usage(WorkContext(&caller), expected[0]).await.unwrap().len(), 2);
        assert!(reader.usage(WorkContext(&foreign), expected[0]).await.unwrap().is_empty());
        assert!(reader.usage_page(WorkContext(&foreign), first.next_task_id, 100).await.unwrap().task_ids.is_empty());
        assert!(reader.task_visible(WorkContext(&caller), expected[0]).await.unwrap());
        assert!(!reader.task_visible(WorkContext(&foreign), expected[0]).await.unwrap());
        // The explicit owner policy keeps its existing cross-context behavior.
        assert_eq!(reader.usage(Owner(&foreign), expected[0]).await.unwrap().len(), 2);
        let pending = create(&writer, &caller, 400, 0).await;
        assert!(reader.task_visible(WorkContext(&caller), pending).await.unwrap());
        assert!(!reader.task_visible(WorkContext(&foreign), pending).await.unwrap());

        let mut invalid = caller.clone();
        invalid.authority.tenant = veoveo_types::TenantId::parse("wrong-tenant").unwrap();
        assert!(matches!(reader.usage_page(WorkContext(&invalid), None, 100).await, Err(TaskError::InvalidAuthority(_))));
        assert!(matches!(reader.usage(WorkContext(&invalid), pending).await, Err(TaskError::InvalidAuthority(_))));
        assert!(matches!(reader.task_visible(WorkContext(&invalid), pending).await, Err(TaskError::InvalidAuthority(_))));

        db.a.client()
            .query("CREATE ONLY usage_policy_probe:authority CONTENT $authority RETURN NONE;")
            .bind(("authority", serde_json::to_value(&caller.authority).unwrap()))
            .await.unwrap().check().unwrap();
        for (number, query) in [
            (505, "UPDATE ONLY $task SET request.owner.authority = usage_policy_probe:authority RETURN NONE;"),
            (506, "UPDATE ONLY $task SET request.owner.data_labels = NONE RETURN NONE;"),
            (507, "UPDATE ONLY $task SET request.owner.data_labels = 'mission' RETURN NONE;"),
            (508, "UPDATE ONLY $task SET request.owner.data_labels = ['mission', NONE] RETURN NONE;"),
            (500, "UPDATE ONLY $task SET work_context = work_context:wrong RETURN NONE;"),
            (501, "UPDATE ONLY $task SET authority.context_key = 'wrong' RETURN NONE;"),
            (502, "UPDATE ONLY $task SET request.owner.authority.work_context = 'wrong' RETURN NONE;"),
            (503, "UPDATE ONLY $task SET request.owner.authority.tenant = 'wrong' RETURN NONE;"),
            (504, "UPDATE ONLY $task SET request.owner.authority = NONE RETURN NONE;"),
        ] {
            let id = create(&writer, &caller, number, 1).await;
            db.a.client().query(query).bind(("task", task_record_id(id))).await.unwrap().check().unwrap();
            assert!(reader.usage(WorkContext(&caller), id).await.unwrap().is_empty(), "{query}");
            assert!(!reader.task_visible(WorkContext(&caller), id).await.unwrap(), "{query}");
        }
        assert_eq!(reader.usage_page(WorkContext(&caller), first.next_task_id, 100).await.unwrap().task_ids, expected[100..]);
        // Previously returned positions and URIs must re-evaluate the current context.
        let last = expected[100];
        db.a.client().query("UPDATE ONLY $task SET request.owner.authority.work_context = 'other-context' RETURN NONE;")
            .bind(("task", task_record_id(last))).await.unwrap().check().unwrap();
        assert!(reader.usage_page(WorkContext(&caller), first.next_task_id, 100).await.unwrap().task_ids.is_empty());
        assert!(reader.usage(WorkContext(&caller), last).await.unwrap().is_empty());
    }).await.expect("usage context qualification exceeded 90 seconds");
}
