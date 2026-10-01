//! Encrypted journal write/feed cost on the real schema, without provider dispatch.
#[path = "support/commands.rs"]
mod commands;
mod support;

use serde::Serialize;
use std::{collections::BTreeMap, time::Duration};
use support::store::{StoreBackend, io};
use surrealdb::types::{RecordId, Value};
use tokio::time::Instant;
use veoveo_computer_execution::ExecutionRequest;
use veoveo_computers::{api::*, secrets::CommandPayload};
use veoveo_platform_store::{ChangefeedEntry, decode_changefeed_entry};

const UPDATES: usize = 16;

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Profile {
    NoFeed,
    NativeFeed,
}

#[derive(Serialize)]
struct Measurement {
    profile: Profile,
    round: usize,
    stdin_bytes: usize,
    ciphertext_bytes: usize,
    updates: usize,
    operation_us: Vec<u64>,
    elapsed_us: u64,
    replay_us: u64,
    feed_rows: usize,
    feed_json_bytes: usize,
    feed_ciphertext_bytes: usize,
    storage_before_bytes: u64,
    storage_after_bytes: u64,
    device_writes: Vec<io::DeviceWrites>,
}

async fn measure(profile: Profile, round: usize, stdin_bytes: usize) -> Measurement {
    let schema = match profile {
        Profile::NoFeed => "ALTER TABLE computer_execution DROP CHANGEFEED;",
        Profile::NativeFeed => "",
    };
    let db = support::TestDb::with_backend_and_schema(StoreBackend::RocksDb, schema).await;
    let (store, _, owner, agent, computer) = support::automation::setup(&db).await;
    let grant = store
        .issue_automation_grant(&owner, &support::automation::input(computer))
        .await
        .unwrap();
    let request = ExecutionRequest::new(
        vec!["/bin/cat".into()],
        ".".into(),
        BTreeMap::new(),
        vec![b'x'; stdin_bytes],
    )
    .unwrap();
    let payload = CommandPayload::new(
        request,
        AutomationExecutionLimits {
            maximum_seconds: 30,
            maximum_output_bytes: 1024,
            on_interruption: AutomationInterruption::StopComputer,
        },
    )
    .unwrap();
    let operation = store
        .queue_command(
            &agent,
            commands::permit(&store, &agent, computer, grant.grant_id).await,
            uuid::Uuid::now_v7(),
            &payload,
            &commands::keys(),
        )
        .await
        .unwrap();
    let record = RecordId::new(
        "computer_execution",
        surrealdb::types::Uuid::from(operation.execution_id().into_uuid()),
    );
    let ciphertext_bytes: Option<u64> =
        db.a.client()
            .query("RETURN string::len((SELECT VALUE sealed.ciphertext FROM ONLY $record));")
            .bind(("record", record.clone()))
            .await
            .unwrap()
            .check()
            .unwrap()
            .take(0)
            .unwrap();
    let ciphertext_bytes = usize::try_from(ciphertext_bytes.unwrap()).unwrap();
    assert!(
        ciphertext_bytes > stdin_bytes,
        "admission must persist an encrypted envelope"
    );

    db.compact().await;
    let probe = db.io_probe().await;
    let before = probe.settled().await;
    let storage_before_bytes = db.storage_bytes().await;
    let mut cursor = db.a.changefeed_head().await.unwrap();
    let mut operation_us = Vec::new();
    let start = Instant::now();
    for _ in 0..UPDATES {
        let operation = Instant::now();
        db.a.client()
            .query("UPDATE ONLY $record SET updated_at = time::now() RETURN NONE;")
            .bind(("record", record.clone()))
            .await
            .unwrap()
            .check()
            .unwrap();
        operation_us.push(operation.elapsed().as_micros() as u64);
    }
    let elapsed_us = start.elapsed().as_micros() as u64;
    db.compact().await;
    let device_writes = io::delta(&before, &probe.settled().await);
    assert!(device_writes.iter().any(|device| device.bytes > 0));
    let storage_after_bytes = db.storage_bytes().await;

    let mut feed_rows = 0;
    let mut feed_json_bytes = 0;
    let mut feed_ciphertext_bytes = 0;
    let mut replay_us = 0;
    loop {
        let start = Instant::now();
        let batches = db.b.replay_changes(cursor, 512).await.unwrap();
        replay_us += start.elapsed().as_micros() as u64;
        let Some(last) = batches.last() else {
            break;
        };
        cursor = veoveo_platform_store::ChangefeedCursor::from_versionstamp(last.versionstamp + 1)
            .unwrap();
        for batch in batches {
            for value in batch.changes {
                let change = decode_changefeed_entry(&value).unwrap();
                if change.record_id() == Some(&record)
                    && let ChangefeedEntry::Upsert(row) = change
                {
                    feed_rows += 1;
                    feed_json_bytes += serde_json::to_vec(&row.clone().into_json_value())
                        .unwrap()
                        .len();
                    let Value::String(ciphertext) = row.get("sealed").get("ciphertext") else {
                        panic!("journal replay must preserve the encrypted envelope");
                    };
                    assert_eq!(ciphertext.len(), ciphertext_bytes);
                    feed_ciphertext_bytes += ciphertext.len();
                }
            }
        }
    }
    assert_eq!(
        feed_rows,
        match profile {
            Profile::NoFeed => 0,
            Profile::NativeFeed => UPDATES,
        }
    );
    Measurement {
        profile,
        round,
        stdin_bytes,
        ciphertext_bytes,
        updates: UPDATES,
        operation_us,
        elapsed_us,
        replay_us,
        feed_rows,
        feed_json_bytes,
        feed_ciphertext_bytes,
        storage_before_bytes,
        storage_after_bytes,
        device_writes,
    }
}

#[tokio::test]
#[ignore = "measurement: local Linux Docker/cgroup-v2 RocksDB; stop cluster and builders first"]
async fn compare_encrypted_journal_feed_cost() {
    tokio::time::timeout(Duration::from_secs(900), async {
        let profiles = [Profile::NoFeed, Profile::NativeFeed];
        for stdin_bytes in [1024, 64 * 1024, 1024 * 1024] {
            for round in 0..3 {
                for offset in 0..profiles.len() {
                    let measurement = measure(
                        profiles[(round + offset) % profiles.len()],
                        round,
                        stdin_bytes,
                    )
                    .await;
                    println!(
                        "JOURNAL_COST {}",
                        serde_json::to_string(&measurement).unwrap()
                    );
                }
            }
        }
    })
    .await
    .expect("journal-cost measurement exceeded fifteen minutes");
}
