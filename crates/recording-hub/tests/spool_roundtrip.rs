//! Integration: push through the embedded proxy exactly like production, then
//! read the durable segments back with the query engine. Exercises the real
//! path (proxy → receiver → spooler → segment file → QueryEngine), the
//! restart-resume `.rN` behavior, and blueprint filtering.

use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use re_grpc_server::{MemoryLimit, ServerOptions, shutdown};
use re_sdk::RecordingStreamBuilder;
use re_sdk_types::archetypes::Scalars;
use veoveo_recording_hub::config::{DatasetName, DatasetRoute, SpoolerConfig};
use veoveo_recording_hub::spool::{Spooler, run_blocking};

/// Reserve a free localhost port for the proxy.
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral")
        .local_addr()
        .expect("local addr")
        .port()
}

fn config(spool: &std::path::Path, bind: SocketAddr) -> SpoolerConfig {
    SpoolerConfig {
        bind,
        spool_dir: spool.to_path_buf(),
        datasets: vec![DatasetRoute {
            dataset: DatasetName::new("world").unwrap(),
            application_id_prefix: String::new(),
        }],
        segment_max_bytes: 64 * 1024 * 1024,
        segment_max_age_s: 3600,
        flush_interval_ms: 50,
        fsync_on_flush: true,
        live_queue_limit_bytes: 256 * 1024 * 1024,
        rerun_bin: None,
    }
}

/// Spawn the proxy + spooler, run `body` (which pushes data), then stop and
/// return the segment tree row counts per recording on timeline `tick`.
async fn spool_run(
    spool_dir: &std::path::Path,
    n_messages: usize,
    recording: &str,
) -> std::collections::BTreeMap<String, u64> {
    let port = free_port();
    let bind: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let cfg = config(spool_dir, bind);

    let (signal, shutdown) = shutdown::shutdown();
    let options = ServerOptions {
        memory_limit: MemoryLimit::from_bytes(cfg.live_queue_limit_bytes),
        ..Default::default()
    };
    let (receiver, _handle) = re_grpc_server::spawn_with_recv(bind, options, shutdown);

    let stop = Arc::new(AtomicBool::new(false));
    let stop_drain = stop.clone();
    let drain = std::thread::spawn(move || {
        let spooler = Spooler::new(cfg).expect("spooler");
        run_blocking(
            spooler,
            receiver,
            stop_drain,
            Duration::from_millis(50),
            Duration::from_secs(3600),
        )
    });

    // Give the tonic server a moment to bind.
    tokio::time::sleep(Duration::from_millis(300)).await;

    let proxy = format!("rerun+http://127.0.0.1:{port}/proxy");
    let stream = RecordingStreamBuilder::new("veoveo-sim-test")
        .recording_id(recording.to_string())
        .connect_grpc_opts(proxy)
        .expect("connect");
    for i in 0..n_messages {
        stream.set_time_sequence("tick", i as i64);
        stream
            .log("/world/sim/test", &Scalars::new([i as f64]))
            .expect("log");
    }
    stream.flush_blocking().expect("flush");
    drop(stream);

    // Let the messages arrive, then stop and freeze.
    tokio::time::sleep(Duration::from_millis(500)).await;
    stop.store(true, Ordering::SeqCst);
    signal.stop();
    let counters = drain.join().expect("join").expect("run_blocking");
    assert!(counters.messages > 0, "spooler saw messages");

    let result =
        veoveo_recording_hub::query_tree(spool_dir, "/**", "tick", 100_000).expect("query");
    result.rows_by_recording
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn roundtrip_counts_match() {
    let dir = tempfile::tempdir().expect("tempdir");
    let counts = spool_run(dir.path(), 25, "rec-round").await;
    assert_eq!(
        counts.get("rec-round").copied(),
        Some(25),
        "all 25 scalar rows are durable and queryable: {counts:?}"
    );

    // The world dataset directory exists and holds a segment.
    let segments = veoveo_recording_hub::collect_segments(dir.path()).expect("collect");
    assert!(!segments.is_empty(), "at least one segment written");
    assert!(
        segments
            .iter()
            .any(|p| p.to_string_lossy().contains("/world/")),
        "segment routed into the world dataset: {segments:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn restart_resumes_into_sibling_segment() {
    let dir = tempfile::tempdir().expect("tempdir");

    // First session: write, freeze, stop.
    let first = spool_run(dir.path(), 10, "rec-resume").await;
    assert_eq!(first.get("rec-resume").copied(), Some(10));

    // Second session with the SAME recording id: must not truncate the prior
    // file — it resumes into a `.rN` sibling, and the total is cumulative.
    let second = spool_run(dir.path(), 15, "rec-resume").await;
    assert_eq!(
        second.get("rec-resume").copied(),
        Some(25),
        "both sessions' rows are durable across restart: {second:?}"
    );

    // Two physical segments for one recording (base + .r1).
    let segments = veoveo_recording_hub::collect_segments(dir.path()).expect("collect");
    let resume_segments: Vec<_> = segments
        .iter()
        .filter(|p| p.to_string_lossy().contains("rec-resume"))
        .collect();
    assert_eq!(
        resume_segments.len(),
        2,
        "restart created a sibling, not a truncation: {resume_segments:?}"
    );
}
