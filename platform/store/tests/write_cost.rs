//! Controlled write-cost measurement; run on an otherwise idle host.
use std::{sync::Arc, time::Duration};

use serde::Serialize;
use surrealdb::types::{QueryError, RecordId, SurrealValue};
use tokio::{sync::Barrier, task::JoinSet, time::Instant};
use veoveo_platform_store::PlatformStore;

#[path = "../../../testing/fixtures/store.rs"]
mod fixture;

const OPERATIONS: usize = 64;
const SCHEMA: &str = r#"
DEFINE TABLE measurement_state SCHEMAFULL CHANGEFEED 30d INCLUDE ORIGINAL;
DEFINE FIELD revision ON measurement_state TYPE int;
DEFINE SEQUENCE measurement_sequence BATCH 1 START 1;
DEFINE TABLE measurement_event SCHEMAFULL CHANGEFEED 30d INCLUDE ORIGINAL;
DEFINE FIELD sequence ON measurement_event TYPE int DEFAULT sequence::nextval('measurement_sequence') READONLY;
DEFINE FIELD tenant ON measurement_event TYPE option<record<tenant>>;
DEFINE FIELD aggregate_type ON measurement_event TYPE string;
DEFINE FIELD aggregate_id ON measurement_event TYPE string;
DEFINE FIELD event_type ON measurement_event TYPE string;
DEFINE FIELD schema_version ON measurement_event TYPE int ASSERT $value > 0;
DEFINE FIELD payload ON measurement_event TYPE object FLEXIBLE;
DEFINE FIELD occurred_at ON measurement_event TYPE datetime DEFAULT time::now();
DEFINE FIELD available_at ON measurement_event TYPE datetime DEFAULT time::now();
DEFINE INDEX measurement_event_sequence ON measurement_event FIELDS sequence UNIQUE;
DEFINE INDEX measurement_event_available ON measurement_event FIELDS available_at, sequence;
DEFINE INDEX measurement_event_aggregate ON measurement_event FIELDS aggregate_type, aggregate_id, sequence;
DEFINE INDEX measurement_event_tenant ON measurement_event FIELDS tenant, sequence;
"#;

const DOMAIN_WRITE: &str = "UPDATE ONLY $state SET revision += 1 RETURN NONE;";
const EVENT_WRITE: &str = r#"
CREATE ONLY $event SET aggregate_type = 'measurement_state', aggregate_id = $aggregate,
    event_type = 'measurement.updated', schema_version = 1,
    payload = { revision: $revision } RETURN NONE;
"#;
const INDEPENDENT_EVENT_WRITE: &str = r#"
CREATE ONLY $event SET sequence = $sequence,
    aggregate_type = 'measurement_state', aggregate_id = $aggregate,
    event_type = 'measurement.updated', schema_version = 1,
    payload = { revision: $revision } RETURN NONE;
"#;

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Profile {
    SharedSequence,
    IndependentEvent,
    ChangefeedOnly,
}

impl Profile {
    fn query(self) -> String {
        let event = match self {
            Self::SharedSequence => EVENT_WRITE,
            Self::IndependentEvent => INDEPENDENT_EVENT_WRITE,
            Self::ChangefeedOnly => "",
        };
        format!("BEGIN TRANSACTION; {DOMAIN_WRITE} {event} COMMIT TRANSACTION;")
    }
}

#[derive(Default, Serialize)]
struct Samples {
    conflicts: usize,
    attempts_us: Vec<u64>,
    operations_us: Vec<u64>,
}

#[derive(Serialize)]
struct Measurement {
    profile: Profile,
    round: usize,
    writers: usize,
    operations: usize,
    domain_rows: usize,
    event_rows: usize,
    elapsed_us: u64,
    samples: Samples,
}

#[derive(SurrealValue)]
struct EventCount {
    total: i64,
}

