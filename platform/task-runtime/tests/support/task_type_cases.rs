use super::*;
use veoveo_task_runtime::TaskSnapshot;
use veoveo_types::TaskTypeName;

#[tokio::test]
async fn operation_names_survive_storage_and_reject_unvalidated_admission() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let (db, writer) = runtime("operation-writer").await;
        let reader = TaskRuntime::new(db.b.clone(), "integration-server", "operation-reader");
        let input = draft("checked-operation", RecoveryClass::Resume);
        let kind = input.task_type.clone();
        let mut admission = serde_json::to_value(&input).unwrap();
        admission["task_type"] = json!("invalid operation");
        assert!(serde_json::from_value::<CreateTask>(admission).is_err());

        let task = writer.create(input).await.unwrap().snapshot;
        assert_eq!(task.task_type, kind);
        let selected = reader.for_owner(&owner()).of_type(kind.clone());
        assert_eq!(
            selected.get(task.task_id).await.unwrap().unwrap().task_type,
            kind
        );
        let mut rows =
            db.b.client()
                .query(include_str!("../queries/support/task_type_cases/operation_names_survive_storage_and_reject_unvalidated_admission/statement_1.surql"))
                .bind(("task", task_record_id(task.task_id)))
                .await
                .unwrap()
                .check()
                .unwrap();
        assert_eq!(
            rows.take::<Vec<serde_json::Value>>(0).unwrap(),
            vec![json!({"task_type": "checked-operation"})]
        );
        let wire = serde_json::to_value(&task).unwrap();
        assert_eq!(wire["task_type"], "checked-operation");
        assert_eq!(
            serde_json::from_value::<TaskSnapshot>(wire.clone())
                .unwrap()
                .task_type,
            kind
        );
        let mut invalid = wire;
        invalid["task_type"] = json!("invalid operation");
        assert!(serde_json::from_value::<TaskSnapshot>(invalid).is_err());

        let changes = db
            .committed(veoveo_platform_store::PlatformTable::Task)
            .await;
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0]["task_type"], "checked-operation");

        db.b.client()
            .query(include_str!("../queries/support/task_type_cases/operation_names_survive_storage_and_reject_unvalidated_admission/statement_2.surql"))
            .bind(("task", task_record_id(task.task_id)))
            .await
            .unwrap()
            .check()
            .unwrap();
        // Selection excludes the malformed operation before native TaskRecord decoding.
        assert!(selected.get(task.task_id).await.unwrap().is_none());
        assert!(selected.page(None, 10).await.unwrap().items.is_empty());
        assert!(reader.for_owner(&owner()).get(task.task_id).await.is_err());
        assert!(TaskTypeName::new("invalid operation").is_err());
    })
    .await
    .expect("Task operation qualification exceeded 90 seconds");
}
