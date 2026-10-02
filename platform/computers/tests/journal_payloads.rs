//! Private payload lifetime, metadata-only feeds and atomic admission on the real schema.
#[path = "support/files.rs"]
mod files;
mod support;

use files::command_fixture as commands;
use std::time::Duration;
use surrealdb::types::{RecordId, Value};
use veoveo_computers::{ComputerError, ComputersStore};
use veoveo_platform_store::{ChangefeedEntry, decode_changefeed_entry};

#[derive(Clone, Copy)]
enum Kind {
    Command,
    File,
}

impl Kind {
    fn journal(self) -> &'static str {
        match self {
            Self::Command => "computer_execution",
            Self::File => "computer_file_transfer",
        }
    }

    fn payload(self) -> &'static str {
        match self {
            Self::Command => "computer_execution_payload",
            Self::File => "computer_file_transfer_payload",
        }
    }

    async fn admit(
        self,
        db: &support::TestDb,
    ) -> Result<(ComputersStore, RecordId), ComputerError> {
        let (store, replica, owner, agent, computer) = files::setup(db).await;
        let id = match self {
            Self::Command => {
                let grant = store
                    .issue_automation_grant(&owner, &support::automation::input(computer))
                    .await
                    .unwrap();
                let command = store
                    .queue_command(
                        &agent,
                        commands::permit(&store, &agent, computer, grant.grant_id).await,
                        veoveo_computers::api::RequestId::new(),
                        &commands::payload("private-payload", 30),
                        &commands::keys(),
                    )
                    .await?;
                replica.ensure_command_task(&command).await.unwrap();
                assert_eq!(replica.pending_commands(None, 1).await.unwrap().len(), 1);
                command.execution_id().into_uuid()
            }
            Self::File => {
                let authority = store
                    .file_transfer_authority(&owner, computer, None)
                    .await
                    .unwrap();
                let file = store
                    .queue_file_transfer(
                        &owner,
                        authority,
                        veoveo_computers::api::RequestId::new(),
                        &files::payload("private-payload"),
                        &files::keys(),
                    )
                    .await?;
                replica.ensure_file_task(&file).await.unwrap();
                assert_eq!(
                    replica.pending_file_transfers(None, 1).await.unwrap().len(),
                    1
                );
                file.transfer_id().into_uuid()
            }
        };
        Ok((
            replica,
            RecordId::new(self.journal(), surrealdb::types::Uuid::from(id)),
        ))
    }
}

async fn payload_value(db: &support::TestDb, payload: &RecordId) -> Option<Value> {
    db.b.client()
        .query("SELECT * FROM ONLY $payload;")
        .bind(("payload", payload.clone()))
        .await
        .unwrap()
        .check()
        .unwrap()
        .take(0)
        .unwrap()
}

