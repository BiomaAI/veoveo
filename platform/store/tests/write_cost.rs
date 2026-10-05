//! Controlled write-cost measurement; run on an otherwise idle host.
use std::{sync::Arc, time::Duration};

use serde::Serialize;
use surrealdb::types::{QueryError, RecordId, SurrealValue};
use tokio::{sync::Barrier, task::JoinSet, time::Instant};
use veoveo_platform_store::PlatformStore;

#[path = "../../../testing/fixtures/store.rs"]
mod fixture;

const OPERATIONS: usize = 64;
const SCHEMA: &str = include_str!("queries/write_cost/statement.surql");

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Profile {
    SharedSequence,
    IndependentEvent,
    ChangefeedOnly,
}

impl Profile {
    fn query(self) -> &'static str {
        match self {
            Self::SharedSequence => include_str!("queries/write_cost/shared_sequence.surql"),
            Self::IndependentEvent => include_str!("queries/write_cost/independent_event.surql"),
            Self::ChangefeedOnly => include_str!("queries/write_cost/changefeed_only.surql"),
        }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    storage: Option<StorageCost>,
}

#[derive(Serialize)]
struct StorageCost {
    before_bytes: u64,
    after_bytes: u64,
    device_writes: Vec<fixture::io::DeviceWrites>,
}

#[derive(Clone, Copy)]
enum Observation {
    Latency,
    HostIo,
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
                .query(query)
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

async fn measure(
    profile: Profile,
    round: usize,
    writers: usize,
    observation: Observation,
) -> Measurement {
    let db = fixture::TestDb::with_backend_and_schema(fixture::StoreBackend::RocksDb, SCHEMA).await;
    for writer in 0..writers {
        db.a.client()
            .query(include_str!("queries/write_cost/measure.surql"))
            .bind((
                "state",
                RecordId::new("measurement_state", writer.to_string()),
            ))
            .await
            .unwrap()
            .check()
            .unwrap();
    }
    let io = match observation {
        Observation::Latency => None,
        Observation::HostIo => {
            db.compact().await;
            let probe = db.io_probe().await;
            let before = probe.settled().await;
            let before_bytes = db.storage_bytes().await;
            Some((probe, before, before_bytes))
        }
    };
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
    let storage = if let Some((probe, before, before_bytes)) = io {
        db.compact().await;
        let writes = fixture::io::delta(&before, &probe.settled().await);
        assert!(
            writes.iter().any(|device| device.bytes > 0),
            "database writes were not attributed to the measured cgroup"
        );
        Some(StorageCost {
            before_bytes,
            after_bytes: db.storage_bytes().await,
            device_writes: writes,
        })
    } else {
        None
    };
    let mut response =
        db.a.client()
            .query(include_str!("queries/write_cost/measure_2.surql"))
            .await
            .unwrap()
            .check()
            .unwrap();
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
        storage,
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
                    let measurement = measure(profile, round, writers, Observation::Latency).await;
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "measurement: local Linux Docker/cgroup-v2 RocksDB; stop cluster and builders first"]
async fn compare_write_io_profiles() {
    tokio::time::timeout(Duration::from_secs(600), async {
        let profiles = [
            Profile::SharedSequence,
            Profile::IndependentEvent,
            Profile::ChangefeedOnly,
        ];
        for round in 0..3 {
            for offset in 0..profiles.len() {
                let measurement = measure(
                    profiles[(round + offset) % profiles.len()],
                    round,
                    8,
                    Observation::HostIo,
                )
                .await;
                println!("WRITE_IO {}", serde_json::to_string(&measurement).unwrap());
            }
        }
    })
    .await
    .expect("write-I/O measurement exceeded ten minutes");
}
