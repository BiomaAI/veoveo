//! An existing but denied retry receipt cannot authorize another admission.
#[path = "support/files.rs"]
#[allow(unused_imports)]
mod file_support;
mod support;
use file_support::command_fixture;
use std::time::Duration;
use surrealdb::types::RecordId;
use uuid::Uuid;
use veoveo_computers::ComputerError;

#[tokio::test]
async fn reservation_receipts_admit_owner_clearance_before_decoding_and_never_reserve_again() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = support::TestDb::new().await;
        let store = veoveo_computers::ComputersStore::new(db.a.clone(), Uuid::from_u128(1)).unwrap();
        store.install_capacity(None, veoveo_computers::CapacityPolicy { per_owner: 2, per_tenant: 2, provider: 2 }).await.unwrap();
        let actor = support::authenticated(&support::owner("alice"));
        let input = veoveo_computers::Reservation {
            request_id: Uuid::now_v7(), template_id: "development".into(), template_fingerprint: support::FINGERPRINT.into(),
        };
        assert!(store.reserved_for_request(actor.owner(), input.request_id).await.unwrap().is_none());
        let computer = store.reserve(&actor, &input).await.unwrap();
        db.a.client().query("UPDATE ONLY $row SET owner_context.authority.policy_revision = 42, owner_context.data_labels = ['private'];")
            .bind(("row", command_fixture::computer_record(computer.computer_id))).await.unwrap().check().unwrap();
        assert!(matches!(store.reserved_for_request(actor.owner(), input.request_id).await, Err(ComputerError::NotFound)));
        assert!(matches!(store.reserve(&actor, &input).await, Err(ComputerError::NotFound)));
        let mut reply = db.a.client().query("SELECT VALUE computer FROM computer_request; SELECT VALUE retained FROM computer_usage;")
            .await.unwrap().check().unwrap();
        let receipts: Vec<RecordId> = reply.take(0).unwrap();
        let usage: Vec<i64> = reply.take(1).unwrap();
        assert_eq!(receipts, vec![command_fixture::computer_record(computer.computer_id)]);
        assert_eq!(usage, vec![1, 1, 1]);
    }).await.expect("reservation receipt admission exceeded 90 seconds");
}

#[tokio::test]
async fn command_receipts_admit_the_accepted_actor_before_decoding_private_payloads() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = support::TestDb::new().await;
        let (store, _, owner, agent, computer) = support::automation::setup(&db).await;
        let grant = store.issue_automation_grant(&owner, &support::automation::input(computer)).await.unwrap();
        let request = Uuid::now_v7();
        let payload = command_fixture::payload("receipt-fixture", 30);
        let keys = command_fixture::keys();
        let authority = command_fixture::permit(&store, &agent, computer, grant.grant_id).await;
        let operation = store.queue_command(&agent, authority, request, &payload, &keys).await.unwrap();
        let row = RecordId::new("computer_execution", surrealdb::types::Uuid::from(operation.execution_id().into_uuid()));
        corrupt_denied_target(&db, row).await;
        let authority = command_fixture::permit(&store, &agent, computer, grant.grant_id).await;
        assert!(matches!(store.queue_command(&agent, authority, request, &payload, &keys).await, Err(ComputerError::NotFound)));
        let mut reply = db.a.client().query("SELECT VALUE execution FROM computer_execution_request; SELECT VALUE execution FROM computer_execution_slot;")
            .await.unwrap().check().unwrap();
        let receipts: Vec<RecordId> = reply.take(0).unwrap();
        let slots: Vec<RecordId> = reply.take(1).unwrap();
        assert_eq!(receipts.len(), 1);
        assert_eq!(slots, receipts);
    }).await.expect("command receipt admission exceeded 90 seconds");
}

#[tokio::test]
async fn file_receipts_admit_the_accepted_actor_before_decoding_private_payloads() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = support::TestDb::new().await;
        let (store, _, owner, _, computer) = file_support::setup(&db).await;
        let request = Uuid::now_v7();
        let payload = file_support::payload("receipt-fixture");
        let keys = command_fixture::keys();
        let authority = store.file_transfer_authority(&owner, computer, None).await.unwrap();
        let operation = store.queue_file_transfer(&owner, authority, request, &payload, &keys).await.unwrap();
        let row = RecordId::new("computer_file_transfer", surrealdb::types::Uuid::from(operation.transfer_id().into_uuid()));
        corrupt_denied_target(&db, row).await;
        let authority = store.file_transfer_authority(&owner, computer, None).await.unwrap();
        assert!(matches!(store.queue_file_transfer(&owner, authority, request, &payload, &keys).await, Err(ComputerError::NotFound)));
        let mut reply = db.a.client().query("SELECT VALUE transfer FROM computer_file_transfer_request; SELECT VALUE execution FROM computer_execution_slot;")
            .await.unwrap().check().unwrap();
        let receipts: Vec<RecordId> = reply.take(0).unwrap();
        let slots: Vec<RecordId> = reply.take(1).unwrap();
        assert_eq!(receipts.len(), 1);
        assert_eq!(slots, receipts);
    }).await.expect("file receipt admission exceeded 90 seconds");
}

async fn corrupt_denied_target(db: &support::TestDb, row: RecordId) {
    db.a.client().query("UPDATE ONLY $row SET actor_key = $actor_key, authority.request_context.access_token.expires_at = 42;")
        .bind(("row", row)).bind(("actor_key", "b".repeat(64))).await.unwrap().check().unwrap();
}
