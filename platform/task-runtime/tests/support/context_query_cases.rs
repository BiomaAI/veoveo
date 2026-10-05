use super::*;
use veoveo_task_runtime::{
    TaskError, TaskInputRequest, cancel_durable_task, get_durable_task, update_durable_task,
};
use veoveo_types::TaskTypeName;

const SELECTED: TaskTypeName = TaskTypeName::from_static("selected");

#[tokio::test]
async fn context_agreement_precedes_decode_limits_and_subscription_admission() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        let runtime = TaskRuntime::new(db.a.clone(), "integration-server", "reader");
        let mut foreign = draft(SELECTED.as_str(), RecoveryClass::Resume);
        foreign.owner.authority.work_context = WorkContextId::parse("another-context").unwrap();
        let foreign_owner = foreign.owner.clone();
        let foreign = runtime.create(foreign).await.unwrap().snapshot;
        let query = runtime
            .for_owner(&owner())
            .of_type(SELECTED)
            .in_work_context()
            .unwrap();
        assert!(
            runtime
                .for_owner(&owner())
                .get(foreign.task_id)
                .await
                .unwrap()
                .is_some()
        );
        assert!(query.get(foreign.task_id).await.unwrap().is_none());
        assert!(
            runtime
                .for_owner(&foreign_owner)
                .in_work_context()
                .unwrap()
                .get(foreign.task_id)
                .await
                .unwrap()
                .is_some()
        );
        db.b.client()
            .query(include_str!("../queries/support/context_query_cases/context_agreement_precedes_decode_limits_and_subscription_admission/statement_1.surql"))
            .bind(("authority", serde_json::to_value(authority()).unwrap()))
            .await
            .unwrap()
            .check()
            .unwrap();
        let mut excluded = vec![foreign.task_id];
        // Each row disagrees at a different stored authority location. Its
        // malformed payload proves rejection happens in SQL, before decoding.
        for (assignment, sql) in [
("work_context case 1", include_str!("../queries/support/context_query_cases/context_agreement_precedes_decode_limits_and_subscription_admission/mutation_01.surql")),
("profile case 2", include_str!("../queries/support/context_query_cases/context_agreement_precedes_decode_limits_and_subscription_admission/mutation_02.surql")),
("owner_context.profile case 3", include_str!("../queries/support/context_query_cases/context_agreement_precedes_decode_limits_and_subscription_admission/mutation_03.surql")),
("owner_context.principal_key case 4", include_str!("../queries/support/context_query_cases/context_agreement_precedes_decode_limits_and_subscription_admission/mutation_04.surql")),
("owner_context.data_labels case 5", include_str!("../queries/support/context_query_cases/context_agreement_precedes_decode_limits_and_subscription_admission/mutation_05.surql")),
("owner_context.data_labels case 6", include_str!("../queries/support/context_query_cases/context_agreement_precedes_decode_limits_and_subscription_admission/mutation_06.surql")),
("owner_context.data_labels case 7", include_str!("../queries/support/context_query_cases/context_agreement_precedes_decode_limits_and_subscription_admission/mutation_07.surql")),
("owner_context.data_labels case 8", include_str!("../queries/support/context_query_cases/context_agreement_precedes_decode_limits_and_subscription_admission/mutation_08.surql")),
("owner_context.authority case 9", include_str!("../queries/support/context_query_cases/context_agreement_precedes_decode_limits_and_subscription_admission/mutation_09.surql")),
("authority.context_key case 10", include_str!("../queries/support/context_query_cases/context_agreement_precedes_decode_limits_and_subscription_admission/mutation_10.surql")),
("owner_context.authority.work_context case 11", include_str!("../queries/support/context_query_cases/context_agreement_precedes_decode_limits_and_subscription_admission/mutation_11.surql")),
("owner_context.authority.tenant case 12", include_str!("../queries/support/context_query_cases/context_agreement_precedes_decode_limits_and_subscription_admission/mutation_12.surql")),
("owner_context.authority.work_context case 13", include_str!("../queries/support/context_query_cases/context_agreement_precedes_decode_limits_and_subscription_admission/mutation_13.surql")),
("task_type case 14", include_str!("../queries/support/context_query_cases/context_agreement_precedes_decode_limits_and_subscription_admission/mutation_14.surql"))
] {
            let id = runtime
                .create(draft(SELECTED.as_str(), RecoveryClass::Resume))
                .await
                .unwrap()
                .snapshot
                .task_id;
            let before = task_storage_admission::row(&db.b, id).await;
            let mutation = db.b.client()
                .query(sql)
                .bind(("id", task_record_id(id)))
                .bind((
                    "foreign_context",
                    veoveo_platform_store::deterministic_work_context_id(
                        "integration-tenant",
                        "another-context",
                    )
                    .unwrap()
                    .record_id(),
                ))
                .await
                .unwrap()
                .check();
            if matches!(assignment, "owner_context.data_labels case 6" | "owner_context.data_labels case 7" | "owner_context.data_labels case 8" | "owner_context.authority case 9" | "owner_context.authority.work_context case 13") {
                task_storage_admission::rejected(&db.b, id, before, mutation.unwrap_err()).await;
                continue;
            }
            mutation.unwrap();
            assert!(runtime.get(id).await.is_err());
            assert!(query.get(id).await.unwrap().is_none());
            assert!(matches!(
                query.cancel(id).await,
                Err(TaskError::NotFound(_))
            ));
            let mut result =
                db.b.client()
                    .query(include_str!("../queries/support/context_query_cases/context_agreement_precedes_decode_limits_and_subscription_admission/statement_2.surql"))
                    .bind(("id", task_record_id(id)))
                    .await
                    .unwrap()
                    .check()
                    .unwrap();
            assert_eq!(
                result
                    .take::<Option<veoveo_task_runtime::TaskStatus>>(0)
                    .unwrap(),
                Some(veoveo_task_runtime::TaskStatus::Queued)
            );
            excluded.push(id);
        }
        let mut expected = Vec::new();
        for _ in 0..2 {
            expected.push(
                runtime
                    .create(draft(SELECTED.as_str(), RecoveryClass::Resume))
                    .await
                    .unwrap()
                    .snapshot
                    .task_id,
            );
        }
        let first = query.page(None, 1).await.unwrap();
        assert_eq!(first.items.len(), 1);
        assert_eq!(first.items[0].task_id, expected[0]);
        let second = query.page(first.next_cursor.as_ref(), 1).await.unwrap();
        assert_eq!(second.items.len(), 1);
        assert_eq!(second.items[0].task_id, expected[1]);
        assert!(second.next_cursor.is_none());
        excluded.extend(expected.iter().copied());
        let mut subscription = query.subscribe(&excluded).await.unwrap();
        assert_eq!(
            subscription
                .accepted_task_ids
                .into_iter()
                .collect::<BTreeSet<_>>(),
            expected.iter().copied().collect()
        );
        let mut observed = BTreeSet::new();
        for _ in 0..2 {
            observed.insert(
                subscription
                    .updates
                    .next()
                    .await
                    .unwrap()
                    .unwrap()
                    .snapshot
                    .task_id,
            );
        }
        assert_eq!(observed, expected.into_iter().collect());

        let mut invalid = owner();
        invalid.authority.tenant = TenantId::parse("another-tenant").unwrap();
        assert!(matches!(
            runtime.for_owner(&invalid).in_work_context(),
            Err(TaskError::InvalidAuthority(_))
        ));
    })
    .await
    .expect("context selection exceeded 60 seconds");
}

