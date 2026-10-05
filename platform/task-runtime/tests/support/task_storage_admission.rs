//! Schema rejection controls share the existing Task fixture and own no lifecycle.
use surrealdb::types::Value;
use veoveo_platform_store::{PlatformStore, TaskRecord, task_record_id};
use veoveo_types::TaskId;

pub async fn row(store: &PlatformStore, task: TaskId) -> Value {
    store
        .client()
        .query(include_str!("../queries/storage/read.surql"))
        .bind(("task", task_record_id(task)))
        .await
        .unwrap()
        .check()
        .unwrap()
        .take(0)
        .unwrap()
}

pub async fn rejected(store: &PlatformStore, task: TaskId, before: Value, error: surrealdb::Error) {
    let message = error.to_string();
    assert!(
        message.contains("field"),
        "expected field admission rejection: {message}"
    );
    assert_eq!(
        row(store, task).await,
        before,
        "rejected control mutation changed Task"
    );
    // Remove the unchanged positive row from the later denied-row page matrix.
    let _: Option<TaskRecord> = store.client().delete(task_record_id(task)).await.unwrap();
}
