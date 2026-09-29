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
        foreign.owner.authority.work_context = WorkContextId::new("another-context").unwrap();
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
        let mut excluded = vec![foreign.task_id];
        // Each row disagrees at a different stored authority location. Its
        // malformed payload proves rejection happens in SQL, before decoding.
        for assignment in [
            "work_context = $foreign_context",
            "authority.context_key = 'another-context'",
            "request.owner.authority.work_context = 'another-context'",
            "request.owner.authority.tenant = 'another-tenant'",
            "request.owner.authority.work_context = NONE",
            "task_type = 'other-operation'",
        ] {
            let id = runtime
                .create(draft(SELECTED.as_str(), RecoveryClass::Resume))
                .await
                .unwrap()
                .snapshot
                .task_id;
            db.b.client()
                .query(format!(
                    "UPDATE ONLY $id SET {assignment}, request.input = NONE RETURN NONE;"
                ))
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
                .check()
                .unwrap();
            assert!(runtime.for_owner(&owner()).get(id).await.is_err());
            assert!(query.get(id).await.unwrap().is_none());
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
        invalid.authority.tenant = TenantId::new("another-tenant").unwrap();
        assert!(matches!(
            runtime.for_owner(&invalid).in_work_context(),
            Err(TaskError::InvalidAuthority(_))
        ));
    })
    .await
    .expect("context selection exceeded 60 seconds");
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
                input.owner.authority.work_context = WorkContextId::new("another-context").unwrap();
            }
            let id = runtime
                .create(input)
                .await
                .unwrap()
                .snapshot
                .task_id
                .to_string();
            runtime.claim(&id, Duration::from_secs(30)).await.unwrap();
            runtime
                .request_input(
                    &id,
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
            let read = get_durable_task(&query, rmcp::model::GetTaskParams::new(id.clone())).await;
            let update = update_durable_task(
                &query,
                serde_json::from_value(json!({
                    "taskId": id,
                    "inputResponses": {"choice": {"action": "accept", "content": {"value": 7}}}
                }))
                .unwrap(),
            )
            .await;
            let cancel = cancel_durable_task(&query, id.clone()).await;
            if allowed {
                read.unwrap();
                update.unwrap();
                cancel.unwrap();
                assert!(runtime.outstanding_inputs(&id).await.unwrap().is_empty());
                assert!(
                    runtime
                        .get(&id)
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
                        .outstanding_inputs(&id)
                        .await
                        .unwrap()
                        .contains_key("choice")
                );
                assert!(
                    runtime
                        .get(&id)
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
            writer.claim(&task.task_id.to_string(), Duration::from_secs(60)).await.unwrap();
            ids.push(task.task_id);
        }
        let query = reader.for_owner(&owner()).of_type(SELECTED).in_work_context().unwrap();
        let mut updates = query.subscribe(&ids).await.unwrap().updates;
        for _ in 0..3 { updates.next().await.unwrap().unwrap(); }
        db.b.client().query("UPDATE ONLY $task SET authority.context_key = 'another-context', request.input = NONE RETURN NONE; CREATE outbox_event SET aggregate_type = 'task', aggregate_id = $id, event_type = 'task.fixture', schema_version = 3, payload = { snapshot: { server: 'integration-server' } } RETURN NONE;")
            .bind(("task", task_record_id(ids[0]))).bind(("id", ids[0].to_string())).await.unwrap().check().unwrap();
        writer.transition(&ids[2].to_string(), TaskTransition::Running { progress: 0.5, message: "halfway".into() }).await.unwrap();
        loop {
            let update = updates.next().await.unwrap().unwrap();
            assert_ne!(update.snapshot.task_id, ids[0]);
            if update.snapshot.task_id == ids[2] && update.snapshot.progress == 0.5 { break; }
        }
        switch.set_enabled(false).await;
        db.b.client().query("UPDATE ONLY $task SET request.owner.authority.work_context = 'another-context', request.input = NONE RETURN NONE;")
            .bind(("task", task_record_id(ids[1]))).await.unwrap().check().unwrap();
        writer.transition(&ids[2].to_string(), TaskTransition::Succeeded { message: "finished".into(), result: json!({"answer":42}) }).await.unwrap();
        db.b.client().query("DELETE outbox_event WHERE aggregate_type = 'task' RETURN NONE;").await.unwrap().check().unwrap();
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
