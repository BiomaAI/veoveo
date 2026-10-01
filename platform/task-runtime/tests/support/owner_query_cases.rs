use super::*;
use veoveo_types::TaskTypeName;

const SELECTED: TaskTypeName = TaskTypeName::from_static("selected");
const ALSO_SELECTED: TaskTypeName = TaskTypeName::from_static("also-selected");

#[tokio::test]
async fn operation_selection_precedes_decode_and_limits_on_every_read() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        let reader = TaskRuntime::new(db.a.clone(), "integration-server", "reader");
        let writer = TaskRuntime::new(db.b.clone(), "integration-server", "writer");
        let mut excluded = Vec::new();
        for _ in 0..3 {
            let task = writer
                .create(draft("other-operation", RecoveryClass::Resume))
                .await
                .unwrap()
                .snapshot;
            db.b.client()
                .query("UPDATE ONLY $id SET request.input = NONE RETURN NONE;")
                .bind(("id", task_record_id(task.task_id)))
                .await
                .unwrap()
                .check()
                .unwrap();
            excluded.push(task.task_id);
        }
        let query = reader
            .for_owner(&owner())
            .of_types([SELECTED, ALSO_SELECTED])
            .unwrap();
        for id in &excluded {
            assert!(reader.for_owner(&owner()).get(*id).await.is_err());
            assert!(query.get(*id).await.unwrap().is_none());
            assert_eq!(
                authorized_snapshot(&query, &id.to_string())
                    .await
                    .unwrap_err()
                    .message,
                "unknown task id"
            );
        }
        let mut expected = Vec::new();
        for kind in [SELECTED, ALSO_SELECTED] {
            expected.push(
                writer
                    .create(draft(kind.as_str(), RecoveryClass::Resume))
                    .await
                    .unwrap()
                    .snapshot
                    .task_id,
            );
        }
        let first = query.page(None, 1).await.unwrap();
        assert_eq!(first.items[0].task_id, expected[0]);
        let second = query.page(first.next_cursor.as_ref(), 1).await.unwrap();
        assert_eq!(second.items[0].task_id, expected[1]);
        assert!(second.next_cursor.is_none());
        let mut ids = excluded;
        ids.extend(expected.iter().copied());
        let mut subscription = query.subscribe(&ids).await.unwrap();
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
        assert_eq!(observed, expected.iter().copied().collect());
        assert!(reader.for_owner(&owner()).of_types([]).is_err());
        assert!(
            reader
                .for_owner(&owner())
                .of_types(std::iter::repeat_n(SELECTED, 33))
                .is_err()
        );
        assert!(
            reader
                .for_owner(&owner())
                .of_types(std::iter::repeat_n(SELECTED, 32))
                .is_ok()
        );
    })
    .await
    .expect("operation selection exceeded 60 seconds");
}

#[tokio::test]
async fn operation_selection_survives_updates_and_store_reconnect_without_events() {
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
        let query = reader.for_owner(&owner()).of_type(SELECTED);
        let mut updates = query.subscribe(&ids).await.unwrap().updates;
        for _ in 0..3 { updates.next().await.unwrap().unwrap(); }

        // Current type selection must survive native Task wakes, even if a
        // previously admitted row changes to a malformed unrelated operation.
        db.b.client().query("UPDATE ONLY $task SET task_type = 'other-operation', request.input = NONE RETURN NONE;")
            .bind(("task", task_record_id(ids[0]))).await.unwrap().check().unwrap();
        writer.transition(&ids[2].to_string(), TaskTransition::Running { progress: 0.5, message: "halfway".into() }).await.unwrap();
        loop {
            let update = updates.next().await.unwrap().unwrap();
            assert_ne!(update.snapshot.task_id, ids[0]);
            if update.snapshot.task_id == ids[2] && update.snapshot.progress == 0.5 { break; }
        }

        switch.set_enabled(false).await;
        db.b.client().query("UPDATE ONLY $task SET task_type = 'other-operation', request.input = NONE RETURN NONE;")
            .bind(("task", task_record_id(ids[1]))).await.unwrap().check().unwrap();
        writer.transition(&ids[2].to_string(), TaskTransition::Succeeded { message: "finished".into(), result: json!({"answer":42}) }).await.unwrap();

        switch.set_enabled(true).await;
        loop {
            let update = updates.next().await.unwrap().unwrap();
            assert_eq!(update.snapshot.task_id, ids[2]);
            if update.snapshot.status == veoveo_task_runtime::TaskStatus::Succeeded {
                assert_eq!(update.snapshot.result, Some(json!({"answer":42})));
                break;
            }
        }
    }).await.expect("operation query recovery exceeded 90 seconds");
}
