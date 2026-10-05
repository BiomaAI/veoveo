//! Controlled storage failures preserve rows; malformed admitted rows test read policy.
use super::TestDb;
use surrealdb::types::{RecordId, SurrealValue, Value};
use veoveo_task_runtime::TaskOwner;

pub fn owner_json(owner: &TaskOwner) -> serde_json::Value {
    veoveo_platform_store::TaskOwnerRecord::try_from(owner)
        .unwrap()
        .into_value()
        .into_json_value()
}

async fn row(db: &TestDb, id: RecordId) -> Value {
    db.a.client()
        .query(include_str!(
            "../queries/support/controlled_storage/read_row.surql"
        ))
        .bind(("row", id))
        .await
        .unwrap()
        .check()
        .unwrap()
        .take(0)
        .unwrap()
}

pub async fn reject_unchanged(
    db: &TestDb,
    id: RecordId,
    sql: &'static str,
    bindings: Vec<(&'static str, Value)>,
) {
    let before = row(db, id.clone()).await;
    let mut query = db.a.client().query(sql);
    for (name, value) in bindings {
        query = query.bind((name, value));
    }
    assert!(
        query.await.unwrap().check().is_err(),
        "controlled write must reject"
    );
    assert_eq!(
        before,
        row(db, id).await,
        "rejected write must preserve the stored row"
    );
}
