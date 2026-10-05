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
                command.execution_id().as_uuid()
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
                file.transfer_id().as_uuid()
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
        .query(include_str!(
            "queries/journal_payloads/payload_value/statement_1.surql"
        ))
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
            let db = support::database().await;
            let (replica, journal) = kind.admit(&db).await.unwrap();
            let payload = RecordId::new(kind.payload(), journal.key.clone());
            let original = payload_value(&db, &payload).await.unwrap();
            assert!(original.get("journal") == Value::RecordId(journal.clone()));
            let info: Value =
                db.a.client()
                    .query(include_str!("queries/journal_payloads/schema.surql"))
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
                    .query(include_str!("queries/journal_payloads/payloads_are_readonly_private_and_owned_by_their_journals/statement_1.surql"))
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
                .query(include_str!("queries/journal_payloads/payloads_are_readonly_private_and_owned_by_their_journals/statement_2.surql"))
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
                    .query(include_str!("queries/journal_payloads/payloads_are_readonly_private_and_owned_by_their_journals/statement_3.surql"))
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
                .query(include_str!("queries/journal_payloads/payloads_are_readonly_private_and_owned_by_their_journals/statement_4.surql"))
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
                .query(include_str!("queries/journal_payloads/payloads_are_readonly_private_and_owned_by_their_journals/statement_5.surql"))
                .bind(("journal", journal.clone()))
                .await
                .unwrap()
                .check()
                .unwrap()
                .take(0)
                .unwrap();
            assert_eq!(fenced.len(), 1);
            db.a.client()
                .query(include_str!("queries/journal_payloads/payloads_are_readonly_private_and_owned_by_their_journals/statement_6.surql"))
                .bind(("payload", payload.clone()))
                .bind(("original", original))
                .await
                .unwrap()
                .check()
                .unwrap();
            db.a.client()
                .query(include_str!("queries/journal_payloads/payloads_are_readonly_private_and_owned_by_their_journals/statement_7.surql"))
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
            let schema = match kind { Kind::Command => include_str!("queries/journal_payloads/payload_failure/command.surql"), Kind::File => include_str!("queries/journal_payloads/payload_failure/file.surql") };
            let db = support::TestDb::with_composition(support::store::StoreBackend::Memory,
                vec![veoveo_computers::schema::module_setup(support::store::module_lanes::execution("computers").unwrap()).unwrap()],
                veoveo_gateway_catalog::audit_target_registry().unwrap()).await;
            db.admin().await.client().query(schema).await.unwrap().check().unwrap();
            assert!(kind.admit(&db).await.is_err());
            let statement = match kind {
                Kind::Command => include_str!("queries/journal_payloads/payload_failure_rolls_back_journal_slot_request_and_audit/statement_1.surql"),
                Kind::File => include_str!("queries/journal_payloads/payload_failure_rolls_back_journal_slot_request_and_audit/statement_2.surql"),
            };
            let mut response = db.a.client().query(statement).await.unwrap().check().unwrap();
            for index in 0..5 {
                let rows: Vec<Value> = response.take(index).unwrap();
                assert!(rows.is_empty(), "failed payload admission left a committed record");
            }
        }
    }).await.expect("payload rollback qualification exceeded two minutes");
}