async fn writer(
    store: PlatformStore,
    start: Arc<Barrier>,
    profile: Profile,
    ordinal: usize,
) -> Samples {
    let aggregate = ordinal.to_string();
    let state = RecordId::new("measurement_state", aggregate.clone());
    let query = profile.query();
    let mut samples = Samples::default();
    start.wait().await;
    for operation in 0..OPERATIONS {
        let event = RecordId::new(
            "measurement_event",
            surrealdb::types::Uuid::from(uuid::Uuid::now_v7()),
        );
        let sequence = (ordinal * OPERATIONS + operation + 1) as i64;
        let operation_start = Instant::now();
        for attempt in 0..128 {
            let attempt_start = Instant::now();
            let result = store
                .client()
                .query(query.clone())
                .bind(("state", state.clone()))
                .bind(("event", event.clone()))
                .bind(("aggregate", aggregate.clone()))
                .bind(("sequence", sequence))
                .bind(("revision", (operation + 1) as i64))
                .await
                .and_then(|response| response.check());
            samples
                .attempts_us
                .push(attempt_start.elapsed().as_micros() as u64);
            match result {
                Ok(_) => break,
                Err(error)
                    if matches!(error.query_details(), Some(QueryError::TransactionConflict)) =>
                {
                    samples.conflicts += 1;
                    assert!(attempt < 127, "measurement exhausted conflict retries");
                    tokio::time::sleep(Duration::from_micros(100 << attempt.min(4))).await;
                }
                Err(error) => panic!("measurement transaction failed: {error}"),
            }
        }
        samples
            .operations_us
            .push(operation_start.elapsed().as_micros() as u64);
    }
    samples
}

async fn measure(profile: Profile, round: usize, writers: usize) -> Measurement {
    let db = fixture::TestDb::with_backend_and_schema(fixture::StoreBackend::RocksDb, SCHEMA).await;
    for writer in 0..writers {
        db.a.client()
            .query("CREATE ONLY $state SET revision = 0 RETURN NONE;")
            .bind((
                "state",
                RecordId::new("measurement_state", writer.to_string()),
            ))
            .await
            .unwrap()
            .check()
            .unwrap();
    }
    let barrier = Arc::new(Barrier::new(writers + 1));
    let mut workers = JoinSet::new();
    for ordinal in 0..writers {
        workers.spawn(writer(db.a.clone(), barrier.clone(), profile, ordinal));
    }
    let start = Instant::now();
    barrier.wait().await;
    let mut samples = Samples::default();
    while let Some(worker) = workers.join_next().await {
        let mut worker = worker.unwrap();
        samples.conflicts += worker.conflicts;
        samples.attempts_us.append(&mut worker.attempts_us);
        samples.operations_us.append(&mut worker.operations_us);
    }
    let elapsed_us = start.elapsed().as_micros() as u64;
    let mut response = db.a.client()
        .query("SELECT VALUE revision FROM measurement_state; SELECT count() AS total FROM measurement_event GROUP ALL;")
        .await.unwrap().check().unwrap();
    let revisions: Vec<i64> = response.take(0).unwrap();
    assert_eq!(revisions.len(), writers);
    assert!(
        revisions
            .iter()
            .all(|revision| *revision == OPERATIONS as i64)
    );
    let events: Vec<EventCount> = response.take(1).unwrap();
    let event_rows = events.first().map_or(0, |row| row.total) as usize;
    let expected_events = match profile {
        Profile::ChangefeedOnly => 0,
        _ => writers * OPERATIONS,
    };
    assert_eq!(event_rows, expected_events);
    assert_eq!(samples.operations_us.len(), writers * OPERATIONS);
    assert_eq!(
        samples.attempts_us.len(),
        writers * OPERATIONS + samples.conflicts
    );
    Measurement {
        profile,
        round,
        writers,
        operations: writers * OPERATIONS,
        domain_rows: revisions.len(),
        event_rows,
        elapsed_us,
        samples,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "measurement: pinned Docker RocksDB fixture; stop cluster and builders first"]
async fn compare_shared_sequence_and_native_feed_writes() {
    tokio::time::timeout(Duration::from_secs(600), async {
        let profiles = [
            Profile::SharedSequence,
            Profile::IndependentEvent,
            Profile::ChangefeedOnly,
        ];
        for writers in [1, 8] {
            for round in 0..3 {
                // Rotate order so each profile runs first once per writer count.
                for offset in 0..profiles.len() {
                    let profile = profiles[(round + offset) % profiles.len()];
                    let measurement = measure(profile, round, writers).await;
                    println!(
                        "WRITE_COST {}",
                        serde_json::to_string(&measurement).unwrap()
                    );
                }
            }
        }
    })
    .await
    .expect("write-cost measurement exceeded ten minutes");
}