#[tokio::test]
async fn owner_cancellation_preserves_provider_uncertainty_and_current_profile() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        let runtime = TaskRuntime::new(db.a.clone(), "integration-server", "reader");
        let query = runtime.for_owner(&owner()).in_work_context().unwrap();
        for recovery in [RecoveryClass::Resume, RecoveryClass::ProviderWait] {
            let task = runtime
                .create(draft(SELECTED.as_str(), recovery))
                .await
                .unwrap()
                .snapshot;
            let mut wrong_profile = owner();
            wrong_profile.profile = "another-profile".into();
            assert!(matches!(
                runtime
                    .for_owner(&wrong_profile)
                    .in_work_context()
                    .unwrap()
                    .cancel(task.task_id)
                    .await,
                Err(TaskError::NotFound(_))
            ));
            let cancelled = query.cancel(task.task_id).await.unwrap();
            assert_eq!(
                cancelled.status,
                if recovery == RecoveryClass::ProviderWait {
                    veoveo_task_runtime::TaskStatus::CancelRequested
                } else {
                    veoveo_task_runtime::TaskStatus::Cancelled
                }
            );
            let retry = query.cancel(task.task_id).await.unwrap();
            assert_eq!(retry.status, cancelled.status);
            assert_eq!(retry.cancel_requested_at, cancelled.cancel_requested_at);
        }
    })
    .await
    .expect("owner cancellation exceeded 60 seconds");
}

