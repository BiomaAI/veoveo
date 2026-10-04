//! Native push/recovery and typed identity decoding against the pinned database.
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;

use futures::StreamExt;
use std::{collections::BTreeSet, time::Duration};
use surrealdb::types::{RecordId, Uuid as DbUuid};
use veoveo_computers::{ComputerChange, schema::ComputerObservationTable};
use veoveo_platform_store::ObservationTable;
use veoveo_platform_store::{
    ChangefeedConsumerId, ChangefeedCursor, ChangefeedDelivery, PlatformTable, TaskChange,
    task_record_id,
};
use veoveo_types::TaskId;

const SCHEMA: &str = "
    REMOVE TABLE task; DEFINE TABLE task SCHEMALESS CHANGEFEED 30d;
    REMOVE TABLE computer_automation_grant; DEFINE TABLE computer_automation_grant SCHEMALESS CHANGEFEED 30d INCLUDE ORIGINAL;
";

#[tokio::test]
async fn computer_changes_catch_baseline_races_restarts_and_deleted_grant_parents() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db =
            fixture::TestDb::with_backend_and_schema(fixture::StoreBackend::RocksDb, SCHEMA).await;
        let consumer = ChangefeedConsumerId::new("qualification/native-reader").unwrap();
        let tables = vec![
            ObservationTable::from(PlatformTable::Task),
            ComputerObservationTable::AutomationGrant.into(),
        ];
        let mut changes =
            db.a.observe_changes(tables.clone(), ChangefeedCursor::initial());
        let first = changes.next().await.unwrap().unwrap();
        assert!(matches!(first, ChangefeedDelivery::Reconcile { .. }));
        db.a.checkpoint_changes(&consumer, first.cursor())
            .await
            .unwrap();
        let task = TaskId::from_uuid(uuid::Uuid::now_v7());
        let computer = veoveo_computers_contract::ComputerId::new();
        let grant = veoveo_computers_contract::AutomationGrantId::new();
        let computer_grant =
            RecordId::new("computer_automation_grant", DbUuid::from(grant.into_uuid()));
        // Writes occur while the source is paused at its baseline yield.
        db.b.client()
            .query(
                "BEGIN TRANSACTION;
            CREATE ONLY $task SET content = 'never decoded by the identity reader';
            CREATE ONLY $computer_grant SET computer_id = $computer, grant_id = $grant;
            COMMIT TRANSACTION;",
            )
            .bind(("task", task_record_id(task)))
            .bind(("computer_grant", computer_grant.clone()))
            .bind(("computer", computer.into_uuid()))
            .bind(("grant", grant.into_uuid()))
            .await
            .unwrap()
            .check()
            .unwrap();
        let mut found = BTreeSet::new();
        let committed = loop {
            let delivery = changes.next().await.unwrap().unwrap();
            let cursor = delivery.cursor();
            if let ChangefeedDelivery::Changes { entries, .. } = delivery {
                for entry in entries {
                    if let Some(change) = TaskChange::decode(&entry).unwrap() {
                        assert_eq!(change.task_id, task);
                        found.insert("task");
                    }
                    if let Some(ComputerChange::Automation {
                        computer: id,
                        grant: g,
                    }) = ComputerChange::decode(&entry).unwrap()
                    {
                        assert_eq!((id, g), (computer, grant));
                        found.insert("computer");
                    }
                }
            }
            if found.len() == 2 {
                break cursor;
            }
        };
        db.a.checkpoint_changes(&consumer, committed).await.unwrap();
        db.a.checkpoint_changes(&consumer, first.cursor())
            .await
            .unwrap();
        assert_eq!(
            db.b.changefeed_checkpoint(&consumer).await.unwrap(),
            committed,
            "a late acknowledgement cannot rewind recovery"
        );
        drop(changes);
        fixture::wait_for_no_live(&db.b, &tables).await;
        db.b.client()
            .query("BEGIN TRANSACTION; DELETE ONLY $c; COMMIT TRANSACTION;")
            .bind(("c", computer_grant))
            .await
            .unwrap()
            .check()
            .unwrap();
        let mut resumed =
            db.a.observe_changes(tables, db.a.changefeed_checkpoint(&consumer).await.unwrap());
        assert!(matches!(
            resumed.next().await.unwrap().unwrap(),
            ChangefeedDelivery::Reconcile { .. }
        ));
        let mut parents = BTreeSet::new();
        while parents.is_empty() {
            if let ChangefeedDelivery::Changes { entries, .. } =
                resumed.next().await.unwrap().unwrap()
            {
                for entry in entries {
                    if let Some(ComputerChange::Automation { computer: id, .. }) =
                        ComputerChange::decode(&entry).unwrap()
                    {
                        assert_eq!(id, computer);
                        parents.insert("computer");
                    }
                }
            }
        }
    })
    .await
    .expect("native changefeed qualification exceeded 60 seconds");
}
