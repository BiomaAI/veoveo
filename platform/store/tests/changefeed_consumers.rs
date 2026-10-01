//! Native push/recovery and typed identity decoding against the pinned database.
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;

use futures::StreamExt;
use std::{collections::BTreeSet, time::Duration};
use surrealdb::types::{RecordId, Uuid as DbUuid};
use veoveo_platform_store::{
    ArtifactChange, ChangefeedConsumerId, ChangefeedCursor, ChangefeedDelivery, ComputerChange,
    PlatformTable, TaskChange, task_record_id,
};
use veoveo_types::TaskId;

const SCHEMA: &str = "
    REMOVE TABLE task; DEFINE TABLE task SCHEMALESS CHANGEFEED 30d;
    REMOVE TABLE artifact_grant; DEFINE TABLE artifact_grant SCHEMALESS CHANGEFEED 30d INCLUDE ORIGINAL;
    REMOVE TABLE computer_automation_grant; DEFINE TABLE computer_automation_grant SCHEMALESS CHANGEFEED 30d INCLUDE ORIGINAL;
";

#[tokio::test]
async fn native_source_catches_baseline_races_restarts_and_delete_parents() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db =
            fixture::TestDb::with_backend_and_schema(fixture::StoreBackend::RocksDb, SCHEMA).await;
        let consumer = ChangefeedConsumerId::new("qualification/native-reader").unwrap();
        let tables = vec![
            PlatformTable::Task,
            PlatformTable::ArtifactGrant,
            PlatformTable::ComputerAutomationGrant,
        ];
        let mut changes =
            db.a.observe_changes(tables.clone(), ChangefeedCursor::initial());
        let first = changes.next().await.unwrap().unwrap();
        assert!(matches!(first, ChangefeedDelivery::Reconcile { .. }));
        db.a.checkpoint_changes(&consumer, first.cursor())
            .await
            .unwrap();
        let task = TaskId::from_uuid(uuid::Uuid::now_v7());
        let artifact = veoveo_artifact_contract::ArtifactId::new();
        let computer = veoveo_computers_contract::ComputerId::new();
        let grant = veoveo_computers_contract::AutomationGrantId::new();
        let artifact_grant = RecordId::new("artifact_grant", DbUuid::from(uuid::Uuid::now_v7()));
        let computer_grant =
            RecordId::new("computer_automation_grant", DbUuid::from(grant.into_uuid()));
        // Writes occur while the source is paused at its baseline yield.
        db.b.client()
            .query(
                "BEGIN TRANSACTION;
            CREATE ONLY $task SET content = 'never decoded by the identity reader';
            CREATE ONLY $artifact_grant SET in = $artifact;
            CREATE ONLY $computer_grant SET computer_id = $computer, grant_id = $grant;
            COMMIT TRANSACTION;",
            )
            .bind(("task", task_record_id(task)))
            .bind(("artifact_grant", artifact_grant.clone()))
            .bind((
                "artifact",
                RecordId::new("artifact_occurrence", DbUuid::from(artifact.as_uuid())),
            ))
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
                    if let Some(change) = ArtifactChange::decode(&entry).unwrap() {
                        assert_eq!(change.artifact_id, artifact);
                        found.insert("artifact");
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
            if found.len() == 3 {
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
        db.b.client()
            .query("BEGIN TRANSACTION; DELETE ONLY $a; DELETE ONLY $c; COMMIT TRANSACTION;")
            .bind(("a", artifact_grant))
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
        while parents.len() < 2 {
            if let ChangefeedDelivery::Changes { entries, .. } =
                resumed.next().await.unwrap().unwrap()
            {
                for entry in entries {
                    if let Some(change) = ArtifactChange::decode(&entry).unwrap() {
                        assert_eq!(change.artifact_id, artifact);
                        parents.insert("artifact");
                    }
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

#[tokio::test]
async fn old_checkpoint_requests_current_state_and_checkpoints_do_not_wake_the_source() {
    tokio::time::timeout(Duration::from_secs(45), async {
        let db = fixture::TestDb::new().await;
        let old = ChangefeedCursor::from_instant(chrono::Utc::now() - chrono::TimeDelta::days(8));
        let mut changes = db.a.observe_changes(vec![PlatformTable::Task], old);
        let baseline = changes.next().await.unwrap().unwrap();
        assert!(matches!(baseline, ChangefeedDelivery::Reconcile { .. }));
        assert!(baseline.cursor() > old);
        let consumer = ChangefeedConsumerId::new("qualification/idle-reader").unwrap();
        db.a.checkpoint_changes(&consumer, baseline.cursor())
            .await
            .unwrap();
        // Initial schema pages may advance the feed. Once drained, an idle
        // observer must remain asleep, including after its own acknowledgement.
        loop {
            match tokio::time::timeout(Duration::from_millis(300), changes.next()).await {
                Err(_) => break,
                Ok(Some(Ok(ChangefeedDelivery::Changes { entries, cursor }))) => {
                    assert!(entries.is_empty());
                    db.a.checkpoint_changes(&consumer, cursor).await.unwrap();
                }
                other => panic!("unexpected idle delivery: {other:?}"),
            }
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(300), changes.next())
                .await
                .is_err()
        );
    })
    .await
    .expect("idle changefeed qualification exceeded 45 seconds");
}
