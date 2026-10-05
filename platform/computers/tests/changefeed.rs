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

const SCHEMA: &str = include_str!("queries/changefeed/changefeed/statement_1.surql");

#[tokio::test]
async fn computer_changes_catch_baseline_races_restarts_and_deleted_grant_parents() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db =
            fixture::TestDb::with_composition(fixture::StoreBackend::RocksDb,
                vec![veoveo_computers::schema::module_setup(fixture::module_lanes::execution("computers").unwrap()).unwrap()],
                veoveo_gateway_catalog::audit_target_registry().unwrap()).await;
        db.admin().await.client().query(SCHEMA).await.unwrap().check().unwrap();
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
            RecordId::new("computer_automation_grant", DbUuid::from(grant.as_uuid()));
        // Writes occur while the source is paused at its baseline yield.
        db.b.client()
            .query(
                include_str!("queries/changefeed/computer_changes_catch_baseline_races_restarts_and_deleted_grant_parents/statement_1.surql"),
            )
            .bind(("task", task_record_id(task)))
            .bind(("computer_grant", computer_grant.clone()))
            .bind(("computer", computer.as_uuid()))
            .bind(("grant", grant.as_uuid()))
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
            .query(include_str!("queries/changefeed/computer_changes_catch_baseline_races_restarts_and_deleted_grant_parents/statement_2.surql"))
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