#[tokio::test]
async fn payloads_are_readonly_private_and_owned_by_their_journals() {
    tokio::time::timeout(Duration::from_secs(120), async {
        for kind in [Kind::Command, Kind::File] {
            let db = support::TestDb::new().await;
            let (replica, journal) = kind.admit(&db).await.unwrap();
            let payload = RecordId::new(kind.payload(), journal.key.clone());
            let original = payload_value(&db, &payload).await.unwrap();
            assert!(original.get("journal") == Value::RecordId(journal.clone()));
            let info: Value =
                db.a.client()
                    .query("INFO FOR DB;")
                    .await
                    .unwrap()
                    .check()
                    .unwrap()
                    .take(0)
                    .unwrap();
            let Value::String(definition) = info.get("tables").get(kind.payload()) else {
                panic!("payload table missing");
            };
            assert!(!definition.contains("CHANGEFEED"));
            assert!(
                db.a.client()
                    .query("UPDATE ONLY $payload SET sealed.ciphertext = 'forbidden';")
                    .bind(("payload", payload.clone()))
                    .await
                    .unwrap()
                    .check()
                    .is_err()
            );
            assert!(
                Some(&original) == payload_value(&db, &payload).await.as_ref(),
                "read-only payload changed"
            );
            let cursor = db.a.changefeed_head().await.unwrap();
            db.a.client()
                .query("UPDATE ONLY $journal SET updated_at = time::now();")
                .bind(("journal", journal.clone()))
                .await
                .unwrap()
                .check()
                .unwrap();
            let batches = db.b.replay_changes(cursor, 512).await.unwrap();
            let mut matched = 0;
            for batch in batches {
                for value in batch.changes {
                    let change = decode_changefeed_entry(&value).unwrap();
                    assert_ne!(change.table(), Some(kind.payload()));
                    if change.record_id() == Some(&journal)
                        && let ChangefeedEntry::Upsert(row) = change
                    {
                        assert!(row.get("sealed").is_nullish());
                        assert!(row.get("payload") == Value::RecordId(payload.clone()));
                        matched += 1;
                    }
                }
            }
            assert_eq!(matched, 1);
            assert!(
                db.a.client()
                    .query("BEGIN; DELETE $journal; THROW 'qualification_rollback'; COMMIT;")
                    .bind(("journal", journal.clone()))
                    .await
                    .unwrap()
                    .check()
                    .is_err()
            );
            assert!(
                Some(&original) == payload_value(&db, &payload).await.as_ref(),
                "rollback lost the payload"
            );

            // Missing retained input fails recovery without releasing the execution fence.
            db.a.client()
                .query("DELETE $payload;")
                .bind(("payload", payload.clone()))
                .await
                .unwrap()
                .check()
                .unwrap();
            match kind {
                Kind::Command => assert!(replica.pending_commands(None, 1).await.is_err()),
                Kind::File => assert!(replica.pending_file_transfers(None, 1).await.is_err()),
            }
            let fenced: Vec<RecordId> = db
                .a
                .client()
                .query("SELECT VALUE id FROM computer_execution_slot WHERE execution = $journal;")
                .bind(("journal", journal.clone()))
                .await
                .unwrap()
                .check()
                .unwrap()
                .take(0)
                .unwrap();
            assert_eq!(fenced.len(), 1);
            db.a.client()
                .query("CREATE ONLY $payload CONTENT $original RETURN NONE;")
                .bind(("payload", payload.clone()))
                .bind(("original", original))
                .await
                .unwrap()
                .check()
                .unwrap();
            db.a.client()
                .query("DELETE $journal;")
                .bind(("journal", journal))
                .await
                .unwrap()
                .check()
                .unwrap();
            assert!(
                payload_value(&db, &payload).await.is_none(),
                "parent deletion left its payload behind"
            );
        }
    })
    .await
    .expect("payload lifetime qualification exceeded two minutes");
}

#[tokio::test]
async fn payload_failure_rolls_back_journal_slot_request_and_audit() {
    tokio::time::timeout(Duration::from_secs(120), async {
        for kind in [Kind::Command, Kind::File] {
            let schema = format!("DEFINE EVENT qualification_failure ON {} WHEN $event = 'CREATE' THEN {{ THROW 'qualification_payload_failure'; }};", kind.payload());
            let db = support::TestDb::with_backend_and_schema(support::store::StoreBackend::Memory, &schema).await;
            assert!(kind.admit(&db).await.is_err());
            let statement = match kind {
                Kind::Command => "SELECT * FROM computer_execution; SELECT * FROM computer_execution_payload; SELECT * FROM computer_execution_request; SELECT * FROM computer_execution_slot; SELECT * FROM audit_record WHERE activity = 'computer_command';",
                Kind::File => "SELECT * FROM computer_file_transfer; SELECT * FROM computer_file_transfer_payload; SELECT * FROM computer_file_transfer_request; SELECT * FROM computer_execution_slot; SELECT * FROM audit_record WHERE activity = 'computer_file_transfer';",
            };
            let mut response = db.a.client().query(statement).await.unwrap().check().unwrap();
            for index in 0..5 {
                let rows: Vec<Value> = response.take(index).unwrap();
                assert!(rows.is_empty(), "failed payload admission left a committed record");
            }
        }
    }).await.expect("payload rollback qualification exceeded two minutes");
}