#[tokio::test]
async fn protocol_reads_and_mutations_preserve_the_selected_context() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        let runtime = TaskRuntime::new(db.a.clone(), "integration-server", "worker");
        let query = runtime.for_owner(&owner()).in_work_context().unwrap();
        for allowed in [false, true] {
            let mut input = draft(SELECTED.as_str(), RecoveryClass::Resume);
            if !allowed {
                input.owner.authority.work_context =
                    WorkContextId::parse("another-context").unwrap();
            }
            let id = runtime.create(input).await.unwrap().snapshot.task_id;
            runtime.claim(id, Duration::from_secs(30)).await.unwrap();
            runtime
                .request_input(
                    id,
                    "choice",
                    TaskInputRequest {
                        method: "elicitation/create".into(),
                        params: std::collections::BTreeMap::from([
                            ("message".into(), json!("choose a value")),
                            (
                                "requestedSchema".into(),
                                json!({
                                    "type": "object",
                                    "properties": {"value": {"type": "integer"}},
                                    "required": ["value"]
                                }),
                            ),
                        ]),
                    },
                )
                .await
                .unwrap();
            let read =
                get_durable_task(&query, rmcp::model::GetTaskParams::new(id.to_string())).await;
            let update = update_durable_task(
                &query,
                serde_json::from_value(json!({
                    "taskId": id,
                    "inputResponses": {"choice": {"action": "accept", "content": {"value": 7}}}
                }))
                .unwrap(),
            )
            .await;
            let cancel = cancel_durable_task(&query, id.to_string()).await;
            if allowed {
                read.unwrap();
                update.unwrap();
                cancel.unwrap();
                assert!(runtime.outstanding_inputs(id).await.unwrap().is_empty());
                assert!(
                    runtime
                        .get(id)
                        .await
                        .unwrap()
                        .expect("created Task exists")
                        .cancel_requested_at
                        .is_some()
                );
            } else {
                assert_eq!(read.unwrap_err().message, "unknown task id");
                assert_eq!(update.unwrap_err().message, "unknown task id");
                assert_eq!(cancel.unwrap_err().message, "unknown task id");
                assert!(
                    runtime
                        .outstanding_inputs(id)
                        .await
                        .unwrap()
                        .contains_key("choice")
                );
                assert!(
                    runtime
                        .get(id)
                        .await
                        .unwrap()
                        .expect("created Task exists")
                        .cancel_requested_at
                        .is_none()
                );
            }
        }
    })
    .await
    .expect("context mutation selection exceeded 60 seconds");
}

#[tokio::test]
async fn context_selection_survives_updates_and_store_reconnect_without_events() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = fixture::TestDb::new().await;
        let endpoint = db.a.config().endpoint();
        let switch = connection_switch::ConnectionSwitch::start(
            endpoint.host_str().unwrap().to_owned(), endpoint.port_or_known_default().unwrap(),
        ).await;
        let reader = TaskRuntime::new(db.connect_via(&switch.endpoint).await, "integration-server", "reader");
        let writer = TaskRuntime::new(db.b.clone(), "integration-server", "writer");
        let mut ids = Vec::new();
        for _ in 0..3 {
            let task = writer.create(draft(SELECTED.as_str(), RecoveryClass::Resume)).await.unwrap().snapshot;
            writer.claim(task.task_id, Duration::from_secs(60)).await.unwrap();
            ids.push(task.task_id);
        }
        let query = reader.for_owner(&owner()).of_type(SELECTED).in_work_context().unwrap();
        let mut updates = query.subscribe(&ids).await.unwrap().updates;
        for _ in 0..3 { updates.next().await.unwrap().unwrap(); }
        db.b.client().query(include_str!("../queries/support/context_query_cases/context_selection_survives_updates_and_store_reconnect_without_events/statement_1.surql"))
            .bind(("task", task_record_id(ids[0]))).await.unwrap().check().unwrap();
        writer.transition(ids[2], TaskTransition::Running { progress: 0.5, message: "halfway".into() }).await.unwrap();
        loop {
            let update = updates.next().await.unwrap().unwrap();
            assert_ne!(update.snapshot.task_id, ids[0]);
            if update.snapshot.task_id == ids[2] && update.snapshot.progress == 0.5 { break; }
        }
        switch.set_enabled(false).await;
        db.b.client().query(include_str!("../queries/support/context_query_cases/context_selection_survives_updates_and_store_reconnect_without_events/statement_2.surql"))
            .bind(("task", task_record_id(ids[1]))).await.unwrap().check().unwrap();
        writer.transition(ids[2], TaskTransition::Succeeded { result_uri: None, message: "finished".into(), result: json!({"answer":42}) }).await.unwrap();

        switch.set_enabled(true).await;
        loop {
            let update = updates.next().await.unwrap().unwrap();
            assert_eq!(update.snapshot.task_id, ids[2]);
            if update.snapshot.status == veoveo_task_runtime::TaskStatus::Succeeded {
                assert_eq!(update.snapshot.result, Some(json!({"answer":42})));
                break;
            }
        }
    }).await.expect("context query recovery exceeded 90 seconds");
}
